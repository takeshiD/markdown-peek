//! UI IR validation (design doc §3.5): the security boundary that every node —
//! whether from `RulesGenerator` or a future `ClaudeGenerator` — must pass
//! before it can be cached or sent to a renderer.
//!
//! Three checks:
//! 1. **Schema** — enforced upstream by serde deserialization (unknown `kind`
//!    / wrong shape fails to parse). [`validate_json`] re-runs it explicitly.
//! 2. **Registry allowlist** — reject any `kind` not in [`super::registry`].
//! 3. **sourceRange bounds** — every range must fall inside the document
//!    (`total_lines`); fabricated ranges are how hallucinations are caught.
//!
//! Nodes whose `confidence` is below [`CONFIDENCE_THRESHOLD`] are *not* rejected
//! but flagged (`low_confidence = true`) so the renderer can badge them.

use super::node::UiNode;
use super::range::SourceRange;
use super::registry;

/// Below this confidence a node is passed through but flagged for the UI.
pub const CONFIDENCE_THRESHOLD: f32 = 0.5;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ValidateError {
    #[error("unknown component kind `{0}` is not in the registry allowlist")]
    UnknownKind(String),
    #[error("sourceRange {0:?} is outside the document (1..={1} lines)")]
    RangeOutOfBounds(SourceRange, u32),
    #[error("invalid IR JSON: {0}")]
    Schema(String),
}

/// Validate a slice of nodes in place: registry allowlist + range bounds, and
/// set the `low_confidence` flag. `total_lines` is the document length used for
/// bounds checking (obtain from `LineIndex::line_count`).
pub fn validate_nodes(nodes: &mut [UiNode], total_lines: u32) -> Result<(), ValidateError> {
    for node in nodes.iter_mut() {
        validate_node(node, total_lines)?;
    }
    Ok(())
}

fn validate_node(node: &mut UiNode, total_lines: u32) -> Result<(), ValidateError> {
    // (2) registry allowlist — belt-and-braces with serde's tag parsing.
    if !registry::is_allowed(node.kind()) {
        return Err(ValidateError::UnknownKind(node.kind().to_string()));
    }

    // (3) sourceRange bounds on the node's own meta.
    check_range(node.meta().source_range, total_lines)?;

    // Recurse into any ranges nested inside node payloads and into Tabs children.
    for r in nested_ranges(node) {
        check_range(Some(r), total_lines)?;
    }
    if let UiNode::Tabs(tabs) = node {
        for tab in tabs.tabs.iter_mut() {
            validate_nodes(&mut tab.children, total_lines)?;
        }
    }

    // confidence flagging (design §3.5): flag, don't reject.
    let low = node
        .meta()
        .confidence
        .map(|c| c < CONFIDENCE_THRESHOLD)
        .unwrap_or(false);
    node.meta_mut().low_confidence = low;

    Ok(())
}

fn check_range(range: Option<SourceRange>, total_lines: u32) -> Result<(), ValidateError> {
    if let Some(r) = range
        && !r.within(total_lines)
    {
        return Err(ValidateError::RangeOutOfBounds(r, total_lines));
    }
    Ok(())
}

/// Collect ranges embedded inside node payloads (list items etc.) for bounds
/// checking. Returned by value since they are `Copy`.
fn nested_ranges(node: &UiNode) -> Vec<SourceRange> {
    match node {
        UiNode::Checklist(n) => n.items.iter().filter_map(|i| i.source_range).collect(),
        UiNode::Timeline(n) => n.events.iter().filter_map(|e| e.source_range).collect(),
        UiNode::RiskPanel(n) => n.risks.iter().filter_map(|r| r.source_range).collect(),
        UiNode::Glossary(n) => n.terms.iter().filter_map(|t| t.source_range).collect(),
        UiNode::StepNavigator(n) => n.steps.iter().filter_map(|s| s.source_range).collect(),
        UiNode::CharacterRoster(n) => n.characters.iter().filter_map(|c| c.first_seen).collect(),
        UiNode::ObligationMatrix(n) => {
            n.obligations.iter().filter_map(|o| o.source_range).collect()
        }
        _ => Vec::new(),
    }
}

/// Parse untrusted JSON (e.g. LLM output) into validated nodes. Combines schema
/// (serde) + allowlist + bounds. Entry point for the LLM backends.
pub fn validate_json(json: &str, total_lines: u32) -> Result<Vec<UiNode>, ValidateError> {
    let mut nodes: Vec<UiNode> =
        serde_json::from_str(json).map_err(|e| ValidateError::Schema(e.to_string()))?;
    validate_nodes(&mut nodes, total_lines)?;
    Ok(nodes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::node::*;

    fn meta_with_range(r: Option<SourceRange>) -> NodeMeta {
        NodeMeta {
            source_range: r,
            ..Default::default()
        }
    }

    #[test]
    fn rejects_out_of_bounds_range() {
        let mut nodes = vec![UiNode::Callout(CalloutNode {
            meta: meta_with_range(Some(SourceRange {
                start_line: 1,
                start_column: 1,
                end_line: 99,
                end_column: 1,
            })),
            severity: Severity::Warning,
            title: None,
            body: "x".into(),
        })];
        let err = validate_nodes(&mut nodes, 10).unwrap_err();
        assert!(matches!(err, ValidateError::RangeOutOfBounds(_, 10)));
    }

    #[test]
    fn rejects_unknown_kind_json() {
        let json = r#"[{"kind":"EvilScript","code":"alert(1)"}]"#;
        let err = validate_json(json, 100).unwrap_err();
        // serde fails first because the tag is not a known variant.
        assert!(matches!(err, ValidateError::Schema(_)));
    }

    #[test]
    fn flags_low_confidence() {
        let mut nodes = vec![UiNode::Callout(CalloutNode {
            meta: NodeMeta {
                confidence: Some(0.2),
                origin: Origin::Llm,
                ..Default::default()
            },
            severity: Severity::Info,
            title: None,
            body: "maybe".into(),
        })];
        validate_nodes(&mut nodes, 10).unwrap();
        assert!(nodes[0].meta().low_confidence);
    }

    #[test]
    fn roundtrips_valid_json() {
        let json = r#"[{"kind":"Checklist","items":[{"title":"do it","checked":false}]}]"#;
        let nodes = validate_json(json, 100).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].kind(), "Checklist");
    }
}
