//! Layer 4 — Repository-aware viewer.
//!
//! Builds on the worktree-scan foundation (#14) to relate a markdown document
//! to the repository around it: README ↔ actual files, docs-code consistency,
//! `Cargo.toml`/`package.json` awareness, ADR ↔ git history, and TODO ↔ issue
//! correlation.
//!
//! This layer is intentionally self-contained: it consumes only the raw
//! document text and the filesystem/git, so it does not depend on the
//! Layer 1–3 parser/IR pipeline being merged.

mod git;
mod manifest;
mod refs;
mod report;
mod scan;

pub use git::AdrInfo;
pub use manifest::Manifest;
pub use refs::DocRefReport;
pub use scan::WorktreeInfo;
pub use todo::TodoReport;

mod todo;

use anyhow::{Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// The complete repository-aware view of a document.
#[derive(Debug, Serialize)]
pub struct RepoReport {
    /// Repository root, or the document's directory when it is not in a repo.
    pub root: PathBuf,
    /// Document analysed, relative to `root`.
    pub document: String,
    pub in_git_repo: bool,
    pub worktrees: Vec<WorktreeInfo>,
    pub manifests: Vec<Manifest>,
    pub doc_refs: DocRefReport,
    pub adrs: Vec<AdrInfo>,
    pub todos: TodoReport,
}

/// Analyse the repository around `doc` and return the report.
pub fn analyze(doc: &Path) -> Result<RepoReport> {
    let content = std::fs::read_to_string(doc)
        .with_context(|| format!("failed to read '{}'", doc.display()))?;
    let doc_abs = doc
        .canonicalize()
        .with_context(|| format!("failed to resolve '{}'", doc.display()))?;
    let doc_dir = doc_abs.parent().unwrap_or(&doc_abs).to_path_buf();

    let repo_root = scan::find_repo_root(&doc_abs);
    let in_git_repo = repo_root.is_some() && git::git_available();
    let root = repo_root.clone().unwrap_or_else(|| doc_dir.clone());

    // Worktrees + file walk (the #14 foundation).
    let worktrees = if in_git_repo {
        scan::list_worktrees(&root, &doc_abs)
    } else {
        Vec::new()
    };
    let files = scan::walk_files(&root);

    // Manifests.
    let manifests = manifest::discover(&root);

    // README ↔ files / docs-code consistency for this document.
    let doc_refs = refs::analyze_doc(&content, &doc_dir, &root);

    // ADR ↔ git history.
    let mut adrs = Vec::new();
    for file in &files {
        let rel = file.strip_prefix(&root).unwrap_or(file);
        if !git::is_adr_path(rel) {
            continue;
        }
        let title = std::fs::read_to_string(file)
            .ok()
            .and_then(|c| git::first_heading(&c));
        let last_commit = if in_git_repo {
            git::last_commit(&root, file)
        } else {
            None
        };
        adrs.push(AdrInfo {
            path: rel.to_string_lossy().replace('\\', "/"),
            title,
            last_commit,
        });
    }
    adrs.sort_by(|a, b| a.path.cmp(&b.path));

    // TODO ↔ issue.
    let todos = todo::scan(&root, &files);

    let document = doc_abs
        .strip_prefix(&root)
        .unwrap_or(&doc_abs)
        .to_string_lossy()
        .replace('\\', "/");

    Ok(RepoReport {
        root,
        document,
        in_git_repo,
        worktrees,
        manifests,
        doc_refs,
        adrs,
        todos,
    })
}

impl RepoReport {
    /// Render a human-readable, coloured terminal report.
    pub fn to_terminal(&self, color: bool) -> String {
        report::render_terminal(self, color)
    }

    /// Serialize the report as pretty JSON.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).context("failed to serialize report")
    }
}
