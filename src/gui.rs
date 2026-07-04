//! Generative-UI pipeline facade (design doc §1): document → generator →
//! validate → cache → UI IR. This is the single entry point the CLI (`gen`
//! subcommand) and, later, the server (`/api/gui`) call.
//!
//! Flow (design §1 pipeline): parse+generate (`RulesGenerator`) → `validate`
//! (schema + allowlist + sourceRange) → `cache`. LLM generation plugs in at the
//! generator step behind `feature = "llm"` without changing this facade.

use std::path::Path;

use anyhow::{Context, Result};

use crate::cache::{CacheStore, GuiCacheEntry, content_hash};
use crate::generator::{GenInput, Generator, RulesGenerator};
use crate::ir::{LineIndex, UiNode, validate_nodes};

/// Generate validated UI IR for `markdown`, using the on-disk cache under
/// `cache_root` when provided. Uses the deterministic [`RulesGenerator`].
pub fn generate(markdown: &str, cache_root: Option<&Path>) -> Result<GuiCacheEntry> {
    let generator = RulesGenerator;
    let model_id = generator.model_id();

    // Cache hit?
    if let Some(root) = cache_root
        && let Some(entry) = CacheStore::new(root).get(markdown, &model_id)
    {
        return Ok(entry);
    }

    // Generate → validate (the security boundary).
    let mut nodes: Vec<UiNode> = generator
        .generate(&GenInput::new(markdown))
        .context("rules generation failed")?;
    let total_lines = LineIndex::new(markdown).line_count();
    validate_nodes(&mut nodes, total_lines).context("generated IR failed validation")?;

    let hash = content_hash(markdown, &model_id);
    let entry = GuiCacheEntry::new("generic".to_string(), nodes, model_id, hash);

    // Best-effort persist; a cache write failure must not fail the request.
    if let Some(root) = cache_root {
        let _ = CacheStore::new(root).put(&entry);
    }

    Ok(entry)
}

/// Convenience: pretty-printed UI IR JSON for the `gen` CLI command.
pub fn generate_json(markdown: &str, cache_root: Option<&Path>) -> Result<String> {
    let entry = generate(markdown, cache_root)?;
    serde_json::to_string_pretty(&entry.ui_ir).context("serializing UI IR")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn end_to_end_generates_and_caches() {
        let tmp = tempfile::tempdir().unwrap();
        let md = "## Tasks\n\n- [ ] a\n- [x] b\n\n> [!WARNING]\n> danger\n";
        let first = generate(md, Some(tmp.path())).unwrap();
        assert!(!first.ui_ir.is_empty());
        // Second call must hit the cache (same content_hash written to disk).
        let second = generate(md, Some(tmp.path())).unwrap();
        assert_eq!(first.content_hash, second.content_hash);
        assert_eq!(first.ui_ir.len(), second.ui_ir.len());
    }

    #[test]
    fn produces_valid_json() {
        let md = "| a | b |\n|---|---|\n| 1 | 2 |\n";
        let json = generate_json(md, None).unwrap();
        assert!(json.contains("DataTable"));
    }
}
