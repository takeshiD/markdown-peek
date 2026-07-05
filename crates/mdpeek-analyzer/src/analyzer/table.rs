//! Table semantics (AGENTS.md §3.2 "表の意味推定(status列など)").
//!
//! The block tree folds a table's cell structure into aggregated text, so this
//! module re-reads the table's source lines (via its [`SourceRange`]) to recover
//! the header row and detect meaningful columns — notably a status/state column,
//! which later layers render as badges rather than plain text.

use mdpeek_parser::{Block, BlockKind, BlockTree};

/// The recognised shape of a GFM table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableInfo {
    pub columns: Vec<String>,
    /// Index of a column that looks like a status/state column, if any.
    pub status_column: Option<usize>,
    pub row_count: usize,
}

/// Analyse a table block by re-reading its source lines. Returns `None` if the
/// block is not a table or has no parseable header.
pub fn analyze(source: &str, block: &Block) -> Option<TableInfo> {
    if !matches!(block.kind, BlockKind::Table) {
        return None;
    }

    // The block's source range spans header + delimiter + body rows. Slice those
    // source lines (1-based, inclusive) and keep the pipe-bearing ones.
    let start = block.range.start_line.saturating_sub(1) as usize;
    let end = block.range.end_line as usize; // exclusive upper bound after skip
    let count = end.saturating_sub(start);
    let mut lines = source
        .lines()
        .skip(start)
        .take(count)
        .map(str::trim)
        .filter(|l| l.contains('|') && !l.is_empty());

    let header = lines.next()?;
    let columns = split_row(header);
    if columns.is_empty() {
        return None;
    }

    // The line after the header is the `|---|---|` delimiter; the rest are rows.
    let mut remaining = lines.peekable();
    if remaining.peek().is_some_and(|l| is_delimiter_row(l)) {
        remaining.next();
    }
    let row_count = remaining.count();

    let status_column = columns.iter().position(|c| is_status_label(c));

    Some(TableInfo {
        columns,
        status_column,
        row_count,
    })
}

/// Analyse every table in the tree.
pub fn analyze_all(source: &str, tree: &BlockTree) -> Vec<(mdpeek_parser::BlockId, TableInfo)> {
    tree.iter()
        .filter_map(|b| analyze(source, b).map(|info| (b.id, info)))
        .collect()
}

/// Split a `| a | b |` row into trimmed cell contents.
fn split_row(row: &str) -> Vec<String> {
    let row = row.trim();
    let row = row.strip_prefix('|').unwrap_or(row);
    let row = row.strip_suffix('|').unwrap_or(row);
    row.split('|').map(|c| c.trim().to_string()).collect()
}

/// A delimiter row is made only of `-`, `:`, `|` and whitespace.
fn is_delimiter_row(row: &str) -> bool {
    let content: String = row.chars().filter(|c| !c.is_whitespace()).collect();
    !content.is_empty()
        && content.chars().all(|c| c == '-' || c == ':' || c == '|')
        && content.contains('-')
}

fn is_status_label(label: &str) -> bool {
    let l = label.to_lowercase();
    matches!(
        l.as_str(),
        "status" | "state" | "状態" | "ステータス" | "進捗" | "結果"
    ) || l.contains("status")
        || l.contains("state")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdpeek_parser::BlockTree;

    fn first_table(source: &str) -> TableInfo {
        let tree = BlockTree::parse(source);
        let block = tree
            .iter()
            .find(|b| matches!(b.kind, BlockKind::Table))
            .expect("table present")
            .clone();
        analyze(source, &block).expect("table analysed")
    }

    #[test]
    fn extracts_columns_and_rows() {
        let md = "| Name | Age |\n|------|-----|\n| Ann  | 30  |\n| Bob  | 25  |\n";
        let info = first_table(md);
        assert_eq!(info.columns, vec!["Name", "Age"]);
        assert_eq!(info.row_count, 2);
        assert_eq!(info.status_column, None);
    }

    #[test]
    fn detects_status_column() {
        let md = "| Task | Status |\n|------|--------|\n| A | done |\n";
        let info = first_table(md);
        assert_eq!(info.status_column, Some(1));
    }

    #[test]
    fn detects_japanese_status_column() {
        let md = "| 項目 | 状態 |\n|------|------|\n| A | 完了 |\n";
        let info = first_table(md);
        assert_eq!(info.columns, vec!["項目", "状態"]);
        assert_eq!(info.status_column, Some(1));
    }

    #[test]
    fn non_table_returns_none() {
        let tree = BlockTree::parse("just a paragraph\n");
        let para = tree.iter().next().unwrap();
        assert_eq!(analyze("just a paragraph\n", para), None);
    }
}
