// Wire types for the server API (mdpeek-server) used by the web UI.
import type { SourceRange, UiNode } from "../ir";

/** `GET /api/doc`, the boot JSON and the WS `update` message. */
export interface DocPayload {
  path: string;
  title: string;
  html: string;
  frontmatter: string;
  /** [start_line, end_line] of each top-level body block, in DOM order. */
  blocks: [number, number][];
  /** Adaptive revision whose layout belongs to this body. */
  revision: number;
  error?: string;
}

export interface FileEntry { path: string; rel: string }
export interface Group { name: string; branch?: string | null; root: string; files: FileEntry[] }
export interface TreeResponse { tree: { groups: Group[]; git: boolean }; active: string }

/** AdaptiveLayout v1 (design §3.6, crates/mdpeek-gui/src/select). */
export interface Anchor { block_id: string; heading: string; range: SourceRange }
export interface Why {
  by: string;
  candidate: string;
  confidence: number;
  helps: number;
  reason_code: string;
  alternatives: [string, number][];
  cached: boolean;
}
export interface Placement { scope: "section" | "document"; anchor: Anchor; node: UiNode; why: Why }
export interface Skipped {
  scope: string;
  anchor: Anchor;
  reason_code: string;
  by: string;
  helps?: number;
  confidence?: number;
  fallback?: string;
}
export interface Selector { provider: string; requested: string; model: string; fallback: string | null; requests: number }
export interface AdaptiveLayout {
  version: number;
  revision: number;
  doc_hash: string;
  selector: Selector;
  doc_type: { value: string; confidence: number; by: string };
  placements: Placement[];
  skipped: Skipped[];
  cache?: { hits: number; misses: number };
  timing_ms: { select: number; total: number };
}

export type Mode = "read" | "split" | "adaptive";
export type ThemePref = "light" | "dark" | "system";
export type DiffMode = "source" | "rendered";
export type DiffLayout = "unified" | "split";
export interface DiffState { a: string; b: string; mode: DiffMode; layout: DiffLayout }

/** Selector status shown in the top bar. */
export type SelectState = "idle" | "selecting" | "ready" | "error";

/** Stable identity of a widget within one document (survives edits). */
export function widgetKey(p: Placement): string {
  return `${p.scope}:${p.node.kind}:${p.anchor.heading}`;
}

const KIND_LABEL: Record<string, string> = {
  StepNavigator: "Steps",
  Checklist: "Checklist",
  DataTable: "Table",
  ConfigViewer: "Commands and config",
  Timeline: "Timeline",
  Callout: "Callout",
  SemanticOutline: "Outline by role",
  RiskPanel: "Risks",
  OpenQuestions: "Open questions",
  ActionItems: "Action items",
  DecisionLog: "Decisions",
  Glossary: "Glossary",
};
export function kindLabel(kind: string): string {
  return KIND_LABEL[kind] ?? kind.replace(/([a-z])([A-Z])/g, "$1 $2");
}

const FALLBACK_TEXT: Record<string, string> = {
  timeout: "Jev did not answer in time",
  offline: "Jev could not be reached",
  rate_limited: "Jev rate limit reached",
  invalid_answer: "Jev returned an answer that could not be used",
  disabled: "Network use is off (allow_network = false)",
  missing_key: "No TYPESAFE_API_KEY was set",
  not_built: "This build has no Jev support",
  fake_script_error: "The fake selector script could not be read",
};
export function fallbackText(code: string): string {
  return FALLBACK_TEXT[code] ?? code.replace(/_/g, " ");
}
