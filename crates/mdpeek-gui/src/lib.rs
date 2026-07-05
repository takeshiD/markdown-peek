//! Layer 3 — Generative UI core (design doc §2 `mdpeek-core`).
//!
//! Owns the UI IR, generators, cache and the pipeline facade so that the CLI
//! (`mdpeek gen`) and the server (`/api/gui`) share one implementation:
//!
//! - [`ir`]        — `UiNode` wire format + validation + registry allowlist.
//! - [`generator`] — rules + LLM backends (claude_code / codex / anthropic_api).
//! - [`cache`]     — content-hash keyed `.cache/mdpeek/*.gui.json`.
//!
//! Pipeline (design §1): parse+generate → `validate` (schema + allowlist +
//! sourceRange) → `cache`. LLM generation plugs in at the generator step
//! without changing this facade.

pub mod cache;
pub mod generator;
pub mod ir;
pub mod planner;

use std::path::Path;

use anyhow::{Context, Result};

use crate::cache::{CacheStore, GuiCacheEntry, content_hash};
use crate::generator::llm::LlmBackendConfig;
use crate::generator::GenInput;
use crate::ir::{LineIndex, UiNode, validate_nodes};

/// Generate validated UI IR for `markdown`. `filename` (when known) sharpens
/// Layer 2's document-type inference. Uses the on-disk cache under `cache_root`
/// when provided, and the deterministic [`RulesGenerator`] + [`planner`].
pub fn generate(
    markdown: &str,
    filename: Option<&str>,
    cache_root: Option<&Path>,
) -> Result<GuiCacheEntry> {
    let model_id = "rules";

    // Cache hit? (Key includes the filename since it affects doctype/output.)
    if let Some(root) = cache_root
        && let Some(entry) = CacheStore::new(root).get(markdown, model_id, filename)
    {
        return Ok(entry);
    }

    // Layer 2 semantic analysis → reading lenses (design §8). Body content
    // (tables / code / diagrams) is NOT reprinted here — it stays in the
    // Markdown Body (§7.2). This is the deterministic fallback for LLM-first.
    let analysis = mdpeek_analyzer::analyze(markdown, filename);
    let mut nodes: Vec<UiNode> = planner::plan(&analysis);
    let doc_type = format!("{:?}", analysis.model.doc_type.value);

    // Validate everything (the security boundary).
    let total_lines = LineIndex::new(markdown).line_count();
    validate_nodes(&mut nodes, total_lines).context("generated IR failed validation")?;

    let hash = content_hash(markdown, model_id, filename);
    let entry = GuiCacheEntry::new(doc_type, nodes, model_id.to_string(), hash);

    // Best-effort persist; a cache write failure must not fail the request.
    if let Some(root) = cache_root {
        let _ = CacheStore::new(root).put(&entry);
    }

    Ok(entry)
}

/// Generate validated UI IR using the configured LLM [`backend`], with the
/// deterministic [`RulesGenerator`] as a fallback when the backend fails
/// (missing CLI, no API key, network error, invalid output). Uses the on-disk
/// cache keyed by the backend's model id.
pub fn generate_with_llm(
    markdown: &str,
    filename: Option<&str>,
    cache_root: Option<&Path>,
    backend: &LlmBackendConfig,
) -> Result<GuiCacheEntry> {
    let generator = match backend.build() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("mdpeek: LLM backend unavailable ({e}); using rules");
            return generate(markdown, filename, cache_root);
        }
    };
    let model_id = generator.model_id();

    if let Some(root) = cache_root
        && let Some(entry) = CacheStore::new(root).get(markdown, &model_id, filename)
    {
        return Ok(entry);
    }

    let total_lines = LineIndex::new(markdown).line_count();
    let nodes = match generator.generate(&GenInput::new(markdown)) {
        Ok(mut nodes) => {
            // Backends validate internally; re-run for defence in depth.
            validate_nodes(&mut nodes, total_lines).context("LLM IR failed validation")?;
            nodes
        }
        Err(e) => {
            eprintln!("mdpeek: LLM generation failed ({e}); falling back to rules");
            return generate(markdown, filename, cache_root);
        }
    };

    let doc_type = format!(
        "{:?}",
        mdpeek_analyzer::analyze(markdown, filename)
            .model
            .doc_type
            .value
    );
    let hash = content_hash(markdown, &model_id, filename);
    let entry = GuiCacheEntry::new(doc_type, nodes, model_id, hash);
    if let Some(root) = cache_root {
        let _ = CacheStore::new(root).put(&entry);
    }
    Ok(entry)
}

/// Convenience: pretty-printed UI IR JSON for the `gen` CLI command. When
/// `backend` is `Some`, uses the configured LLM; otherwise rules only.
pub fn generate_json(
    markdown: &str,
    filename: Option<&str>,
    cache_root: Option<&Path>,
    backend: Option<&LlmBackendConfig>,
) -> Result<String> {
    let entry = match backend {
        Some(b) => generate_with_llm(markdown, filename, cache_root, b)?,
        None => generate(markdown, filename, cache_root)?,
    };
    serde_json::to_string_pretty(&entry.ui_ir).context("serializing UI IR")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn end_to_end_generates_and_caches() {
        let tmp = tempfile::tempdir().unwrap();
        let md = "## Tasks\n\n- [ ] a\n- [x] b\n\n> [!WARNING]\n> danger\n";
        let first = generate(md, None, Some(tmp.path())).unwrap();
        assert!(!first.ui_ir.is_empty());
        // Second call must hit the cache (same content_hash written to disk).
        let second = generate(md, None, Some(tmp.path())).unwrap();
        assert_eq!(first.content_hash, second.content_hash);
        assert_eq!(first.ui_ir.len(), second.ui_ir.len());
    }

    #[test]
    fn produces_valid_json_lenses() {
        // Task list → ActionItems lens (not a body reprint).
        let md = "## Tasks\n\n- [ ] do it\n";
        let json = generate_json(md, None, None, None).unwrap();
        assert!(json.contains("ActionItems"), "{json}");
    }

    #[test]
    fn no_body_reprint_nodes() {
        // Tables / code / diagrams must NOT surface as lenses (they stay in the
        // Markdown Body, design §7.2).
        let md = "| a | b |\n|---|---|\n| 1 | 2 |\n\n```json\n{}\n```\n";
        let json = generate_json(md, None, None, None).unwrap();
        assert!(!json.contains("DataTable"), "{json}");
        assert!(!json.contains("ConfigViewer"), "{json}");
    }

    #[test]
    fn design_doc_adds_semantic_nodes() {
        // Overview + Architecture + Risks give the analyser enough signal to
        // classify this as a design doc.
        let md = "# Design\n\n## Overview\n\nWhat.\n\n## Architecture\n\nHow.\n\n\
                  ## Risks\n\nMay overheat.\n";
        let entry = generate(md, Some("DESIGN.md"), None).unwrap();
        let kinds: Vec<&str> = entry.ui_ir.iter().map(|n| n.kind()).collect();
        // Layer 2 risk extraction surfaces as a RiskPanel (a kind rules alone
        // never produced).
        assert!(kinds.contains(&"RiskPanel"), "kinds: {kinds:?}");
        assert!(
            entry.document_type.contains("DesignDoc"),
            "doc_type was {}",
            entry.document_type
        );
    }
}
