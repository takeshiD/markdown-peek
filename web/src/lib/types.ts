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

/** AdaptiveLayout v2 (design §3.6, crates/mdpeek-gui/src/select). */
export interface Anchor { block_id: string; heading: string; range: SourceRange }
export interface Why {
  /** Model entry name. */
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
}
export type ModelKind = "decision" | "llm";
export interface Selector {
  /** `[models.<name>]` entry that answers. */
  name: string;
  kind: ModelKind;
  model: string;
  error: string | null;
  message: string | null;
  requests: number;
}
export type JobState = "idle" | "running" | "stopped" | "done" | "error";
export interface Progress { state: JobState; done: number; total: number }
export interface AdaptiveLayout {
  version: number;
  revision: number;
  doc_hash: string;
  selector: Selector;
  progress: Progress;
  doc_type: { value: string; confidence: number; by: string };
  placements: Placement[];
  skipped: Skipped[];
  cache?: { hits: number; misses: number };
  timing_ms: { select: number; total: number };
}

/** `GET /api/models` and the WS `models` message. */
export interface ModelInfo { name: string; kind: ModelKind; api: string; model: string; problem: string | null }
export interface ModelsInfo {
  default: string;
  active: string;
  models: ModelInfo[];
  configErrors: string[];
  configPath: string | null;
  aids: { defaultOn: boolean; autoGenerate: boolean };
  cacheDir: string | null;
}

export type ThemePref = "light" | "dark" | "system";

/** Request state of `GET /api/adaptive` (the run itself is `layout.progress`). */
export type SelectState = "idle" | "selecting" | "ready" | "error";

/** Stable identity of a widget within one document (survives edits). */
export function widgetKey(p: Placement): string {
  return `${p.scope}:${p.node.kind}:${p.anchor.heading}`;
}

const KIND_LABEL: Record<string, string> = {
  StepNavigator: "Steps",
  Checklist: "Checklist",
  DataTable: "Table",
  Chart: "Chart",
  NodeGraph: "Flow",
  Comparison: "Comparison",
  DirectoryTree: "Directory tree",
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

/** Error codes that mean "the settings are wrong", not "try again later". */
export const CONFIG_ERRORS = new Set(["config_invalid", "missing_key", "unauthorized", "payment_required", "not_found", "not_built", "cli_missing", "fake_script_error"]);

/** A few words for the status chip (the popover has the full sentence). */
const ERROR_SHORT: Record<string, string> = {
  config_invalid: "settings invalid",
  missing_key: "no key",
  unauthorized: "key rejected",
  not_found: "not found",
  payment_required: "no credit",
  timeout: "timed out",
  offline: "offline",
  rate_limited: "rate limited",
  overloaded: "overloaded",
  server_error: "server error",
  invalid_response: "bad answer",
  cli_missing: "CLI missing",
};
export function errorShort(code: string): string {
  return ERROR_SHORT[code] ?? code.replace(/_/g, " ");
}
