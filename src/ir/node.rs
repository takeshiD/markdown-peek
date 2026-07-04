//! UI IR node types — the canonical (source-of-truth) wire format between the
//! Rust core and the web / TUI renderers (design doc §4.1).
//!
//! `#[serde(tag = "kind")]` gives a TypeScript-style discriminated union so the
//! same JSON is consumed by the Preact registry (`web/src/registry.ts`) keyed on
//! `node.kind`. `NodeMeta` is flattened into every node (design §4.1 論点 D:
//! flatten chosen) so `sourceRange` / `confidence` / `origin` / `visibility`
//! ride along uniformly.

use serde::{Deserialize, Serialize};

use super::range::SourceRange;

/// Where a node came from. Renderers badge `Llm` nodes as "generated / verify".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    #[default]
    Rules,
    Llm,
}

/// Reading-position-aware visibility (design §9.3). Novels etc. hide content
/// past the reader's current position to avoid spoilers.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    #[default]
    Always,
    /// Only revealed once the reader has read past `reveal_after_line`.
    UntilRead { reveal_after_line: u32 },
}

/// Common metadata carried by every node (design §4.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NodeMeta {
    #[serde(rename = "sourceRange", skip_serializing_if = "Option::is_none")]
    pub source_range: Option<SourceRange>,
    /// 0.0–1.0, present for LLM-generated nodes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub origin: Origin,
    #[serde(default)]
    pub visibility: Visibility,
    /// Set by the validator when `confidence` is below threshold (design §3.5).
    /// The renderer shows an explicit "low confidence" badge.
    #[serde(rename = "lowConfidence", default, skip_serializing_if = "is_false")]
    pub low_confidence: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Numbers made *operable* rather than just readable (design §9.3): tolerance
/// meters, ingredient scaling and charts all consume this.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quantity {
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nominal: Option<f64>,
    #[serde(default)]
    pub scalable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub key: String,
    pub label: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub col_type: Option<ColumnType>,
}

/// A single generated UI node. Renderers dispatch on `kind` via the registry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum UiNode {
    // --- core registry (design §5.1, always available) ---
    Tabs(TabsNode),
    Timeline(TimelineNode),
    Checklist(ChecklistNode),
    DataTable(DataTableNode),
    Diagram(DiagramNode),
    Callout(CalloutNode),
    RiskPanel(RiskPanelNode),
    ApiExplorer(ApiExplorerNode),
    ConfigViewer(ConfigViewerNode),
    DependencyGraph(DependencyGraphNode),
    LogTimeline(LogTimelineNode),
    CommitGraph(CommitGraphNode),

    // --- domain primitives (design §5.1 outer layer / §9.3) ---
    Glossary(GlossaryNode),
    CharacterRoster(CharacterRosterNode),
    StepNavigator(StepNavigatorNode),
    ToleranceMeter(ToleranceMeterNode),
    ScalableTable(ScalableTableNode),
    ObligationMatrix(ObligationMatrixNode),
}

impl UiNode {
    /// The registry key / discriminant string. Matches the serde `tag` value.
    pub fn kind(&self) -> &'static str {
        match self {
            UiNode::Tabs(_) => "Tabs",
            UiNode::Timeline(_) => "Timeline",
            UiNode::Checklist(_) => "Checklist",
            UiNode::DataTable(_) => "DataTable",
            UiNode::Diagram(_) => "Diagram",
            UiNode::Callout(_) => "Callout",
            UiNode::RiskPanel(_) => "RiskPanel",
            UiNode::ApiExplorer(_) => "ApiExplorer",
            UiNode::ConfigViewer(_) => "ConfigViewer",
            UiNode::DependencyGraph(_) => "DependencyGraph",
            UiNode::LogTimeline(_) => "LogTimeline",
            UiNode::CommitGraph(_) => "CommitGraph",
            UiNode::Glossary(_) => "Glossary",
            UiNode::CharacterRoster(_) => "CharacterRoster",
            UiNode::StepNavigator(_) => "StepNavigator",
            UiNode::ToleranceMeter(_) => "ToleranceMeter",
            UiNode::ScalableTable(_) => "ScalableTable",
            UiNode::ObligationMatrix(_) => "ObligationMatrix",
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
            UiNode::ApiExplorer(n) => &n.meta,
            UiNode::ConfigViewer(n) => &n.meta,
            UiNode::DependencyGraph(n) => &n.meta,
            UiNode::LogTimeline(n) => &n.meta,
            UiNode::CommitGraph(n) => &n.meta,
            UiNode::Glossary(n) => &n.meta,
            UiNode::CharacterRoster(n) => &n.meta,
            UiNode::StepNavigator(n) => &n.meta,
            UiNode::ToleranceMeter(n) => &n.meta,
            UiNode::ScalableTable(n) => &n.meta,
            UiNode::ObligationMatrix(n) => &n.meta,
        }
    }

    pub fn meta_mut(&mut self) -> &mut NodeMeta {
        match self {
            UiNode::Tabs(n) => &mut n.meta,
            UiNode::Timeline(n) => &mut n.meta,
            UiNode::Checklist(n) => &mut n.meta,
            UiNode::DataTable(n) => &mut n.meta,
            UiNode::Diagram(n) => &mut n.meta,
            UiNode::Callout(n) => &mut n.meta,
            UiNode::RiskPanel(n) => &mut n.meta,
            UiNode::ApiExplorer(n) => &mut n.meta,
            UiNode::ConfigViewer(n) => &mut n.meta,
            UiNode::DependencyGraph(n) => &mut n.meta,
            UiNode::LogTimeline(n) => &mut n.meta,
            UiNode::CommitGraph(n) => &mut n.meta,
            UiNode::Glossary(n) => &mut n.meta,
            UiNode::CharacterRoster(n) => &mut n.meta,
            UiNode::StepNavigator(n) => &mut n.meta,
            UiNode::ToleranceMeter(n) => &mut n.meta,
            UiNode::ScalableTable(n) => &mut n.meta,
            UiNode::ObligationMatrix(n) => &mut n.meta,
        }
    }

}

// ---------------------------------------------------------------------------
// Core registry nodes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabsNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub tabs: Vec<Tab>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tab {
    pub title: String,
    pub children: Vec<UiNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimelineNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub events: Vec<TimelineEvent>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "sourceRange", skip_serializing_if = "Option::is_none")]
    pub source_range: Option<SourceRange>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChecklistNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub items: Vec<ChecklistItem>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChecklistItem {
    pub title: String,
    pub checked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(rename = "sourceRange", skip_serializing_if = "Option::is_none")]
    pub source_range: Option<SourceRange>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataTableNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub columns: Vec<Column>,
    pub rows: Vec<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagramFormat {
    Mermaid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagramNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub format: DiagramFormat,
    /// Diagram source (e.g. mermaid). Rendered client-side in a sandbox (§8).
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalloutNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub severity: Severity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskPanelNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub risks: Vec<RiskItem>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskItem {
    pub title: String,
    pub severity: Severity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(rename = "sourceRange", skip_serializing_if = "Option::is_none")]
    pub source_range: Option<SourceRange>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiExplorerNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub endpoints: Vec<ApiEndpoint>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiEndpoint {
    pub method: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigFormat {
    Json,
    Yaml,
    Toml,
    Env,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigViewerNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub format: ConfigFormat,
    /// Raw config text; renderer escapes into `<pre>` (never eval'd, §8).
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DependencyGraphNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogTimelineNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub entries: Vec<LogEntry>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    pub severity: Severity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommitGraphNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub commits: Vec<Commit>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Commit {
    pub hash: String,
    pub subject: String,
    /// rules-classified intent: feat | fix | refactor | docs | ...
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

// ---------------------------------------------------------------------------
// Domain primitives
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlossaryNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub terms: Vec<GlossaryTerm>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlossaryTerm {
    pub term: String,
    pub definition: String,
    #[serde(rename = "sourceRange", skip_serializing_if = "Option::is_none")]
    pub source_range: Option<SourceRange>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CharacterRosterNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub characters: Vec<Character>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Character {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(rename = "firstSeen", skip_serializing_if = "Option::is_none")]
    pub first_seen: Option<SourceRange>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepNavigatorNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub steps: Vec<Step>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prerequisites: Vec<String>,
    #[serde(rename = "sourceRange", skip_serializing_if = "Option::is_none")]
    pub source_range: Option<SourceRange>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToleranceMeterNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub label: String,
    pub quantity: Quantity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScalableTableNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    /// Base quantity the amounts below are expressed for (e.g. servings=2).
    #[serde(rename = "baseScale")]
    pub base_scale: f64,
    pub columns: Vec<Column>,
    /// Each row: label + a scalable [`Quantity`].
    pub rows: Vec<ScalableRow>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScalableRow {
    pub label: String,
    pub quantity: Quantity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObligationMatrixNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub parties: Vec<String>,
    pub obligations: Vec<Obligation>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Obligation {
    pub party: String,
    pub duty: String,
    #[serde(rename = "sourceRange", skip_serializing_if = "Option::is_none")]
    pub source_range: Option<SourceRange>,
}
