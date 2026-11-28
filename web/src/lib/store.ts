// Tiny external store (useSyncExternalStore) for the whole web UI.
//
// - `prefs` are persisted to localStorage under one key; failures to read or
//   write storage never break rendering (private mode, quota, disabled).
// - Everything else is session state.
import { useSyncExternalStore } from "react";
import type { HeadingInfo } from "./body";
import type {
  AdaptiveLayout,
  DocPayload,
  ModelsInfo,
  SelectState,
  ThemePref,
  TreeResponse,
} from "./types";

export const STORAGE_KEY = "mdpeek.ui.v1";

export interface DocPrefs {
  pinned: string[];
  hidden: string[];
  /** Reader-side ticks for checklist / action items, keyed by widget + item text.
   *  Kept in this browser only; the Markdown file is never modified. */
  marks?: Record<string, boolean>;
  /** Current step per StepNavigator widget. */
  steps?: Record<string, number>;
  /** Table / checklist aids the reader switched back to the original block (#63). */
  original?: string[];
}
export interface Prefs {
  /** Reading aids on or off; `null` follows `[genui] default` in the config. */
  aids: boolean | null;
  theme: ThemePref;
  railSize: number;
  railCollapsed: boolean;
  /** Outline sidebar on the right: hidden, and docked vs floating over the text. */
  outlineCollapsed: boolean;
  outlineSize: number;
  outlineFloating: boolean;
  /** Floating outline offset from the document column's top-right corner. */
  outlineFloatPos: { right: number; top: number };
  autoscroll: boolean;
  /** Where pinned GenUI views go, outside the scrolling text: a floating
   * panel over it (default), or docked above it or to its left. */
  pinSide: PinSide;
  /** Floating pinned panel offset from the document column's top-left corner. */
  pinFloatPos: { left: number; top: number };
  docs: Record<string, DocPrefs>;
}
export type PinSide = "float" | "top" | "left";

export const DEFAULT_PREFS: Prefs = {
  aids: null,
  theme: "system",
  railSize: 256,
  railCollapsed: false,
  outlineCollapsed: false,
  outlineSize: 248,
  outlineFloating: false,
  outlineFloatPos: { right: 16, top: 16 },
  autoscroll: false,
  pinSide: "float",
  pinFloatPos: { left: 16, top: 16 },
  docs: {},
};

/** `theme = "dark"` in the mdpeek config makes dark the default; else follow the OS. */
function defaultTheme(): Prefs["theme"] {
  return document.documentElement.dataset.defaultTheme === "dark" ? "dark" : "system";
}

function loadPrefs(): Prefs {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { ...DEFAULT_PREFS, theme: defaultTheme() };
    const p = JSON.parse(raw) as Partial<Prefs> & Record<string, unknown>;
    // The Read / Adaptive switch became the reading-aids toggle.
    if (p.aids === undefined && typeof p.mode === "string") p.aids = p.mode === "read" ? false : null;
    // Settings of removed features (side pane, worktree grouping, modes) are dropped.
    for (const k of ["widgetsSize", "widgetsCollapsed", "groupBy", "railTab", "mode"]) delete p[k];
    const out = { ...DEFAULT_PREFS, theme: defaultTheme(), ...p };
    if (out.aids !== null && typeof out.aids !== "boolean") out.aids = null;
    if (!["light", "dark", "system"].includes(out.theme)) out.theme = "system";
    if (typeof out.docs !== "object" || !out.docs) out.docs = {};
    if (!["float", "top", "left"].includes(out.pinSide)) out.pinSide = "float";
    const pp = out.pinFloatPos as { left?: unknown; top?: unknown } | undefined;
    if (!pp || typeof pp.left !== "number" || typeof pp.top !== "number") out.pinFloatPos = { ...DEFAULT_PREFS.pinFloatPos };
    const fp = out.outlineFloatPos as { right?: unknown; top?: unknown } | undefined;
    if (!fp || typeof fp.right !== "number" || typeof fp.top !== "number") out.outlineFloatPos = { ...DEFAULT_PREFS.outlineFloatPos };
    return out;
  } catch {
    return { ...DEFAULT_PREFS };
  }
}

function savePrefs(p: Prefs) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(p));
  } catch {
    /* storage unavailable: keep working in memory */
  }
}

export interface State {
  prefs: Prefs;
  doc: DocPayload;
  /** Bumps on every body change (initial, live update, file switch). */
  docVersion: number;
  /** Indices (into the new body) of blocks changed by the last live update. */
  changed: number[];
  layout: AdaptiveLayout | null;
  /** How the current layout arrived: fetched (`http`) or pushed by a run (`ws`). */
  layoutFrom: "http" | "ws";
  /** Highest revision the server announced for the current body. */
  expectedRevision: number;
  select: SelectState;
  selectError: string | null;
  /** Models from the config and the one in use (`GET /api/models`). */
  models: ModelsInfo | null;
  toasts: Toast[];
  tree: TreeResponse | null;
  headings: HeadingInfo[];
  /** Source line of the heading of the section currently in view. */
  activeLine: number | null;
  paletteOpen: boolean;
  shortcutsOpen: boolean;
  guidedOpen: boolean;
  sheet: null | "files" | "outline";
  /** Polite announcement for screen readers (selector / mode changes). */
  announce: string;
  connected: boolean;
}

export interface Toast {
  id: string;
  kind: "error" | "info";
  title: string;
  /** Lines of detail (config problems, the model's error). */
  body: string[];
  action?: { label: string; run: () => void };
}

type Listener = () => void;
const listeners = new Set<Listener>();
let state: State;

export function initStore(doc: DocPayload) {
  state = {
    prefs: loadPrefs(),
    doc,
    docVersion: 1,
    changed: [],
    layout: null,
    layoutFrom: "http",
    expectedRevision: doc.revision,
    select: "selecting",
    selectError: null,
    models: null,
    toasts: [],
    tree: null,
    activeLine: null,
    headings: [],
    paletteOpen: false,
    shortcutsOpen: false,
    guidedOpen: false,
    sheet: null,
    announce: "",
    connected: false,
  };
}

export function getState(): State {
  return state;
}

export function setState(patch: Partial<State> | ((s: State) => Partial<State>)) {
  const next = typeof patch === "function" ? patch(state) : patch;
  state = { ...state, ...next };
  if (next.prefs) savePrefs(state.prefs);
  listeners.forEach((l) => l());
}

export function setPrefs(patch: Partial<Prefs>) {
  setState((s) => ({ prefs: { ...s.prefs, ...patch } }));
}

export function subscribe(l: Listener) {
  listeners.add(l);
  return () => listeners.delete(l);
}

export function useStore<T>(sel: (s: State) => T): T {
  return useSyncExternalStore(subscribe, () => sel(state));
}

const NO_DOC_PREFS: DocPrefs = Object.freeze({ pinned: [], hidden: [] }) as DocPrefs;
export function docPrefs(s: State): DocPrefs {
  return s.prefs.docs[s.doc.path] ?? NO_DOC_PREFS;
}

export function updateDocPrefs(fn: (d: DocPrefs) => DocPrefs) {
  setState((s) => {
    const cur = s.prefs.docs[s.doc.path] ?? { pinned: [], hidden: [] };
    return { prefs: { ...s.prefs, docs: { ...s.prefs.docs, [s.doc.path]: fn(cur) } } };
  });
}

export function announce(text: string) {
  // Reset first so repeating the same message is still announced.
  setState({ announce: "" });
  setTimeout(() => setState({ announce: text }), 30);
}

/** Reading aids on? The reader's choice, else `[genui] default`. */
export function aidsOn(s: State): boolean {
  return s.prefs.aids ?? s.models?.aids.defaultOn ?? true;
}
