//! README ↔ actual-file correspondence and docs-code consistency.
//!
//! Extracts every file reference from a markdown document — link/image
//! destinations plus inline-code spans that look like paths — resolves them
//! against the working tree and flags the ones that do not resolve.

use pulldown_cmark::{Event, Parser, Tag};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// One file reference found in the document and its resolution status.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FileRef {
    /// The raw target as written in the document.
    pub target: String,
    /// Where the reference came from.
    pub kind: RefKind,
    /// Resolved path, when the target pointed at a local file that exists.
    pub resolved: Option<PathBuf>,
    pub exists: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RefKind {
    Link,
    Image,
    /// A path spotted inside an inline `code` span.
    Code,
}

/// The set of references extracted from one document.
#[derive(Debug, Clone, Serialize, Default)]
pub struct DocRefReport {
    pub refs: Vec<FileRef>,
}

impl DocRefReport {
    pub fn broken(&self) -> impl Iterator<Item = &FileRef> {
        self.refs.iter().filter(|r| !r.exists)
    }

    pub fn ok_count(&self) -> usize {
        self.refs.iter().filter(|r| r.exists).count()
    }
}

/// Extract and resolve references from `content` (the document's markdown).
///
/// `doc_dir` is the directory containing the document; relative references are
/// resolved against it, and absolute-looking references (`/foo`) against
/// `repo_root`.
pub fn analyze_doc(content: &str, doc_dir: &Path, repo_root: &Path) -> DocRefReport {
    let mut refs = Vec::new();
    let parser = Parser::new(content);
    for event in parser {
        match event {
            Event::Start(Tag::Link { dest_url, .. }) => {
                push_ref(
                    &mut refs,
                    dest_url.as_ref(),
                    RefKind::Link,
                    doc_dir,
                    repo_root,
                );
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                push_ref(
                    &mut refs,
                    dest_url.as_ref(),
                    RefKind::Image,
                    doc_dir,
                    repo_root,
                );
            }
            Event::Code(text) if looks_like_path(&text) => {
                push_ref(&mut refs, &text, RefKind::Code, doc_dir, repo_root);
            }
            _ => {}
        }
    }
    DocRefReport { refs }
}

/// Resolve a single reference target and record it if it points at the local
/// tree. External URLs, anchors and mailto links are skipped.
fn push_ref(
    refs: &mut Vec<FileRef>,
    target: &str,
    kind: RefKind,
    doc_dir: &Path,
    repo_root: &Path,
) {
    if !is_local_target(target) {
        return;
    }
    // Drop any `#fragment` / `?query` suffix before touching the filesystem.
    let path_part = target.split(['#', '?']).next().unwrap_or(target).trim();
    if path_part.is_empty() {
        return;
    }
    let candidate = if let Some(rest) = path_part.strip_prefix('/') {
        repo_root.join(rest)
    } else {
        doc_dir.join(path_part)
    };
    let exists = candidate.exists();
    refs.push(FileRef {
        target: target.to_string(),
        kind,
        resolved: exists.then(|| candidate.clone()),
        exists,
    });
}

/// Whether a link target refers to something inside the repository (as opposed
/// to an external URL, an in-page anchor, or a mail link).
fn is_local_target(target: &str) -> bool {
    let t = target.trim();
    if t.is_empty() || t.starts_with('#') {
        return false;
    }
    // Any URL scheme (http:, https:, mailto:, tel:, ftp:, data:) is external.
    if let Some(colon) = t.find(':') {
        let scheme = &t[..colon];
        if scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
            && !scheme.is_empty()
            && t[colon..].starts_with(":/")
        {
            return false;
        }
        // mailto:foo@bar has no `:/` — still external.
        if scheme.eq_ignore_ascii_case("mailto") || scheme.eq_ignore_ascii_case("tel") {
            return false;
        }
    }
    if t.starts_with("//") {
        return false; // protocol-relative URL
    }
    true
}

/// Heuristic for whether an inline-code span names a file path worth checking:
/// it contains a `/` or a dotted extension and no whitespace.
fn looks_like_path(code: &str) -> bool {
    let c = code.trim();
    if c.is_empty() || c.contains(char::is_whitespace) {
        return false;
    }
    if c.starts_with("http://") || c.starts_with("https://") {
        return false;
    }
    let has_slash = c.contains('/');
    // A dotted extension like `foo.rs` (but not a version like `1.2` or a
    // trailing dot).
    let has_ext = c.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty()
            && (1..=8).contains(&ext.len())
            && ext.chars().all(|ch| ch.is_ascii_alphanumeric())
            && ext.chars().any(|ch| ch.is_ascii_alphabetic())
    });
    has_slash || has_ext
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_local_vs_external() {
        assert!(is_local_target("./src/main.rs"));
        assert!(is_local_target("docs/guide.md"));
        assert!(is_local_target("/README.md"));
        assert!(!is_local_target("https://example.com"));
        assert!(!is_local_target("http://x.y/z"));
        assert!(!is_local_target("mailto:a@b.com"));
        assert!(!is_local_target("#section"));
        assert!(!is_local_target("//cdn.example.com/x.js"));
    }

    #[test]
    fn path_heuristic() {
        assert!(looks_like_path("src/main.rs"));
        assert!(looks_like_path("Cargo.toml"));
        assert!(looks_like_path("./a/b"));
        assert!(!looks_like_path("some text"));
        assert!(!looks_like_path("1.2"));
        assert!(!looks_like_path("cargo build"));
        assert!(!looks_like_path("https://x.y"));
    }

    #[test]
    fn resolves_and_flags_references() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::write(root.join("real.rs"), "fn main() {}").unwrap();
        std::fs::write(root.join("docs/guide.md"), "guide").unwrap();

        let content = "\
See [the guide](docs/guide.md) and [missing](docs/nope.md).
Source lives in `real.rs` but `ghost.rs` is gone.
External [site](https://example.com) is ignored.
![logo](/assets/logo.png)
";
        let report = analyze_doc(content, root, root);
        // guide.md (ok), nope.md (broken), real.rs (ok), ghost.rs (broken),
        // logo.png (broken). External link excluded.
        assert_eq!(report.refs.len(), 5);
        assert_eq!(report.ok_count(), 2);
        let broken: Vec<_> = report.broken().map(|r| r.target.clone()).collect();
        assert!(broken.contains(&"docs/nope.md".to_string()));
        assert!(broken.contains(&"ghost.rs".to_string()));
        assert!(broken.contains(&"/assets/logo.png".to_string()));
    }

    #[test]
    fn strips_fragments_before_resolving() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("guide.md"), "x").unwrap();
        let report = analyze_doc("[x](guide.md#heading)", root, root);
        assert_eq!(report.refs.len(), 1);
        assert!(report.refs[0].exists);
    }
}
