// Server I/O and user actions. Components call these; they update the store.
import { announce, getState, setPrefs, setState, updateDocPrefs } from "./store";
import type { AdaptiveLayout, DiffState, DocPayload, Mode, Placement, ThemePref, TreeResponse } from "./types";
import { fallbackText, kindLabel, widgetKey } from "./types";

const MODE_LABEL: Record<Mode, string> = { read: "Read", split: "Split", adaptive: "Adaptive" };

export function setMode(mode: Mode) {
  if (getState().prefs.mode === mode) return;
  setPrefs({ mode });
  announce(`${MODE_LABEL[mode]} mode`);
}

export function setTheme(theme: ThemePref) {
  setPrefs({ theme });
  announce(`Theme: ${theme}`);
}

export function applyTheme() {
  const pref = getState().prefs.theme;
  const dark = pref === "dark" || (pref === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.classList.toggle("dark", dark);
}

function describeLayout(l: AdaptiveLayout): string {
  const n = l.placements.length;
  const who = l.selector.provider;
  const fb = l.selector.fallback ? ` Fallback: ${fallbackText(l.selector.fallback)}.` : "";
  return `Reading aids updated: ${n} ${n === 1 ? "widget" : "widgets"} chosen by ${who}.${fb}`;
}

/** Accept a layout unless it belongs to an older revision than the body shown. */
export function acceptLayout(layout: AdaptiveLayout, source: "http" | "ws") {
  const s = getState();
  if (layout.revision < s.expectedRevision || (s.layout && layout.revision < s.layout.revision)) {
    console.debug(`mdpeek: dropped stale ${source} layout r${layout.revision} (expecting r${s.expectedRevision})`);
    return;
  }
  setState({ layout, select: "ready", selectError: null, expectedRevision: layout.revision });
  announce(describeLayout(layout));
}

let adaptiveSeq = 0;
/** GET /api/adaptive (initial load and "re-select"). */
export async function fetchAdaptive(reason: "initial" | "reselect" = "initial") {
  const seq = ++adaptiveSeq;
  setState({ select: "selecting" });
  if (reason === "reselect") announce("Selecting reading aids…");
  try {
    const res = await fetch("/api/adaptive", { cache: "no-store" });
    if (!res.ok) {
      const info = (await res.json().catch(() => null)) as { error?: string } | null;
      throw new Error(info?.error ?? `server answered ${res.status}`);
    }
    const layout = (await res.json()) as AdaptiveLayout;
    if (seq !== adaptiveSeq && getState().select !== "selecting") return;
    acceptLayout(layout, "http");
  } catch (e) {
    setState({ select: "error", selectError: `Reading aids could not be loaded (${(e as Error).message}). The document is unaffected.` });
    announce("Reading aids could not be loaded");
  }
}

export async function fetchTree() {
  try {
    const res = await fetch("/api/tree", { cache: "no-store" });
    if (res.ok) setState({ tree: (await res.json()) as TreeResponse });
  } catch {
    /* explorer stays empty */
  }
}

export async function selectFile(path: string) {
  if (getState().diff) setState({ diff: null, diffHtml: "" });
  try {
    const res = await fetch("/api/select", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ path }),
    });
    if (!res.ok) throw new Error(String(res.status));
    // The server broadcasts the new body over the WebSocket; also fetch it so
    // the switch works even while the socket is reconnecting.
    const d = await fetch("/api/doc", { cache: "no-store" });
    if (d.ok) applyDoc((await d.json()) as DocPayload);
    setState((s) => ({ tree: s.tree ? { ...s.tree, active: path } : s.tree, sheet: null }));
  } catch (e) {
    announce(`Could not open ${path}`);
    console.warn("select failed", e);
  }
}

/** Apply a new body (live update or file switch). */
export function applyDoc(doc: DocPayload) {
  const s = getState();
  const switched = doc.path !== s.doc.path;
  if (!switched && doc.html === s.doc.html && doc.frontmatter === s.doc.frontmatter) {
    if (doc.revision > s.expectedRevision) setState({ expectedRevision: doc.revision, select: "selecting" });
    return;
  }
  setState({
    doc,
    docVersion: s.docVersion + 1,
    expectedRevision: Math.max(doc.revision, switched ? 0 : s.expectedRevision),
    select: "selecting",
    ...(switched ? { layout: null, activeLine: null } : {}),
  });
  if (switched) announce(`Opened ${doc.title}`);
}

export function onSocketMessage(raw: string) {
  let msg: { type?: string } & Record<string, unknown>;
  try {
    msg = JSON.parse(raw);
  } catch {
    return;
  }
  if (msg.type === "update") {
    if (getState().diff) return;
    applyDoc(msg as unknown as DocPayload);
  } else if (msg.type === "adaptive") {
    const layout = msg.layout as AdaptiveLayout;
    if (layout && typeof msg.revision === "number") layout.revision = msg.revision;
    // The layout belongs to whatever file is active on the server; only use it
    // when the body we show is that file (the server only selects for it).
    if (!getState().diff) acceptLayout(layout, "ws");
  } else if (msg.type === "diff-update") {
    if (getState().diff) setState({ diffHtml: String(msg.html ?? "") });
  }
}

export function connectSocket() {
  const url = `${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/ws`;
  let ws: WebSocket;
  const open = () => {
    ws = new WebSocket(url);
    ws.onopen = () => setState({ connected: true });
    ws.onmessage = (e) => onSocketMessage(String(e.data));
    ws.onclose = () => {
      setState({ connected: false });
      window.setTimeout(open, 3000);
    };
  };
  open();
}

// ---- diff (#15) -----------------------------------------------------------

export async function requestDiff(next: DiffState) {
  setState({ diff: next, compareA: null });
  try {
    const res = await fetch("/api/diff", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(next),
    });
    if (!res.ok) throw new Error(String(res.status));
    const d = (await res.json()) as { html: string };
    setState({ diffHtml: d.html });
    announce("Comparison open");
  } catch {
    setState({ diffHtml: "<p>Could not compare these files.</p>" });
  }
}

export function compareWith(path: string) {
  const a = getState().compareA;
  if (!a) {
    setState({ compareA: path });
    announce("Picked for comparison. Choose a second file.");
  } else if (a === path) {
    setState({ compareA: null });
  } else {
    void requestDiff({ a, b: path, mode: "source", layout: "unified" });
    setState({ sheet: null });
  }
}

export function closeDiff() {
  setState({ diff: null, diffHtml: "" });
  void selectFile(getState().doc.path);
}

// ---- widgets --------------------------------------------------------------

export function togglePin(p: Placement) {
  const k = widgetKey(p);
  let pinned = false;
  updateDocPrefs((d) => {
    pinned = !d.pinned.includes(k);
    return { ...d, pinned: pinned ? [...d.pinned, k] : d.pinned.filter((x) => x !== k) };
  });
  announce(`${kindLabel(p.node.kind)} ${pinned ? "pinned" : "unpinned"}`);
}

export function hideWidget(p: Placement) {
  const k = widgetKey(p);
  updateDocPrefs((d) => ({ pinned: d.pinned.filter((x) => x !== k), hidden: d.hidden.includes(k) ? d.hidden : [...d.hidden, k] }));
  announce(`${kindLabel(p.node.kind)} hidden. Restore it from Hidden widgets.`);
}

export function showWidget(key: string) {
  updateDocPrefs((d) => ({ ...d, hidden: d.hidden.filter((x) => x !== key) }));
  announce("Widget shown again");
}
