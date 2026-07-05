//! SourceRange-aware Markdown parser (Layer 1 foundation).
//!
//! This module folds the `pulldown_cmark` event stream — obtained through
//! [`Parser::into_offset_iter`] so every event carries a byte range — into a
//! lightweight [`BlockTree`] of top-level blocks. Each [`Block`] records its
//! [`SourceRange`] (line/column span in the original document) and a
//! content-stable [`BlockId`].
//!
//! Nothing here replaces the existing emitters: the HTML / terminal renderers
//! keep consuming the raw `pulldown_cmark` stream directly. `BlockTree` is the
//! shared substrate the later layers build on:
//!
//! * SourceRange → jump-to-source / scroll-and-highlight links.
//! * `BlockId` → stable identity for incremental re-render and live-update
//!   diff highlighting (see issue #16).
//! * The block hierarchy + [`BlockTree::outline`] → outline / TOC panels and
//!   the rules analyzer (Layer 2).
//!
//! See `AGENTS.md` §3.1 / §10 Layer 1.
//!
//! The module is a foundation consumed by later layers; some public items are
//! not yet wired into the binary's render paths, hence the crate-level allow.
#![allow(dead_code)]

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, MetadataBlockKind, Parser, Tag};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ops::Range;

/// A span in the source document, expressed in 1-based lines and 1-based
/// columns. Columns are counted in UTF-8 **bytes** from the start of the line
/// (so they are stable without needing the source text, at the cost of not
/// being character-accurate for multi-byte runs). The end position is the
/// exclusive end of the span (one past the last byte).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRange {
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

/// Maps byte offsets to `(line, column)` positions.
///
/// Built in a single pass over the source by recording the byte offset of the
/// start of every line. Kept on the [`BlockTree`] so later consumers can turn
/// additional byte offsets into positions without re-scanning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineIndex {
    /// Byte offset of the first character of each line. Always starts with `0`.
    line_starts: Vec<usize>,
    /// Total length of the source in bytes (used to clamp out-of-range queries).
    len: usize,
}

impl LineIndex {
    /// Builds the index from the full source text.
    pub fn new(src: &str) -> Self {
        let mut line_starts = vec![0usize];
        for (i, b) in src.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        LineIndex {
            line_starts,
            len: src.len(),
        }
    }

    /// Converts a byte offset into a 1-based `(line, column)` pair. Offsets past
    /// the end of the source are clamped to the final position.
    pub fn line_col(&self, byte: usize) -> (u32, u32) {
        let byte = byte.min(self.len);
        let line = match self.line_starts.binary_search(&byte) {
            // `byte` is exactly the start of a line.
            Ok(i) => i,
            // `byte` falls inside line `i - 1` (i >= 1 because line_starts[0] == 0).
            Err(i) => i - 1,
        };
        let col = byte - self.line_starts[line] + 1;
        (line as u32 + 1, col as u32)
    }

    /// Converts a byte range into a [`SourceRange`].
    pub fn source_range(&self, range: Range<usize>) -> SourceRange {
        let (start_line, start_column) = self.line_col(range.start);
        let (end_line, end_column) = self.line_col(range.end);
        SourceRange {
            start_line,
            start_column,
            end_line,
            end_column,
        }
    }

    /// Number of lines in the source.
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }
}

/// Content-stable identifier for a [`Block`].
///
/// Derived from the block's kind and extracted text (not from its position), so
/// editing one block does not renumber the others. Blocks with identical
/// content are disambiguated by their occurrence order in the document. This
/// stability is what lets incremental re-render / live-highlight (#16) match a
/// block across successive parses of an edited document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockId(pub u64);

/// The structural kind of a [`Block`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BlockKind {
    /// ATX / setext heading, `level` in 1..=6.
    Heading {
        level: u8,
    },
    Paragraph,
    /// Fenced or indented code block. `language` is the fenced info string's
    /// first token, if any.
    CodeBlock {
        language: Option<String>,
    },
    BlockQuote,
    /// Ordered (`ordered = true`, `start` = first number) or bullet list.
    List {
        ordered: bool,
        start: Option<u64>,
    },
    /// List item. `task` is `Some(checked)` for GFM task-list items.
    Item {
        task: Option<bool>,
    },
    /// GFM table (rows/cells are folded into `text`, not child blocks).
    Table,
    ThematicBreak,
    FootnoteDefinition {
        label: String,
    },
    HtmlBlock,
    /// YAML / `+++` front matter block.
    MetadataBlock,
    DefinitionList,
    DefinitionTitle,
    DefinitionDetails,
}

/// A single node of the [`BlockTree`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub id: BlockId,
    pub kind: BlockKind,
    pub range: SourceRange,
    pub children: Vec<Block>,
    /// Text extracted for display / analysis (inline formatting flattened,
    /// soft breaks collapsed to spaces, trimmed).
    pub text: String,
}

impl Block {
    /// Depth-first iterator over this block and all its descendants.
    pub fn descendants(&self) -> impl Iterator<Item = &Block> {
        let mut stack = vec![self];
        std::iter::from_fn(move || {
            let node = stack.pop()?;
            // Push children in reverse so they are visited in document order.
            stack.extend(node.children.iter().rev());
            Some(node)
        })
    }
}

/// A lightweight, top-level-block tree of a Markdown document with source
/// positions. See the module docs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockTree {
    pub blocks: Vec<Block>,
    pub line_index: LineIndex,
}

/// One entry of a document [outline](BlockTree::outline): a heading and the
/// headings nested beneath it (by level).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutlineNode {
    pub id: BlockId,
    pub level: u8,
    pub title: String,
    pub range: SourceRange,
    pub children: Vec<OutlineNode>,
}

impl BlockTree {
    /// Parses `src` (with the shared GFM options) into a `BlockTree`.
    pub fn parse(src: &str) -> Self {
        Self::parse_with_options(src, mdpeek_gfm::parser_options())
    }

    /// Parses `src` with explicit `pulldown_cmark` options.
    pub fn parse_with_options(src: &str, options: pulldown_cmark::Options) -> Self {
        let line_index = LineIndex::new(src);
        let mut iter = Parser::new_ext(src, options).into_offset_iter();

        let mut blocks = Vec::new();
        let mut sink_text = String::new();
        let mut sink_task = None;
        consume(
            &mut iter,
            &line_index,
            true,
            &mut blocks,
            &mut sink_text,
            &mut sink_task,
        );

        // Second pass: assign content-stable ids in document (pre-)order.
        let mut seen = HashMap::new();
        assign_ids(&mut blocks, &mut seen);

        BlockTree { blocks, line_index }
    }

    /// Depth-first iterator over every block in the tree, in document order.
    pub fn iter(&self) -> impl Iterator<Item = &Block> {
        self.blocks.iter().flat_map(|b| b.descendants())
    }

    /// Finds a block by its [`BlockId`].
    pub fn find(&self, id: BlockId) -> Option<&Block> {
        self.iter().find(|b| b.id == id)
    }

    /// Returns the raw front matter text (YAML / `+++`) if the document opens
    /// with a metadata block. The delimiters themselves are not included — this
    /// is the body captured by `pulldown_cmark`. Used by the viewer's front
    /// matter panel (#19).
    pub fn frontmatter(&self) -> Option<&str> {
        match self.blocks.first() {
            Some(Block {
                kind: BlockKind::MetadataBlock,
                text,
                ..
            }) => Some(text.as_str()),
            _ => None,
        }
    }

    /// Builds the heading hierarchy (nested by heading level) for outline / TOC
    /// panels. Headings deeper than a preceding shallower heading become its
    /// children; e.g. an `H3` after an `H2` nests under it.
    pub fn outline(&self) -> Vec<OutlineNode> {
        let mut roots: Vec<OutlineNode> = Vec::new();
        // Stack of (level, index-path) into the growing tree.
        let mut stack: Vec<u8> = Vec::new();

        for block in self.iter() {
            let BlockKind::Heading { level } = block.kind else {
                continue;
            };
            let node = OutlineNode {
                id: block.id,
                level,
                title: block.text.clone(),
                range: block.range,
                children: Vec::new(),
            };
            // Pop until the top of the stack is a strictly shallower heading.
            while stack.last().is_some_and(|&l| l >= level) {
                stack.pop();
            }
            // Descend into the current path and push there.
            push_at_depth(&mut roots, stack.len(), node);
            stack.push(level);
        }
        roots
    }
}

/// Pushes `node` as a child at `depth` levels down the last-branch of `roots`
/// (depth 0 == top level). The path always follows the most recently added
/// child, which matches the outline stack discipline.
fn push_at_depth(roots: &mut Vec<OutlineNode>, depth: usize, node: OutlineNode) {
    if depth == 0 {
        roots.push(node);
        return;
    }
    let last = roots
        .last_mut()
        .expect("outline path points at an existing parent");
    push_at_depth(&mut last.children, depth - 1, node);
}

/// Data captured from a block-level `Start` tag before its children are
/// consumed (task state is only known after the item body is read).
enum Seed {
    Heading(u8),
    Paragraph,
    CodeBlock(Option<String>),
    BlockQuote,
    List { ordered: bool, start: Option<u64> },
    Item,
    Table,
    FootnoteDefinition(String),
    HtmlBlock,
    MetadataBlock,
    DefinitionList,
    DefinitionTitle,
    DefinitionDetails,
}

impl Seed {
    /// Whether this block may itself contain child blocks (as opposed to only
    /// inline content that should be flattened into `text`).
    fn holds_block_children(&self) -> bool {
        matches!(
            self,
            Seed::BlockQuote
                | Seed::List { .. }
                | Seed::Item
                | Seed::FootnoteDefinition(_)
                | Seed::DefinitionList
                | Seed::DefinitionDetails
        )
    }

    /// Finalizes into a [`BlockKind`], using the task marker discovered while
    /// consuming the block body.
    fn into_kind(self, task: Option<bool>) -> BlockKind {
        match self {
            Seed::Heading(level) => BlockKind::Heading { level },
            Seed::Paragraph => BlockKind::Paragraph,
            Seed::CodeBlock(language) => BlockKind::CodeBlock { language },
            Seed::BlockQuote => BlockKind::BlockQuote,
            Seed::List { ordered, start } => BlockKind::List { ordered, start },
            Seed::Item => BlockKind::Item { task },
            Seed::Table => BlockKind::Table,
            Seed::FootnoteDefinition(label) => BlockKind::FootnoteDefinition { label },
            Seed::HtmlBlock => BlockKind::HtmlBlock,
            Seed::MetadataBlock => BlockKind::MetadataBlock,
            Seed::DefinitionList => BlockKind::DefinitionList,
            Seed::DefinitionTitle => BlockKind::DefinitionTitle,
            Seed::DefinitionDetails => BlockKind::DefinitionDetails,
        }
    }
}

/// Maps a block-level `Tag` to its [`Seed`]. Inline tags (emphasis, links, …)
/// and table-internal tags (head/row/cell) return `None`: their content is
/// flattened into the enclosing block's text.
fn seed_of(tag: &Tag) -> Option<Seed> {
    Some(match tag {
        Tag::Heading { level, .. } => Seed::Heading(heading_level(*level)),
        Tag::Paragraph => Seed::Paragraph,
        Tag::CodeBlock(kind) => Seed::CodeBlock(code_language(kind)),
        Tag::BlockQuote(_) => Seed::BlockQuote,
        Tag::List(start) => Seed::List {
            ordered: start.is_some(),
            start: *start,
        },
        Tag::Item => Seed::Item,
        Tag::Table(_) => Seed::Table,
        Tag::FootnoteDefinition(label) => Seed::FootnoteDefinition(label.to_string()),
        Tag::HtmlBlock => Seed::HtmlBlock,
        Tag::MetadataBlock(kind) => {
            // Both YAML and +++ styles fold to the same block kind.
            let _ = matches!(
                kind,
                MetadataBlockKind::YamlStyle | MetadataBlockKind::PlusesStyle
            );
            Seed::MetadataBlock
        }
        Tag::DefinitionList => Seed::DefinitionList,
        Tag::DefinitionListTitle => Seed::DefinitionTitle,
        Tag::DefinitionListDefinition => Seed::DefinitionDetails,
        // Inline and table-internal tags are not blocks.
        _ => return None,
    })
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

fn code_language(kind: &CodeBlockKind) -> Option<String> {
    match kind {
        CodeBlockKind::Fenced(info) => info
            .split_whitespace()
            .next()
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        CodeBlockKind::Indented => None,
    }
}

/// Recursively consumes the contents of the current container from `iter`,
/// returning when it reaches the `End` event that closes the container (that
/// `End` is consumed here). Block-level `Start` events become child [`Block`]s
/// when `collect_blocks` is set; inline content is flattened into `out_text`.
fn consume<'a, I>(
    iter: &mut I,
    li: &LineIndex,
    collect_blocks: bool,
    out_blocks: &mut Vec<Block>,
    out_text: &mut String,
    out_task: &mut Option<bool>,
) where
    I: Iterator<Item = (Event<'a>, Range<usize>)>,
{
    while let Some((event, range)) = iter.next() {
        match event {
            Event::Start(tag) => {
                if let Some(seed) = seed_of(&tag) {
                    let inner_collect = seed.holds_block_children();
                    let mut children = Vec::new();
                    let mut text = String::new();
                    let mut task = None;
                    consume(iter, li, inner_collect, &mut children, &mut text, &mut task);
                    let block = Block {
                        id: BlockId(0), // assigned in the id pass
                        kind: seed.into_kind(task),
                        range: li.source_range(range),
                        children,
                        text: normalize_text(&text),
                    };
                    if collect_blocks {
                        out_blocks.push(block);
                    } else {
                        // Defensive: a block where only text is expected — keep
                        // its text so nothing is silently dropped.
                        out_text.push_str(&block.text);
                    }
                } else {
                    // Inline or table-internal container: flatten its content
                    // into the current text (it produces no child blocks).
                    consume(iter, li, false, out_blocks, out_text, out_task);
                }
            }
            Event::End(_) => return,
            Event::Text(t) | Event::Code(t) => out_text.push_str(&t),
            Event::InlineMath(t) | Event::DisplayMath(t) => out_text.push_str(&t),
            Event::Html(t) | Event::InlineHtml(t) => out_text.push_str(&t),
            Event::SoftBreak => out_text.push(' '),
            Event::HardBreak => out_text.push('\n'),
            Event::Rule => {
                if collect_blocks {
                    out_blocks.push(Block {
                        id: BlockId(0),
                        kind: BlockKind::ThematicBreak,
                        range: li.source_range(range),
                        children: Vec::new(),
                        text: String::new(),
                    });
                }
            }
            Event::TaskListMarker(checked) => *out_task = Some(checked),
            Event::FootnoteReference(_) => {}
        }
    }
}

/// Trims surrounding whitespace from extracted block text.
fn normalize_text(text: &str) -> String {
    text.trim().to_string()
}

/// Assigns content-stable ids to `blocks` and their descendants in pre-order.
/// Blocks that hash identically are disambiguated by occurrence count.
fn assign_ids(blocks: &mut [Block], seen: &mut HashMap<u64, u32>) {
    for block in blocks.iter_mut() {
        let base = content_hash(&block.kind, &block.text);
        let occurrence = seen.entry(base).or_insert(0);
        block.id = BlockId(mix(base, *occurrence));
        *occurrence += 1;
        assign_ids(&mut block.children, seen);
    }
}

/// Position-independent hash of a block's identity (kind + extracted text).
fn content_hash(kind: &BlockKind, text: &str) -> u64 {
    let mut h = DefaultHasher::new();
    // Discriminant + kind-carried data.
    std::mem::discriminant(kind).hash(&mut h);
    match kind {
        BlockKind::Heading { level } => level.hash(&mut h),
        BlockKind::CodeBlock { language } => language.hash(&mut h),
        BlockKind::List { ordered, start } => {
            ordered.hash(&mut h);
            start.hash(&mut h);
        }
        BlockKind::Item { task } => task.hash(&mut h),
        BlockKind::FootnoteDefinition { label } => label.hash(&mut h),
        _ => {}
    }
    text.hash(&mut h);
    h.finish()
}

/// Combines a content hash with an occurrence counter into a final id.
fn mix(base: u64, occurrence: u32) -> u64 {
    let mut h = DefaultHasher::new();
    base.hash(&mut h);
    occurrence.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_index_maps_offsets() {
        let src = "a\nbb\nccc";
        let li = LineIndex::new(src);
        assert_eq!(li.line_col(0), (1, 1)); // 'a'
        assert_eq!(li.line_col(1), (1, 2)); // newline (end of line 1)
        assert_eq!(li.line_col(2), (2, 1)); // 'b'
        assert_eq!(li.line_col(5), (3, 1)); // 'c'
        assert_eq!(li.line_col(8), (3, 4)); // end of source (clamped)
        // Past the end clamps to the final position.
        assert_eq!(li.line_col(999), (3, 4));
        assert_eq!(li.line_count(), 3);
    }

    #[test]
    fn heading_and_paragraph_ranges() {
        let src = "# Title\n\nHello world.\n";
        let tree = BlockTree::parse(src);
        assert_eq!(tree.blocks.len(), 2);

        let heading = &tree.blocks[0];
        assert_eq!(heading.kind, BlockKind::Heading { level: 1 });
        assert_eq!(heading.text, "Title");
        assert_eq!(heading.range.start_line, 1);
        assert_eq!(heading.range.start_column, 1);

        let para = &tree.blocks[1];
        assert_eq!(para.kind, BlockKind::Paragraph);
        assert_eq!(para.text, "Hello world.");
        assert_eq!(para.range.start_line, 3);
    }

    #[test]
    fn soft_break_becomes_space() {
        let src = "one\ntwo\n";
        let tree = BlockTree::parse(src);
        assert_eq!(tree.blocks[0].text, "one two");
    }

    #[test]
    fn fenced_code_block_language() {
        let src = "```rust\nfn main() {}\n```\n";
        let tree = BlockTree::parse(src);
        assert_eq!(tree.blocks.len(), 1);
        assert_eq!(
            tree.blocks[0].kind,
            BlockKind::CodeBlock {
                language: Some("rust".to_string())
            }
        );
        assert!(tree.blocks[0].text.contains("fn main"));
    }

    #[test]
    fn indented_code_block_has_no_language() {
        let src = "    let x = 1;\n";
        let tree = BlockTree::parse(src);
        assert_eq!(tree.blocks[0].kind, BlockKind::CodeBlock { language: None });
    }

    #[test]
    fn task_list_items_carry_checked_state() {
        let src = "- [x] done\n- [ ] todo\n- plain\n";
        let tree = BlockTree::parse(src);
        assert_eq!(tree.blocks.len(), 1);
        let list = &tree.blocks[0];
        assert_eq!(
            list.kind,
            BlockKind::List {
                ordered: false,
                start: None
            }
        );
        assert_eq!(list.children.len(), 3);
        assert_eq!(list.children[0].kind, BlockKind::Item { task: Some(true) });
        assert_eq!(list.children[1].kind, BlockKind::Item { task: Some(false) });
        assert_eq!(list.children[2].kind, BlockKind::Item { task: None });
        assert_eq!(list.children[0].text, "done");
    }

    #[test]
    fn ordered_list_records_start() {
        let src = "3. three\n4. four\n";
        let tree = BlockTree::parse(src);
        assert_eq!(
            tree.blocks[0].kind,
            BlockKind::List {
                ordered: true,
                start: Some(3)
            }
        );
    }

    #[test]
    fn blockquote_nests_children() {
        let src = "> quoted paragraph\n>\n> second\n";
        let tree = BlockTree::parse(src);
        assert_eq!(tree.blocks.len(), 1);
        let bq = &tree.blocks[0];
        assert_eq!(bq.kind, BlockKind::BlockQuote);
        // Two paragraphs inside the quote.
        assert_eq!(bq.children.len(), 2);
        assert!(bq.children.iter().all(|c| c.kind == BlockKind::Paragraph));
    }

    #[test]
    fn table_is_single_block() {
        let src = "| a | b |\n|---|---|\n| 1 | 2 |\n";
        let tree = BlockTree::parse(src);
        assert_eq!(tree.blocks.len(), 1);
        assert_eq!(tree.blocks[0].kind, BlockKind::Table);
        // No child blocks: rows/cells are flattened into text.
        assert!(tree.blocks[0].children.is_empty());
        let text = &tree.blocks[0].text;
        assert!(text.contains('a') && text.contains('2'));
    }

    #[test]
    fn thematic_break_is_a_block() {
        let src = "para\n\n---\n\nmore\n";
        let tree = BlockTree::parse(src);
        let kinds: Vec<_> = tree.blocks.iter().map(|b| &b.kind).collect();
        assert!(kinds.contains(&&BlockKind::ThematicBreak));
    }

    #[test]
    fn frontmatter_becomes_metadata_block() {
        let src = "---\ntitle: Doc\n---\n\n# Heading\n";
        let tree = BlockTree::parse(src);
        assert_eq!(tree.blocks[0].kind, BlockKind::MetadataBlock);
        assert!(tree.blocks[0].text.contains("title"));
    }

    #[test]
    fn frontmatter_returns_leading_metadata_text() {
        let src = "---\ntitle: Doc\nauthor: me\n---\n\n# Heading\n";
        let tree = BlockTree::parse(src);
        let fm = tree.frontmatter().expect("frontmatter present");
        assert!(fm.contains("title: Doc"));
        assert!(fm.contains("author: me"));
    }

    #[test]
    fn frontmatter_absent_without_metadata_block() {
        let tree = BlockTree::parse("# Heading\n\nbody\n");
        assert!(tree.frontmatter().is_none());
    }

    #[test]
    fn identical_blocks_get_distinct_ids() {
        let src = "same\n\nsame\n";
        let tree = BlockTree::parse(src);
        assert_eq!(tree.blocks.len(), 2);
        assert_ne!(tree.blocks[0].id, tree.blocks[1].id);
    }

    #[test]
    fn block_id_is_stable_across_unrelated_edits() {
        // The heading's id must not change when an unrelated paragraph is added
        // elsewhere in the document (position-independent identity).
        let before = BlockTree::parse("# Stable Heading\n\noriginal para\n");
        let after = BlockTree::parse("# Stable Heading\n\noriginal para\n\nnew para\n");

        let id_before = before
            .iter()
            .find(|b| matches!(b.kind, BlockKind::Heading { .. }))
            .unwrap()
            .id;
        let id_after = after
            .iter()
            .find(|b| matches!(b.kind, BlockKind::Heading { .. }))
            .unwrap()
            .id;
        assert_eq!(id_before, id_after);
    }

    #[test]
    fn find_by_id_round_trips() {
        let tree = BlockTree::parse("# H\n\nbody\n");
        let id = tree.blocks[0].id;
        assert_eq!(tree.find(id).unwrap().text, "H");
    }

    #[test]
    fn outline_nests_by_heading_level() {
        let src = "\
# Top
intro
## Sub A
## Sub B
### Deep
# Second
";
        let tree = BlockTree::parse(src);
        let outline = tree.outline();

        assert_eq!(outline.len(), 2); // Top, Second
        assert_eq!(outline[0].title, "Top");
        assert_eq!(outline[0].level, 1);
        assert_eq!(outline[0].children.len(), 2); // Sub A, Sub B
        assert_eq!(outline[0].children[1].title, "Sub B");
        assert_eq!(outline[0].children[1].children.len(), 1); // Deep
        assert_eq!(outline[0].children[1].children[0].title, "Deep");
        assert_eq!(outline[1].title, "Second");
        assert!(outline[1].children.is_empty());
    }

    #[test]
    fn outline_handles_leading_deep_heading() {
        // A document starting at H3 should not panic and should treat it as a
        // top-level outline entry.
        let src = "### Only Deep\n\nbody\n";
        let tree = BlockTree::parse(src);
        let outline = tree.outline();
        assert_eq!(outline.len(), 1);
        assert_eq!(outline[0].level, 3);
        assert_eq!(outline[0].title, "Only Deep");
    }

    #[test]
    fn iter_visits_nested_blocks_in_order() {
        let src = "- a\n- b\n";
        let tree = BlockTree::parse(src);
        let texts: Vec<_> = tree
            .iter()
            .filter(|b| matches!(b.kind, BlockKind::Item { .. }))
            .map(|b| b.text.clone())
            .collect();
        assert_eq!(texts, vec!["a", "b"]);
    }

    #[test]
    fn emphasis_is_flattened_into_paragraph_text() {
        let src = "This is **bold** and *italic*.\n";
        let tree = BlockTree::parse(src);
        assert_eq!(tree.blocks.len(), 1);
        assert_eq!(tree.blocks[0].kind, BlockKind::Paragraph);
        assert_eq!(tree.blocks[0].text, "This is bold and italic.");
        assert!(tree.blocks[0].children.is_empty());
    }
}
