//! Semantic model — the analyser's intermediate representation (AGENTS.md §4.2).
//!
//! This is the rules-stage output that later layers (planner → generator → IR)
//! build on. It deliberately avoids the UI IR types (Layer 3): Layer 2 only
//! produces a `DocumentModel` and the side-panel view over it. Block ids and
//! source ranges are re-exported from Layer 1's `mdpeek-parser`.

use crate::links::Link;
use mdpeek_parser::{BlockId, SourceRange};
use serde::Serialize;

/// Where a piece of information came from. Rules today; `Llm` reserved for
/// Layer 3's `ClaudeGenerator`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    #[default]
    Rules,
    Llm,
}

/// A value plus how confident we are in it and who produced it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Classified<T> {
    pub value: T,
    pub confidence: f32,
    pub by: Origin,
}

impl<T> Classified<T> {
    pub fn rules(value: T, confidence: f32) -> Self {
        Classified {
            value,
            confidence,
            by: Origin::Rules,
        }
    }
}

/// Document type. Development docs first, then the non-dev domains of §9.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentType {
    // 開発文書
    DesignDoc,
    Readme,
    Adr,
    Minutes,
    Runbook,
    Investigation,
    Changelog,
    GitLog,
    // 非開発ドメイン (§9.2)
    Novel,
    ProductionOrder,
    Procedure,
    Recipe,
    Contract,
    Paper,
    Faq,
    Generic,
}

/// Semantic role of a block, derived by rules from its section heading / kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockClass {
    Overview,
    Architecture,
    DataModel,
    Risk,
    OpenQuestion,
    Decision,
    Consequence,
    Step,
    Usage,
    Configuration,
    Troubleshooting,
    Task,
    CodeExample,
    Table,
    Heading,
    Generic,
}

/// A block tagged with its semantic class.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClassifiedBlock {
    pub block_id: BlockId,
    pub class: BlockClass,
    pub confidence: f32,
    pub range: SourceRange,
}

/// One heading in the document outline.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OutlineEntry {
    pub level: u8,
    pub title: String,
    pub block_id: BlockId,
    pub range: SourceRange,
}

/// A task-list item extracted from the document.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Task {
    pub text: String,
    pub checked: bool,
    pub block_id: BlockId,
    pub range: SourceRange,
}

/// The full semantic model produced by the rules analyser.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DocumentModel {
    pub doc_type: Classified<DocumentType>,
    pub blocks: Vec<ClassifiedBlock>,
    pub frontmatter: Option<String>,
    pub outline: Vec<OutlineEntry>,
    pub links: Vec<Link>,
    pub tasks: Vec<Task>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classified_rules_constructor_sets_origin() {
        let c = Classified::rules(DocumentType::Readme, 0.9);
        assert_eq!(c.value, DocumentType::Readme);
        assert_eq!(c.by, Origin::Rules);
        assert!((c.confidence - 0.9).abs() < f32::EPSILON);
    }
}
