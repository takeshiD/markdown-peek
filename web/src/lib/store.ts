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
  Mode,
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
}
export interface Prefs {
  mode: Mode;
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
  docs: Record<string, DocPrefs>;
}

export const DEFAULT_PREFS: Prefs = {
  mode: "adaptive",
  theme: "system",
  railSize: 256,
  railCollapsed: false,
  outlineCollapsed: false,
  outlineSize: 248,
  outlineFloating: false,
  outlineFloatPos: { right: 16, top: 16 },
  autoscroll: false,
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
    // Settings of removed features (side pane, worktree grouping) are dropped.
    for (const k of ["widgetsSize", "widgetsCollapsed", "groupBy", "railTab"]) delete p[k];
    const out = { ...DEFAULT_PREFS, theme: defaultTheme(), ...p };
    // "split" (aids in a side pane) was folded into adaptive (aids in the text).
    if (!["read", "adaptive"].includes(out.mode)) out.mode = "adaptive";
    if (!["light", "dark", "system"].includes(out.theme)) out.theme = "system";
    if (typeof out.docs !== "object" || !out.docs) out.docs = {};
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
  /** Highest revision the server announced for the current body. */
  expectedRevision: number;
  select: SelectState;
  selectError: string | null;
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
    expectedRevision: doc.revision,
    select: "selecting",
    selectError: null,
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
