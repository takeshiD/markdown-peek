//! Layer 2 — Semantic viewer (rules-centric).
//!
//! Turns a parsed [`mdpeek_parser::BlockTree`] into a [`model::DocumentModel`]
//! and a [`panel::SemanticPanel`] using deterministic rules only (no LLM). This
//! is the foundation later layers build on:
//!
//! ```text
//! markdown ─▶ mdpeek_parser::BlockTree ─▶ analyzer(rules) ─▶ DocumentModel ─▶ SemanticPanel
//! ```
//!
//! See AGENTS.md §10 "Layer 2 — Semantic viewer". Nothing here is UI IR
//! (Layer 3). The public entry point is [`analyze`], returning an [`Analysis`]
//! that bundles the tree, model and side panel.
//!
//! Block parsing and `SourceRange`s come from Layer 1's `mdpeek-parser`
//! (`BlockTree`); this crate adds the semantic layer on top.

pub mod analyzer;
pub mod generation;
pub mod links;
pub mod model;
pub mod panel;

pub use generation::{GenerationConfig, GenerationStrategy};
pub use mdpeek_parser::{Block, BlockId, BlockKind, BlockTree, SourceRange};

use self::model::DocumentModel;
use self::panel::SemanticPanel;

/// The complete Layer 2 analysis of a document.
#[derive(Debug, Clone)]
pub struct Analysis {
    pub tree: BlockTree,
    pub model: DocumentModel,
    pub panel: SemanticPanel,
}

/// Analyse a markdown document end-to-end (parse → rules model → side panel).
///
/// `filename` (when known) sharpens document-type inference; pass `None` if
/// analysing an in-memory buffer.
pub fn analyze(markdown: &str, filename: Option<&str>) -> Analysis {
    let tree = BlockTree::parse(markdown);
    let model = analyzer::build_model(markdown, &tree, filename);
    let panel = panel::build(&model, &tree);
    Analysis { tree, model, panel }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DocumentType;

    #[test]
    fn end_to_end_readme_analysis() {
        let md = "---\ntype: readme\n---\n\
                  # Widget\n\n\
                  A small widget.\n\n\
                  ## Usage\n\n\
                  ```bash\nwidget --help\n```\n\n\
                  ## Risks\n\n\
                  It may overheat.\n\n\
                  ## TODO\n\n\
                  - [ ] add tests\n- [x] write docs\n";
        let a = analyze(md, Some("README.md"));

        assert_eq!(a.model.doc_type.value, DocumentType::Readme);
        assert_eq!(a.panel.outline.len(), 4);
        assert_eq!(a.panel.todos.len(), 2);
        assert!(a.panel.risks.iter().any(|e| e.text.contains("overheat")));
        // Source ranges are populated and 1-based.
        assert!(a.panel.outline.iter().all(|o| o.link.range.start_line >= 1));
    }

    #[test]
    fn analysis_is_deterministic() {
        let md = "# A\n\n## B\n\ncontent\n";
        let a = analyze(md, None);
        let b = analyze(md, None);
        assert_eq!(a.model, b.model);
        assert_eq!(a.panel, b.panel);
    }
}
