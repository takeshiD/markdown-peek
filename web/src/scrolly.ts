// Generative Scrollytelling — reader-paced guided reading (design §5).
//
// A separate mode from the "Generated UI" panel: instead of a static component
// pane, it turns the document itself into a guided read. On enter it fetches a
// ScrollyGuide (`/api/scrolly`: whole-doc overview + per-section commentary),
// greys out the article, and — as the *reader* scrolls (never auto-driven) —
// highlights the section they've reached and reveals its commentary with a light
// typewriter effect (the "simple streaming" of this prototype slice).
//
// Also: a language selector (auto / 日本語 / English, persisted) and in-panel
// Q&A scoped to the section the reader is on.
//
// Deliberately imperative DOM (not Preact): the typewriter + scroll-sync are
// easier to reason about without a vdom re-render fighting the animation, and
// the highlight/dim works directly on the server-rendered body.
//
// LLM-only (no offline fallback): generation errors surface in the panel.
// Deferred to later slices: resumable/saved sessions, true server-side token
// streaming, inline (anchored) commentary cards.

import "./scrolly.css";

const PANEL_ID = "mdpeek-scrolly-panel";
const TOGGLE_ID = "mdpeek-scrolly-toggle";
const OPEN_CLASS = "mdpeek-scrolly-open";
const GUI_OPEN_CLASS = "mdpeek-gui-open";
const FOCUS_CLASS = "mdpeek-scrolly-focus";
const LANG_KEY = "mdpeek-scrolly-lang";
/** A heading counts as "current" once its top passes this many px from the top. */
const ACTIVE_OFFSET = 140;

type Lang = "auto" | "ja" | "en";
const LANGS: { value: Lang; label: string }[] = [
  { value: "auto", label: "Auto" },
  { value: "ja", label: "日本語" },
  { value: "en", label: "English" },
];

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
interface ChatTurn {
  role: "user" | "assistant";
  content: string;
}

/** A guide section resolved to the DOM elements it spans. */
interface DomSection {
  section: Section;
  heading: HTMLElement;
  els: HTMLElement[];
}

interface Runtime {
  dom: DomSection[];
  activeAnchor: string | null;
  lang: Lang;
  overview: string;
  history: ChatTurn[];
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
    chatLog: HTMLElement;
    chatInput: HTMLInputElement;
    chatSend: HTMLButtonElement;
  };
}

let rt: Runtime | null = null;

const reduceMotion = () =>
  window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;

function readLang(): Lang {
  const v = localStorage.getItem(LANG_KEY);
  return v === "ja" || v === "en" || v === "auto" ? v : "auto";
}

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

/** A small inline spinner + label (used while generating / answering). */
function spinner(label: string): HTMLElement {
  const wrap = el("span", "scrolly-loading");
  wrap.append(el("span", "scrolly-spinner"), el("span", "scrolly-loading__label", label));
  return wrap;
}

/** Build the fixed side panel once; returns its dynamic sub-elements. */
function buildPanel(panel: HTMLElement, lang: Lang): Runtime["els"] {
  panel.textContent = "";

  const head = el("div", "scrolly-head");
  const title = el("span", "scrolly-head__title", "✨ Guided Reading");
  const langSel = el("select", "scrolly-lang");
  langSel.setAttribute("aria-label", "Commentary language");
  for (const opt of LANGS) {
    const o = el("option", undefined, opt.label);
    o.value = opt.value;
    if (opt.value === lang) o.selected = true;
    langSel.append(o);
  }
  langSel.addEventListener("change", () => setLang(langSel.value as Lang));
  const origin = el("span", "scrolly-origin");
  const close = el("button", "scrolly-close");
  close.type = "button";
  close.setAttribute("aria-label", "Close guided reading");
  close.textContent = "✕";
  close.addEventListener("click", exit);
  head.append(title, langSel, origin, close);

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

  // Q&A about the section the reader is on.
  const chat = el("section", "scrolly-chat");
  chat.append(el("h3", "scrolly-block__label", "Ask about this section"));
  const chatLog = el("div", "scrolly-chatlog");
  const form = el("form", "scrolly-chatform");
  const chatInput = el("input", "scrolly-chatinput");
  chatInput.type = "text";
  chatInput.placeholder = "質問を入力…";
  chatInput.setAttribute("aria-label", "Ask a question about this section");
  const chatSend = el("button", "scrolly-chatsend");
  chatSend.type = "submit";
  chatSend.textContent = "Send";
  form.append(chatInput, chatSend);
  form.addEventListener("submit", (e) => {
    e.preventDefault();
    void ask();
  });
  chat.append(chatLog, form);

  body.append(overviewWrap, progress, current, nav, chat);
  panel.append(head, body);

  return {
    origin,
    overview,
    progressText,
    progressFill,
    sectionTitle,
    commentary,
    prev,
    next,
    chatLog,
    chatInput,
    chatSend,
  };
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

function activeSection(): DomSection | null {
  if (!rt) return null;
  return rt.dom.find((d) => d.section.anchor === rt!.activeAnchor) ?? null;
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
  rt.els.chatInput.placeholder = `「${d.section.title}」について質問…`;

  typewrite(rt.els.commentary, d.section.commentary || "（この節の解説は生成されませんでした）");
}

/** Reveal `text` into `target` char-by-char (instant under reduced motion). */
function typewrite(target: HTMLElement, text: string, done?: () => void) {
  if (rt?.typer != null) {
    window.clearInterval(rt.typer);
    rt.typer = null;
  }
  if (reduceMotion()) {
    target.textContent = text;
    done?.();
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
      done?.();
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

// ---- Q&A ------------------------------------------------------------------

function addBubble(role: "user" | "assistant", text: string): HTMLElement {
  const b = el("div", `scrolly-bubble scrolly-bubble--${role}`);
  b.textContent = text;
  rt?.els.chatLog.append(b);
  rt?.els.chatLog.scrollTo({ top: rt.els.chatLog.scrollHeight });
  return b;
}

async function ask() {
  if (!rt) return;
  const q = rt.els.chatInput.value.trim();
  if (!q) return;
  const section = activeSection();

  rt.els.chatInput.value = "";
  rt.els.chatInput.disabled = true;
  rt.els.chatSend.disabled = true;
  addBubble("user", q);

  const pending = el("div", "scrolly-bubble scrolly-bubble--assistant");
  pending.append(spinner("考え中…"));
  rt.els.chatLog.append(pending);
  rt.els.chatLog.scrollTo({ top: rt.els.chatLog.scrollHeight });

  try {
    const res = await fetch("/api/scrolly/ask", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        anchor: section?.section.anchor ?? "",
        question: q,
        lang: rt.lang,
        history: rt.history.slice(-8),
        guide_overview: rt.overview,
        guide_commentary: section?.section.commentary ?? "",
      }),
    });
    if (!res.ok) throw new Error(`ask returned ${res.status}`);
    const data = (await res.json()) as { answer: string };
    pending.textContent = "";
    typewrite(pending, data.answer, () =>
      rt?.els.chatLog.scrollTo({ top: rt.els.chatLog.scrollHeight }),
    );
    rt.history.push({ role: "user", content: q }, { role: "assistant", content: data.answer });
  } catch (e) {
    pending.classList.add("scrolly-bubble--error");
    pending.textContent = `回答の取得に失敗しました: ${String(e)}`;
  } finally {
    if (rt) {
      rt.els.chatInput.disabled = false;
      rt.els.chatSend.disabled = false;
      rt.els.chatInput.focus();
    }
  }
}

// ---- Guide loading --------------------------------------------------------

async function loadGuide() {
  if (!rt) return;
  const els = rt.els;

  // Detach any previous focus/scroll wiring while we regenerate.
  window.removeEventListener("scroll", rt.onScroll);
  for (const d of rt.dom) for (const n of d.els) n.classList.remove(FOCUS_CLASS);
  rt.dom = [];
  rt.activeAnchor = null;

  els.overview.textContent = "";
  els.overview.append(spinner("ガイドを生成中…"));
  els.sectionTitle.textContent = "…";
  els.commentary.textContent = "";
  els.origin.textContent = "";

  let guide: Guide;
  try {
    const res = await fetch(`/api/scrolly?lang=${encodeURIComponent(rt.lang)}`);
    if (!res.ok) {
      const info = (await res.json().catch(() => null)) as { error?: string } | null;
      throw new Error(info?.error ?? `/api/scrolly returned ${res.status}`);
    }
    guide = (await res.json()) as Guide;
  } catch (e) {
    els.overview.textContent = `ガイドの生成に失敗しました: ${String(e)}`;
    return;
  }

  rt.overview = guide.overview;
  els.origin.textContent = guide.origin === "llm" ? "LLM" : guide.origin;
  els.origin.classList.toggle("scrolly-origin--llm", guide.origin === "llm");
  els.overview.textContent = guide.overview;

  rt.dom = resolveDom(guide.sections);
  if (rt.dom.length === 0) {
    els.sectionTitle.textContent = "No sections found";
    els.commentary.textContent = "This document has no H1–H3 headings to guide through.";
    return;
  }

  window.addEventListener("scroll", rt.onScroll, { passive: true });
  setActive(currentAnchor());
}

function setLang(lang: Lang) {
  if (!rt || lang === rt.lang) return;
  rt.lang = lang;
  localStorage.setItem(LANG_KEY, lang);
  void loadGuide();
}

// ---- Enter / exit ---------------------------------------------------------

async function enter() {
  const panel = document.getElementById(PANEL_ID);
  if (!panel) return;

  // Mutually exclusive with the Generated UI pane.
  document.body.classList.remove(GUI_OPEN_CLASS);
  document.getElementById("mdpeek-gui-toggle")?.setAttribute("aria-pressed", "false");

  document.body.classList.add(OPEN_CLASS);
  document.getElementById(TOGGLE_ID)?.setAttribute("aria-pressed", "true");

  const lang = readLang();
  const els = buildPanel(panel, lang);
  rt = {
    dom: [],
    activeAnchor: null,
    lang,
    overview: "",
    history: [],
    onScroll: () => setActive(currentAnchor()),
    typer: null,
    els,
  };
  await loadGuide();
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
    // Don't hijack Escape while typing a question.
    if (
      e.key === "Escape" &&
      document.body.classList.contains(OPEN_CLASS) &&
      document.activeElement !== rt?.els.chatInput
    ) {
      exit();
    }
  });
}
