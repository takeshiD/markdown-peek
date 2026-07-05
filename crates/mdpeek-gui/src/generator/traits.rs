//! Generator trait + input contract (design doc §3.4).
//!
//! In the full design a `Generator` consumes a `UiPlan` + `DocumentModel`
//! produced by Layer 2 (`analyzer`/`planner`). Those modules are being built in
//! separate worktrees, so Layer 3 defines a **lightweight input** ([`GenInput`])
//! here: raw markdown + a document-type hint. When Layer 2 lands, `GenInput`
//! becomes a thin adapter over `DocumentModel` — no renderer/IR changes needed,
//! because the contract below (`-> Vec<UiNode>`) is what the rest of Layer 3
//! depends on.

use anyhow::Result;

use crate::ir::UiNode;

/// Coarse document-type hint. A stand-in for Layer 2's `DocumentType`
/// classification; `RulesGenerator` works for any value (falls back to generic
/// structural extraction), so callers may pass [`DocType::Generic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)] // non-Generic variants are consumed once Layer 2 doctype classification lands
pub enum DocType {
    #[default]
    Generic,
    Readme,
    DesignDoc,
    Runbook,
    Changelog,
    Recipe,
}

/// Input to a [`Generator`]. Deliberately minimal; see module docs.
pub struct GenInput<'a> {
    pub markdown: &'a str,
    pub doc_type: DocType,
}

impl<'a> GenInput<'a> {
    pub fn new(markdown: &'a str) -> Self {
        GenInput {
            markdown,
            doc_type: DocType::Generic,
        }
    }

    #[allow(dead_code)]
    pub fn with_doc_type(mut self, doc_type: DocType) -> Self {
        self.doc_type = doc_type;
        self
    }
}

/// Produces UI IR from a document. Rules implementation is the offline default;
/// the LLM implementation (`feature = "llm"`) only fills nodes rules can't.
///
/// Output is *unvalidated*; callers must run [`crate::ir::validate_nodes`]
/// before caching or rendering.
pub trait Generator {
    fn generate(&self, input: &GenInput<'_>) -> Result<Vec<UiNode>>;

    /// Short identifier recorded in the cache key (`"rules"`, `"claude-…"`).
    fn model_id(&self) -> String;
}
