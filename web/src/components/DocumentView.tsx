// The document column: front matter, the GFM body (server HTML, patched in
// place on live updates), adaptive margin notes, and the diff view.
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { ArrowLeftRight, Code, Columns2, Eye, Rows3, X } from "lucide-react";
import { cn } from "cn";
import { Button } from "@/components/ui/button";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { getState, setState, useStore } from "../lib/store";
import { closeDiff, requestDiff } from "../lib/actions";
import {
  SLOT_CLASS,
  annotateLines,
  bodyBlocks,
  collectHeadings,
  enhance,
  flash,
  headingFor,
  patchArticle,
  prefersReducedMotion,
} from "../lib/body";
import { registerDocument } from "../lib/nav";
import { isActive, useVisiblePlacements, WidgetNote } from "./Widgets";
import type { Placement } from "../lib/types";
import { widgetKey } from "../lib/types";

const BODY_EVENT = "mp:body";

export function DocumentView() {
  const doc = useStore((s) => s.doc);
  const docVersion = useStore((s) => s.docVersion);
  const mode = useStore((s) => s.prefs.mode);
  const diff = useStore((s) => s.diff);
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
  }, [docVersion, doc, diff]);

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
  }, [diff]);

  return (
    <main
      id="main"
      ref={scrollRef}
      tabIndex={-1}
      aria-label="Document"
      className="h-full overflow-y-auto overflow-x-hidden outline-none [overflow-anchor:auto]"
    >
      {diff ? (
        <DiffView />
      ) : (
        <div className={cn("mp-doc px-5 pt-6 pb-[50vh] sm:px-10 sm:pt-10", mode === "adaptive" && "mp-adaptive")}>
          <div
            className="mx-auto text-[16px]"
            style={{ maxWidth: mode === "adaptive" ? "calc(var(--measure) + var(--note-width) + var(--note-gap))" : "var(--measure)" }}
          >
            {doc.error && (
              <p role="alert" className="mb-6 text-danger">
                {doc.error}
              </p>
            )}
            {doc.frontmatter && <FrontMatter text={doc.frontmatter} />}
            <article ref={articleRef} className="markdown-body" aria-label={doc.title} />
            {mode === "adaptive" && <AdaptiveSlots articleRef={articleRef} />}
          </div>
        </div>
      )}
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

interface Slot { side: HTMLDivElement; inline: HTMLDivElement }

/**
 * Adaptive mode: each widget gets two slots inside the article — one floated
 * into the right margin next to its section heading (wide columns), one right
 * after its section (narrow columns). CSS container queries show one of them.
 */
function AdaptiveSlots({ articleRef }: { articleRef: React.RefObject<HTMLElement | null> }) {
  const { visible, pinned } = useVisiblePlacements();
  const activeLine = useStore((s) => s.activeLine);
  const slots = useRef(new Map<string, Slot>());
  const [, force] = useState(0);
  const keys = visible.map(widgetKey).join("|");

  useLayoutEffect(() => {
    const article = articleRef.current;
    if (!article) return;
    const map = slots.current;
    const wanted = new Set(visible.map(widgetKey));
    for (const [k, s] of map) {
      if (!wanted.has(k)) {
        s.side.remove();
        s.inline.remove();
        map.delete(k);
      }
    }
    let created = false;
    for (const p of visible) {
      const k = widgetKey(p);
      if (!map.has(k)) {
        const mk = (place: string) => {
          const d = document.createElement("div");
          d.className = SLOT_CLASS;
          d.dataset.place = place;
          d.dataset.slot = k;
          return d;
        };
        map.set(k, { side: mk("side"), inline: mk("inline") });
        created = true;
      }
    }
    const place = () => placeSlots(article, visible, map);
    place();
    window.addEventListener(BODY_EVENT, place);
    if (created) force((n) => n + 1);
    return () => window.removeEventListener(BODY_EVENT, place);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [keys]);

  useEffect(
    () => () => {
      for (const s of slots.current.values()) {
        s.side.remove();
        s.inline.remove();
      }
      slots.current.clear();
    },
    [],
  );

  return (
    <>
      {visible.map((p) => {
        const s = slots.current.get(widgetKey(p));
        if (!s) return null;
        const pin = pinned.has(widgetKey(p));
        const act = isActive(p, activeLine);
        return (
          <PortalPair key={widgetKey(p)} slot={s}>
            {(where) => <WidgetNote p={p} pinned={pin} active={act} compact={where === "inline"} collapsible={p.scope === "document"} />}
          </PortalPair>
        );
      })}
    </>
  );
}

function PortalPair({ slot, children }: { slot: Slot; children: (where: "side" | "inline") => React.ReactNode }) {
  return (
    <>
      {createPortal(children("side"), slot.side)}
      {createPortal(children("inline"), slot.inline)}
    </>
  );
}

function placeSlots(article: HTMLElement, visible: Placement[], map: Map<string, Slot>) {
  const blocks = bodyBlocks(article);
  for (const p of visible) {
    const s = map.get(widgetKey(p));
    if (!s) continue;
    if (p.scope === "document" || blocks.length === 0) {
      // Whole-document aids: beside the title in the margin; inline (narrow)
      // after the opening section so the text still comes first.
      const now = bodyBlocks(article);
      article.insertBefore(s.side, now[0] ?? null);
      const second = now.findIndex((b, i) => i > 0 && /^H[1-6]$/.test(b.tagName));
      article.insertBefore(s.inline, second > 0 ? now[second] : null);
      continue;
    }
    const r = p.anchor.range;
    const heading = headingFor(article, r.start_line, p.anchor.heading);
    // Side: right after the section heading (so the note starts level with it).
    const afterHeading = heading ? nextBlock(heading) : firstBlockAtOrAfter(blocks, r.start_line);
    article.insertBefore(s.side, afterHeading);
    // Inline: right after the last block of the section.
    const endLine = r.end_column === 1 ? r.end_line : r.end_line + 1;
    article.insertBefore(s.inline, firstBlockAtOrAfter(blocks, endLine));
  }
}

function nextBlock(el: Element): Element | null {
  let n = el.nextElementSibling;
  while (n && n.classList.contains(SLOT_CLASS)) n = n.nextElementSibling;
  return n;
}

function firstBlockAtOrAfter(blocks: HTMLElement[], line: number): HTMLElement | null {
  return blocks.find((b) => Number(b.dataset.lineStart) >= line) ?? null;
}

// ---- diff view (#15) -------------------------------------------------------

function diffUnits(root: Element): Element[] {
  return Array.from(root.querySelectorAll("table.mdpeek-diff > tbody > tr, .mdpeek-rdiff > .mdpeek-rdiff-block"));
}

function changedUnits(oldHtml: string, newHtml: string): number[] {
  const parse = (h: string) => {
    const d = document.createElement("div");
    d.innerHTML = h;
    return diffUnits(d).map((e) => e.outerHTML);
  };
  const o = parse(oldHtml);
  const n = parse(newHtml);
  let a = 0;
  while (a < o.length && a < n.length && o[a] === n[a]) a++;
  let bo = o.length - 1;
  let bn = n.length - 1;
  while (bo >= a && bn >= a && o[bo] === n[bn]) {
    bo--;
    bn--;
  }
  const out: number[] = [];
  for (let i = a; i <= bn; i++) out.push(i);
  return out;
}

function fileLabel(path: string): { scope: string; rel: string } {
  const tree = getState().tree?.tree;
  let best: { name: string; branch?: string | null; root: string; files: { path: string; rel: string }[] } | null = null;
  for (const g of tree?.groups ?? []) {
    if (path === g.root || path.startsWith(g.root + "/")) if (!best || g.root.length > best.root.length) best = g;
  }
  const rel = best?.files.find((f) => f.path === path)?.rel ?? path.split("/").pop() ?? path;
  return { scope: best ? (best.branch ? `${best.name} (${best.branch})` : best.name) : "", rel };
}

function DiffView() {
  const diff = useStore((s) => s.diff)!;
  const html = useStore((s) => s.diffHtml);
  const bodyRef = useRef<HTMLDivElement>(null);
  const last = useRef<{ html: string; key: string } | null>(null);
  const key = `${diff.a}|${diff.b}|${diff.mode}|${diff.layout}`;

  useLayoutEffect(() => {
    const el = bodyRef.current;
    if (!el) return;
    const prev = last.current;
    const idx = prev && prev.key === key && prev.html ? changedUnits(prev.html, html) : [];
    el.innerHTML = html;
    last.current = { html, key };
    if (diff.mode === "rendered") enhance([el]);
    if (idx.length) {
      const units = diffUnits(el);
      flash(idx.map((i) => units[i]).filter(Boolean), "mp-changed", 2600);
    }
  }, [html, key, diff.mode]);

  const a = fileLabel(diff.a);
  const b = fileLabel(diff.b);
  return (
    <section aria-label="Comparison" className="flex min-h-full flex-col">
      <div className="sticky top-0 z-10 flex flex-wrap items-center gap-x-3 gap-y-2 border-b bg-background/95 px-4 py-2 backdrop-blur">
        <div className="flex min-w-0 flex-1 items-center gap-2 text-[13px]">
          <span className="min-w-0 truncate" data-testid="diff-a" title={diff.a}>
            <span className="text-danger">−</span> {a.scope && <span className="text-muted-foreground">{a.scope} </span>}
            {a.rel}
          </span>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label="Swap left and right"
            onClick={() => requestDiff({ ...diff, a: diff.b, b: diff.a })}
          >
            <ArrowLeftRight />
          </Button>
          <span className="min-w-0 truncate" data-testid="diff-b" title={diff.b}>
            <span className="text-ok">+</span> {b.scope && <span className="text-muted-foreground">{b.scope} </span>}
            {b.rel}
          </span>
        </div>
        <ToggleGroup
          type="single"
          variant="outline"
          size="sm"
          value={diff.mode}
          onValueChange={(v) => v && requestDiff({ ...diff, mode: v as "source" | "rendered" })}
          aria-label="Compare"
        >
          <ToggleGroupItem value="source" aria-label="Source">
            <Code /> Source
          </ToggleGroupItem>
          <ToggleGroupItem value="rendered" aria-label="Rendered">
            <Eye /> Rendered
          </ToggleGroupItem>
        </ToggleGroup>
        <ToggleGroup
          type="single"
          variant="outline"
          size="sm"
          value={diff.layout}
          onValueChange={(v) => v && requestDiff({ ...diff, layout: v as "unified" | "split" })}
          aria-label="Layout"
        >
          <ToggleGroupItem value="unified" aria-label="Unified">
            <Rows3 /> Unified
          </ToggleGroupItem>
          <ToggleGroupItem value="split" aria-label="Side by side">
            <Columns2 /> Side by side
          </ToggleGroupItem>
        </ToggleGroup>
        <Button variant="ghost" size="sm" onClick={closeDiff}>
          <X /> Close comparison
        </Button>
      </div>
      <div ref={bodyRef} className="flex-1 overflow-x-auto px-4 py-3" data-testid="diff-body" />
    </section>
  );
}
