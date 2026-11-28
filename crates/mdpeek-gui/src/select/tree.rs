//! Directory-tree candidates from code blocks drawn with tree glyphs (#64).
//!
//! A code block such as
//!
//! ```text
//! markdown-peek/
//! ├── Cargo.toml          # [workspace]
//! ├── crates/
//! │   └── mdpeek-core/    # ★ core
//! └── web/
//! ```
//!
//! is read line by line, deterministically:
//! - the hierarchy comes from the column of the branch glyph (`├──`, `└──`,
//!   or the ASCII `|--`, `` `-- ``, `+--`), behind a prefix of `│` / `|` /
//!   spaces;
//! - a line without a prefix before the first branch (`markdown-peek/`) is a
//!   root;
//! - text after ` #` is the entry's comment; `★` / `☆` / `⭐` mark an entry.
//!
//! Names and comments are kept verbatim. Each entry carries the source line it
//! came from. A block that does not read as a tree gives `None` (no aid).

use crate::ir::SourceRange;
use crate::ir::node::TreeEntry;

/// Fewest entries worth a tree aid.
pub const MIN_ENTRIES: usize = 3;
/// Most entries accepted (larger blocks are left as they are).
pub const MAX_ENTRIES: usize = 1000;
/// Deepest nesting accepted.
pub const MAX_DEPTH: u32 = 32;
const MAX_NAME_CHARS: usize = 200;

/// Branch tokens, longest first. The ASCII ones are only branches when they
/// stand where a glyph would (after the prefix).
const BRANCHES: &[&str] = &[
    "├──",
    "└──",
    "┣━━",
    "┗━━",
    "├─",
    "└─",
    "┣━",
    "┗━",
    "|--",
    "`--",
    "+--",
    "\\--",
];

/// Characters that may stand before a branch (vertical rails and spaces).
fn is_rail(c: char) -> bool {
    matches!(c, '│' | '┃' | '|' | ' ' | '\u{a0}' | '\t')
}

fn is_marker(c: char) -> bool {
    matches!(c, '★' | '☆' | '⭐')
}

enum Line<'a> {
    /// Blank, rails only, or a rail-prefixed note (`│   # …`): skipped.
    Skip,
    /// `col` = char column of the branch glyph; `rest` = text after it.
    Branch { col: usize, rest: &'a str },
    /// Unprefixed text at column 0 (a root such as `markdown-peek/`).
    Plain(&'a str),
    /// Anything else (indented prose, …): not a tree.
    Other,
}

fn classify(line: &str) -> Line<'_> {
    let mut rails = false;
    for (col, (i, c)) in line.char_indices().enumerate() {
        let rest = &line[i..];
        if let Some(tok) = BRANCHES.iter().find(|t| rest.starts_with(**t)) {
            let after = rest[tok.len()..].trim_start_matches(['─', '━', '-']);
            return Line::Branch {
                col,
                rest: after.trim(),
            };
        }
        if c == '#' && rails {
            return Line::Skip;
        }
        if !is_rail(c) {
            return if col == 0 {
                Line::Plain(line.trim_end())
            } else {
                Line::Other
            };
        }
        rails = true;
    }
    Line::Skip
}

/// `"name   # comment"` → (`"name"`, `Some("comment")`). A `#` only starts a
/// comment at the start or after whitespace, so `C#` stays a name.
fn split_comment(text: &str) -> (String, Option<String>) {
    let mut prev_ws = true;
    for (i, c) in text.char_indices() {
        if c == '#' && prev_ws {
            let name = text[..i].trim().to_string();
            let comment = text[i..].trim_start_matches('#').trim();
            return (name, (!comment.is_empty()).then(|| comment.to_string()));
        }
        prev_ws = c.is_whitespace();
    }
    (text.trim().to_string(), None)
}

fn entry(text: &str, depth: u32, line_no: u32, line: &str) -> Option<TreeEntry> {
    let (name, comment) = split_comment(text);
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return None;
    }
    let marked =
        name.chars().any(is_marker) || comment.as_deref().is_some_and(|c| c.chars().any(is_marker));
    Some(TreeEntry {
        is_dir: name.ends_with('/') || name.ends_with('\\'),
        name,
        depth,
        comment,
        marked,
        source_range: Some(SourceRange {
            start_line: line_no,
            start_column: 1,
            end_line: line_no,
            end_column: line.len() as u32 + 1,
        }),
    })
}

/// `tree` output summary (`3 directories, 5 files`).
fn is_summary(line: &str) -> bool {
    let t = line.trim();
    let mut words = t.split_whitespace();
    words
        .next()
        .is_some_and(|w| w.chars().all(|c| c.is_ascii_digit()))
        && words.next().is_some_and(|w| w.starts_with("director"))
}

/// Parse the inner lines of a code block (fences removed). `first_line` is the
/// 1-based document line of `lines[0]`.
pub fn tree_from_code(lines: &[&str], first_line: u32) -> Option<Vec<TreeEntry>> {
    let mut entries: Vec<TreeEntry> = Vec::new();
    // Branch-glyph columns of the open ancestors.
    let mut stack: Vec<usize> = Vec::new();
    let mut has_root = false;
    let mut branches = 0usize;
    let mut content_lines = 0usize;

    for (i, line) in lines.iter().enumerate() {
        let line_no = first_line + i as u32;
        if line.trim().is_empty() {
            continue;
        }
        // A shell prompt before the tree (`$ tree src`).
        if entries.is_empty() && (line.starts_with("$ ") || line.starts_with("% ")) {
            continue;
        }
        if !entries.is_empty() && is_summary(line) {
            continue;
        }
        content_lines += 1;
        match classify(line) {
            Line::Skip => {}
            Line::Branch { col, rest } => {
                branches += 1;
                while stack.last().is_some_and(|&top| top > col) {
                    stack.pop();
                }
                if stack.last() != Some(&col) {
                    stack.push(col);
                }
                let depth = (stack.len() - 1) as u32 + u32::from(has_root);
                if depth > MAX_DEPTH {
                    return None;
                }
                entries.push(entry(rest, depth, line_no, line)?);
            }
            Line::Plain(text) => {
                // A root starts a (new) tree; a root that is not followed by
                // its branches is prose, checked below.
                if entries.last().is_some_and(|e| e.depth == 0 && has_root) {
                    return None;
                }
                stack.clear();
                has_root = true;
                entries.push(entry(text, 0, line_no, line)?);
            }
            Line::Other => return None,
        }
        if entries.len() > MAX_ENTRIES {
            return None;
        }
    }

    // A root must have children.
    if entries.last().is_some_and(|e| e.depth == 0 && has_root) {
        return None;
    }
    // Mostly branch lines, enough entries, and at least one nesting level.
    let nested = entries.windows(2).any(|w| w[1].depth > w[0].depth);
    if entries.len() < MIN_ENTRIES || !nested || branches * 5 < content_lines * 3 {
        return None;
    }
    // An entry with children is a folder even without a trailing `/`.
    let n = entries.len();
    for i in 0..n {
        if i + 1 < n && entries[i + 1].depth > entries[i].depth {
            entries[i].is_dir = true;
        }
    }
    Some(entries)
}

/// Words that mark a section as being about a layout (raises the rules score).
pub fn is_tree_heading(h: &str) -> bool {
    let l = h.to_lowercase();
    [
        "director",
        "layout",
        "structure",
        "tree",
        "folder",
        "crate",
        "ディレクトリ",
        "フォルダ",
        "構成",
        "ツリー",
        "配置",
    ]
    .iter()
    .any(|w| l.contains(w))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Option<Vec<TreeEntry>> {
        let lines: Vec<&str> = src.lines().collect();
        tree_from_code(&lines, 10)
    }

    fn shape(es: &[TreeEntry]) -> Vec<(u32, &str, bool)> {
        es.iter()
            .map(|e| (e.depth, e.name.as_str(), e.is_dir))
            .collect()
    }

    #[test]
    fn unicode_tree_with_root_comments_and_marker() {
        let src = "\
markdown-peek/
├── Cargo.toml                      # [workspace]
├── crates/
│   ├── mdpeek-core/                # ★ 解析〜IR〜キャッシュの心臓部
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   └── parser/             # markdown -> block tree
│   │   └── Cargo.toml
│   └── mdpeek-cli/                 # バイナリ `mdpeek`
└── docs/
    └── architecture.md             # 本書
";
        let es = parse(src).unwrap();
        assert_eq!(
            shape(&es),
            [
                (0, "markdown-peek/", true),
                (1, "Cargo.toml", false),
                (1, "crates/", true),
                (2, "mdpeek-core/", true),
                (3, "src/", true),
                (4, "lib.rs", false),
                (4, "parser/", true),
                (3, "Cargo.toml", false),
                (2, "mdpeek-cli/", true),
                (1, "docs/", true),
                (2, "architecture.md", false),
            ]
        );
        assert_eq!(es[1].comment.as_deref(), Some("[workspace]"));
        assert_eq!(
            es[3].comment.as_deref(),
            Some("★ 解析〜IR〜キャッシュの心臓部")
        );
        assert!(es[3].marked);
        assert!(!es[2].marked);
        assert_eq!(es[8].comment.as_deref(), Some("バイナリ `mdpeek`"));
        // Source line of each entry (first line = 10).
        assert_eq!(es[0].source_range.unwrap().start_line, 10);
        assert_eq!(es[10].source_range.unwrap().start_line, 20);
    }

    #[test]
    fn agents_md_section_2_tree() {
        let md = include_str!("../../../../AGENTS.md");
        let start = md.find("```\nmarkdown-peek/").expect("AGENTS.md §2 tree") + 4;
        let end = start + md[start..].find("```").unwrap();
        let es = parse(&md[start..end]).unwrap();
        assert_eq!(es[0].name, "markdown-peek/");
        assert!(es.len() > 50, "{}", es.len());
        let core = es.iter().find(|e| e.name == "mdpeek-core/").unwrap();
        assert_eq!(core.depth, 2);
        assert!(core.marked);
        let web = es.iter().find(|e| e.name == "web/").unwrap();
        assert!(web.marked && web.depth == 1);
        let recipe = es.iter().find(|e| e.name == "recipe.rs").unwrap();
        assert_eq!(recipe.depth, 6);
        assert_eq!(recipe.comment.as_deref(), Some("レシピ(手順書の身近な例)"));
        // The rail-prefixed note line (`│ … # 注: …`) is skipped, not an entry.
        assert!(es.iter().all(|e| !e.name.starts_with("注")));
        // `security/` keeps its children after the note line.
        let i = es.iter().position(|e| e.name == "security/").unwrap();
        assert_eq!(es[i + 1].name, "mod.rs");
        assert_eq!(es[i + 1].depth, es[i].depth + 1);
    }

    #[test]
    fn ascii_variants() {
        let src = "\
.
|-- src
|   |-- main.rs   # entry
|   `-- lib.rs
`-- tests
    +-- it.rs
";
        let es = parse(src).unwrap();
        assert_eq!(
            shape(&es),
            [
                (0, ".", true),
                (1, "src", true),
                (2, "main.rs", false),
                (2, "lib.rs", false),
                (1, "tests", true),
                (2, "it.rs", false),
            ]
        );
        assert_eq!(es[2].comment.as_deref(), Some("entry"));
    }

    #[test]
    fn no_root_and_cjk_names() {
        let src = "\
├── 設計書/
│   ├── 概要.md       # はじめに読む ☆
│   └── 詳細.md
└── 議事録/
    └── 2026-11-01.md
";
        let es = parse(src).unwrap();
        assert_eq!(
            shape(&es),
            [
                (0, "設計書/", true),
                (1, "概要.md", false),
                (1, "詳細.md", false),
                (0, "議事録/", true),
                (1, "2026-11-01.md", false),
            ]
        );
        assert!(es[1].marked);
    }

    #[test]
    fn tree_command_output() {
        let src = "$ tree app\napp\n├── a.txt\n└── lib\n    └── b.c\n\n2 directories, 2 files\n";
        let es = parse(src).unwrap();
        assert_eq!(shape(&es)[0], (0, "app", true));
        assert_eq!(es.len(), 4);
    }

    #[test]
    fn hash_inside_a_name_is_not_a_comment() {
        let src = "root/\n├── C#/\n│   └── a.cs\n└── b # note\n";
        let es = parse(src).unwrap();
        assert_eq!(es[1].name, "C#/");
        assert_eq!(es[3].comment.as_deref(), Some("note"));
    }

    #[test]
    fn non_trees_give_none() {
        // Ordinary code.
        assert!(parse("fn main() {\n    println!(\"hi\");\n}\n").is_none());
        // A flat list without nesting.
        assert!(parse("├── a\n├── b\n└── c\n").is_none());
        // Too few entries.
        assert!(parse("root/\n└── a\n").is_none());
        // Prose mixed in.
        assert!(parse("root/\n├── a\n│   └── b\n    some indented prose\n").is_none());
        // A root without children followed by another root.
        assert!(parse("one\ntwo\n├── a\n│   └── b\n").is_none());
        // Shell commands that happen to contain `|--`.
        assert!(parse("cat a |--x\nls -la\necho done\n").is_none());
    }

    #[test]
    fn crlf_safe_names() {
        let src = "root/\r\n├── a/\r\n│   └── b\r\n└── c\r\n";
        let lines: Vec<&str> = src.split('\n').map(|l| l.trim_end_matches('\r')).collect();
        let es = tree_from_code(&lines, 1).unwrap();
        assert_eq!(es[3].name, "c");
    }
}
