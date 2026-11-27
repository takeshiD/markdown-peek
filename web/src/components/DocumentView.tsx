// The document column: front matter, the GFM body (server HTML, patched in
// place on live updates) and, in Adaptive mode, reading aids set into the
// text next to the lines they come from.
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { cn } from "cn";
import { getState, setState, useStore } from "../lib/store";
import {
  SLOT_CLASS,
  annotateLines,
  bodyBlocks,
  collectHeadings,
  enhance,
  flash,
  patchArticle,
  prefersReducedMotion,
} from "../lib/body";
import { registerDocument } from "../lib/nav";
import { AidsStrip, isActive, useVisiblePlacements, WidgetNote } from "./Widgets";
import type { Placement } from "../lib/types";
import { widgetKey } from "../lib/types";

const BODY_EVENT = "mp:body";

export function DocumentView() {
  const doc = useStore((s) => s.doc);
  const docVersion = useStore((s) => s.docVersion);
  // Kana means Japanese: lets the browser apply Japanese line breaking and
  // fonts (the UI chrome itself stays English).
  const lang = /[\u3040-\u30ff]/.test(doc.html) ? "ja" : undefined;
  const mode = useStore((s) => s.prefs.mode);
  const scrollRef = useRef<HTMLElement>(null);
  const articleRef = useRef<HTMLElement>(null);
  const shown = useRef<{ html: string; path: string; el: HTMLElement } | null>(null);

  // Render / patch the body whenever a new version arrives (#16).
  useLayoutEffect(() => {
    const article = articleRef.current;
    if (!article) return;
    registerDocument(article, scrollRef.current);
    const prev = shown.current;
    let changed: HTMLElement[] = [];
    if (!prev || prev.path !== doc.path || prev.el !== article) {
      bodyBlocks(article).forEach((e) => e.remove());
      const tmp = document.createElement("div");
      tmp.innerHTML = doc.html;
      const first = article.firstChild;
      for (const n of Array.from(tmp.children)) article.insertBefore(n, first);
      annotateLines(article, doc.blocks);
      enhance(bodyBlocks(article));
      if (prev && prev.path !== doc.path && scrollRef.current) scrollRef.current.scrollTop = 0;
    } else {
      // Remember where an unchanged block on screen sits, to keep the reading
      // position even when blocks above it change height.
      const sc = scrollRef.current;
      const scTop = sc?.getBoundingClientRect().top ?? 0;
      const before = new Map<Element, number>();
      for (const b of bodyBlocks(article)) {
        const r = b.getBoundingClientRect();
        if (r.bottom > scTop) before.set(b, r.top);
      }
      const res = patchArticle(article, prev.html, doc.html);
      if (sc) {
        const anchor = bodyBlocks(article).find((b) => before.has(b));
        if (anchor) sc.scrollTop += anchor.getBoundingClientRect().top - before.get(anchor)!;
      }
      annotateLines(article, doc.blocks);
      enhance(res.nodes);
      changed = res.nodes;
      setState({ changed: res.indices });
    }
    shown.current = { html: doc.html, path: doc.path, el: article };
    // The browser tab follows the open file.
    if (doc.title) document.title = doc.title;
    setState({ headings: collectHeadings(article) });
    window.dispatchEvent(new Event(BODY_EVENT));
    if (changed.length) {
      flash(changed, "mp-changed", 2600);
      if (getState().prefs.autoscroll) {
        changed[0].scrollIntoView({ behavior: prefersReducedMotion() ? "auto" : "smooth", block: "center" });
      }
    }
  }, [docVersion, doc]);

  // Scrollspy: the section whose heading was last scrolled past.
  useEffect(() => {
    const sc = scrollRef.current;
    if (!sc) return;
    let raf = 0;
    const update = () => {
      raf = 0;
      const top = sc.getBoundingClientRect().top + 96;
      let line: number | null = null;
      for (const h of getState().headings) {
        if (!h.el.isConnected) continue;
        if (h.el.getBoundingClientRect().top <= top) line = h.line;
        else break;
      }
      if (line == null && getState().headings.length) line = getState().headings[0].line;
      // At the very end, the last heading on screen is the one being read.
      if (sc.scrollTop + sc.clientHeight >= sc.scrollHeight - 2) {
        const bottom = sc.getBoundingClientRect().bottom;
        for (const h of getState().headings) if (h.el.isConnected && h.el.getBoundingClientRect().top < bottom) line = h.line;
      }
      if (line !== getState().activeLine) setState({ activeLine: line });
    };
    const onScroll = () => {
      if (!raf) raf = requestAnimationFrame(update);
    };
    sc.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener(BODY_EVENT, onScroll);
    update();
    return () => {
      sc.removeEventListener("scroll", onScroll);
      window.removeEventListener(BODY_EVENT, onScroll);
      if (raf) cancelAnimationFrame(raf);
    };
  }, []);

  return (
    <main
      id="main"
      ref={scrollRef}
      tabIndex={-1}
      aria-label="Document"
      className="h-full overflow-y-auto overflow-x-hidden outline-none [overflow-anchor:auto]"
    >
      <div className={cn("mp-doc px-5 pt-6 pb-[50vh] sm:px-10 sm:pt-10", mode === "adaptive" && "mp-adaptive")}>
        <div className="mx-auto max-w-(--measure) text-[16px]">
          {doc.error && (
            <p role="alert" className="mb-6 text-danger">
              {doc.error}
            </p>
          )}
          {mode === "adaptive" && <AidsStrip />}
          {doc.frontmatter && <FrontMatter text={doc.frontmatter} />}
          <article ref={articleRef} className="markdown-body" lang={lang} aria-label={doc.title} />
          {mode === "adaptive" && <InlineAids articleRef={articleRef} />}
        </div>
      </div>
    </main>
  );
}

function FrontMatter({ text }: { text: string }) {
  return (
    <details className="mb-8 border-b pb-3 text-[13px]" data-testid="frontmatter">
      <summary className="cursor-pointer text-muted-foreground select-none hover:text-foreground max-md:flex max-md:min-h-11 max-md:items-center">Front matter</summary>
      <pre className="mt-2 overflow-auto rounded-md bg-muted px-3 py-2 font-mono text-[12.5px] leading-relaxed whitespace-pre">{text}</pre>
    </details>
  );
}

/**
 * Adaptive mode: every reading aid gets one slot inside the article, placed
 * right after the blocks it was built from (a chart under its table, steps
 * under their list). Whole-document aids sit after the opening section;
 * pinned aids sit at the top, under the title.
 */
function InlineAids({ articleRef }: { articleRef: React.RefObject<HTMLElement | null> }) {
  const { visible, pinned } = useVisiblePlacements();
  const activeLine = useStore((s) => s.activeLine);
  const slots = useRef(new Map<string, HTMLDivElement>());
  const [, force] = useState(0);
  const keys = visible.map((p) => `${widgetKey(p)}${pinned.has(widgetKey(p)) ? "^" : ""}`).join("|");

  useLayoutEffect(() => {
    const article = articleRef.current;
    if (!article) return;
    const map = slots.current;
    const wanted = new Set(visible.map(widgetKey));
    for (const [k, el] of map) {
      if (!wanted.has(k)) {
        el.remove();
        map.delete(k);
      }
    }
    let created = false;
    for (const p of visible) {
      const k = widgetKey(p);
      if (!map.has(k)) {
        const d = document.createElement("div");
        d.className = SLOT_CLASS;
        d.dataset.slot = k;
        map.set(k, d);
        created = true;
      }
    }
    const place = () => placeSlots(article, visible, pinned, map);
    place();
    window.addEventListener(BODY_EVENT, place);
    if (created) force((n) => n + 1);
    return () => window.removeEventListener(BODY_EVENT, place);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [keys]);

  useEffect(
    () => () => {
      for (const el of slots.current.values()) el.remove();
      slots.current.clear();
    },
    [],
  );

  return (
    <>
      {visible.map((p) => {
        const el = slots.current.get(widgetKey(p));
        if (!el) return null;
        const pin = pinned.has(widgetKey(p));
        return createPortal(<WidgetNote p={p} pinned={pin} active={isActive(p, activeLine)} atTop={pin} />, el, widgetKey(p));
      })}
    </>
  );
}

/** Where each slot goes; slots aimed at the same spot keep their order. */
function placeSlots(article: HTMLElement, visible: Placement[], pinned: Set<string>, map: Map<string, HTMLDivElement>) {
  const blocks = bodyBlocks(article);
  const title = blocks[0] && /^H1$/.test(blocks[0].tagName) ? blocks[0] : null;
  const top = title ? nextBlock(title) : (blocks[0] ?? null);
  const secondHeading = blocks.find((b, i) => i > 0 && /^H[1-6]$/.test(b.tagName)) ?? null;
  for (const p of visible) {
    const el = map.get(widgetKey(p));
    if (!el) continue;
    el.dataset.place = pinned.has(widgetKey(p)) ? "top" : p.scope;
    let before: Element | null;
    if (pinned.has(widgetKey(p))) before = top;
    else if (p.scope === "document" || blocks.length === 0) before = secondHeading;
    else before = afterSource(blocks, p);
    article.insertBefore(el, before);
  }
}

/** The block after the last one inside the aid's source lines (its section as a fallback). */
function afterSource(blocks: HTMLElement[], p: Placement): Element | null {
  const sec = p.anchor.range;
  const r = p.node.sourceRange ?? sec;
  const lastLine = (x: typeof r) => (x.end_column === 1 && x.end_line > x.start_line ? x.end_line - 1 : x.end_line);
  const end = Math.min(lastLine(r), lastLine(sec));
  let last: HTMLElement | null = null;
  for (const b of blocks) {
    const l = Number(b.dataset.lineStart);
    if (!Number.isFinite(l)) continue;
    if (l >= sec.start_line && l <= end) last = b;
  }
  if (last) return nextBlock(last);
  return blocks.find((b) => Number(b.dataset.lineStart) > lastLine(sec)) ?? null;
}

function nextBlock(el: Element): Element | null {
  let n = el.nextElementSibling;
  while (n && n.classList.contains(SLOT_CLASS)) n = n.nextElementSibling;
  return n;
}
