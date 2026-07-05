//! Repository + worktree markdown discovery for the explorer sidebar (#14).
//!
//! Shells out to `git` (assumed present at runtime) to find the enclosing
//! repository and its linked worktrees, scans each for markdown, and validates
//! client-supplied paths against the discovered roots to prevent directory
//! traversal. Outside a git repository it gracefully degrades to scanning the
//! start directory.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

/// A markdown file within a group, identified to the client by its absolute
/// path and shown by its path relative to the group root.
#[derive(Debug, Clone, Serialize)]
pub struct FileEntry {
    pub path: String,
    pub rel: String,
}

/// One worktree's markdown files.
#[derive(Debug, Clone, Serialize)]
pub struct Group {
    /// Directory name of the worktree root (used as the worktree-view label).
    pub name: String,
    /// Checked-out branch, when known (used as the branch-view label).
    pub branch: Option<String>,
    pub root: String,
    pub files: Vec<FileEntry>,
}

/// The full discovered tree served at `/api/tree`.
#[derive(Debug, Clone, Serialize)]
pub struct Tree {
    pub groups: Vec<Group>,
    /// True when discovery came from git; false for the CWD fallback.
    pub git: bool,
}

struct Worktree {
    root: PathBuf,
    branch: Option<String>,
}

fn markdown_ext(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("md") | Some("markdown")
    )
}

/// The git repository root containing `start`, if any.
fn git_toplevel(start: &Path) -> Option<PathBuf> {
    let out = Command::new("git")
        .arg("-C")
        .arg(start)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let path = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim().to_string());
    (!path.as_os_str().is_empty()).then_some(path)
}

/// Parse `git worktree list --porcelain` into the main + linked worktrees.
fn worktrees(toplevel: &Path) -> Vec<Worktree> {
    let out = match Command::new("git")
        .arg("-C")
        .arg(toplevel)
        .args(["worktree", "list", "--porcelain"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    let text = String::from_utf8_lossy(&out.stdout);

    let mut result = Vec::new();
    let mut cur_path: Option<PathBuf> = None;
    let mut cur_branch: Option<String> = None;
    let mut flush = |path: &mut Option<PathBuf>, branch: &mut Option<String>| {
        if let Some(root) = path.take() {
            result.push(Worktree {
                root,
                branch: branch.take(),
            });
        }
    };
    for line in text.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            flush(&mut cur_path, &mut cur_branch);
            cur_path = Some(PathBuf::from(p));
        } else if let Some(b) = line.strip_prefix("branch ") {
            cur_branch = Some(b.trim_start_matches("refs/heads/").to_string());
        } else if line.is_empty() {
            flush(&mut cur_path, &mut cur_branch);
        }
    }
    flush(&mut cur_path, &mut cur_branch);
    result
}

/// Scan `root` for markdown files, skipping `.git` and any nested directory that
/// is itself another worktree root (so a worktree's files aren't double-listed
/// under an enclosing root).
fn scan_markdown(root: &Path, exclude: &[PathBuf]) -> Vec<PathBuf> {
    WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            if e.file_name() == ".git" {
                return false;
            }
            if e.file_type().is_dir() && exclude.iter().any(|x| x == e.path()) {
                return false;
            }
            true
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && markdown_ext(e.path()))
        .map(|e| e.into_path())
        .collect()
}

fn to_entries(files: Vec<PathBuf>, root: &Path) -> Vec<FileEntry> {
    let mut entries: Vec<FileEntry> = files
        .into_iter()
        .map(|p| {
            let rel = p
                .strip_prefix(root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            FileEntry {
                path: p.to_string_lossy().to_string(),
                rel,
            }
        })
        .collect();
    entries.sort_by(|a, b| a.rel.cmp(&b.rel));
    entries
}

/// Build the markdown tree for the explorer sidebar, discovering from `start`.
pub fn build_tree(start: &Path) -> Tree {
    match git_toplevel(start) {
        Some(top) => {
            let wts = worktrees(&top);
            let roots: Vec<PathBuf> = wts.iter().map(|w| w.root.clone()).collect();
            let groups = wts
                .iter()
                .map(|w| {
                    let exclude: Vec<PathBuf> =
                        roots.iter().filter(|r| **r != w.root).cloned().collect();
                    let files = to_entries(scan_markdown(&w.root, &exclude), &w.root);
                    let name = w
                        .root
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| w.root.to_string_lossy().to_string());
                    Group {
                        name,
                        branch: w.branch.clone(),
                        root: w.root.to_string_lossy().to_string(),
                        files,
                    }
                })
                .collect();
            Tree { groups, git: true }
        }
        None => {
            let files = to_entries(scan_markdown(start, &[]), start);
            Tree {
                groups: vec![Group {
                    name: start
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "files".to_string()),
                    branch: None,
                    root: start.to_string_lossy().to_string(),
                    files,
                }],
                git: false,
            }
        }
    }
}

/// Canonical roots a client-selected file must live under. Falls back to the
/// start dir when git discovery yields nothing.
pub fn allowed_roots(start: &Path) -> Vec<PathBuf> {
    let raw = match git_toplevel(start) {
        Some(top) => {
            let mut wts: Vec<PathBuf> = worktrees(&top).into_iter().map(|w| w.root).collect();
            if wts.is_empty() {
                wts.push(top);
            }
            wts
        }
        None => vec![start.to_path_buf()],
    };
    raw.iter().filter_map(|r| r.canonicalize().ok()).collect()
}

/// Resolve a client-supplied path: it must canonicalize to an existing markdown
/// file located under one of `roots`. Returns `None` (reject) otherwise.
pub fn resolve_within(roots: &[PathBuf], requested: &str) -> Option<PathBuf> {
    let canon = Path::new(requested).canonicalize().ok()?;
    if !canon.is_file() || !markdown_ext(&canon) {
        return None;
    }
    roots.iter().any(|r| canon.starts_with(r)).then_some(canon)
}

/// Pick an initial active file: the requested one if it is a readable file,
/// otherwise the first markdown discovered from `start`.
pub fn initial_active(requested: &Path, start: &Path) -> Option<PathBuf> {
    if requested.is_file() {
        return Some(
            requested
                .canonicalize()
                .unwrap_or_else(|_| requested.to_path_buf()),
        );
    }
    build_tree(start)
        .groups
        .into_iter()
        .flat_map(|g| g.files.into_iter())
        .next()
        .map(|f| PathBuf::from(f.path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_ext_matches_md_and_markdown() {
        assert!(markdown_ext(Path::new("a.md")));
        assert!(markdown_ext(Path::new("dir/b.markdown")));
        assert!(!markdown_ext(Path::new("c.txt")));
        assert!(!markdown_ext(Path::new("noext")));
    }

    #[test]
    fn resolve_within_rejects_outside_and_non_markdown() {
        let dir = std::env::temp_dir()
            .canonicalize()
            .expect("temp dir canonicalizes");
        let md = dir.join(format!("mdpeek_explorer_test_{}.md", std::process::id()));
        std::fs::write(&md, "# hi").unwrap();
        let roots = vec![dir.clone()];

        // Inside a root + markdown -> accepted.
        let got = resolve_within(&roots, md.to_str().unwrap());
        assert_eq!(got.as_deref(), Some(md.canonicalize().unwrap().as_path()));

        // A path outside every root is rejected.
        let outside = vec![md.join("nonexistent-root")];
        assert!(resolve_within(&outside, md.to_str().unwrap()).is_none());

        // A non-markdown file is rejected.
        let txt = dir.join(format!("mdpeek_explorer_test_{}.txt", std::process::id()));
        std::fs::write(&txt, "x").unwrap();
        assert!(resolve_within(&roots, txt.to_str().unwrap()).is_none());

        let _ = std::fs::remove_file(&md);
        let _ = std::fs::remove_file(&txt);
    }

    #[test]
    fn build_tree_finds_this_repos_markdown() {
        // Running inside the mdpeek git repo, discovery should be git-backed and
        // surface at least one markdown file (this crate has plenty).
        let here = std::env::current_dir().unwrap();
        let tree = build_tree(&here);
        let total: usize = tree.groups.iter().map(|g| g.files.len()).sum();
        assert!(total > 0, "expected to discover markdown files");
    }
}
