//! Planner — turns Layer 2's analysis into **reading lenses** (design doc §8).
//!
//! This is the deterministic *rules fallback* for lens generation (the LLM is
//! primary under LLM-first; see [`crate::generate_with_llm`]). Unlike the old
//! structural extractor, it emits only *derived reading aids* — never a reprint
//! of body content (tables / code / diagrams stay in the Markdown Body, design
//! §7.2). Every item carries a `sourceRange` + `confidence` (§14).
//!
//! Lenses produced from `mdpeek_analyzer::Analysis`:
//! - Semantic Outline (§8.1) — sections grouped by meaning (Layer 2 `BlockClass`)
//! - Decision Log (§8.5) — explicit decision markers
//! - Action Items (§8.6) — task list + TODO/FIXME (Layer 2 `panel.todos`)
//! - Open Questions (§8.8) — TBD/未定/要確認 (Layer 2 `panel.open_questions`)
//! - Risk / Assumption Panel (§8.9) — `## Risk` sections
//! - Glossary (§8.7) — acronyms + inline definitions

use std::collections::BTreeMap;

use mdpeek_analyzer::Analysis;
use mdpeek_analyzer::model::BlockClass;

use crate::ir::SourceRange;
use crate::ir::node::*;

fn conv(r: mdpeek_analyzer::SourceRange) -> SourceRange {
    SourceRange {
        start_line: r.start_line,
        start_column: r.start_column,
        end_line: r.end_line,
        end_column: r.end_column,
    }
}

fn rules_meta(confidence: f32) -> NodeMeta {
    NodeMeta {
        source_range: None,
        confidence: Some(confidence),
        origin: Origin::Rules,
        ..Default::default()
    }
}

/// Produce reading lenses (rules fallback) from a Layer 2 analysis. Ordered so
/// the most orienting lens (Semantic Outline) comes first.
pub fn plan(analysis: &Analysis) -> Vec<UiNode> {
    let conf = analysis.model.doc_type.confidence;
    let mut out = Vec::new();
    if let Some(n) = semantic_outline(analysis, conf) {
        out.push(n);
    }
    if let Some(n) = decision_log(analysis, conf) {
        out.push(n);
    }
    if let Some(n) = action_items(analysis, conf) {
        out.push(n);
    }
    if let Some(n) = open_questions(analysis, conf) {
        out.push(n);
    }
    if let Some(n) = risk_panel(analysis, conf) {
        out.push(n);
    }
    if let Some(n) = glossary(analysis, conf) {
        out.push(n);
    }
    out
}

// --- Semantic Outline (§8.1) -------------------------------------------------

/// Group label + sort order for a block class.
fn group_of(class: BlockClass) -> (&'static str, u8) {
    match class {
        BlockClass::Overview => ("Overview", 0),
        BlockClass::Architecture | BlockClass::DataModel => ("Design", 1),
        BlockClass::Decision | BlockClass::Consequence => ("Decisions", 2),
        BlockClass::Usage | BlockClass::Configuration | BlockClass::Troubleshooting => ("Usage", 3),
        BlockClass::Step => ("Steps", 4),
        BlockClass::Task => ("Next Actions", 5),
        BlockClass::Risk => ("Risks", 6),
        BlockClass::OpenQuestion => ("Open Questions", 7),
        _ => ("Other", 8),
    }
}

fn semantic_outline(analysis: &Analysis, conf: f32) -> Option<UiNode> {
    if analysis.model.outline.is_empty() {
        return None;
    }
    // heading block_id -> its classified semantic class.
    let class_of = |block_id| {
        analysis
            .model
            .blocks
            .iter()
            .find(|b| b.block_id == block_id)
            .map(|b| b.class)
            .unwrap_or(BlockClass::Generic)
    };

    // group order -> (label, items)
    let mut groups: BTreeMap<u8, (String, Vec<OutlineItem>)> = BTreeMap::new();
    for entry in &analysis.model.outline {
        let (label, order) = group_of(class_of(entry.block_id));
        let item = OutlineItem {
            title: entry.title.clone(),
            reason: Some(label.to_string()),
            source_range: Some(conv(entry.range)),
        };
        groups
            .entry(order)
            .or_insert_with(|| (label.to_string(), Vec::new()))
            .1
            .push(item);
    }

    let groups: Vec<OutlineGroup> = groups
        .into_values()
        .map(|(label, items)| OutlineGroup {
            label,
            description: None,
            items,
        })
        .collect();

    Some(UiNode::SemanticOutline(SemanticOutlineNode {
        meta: rules_meta(conf),
        groups,
    }))
}

// --- Decision Log (§8.5) -----------------------------------------------------

/// Explicit decision markers (rules; LLM adds rationale/alternatives).
const DECISION_MARKERS: &[&str] = &[
    "決定", "採用", "却下", "方針とする", "合意", "we decided", "decided to", "we will use",
    "chosen", "adopt",
];

fn decision_log(analysis: &Analysis, conf: f32) -> Option<UiNode> {
    let mut decisions = Vec::new();
    for block in analysis.tree.iter() {
        let text = block.text.trim();
        if text.is_empty() {
            continue;
        }
        let lower = text.to_lowercase();
        let hit = DECISION_MARKERS.iter().any(|m| {
            if m.is_ascii() {
                lower.contains(m)
            } else {
                text.contains(m)
            }
        });
        // Skip headings themselves; use their body.
        if hit && !matches!(block.kind, mdpeek_analyzer::BlockKind::Heading { .. }) {
            let title = first_line(text);
            decisions.push(Decision {
                title: title.clone(),
                decision: title,
                alternatives: Vec::new(),
                reason: None,
                impact: None,
                status: DecisionStatus::Decided,
                confidence: Some(Confidence::Medium),
                source_range: Some(conv(block.range)),
            });
        }
    }
    if decisions.is_empty() {
        return None;
    }
    Some(UiNode::DecisionLog(DecisionLogNode {
        meta: rules_meta(conf),
        decisions,
    }))
}

// --- Action Items (§8.6) -----------------------------------------------------

fn action_items(analysis: &Analysis, conf: f32) -> Option<UiNode> {
    let todos = &analysis.panel.todos;
    if todos.is_empty() {
        return None;
    }
    let items = todos
        .iter()
        .map(|t| ActionItem {
            task: t.text.clone(),
            assignee: None,
            due_date: None,
            status: if t.done {
                ActionStatus::Done
            } else {
                ActionStatus::Todo
            },
            confidence: Some(Confidence::High),
            source_range: Some(conv(t.link.range)),
        })
        .collect();
    Some(UiNode::ActionItems(ActionItemsNode {
        meta: rules_meta(conf),
        items,
    }))
}

// --- Open Questions (§8.8) ---------------------------------------------------

fn open_questions(analysis: &Analysis, conf: f32) -> Option<UiNode> {
    let qs = &analysis.panel.open_questions;
    if qs.is_empty() {
        return None;
    }
    let questions = qs
        .iter()
        .map(|e| OpenQuestion {
            question: e.text.clone(),
            context: None,
            severity: Severity::Warning,
            confidence: Some(Confidence::High),
            source_range: Some(conv(e.link.range)),
        })
        .collect();
    Some(UiNode::OpenQuestions(OpenQuestionsNode {
        meta: rules_meta(conf),
        questions,
    }))
}

// --- Risk / Assumption Panel (§8.9) ------------------------------------------

fn risk_panel(analysis: &Analysis, conf: f32) -> Option<UiNode> {
    let risks = &analysis.panel.risks;
    if risks.is_empty() {
        return None;
    }
    let items = risks
        .iter()
        .map(|e| RiskItem {
            title: e.text.clone(),
            severity: Severity::Warning,
            note: None,
            likelihood: None,
            mitigation: None,
            confidence: Some(Confidence::from_score(conf)),
            source_range: Some(conv(e.link.range)),
        })
        .collect();
    Some(UiNode::RiskPanel(RiskPanelNode {
        meta: rules_meta(conf),
        risks: items,
        assumptions: Vec::new(),
    }))
}

// --- Glossary (§8.7) ---------------------------------------------------------

/// Acronyms: 2+ uppercase letters/digits, first char alpha (JWT, ADR, S3, API).
fn is_acronym(word: &str) -> bool {
    let w = word.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    w.len() >= 2
        && w.len() <= 8
        && w.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && w.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && w.chars().any(|c| c.is_ascii_alphabetic())
}

fn glossary(analysis: &Analysis, conf: f32) -> Option<UiNode> {
    // First appearance of each acronym.
    let mut seen: BTreeMap<String, SourceRange> = BTreeMap::new();
    for block in analysis.tree.iter() {
        if matches!(block.kind, mdpeek_analyzer::BlockKind::CodeBlock { .. }) {
            continue;
        }
        for raw in block.text.split(|c: char| c.is_whitespace()) {
            let w = raw.trim_matches(|c: char| !c.is_ascii_alphanumeric());
            if is_acronym(w) && !seen.contains_key(w) {
                seen.insert(w.to_string(), conv(block.range));
            }
        }
    }
    if seen.len() < 2 {
        return None; // not worth a glossary for 0–1 acronyms
    }
    let terms = seen
        .into_iter()
        .map(|(term, range)| GlossaryTerm {
            term,
            aliases: Vec::new(),
            definition: None,
            inferred_definition: None,
            confidence: Some(Confidence::Low), // rules only located it, didn't define
            source_range: Some(range),
        })
        .collect();
    Some(UiNode::Glossary(GlossaryNode {
        meta: rules_meta(conf),
        terms,
    }))
}

fn first_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or(text).trim();
    if line.chars().count() > 120 {
        line.chars().take(117).collect::<String>() + "…"
    } else {
        line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan_md(md: &str, filename: Option<&str>) -> Vec<UiNode> {
        plan(&mdpeek_analyzer::analyze(md, filename))
    }

    #[test]
    fn design_doc_emits_reading_lenses() {
        let md = "# Design\n\n## Overview\n\nWe adopt JWT for API auth.\n\n\
                  ## Architecture\n\nAPI Gateway calls Lambda.\n\n\
                  ## Risks\n\nToken revocation is unresolved.\n\n\
                  ## TODO\n\n- [ ] design revocation\n";
        let kinds: Vec<&str> = plan_md(md, Some("DESIGN.md")).iter().map(|n| n.kind()).collect();
        assert!(kinds.contains(&"SemanticOutline"), "{kinds:?}");
        assert!(kinds.contains(&"RiskPanel"), "{kinds:?}");
        assert!(kinds.contains(&"ActionItems"), "{kinds:?}");
        // Body reprints must NOT appear as lenses.
        assert!(!kinds.contains(&"DataTable"));
        assert!(!kinds.contains(&"Diagram"));
    }

    #[test]
    fn glossary_collects_acronyms() {
        let md = "# Doc\n\nWe use JWT and ADR. JWT is a token; ADR records decisions.\n";
        let g = plan_md(md, None)
            .into_iter()
            .find_map(|n| match n {
                UiNode::Glossary(g) => Some(g),
                _ => None,
            })
            .expect("glossary");
        let terms: Vec<String> = g.terms.iter().map(|t| t.term.clone()).collect();
        assert!(terms.contains(&"JWT".to_string()));
        assert!(terms.contains(&"ADR".to_string()));
    }

    #[test]
    fn plain_prose_plans_nothing() {
        assert!(plan_md("Just a sentence.\n", None).is_empty());
    }
}
