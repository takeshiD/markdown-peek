//! Stage-flow candidates (#65): a section whose subsections describe the
//! stages of a process one by one (`### 3.1 parser — markdown -> BlockTree`,
//! `### 3.2 analyzer — …`) gets a flow diagram of those stages.
//!
//! Deterministic and verbatim: node labels and notes are the subsection
//! headings' own text, edge labels are the input / output a heading states
//! (`A -> B`). Each node points at its subsection heading (`sourceRange`).

use std::sync::OnceLock;

use regex::Regex;

use super::chart::plain;
use crate::ir::SourceRange;
use crate::ir::node::{FlowEdge, FlowNode, NodeGraphNode, NodeMeta};

const MIN_STAGES: usize = 3;
const MAX_STAGES: usize = 16;
const MAX_NAME_CHARS: usize = 32;

/// Words that mark a section as describing stages (raises the rules score;
/// without them the stage headings must state their inputs / outputs).
pub fn is_stage_heading(h: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)pipeline|stage|phase|flow|process|workflow|lifecycle|段階|各段|工程|パイプライン|フロー|処理|流れ|フェーズ|ステップ",
        )
        .unwrap()
    })
    .is_match(h)
}

/// Leading numbering: `3.1`, `3.1.`, `1)`, `Step 2:`, `Phase 3 -`, `第2段階`, `①`.
fn numbering_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)^\s*(?:(?:step|phase|stage)\s*\d+\s*[:.)\-–—]?|\d+(?:\.\d+)*[.)]?|第\s*\d+\s*(?:段階|工程|段)?[:：.]?|[①-⑳])\s*",
        )
        .unwrap()
    })
}

/// `name — description` (em/en dash, spaced hyphen, colon).
fn split_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(?P<n>.+?)\s*(?:\s[—–-]\s|—|–|[:：])\s*(?P<d>.+)$").unwrap())
}

/// `input -> output` inside a description.
fn io_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(?P<i>.+?)\s*(?:->|→|=>|⇒|⟶)\s*(?P<o>.+)$").unwrap())
}

/// Inline code / emphasis removed from a whole fragment, and backticks inside
/// (`` `markdown -> BlockTree` `` → `markdown -> BlockTree`).
fn unmark(s: &str) -> String {
    plain(s).replace('`', "").trim().to_string()
}

#[derive(Debug, Clone, PartialEq)]
struct Stage {
    name: String,
    note: Option<String>,
    input: Option<String>,
    output: Option<String>,
    range: SourceRange,
}

fn parse_stage(heading: &str, range: SourceRange) -> Option<Stage> {
    let rest = numbering_re().replace(heading.trim(), "");
    let rest = rest.trim();
    let (name, note) = match split_re().captures(rest) {
        Some(c) => (unmark(&c["n"]), Some(unmark(&c["d"]))),
        None => (unmark(rest), None),
    };
    // A heading that is itself `A -> B` names its I/O but no stage.
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS || io_re().is_match(&name) {
        return None;
    }
    let (input, output) = note
        .as_deref()
        .and_then(|d| io_re().captures(d))
        .map(|c| {
            (
                Some(c["i"].trim().to_string()),
                Some(c["o"].trim().to_string()),
            )
        })
        .unwrap_or((None, None));
    // A stage heading says what the stage is: `name — what` or `name — in -> out`.
    note.as_ref()?;
    Some(Stage {
        name,
        note,
        input,
        output,
        range,
    })
}

/// Build the stage flow of a section from its direct subsection headings
/// (text, range), in document order. `None` unless at least three consecutive
/// subsections read as stages and the section is about stages (its heading
/// says so, or the stages state their inputs / outputs).
pub fn stages_from_headings(
    section_heading: &str,
    children: &[(String, SourceRange)],
    title: Option<String>,
    meta: NodeMeta,
) -> Option<NodeGraphNode> {
    // The longest run of consecutive stage headings.
    let mut best: Vec<Stage> = Vec::new();
    let mut cur: Vec<Stage> = Vec::new();
    for (text, r) in children {
        match parse_stage(text, *r) {
            Some(s) => cur.push(s),
            None => {
                if cur.len() > best.len() {
                    best = std::mem::take(&mut cur);
                }
                cur.clear();
            }
        }
    }
    if cur.len() > best.len() {
        best = cur;
    }
    if best.len() < MIN_STAGES || best.len() > MAX_STAGES {
        return None;
    }
    let with_io = best.iter().filter(|s| s.output.is_some()).count();
    if !is_stage_heading(section_heading) && with_io < 2 {
        return None;
    }
    // Distinct names only (a repeated name is not a sequence of stages).
    let mut ids: Vec<String> = Vec::new();
    for s in &best {
        let id = s.name.to_lowercase();
        if ids.contains(&id) {
            return None;
        }
        ids.push(id);
    }
    let nodes = best
        .iter()
        .zip(&ids)
        .map(|(s, id)| FlowNode {
            id: id.clone(),
            label: s.name.clone(),
            note: s.note.clone(),
            source_range: Some(s.range),
        })
        .collect();
    let edges = best
        .windows(2)
        .zip(ids.windows(2))
        .map(|(w, i)| FlowEdge {
            from: i[0].clone(),
            to: i[1].clone(),
            // What passes between them: the output the first one names, else
            // the input the next one names.
            label: w[0].output.clone().or_else(|| w[1].input.clone()),
            source_range: Some(w[1].range),
        })
        .collect();
    Some(NodeGraphNode {
        meta,
        title,
        nodes,
        edges,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(l: u32) -> SourceRange {
        SourceRange {
            start_line: l,
            start_column: 1,
            end_line: l + 1,
            end_column: 1,
        }
    }
    fn heads(v: &[&str]) -> Vec<(String, SourceRange)> {
        v.iter()
            .enumerate()
            .map(|(i, s)| (s.to_string(), r(10 * (i as u32 + 1))))
            .collect()
    }

    #[test]
    fn agents_md_pipeline_section() {
        // AGENTS.md §3 "パイプライン各段の責務".
        let g = stages_from_headings(
            "3. パイプライン各段の責務",
            &heads(&[
                "3.1 parser — `markdown -> BlockTree`",
                "3.2 analyzer — 分類・抽出",
                "3.3 planner — semantic model → UI plan",
                "3.4 generator — UI plan → UI IR",
                "3.5 validate — schema + sourceRange 検証",
                "3.6 生成物の非永続方針",
            ]),
            Some("Pipeline".into()),
            NodeMeta::default(),
        )
        .unwrap();
        let labels: Vec<_> = g.nodes.iter().map(|n| n.label.as_str()).collect();
        assert_eq!(
            labels,
            ["parser", "analyzer", "planner", "generator", "validate"]
        );
        assert_eq!(g.nodes[0].note.as_deref(), Some("markdown -> BlockTree"));
        assert_eq!(g.nodes[1].note.as_deref(), Some("分類・抽出"));
        assert_eq!(g.nodes[0].source_range.unwrap().start_line, 10);
        let labels: Vec<_> = g.edges.iter().map(|e| e.label.as_deref()).collect();
        assert_eq!(
            labels,
            [
                Some("BlockTree"),
                Some("semantic model"),
                Some("UI plan"),
                Some("UI IR")
            ]
        );
        assert_eq!(g.edges[0].from, "parser");
        assert_eq!(g.edges[3].to, "validate");
    }

    #[test]
    fn io_without_cue_words_and_english_numbering() {
        let g = stages_from_headings(
            "How it works",
            &heads(&[
                "Step 1: Fetch — URL -> HTML",
                "Step 2: Parse — HTML -> DOM",
                "Step 3: Render — DOM → pixels",
            ]),
            None,
            NodeMeta::default(),
        )
        .unwrap();
        assert_eq!(g.nodes.len(), 3);
        assert_eq!(g.nodes[0].label, "Fetch");
        assert_eq!(g.edges[1].label.as_deref(), Some("DOM"));
    }

    #[test]
    fn ordinary_subsections_are_not_stages() {
        // Plain topic headings: no "name — what" form.
        assert!(
            stages_from_headings(
                "処理の流れ",
                &heads(&["概要", "インストール", "設定"]),
                None,
                NodeMeta::default()
            )
            .is_none()
        );
        // Named sections without a stage cue and without I/O.
        assert!(
            stages_from_headings(
                "FAQ",
                &heads(&["Q1 — billing", "Q2 — accounts", "Q3 — refunds"]),
                None,
                NodeMeta::default()
            )
            .is_none()
        );
        // Too few.
        assert!(
            stages_from_headings(
                "Pipeline",
                &heads(&["parse — a -> b", "emit — b -> c"]),
                None,
                NodeMeta::default()
            )
            .is_none()
        );
    }
}
