// Layer 3.5 ドメインノードの wire format 型 (AGENTS.md §4.1)。
//
// ⚠ 統合時: 正本は Rust 型。Layer 3 の `ts-rs` 生成に合流させ、この手書き型は
// `web/src/ir.ts` の生成物に置き換える (§1.1)。現状は Layer 3 が未マージのため、
// crates/mdpeek-domain/tests/wire_format.rs が固定する JSON と 1:1 一致させた
// 手書き定義を暫定で置く。

export interface SourceRange {
  start_line: number;
  start_column: number;
  end_line: number;
  end_column: number;
}

export type Origin = "rules" | "llm";

export type Visibility =
  | "always"
  | { until_read: { reveal_after_line: number } };

// NodeMeta は各ノードに flatten されて入る (論点 D)。
export interface NodeMeta {
  source_range?: SourceRange;
  confidence?: number;
  origin?: Origin;
  visibility?: Visibility;
}

export interface Quantity {
  value: number;
  unit?: string;
  min?: number;
  max?: number;
  nominal?: number;
  scalable?: boolean;
}

// --- ドメインプリミティブ (§5.1 domainRegistry の外側層) ---

export interface GlossaryEntry {
  term: string;
  definition: string;
  first_occurrence?: SourceRange;
}
export interface GlossaryNode extends NodeMeta {
  kind: "Glossary";
  entries: GlossaryEntry[];
}

export interface Character {
  name: string;
  summary?: string;
  aliases?: string[];
  first_occurrence?: SourceRange;
}
export interface CharacterRosterNode extends NodeMeta {
  kind: "CharacterRoster";
  characters: Character[];
}

export interface Step {
  index: number;
  title: string;
  detail?: string;
  duration?: Quantity;
  caution?: string;
  rollback?: string;
  source_range?: SourceRange;
}
export interface StepNavigatorNode extends NodeMeta {
  kind: "StepNavigator";
  steps: Step[];
  prerequisites?: string[];
}

export interface ToleranceMeter {
  label: string;
  quantity: Quantity;
}
export interface ToleranceMeterNode extends NodeMeta {
  kind: "ToleranceMeter";
  meters: ToleranceMeter[];
}

export interface Column {
  key: string;
  label: string;
}
// untagged: 数量セルは Quantity、テキストセルは string。
export type Cell = Quantity | string;
export interface ScalableRow {
  cells: Cell[];
}
export interface ScalableTableNode extends NodeMeta {
  kind: "ScalableTable";
  columns: Column[];
  rows: ScalableRow[];
  base_scale?: Quantity;
}

export type ObligationKind = "obligation" | "right";
export interface Obligation {
  party: string;
  kind: ObligationKind;
  description: string;
  source_range?: SourceRange;
}
export interface ObligationMatrixNode extends NodeMeta {
  kind: "ObligationMatrix";
  parties: string[];
  obligations: Obligation[];
}

export type DomainNode =
  | GlossaryNode
  | CharacterRosterNode
  | StepNavigatorNode
  | ToleranceMeterNode
  | ScalableTableNode
  | ObligationMatrixNode;

// Cell の判別ヘルパ (untagged なので構造で見分ける)。
export function isAmount(cell: Cell): cell is Quantity {
  return typeof cell === "object" && cell !== null && "value" in cell;
}

// 現在の既読位置でノードが可視か (Rust visibility::is_visible と同じ規則)。
export function isVisible(node: NodeMeta, readLine: number | null): boolean {
  const v = node.visibility ?? "always";
  if (v === "always") return true;
  return readLine !== null && readLine >= v.until_read.reveal_after_line;
}
