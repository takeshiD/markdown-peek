//! Markdown discovery for the explorer sidebar (#14).
//!
//! Shells out to `git` (assumed present at runtime) to find the enclosing
//! working tree — the current branch's checkout only; other worktrees are not
//! listed — scans it for markdown, and validates client-supplied paths against
//! that root to prevent directory traversal. Outside a git repository it
//! gracefully degrades to scanning the start directory.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A markdown file within a group, identified to the client by its absolute
/// path and shown by its path relative to the group root.
#[derive(Debug, Clone, Serialize)]
pub struct FileEntry {
    pub path: String,
    pub rel: String,
}

/// The current working tree's markdown files.
#[derive(Debug, Clone, Serialize)]
pub struct Group {
    /// Directory name of the working-tree root.
    pub name: String,
    /// Checked-out branch, when known (shown in the breadcrumb).
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

/// The branch checked out in `toplevel` (`None` when detached or unknown).
fn current_branch(toplevel: &Path) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(toplevel)
        .args(["symbolic-ref", "--quiet", "--short", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let b = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!b.is_empty()).then_some(b)
}

/// Linked worktrees nested inside `toplevel` (e.g. `.worktrees/x`), so their
/// files are not listed as if they belonged to the current branch.
fn nested_worktrees(toplevel: &Path) -> Vec<PathBuf> {
    let out = match Command::new("git")
        .arg("-C")
        .arg(toplevel)
        .args(["worktree", "list", "--porcelain"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.strip_prefix("worktree "))
        .map(PathBuf::from)
        .filter(|p| p != toplevel && p.starts_with(toplevel))
        .collect()
}

/// Scan `root` for markdown files, honouring `.gitignore` / git excludes (so
/// `node_modules`, build output etc. never reach the explorer or the palette),
/// skipping `.git` and any nested directory that is itself another worktree
/// root (another branch's checkout).
/// Dot-directories such as `.github` are kept.
fn scan_markdown(root: &Path, exclude: &[PathBuf]) -> Vec<PathBuf> {
    let exclude: Vec<PathBuf> = exclude.to_vec();
    let mut out: Vec<PathBuf> = ignore::WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .git_global(true)
        .parents(true)
        .filter_entry(move |e| {
            if e.file_name() == ".git" {
                return false;
            }
            let is_dir = e.file_type().is_some_and(|t| t.is_dir());
            !(is_dir && exclude.iter().any(|x| x == e.path()))
        })
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()) && markdown_ext(e.path()))
        .map(|e| e.into_path())
        .collect();
    out.sort();
    out
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

fn dir_name(p: &Path, fallback: &str) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| fallback.to_string())
}

/// Build the markdown tree for the explorer sidebar, discovering from `start`:
/// the current working tree (one group), never other worktrees.
pub fn build_tree(start: &Path) -> Tree {
    match git_toplevel(start) {
        Some(top) => {
            let files = to_entries(scan_markdown(&top, &nested_worktrees(&top)), &top);
            Tree {
                groups: vec![Group {
                    name: dir_name(&top, "repository"),
                    branch: current_branch(&top),
                    root: top.to_string_lossy().to_string(),
                    files,
                }],
                git: true,
            }
        }
        None => {
            let files = to_entries(scan_markdown(start, &[]), start);
            Tree {
                groups: vec![Group {
                    name: dir_name(start, "files"),
                    branch: None,
                    root: start.to_string_lossy().to_string(),
                    files,
                }],
                git: false,
            }
        }
    }
}

/// Canonical roots a client-selected file must live under: the current working
/// tree (or the start dir outside git).
pub fn allowed_roots(start: &Path) -> Vec<PathBuf> {
    let root = git_toplevel(start).unwrap_or_else(|| start.to_path_buf());
    root.canonicalize().ok().into_iter().collect()
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

    #[test]
    fn build_tree_lists_only_the_current_working_tree() {
        let here = std::env::current_dir().unwrap();
        let tree = build_tree(&here);
        assert_eq!(
            tree.groups.len(),
            1,
            "other worktrees/branches are not listed"
        );
        let top = git_toplevel(&here).unwrap();
        assert_eq!(tree.groups[0].root, top.to_string_lossy());
        assert_eq!(allowed_roots(&here), vec![top.canonicalize().unwrap()]);
    }
}
