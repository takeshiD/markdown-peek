// Generative Scrollytelling — reader-paced guided reading (design §5).
//
// A separate mode from the "Generated UI" panel: instead of a static component
// pane, it turns the document itself into a guided read. On enter it fetches a
// ScrollyGuide (`/api/scrolly`: whole-doc overview + per-section commentary),
// greys out the article, and — as the *reader* scrolls (never auto-driven) —
// highlights the section they've reached and reveals its commentary with a light
// typewriter effect (the "simple streaming" of this prototype slice).
//
// Deliberately imperative DOM (not Preact): the typewriter + scroll-sync are
// easier to reason about without a vdom re-render fighting the animation, and
// the highlight/dim works directly on the server-rendered body.
//
// Deferred to later slices: in-section Q&A, resumable/saved sessions, true
// server-side token streaming, inline (anchored) commentary cards.

import "./scrolly.css";

const PANEL_ID = "mdpeek-scrolly-panel";
const TOGGLE_ID = "mdpeek-scrolly-toggle";
const OPEN_CLASS = "mdpeek-scrolly-open";
const GUI_OPEN_CLASS = "mdpeek-gui-open";
const FOCUS_CLASS = "mdpeek-scrolly-focus";
/** A heading counts as "current" once its top passes this many px from the top. */
const ACTIVE_OFFSET = 140;

interface Section {
  index: number;
  anchor: string;
  title: string;
  level: number;
  commentary: string;
}
interface Guide {
  overview: string;
  sections: Section[];
  origin: string;
}

/** A guide section resolved to the DOM elements it spans. */
interface DomSection {
  section: Section;
  heading: HTMLElement;
  els: HTMLElement[];
}

interface Runtime {
  guide: Guide;
  dom: DomSection[];
  activeAnchor: string | null;
  onScroll: () => void;
  typer: number | null;
  els: {
    origin: HTMLElement;
    overview: HTMLElement;
    progressText: HTMLElement;
    progressFill: HTMLElement;
    sectionTitle: HTMLElement;
    commentary: HTMLElement;
    prev: HTMLButtonElement;
    next: HTMLButtonElement;
  };
}

let rt: Runtime | null = null;

const reduceMotion = () =>
  window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  cls?: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (cls) node.className = cls;
  if (text != null) node.textContent = text;
  return node;
}

/** Build the fixed side panel once; returns its dynamic sub-elements. */
function buildPanel(panel: HTMLElement): Runtime["els"] {
  panel.textContent = "";

  const head = el("div", "scrolly-head");
  const title = el("span", "scrolly-head__title", "✨ Guided Reading");
  const origin = el("span", "scrolly-origin");
  const close = el("button", "scrolly-close");
  close.type = "button";
  close.setAttribute("aria-label", "Close guided reading");
  close.textContent = "✕";
  close.addEventListener("click", exit);
  head.append(title, origin, close);

  const body = el("div", "scrolly-body");

  const overviewWrap = el("section", "scrolly-block scrolly-overview");
  overviewWrap.append(el("h3", "scrolly-block__label", "Overview"));
  const overview = el("p", "scrolly-text");
  overviewWrap.append(overview);

  const progress = el("div", "scrolly-progress");
  const progressText = el("span", "scrolly-progress__text", "—");
  const track = el("div", "scrolly-progress__track");
  const progressFill = el("div", "scrolly-progress__fill");
  track.append(progressFill);
  progress.append(progressText, track);

  const current = el("section", "scrolly-block scrolly-current");
  const sectionTitle = el("h3", "scrolly-block__label", "Scroll to begin");
  const commentary = el("p", "scrolly-text");
  current.append(sectionTitle, commentary);

  const nav = el("div", "scrolly-nav");
  const prev = el("button", "scrolly-navbtn");
  prev.type = "button";
  prev.textContent = "↑ Prev";
  prev.addEventListener("click", () => step(-1));
  const next = el("button", "scrolly-navbtn");
  next.type = "button";
  next.textContent = "Next ↓";
  next.addEventListener("click", () => step(1));
  nav.append(prev, next);

  body.append(overviewWrap, progress, current, nav);
  panel.append(head, body);

  return { origin, overview, progressText, progressFill, sectionTitle, commentary, prev, next };
}

/** Resolve guide sections to spans of `.markdown-body` direct children. */
function resolveDom(sections: Section[]): DomSection[] {
  const article = document.querySelector<HTMLElement>(".markdown-body");
  if (!article) return [];
  const anchors = new Map(sections.map((s) => [s.anchor, s]));
  const out: DomSection[] = [];
  let cur: DomSection | null = null;
  for (const child of Array.from(article.children) as HTMLElement[]) {
    const id = child.id;
    const matched = id ? anchors.get(id) : undefined;
    const isHeading = /^H[1-6]$/.test(child.tagName);
    if (isHeading && matched) {
      cur = { section: matched, heading: child, els: [child] };
      out.push(cur);
    } else if (cur) {
      cur.els.push(child);
    }
  }
  // Keep document order stable regardless of guide ordering.
  out.sort((a, b) => a.section.index - b.section.index);
  return out;
}

/** Pick the section whose heading is the last one scrolled past the offset. */
function currentAnchor(): string | null {
  if (!rt || rt.dom.length === 0) return null;
  let anchor = rt.dom[0].section.anchor;
  for (const d of rt.dom) {
    if (d.heading.getBoundingClientRect().top <= ACTIVE_OFFSET) {
      anchor = d.section.anchor;
    } else {
      break;
    }
  }
  return anchor;
}

function setActive(anchor: string | null) {
  if (!rt || anchor == null || anchor === rt.activeAnchor) return;
  rt.activeAnchor = anchor;

  for (const d of rt.dom) {
    const focus = d.section.anchor === anchor;
    for (const node of d.els) node.classList.toggle(FOCUS_CLASS, focus);
  }

  const idx = rt.dom.findIndex((d) => d.section.anchor === anchor);
  const d = idx >= 0 ? rt.dom[idx] : null;
  if (!d) return;

  rt.els.sectionTitle.textContent = d.section.title;
  rt.els.progressText.textContent = `Section ${idx + 1} / ${rt.dom.length}`;
  rt.els.progressFill.style.width = `${((idx + 1) / rt.dom.length) * 100}%`;
  rt.els.prev.disabled = idx <= 0;
  rt.els.next.disabled = idx >= rt.dom.length - 1;

  typewrite(rt.els.commentary, d.section.commentary);
}

/** Reveal `text` into `target` char-by-char (instant under reduced motion). */
function typewrite(target: HTMLElement, text: string) {
  if (rt?.typer != null) {
    window.clearInterval(rt.typer);
    rt.typer = null;
  }
  if (reduceMotion()) {
    target.textContent = text;
    return;
  }
  target.textContent = "";
  const chars = Array.from(text);
  let i = 0;
  const timer = window.setInterval(() => {
    target.textContent += chars[i] ?? "";
    i += 1;
    if (i >= chars.length) {
      window.clearInterval(timer);
      if (rt) rt.typer = null;
    }
  }, 12);
  if (rt) rt.typer = timer;
}

/** Reader-driven jump to the prev/next section (still scroll, not auto-play). */
function step(delta: number) {
  if (!rt) return;
  const idx = rt.dom.findIndex((d) => d.section.anchor === rt!.activeAnchor);
  const target = rt.dom[(idx < 0 ? 0 : idx) + delta];
  if (!target) return;
  target.heading.scrollIntoView({
    behavior: reduceMotion() ? "auto" : "smooth",
    block: "start",
  });
}

async function enter() {
  const panel = document.getElementById(PANEL_ID);
  if (!panel) return;

  // Mutually exclusive with the Generated UI pane.
  document.body.classList.remove(GUI_OPEN_CLASS);
  document
    .getElementById("mdpeek-gui-toggle")
    ?.setAttribute("aria-pressed", "false");

  document.body.classList.add(OPEN_CLASS);
  document.getElementById(TOGGLE_ID)?.setAttribute("aria-pressed", "true");

  const els = buildPanel(panel);
  els.overview.textContent = "Generating guide…";

  let guide: Guide;
  try {
    const res = await fetch("/api/scrolly");
    if (!res.ok) throw new Error(`/api/scrolly returned ${res.status}`);
    guide = (await res.json()) as Guide;
  } catch (e) {
    els.overview.textContent = `Failed to build guide: ${String(e)}`;
    return;
  }

  const dom = resolveDom(guide.sections);
  const onScroll = () => setActive(currentAnchor());
  rt = { guide, dom, activeAnchor: null, onScroll, typer: null, els };

  els.origin.textContent = guide.origin === "llm" ? "LLM" : "offline";
  els.origin.classList.toggle("scrolly-origin--llm", guide.origin === "llm");
  els.overview.textContent = guide.overview;

  if (dom.length === 0) {
    els.sectionTitle.textContent = "No sections found";
    els.commentary.textContent =
      "This document has no H1–H3 headings to guide through.";
    return;
  }

  window.addEventListener("scroll", onScroll, { passive: true });
  setActive(currentAnchor());
}

function exit() {
  document.body.classList.remove(OPEN_CLASS);
  document.getElementById(TOGGLE_ID)?.setAttribute("aria-pressed", "false");
  if (!rt) return;
  window.removeEventListener("scroll", rt.onScroll);
  if (rt.typer != null) window.clearInterval(rt.typer);
  for (const d of rt.dom) {
    for (const node of d.els) node.classList.remove(FOCUS_CLASS);
  }
  rt = null;
}

export function initScrolly() {
  const toggle = document.getElementById(TOGGLE_ID);
  const panel = document.getElementById(PANEL_ID);
  if (!toggle || !panel) return;

  toggle.hidden = false;
  toggle.addEventListener("click", () => {
    if (document.body.classList.contains(OPEN_CLASS)) {
      exit();
    } else {
      void enter();
    }
  });

  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && document.body.classList.contains(OPEN_CLASS)) {
      exit();
    }
  });
}
