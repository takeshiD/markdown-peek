// Imperative helpers for the server-rendered Markdown body (#16 in-place
// patching, code highlighting, mermaid, MathJax, source-line mapping).
//
// The body HTML comes only from mdpeek's own renderer (design §8: trusted
// fragment); widget content never goes through innerHTML.
import { announce } from "./store";

declare global {
  interface Window {
    hljs?: { highlightElement(el: Element): void };
    mermaid?: { initialize(o: object): void; run(o: object): Promise<void> | void };
    MathJax?: { typesetPromise?: (els?: Element[]) => Promise<void>; startup?: { promise?: Promise<void> } };
  }
}

/** Elements we insert into the article ourselves (adaptive note slots). */
export const SLOT_CLASS = "mp-note-slot";

/** Top-level body blocks (excluding our own note slots). */
export function bodyBlocks(article: HTMLElement): HTMLElement[] {
  return (Array.from(article.children) as HTMLElement[]).filter((e) => !e.classList.contains(SLOT_CLASS));
}

function fragment(html: string): HTMLElement {
  const d = document.createElement("div");
  d.innerHTML = html;
  return d;
}

/** Index pairs (old, new) of the longest common subsequence of equal blocks. */
function lcsPairs(o: string[], n: string[]): [number, number][] {
  const W = n.length + 1;
  const dp = new Uint16Array((o.length + 1) * W);
  for (let i = o.length - 1; i >= 0; i--)
    for (let j = n.length - 1; j >= 0; j--)
      dp[i * W + j] = o[i] === n[j] ? dp[(i + 1) * W + j + 1] + 1 : Math.max(dp[(i + 1) * W + j], dp[i * W + j + 1]);
  const out: [number, number][] = [];
  let i = 0;
  let j = 0;
  while (i < o.length && j < n.length) {
    if (o[i] === n[j]) {
      out.push([i, j]);
      i++;
      j++;
    } else if (dp[(i + 1) * W + j] >= dp[i * W + j + 1]) i++;
    else j++;
  }
  return out;
}

/**
 * Patch `article` from `oldHtml` to `newHtml`, replacing only the top-level
 * blocks whose HTML changed (front/back trim, then an LCS match of the middle
 * so that two separate edits do not re-render everything between them).
 * Returns the inserted elements and their indices. Unchanged blocks keep their
 * DOM nodes, so scroll position and enhancements (highlighting, diagrams) stay.
 */
export function patchArticle(article: HTMLElement, oldHtml: string, newHtml: string): { nodes: HTMLElement[]; indices: number[] } {
  const oldClean = Array.from(fragment(oldHtml).children).map((e) => e.outerHTML);
  const newNodes = Array.from(fragment(newHtml).children) as HTMLElement[];
  const newClean = newNodes.map((e) => e.outerHTML);
  const live = bodyBlocks(article);
  if (live.length !== oldClean.length) {
    live.forEach((e) => e.remove());
    newNodes.forEach((n) => article.appendChild(n));
    return { nodes: newNodes, indices: newNodes.map((_, i) => i) };
  }
  let a = 0;
  while (a < oldClean.length && a < newClean.length && oldClean[a] === newClean[a]) a++;
  let bOld = oldClean.length - 1;
  let bNew = newClean.length - 1;
  while (bOld >= a && bNew >= a && oldClean[bOld] === newClean[bNew]) {
    bOld--;
    bNew--;
  }
  const oMid = oldClean.slice(a, bOld + 1);
  const nMid = newClean.slice(a, bNew + 1);
  const pairs = oMid.length * nMid.length <= 250_000 ? lcsPairs(oMid, nMid) : [];
  const keepOld = new Set(pairs.map(([i]) => i + a));
  const reuse = new Map(pairs.map(([i, j]) => [j + a, live[i + a]]));
  for (let i = a; i <= bOld; i++) if (!keepOld.has(i)) live[i].remove();
  // Walk the new middle in order, inserting fresh nodes before the next kept one.
  const tail = live[bOld + 1] ?? null;
  const nodes: HTMLElement[] = [];
  const indices: number[] = [];
  let cursor: Element | null = tail;
  for (let j = bNew; j >= a; j--) {
    const kept = reuse.get(j);
    if (kept) {
      cursor = kept;
      continue;
    }
    const ref = cursor && cursor.parentNode === article ? cursor : null;
    article.insertBefore(newNodes[j], ref);
    cursor = newNodes[j];
    nodes.unshift(newNodes[j]);
    indices.unshift(j);
  }
  return { nodes, indices };
}

// highlight.js and mermaid are embedded but loaded on demand: as blocking
// <script>s they held the first paint back by ~250 ms (mermaid alone is
// ~2.7 MB), and many documents need neither.
const HLJS_SRC = "/static/js/highlight/highlight.min.js";
const MERMAID_SRC = "/static/js/mermaid/mermaid.min.js";
const loading: Record<string, Promise<void>> = {};
function loadScript(src: string): Promise<void> {
  return (loading[src] ??= new Promise<void>((resolve, reject) => {
    const el = document.createElement("script");
    el.src = src;
    el.async = true;
    el.onload = () => resolve();
    el.onerror = () => reject(new Error(`failed to load ${src}`));
    document.head.appendChild(el);
  }));
}

// Named access on `window` (an element with id="mermaid" is `window.mermaid`)
// makes the global truthy before the script has loaded when the document has
// a heading such as `# Mermaid` (#60). Check for the functions we call.
export function hljsLoaded(): boolean {
  return typeof window.hljs?.highlightElement === "function";
}
export function mermaidLoaded(): boolean {
  return typeof window.mermaid?.initialize === "function" && typeof window.mermaid?.run === "function";
}

let mermaidReady = false;
function ensureMermaid(dark: boolean) {
  if (!mermaidLoaded()) return false;
  if (!mermaidReady) {
    window.mermaid!.initialize({ startOnLoad: false, theme: dark ? "dark" : "neutral", securityLevel: "strict" });
    mermaidReady = true;
  }
  return true;
}

const COPY_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect width="14" height="14" x="8" y="8" rx="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>';
const CHECK_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg>';

async function writeClipboard(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    try {
      const ta = document.createElement("textarea");
      ta.value = text;
      ta.setAttribute("readonly", "");
      ta.style.position = "fixed";
      ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.select();
      const ok = document.execCommand("copy");
      ta.remove();
      return ok;
    } catch {
      return false;
    }
  }
}

/**
 * A copy button on each code block of the body (#67: replaces the separate
 * "Commands" aid, which only repeated the blocks). Icons are our own static
 * markup; the copied text is the code's text content.
 */
function addCopyButton(pre: HTMLElement, code: Element) {
  if (pre.querySelector(":scope > .mp-copy")) return;
  pre.classList.add("mp-has-copy");
  const lang = Array.from(code.classList).find((c) => c.startsWith("language-"))?.slice(9);
  const label = `Copy ${lang ? `${lang} ` : ""}code`;
  const b = document.createElement("button");
  b.type = "button";
  b.className = "mp-copy";
  b.setAttribute("aria-label", label);
  b.title = label;
  b.innerHTML = COPY_ICON;
  let timer = 0;
  b.addEventListener("click", async () => {
    const ok = await writeClipboard((code.textContent ?? "").replace(/\n$/, ""));
    b.innerHTML = ok ? CHECK_ICON : COPY_ICON;
    b.dataset.state = ok ? "copied" : "failed";
    b.setAttribute("aria-label", ok ? "Copied" : "Copy failed");
    // The label change alone is not reliably read out: say it in the live region.
    announce(ok ? "Code copied" : "Could not copy the code");
    window.clearTimeout(timer);
    timer = window.setTimeout(() => {
      b.innerHTML = COPY_ICON;
      delete b.dataset.state;
      b.setAttribute("aria-label", label);
    }, 1600);
  });
  pre.insertBefore(b, pre.firstChild);
}

/** Highlight code, draw mermaid diagrams and typeset math inside `nodes`. */
export function enhance(nodes: Element[]) {
  const codes: Element[] = [];
  const diagrams: Element[] = [];
  for (const n of nodes) {
    n.querySelectorAll("pre code").forEach((c) => {
      if (c.classList.contains("language-mermaid")) diagrams.push(c);
      else if (!c.classList.contains("language-math")) {
        codes.push(c);
        const pre = c.parentElement;
        if (pre?.tagName === "PRE") addCopyButton(pre, c);
      }
    });
  }
  // GFM task-list checkboxes come without a label: name them by their item.
  for (const n of nodes) {
    n.querySelectorAll<HTMLInputElement>('input[type="checkbox"]:not([aria-label])').forEach((cb) => {
      const text = (cb.closest("li")?.textContent ?? "").trim().slice(0, 120);
      cb.setAttribute("aria-label", text || "task");
    });
  }
  if (codes.length) {
    const run = () => codes.forEach((c) => c.isConnected && window.hljs!.highlightElement(c));
    if (hljsLoaded()) run();
    else loadScript(HLJS_SRC).then(run, () => {});
  }
  if (diagrams.length) {
    const draw = () => {
      if (!ensureMermaid(document.documentElement.classList.contains("dark"))) return;
      try {
        const r = window.mermaid!.run({ nodes: diagrams.filter((d) => d.isConnected) });
        if (r && typeof (r as Promise<void>).catch === "function") (r as Promise<void>).catch(() => {});
      } catch {
        /* a broken diagram stays as its source */
      }
    };
    if (mermaidLoaded()) draw();
    else loadScript(MERMAID_SRC).then(draw, () => {});
  }
  const mj = window.MathJax;
  if (typeof mj?.typesetPromise === "function") {
    const go = () => mj.typesetPromise!(nodes).catch(() => {});
    if (typeof mj.startup?.promise?.then === "function") mj.startup.promise.then(go).catch(() => {});
    else go();
  }
}

/** Tag every top-level block with its source line span (from the server). */
export function annotateLines(article: HTMLElement, blocks: [number, number][]) {
  const els = bodyBlocks(article);
  const n = Math.min(els.length, blocks.length);
  for (let i = 0; i < n; i++) {
    els[i].dataset.lineStart = String(blocks[i][0]);
    els[i].dataset.lineEnd = String(blocks[i][1]);
  }
  for (let i = n; i < els.length; i++) {
    delete els[i].dataset.lineStart;
    delete els[i].dataset.lineEnd;
  }
}

/** Blocks overlapping the 1-based inclusive line range. */
export function blocksForRange(article: HTMLElement, start: number, end: number): HTMLElement[] {
  return bodyBlocks(article).filter((e) => {
    const s = Number(e.dataset.lineStart);
    const en = Number(e.dataset.lineEnd);
    return Number.isFinite(s) && s <= end && en >= start;
  });
}

/** The heading block that starts at `line`, else the first heading with `text`. */
export function headingFor(article: HTMLElement, line: number, text?: string): HTMLElement | null {
  const hs = bodyBlocks(article).filter((e) => /^H[1-6]$/.test(e.tagName));
  return (
    hs.find((h) => Number(h.dataset.lineStart) === line) ??
    (text ? hs.find((h) => h.textContent?.trim() === text.trim()) : undefined) ??
    null
  );
}

export function prefersReducedMotion(): boolean {
  return window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
}

/** Restart a one-shot highlight class on elements. */
export function flash(els: Element[], cls: string, ms: number) {
  for (const el of els) {
    el.classList.remove(cls);
    void (el as HTMLElement).offsetWidth;
    el.classList.add(cls);
    window.setTimeout(() => el.classList.remove(cls), ms);
  }
}

export interface HeadingInfo { id: string; level: number; text: string; line: number | null; el: HTMLElement }

export function collectHeadings(article: HTMLElement): HeadingInfo[] {
  return (Array.from(article.querySelectorAll(":scope > h1, :scope > h2, :scope > h3, :scope > h4, :scope > h5, :scope > h6")) as HTMLElement[]).map(
    (el, i) => {
      if (!el.id) el.id = `mp-h-${i}`;
      const l = Number(el.dataset.lineStart);
      return { id: el.id, level: Number(el.tagName[1]), text: el.textContent?.trim() ?? "", line: Number.isFinite(l) ? l : null, el };
    },
  );
}
