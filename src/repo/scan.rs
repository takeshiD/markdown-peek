//! Repository and worktree scanning — the Layer 4 foundation (#14).
//!
//! Locates the enclosing git repository, enumerates its git worktrees and
//! provides a lightweight file walk used by the reference and TODO analyses.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Directories that are never worth walking for source/documentation analysis.
const IGNORED_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "dist",
    "build",
    ".venv",
    "vendor",
    ".cache",
];

/// A single git worktree as reported by `git worktree list --porcelain`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    /// Short branch name (`refs/heads/foo` → `foo`), if the worktree is on a branch.
    pub branch: Option<String>,
    pub head: Option<String>,
    pub is_bare: bool,
    pub is_detached: bool,
    /// True when this worktree contains the document being analysed.
    pub is_current: bool,
}

/// Walk up from `start` looking for the git repository root (a directory that
/// contains a `.git` entry — either a directory or a gitdir-file for worktrees).
pub fn find_repo_root(start: &Path) -> Option<PathBuf> {
    let start = if start.is_file() {
        start.parent()?
    } else {
        start
    };
    let mut dir = start.canonicalize().ok();
    while let Some(cur) = dir {
        if cur.join(".git").exists() {
            return Some(cur);
        }
        dir = cur.parent().map(Path::to_path_buf);
    }
    None
}

/// List every worktree attached to the repository at `root`.
///
/// Uses `git worktree list --porcelain`. Returns an empty vec (rather than an
/// error) when git is unavailable or the command fails, so the rest of the
/// report can still render.
pub fn list_worktrees(root: &Path, current: &Path) -> Vec<WorktreeInfo> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["worktree", "list", "--porcelain"])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let current = current
        .canonicalize()
        .unwrap_or_else(|_| current.to_path_buf());
    parse_worktree_porcelain(&text, &current)
}

/// Parse the porcelain output of `git worktree list`. Records are separated by
/// blank lines; each begins with a `worktree <path>` line.
fn parse_worktree_porcelain(text: &str, current: &Path) -> Vec<WorktreeInfo> {
    let mut out = Vec::new();
    let mut cur: Option<WorktreeInfo> = None;
    for line in text.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(done) = cur.take() {
                out.push(done);
            }
            cur = Some(WorktreeInfo {
                path: PathBuf::from(path),
                branch: None,
                head: None,
                is_bare: false,
                is_detached: false,
                is_current: false,
            });
        } else if let Some(wt) = cur.as_mut() {
            if let Some(head) = line.strip_prefix("HEAD ") {
                wt.head = Some(head.to_string());
            } else if let Some(branch) = line.strip_prefix("branch ") {
                wt.branch = Some(branch.trim_start_matches("refs/heads/").to_string());
            } else if line == "bare" {
                wt.is_bare = true;
            } else if line == "detached" {
                wt.is_detached = true;
            }
        }
    }
    if let Some(done) = cur.take() {
        out.push(done);
    }
    for wt in &mut out {
        // A worktree is "current" when the analysed file lives under its path.
        let wt_path = wt.path.canonicalize().unwrap_or_else(|_| wt.path.clone());
        wt.is_current = current.starts_with(&wt_path);
    }
    out
}

/// Recursively collect files under `root`, skipping [`IGNORED_DIRS`] and hidden
/// directories. Follows no symlinks.
pub fn walk_files(root: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            if e.file_type().is_dir() {
                let name = e.file_name().to_string_lossy();
                !IGNORED_DIRS.contains(&name.as_ref())
            } else {
                true
            }
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_records() {
        let text = "\
worktree /repo/main
HEAD abc123
branch refs/heads/main

worktree /repo/wt/feature
HEAD def456
branch refs/heads/feature

worktree /repo/wt/detached
HEAD 999aaa
detached
";
        let wts = parse_worktree_porcelain(text, Path::new("/repo/wt/feature/src"));
        assert_eq!(wts.len(), 3);
        assert_eq!(wts[0].branch.as_deref(), Some("main"));
        assert_eq!(wts[0].head.as_deref(), Some("abc123"));
        assert!(!wts[0].is_current);
        assert_eq!(wts[1].branch.as_deref(), Some("feature"));
        assert!(wts[1].is_current);
        assert!(wts[2].is_detached);
        assert!(wts[2].branch.is_none());
    }

    #[test]
    fn ignores_target_and_git_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("target/debug")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(root.join("target/debug/junk.rs"), "x").unwrap();
        std::fs::write(root.join(".git/config"), "x").unwrap();
        let files = walk_files(root);
        let names: Vec<_> = files
            .iter()
            .map(|p| {
                p.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        assert!(names.contains(&"src/main.rs".to_string()));
        assert!(!names.iter().any(|n| n.starts_with("target")));
        assert!(!names.iter().any(|n| n.starts_with(".git")));
    }

    #[test]
    fn find_repo_root_walks_up() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        let found = find_repo_root(&root.join("a/b")).unwrap();
        assert_eq!(found, root);
    }
}
