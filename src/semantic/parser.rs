//! SourceRange-aware block tree.
//!
//! Layer 2 needs every analysed unit to carry a stable `SourceRange` back to the
//! original document (design思想「全 UI は sourceRange に紐づく」). This module
//! folds `pulldown-cmark`'s `into_offset_iter()` byte-ranged event stream into a
//! lightweight [`BlockTree`] whose blocks keep both a byte range and a
//! line/column [`SourceRange`].
//!
//! > Integration note: AGENTS.md §10 makes the `SourceRange` parser a Layer 1 /
//! > `mdpeek-core::parser` deliverable (worktree `layer1-sourcerange-parser`).
//! > This module is intentionally self-contained under `semantic::` so it does
//! > not collide with that work; on integration it is expected to be replaced by
//! > `mdpeek-core::parser::BlockTree`.

use crate::gfm::parser_options;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Parser, Tag, TagEnd};
use serde::Serialize;

/// 1-based line/column span into the source document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SourceRange {
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

/// Byte-offset ⇄ (line, column) lookup table, built in one pass over the source.
#[derive(Debug, Clone)]
pub struct LineIndex {
    /// Byte offset at which each line starts. `line_starts[0] == 0`.
    line_starts: Vec<usize>,
    len: usize,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![0usize];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        LineIndex {
            line_starts,
            len: text.len(),
        }
    }

    /// Map a byte offset to a 1-based (line, column). Column counts bytes from
    /// the line start (good enough for jump-to-line; not grapheme aware).
    pub fn locate(&self, offset: usize) -> (u32, u32) {
        let offset = offset.min(self.len);
        // Largest line start that is <= offset.
        let line = match self.line_starts.binary_search(&offset) {
            Ok(idx) => idx,
            Err(idx) => idx - 1,
        };
        let column = offset - self.line_starts[line];
        ((line as u32) + 1, (column as u32) + 1)
    }

    /// Convert a byte range into a line/column [`SourceRange`].
    pub fn range(&self, byte_range: std::ops::Range<usize>) -> SourceRange {
        // An empty/degenerate range still yields a valid single-point range.
        let end = byte_range.end.max(byte_range.start);
        let (start_line, start_column) = self.locate(byte_range.start);
        let (end_line, end_column) = self.locate(end);
        SourceRange {
            start_line,
            start_column,
            end_line,
            end_column,
        }
    }
}

/// Stable identifier of a block within a single [`BlockTree`], assigned in
/// document (pre-order) order. Used for差分再生成/ハイライトand SourceRangeLink.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct BlockId(pub u32);

/// Coarse block classification carried by the tree (structural only; semantic
/// classification lives in [`crate::semantic::model::BlockClass`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    Heading { level: u8 },
    Paragraph,
    CodeBlock { lang: Option<String> },
    BlockQuote,
    List { task: bool },
    Item { checked: Option<bool> },
    Table,
    Html,
    FootnoteDefinition,
    Other,
}

/// One node of the block tree.
#[derive(Debug, Clone)]
pub struct Block {
    pub id: BlockId,
    pub kind: BlockKind,
    pub range: SourceRange,
    /// Byte range into the source (`start..end`).
    pub byte_range: std::ops::Range<usize>,
    pub children: Vec<Block>,
    /// Extracted plain text (headings/paragraphs directly, containers aggregated
    /// from their children).
    pub text: String,
}

impl Block {
    /// Depth-first pre-order iterator over this block and its descendants.
    pub fn iter(&self) -> BlockIter<'_> {
        BlockIter { stack: vec![self] }
    }
}

/// Depth-first pre-order iterator over a slice of blocks and their descendants.
pub struct BlockIter<'a> {
    stack: Vec<&'a Block>,
}

impl<'a> Iterator for BlockIter<'a> {
    type Item = &'a Block;
    fn next(&mut self) -> Option<Self::Item> {
        let block = self.stack.pop()?;
        // Push children in reverse so they are visited left-to-right.
        for child in block.children.iter().rev() {
            self.stack.push(child);
        }
        Some(block)
    }
}

/// A hyperlink discovered in the source, with its span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Link {
    pub text: String,
    pub url: String,
    pub range: SourceRange,
}

/// The parsed document: top-level blocks plus a line index and extracted links.
#[derive(Debug, Clone)]
pub struct BlockTree {
    pub blocks: Vec<Block>,
    pub line_index: LineIndex,
    pub links: Vec<Link>,
    /// Raw frontmatter text (YAML/TOML metadata block), if present.
    pub frontmatter: Option<String>,
}

impl BlockTree {
    /// Pre-order iterator over every block in the tree.
    pub fn iter(&self) -> impl Iterator<Item = &Block> {
        BlockIter {
            stack: self.blocks.iter().rev().collect(),
        }
    }

    /// Find a block by id.
    pub fn find(&self, id: BlockId) -> Option<&Block> {
        self.iter().find(|b| b.id == id)
    }
}

/// Parse markdown into a [`BlockTree`] with source ranges.
pub fn parse(markdown: &str) -> BlockTree {
    let parser = Parser::new_ext(markdown, parser_options()).into_offset_iter();

    let mut builder = TreeBuilder::new(markdown);
    for (event, range) in parser {
        builder.event(event, range);
    }
    let (blocks, links, frontmatter) = builder.finish();

    BlockTree {
        blocks,
        line_index: LineIndex::new(markdown),
        links,
        frontmatter,
    }
}

/// In-progress block being assembled from a `Start`..`End` event pair.
struct Frame {
    id: BlockId,
    kind: BlockKind,
    byte_range: std::ops::Range<usize>,
    text: String,
    children: Vec<Block>,
    /// Set when a task-list marker is seen inside an `Item`.
    checked: Option<bool>,
    /// Set on a `List` when any of its items carry a task marker.
    list_has_task: bool,
}

/// Link being assembled between `Start(Link)` and `End(Link)`.
struct LinkFrame {
    url: String,
    text: String,
    byte_range: std::ops::Range<usize>,
}

struct TreeBuilder<'s> {
    src: &'s str,
    line_index: LineIndex,
    stack: Vec<Frame>,
    roots: Vec<Block>,
    links: Vec<Link>,
    link_stack: Vec<LinkFrame>,
    next_id: u32,
    /// Depth of the currently-open metadata (frontmatter) block, if any.
    in_metadata: bool,
    frontmatter: Option<String>,
}

impl<'s> TreeBuilder<'s> {
    fn new(src: &'s str) -> Self {
        TreeBuilder {
            src,
            line_index: LineIndex::new(src),
            stack: Vec::new(),
            roots: Vec::new(),
            links: Vec::new(),
            link_stack: Vec::new(),
            next_id: 0,
            in_metadata: false,
            frontmatter: None,
        }
    }

    fn alloc_id(&mut self) -> BlockId {
        let id = BlockId(self.next_id);
        self.next_id += 1;
        id
    }

    fn event(&mut self, event: Event<'_>, range: std::ops::Range<usize>) {
        match event {
            Event::Start(tag) => self.start(tag, range),
            Event::End(tag) => self.end(tag),
            Event::Text(t) => self.push_text(&t),
            Event::Code(t) => {
                // Inline code: keep the text so headings/paragraphs read naturally.
                self.push_text(&t);
            }
            Event::SoftBreak => self.push_text(" "),
            Event::HardBreak => self.push_text("\n"),
            Event::TaskListMarker(checked) => self.mark_task(checked),
            // Html/InlineHtml/FootnoteReference/Math/Rule contribute no plain text
            // we want in block titles; ranges are still captured via their blocks.
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>, range: std::ops::Range<usize>) {
        // Metadata (frontmatter) block: capture its text, do not emit a block.
        if matches!(tag, Tag::MetadataBlock(_)) {
            self.in_metadata = true;
            return;
        }
        // Inline link: track separately for the links list.
        if let Tag::Link { dest_url, .. } = &tag {
            self.link_stack.push(LinkFrame {
                url: dest_url.to_string(),
                text: String::new(),
                byte_range: range.clone(),
            });
            return;
        }
        let Some(kind) = block_kind_of(&tag) else {
            // Inline / sub-table tag: no frame, text still accrues to the parent.
            return;
        };
        let id = self.alloc_id();
        self.stack.push(Frame {
            id,
            kind,
            byte_range: range,
            text: String::new(),
            children: Vec::new(),
            checked: None,
            list_has_task: false,
        });
    }

    fn end(&mut self, tag: TagEnd) {
        if matches!(tag, TagEnd::MetadataBlock(_)) {
            self.in_metadata = false;
            return;
        }
        if matches!(tag, TagEnd::Link) {
            if let Some(link) = self.link_stack.pop() {
                self.links.push(Link {
                    text: link.text.trim().to_string(),
                    url: link.url,
                    range: self.line_index.range(link.byte_range),
                });
            }
            return;
        }
        if !ends_frame(&tag) {
            return;
        }
        let Some(mut frame) = self.stack.pop() else {
            return;
        };

        // Aggregate container text from children when the frame has none of its own.
        if frame.text.trim().is_empty() && !frame.children.is_empty() {
            frame.text = frame
                .children
                .iter()
                .map(|c| c.text.trim())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
        }

        // Fold list/item task state into the kind.
        let kind = match frame.kind {
            BlockKind::List { .. } => BlockKind::List {
                task: frame.list_has_task,
            },
            BlockKind::Item { .. } => BlockKind::Item {
                checked: frame.checked,
            },
            other => other,
        };

        let block = Block {
            id: frame.id,
            kind,
            range: self.line_index.range(frame.byte_range.clone()),
            byte_range: frame.byte_range,
            children: frame.children,
            text: frame.text.trim().to_string(),
        };

        match self.stack.last_mut() {
            Some(parent) => parent.children.push(block),
            None => self.roots.push(block),
        }
    }

    fn push_text(&mut self, s: &str) {
        if self.in_metadata {
            let fm = self.frontmatter.get_or_insert_with(String::new);
            fm.push_str(s);
            return;
        }
        if let Some(link) = self.link_stack.last_mut() {
            link.text.push_str(s);
        }
        if let Some(frame) = self.stack.last_mut() {
            frame.text.push_str(s);
        }
    }

    /// Apply a task-list marker to the nearest enclosing `Item` and flag its
    /// parent `List`.
    fn mark_task(&mut self, checked: bool) {
        // Find nearest Item frame from the top of the stack.
        for i in (0..self.stack.len()).rev() {
            if matches!(self.stack[i].kind, BlockKind::Item { .. }) {
                self.stack[i].checked = Some(checked);
                // Flag the closest enclosing List below it.
                for j in (0..i).rev() {
                    if matches!(self.stack[j].kind, BlockKind::List { .. }) {
                        self.stack[j].list_has_task = true;
                        break;
                    }
                }
                break;
            }
        }
    }

    fn finish(mut self) -> (Vec<Block>, Vec<Link>, Option<String>) {
        // Close any dangling frames defensively (well-formed docs leave none).
        while let Some(frame) = self.stack.pop() {
            let block = Block {
                id: frame.id,
                kind: frame.kind,
                range: self.line_index.range(frame.byte_range.clone()),
                byte_range: frame.byte_range,
                children: frame.children,
                text: frame.text.trim().to_string(),
            };
            match self.stack.last_mut() {
                Some(parent) => parent.children.push(block),
                None => self.roots.push(block),
            }
        }
        let frontmatter = self
            .frontmatter
            .map(|f| f.trim_end_matches('\n').to_string())
            .filter(|f| !f.trim().is_empty());
        // Keep `src` referenced so the borrow is meaningful across the builder.
        let _ = self.src;
        (self.roots, self.links, frontmatter)
    }
}

/// Structural block kind for a start tag, or `None` for inline / sub-table tags
/// that should not open a frame (their text still accrues to the parent block).
fn block_kind_of(tag: &Tag<'_>) -> Option<BlockKind> {
    Some(match tag {
        Tag::Paragraph => BlockKind::Paragraph,
        Tag::Heading { level, .. } => BlockKind::Heading {
            level: heading_level(*level),
        },
        Tag::BlockQuote(_) => BlockKind::BlockQuote,
        Tag::CodeBlock(kind) => BlockKind::CodeBlock {
            lang: code_lang(kind),
        },
        Tag::List(_) => BlockKind::List { task: false },
        Tag::Item => BlockKind::Item { checked: None },
        Tag::Table(_) => BlockKind::Table,
        Tag::HtmlBlock => BlockKind::Html,
        Tag::FootnoteDefinition(_) => BlockKind::FootnoteDefinition,
        _ => return None,
    })
}

/// Whether a closing tag ends a frame opened by [`block_kind_of`].
fn ends_frame(tag: &TagEnd) -> bool {
    matches!(
        tag,
        TagEnd::Paragraph
            | TagEnd::Heading(_)
            | TagEnd::BlockQuote(_)
            | TagEnd::CodeBlock
            | TagEnd::List(_)
            | TagEnd::Item
            | TagEnd::Table
            | TagEnd::HtmlBlock
            | TagEnd::FootnoteDefinition
    )
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn code_lang(kind: &CodeBlockKind<'_>) -> Option<String> {
    match kind {
        CodeBlockKind::Fenced(info) => {
            let lang = info.split_whitespace().next().unwrap_or("");
            if lang.is_empty() {
                None
            } else {
                Some(lang.to_string())
            }
        }
        CodeBlockKind::Indented => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_index_locates_lines_and_columns() {
        let idx = LineIndex::new("abc\ndef\n\nghi");
        assert_eq!(idx.locate(0), (1, 1));
        assert_eq!(idx.locate(2), (1, 3));
        assert_eq!(idx.locate(4), (2, 1)); // 'd'
        assert_eq!(idx.locate(8), (3, 1)); // blank line
        assert_eq!(idx.locate(9), (4, 1)); // 'g'
    }

    #[test]
    fn heading_carries_level_text_and_range() {
        let tree = parse("# Title\n\nbody text\n");
        let heading = tree.iter().find(|b| matches!(b.kind, BlockKind::Heading { .. }));
        let heading = heading.expect("heading present");
        assert_eq!(heading.kind, BlockKind::Heading { level: 1 });
        assert_eq!(heading.text, "Title");
        assert_eq!(heading.range.start_line, 1);
    }

    #[test]
    fn code_block_captures_language() {
        let tree = parse("```rust\nfn main() {}\n```\n");
        let code = tree
            .iter()
            .find_map(|b| match &b.kind {
                BlockKind::CodeBlock { lang } => Some(lang.clone()),
                _ => None,
            })
            .expect("code block present");
        assert_eq!(code, Some("rust".to_string()));
    }

    #[test]
    fn task_list_items_carry_checked_state() {
        let md = "- [ ] todo one\n- [x] done two\n- plain three\n";
        let tree = parse(md);
        let list = tree
            .iter()
            .find(|b| matches!(b.kind, BlockKind::List { .. }))
            .expect("list present");
        assert_eq!(list.kind, BlockKind::List { task: true });

        let checks: Vec<Option<bool>> = tree
            .iter()
            .filter_map(|b| match b.kind {
                BlockKind::Item { checked } => Some(checked),
                _ => None,
            })
            .collect();
        assert_eq!(checks, vec![Some(false), Some(true), None]);
    }

    #[test]
    fn links_are_extracted_with_url() {
        let tree = parse("See [the docs](https://example.com/docs) here.\n");
        assert_eq!(tree.links.len(), 1);
        assert_eq!(tree.links[0].url, "https://example.com/docs");
        assert_eq!(tree.links[0].text, "the docs");
    }

    #[test]
    fn frontmatter_is_captured_not_a_block() {
        let md = "---\ntitle: Hello\ntype: readme\n---\n\n# Body\n";
        let tree = parse(md);
        let fm = tree.frontmatter.as_ref().expect("frontmatter present");
        assert!(fm.contains("title: Hello"));
        assert!(fm.contains("type: readme"));
        // The frontmatter must not appear as a heading/paragraph block.
        assert!(tree.iter().all(|b| !b.text.contains("title: Hello")));
    }

    #[test]
    fn nested_list_items_are_children() {
        let md = "- parent\n  - child a\n  - child b\n";
        let tree = parse(md);
        let top_list = tree
            .blocks
            .iter()
            .find(|b| matches!(b.kind, BlockKind::List { .. }))
            .expect("top list");
        // The parent item should itself contain a nested list.
        let has_nested = top_list
            .iter()
            .any(|b| matches!(b.kind, BlockKind::List { .. }) && b.id != top_list.id);
        assert!(has_nested, "expected a nested list among descendants");
    }
}
