//! Provisional UI IR (mirror of AGENTS.md §4).
//!
//! This is the wire format contract shared between the web renderer (Preact)
//! and this TUI renderer (ratatui): the exact same JSON is consumed by both.
//! It is defined here only so Layer 5 can be built and tested before Layer 3's
//! `mdpeek-core::ir` exists. Once that crate lands, delete this file and
//! `use mdpeek_core::ir::*;` instead — the JSON shape is identical.
//!
//! Design invariants honoured (DESIGN.md "重要な設計思想"):
//! - every node may carry a `sourceRange` (via [`NodeMeta`]);
//! - unknown `kind`s are rejected at render time (the renderer only knows the
//!   fixed registry below);
//! - the renderer is purely deterministic — it never executes arbitrary code.

use serde::{Deserialize, Serialize};

/// A single generated-UI node. The renderer dispatches on `kind`.
///
/// `#[serde(tag = "kind")]` gives a discriminated union matching the web
/// registry 1:1. The `snake_case`/PascalCase tags match `DESIGN.md`'s
/// `componentRegistry` keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum UiNode {
    // --- core registry (§5.1 inner layer) ---
    Tabs(TabsNode),
    Timeline(TimelineNode),
    Checklist(ChecklistNode),
    DataTable(DataTableNode),
    Diagram(DiagramNode),
    Callout(CalloutNode),
    RiskPanel(RiskPanelNode),
    DependencyGraph(DependencyGraphNode),
    LogTimeline(LogTimelineNode),
    CommitGraph(CommitGraphNode),

    // --- domain primitives (§5.1 outer layer) ---
    Glossary(GlossaryNode),
    StepNavigator(StepNavigatorNode),
}

/// Source location in the original Markdown. All UI is anchored to a range so
/// the renderer can highlight / jump to the source (DESIGN.md).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SourceRange {
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

/// Common metadata carried by every node (sourceRange + confidence + origin +
/// visibility). Flattened into each node per AGENTS.md 論点 D (flatten 推奨).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeMeta {
    #[serde(default)]
    pub source_range: Option<SourceRange>,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub origin: Origin,
    #[serde(default)]
    pub visibility: Visibility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    #[default]
    Rules,
    Llm,
}

/// Node visibility, e.g. novels hide content past the reader's position
/// (§9.3 reading-position aware). Default is always visible.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    #[default]
    Always,
    /// Only visible once the reader has read past `reveal_after_line`.
    UntilRead { reveal_after_line: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

// ---------------------------------------------------------------------------
// Core nodes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabsNode {
    #[serde(flatten, default)]
    pub meta: NodeMeta,
    pub tabs: Vec<Tab>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tab {
    pub title: String,
    pub children: Vec<UiNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecklistNode {
    #[serde(flatten, default)]
    pub meta: NodeMeta,
    pub items: Vec<ChecklistItem>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecklistItem {
    pub title: String,
    pub checked: bool,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub source_range: Option<SourceRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataTableNode {
    #[serde(flatten, default)]
    pub meta: NodeMeta,
    pub columns: Vec<Column>,
    pub rows: Vec<serde_json::Map<String, serde_json::Value>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    pub key: String,
    pub label: String,
    #[serde(rename = "type", default)]
    pub col_type: Option<ColumnType>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColumnType {
    Text,
    Number,
    Status,
    Link,
    Code,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineNode {
    #[serde(flatten, default)]
    pub meta: NodeMeta,
    pub events: Vec<TimelineEvent>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub label: String,
    #[serde(default)]
    pub timestamp: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalloutNode {
    #[serde(flatten, default)]
    pub meta: NodeMeta,
    pub severity: Severity,
    #[serde(default)]
    pub title: Option<String>,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskPanelNode {
    #[serde(flatten, default)]
    pub meta: NodeMeta,
    pub risks: Vec<Risk>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Risk {
    pub title: String,
    pub severity: Severity,
    #[serde(default)]
    pub mitigation: Option<String>,
}

/// A rendered diagram (e.g. Mermaid). The TUI cannot draw it, so it falls back
/// to a summary + "open in web" hint (§5.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagramNode {
    #[serde(flatten, default)]
    pub meta: NodeMeta,
    #[serde(default)]
    pub title: Option<String>,
    /// Diagram language, e.g. "mermaid", "dot".
    #[serde(default)]
    pub lang: Option<String>,
    /// Plain-text summary the TUI shows instead of the picture.
    #[serde(default)]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyGraphNode {
    #[serde(flatten, default)]
    pub meta: NodeMeta,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub nodes: Vec<String>,
    #[serde(default)]
    pub edges: Vec<Edge>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogTimelineNode {
    #[serde(flatten, default)]
    pub meta: NodeMeta,
    pub entries: Vec<LogEntry>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    #[serde(default)]
    pub timestamp: Option<String>,
    pub level: Severity,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitGraphNode {
    #[serde(flatten, default)]
    pub meta: NodeMeta,
    pub commits: Vec<Commit>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Commit {
    pub hash: String,
    pub summary: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub date: Option<String>,
}

// ---------------------------------------------------------------------------
// Domain primitives
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlossaryNode {
    #[serde(flatten, default)]
    pub meta: NodeMeta,
    pub terms: Vec<GlossaryTerm>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlossaryTerm {
    pub term: String,
    pub definition: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepNavigatorNode {
    #[serde(flatten, default)]
    pub meta: NodeMeta,
    pub steps: Vec<Step>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub title: String,
    #[serde(default)]
    pub prerequisite: Option<String>,
    #[serde(default)]
    pub duration: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
}

impl UiNode {
    /// Human-readable node kind, used for the fallback header and tests.
    pub fn kind_name(&self) -> &'static str {
        match self {
            UiNode::Tabs(_) => "Tabs",
            UiNode::Timeline(_) => "Timeline",
            UiNode::Checklist(_) => "Checklist",
            UiNode::DataTable(_) => "DataTable",
            UiNode::Diagram(_) => "Diagram",
            UiNode::Callout(_) => "Callout",
            UiNode::RiskPanel(_) => "RiskPanel",
            UiNode::DependencyGraph(_) => "DependencyGraph",
            UiNode::LogTimeline(_) => "LogTimeline",
            UiNode::CommitGraph(_) => "CommitGraph",
            UiNode::Glossary(_) => "Glossary",
            UiNode::StepNavigator(_) => "StepNavigator",
        }
    }

    pub fn meta(&self) -> &NodeMeta {
        match self {
            UiNode::Tabs(n) => &n.meta,
            UiNode::Timeline(n) => &n.meta,
            UiNode::Checklist(n) => &n.meta,
            UiNode::DataTable(n) => &n.meta,
            UiNode::Diagram(n) => &n.meta,
            UiNode::Callout(n) => &n.meta,
            UiNode::RiskPanel(n) => &n.meta,
            UiNode::DependencyGraph(n) => &n.meta,
            UiNode::LogTimeline(n) => &n.meta,
            UiNode::CommitGraph(n) => &n.meta,
            UiNode::Glossary(n) => &n.meta,
            UiNode::StepNavigator(n) => &n.meta,
        }
    }
}
