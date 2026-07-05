//! 軽量 Markdown パーサ (Layer 1/2 の `BlockTree` が入るまでの繋ぎ)。
//!
//! ⚠ 統合時: このモジュールは丸ごと Layer 1 の
//! `mdpeek_core::parser::BlockTree` / `Block` に置換する。generator はこの
//! `ParsedDoc` の代わりに `BlockTree` を受け取る形に変える (README「統合手順」)。
//!
//! AGENTS.md §3.1 と同じ方針: `pulldown-cmark` の `into_offset_iter()` で各
//! イベントに byte range を持たせ、行頭 offset テーブルで `SourceRange`
//! (1 始まりの line/col) に変換する。パーサ乗り換えはしない。

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::seam::SourceRange;

/// byte offset ↔ (line, col) 変換テーブル。
pub struct LineIndex {
    /// 各行の開始 byte offset。
    line_starts: Vec<usize>,
    len: usize,
}

impl LineIndex {
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0usize];
        for (i, b) in source.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        LineIndex {
            line_starts,
            len: source.len(),
        }
    }

    /// byte offset → 1 始まりの (line, column)。
    fn line_col(&self, offset: usize) -> (u32, u32) {
        let offset = offset.min(self.len);
        // offset を含む行 = line_starts で offset 以下の最後の要素。
        let line = match self.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let col = offset - self.line_starts[line];
        ((line as u32) + 1, (col as u32) + 1)
    }

    /// byte range → SourceRange。
    pub fn source_range(&self, range: std::ops::Range<usize>) -> SourceRange {
        let (start_line, start_column) = self.line_col(range.start);
        let (end_line, end_column) = self.line_col(range.end);
        SourceRange {
            start_line,
            start_column,
            end_line,
            end_column,
        }
    }
}

/// 抽出したトップレベルブロック。
#[derive(Debug, Clone)]
pub struct ParsedBlock {
    pub kind: BlockKind,
    pub range: SourceRange,
    /// ブロックの平文テキスト (見出し語・段落本文・コード中身)。
    pub text: String,
    /// リスト項目 (List のとき)。各項目テキスト + range。
    pub items: Vec<ListItem>,
    /// 表 (Table のとき)。先頭行がヘッダ。
    pub table: Option<Table>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    Heading { level: u8 },
    Paragraph,
    List { ordered: bool },
    Table,
    CodeBlock,
    Other,
}

#[derive(Debug, Clone)]
pub struct ListItem {
    pub text: String,
    pub range: SourceRange,
}

#[derive(Debug, Clone)]
pub struct Table {
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// パース済みドキュメント。
pub struct ParsedDoc {
    pub blocks: Vec<ParsedBlock>,
    pub line_index: LineIndex,
}

impl ParsedDoc {
    pub fn parse(source: &str) -> Self {
        let line_index = LineIndex::new(source);
        let mut opts = Options::empty();
        opts.insert(Options::ENABLE_TABLES);
        opts.insert(Options::ENABLE_STRIKETHROUGH);
        opts.insert(Options::ENABLE_TASKLISTS);

        let blocks = collect_blocks(source, opts, &line_index);
        ParsedDoc { blocks, line_index }
    }

    /// 指定レベルの見出しを順に返す。
    pub fn headings(&self) -> impl Iterator<Item = &ParsedBlock> {
        self.blocks
            .iter()
            .filter(|b| matches!(b.kind, BlockKind::Heading { .. }))
    }

    /// 全表を返す。
    pub fn tables(&self) -> impl Iterator<Item = &ParsedBlock> {
        self.blocks.iter().filter(|b| b.kind == BlockKind::Table)
    }
}

/// pulldown-cmark のイベント列をトップレベルブロックに畳む。
fn collect_blocks(source: &str, opts: Options, line_index: &LineIndex) -> Vec<ParsedBlock> {
    let mut blocks = Vec::new();
    let parser = Parser::new_ext(source, opts).into_offset_iter();

    // ネスト深さ 0 のときだけ新しいトップレベルブロックを開始する。
    let mut depth: i32 = 0;
    let mut current: Option<Builder> = None;
    // テーブルセル収集用の状態。
    let mut in_table = false;
    let mut in_head = false;
    let mut cell = String::new();
    let mut row: Vec<String> = Vec::new();
    // リスト項目収集用。
    let mut item_text = String::new();
    let mut item_range: Option<std::ops::Range<usize>> = None;

    for (event, range) in parser {
        match event {
            Event::Start(tag) => {
                match &tag {
                    Tag::Heading { level, .. } if depth == 0 => {
                        current = Some(Builder::new(
                            BlockKind::Heading {
                                level: *level as u8,
                            },
                            range.clone(),
                        ));
                    }
                    Tag::Paragraph if depth == 0 => {
                        current = Some(Builder::new(BlockKind::Paragraph, range.clone()));
                    }
                    Tag::CodeBlock(_) if depth == 0 => {
                        current = Some(Builder::new(BlockKind::CodeBlock, range.clone()));
                    }
                    Tag::List(first) if depth == 0 => {
                        current = Some(Builder::new(
                            BlockKind::List {
                                ordered: first.is_some(),
                            },
                            range.clone(),
                        ));
                    }
                    Tag::Table(_) if depth == 0 => {
                        in_table = true;
                        current = Some(Builder::new(BlockKind::Table, range.clone()));
                    }
                    Tag::TableHead => in_head = true,
                    Tag::TableCell => cell.clear(),
                    Tag::Item => {
                        item_text.clear();
                        item_range = Some(range.clone());
                    }
                    _ => {}
                }
                depth += 1;
            }
            Event::End(end) => {
                depth -= 1;
                match end {
                    TagEnd::TableCell => row.push(cell.trim().to_string()),
                    TagEnd::TableRow | TagEnd::TableHead => {
                        if let Some(b) = current.as_mut() {
                            let t = b.table.get_or_insert_with(|| Table {
                                header: Vec::new(),
                                rows: Vec::new(),
                            });
                            if in_head {
                                t.header = std::mem::take(&mut row);
                            } else {
                                t.rows.push(std::mem::take(&mut row));
                            }
                        }
                        in_head = false;
                    }
                    TagEnd::Item => {
                        if let (Some(b), Some(r)) = (current.as_mut(), item_range.take()) {
                            b.items.push(ListItem {
                                text: item_text.trim().to_string(),
                                range: line_index.source_range(r),
                            });
                        }
                    }
                    TagEnd::Table => in_table = false,
                    _ => {}
                }
                if depth == 0
                    && let Some(b) = current.take()
                {
                    blocks.push(b.finish(line_index));
                }
            }
            Event::Text(t) | Event::Code(t) => {
                if in_table {
                    cell.push_str(&t);
                } else if item_range.is_some() {
                    item_text.push_str(&t);
                } else if let Some(b) = current.as_mut() {
                    b.text.push_str(&t);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if item_range.is_some() {
                    item_text.push(' ');
                } else if let Some(b) = current.as_mut() {
                    b.text.push(' ');
                }
            }
            _ => {}
        }
    }

    blocks
}

struct Builder {
    kind: BlockKind,
    byte_range: std::ops::Range<usize>,
    text: String,
    items: Vec<ListItem>,
    table: Option<Table>,
}

impl Builder {
    fn new(kind: BlockKind, byte_range: std::ops::Range<usize>) -> Self {
        Builder {
            kind,
            byte_range,
            text: String::new(),
            items: Vec::new(),
            table: None,
        }
    }
    fn finish(self, line_index: &LineIndex) -> ParsedBlock {
        ParsedBlock {
            kind: self.kind,
            range: line_index.source_range(self.byte_range),
            text: self.text.trim().to_string(),
            items: self.items,
            table: self.table,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_index_maps_offsets() {
        let src = "ab\ncde\n";
        let idx = LineIndex::new(src);
        assert_eq!(idx.line_col(0), (1, 1));
        assert_eq!(idx.line_col(1), (1, 2));
        assert_eq!(idx.line_col(3), (2, 1)); // after first newline
        assert_eq!(idx.line_col(4), (2, 2));
    }

    #[test]
    fn parses_heading_and_paragraph() {
        let doc = ParsedDoc::parse("# Title\n\nhello world\n");
        let heads: Vec<_> = doc.headings().collect();
        assert_eq!(heads.len(), 1);
        assert_eq!(heads[0].text, "Title");
        assert_eq!(heads[0].kind, BlockKind::Heading { level: 1 });
        assert_eq!(heads[0].range.start_line, 1);
    }

    #[test]
    fn parses_table() {
        let md = "| 品目 | 数量 |\n|---|---|\n| ネジ | 4 |\n| 板 | 2 |\n";
        let doc = ParsedDoc::parse(md);
        let table = doc.tables().next().unwrap().table.as_ref().unwrap();
        assert_eq!(table.header, vec!["品目", "数量"]);
        assert_eq!(table.rows, vec![vec!["ネジ", "4"], vec!["板", "2"]]);
    }

    #[test]
    fn parses_list_items_with_ranges() {
        let md = "1. まず粉をふるう\n2. 卵を混ぜる\n";
        let doc = ParsedDoc::parse(md);
        let list = doc
            .blocks
            .iter()
            .find(|b| matches!(b.kind, BlockKind::List { .. }))
            .unwrap();
        assert_eq!(list.items.len(), 2);
        assert_eq!(list.items[0].text, "まず粉をふるう");
        assert_eq!(list.items[1].range.start_line, 2);
    }
}
