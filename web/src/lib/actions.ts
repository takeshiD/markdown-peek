// Server I/O and user actions. Components call these; they update the store.
import { aidsOn, announce, getState, setPrefs, setState, updateDocPrefs, type Toast } from "./store";
import { keepReadingPosition } from "./nav";
import type { AdaptiveLayout, DocPayload, ModelsInfo, Placement, ThemePref, TreeResponse } from "./types";
import { CONFIG_ERRORS, kindLabel, widgetKey } from "./types";

export function setTheme(theme: ThemePref) {
  setPrefs({ theme });
  announce(`Theme: ${theme}`);
}

export function applyTheme() {
  const pref = getState().prefs.theme;
  const dark = pref === "dark" || (pref === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.classList.toggle("dark", dark);
}

// ---- notifications ---------------------------------------------------------

let toastSeq = 0;
/** Show a notification; `key` keeps the same one from piling up. */
export function notify(t: Omit<Toast, "id">, key?: string) {
  const id = key ?? `t${++toastSeq}`;
  setState((s) => ({ toasts: [...s.toasts.filter((x) => x.id !== id), { ...t, id }] }));
  if (t.kind === "info") window.setTimeout(() => dismiss(id), 6000);
}

export function dismiss(id: string) {
  setState((s) => ({ toasts: s.toasts.filter((x) => x.id !== id) }));
}

/** Errors already shown, so a repeated push does not re-open a closed one. */
const shown = new Set<string>();
function notifyOnce(key: string, t: Omit<Toast, "id">) {
  if (shown.has(key)) return;
  shown.add(key);
  notify(t, key);
}

function configWhere(): string {
  const p = getState().models?.configPath;
  return p ? `Fix it in ${p}, or pick another model.` : "Fix the config file, or pick another model.";
}

/** Tell the reader, once, when the settings cannot work. */
function reportConfig(m: ModelsInfo) {
  if (m.configErrors.length) {
    notifyOnce(`config:${m.configErrors.join("|")}`, {
      kind: "error",
      title: "Settings are invalid",
      body: m.configErrors,
    });
  }
  const active = m.models.find((x) => x.name === m.active);
  if (active?.problem && aidsOn(getState())) {
    notifyOnce(`model:${active.name}:${active.problem}`, {
      kind: "error",
      title: `Model “${active.name}” cannot be used`,
      body: [active.problem, configWhere()],
    });
  }
}

// ---- reading aids ------------------------------------------------------------

function describeLayout(l: AdaptiveLayout): string {
  const n = l.placements.length;
  const st = l.progress.state;
  if (st === "running") return `Choosing reading aids: ${l.progress.done} of ${l.progress.total}`;
  if (st === "stopped") return `Stopped: ${n} ${n === 1 ? "aid" : "aids"} so far`;
  if (st === "error") return `Reading aids failed: ${l.selector.message ?? l.selector.error}`;
  return `Reading aids updated: ${n} ${n === 1 ? "aid" : "aids"} chosen by ${l.selector.name}`;
}

/** Accept a layout unless it belongs to an older revision than the body shown. */
export function acceptLayout(layout: AdaptiveLayout, source: "http" | "ws") {
  const s = getState();
  if (layout.revision < s.expectedRevision || (s.layout && layout.revision < s.layout.revision)) {
    console.debug(`mdpeek: dropped stale ${source} layout r${layout.revision} (expecting r${s.expectedRevision})`);
    return;
  }
  const before = s.layout?.progress.state;
  layout = keepWhileChoosing(s.layout, layout);
  setState({ layout, layoutFrom: source, select: "ready", selectError: null, expectedRevision: layout.revision });
  const st = layout.progress.state;
  // Announce starts and ends, not every batch.
  if (st !== "running" || before !== "running") announce(describeLayout(layout));
  if (st === "error" && aidsOn(getState())) {
    const code = layout.selector.error ?? "";
    const config = CONFIG_ERRORS.has(code);
    const msg = layout.selector.message ?? code;
    // Same key as the model-problem notice, so one problem is reported once.
    notifyOnce(config ? `model:${layout.selector.name}:${msg}` : `run:${layout.selector.name}:${code}:${layout.revision}`, {
      kind: "error",
      title: config ? `Model “${layout.selector.name}” settings are invalid` : `Reading aids failed (${layout.selector.name})`,
      body: [layout.selector.message ?? code, ...(config ? [configWhere()] : [])],
      action: config ? undefined : { label: "Try again", run: () => void runAids() },
    });
  }
}

/**
 * While a run is still choosing, a section it has not answered yet keeps the
 * aid it had, so an edit does not make aids blink out and back in.
 */
function keepWhileChoosing(prev: AdaptiveLayout | null, next: AdaptiveLayout): AdaptiveLayout {
  if (!prev || next.progress.state !== "running") return next;
  const pending = new Set(next.skipped.filter((k) => k.reason_code === "pending").map((k) => `${k.scope}:${k.anchor.heading}`));
  const shown = new Set(next.placements.map((p) => `${p.scope}:${p.anchor.heading}`));
  const kept = prev.placements.filter((p) => {
    const k = `${p.scope}:${p.anchor.heading}`;
    return pending.has(k) && !shown.has(k);
  });
  if (!kept.length) return next;
  return { ...next, placements: [...next.placements, ...kept.map((p) => ({ ...p, why: { ...p.why, cached: true } }))] };
}

let adaptiveSeq = 0;
/** GET /api/adaptive: what is known now (never asks a model). */
export async function fetchAdaptive() {
  const seq = ++adaptiveSeq;
  setState({ select: "selecting" });
  try {
    const res = await fetch("/api/adaptive", { cache: "no-store" });
    if (!res.ok) {
      const info = (await res.json().catch(() => null)) as { error?: string } | null;
      throw new Error(info?.error ?? `server answered ${res.status}`);
    }
    const layout = (await res.json()) as AdaptiveLayout;
    if (seq !== adaptiveSeq && getState().select !== "selecting") return;
    acceptLayout(layout, "http");
    return layout;
  } catch (e) {
    setState({ select: "error", selectError: `Reading aids could not be loaded (${(e as Error).message}). The document is unaffected.` });
    announce("Reading aids could not be loaded");
  }
}

async function post(path: string, body?: unknown): Promise<Response> {
  return fetch(path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}

/** Start (or resume) choosing aids for the open file. */
export async function runAids() {
  try {
    await post("/api/adaptive/run");
  } catch {
    notify({ kind: "error", title: "Could not reach mdpeek", body: ["The server did not answer. Is `mdpeek serve` still running?"] });
  }
}

/** Stop the running selection; aids chosen so far stay. */
export async function stopAids() {
  try {
    await post("/api/adaptive/stop");
    announce("Stopped choosing reading aids");
  } catch {
    /* the next push tells the truth */
  }
}

/** The single on/off toggle for reading aids. */
export function setAids(on: boolean) {
  if (aidsOn(getState()) === on && getState().prefs.aids !== null) return;
  keepReadingPosition(() => setPrefs({ aids: on }));
  announce(on ? "Reading aids on" : "Reading aids off");
  if (on) {
    const m = getState().models;
    if (m) reportConfig(m);
    const st = getState().layout?.progress.state;
    if (st !== "done") void runAids();
  } else if (getState().layout?.progress.state === "running") {
    void stopAids();
  }
}

export function toggleAids() {
  setAids(!aidsOn(getState()));
}

function acceptModels(m: ModelsInfo) {
  setState({ models: m });
}

/** GET /api/models. */
export async function fetchModels(): Promise<ModelsInfo | null> {
  try {
    const res = await fetch("/api/models", { cache: "no-store" });
    if (!res.ok) return null;
    const m = (await res.json()) as ModelsInfo;
    acceptModels(m);
    reportConfig(m);
    return m;
  } catch {
    return null;
  }
}

/** Use `name` for this session (the default stays as it is). */
export async function setActiveModel(name: string) {
  const m = getState().models;
  if (!m || m.active === name) return;
  // Said before the request: the run's own announcements follow it.
  announce(`Model ${name} for this session${name === m.default ? "" : `; the default is still ${m.default}`}`);
  const res = await post("/api/models/active", { name }).catch(() => null);
  if (!res?.ok) {
    const info = (await res?.json().catch(() => null)) as { error?: string } | null;
    notify({ kind: "error", title: "Could not switch the model", body: [info?.error ?? "The server did not answer."] });
    return;
  }
  const next = (await res.json()) as ModelsInfo;
  acceptModels(next);
  const info = next.models.find((x) => x.name === name);
  if (info?.problem) {
    notifyOnce(`model:${name}:${info.problem}`, { kind: "error", title: `Model “${name}” cannot be used`, body: [info.problem, configWhere()] });
  }
}

/** Save the model in use as `[model] use` in the config file. */
export async function makeDefaultModel() {
  const m = getState().models;
  if (!m) return;
  const res = await post("/api/models/default", { name: m.active }).catch(() => null);
  if (!res?.ok) {
    const info = (await res?.json().catch(() => null)) as { error?: string } | null;
    notify({ kind: "error", title: "Could not save the default model", body: [info?.error ?? "The server did not answer."] });
    return;
  }
  const next = (await res.json()) as ModelsInfo;
  acceptModels(next);
  notify({ kind: "info", title: `“${next.default}” is now the default model`, body: next.configPath ? [`Saved in ${next.configPath}`] : [] });
}

/** First load: models, what the cache already knows, then start if wanted. */
export async function bootAids() {
  const m = await fetchModels();
  const layout = await fetchAdaptive();
  const s = getState();
  if (!layout || !aidsOn(s)) return;
  if (m && !m.aids.autoGenerate) return;
  // Only a run that has not started: a stopped one waits for "Resume", a
  // finished or failed one is not repeated by a reload.
  if (layout.progress.state !== "idle") return;
  void runAids();
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
    applyDoc(msg as unknown as DocPayload);
  } else if (msg.type === "models") {
    acceptModels(msg.models as ModelsInfo);
  } else if (msg.type === "adaptive") {
    const layout = msg.layout as AdaptiveLayout;
    if (layout && typeof msg.revision === "number") layout.revision = msg.revision;
    // The layout belongs to whatever file is active on the server; only use it
    // when the body we show is that file (the server only selects for it).
    acceptLayout(layout, "ws");
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

// ---- widgets --------------------------------------------------------------

/**
 * Scroll a reading aid into view and flash it, so the reader sees where it
 * went (a pinned aid jumps to the top of the text, #61).
 */
export function revealWidget(key: string) {
  window.requestAnimationFrame(() => {
    const el = document.querySelector<HTMLElement>(`[data-widget="${CSS.escape(key)}"]`);
    if (!el) return;
    const reduce = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
    el.scrollIntoView({ behavior: reduce ? "auto" : "smooth", block: "nearest" });
    el.classList.remove("mp-aid--moved");
    void el.offsetWidth;
    el.classList.add("mp-aid--moved");
    window.setTimeout(() => el.classList.remove("mp-aid--moved"), 1600);
    el.querySelector<HTMLElement>(".mp-aid__toggle")?.focus({ preventScroll: true });
  });
}

export function togglePin(p: Placement) {
  const k = widgetKey(p);
  let pinned = false;
  updateDocPrefs((d) => {
    pinned = !d.pinned.includes(k);
    return { ...d, pinned: pinned ? [...d.pinned, k] : d.pinned.filter((x) => x !== k) };
  });
  const name = kindLabel(p.node.kind);
  // The aid moves away from under the pointer (to the top, or back to its
  // section): say where it went and offer to go there, so it does not look
  // like it vanished (#61).
  announce(`${name} ${pinned ? "pinned to the top" : "unpinned, back in its section"}`);
  notify(
    {
      kind: "info",
      title: pinned ? `${name} pinned to the top` : `${name} back in its section`,
      body: [pinned ? "It stays at the top of the text while you read." : p.scope === "section" ? `Under “${p.anchor.heading}”.` : ""].filter(Boolean),
      // Pinned aids stick at the top, already in view; one put back can be far away.
      ...(pinned ? {} : { action: { label: "Show it", run: () => revealWidget(k) } }),
    },
    `aid-move:${k}`,
  );
}

export function hideWidget(p: Placement) {
  const k = widgetKey(p);
  // Spread `d`: ticks and step positions of other aids must survive a hide.
  updateDocPrefs((d) => ({ ...d, pinned: d.pinned.filter((x) => x !== k), hidden: d.hidden.includes(k) ? d.hidden : [...d.hidden, k] }));
  const name = kindLabel(p.node.kind);
  announce(`${name} hidden. Undo from the notification, or restore it from Hidden above the text.`);
  // Hiding is intended, but the way back must be obvious and close (#61).
  notify(
    {
      kind: "info",
      title: `${name} hidden`,
      body: ["Bring it back with Undo, or later from “hidden” above the text or the command palette."],
      action: {
        label: "Undo",
        run: () => {
          showWidget(k);
          revealWidget(k);
        },
      },
    },
    `aid-hide:${k}`,
  );
}

/**
 * A table / checklist aid stands in for its block in the text (#63); this
 * switches one of them back to the original Markdown rendering, or again to
 * the aid. Display only: the file is never touched.
 */
export function toggleOriginal(p: Placement) {
  const k = widgetKey(p);
  let original = false;
  updateDocPrefs((d) => {
    const cur = d.original ?? [];
    original = !cur.includes(k);
    return { ...d, original: original ? [...cur, k] : cur.filter((x) => x !== k) };
  });
  announce(original ? `Showing the original ${kindLabel(p.node.kind).toLowerCase()} from the text` : `Showing the ${kindLabel(p.node.kind).toLowerCase()} as a reading aid`);
}

export function showWidget(key: string) {
  updateDocPrefs((d) => ({ ...d, hidden: d.hidden.filter((x) => x !== key) }));
  dismiss(`aid-hide:${key}`);
  announce("Reading aid shown again");
}
