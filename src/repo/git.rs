//! Git-history awareness. Correlates documents (notably ADRs) with the commits
//! that last touched them, giving Layer 4 its "ADR ↔ git history" view.

use serde::Serialize;
use std::path::Path;
use std::process::Command;

/// The most recent commit that touched a given path.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LastCommit {
    pub hash: String,
    pub date: String,
    pub subject: String,
    /// Total number of commits in the file's history.
    pub commit_count: usize,
}

/// True when a usable `git` binary is on `PATH`.
pub fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Return the last commit that modified `path` (relative to `root`), or `None`
/// when the file is untracked or git is unavailable.
pub fn last_commit(root: &Path, path: &Path) -> Option<LastCommit> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "log",
            "-1",
            "--format=%h%x00%ad%x00%s",
            "--date=short",
            "--",
        ])
        .arg(path)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&out.stdout);
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let mut parts = line.split('\u{0}');
    let hash = parts.next()?.to_string();
    let date = parts.next().unwrap_or_default().to_string();
    let subject = parts.next().unwrap_or_default().to_string();
    let commit_count = count_commits(root, path);
    Some(LastCommit {
        hash,
        date,
        subject,
        commit_count,
    })
}

/// Count how many commits touched `path`.
fn count_commits(root: &Path, path: &Path) -> usize {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-list", "--count", "HEAD", "--"])
        .arg(path)
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .trim()
            .parse()
            .unwrap_or(0),
        _ => 0,
    }
}

/// An Architecture Decision Record correlated with its git history.
#[derive(Debug, Clone, Serialize)]
pub struct AdrInfo {
    /// Path relative to the repo root.
    pub path: String,
    /// First `# heading` in the document, used as its title.
    pub title: Option<String>,
    pub last_commit: Option<LastCommit>,
}

/// Whether a path looks like an ADR: it lives in an `adr`/`decisions` directory
/// or its filename matches the `adr-0001-...` / `0001-...` convention.
pub fn is_adr_path(rel: &Path) -> bool {
    let lower = rel.to_string_lossy().to_lowercase();
    let in_adr_dir = lower.split('/').any(|seg| {
        seg == "adr" || seg == "adrs" || seg == "decisions" || seg == "architecture-decisions"
    });
    let file = rel
        .file_name()
        .map(|f| f.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let is_md = file.ends_with(".md") || file.ends_with(".markdown");
    if !is_md {
        return false;
    }
    let named_adr = file.starts_with("adr-") || file.starts_with("adr_");
    // `NNNN-title.md` numeric-prefixed record.
    let numeric_prefix = file
        .split(['-', '_'])
        .next()
        .is_some_and(|p| p.len() >= 3 && p.chars().all(|c| c.is_ascii_digit()));
    in_adr_dir && is_md || named_adr || (numeric_prefix && in_adr_dir)
}

/// Pull the first level-1/2 markdown heading from `content`, for use as a title.
pub fn first_heading(content: &str) -> Option<String> {
    for line in content.lines() {
        let t = line.trim_start();
        if let Some(h) = t.strip_prefix("# ").or_else(|| t.strip_prefix("## ")) {
            let title = h.trim();
            if !title.is_empty() {
                return Some(title.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_adr_paths() {
        assert!(is_adr_path(Path::new("docs/adr/0001-use-rust.md")));
        assert!(is_adr_path(Path::new("decisions/0002-split-crates.md")));
        assert!(is_adr_path(Path::new("ADR-0003-caching.md")));
        assert!(is_adr_path(Path::new("doc/architecture-decisions/x.md")));
        assert!(!is_adr_path(Path::new("README.md")));
        assert!(!is_adr_path(Path::new("src/adr.rs")));
        assert!(!is_adr_path(Path::new("docs/adr/diagram.png")));
    }

    #[test]
    fn extracts_first_heading() {
        assert_eq!(
            first_heading("intro\n\n# Title Here\n\nbody"),
            Some("Title Here".to_string())
        );
        assert_eq!(
            first_heading("## Sub Title\ntext"),
            Some("Sub Title".to_string())
        );
        assert_eq!(first_heading("no heading here"), None);
    }
}
