//! Node-graph candidates from flows written in the document (deterministic).
//!
//! Sources, both taken verbatim:
//! - list items that are arrow chains: `Planning → Frontend, Backend`,
//!   `Verification --PASS--> Release`, `A -->|RETRY| B`, `A -[ok]-> B`;
//! - tables with a *from* and a *to* column (and optionally a label column).
//!
//! A node's subtitle comes from `Name (subtitle)` in a chain, or from a
//! definition item in the same section (`- **Name**: subtitle`).

use std::sync::OnceLock;

use regex::Regex;

use super::chart::plain;

/// A Markdown table: header cells, body rows, source range.
pub type Table = (Vec<String>, Vec<Vec<String>>, SourceRange);
use crate::ir::SourceRange;
use crate::ir::node::{FlowEdge, FlowNode, NodeGraphNode, NodeMeta};

pub(super) const MIN_NODES: usize = 3;
pub(super) const MIN_EDGES: usize = 2;
pub(super) const MAX_NODES: usize = 24;
pub(super) const MAX_NAME_CHARS: usize = 40;

fn arrow_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"\s*(?:--\s*(?P<a>[^\s>\-][^>]*?)\s*-->|-->\s*\|(?P<b>[^|]+)\||-\[(?P<c>[^\]]+)\]->|-->|->|=>|→|⇒|⟶|➡)\s*",
        )
        .unwrap()
    })
}

fn paren_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(?P<l>.+?)\s*[（(](?P<n>[^()（）]+)[)）]$").unwrap())
}

fn def_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(?P<k>[^:：—–]{1,40}?)\s*(?:[:：]|\s[—–-]\s)\s*(?P<v>.+)$").unwrap()
    })
}

fn from_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^(from|source|src|start|始点|起点|元|遷移元|前工程|前)$").unwrap()
    })
}
fn to_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^(to|target|dst|dest|next|end|終点|先|遷移先|次|後工程|後)$").unwrap()
    })
}
fn label_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)^(label|condition|when|trigger|event|result|条件|契機|イベント|結果|ラベル)$",
        )
        .unwrap()
    })
}

/// Words that mark a section as being about a flow (raises the rules score).
pub fn is_flow_heading(h: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)flow|pipeline|graph|workflow|architecture|state|dependenc|フロー|パイプライン|グラフ|ワークフロー|構成|流れ|遷移|全体像|アーキテクチャ|依存関係")
            .unwrap()
    })
    .is_match(h)
}

/// `"Planning (Orchestrator)"` → (`"Planning"`, `Some("Orchestrator")`).
pub(super) fn split_name(raw: &str) -> Option<(String, Option<String>)> {
    let t = plain(raw.trim().trim_matches(['"', '\'', '「', '」']));
    if t.is_empty() || t.chars().count() > MAX_NAME_CHARS || t.ends_with(['。', '.']) {
        return None;
    }
    if let Some(c) = paren_re().captures(&t) {
        return Some((plain(&c["l"]), Some(c["n"].trim().to_string())));
    }
    Some((t, None))
}

fn key(label: &str) -> String {
    label.trim().to_lowercase()
}

#[derive(Default)]
pub(super) struct Builder {
    pub(super) nodes: Vec<FlowNode>,
    pub(super) edges: Vec<FlowEdge>,
}

impl Builder {
    pub(super) fn node(&mut self, label: &str, note: Option<String>, r: SourceRange) -> String {
        let id = key(label);
        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == id) {
            if n.note.is_none() {
                n.note = note;
            }
        } else {
            self.nodes.push(FlowNode {
                id: id.clone(),
                label: label.to_string(),
                note,
                source_range: Some(r),
            });
        }
        id
    }
    pub(super) fn edge(&mut self, from: String, to: String, label: Option<String>, r: SourceRange) {
        if from == to
            || self
                .edges
                .iter()
                .any(|e| e.from == from && e.to == to && e.label == label)
        {
            return;
        }
        self.edges.push(FlowEdge {
            from,
            to,
            label,
            source_range: Some(r),
        });
    }
}

/// Parse one list item as an arrow chain; `None` when it is not one.
#[allow(clippy::type_complexity)]
fn parse_chain(text: &str) -> Option<(Vec<Vec<(String, Option<String>)>>, Vec<Option<String>>)> {
    let re = arrow_re();
    let mut segs = Vec::new();
    let mut labels = Vec::new();
    let mut last = 0;
    for c in re.captures_iter(text) {
        let m = c.get(0).unwrap();
        segs.push(&text[last..m.start()]);
        labels.push(
            c.name("a")
                .or(c.name("b"))
                .or(c.name("c"))
                .map(|x| x.as_str().trim().to_string()),
        );
        last = m.end();
    }
    if labels.is_empty() {
        return None;
    }
    segs.push(&text[last..]);
    let groups: Option<Vec<Vec<(String, Option<String>)>>> = segs
        .iter()
        .map(|s| {
            let names: Option<Vec<_>> = s.split([',', '、', '，']).map(split_name).collect();
            names.filter(|v| !v.is_empty())
        })
        .collect();
    Some((groups?, labels))
}

/// Build a node graph from a section's list items and tables.
pub fn graph_from_section(
    items: &[(String, SourceRange)],
    tables: &[Table],
    title: Option<String>,
    meta: NodeMeta,
) -> Option<NodeGraphNode> {
    let mut b = Builder::default();
    let mut defs: Vec<(&str, SourceRange)> = Vec::new();
    for (text, r) in items {
        match parse_chain(text) {
            Some((groups, labels)) => {
                let ids: Vec<Vec<String>> = groups
                    .iter()
                    .map(|g| g.iter().map(|(l, n)| b.node(l, n.clone(), *r)).collect())
                    .collect();
                for (i, label) in labels.iter().enumerate() {
                    for from in &ids[i] {
                        for to in &ids[i + 1] {
                            b.edge(from.clone(), to.clone(), label.clone(), *r);
                        }
                    }
                }
            }
            None => defs.push((text.as_str(), *r)),
        }
    }
    for (header, rows, r) in tables {
        let find = |re: &Regex| header.iter().position(|h| re.is_match(plain(h).trim()));
        let (Some(fc), Some(tc)) = (find(from_re()), find(to_re())) else {
            continue;
        };
        let lc = find(label_re());
        for (i, row) in rows.iter().enumerate() {
            let line = r.start_line + 2 + i as u32;
            let width = 1 + rows[i]
                .iter()
                .map(|c| c.chars().count() as u32 + 3)
                .sum::<u32>();
            let rr = SourceRange {
                start_line: line,
                start_column: 1,
                end_line: line,
                end_column: width.max(2),
            };
            let cell = |c: usize| row.get(c).map(String::as_str).unwrap_or("");
            let froms: Option<Vec<_>> = cell(fc).split([',', '、']).map(split_name).collect();
            let tos: Option<Vec<_>> = cell(tc).split([',', '、']).map(split_name).collect();
            let (Some(froms), Some(tos)) = (froms, tos) else {
                continue;
            };
            let label = lc.map(|c| plain(cell(c))).filter(|l| !l.is_empty());
            for (fl, fnote) in &froms {
                let f = b.node(fl, fnote.clone(), rr);
                for (tl, tnote) in &tos {
                    let t = b.node(tl, tnote.clone(), rr);
                    b.edge(f.clone(), t, label.clone(), rr);
                }
            }
        }
    }
    if b.nodes.len() < MIN_NODES || b.edges.len() < MIN_EDGES || b.nodes.len() > MAX_NODES {
        return None;
    }
    // Definition items describe nodes: `- **Planning**: the orchestrator`.
    for (text, r) in defs {
        let t = plain(text);
        let Some(c) = def_re().captures(&t) else {
            continue;
        };
        let k = key(&plain(&c["k"]));
        if let Some(n) = b.nodes.iter_mut().find(|n| n.id == k) {
            n.note = Some(c["v"].trim().to_string());
            n.source_range = Some(r);
        }
    }
    Some(NodeGraphNode {
        meta,
        title,
        nodes: b.nodes,
        edges: b.edges,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(l: u32) -> SourceRange {
        SourceRange {
            start_line: l,
            start_column: 1,
            end_line: l,
            end_column: 20,
        }
    }
    fn items(v: &[&str]) -> Vec<(String, SourceRange)> {
        v.iter()
            .enumerate()
            .map(|(i, s)| (s.to_string(), r(i as u32 + 1)))
            .collect()
    }

    #[test]
    fn arrow_chains_with_fan_out_labels_and_notes() {
        let g = graph_from_section(
            &items(&[
                "Planning (Orchestrator) → Frontend, Backend",
                "Frontend, Backend → Verification",
                "Verification --PASS--> Release",
                "Verification -->|RETRY| Backend",
                "**Release**: Completed",
                "This sentence has no arrow at all.",
            ]),
            &[],
            Some("Flow".into()),
            NodeMeta::default(),
        )
        .unwrap();
        let labels: Vec<_> = g.nodes.iter().map(|n| n.label.as_str()).collect();
        assert_eq!(
            labels,
            ["Planning", "Frontend", "Backend", "Verification", "Release"]
        );
        assert_eq!(g.nodes[0].note.as_deref(), Some("Orchestrator"));
        assert_eq!(g.nodes[4].note.as_deref(), Some("Completed"));
        assert_eq!(g.edges.len(), 6);
        let pass = g.edges.iter().find(|e| e.to == "release").unwrap();
        assert_eq!(pass.label.as_deref(), Some("PASS"));
        let retry = g
            .edges
            .iter()
            .find(|e| e.from == "verification" && e.to == "backend")
            .unwrap();
        assert_eq!(retry.label.as_deref(), Some("RETRY"));
    }

    #[test]
    fn from_to_table() {
        let h: Vec<String> = ["From", "To", "条件"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let rows: Vec<Vec<String>> = [
            ["受付", "審査", ""],
            ["審査", "承認", "OK"],
            ["審査", "差戻し", "NG"],
        ]
        .iter()
        .map(|r| r.iter().map(|s| s.to_string()).collect())
        .collect();
        let g = graph_from_section(&[], &[(h, rows, r(10))], None, NodeMeta::default()).unwrap();
        assert_eq!(g.nodes.len(), 4);
        assert_eq!(g.edges[1].label.as_deref(), Some("OK"));
        assert_eq!(g.edges[2].source_range.unwrap().start_line, 14);
    }

    #[test]
    fn prose_and_tiny_flows_are_not_graphs() {
        assert!(graph_from_section(&items(&["A → B"]), &[], None, NodeMeta::default()).is_none());
        assert!(
            graph_from_section(
                &items(&["When the build fails we usually go back and look at the logs → then retry the whole thing", "x"]),
                &[],
                None,
                NodeMeta::default()
            )
            .is_none()
        );
    }
}
