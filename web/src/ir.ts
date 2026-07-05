// UI IR wire-format types (design doc §4.1).
//
// This mirrors the Rust source of truth in `src/ir/node.rs`. The design calls
// for auto-generating this file via `ts-rs`; until the workspace split lands it
// is hand-maintained and kept in lockstep with the Rust `#[serde]` layout
// (discriminated union on `kind`, `NodeMeta` flattened onto every node).

export interface SourceRange {
  start_line: number;
  start_column: number;
  end_line: number;
  end_column: number;
}

export type Origin = "rules" | "llm";

export type Visibility = "always" | { reveal_after_line: number };

export type Severity = "info" | "warning" | "error";

export type Confidence = "low" | "medium" | "high";

export type ColumnType = "text" | "number" | "status" | "link" | "code";

export interface Column {
  key: string;
  label: string;
  type?: ColumnType;
}

export interface Quantity {
  value: number;
  unit?: string;
  min?: number;
  max?: number;
  nominal?: number;
  scalable?: boolean;
}

// Flattened NodeMeta fields present on every node.
export interface NodeMeta {
  sourceRange?: SourceRange;
  confidence?: number;
  origin?: Origin;
  visibility?: Visibility;
  lowConfidence?: boolean;
}

export interface ChecklistItem {
  title: string;
  checked: boolean;
  category?: string;
  sourceRange?: SourceRange;
}

export interface TimelineEvent {
  title: string;
  timestamp?: string;
  description?: string;
  sourceRange?: SourceRange;
}

export interface RiskItem {
  title: string;
  severity: Severity;
  note?: string;
  likelihood?: Severity;
  mitigation?: string;
  confidence?: Confidence;
  sourceRange?: SourceRange;
}

export interface Assumption {
  statement: string;
  impactIfFalse?: string;
  confidence?: Confidence;
  sourceRange?: SourceRange;
}

// --- reading lens item types (design §8) ---
export interface OutlineItemT {
  title: string;
  reason?: string;
  sourceRange?: SourceRange;
}
export interface OutlineGroup {
  label: string;
  description?: string;
  items: OutlineItemT[];
}
export interface SummaryCard {
  title: string;
  summary: string;
  keyPoints?: string[];
  confidence?: Confidence;
  sourceRange?: SourceRange;
}
export type DecisionStatus = "decided" | "proposed" | "rejected" | "superseded";
export interface DecisionT {
  title: string;
  decision: string;
  alternatives?: string[];
  reason?: string;
  impact?: string;
  status: DecisionStatus;
  confidence?: Confidence;
  sourceRange?: SourceRange;
}
export type ActionStatus = "todo" | "doing" | "done" | "blocked" | "unknown";
export interface ActionItemT {
  task: string;
  assignee?: string;
  dueDate?: string;
  status: ActionStatus;
  confidence?: Confidence;
  sourceRange?: SourceRange;
}
export interface OpenQuestionT {
  question: string;
  context?: string;
  severity: Severity;
  confidence?: Confidence;
  sourceRange?: SourceRange;
}

export interface ApiEndpoint {
  method: string;
  path: string;
  description?: string;
}

export interface GraphNode {
  id: string;
  label: string;
}
export interface GraphEdge {
  from: string;
  to: string;
  label?: string;
}

export interface LogEntryT {
  severity: Severity;
  message: string;
  timestamp?: string;
}

export interface Commit {
  hash: string;
  subject: string;
  kind?: string;
}

export interface GlossaryTerm {
  term: string;
  aliases?: string[];
  definition?: string;
  inferredDefinition?: string;
  confidence?: Confidence;
  sourceRange?: SourceRange;
}

export interface CharacterT {
  name: string;
  summary?: string;
  firstSeen?: SourceRange;
}

export interface StepT {
  title: string;
  body?: string;
  duration?: string;
  prerequisites?: string[];
  sourceRange?: SourceRange;
}

export interface ScalableRow {
  label: string;
  quantity: Quantity;
}

export interface ObligationT {
  party: string;
  duty: string;
  sourceRange?: SourceRange;
}

// Discriminated union — `kind` selects the component in the registry.
export type UiNode = NodeMeta &
  (
    | { kind: "Tabs"; tabs: { title: string; children: UiNode[] }[] }
    | { kind: "Timeline"; events: TimelineEvent[] }
    | { kind: "Checklist"; items: ChecklistItem[] }
    | { kind: "DataTable"; columns: Column[]; rows: Record<string, unknown>[] }
    | { kind: "Diagram"; format: "mermaid"; code: string; title?: string }
    | { kind: "Callout"; severity: Severity; title?: string; body: string }
    | { kind: "RiskPanel"; risks: RiskItem[]; assumptions?: Assumption[] }
    | { kind: "SemanticOutline"; groups: OutlineGroup[] }
    | { kind: "SummaryCards"; cards: SummaryCard[] }
    | { kind: "DecisionLog"; decisions: DecisionT[] }
    | { kind: "ActionItems"; items: ActionItemT[] }
    | { kind: "OpenQuestions"; questions: OpenQuestionT[] }
    | { kind: "ApiExplorer"; endpoints: ApiEndpoint[] }
    | { kind: "ConfigViewer"; format: "json" | "yaml" | "toml" | "env"; content: string; title?: string }
    | { kind: "DependencyGraph"; nodes: GraphNode[]; edges: GraphEdge[] }
    | { kind: "LogTimeline"; entries: LogEntryT[] }
    | { kind: "CommitGraph"; commits: Commit[] }
    | { kind: "Glossary"; terms: GlossaryTerm[] }
    | { kind: "CharacterRoster"; characters: CharacterT[] }
    | { kind: "StepNavigator"; steps: StepT[] }
    | { kind: "ToleranceMeter"; label: string; quantity: Quantity }
    | { kind: "ScalableTable"; baseScale: number; columns: Column[]; rows: ScalableRow[] }
    | { kind: "ObligationMatrix"; parties: string[]; obligations: ObligationT[] }
  );

export type UiNodeKind = UiNode["kind"];
