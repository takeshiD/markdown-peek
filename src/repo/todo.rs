//! TODO ↔ issue correlation.
//!
//! Scans source files for `TODO` / `FIXME` / `HACK` / `XXX` markers, extracts
//! any issue reference on the same line (`#123`, `GH-123`, or an issue URL) and
//! reports which markers are tracked by an issue and which are orphaned.

use regex::Regex;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Source-file extensions worth scanning for code comments.
const SOURCE_EXTS: &[&str] = &[
    "rs", "js", "ts", "jsx", "tsx", "py", "go", "java", "kt", "c", "h", "cpp", "hpp", "cc", "rb",
    "php", "swift", "scala", "sh", "bash", "lua", "toml", "yaml", "yml",
];

/// A single TODO-family marker found in the tree.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TodoItem {
    /// Path relative to the repo root.
    pub file: String,
    pub line: usize,
    /// The marker keyword (TODO, FIXME, ...).
    pub kind: String,
    /// The comment text after the marker.
    pub text: String,
    /// Issue reference found on the line, if any (e.g. `#123`).
    pub issue: Option<String>,
}

/// Aggregate TODO report.
#[derive(Debug, Clone, Serialize, Default)]
pub struct TodoReport {
    pub items: Vec<TodoItem>,
}

impl TodoReport {
    pub fn linked(&self) -> usize {
        self.items.iter().filter(|i| i.issue.is_some()).count()
    }

    pub fn orphaned(&self) -> usize {
        self.items.iter().filter(|i| i.issue.is_none()).count()
    }
}

fn marker_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Marker must be a standalone word, optionally followed by `(...)` or `:`.
        Regex::new(r"\b(TODO|FIXME|HACK|XXX|BUG)\b[:(]?\s*(.*)").unwrap()
    })
}

fn issue_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // `#123`, an uppercase project key like `GH-123`/`PROJ-45` (JIRA style),
        // or an issues/pull URL. The key is required to be uppercase so ordinary
        // hyphenated words (`return-1`, `utf-8`) are not mistaken for issues.
        Regex::new(r"(#\d+|\b[A-Z][A-Z0-9]+-\d+\b|https?://\S+/(?:issues|pull)/\d+)").unwrap()
    })
}

/// Scan a set of files (already filtered to the repo) for markers.
pub fn scan(root: &Path, files: &[PathBuf]) -> TodoReport {
    let mut items = Vec::new();
    for file in files {
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        // Skip minified/bundled assets: they are vendored, unreadable, and full
        // of false-positive marker substrings.
        if name.contains(".min.") {
            continue;
        }
        let ext = file
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if !SOURCE_EXTS.contains(&ext.as_str()) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(file) else {
            continue;
        };
        let rel = file
            .strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        scan_content(&content, &rel, &mut items);
    }
    TodoReport { items }
}

/// Extract markers from one file's `content`.
fn scan_content(content: &str, rel: &str, items: &mut Vec<TodoItem>) {
    for (idx, line) in content.lines().enumerate() {
        if let Some(caps) = marker_re().captures(line) {
            let kind = caps.get(1).unwrap().as_str().to_string();
            let text = caps
                .get(2)
                .map(|m| m.as_str().trim())
                .unwrap_or("")
                .to_string();
            let issue = issue_re()
                .find(&text)
                .or_else(|| issue_re().find(line))
                .map(|m| m.as_str().to_string());
            items.push(TodoItem {
                file: rel.to_string(),
                line: idx + 1,
                kind,
                text,
                issue,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_markers_and_issue_refs() {
        let mut items = Vec::new();
        let content = "\
fn a() {} // TODO: rewrite this #42
fn b() {} // FIXME broken, see GH-7
fn c() {} // just a normal comment
// HACK(temp): workaround
let todos = 5; // not a marker word
";
        scan_content(content, "src/lib.rs", &mut items);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].kind, "TODO");
        assert_eq!(items[0].issue.as_deref(), Some("#42"));
        assert_eq!(items[1].kind, "FIXME");
        assert_eq!(items[1].issue.as_deref(), Some("GH-7"));
        assert_eq!(items[2].kind, "HACK");
        assert!(items[2].issue.is_none());
    }

    #[test]
    fn counts_linked_and_orphaned() {
        let mut items = Vec::new();
        scan_content(
            "// TODO #1\n// TODO nothing\n// FIXME #2",
            "x.rs",
            &mut items,
        );
        let report = TodoReport { items };
        assert_eq!(report.linked(), 2);
        assert_eq!(report.orphaned(), 1);
    }

    #[test]
    fn lowercase_hyphenated_words_are_not_issues() {
        let mut items = Vec::new();
        // `utf-8` and `return-1` look like keys but are lowercase, so they must
        // not be treated as issue references.
        scan_content(
            "// TODO handle utf-8 and return-1 paths\n",
            "x.rs",
            &mut items,
        );
        assert_eq!(items.len(), 1);
        assert!(items[0].issue.is_none());
    }

    #[test]
    fn ignores_marker_substrings() {
        let mut items = Vec::new();
        scan_content("let mytodos = TODONT();\n", "x.rs", &mut items);
        // "TODONT" is not a standalone TODO word.
        assert!(items.is_empty());
    }
}
