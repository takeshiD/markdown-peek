//! Planner — document-type-aware UI planning over Layer 2's analysis
//! (design doc §3.3 / §9).
//!
//! Where [`crate::generator::rules`] does *structural* extraction (task lists →
//! Checklist, tables → DataTable, …) straight from the token stream, the planner
//! consumes [`mdpeek_analyzer::Analysis`] — the Layer 2 `DocumentModel` +
//! `SemanticPanel` — and decides which *semantic* nodes to emit and in what
//! shape, based on the inferred document type:
//!
//! - risks (`## Risk` sections) → [`RiskPanel`](crate::ir::node::RiskPanelNode)
//! - open questions            → a `Checklist` of unresolved items
//! - DesignDoc / Readme → a "review" `Checklist` of expected vs. missing
//!   sections (design §9.1 "missing section 検出")
//! - Adr / Changelog / Minutes → a `Timeline` from the section/version outline
//!
//! These complement (don't replace) the structural nodes; the pipeline
//! concatenates both, then validates.

use mdpeek_analyzer::Analysis;
use mdpeek_analyzer::model::DocumentType;

use crate::ir::SourceRange;
use crate::ir::node::*;

/// Convert Layer 1/2's `SourceRange` into the IR's (identical shape).
fn conv(r: mdpeek_analyzer::SourceRange) -> SourceRange {
    SourceRange {
        start_line: r.start_line,
        start_column: r.start_column,
        end_line: r.end_line,
        end_column: r.end_column,
    }
}

fn rules_meta(range: Option<SourceRange>, confidence: f32) -> NodeMeta {
    NodeMeta {
        source_range: range,
        confidence: Some(confidence),
        origin: Origin::Rules,
        ..Default::default()
    }
}

/// Produce document-type-aware semantic nodes from a Layer 2 analysis.
pub fn plan(analysis: &Analysis) -> Vec<UiNode> {
    let mut out = Vec::new();

    // Common semantic panels, emitted whenever the analyser found the content.
    if let Some(node) = risk_panel(analysis) {
        out.push(node);
    }
    if let Some(node) = open_questions(analysis) {
        out.push(node);
    }

    // Document-type-specific additions.
    match analysis.model.doc_type.value {
        DocumentType::DesignDoc => {
            if let Some(node) = review_checklist(analysis, DESIGN_SECTIONS) {
                out.push(node);
            }
        }
        DocumentType::Readme => {
            if let Some(node) = review_checklist(analysis, README_SECTIONS) {
                out.push(node);
            }
        }
        DocumentType::Adr | DocumentType::Changelog | DocumentType::Minutes => {
            if let Some(node) = outline_timeline(analysis) {
                out.push(node);
            }
        }
        _ => {}
    }

    out
}

/// `## Risk` sections → a RiskPanel (severity defaults to warning; the analyser
/// doesn't rank severity yet).
fn risk_panel(analysis: &Analysis) -> Option<UiNode> {
    let risks = &analysis.panel.risks;
    if risks.is_empty() {
        return None;
    }
    let conf = analysis.model.doc_type.confidence;
    let items = risks
        .iter()
        .map(|e| RiskItem {
            title: e.text.clone(),
            severity: Severity::Warning,
            note: None,
            source_range: Some(conv(e.link.range)),
        })
        .collect();
    Some(UiNode::RiskPanel(RiskPanelNode {
        meta: rules_meta(None, conf),
        risks: items,
    }))
}

/// Open questions → an unchecked Checklist (jump links preserved).
fn open_questions(analysis: &Analysis) -> Option<UiNode> {
    let qs = &analysis.panel.open_questions;
    if qs.is_empty() {
        return None;
    }
    let conf = analysis.model.doc_type.confidence;
    let items = qs
        .iter()
        .map(|e| ChecklistItem {
            title: e.text.clone(),
            checked: false,
            category: Some("Open question".to_string()),
            source_range: Some(conv(e.link.range)),
        })
        .collect();
    Some(UiNode::Checklist(ChecklistNode {
        meta: rules_meta(None, conf),
        items,
    }))
}

/// Expected sections per document type: `(label, lowercase keyword)`. A section
/// is "present" when any outline heading contains the keyword.
type Sections = &'static [(&'static str, &'static str)];

const DESIGN_SECTIONS: Sections = &[
    ("Overview", "overview"),
    ("Architecture", "architecture"),
    ("Data model", "model"),
    ("Risks", "risk"),
    ("Open questions", "open question"),
];

const README_SECTIONS: Sections = &[
    ("Installation", "install"),
    ("Usage", "usage"),
    ("Configuration", "config"),
    ("License", "licen"),
];

/// A review checklist: each expected section is an item, checked when present in
/// the outline, unchecked (i.e. missing) otherwise. Design §9.1 "review
/// checklist / missing section 検出".
fn review_checklist(analysis: &Analysis, sections: Sections) -> Option<UiNode> {
    let titles: Vec<String> = analysis
        .model
        .outline
        .iter()
        .map(|e| e.title.to_lowercase())
        .collect();

    let items: Vec<ChecklistItem> = sections
        .iter()
        .map(|(label, keyword)| {
            // Source range of the matching heading, if present.
            let range = analysis
                .model
                .outline
                .iter()
                .find(|e| e.title.to_lowercase().contains(keyword))
                .map(|e| conv(e.range));
            ChecklistItem {
                title: label.to_string(),
                checked: titles.iter().any(|t| t.contains(keyword)),
                category: Some("Section".to_string()),
                source_range: range,
            }
        })
        .collect();

    // Only worth showing if the document has any structure to review against.
    if analysis.model.outline.is_empty() {
        return None;
    }
    Some(UiNode::Checklist(ChecklistNode {
        meta: rules_meta(None, analysis.model.doc_type.confidence),
        items,
    }))
}

/// Section/version outline → a Timeline (top-level headings, in document order).
fn outline_timeline(analysis: &Analysis) -> Option<UiNode> {
    let events: Vec<TimelineEvent> = analysis
        .model
        .outline
        .iter()
        .filter(|e| e.level <= 2)
        .map(|e| TimelineEvent {
            title: e.title.clone(),
            timestamp: None,
            description: None,
            source_range: Some(conv(e.range)),
        })
        .collect();
    if events.len() < 2 {
        return None;
    }
    Some(UiNode::Timeline(TimelineNode {
        meta: rules_meta(None, analysis.model.doc_type.confidence),
        events,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan_md(md: &str, filename: Option<&str>) -> Vec<UiNode> {
        plan(&mdpeek_analyzer::analyze(md, filename))
    }

    #[test]
    fn design_doc_emits_risk_panel_and_review_checklist() {
        let md = "# Design\n\n## Overview\n\nx\n\n## Architecture\n\ny\n\n\
                  ## Risks\n\nThe cache may go stale.\n";
        let nodes = plan_md(md, Some("DESIGN.md"));
        assert!(
            nodes.iter().any(|n| matches!(n, UiNode::RiskPanel(_))),
            "expected a RiskPanel, got {:?}",
            nodes.iter().map(|n| n.kind()).collect::<Vec<_>>()
        );
        // Review checklist: Open questions section is missing → an unchecked item.
        let cl = nodes.iter().find_map(|n| match n {
            UiNode::Checklist(c) if c.items.iter().any(|i| i.category.as_deref() == Some("Section")) => Some(c),
            _ => None,
        });
        let cl = cl.expect("review checklist");
        let oq = cl.items.iter().find(|i| i.title == "Open questions").unwrap();
        assert!(!oq.checked, "Open questions section should be flagged missing");
        let overview = cl.items.iter().find(|i| i.title == "Overview").unwrap();
        assert!(overview.checked);
    }

    #[test]
    fn generic_prose_plans_nothing() {
        let nodes = plan_md("Just a paragraph.\n", None);
        assert!(nodes.is_empty());
    }
}
