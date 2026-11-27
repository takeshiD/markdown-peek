//! ① Section split + feature extraction and ② candidate registry + eligibility
//! prefilter (design `docs/design/jev-adaptive-ui.md` §3.1 / §3.2).
//!
//! Everything here is deterministic. Each candidate carries the *already built*
//! widget node (content extracted verbatim from the document, anchored by
//! `sourceRange`), so a selector — Jev, rules or fake — only ever picks an id.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use mdpeek_analyzer::Analysis;
use mdpeek_analyzer::model::BlockClass;
use mdpeek_parser::{Block, BlockKind};
use regex::Regex;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::ir::SourceRange;
use crate::ir::node::*;

/// Bumped whenever the candidate catalogue / prefilter / extraction changes.
/// Part of the selection cache key.
pub const REGISTRY_VERSION: u32 = 6;

/// Deterministic features of a section (sent to Jev instead of the full text).
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Features {
    pub chars: usize,
    pub paragraphs: usize,
    pub ordered_items: usize,
    pub bullet_items: usize,
    pub task_items: usize,
    pub tasks_done: usize,
    pub table_rows: usize,
    pub table_cols: usize,
    pub status_column: bool,
    /// Measure columns of the section's chartable table (0 = not chartable).
    pub chart_series: usize,
    /// Edges of a flow written as arrows / a from-to table (0 = none).
    pub flow_edges: usize,
    /// Subsections that read as consecutive stages of a process (0 = none, #65).
    pub stages: usize,
    /// Options of a scored comparison table (0 = none).
    pub compared_options: usize,
    pub shell_blocks: usize,
    pub command_lines: usize,
    pub config_blocks: usize,
    pub other_code_blocks: usize,
    /// Entries of the largest directory tree drawn in a code block (0 = none).
    pub tree_entries: usize,
    pub dated_lines: usize,
    pub alerts: usize,
    pub risk_blocks: usize,
    pub question_lines: usize,
}

/// One selectable widget for a section or the document.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// Stable option id, e.g. `"StepNavigator"` or `"ConfigViewer:config"`.
    pub id: &'static str,
    /// Short, fixed description used as the Jev `criteria` text.
    pub description: &'static str,
    /// Rules score in 0..1 (used as `helps` by the rules selector).
    pub score: f32,
    /// The deterministic widget content.
    pub node: UiNode,
}

/// A heading-delimited section (H1–H3) of the document.
#[derive(Debug, Clone)]
pub struct Section {
    /// Positional id used in Jev question names (`s0`, `s1`, …).
    pub id: String,
    /// Content-stable block id of the heading (or first block for a preamble).
    pub block_id: String,
    pub heading: String,
    pub level: u8,
    pub range: SourceRange,
    /// Content hash of the section source (cache key component).
    pub hash: String,
    /// First `excerpt_chars` characters of the body (never the full text).
    pub excerpt: String,
    pub features: Features,
    pub candidates: Vec<Candidate>,
}

impl Section {
    /// Strongest candidate score (used to order sections under the question cap).
    pub fn strength(&self) -> f32 {
        self.candidates.iter().map(|c| c.score).fold(0.0, f32::max)
    }
}

/// The whole document after split + prefilter.
#[derive(Debug, Clone)]
pub struct DocPlan {
    pub doc_hash: String,
    pub total_lines: u32,
    pub title: String,
    pub doc_anchor: (String, SourceRange),
    pub headings: Vec<String>,
    pub rules_doc_type: String,
    pub rules_doc_type_confidence: f32,
    pub sections: Vec<Section>,
    pub doc_candidates: Vec<Candidate>,
    /// Hash over the document-level signals (cache key for the doc question).
    pub doc_signature: String,
}

pub fn short_hash(s: &str) -> String {
    let digest = Sha256::digest(s.as_bytes());
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

fn conv(r: mdpeek_parser::SourceRange) -> SourceRange {
    SourceRange {
        start_line: r.start_line,
        start_column: r.start_column,
        end_line: r.end_line,
        end_column: r.end_column,
    }
}

fn block_id_str(b: &Block) -> String {
    format!("{:016x}", b.id.0)
}

/// Source lines covered by `r` (1-based, end exclusive at `end_column`).
fn lines_of<'a>(lines: &[&'a str], r: SourceRange) -> Vec<&'a str> {
    let start = r.start_line.saturating_sub(1) as usize;
    let mut end = r.end_line as usize; // exclusive index
    if r.end_column <= 1 && r.end_line > r.start_line {
        end -= 1;
    }
    lines[start.min(lines.len())..end.min(lines.len())].to_vec()
}

fn date_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(\b(19|20)\d{2}[-/.](0?[1-9]|1[0-2])([-/.](0?[1-9]|[12]\d|3[01]))?\b)|((19|20)\d{2}年\s*\d{1,2}月)|(\bv?\d+\.\d+\.\d+\b)",
        )
        .unwrap()
    })
}

fn step_heading_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)step|install|setup|手順|導入|セットアップ|インストール|procedure").unwrap()
    })
}

fn status_header_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^(status|state|result|状態|ステータス|結果|進捗)$").unwrap())
}

const SHELL_LANGS: &[&str] = &[
    "sh", "bash", "shell", "console", "zsh", "fish", "powershell", "ps1", "pwsh", "cmd", "bat",
    "shell-session", "terminal",
];

/// Code-block languages that may hold a diagram drawn in text (never real code).
const DIAGRAM_LANGS: &[&str] = &["", "text", "txt", "plain", "plaintext", "ascii", "diagram", "art"];

fn config_format(lang: &str) -> Option<ConfigFormat> {
    match lang {
        "json" | "jsonc" => Some(ConfigFormat::Json),
        "yaml" | "yml" => Some(ConfigFormat::Yaml),
        "toml" => Some(ConfigFormat::Toml),
        "env" | "dotenv" => Some(ConfigFormat::Env),
        _ => None,
    }
}

fn meta(range: SourceRange, score: f32) -> NodeMeta {
    NodeMeta {
        source_range: Some(range),
        confidence: Some(score),
        origin: Origin::Rules,
        ..Default::default()
    }
}

fn span(ranges: impl IntoIterator<Item = SourceRange>) -> Option<SourceRange> {
    let mut it = ranges.into_iter();
    let first = it.next()?;
    Some(it.fold(first, |a, r| {
        let (sl, sc) = if (r.start_line, r.start_column) < (a.start_line, a.start_column) {
            (r.start_line, r.start_column)
        } else {
            (a.start_line, a.start_column)
        };
        let (el, ec) = if (r.end_line, r.end_column) > (a.end_line, a.end_column) {
            (r.end_line, r.end_column)
        } else {
            (a.end_line, a.end_column)
        };
        SourceRange {
            start_line: sl,
            start_column: sc,
            end_line: el,
            end_column: ec,
        }
    }))
}

fn truncate_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Whether a semantic outline says more than the plain heading list: most
/// headings must land in a real group (not "Other"), and groups must not just
/// repeat their only heading ("Risks" → "Risks").
fn outline_is_informative(n: &crate::ir::node::SemanticOutlineNode) -> bool {
    let total: usize = n.groups.iter().map(|g| g.items.len()).sum();
    if total == 0 {
        return false;
    }
    let other: usize = n
        .groups
        .iter()
        .filter(|g| g.label.eq_ignore_ascii_case("other"))
        .map(|g| g.items.len())
        .sum();
    let tautological = n
        .groups
        .iter()
        .filter(|g| g.items.len() == 1 && g.items[0].title.trim().eq_ignore_ascii_case(g.label.trim()))
        .count();
    other * 2 < total && tautological * 2 < n.groups.len().max(1)
}

/// Split `markdown` into sections, compute features and eligible candidates.
pub fn plan_document(
    markdown: &str,
    filename: Option<&str>,
    analysis: &Analysis,
    excerpt_chars: usize,
) -> DocPlan {
    // Tolerate CRLF files: widget text (e.g. copyable commands) must not carry `\r`.
    let lines: Vec<&str> = markdown
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    let total_lines = crate::ir::LineIndex::new(markdown).line_count();
    let blocks = &analysis.tree.blocks;

    // Group top-level blocks into sections at H1–H3.
    struct Raw<'b> {
        head: Option<&'b Block>,
        level: u8,
        blocks: Vec<&'b Block>,
    }
    let mut raws: Vec<Raw> = vec![Raw {
        head: None,
        level: 0,
        blocks: Vec::new(),
    }];
    for b in blocks {
        match b.kind {
            BlockKind::Heading { level } if level <= 3 => raws.push(Raw {
                head: Some(b),
                level,
                blocks: Vec::new(),
            }),
            BlockKind::MetadataBlock => {}
            _ => raws.last_mut().unwrap().blocks.push(b),
        }
    }
    if raws[0].blocks.is_empty() {
        raws.remove(0);
    }

    // Heading lines carrying a date/version, used for nested timelines (e.g. a
    // changelog's `## [1.2.0] - 2024-01-01` releases under `# Changelog`).
    let all_heads: Vec<(usize, u8, &Block)> = raws
        .iter()
        .enumerate()
        .filter_map(|(i, r)| r.head.map(|h| (i, r.level, h)))
        .collect();

    let mut sections = Vec::new();
    for (idx, raw) in raws.iter().enumerate() {
        let heading = raw.head.map(|h| h.text.clone()).unwrap_or_default();
        let all_ranges = raw
            .head
            .iter()
            .chain(raw.blocks.iter())
            .map(|b| conv(b.range));
        let Some(range) = span(all_ranges) else {
            continue;
        };
        let block_id = raw
            .head
            .or_else(|| raw.blocks.first().copied())
            .map(block_id_str)
            .unwrap_or_default();
        let src = lines_of(&lines, range).join("\n");
        let body_range = span(raw.blocks.iter().map(|b| conv(b.range)));
        let body = body_range
            .map(|r| lines_of(&lines, r).join("\n"))
            .unwrap_or_default();

        let mut f = Features {
            chars: body.chars().count(),
            ..Default::default()
        };
        let mut cands: Vec<Candidate> = Vec::new();

        // --- walk blocks ---
        let mut steps: Vec<Step> = Vec::new();
        let mut checklist: Vec<ChecklistItem> = Vec::new();
        let mut configs: Vec<(ConfigFormat, String, SourceRange)> = Vec::new();
        let mut trees: Vec<(Vec<TreeEntry>, SourceRange)> = Vec::new();
        let mut events: Vec<TimelineEvent> = Vec::new();
        let mut tables: Vec<(Vec<String>, Vec<Vec<String>>, SourceRange)> = Vec::new();
        let mut list_items: Vec<(String, SourceRange)> = Vec::new();
        let mut alert: Option<(String, SourceRange)> = None;
        // Code blocks that may be drawn diagrams: (inner lines, line of the first).
        let mut diagrams: Vec<(Vec<&str>, u32)> = Vec::new();
        let mut risk_para: Option<(String, SourceRange)> = None;

        let sec_line_range = range.start_line..=range.end_line;
        let risk_ids: Vec<_> = analysis
            .model
            .blocks
            .iter()
            .filter(|c| c.class == BlockClass::Risk && sec_line_range.contains(&c.range.start_line))
            .map(|c| c.block_id)
            .collect();
        f.risk_blocks = risk_ids.len();

        for b in &raw.blocks {
            match &b.kind {
                BlockKind::Paragraph => {
                    f.paragraphs += 1;
                    if risk_para.is_none() && risk_ids.contains(&b.id) {
                        risk_para = Some((source_text(&lines, conv(b.range)), conv(b.range)));
                    }
                }
                BlockKind::List { ordered, .. } => {
                    for item in &b.children {
                        let BlockKind::Item { task } = item.kind else { continue };
                        let title = source_title(&lines, conv(item.range));
                        list_items.push((title.clone(), conv(item.range)));
                        if let Some(done) = task {
                            f.task_items += 1;
                            if done {
                                f.tasks_done += 1;
                            }
                            checklist.push(ChecklistItem {
                                title: title.clone(),
                                checked: done,
                                category: (!heading.is_empty()).then(|| heading.clone()),
                                source_range: Some(conv(item.range)),
                            });
                        } else if *ordered {
                            f.ordered_items += 1;
                            steps.push(Step {
                                title: title.clone(),
                                body: None,
                                duration: None,
                                prerequisites: Vec::new(),
                                source_range: Some(conv(item.range)),
                            });
                        } else {
                            f.bullet_items += 1;
                        }
                        if task.is_none() && date_re().is_match(&title) {
                            f.dated_lines += 1;
                            events.push(TimelineEvent {
                                title: title.clone(),
                                timestamp: date_re().find(&title).map(|m| m.as_str().to_string()),
                                description: None,
                                source_range: Some(conv(item.range)),
                            });
                        }
                        if title.trim_end().ends_with('?') || title.trim_end().ends_with('？') {
                            f.question_lines += 1;
                        }
                    }
                }
                BlockKind::CodeBlock { language } => {
                    let lang = language.as_deref().unwrap_or("").to_ascii_lowercase();
                    let r = conv(b.range);
                    let code_lines = lines_of(&lines, r);
                    // Strip the fences: keep the inner lines verbatim.
                    let fenced = code_lines.len() >= 2 && code_lines[0].trim_start().starts_with(['`', '~']);
                    let inner: Vec<&str> = if fenced {
                        let last = code_lines.len() - 1;
                        let end = if code_lines[last].trim_start().starts_with(['`', '~']) {
                            last
                        } else {
                            code_lines.len()
                        };
                        code_lines[1..end].to_vec()
                    } else {
                        code_lines.clone()
                    };
                    let content = inner.join("\n");
                    let first_inner = if inner.len() < code_lines.len() { r.start_line + 1 } else { r.start_line };
                    let tree = (lang != "mermaid")
                        .then(|| super::tree::tree_from_code(&inner, first_inner))
                        .flatten();
                    if let Some(entries) = tree {
                        f.tree_entries = f.tree_entries.max(entries.len());
                        trees.push((entries, r));
                    } else if SHELL_LANGS.contains(&lang.as_str()) {
                        f.shell_blocks += 1;
                        f.command_lines += inner
                            .iter()
                            .filter(|l| {
                                let t = l.trim();
                                !t.is_empty() && !t.starts_with('#')
                            })
                            .count();
                    } else if let Some(fmt) = config_format(&lang) {
                        f.config_blocks += 1;
                        configs.push((fmt, content, r));
                    } else {
                        f.other_code_blocks += 1;
                        if DIAGRAM_LANGS.contains(&lang.as_str()) {
                            diagrams.push((inner.clone(), r.start_line + u32::from(fenced)));
                        }
                    }
                }
                BlockKind::Table => {
                    let r = conv(b.range);
                    let tl = lines_of(&lines, r);
                    if tl.len() >= 2 {
                        let header = split_row(tl[0]);
                        let rows: Vec<Vec<String>> = tl[2..]
                            .iter()
                            .filter(|l| !l.trim().is_empty())
                            .map(|l| split_row(l))
                            .collect();
                        f.table_rows = f.table_rows.max(rows.len());
                        f.table_cols = f.table_cols.max(header.len());
                        if header.iter().any(|h| status_header_re().is_match(h.trim())) {
                            f.status_column = true;
                        }
                        tables.push((header, rows, r));
                    }
                }
                BlockKind::BlockQuote => {
                    let r = conv(b.range);
                    let first = lines_of(&lines, r).first().copied().unwrap_or("").to_string();
                    let up = first.to_ascii_uppercase();
                    if up.contains("[!WARNING]") || up.contains("[!CAUTION]") || up.contains("[!IMPORTANT]") {
                        f.alerts += 1;
                        if alert.is_none() {
                            alert = Some((alert_body(&lines, r), r));
                        }
                    }
                }
                _ => {}
            }
        }

        // Dated/versioned headings nested beneath this section (deeper levels).
        if let Some(h) = raw.head {
            let _ = h;
            for (j, lvl, hb) in all_heads.iter().filter(|(j, _, _)| *j > idx) {
                let _ = j;
                if *lvl <= raw.level {
                    break;
                }
                if date_re().is_match(&hb.text) {
                    f.dated_lines += 1;
                    events.push(TimelineEvent {
                        title: source_title(&lines, conv(hb.range)),
                        timestamp: date_re().find(&hb.text).map(|m| m.as_str().to_string()),
                        description: None,
                        source_range: Some(conv(hb.range)),
                    });
                }
            }
        }

        // --- candidates (prefilter + deterministic extraction) ---
        if steps.len() >= 3 {
            let mut s = if steps.len() >= 5 { 0.8 } else { 0.65 };
            if step_heading_re().is_match(&heading) {
                s += 0.1f32;
            }
            let r = span(steps.iter().filter_map(|x| x.source_range)).unwrap();
            cands.push(Candidate {
                id: "StepNavigator",
                description: "Step-by-step navigator over the section's ordered list items",
                score: s,
                node: UiNode::StepNavigator(StepNavigatorNode {
                    meta: meta(r, s),
                    steps: steps.clone(),
                }),
            });
        }
        if checklist.len() >= 2 {
            let s = if checklist.len() >= 3 { 0.75 } else { 0.6 };
            let r = span(checklist.iter().filter_map(|x| x.source_range)).unwrap();
            cands.push(Candidate {
                id: "Checklist",
                description: "Interactive checklist of the section's task items",
                score: s,
                node: UiNode::Checklist(ChecklistNode {
                    meta: meta(r, s),
                    items: checklist.clone(),
                }),
            });
        }
        if let Some((header, rows, r)) = tables.iter().max_by_key(|t| t.1.len())
            && rows.len() >= 3
        {
            let status = header.iter().any(|h| status_header_re().is_match(h.trim()));
            let columns: Vec<Column> = header
                .iter()
                .enumerate()
                .map(|(i, h)| Column {
                    key: format!("c{i}"),
                    label: h.clone(),
                    col_type: status_header_re().is_match(h.trim()).then_some(ColumnType::Status),
                })
                .collect();
            let rows_json = rows
                .iter()
                .map(|row| {
                    let mut m = serde_json::Map::new();
                    for (i, _) in header.iter().enumerate() {
                        let v = row.get(i).cloned().unwrap_or_default();
                        m.insert(format!("c{i}"), serde_json::Value::String(v));
                    }
                    m
                })
                .collect::<Vec<_>>();
            let node = |s: f32| {
                UiNode::DataTable(DataTableNode {
                    meta: meta(*r, s),
                    columns: columns.clone(),
                    rows: rows_json.clone(),
                })
            };
            if status {
                cands.push(Candidate {
                    id: "DataTable:status",
                    description: "Status board over a table that has a status column",
                    score: 0.75,
                    node: node(0.75),
                });
            }
            let s = if rows.len() >= 6 { 0.65 } else { 0.4 };
            cands.push(Candidate {
                id: "DataTable:sortable",
                description: "Sortable / filterable view of the section's table",
                score: s,
                node: node(s),
            });
        }
        // A table of measures reads faster as a chart than as numbers.
        if let Some(chart) = tables.iter().find_map(|(header, rows, r)| {
            super::chart::chart_from_table(
                header,
                rows,
                (!heading.is_empty()).then(|| heading.clone()),
                meta(*r, 0.0),
            )
        }) {
            f.chart_series = chart.series.len();
            let line = chart.chart == ChartType::Line;
            let s = if line { 0.75 } else { 0.7 } + if chart.labels.len() >= 6 { 0.05 } else { 0.0 };
            let mut chart = chart;
            chart.meta.confidence = Some(s);
            cands.push(Candidate {
                id: "Chart",
                description: "Chart of the numeric columns of the section's table (values parsed verbatim from the cells)",
                score: s,
                node: UiNode::Chart(chart),
            });
        }
        // A flow written as arrows reads faster as a graph.
        if let Some(mut g) = super::graph::graph_from_section(
            &list_items,
            &tables,
            (!heading.is_empty()).then(|| heading.clone()),
            meta(range, 0.0),
        ) {
            f.flow_edges = g.edges.len();
            let s = 0.72 + if super::graph::is_flow_heading(&heading) { 0.06 } else { 0.0 };
            g.meta.source_range = span(
                g.edges.iter().filter_map(|e| e.source_range).chain(g.nodes.iter().filter_map(|n| n.source_range)),
            );
            g.meta.confidence = Some(s);
            cands.push(Candidate {
                id: "NodeGraph",
                description: "Interactive node graph of the flow the section writes as arrows or a from/to table",
                score: s,
                node: UiNode::NodeGraph(g),
            });
        }
        // A diagram drawn with box-drawing / ASCII characters reads faster as a
        // graph. One node graph per section: the arrows written as a list win.
        if !cands.iter().any(|c| matches!(c.node, UiNode::NodeGraph(_)))
            && let Some(mut g) = diagrams.iter().find_map(|(inner, first)| {
                super::ascii::graph_from_diagram(
                    inner,
                    *first,
                    (!heading.is_empty()).then(|| heading.clone()),
                    meta(range, 0.0),
                )
            })
        {
            f.flow_edges = g.edges.len();
            let s = 0.72 + if super::graph::is_flow_heading(&heading) { 0.06 } else { 0.0 };
            g.meta.source_range = span(
                g.edges.iter().filter_map(|e| e.source_range).chain(g.nodes.iter().filter_map(|n| n.source_range)),
            );
            g.meta.confidence = Some(s);
            cands.push(Candidate {
                id: "NodeGraph:diagram",
                description: "Interactive node graph redrawn from the box-and-arrow diagram drawn in the section's code block",
                score: s,
                node: UiNode::NodeGraph(g),
            });
        }
        // Subsections that walk through the stages of a process (`### 3.1
        // parser — markdown -> BlockTree`, …) get a flow of those stages,
        // set in under the section's own text (#65). One flow per section.
        if !cands.iter().any(|c| matches!(c.node, UiNode::NodeGraph(_)))
            && let Some(h) = raw.head
        {
            let pos = blocks.iter().position(|b| b.id == h.id);
            let children: Vec<(String, SourceRange)> = pos
                .map(|p| {
                    blocks[p + 1..]
                        .iter()
                        .filter_map(|b| match b.kind {
                            BlockKind::Heading { level } => Some((level, b)),
                            _ => None,
                        })
                        .take_while(|(level, _)| *level > raw.level)
                        .filter(|(level, _)| *level == raw.level + 1)
                        .map(|(_, b)| (source_title(&lines, conv(b.range)), conv(b.range)))
                        .collect()
                })
                .unwrap_or_default();
            if let Some(mut g) = super::stages::stages_from_headings(
                &heading,
                &children,
                (!heading.is_empty()).then(|| heading.clone()),
                meta(range, 0.0),
            ) {
                f.stages = g.nodes.len();
                let s = 0.72 + if super::stages::is_stage_heading(&heading) { 0.06 } else { 0.0 };
                g.meta.source_range = span(g.nodes.iter().filter_map(|n| n.source_range));
                g.meta.confidence = Some(s);
                cands.push(Candidate {
                    id: "NodeGraph:stages",
                    description: "Flow diagram of the stages the section's subsections describe one by one",
                    score: s,
                    node: UiNode::NodeGraph(g),
                });
            }
        }
        // Options scored on criteria read faster side by side.
        if let Some(mut c) = tables.iter().find_map(|(header, rows, r)| {
            super::compare::comparison_from_table(
                header,
                rows,
                &body,
                (!heading.is_empty()).then(|| heading.clone()),
                meta(*r, 0.0),
            )
        }) {
            f.compared_options = c.options.len();
            let cue = super::compare::is_comparison_heading(&heading)
                || tables.iter().any(|(h, _, _)| h.first().is_some_and(|x| super::compare::is_comparison_heading(x)));
            let s = 0.72 + if cue { 0.08 } else { 0.0 };
            c.meta.confidence = Some(s);
            cands.push(Candidate {
                id: "Comparison",
                description: "Side-by-side cards comparing the options of the section's table on its scored criteria",
                score: s,
                node: UiNode::Comparison(c),
            });
        }
        // A directory layout drawn with `├──` / `└──` reads faster as a tree
        // that folds (#64). The largest one in the section.
        if let Some((entries, r)) = trees.into_iter().max_by_key(|t| t.0.len()) {
            let s = 0.74 + if super::tree::is_tree_heading(&heading) { 0.06 } else { 0.0 };
            cands.push(Candidate {
                id: "DirectoryTree",
                description: "Collapsible directory tree of the layout the section draws with tree lines in a code block",
                score: s,
                node: UiNode::DirectoryTree(DirectoryTreeNode {
                    meta: meta(r, s),
                    title: (!heading.is_empty()).then(|| heading.clone()),
                    entries,
                }),
            });
        }
        // No "Commands" aid (#67): a copy of the section's shell blocks added
        // little next to the blocks themselves, which now carry a copy button
        // in the body. `shell_blocks` / `command_lines` stay as features.
        if let Some((fmt, content, r)) = configs.first() {
            let n = content.lines().count();
            let s = if n >= 8 { 0.6 } else { 0.4 };
            cands.push(Candidate {
                id: "ConfigViewer:config",
                description: "Structured viewer for the section's JSON/YAML/TOML config block",
                score: s,
                node: UiNode::ConfigViewer(ConfigViewerNode {
                    meta: meta(*r, s),
                    format: *fmt,
                    content: content.clone(),
                    title: (!heading.is_empty()).then(|| heading.clone()),
                }),
            });
        }
        if events.len() >= 3 {
            let s = if events.len() >= 5 { 0.75 } else { 0.6 };
            let r = span(events.iter().filter_map(|e| e.source_range)).unwrap();
            cands.push(Candidate {
                id: "Timeline",
                description: "Timeline of the dated / versioned entries in this section",
                score: s,
                node: UiNode::Timeline(TimelineNode {
                    meta: meta(r, s),
                    events: events.clone(),
                }),
            });
        }
        // An alert with no text (`> [!WARNING]` alone) is not worth a callout.
        // A GFM alert is already highlighted in the body: repeating it in the
        // margin adds nothing. Only an un-highlighted risk paragraph is lifted.
        if let Some((text, r)) = risk_para.clone().filter(|_| alert.is_none()) {
            let s = 0.55;
            cands.push(Candidate {
                id: "Callout:risk",
                description: "Highlighted warning / risk callout quoting the section's text",
                score: s,
                node: UiNode::Callout(CalloutNode {
                    meta: meta(r, s),
                    severity: Severity::Warning,
                    title: (!heading.is_empty()).then(|| heading.clone()),
                    body: text,
                }),
            });
        }

        let excerpt = truncate_chars(&body, excerpt_chars);
        sections.push(Section {
            id: format!("s{}", sections.len()),
            block_id,
            heading,
            level: raw.level,
            range,
            hash: short_hash(&src),
            excerpt,
            features: f,
            candidates: cands,
        });
    }

    // --- document-level candidates: reuse the Layer 2 planner lenses ---
    let mut lenses = crate::planner::plan(analysis);
    crate::planner::order_lenses(analysis.model.doc_type.value, &mut lenses);
    let mut doc_candidates = Vec::new();
    let heading_count = analysis.model.outline.len();
    for mut node in lenses {
        // A glossary entry the document never defines only says "not defined
        // here": keep terms with a stated definition.
        if let UiNode::Glossary(n) = &mut node {
            n.terms
                .retain(|t| t.definition.as_deref().is_some_and(|d| !d.trim().is_empty()));
        }
        let (id, desc, items): (&'static str, &'static str, Vec<SourceRange>) = match &node {
            UiNode::SemanticOutline(n) => (
                "SemanticOutline",
                "Outline of the sections grouped by meaning",
                n.groups.iter().flat_map(|g| g.items.iter().filter_map(|i| i.source_range)).collect(),
            ),
            UiNode::RiskPanel(n) => (
                "RiskPanel",
                "Panel listing risks / assumptions stated in the document",
                n.risks.iter().filter_map(|r| r.source_range).collect(),
            ),
            UiNode::OpenQuestions(n) => (
                "OpenQuestions",
                "List of open / undecided questions in the document",
                n.questions.iter().filter_map(|q| q.source_range).collect(),
            ),
            UiNode::ActionItems(n) => (
                "ActionItems",
                "Action items / TODOs collected from the document",
                n.items.iter().filter_map(|i| i.source_range).collect(),
            ),
            UiNode::DecisionLog(n) => (
                "DecisionLog",
                "Log of the decisions recorded in the document",
                n.decisions.iter().filter_map(|d| d.source_range).collect(),
            ),
            UiNode::Glossary(n) => (
                "Glossary",
                "Glossary of terms / acronyms defined in the document",
                n.terms.iter().filter_map(|t| t.source_range).collect(),
            ),
            _ => continue,
        };
        // Prefilter thresholds (§3.2).
        let ok = match id {
            "SemanticOutline" => {
                heading_count >= 4 && matches!(&node, UiNode::SemanticOutline(n) if outline_is_informative(n))
            }
            "Glossary" => items.len() >= 3,
            // A list that is mostly ticked off is a status matrix (e.g. a
            // README feature list), not work to do.
            "ActionItems" => matches!(&node, UiNode::ActionItems(n) if {
                let done = n.items.iter().filter(|i| matches!(i.status, crate::ir::node::ActionStatus::Done)).count();
                !n.items.is_empty() && done * 4 < n.items.len() * 3
            }),
            _ => !items.is_empty(),
        };
        if !ok {
            continue;
        }
        let Some(r) = span(items.iter().copied()) else { continue };
        node.meta_mut().source_range = Some(r);
        let score = node.meta().confidence.unwrap_or(0.5);
        doc_candidates.push(Candidate {
            id,
            description: desc,
            score,
            node,
        });
    }

    let headings: Vec<String> = analysis.model.outline.iter().map(|o| o.title.clone()).collect();
    let title = analysis
        .model
        .outline
        .iter()
        .find(|o| o.level == 1)
        .map(|o| o.title.clone())
        .or_else(|| filename.map(str::to_string))
        .unwrap_or_default();
    let doc_block = blocks.first().map(block_id_str).unwrap_or_default();
    let doc_range = SourceRange {
        start_line: 1,
        start_column: 1,
        end_line: total_lines.max(1),
        end_column: 1,
    };
    let rules_doc_type = serde_json::to_value(analysis.model.doc_type.value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "generic".into());
    let mut sig = BTreeMap::new();
    sig.insert("headings", headings.join("\u{1f}"));
    sig.insert("doc_type", rules_doc_type.clone());
    sig.insert(
        "cands",
        doc_candidates.iter().map(|c| c.id).collect::<Vec<_>>().join(","),
    );
    sig.insert("file", filename.unwrap_or("").to_string());
    let doc_signature = short_hash(&format!("{sig:?}"));

    DocPlan {
        doc_hash: short_hash(markdown),
        total_lines,
        title,
        doc_anchor: (doc_block, doc_range),
        headings,
        rules_doc_type,
        rules_doc_type_confidence: analysis.model.doc_type.confidence,
        sections,
        doc_candidates,
        doc_signature,
    }
}

/// Verbatim source lines of a block (trimmed, joined with newlines).
fn source_text(lines: &[&str], r: SourceRange) -> String {
    lines_of(lines, r)
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Body of a GFM alert blockquote: its source lines with the `>` prefix and the
/// `[!WARNING]`-style marker removed (text kept verbatim).
fn alert_body(lines: &[&str], r: SourceRange) -> String {
    let mut out = Vec::new();
    for l in lines_of(lines, r) {
        let mut t = l.trim_start();
        while let Some(rest) = t.strip_prefix('>') {
            t = rest.trim_start();
        }
        let t = t.trim_end();
        if out.is_empty() && t.starts_with("[!") {
            // Marker line; text after `]` on the same line (rare) is kept.
            if let Some(i) = t.find(']') {
                let after = t[i + 1..].trim();
                if !after.is_empty() {
                    out.push(after.to_string());
                }
            }
            continue;
        }
        if !t.is_empty() {
            out.push(t.to_string());
        }
    }
    out.join("\n")
}

/// First source line of a list item / heading with its marker removed — the
/// verbatim text (inline markup kept), so widget content never paraphrases.
fn source_title(lines: &[&str], r: SourceRange) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"^\s*(?:#{1,6}\s+|(?:\d+[.)]|[-*+])\s+(?:\[[ xX]\]\s+)?)").unwrap()
    });
    let line = lines
        .get(r.start_line.saturating_sub(1) as usize)
        .copied()
        .unwrap_or("");
    // Drop a leading blockquote / container prefix before the marker column.
    let start = (r.start_column.saturating_sub(1) as usize).min(line.len());
    let line = line.get(start..).unwrap_or(line);
    re.replace(line, "").trim().trim_end_matches('#').trim().to_string()
}

/// Split a GFM table row into trimmed cell texts (escaped `\|` preserved).
fn split_row(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut prev_bs = false;
    for ch in t.chars() {
        if ch == '|' && !prev_bs {
            cells.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(ch);
        }
        prev_bs = ch == '\\';
    }
    cells.push(cur.trim().to_string());
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(md: &str) -> DocPlan {
        let a = mdpeek_analyzer::analyze(md, Some("README.md"));
        plan_document(md, Some("README.md"), &a, 600)
    }

    #[test]
    fn splits_sections_and_prefilters() {
        let md = "# Tool\n\nIntro.\n\n## Install\n\n```sh\ncargo build\ncargo install --path .\n```\n\n## Steps\n\n1. one\n2. two\n3. three\n\n## License\n\nMIT\n";
        let p = plan(md);
        let names: Vec<_> = p.sections.iter().map(|s| s.heading.as_str()).collect();
        assert_eq!(names, ["Tool", "Install", "Steps", "License"]);
        // Shell blocks get no "Commands" aid any more (#67).
        assert!(p.sections[1].candidates.is_empty());
        assert_eq!(p.sections[1].features.command_lines, 2);
        assert_eq!(p.sections[2].candidates[0].id, "StepNavigator");
        assert!(p.sections[3].candidates.is_empty());
    }

    #[test]
    fn table_cells_are_verbatim() {
        let md = "## T\n\n| Name | Status |\n|---|---|\n| a | ok |\n| b | ng |\n| c | ok |\n";
        let p = plan(md);
        let c = &p.sections[0].candidates;
        assert_eq!(c[0].id, "DataTable:status");
        let UiNode::DataTable(t) = &c[0].node else { panic!() };
        assert_eq!(t.rows[1]["c1"], "ng");
    }

    #[test]
    fn alert_already_highlighted_in_body_gets_no_callout() {
        // A GFM alert is shown highlighted in the body; a margin callout would
        // only repeat it (independent UX review, 2026-11-27).
        let md = "## Rollback\n\n> [!CAUTION]\n> Rolling back deletes data.\n> Be careful.\n";
        let p = plan(md);
        assert!(p.sections[0].candidates.iter().all(|c| c.id != "Callout:risk"));
        assert_eq!(p.sections[0].features.alerts, 1);
    }

    #[test]
    fn tree_code_block_yields_directory_tree() {
        let md = "# Repo\n\n## Layout\n\n```\nrepo/\n├── src/          # ★ code\n│   └── main.rs\n└── README.md\n```\n\n## Code\n\n```rust\nfn main() {}\n```\n";
        let p = plan(md);
        let c = &p.sections[1].candidates;
        assert_eq!(c[0].id, "DirectoryTree");
        let UiNode::DirectoryTree(t) = &c[0].node else { panic!() };
        assert_eq!(t.title.as_deref(), Some("Layout"));
        let names: Vec<_> = t.entries.iter().map(|e| (e.depth, e.name.as_str())).collect();
        assert_eq!(names, [(0, "repo/"), (1, "src/"), (2, "main.rs"), (1, "README.md")]);
        // Entry lines point at the document lines inside the fence.
        assert_eq!(t.entries[0].source_range.unwrap().start_line, 6);
        assert_eq!(t.entries[3].source_range.unwrap().start_line, 9);
        assert!(t.entries[1].marked);
        assert_eq!(p.sections[1].features.tree_entries, 4);
        assert_eq!(p.sections[1].features.other_code_blocks, 0);
        // Ordinary code gives no tree.
        assert!(p.sections[2].candidates.iter().all(|c| c.id != "DirectoryTree"));
        // The node passes validation.
        let mut nodes = vec![c[0].node.clone()];
        crate::ir::validate_nodes(&mut nodes, p.total_lines).unwrap();
    }

    #[test]
    fn stage_subsections_get_a_flow_in_the_parent_section() {
        // #65: AGENTS.md §3 style.
        let md = "## 3. パイプライン各段の責務\n\n各段の責務。\n\n### 3.1 parser — `markdown -> BlockTree`\n\ntext\n\n### 3.2 analyzer — 分類・抽出\n\ntext\n\n### 3.3 planner — semantic model → UI plan\n\ntext\n\n### 3.4 生成物の非永続方針\n\ntext\n";
        let p = plan(md);
        let c = p.sections[0].candidates.iter().find(|c| c.id == "NodeGraph:stages").unwrap();
        let UiNode::NodeGraph(g) = &c.node else { panic!() };
        assert_eq!(g.nodes.len(), 3);
        assert_eq!(g.nodes[1].source_range.unwrap().start_line, 9);
        assert_eq!(g.edges[0].label.as_deref(), Some("BlockTree"));
        assert_eq!(p.sections[0].features.stages, 3);
        // The subsections themselves get no stage flow.
        assert!(p.sections[1..].iter().all(|s| s.candidates.iter().all(|c| c.id != "NodeGraph:stages")));
    }

    #[test]
    fn drawn_diagrams_become_one_node_graph() {
        let diagram = include_str!("testdata/agents-critical-path.txt");
        let md = format!("## Path\n\n```\n{diagram}```\n\n## Mermaid\n\n```mermaid\n{diagram}```\n\n## Both\n\n- A → B\n- B → C\n\n```text\n{diagram}```\n");
        let p = plan(&md);
        let ids = |i: usize| p.sections[i].candidates.iter().map(|c| c.id).collect::<Vec<_>>();
        assert_eq!(ids(0), ["NodeGraph:diagram"]);
        let UiNode::NodeGraph(g) = &p.sections[0].candidates[0].node else { panic!() };
        assert_eq!(g.nodes.len(), 7);
        assert_eq!(g.nodes[0].source_range.unwrap().start_line, 4);
        assert_eq!(p.sections[0].features.flow_edges, 5);
        assert!(ids(1).is_empty(), "mermaid is never redrawn");
        assert_eq!(ids(2), ["NodeGraph"], "one graph per section: the list wins");
    }

    #[test]
    fn excerpt_is_bounded() {
        let body = "あ".repeat(2000);
        let md = format!("## Long\n\n{body}\n");
        let p = plan(&md);
        assert_eq!(p.sections[0].excerpt.chars().count(), 600);
    }
}
