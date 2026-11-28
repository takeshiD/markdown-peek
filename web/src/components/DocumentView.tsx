// The document column: front matter, the GFM body (server HTML, patched in
// place on live updates) and, in Adaptive mode, reading aids set into the
// text next to the lines they come from.
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { GripVertical, PanelLeft, PictureInPicture2, Pin } from "lucide-react";
import { cn } from "cn";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { setPinSide } from "../lib/actions";
import { setPrefs, type PinSide } from "../lib/store";
import { aidsOn, docPrefs, getState, setState, useStore } from "../lib/store";
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
import { keepReadingPosition, registerDocument } from "../lib/nav";
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
  const aids = useStore(aidsOn);
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

  // Pinned views live in a dock outside the scrolling text (above it or to
  // its left), so the text never slides under them and a tall view scrolls
  // in its own box.
  const pinSide = useStore((s) => s.prefs.pinSide);
  const { visible, pinned } = useVisiblePlacements();
  const pinnedCount = aids ? visible.filter((p) => pinned.has(widgetKey(p))).length : 0;
  const [dock, setDock] = useState<HTMLDivElement | null>(null);
  useFilesMakeRoom(pinnedCount > 0 && pinSide === "left");

  return (
    <div className="mp-docwrap" data-pin-side={pinSide}>
    {pinnedCount > 0 && <PinDock side={pinSide} count={pinnedCount} listRef={setDock} />}
    <main
      id="main"
      ref={scrollRef}
      tabIndex={-1}
      aria-label="Document"
      className="min-h-0 min-w-0 flex-1 overflow-y-auto overflow-x-hidden outline-none [overflow-anchor:auto]"
    >
      <div className={cn("mp-doc px-5 pt-6 pb-[50vh] sm:px-10 sm:pt-10", aids && "mp-adaptive")}>
        <div className="mx-auto max-w-(--measure) text-[16px]">
          {doc.error && (
            <p role="alert" className="mb-6 text-danger">
              {doc.error}
            </p>
          )}
          {aids && <AidsStrip />}
          {doc.frontmatter && <FrontMatter text={doc.frontmatter} />}
          <article ref={articleRef} className="markdown-body" lang={lang} aria-label={doc.title} />
          {aids && <InlineAids articleRef={articleRef} dock={pinnedCount > 0 ? dock : null} />}
        </div>
      </div>
    </main>
    </div>
  );
}

/**
 * The left dock takes the place of Files: while it is shown (desktop), the
 * Files rail is folded, and unfolded again when the dock goes away, unless the
 * reader had folded it already.
 */
function useFilesMakeRoom(leftDock: boolean) {
  useEffect(() => {
    if (!window.matchMedia?.("(min-width: 768px)").matches) return;
    const { railCollapsed, railFoldedForPins } = getState().prefs;
    if (leftDock) {
      if (!railCollapsed) keepReadingPosition(() => setPrefs({ railCollapsed: true, railFoldedForPins: true }));
    } else if (railFoldedForPins) {
      keepReadingPosition(() => setPrefs({ railCollapsed: false, railFoldedForPins: false }));
    }
  }, [leftDock]);
}

/** Floating panel width (it never exceeds the column). */
const FLOAT_W = 400;
const FLOAT_STEP = 24;

/**
 * Pinned views, outside the scrolling text: a floating panel over it (drag it
 * by its bar, or move it with the arrow keys on the grip), or a dock to its
 * left. The header holds the count and the place switch.
 */
function PinDock({ side, count, listRef }: { side: PinSide; count: number; listRef: (el: HTMLDivElement | null) => void }) {
  const saved = useStore((s) => s.prefs.pinFloatPos);
  const [pos, setPos] = useState(saved);
  const box = useRef<HTMLElement>(null);
  const drag = useRef<{ x: number; y: number; left: number; top: number } | null>(null);
  const float = side === "float";
  const clamp = (p: { left: number; top: number }) => {
    const parent = box.current?.parentElement;
    const pw = parent?.clientWidth ?? 1000;
    const ph = parent?.clientHeight ?? 800;
    const bw = box.current?.offsetWidth ?? FLOAT_W;
    return {
      left: Math.round(Math.max(8, Math.min(p.left, pw - bw - 8))),
      top: Math.round(Math.max(8, Math.min(p.top, ph - 80))),
    };
  };
  const commit = (p: { left: number; top: number }) => {
    const c = clamp(p);
    setPos(c);
    setPrefs({ pinFloatPos: c });
  };
  const sideButton = (s: PinSide, label: string, icon: React.ReactNode) => (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size="icon-sm"
          className={cn("size-7", side === s ? "bg-muted text-foreground" : "text-muted-foreground")}
          aria-label={label}
          aria-pressed={side === s}
          onClick={() => setPinSide(s)}
        >
          {icon}
        </Button>
      </TooltipTrigger>
      <TooltipContent side="bottom">{label}</TooltipContent>
    </Tooltip>
  );
  return (
    <section
      ref={box}
      className="mp-dock"
      data-side={side}
      aria-label="Pinned GenUI views"
      data-testid="pinned-aids"
      style={float ? { left: `max(8px, min(${pos.left}px, calc(100% - min(${FLOAT_W}px, 100% - 16px) - 8px)))`, top: `max(8px, min(${pos.top}px, calc(100% - 80px)))` } : undefined}
    >
      <div
        className="mp-dock__bar"
        data-testid="pinned-bar"
        onPointerDown={(e) => {
          if (!float || e.button !== 0 || (e.target as HTMLElement).closest("button:not([data-grip])")) return;
          e.currentTarget.setPointerCapture(e.pointerId);
          drag.current = { x: e.clientX, y: e.clientY, left: pos.left, top: pos.top };
        }}
        onPointerMove={(e) => {
          const d = drag.current;
          if (!d) return;
          setPos(clamp({ left: d.left + (e.clientX - d.x), top: d.top + (e.clientY - d.y) }));
        }}
        onPointerUp={() => {
          if (!drag.current) return;
          drag.current = null;
          commit(pos);
        }}
      >
        {float ? (
          <Button
            variant="ghost"
            size="icon-sm"
            data-grip
            className="-ml-1.5 size-7 cursor-grab text-muted-foreground max-md:hidden"
            aria-label="Move the pinned views (arrow keys)"
            onKeyDown={(e) => {
              const d = ({ ArrowLeft: [-FLOAT_STEP, 0], ArrowRight: [FLOAT_STEP, 0], ArrowUp: [0, -FLOAT_STEP], ArrowDown: [0, FLOAT_STEP] } as Record<string, number[]>)[e.key];
              if (!d) return;
              e.preventDefault();
              commit({ left: pos.left + d[0], top: pos.top + d[1] });
            }}
          >
            <GripVertical />
          </Button>
        ) : null}
        <Pin className={cn("size-3.5", float && "md:hidden")} aria-hidden="true" />
        <span className="font-medium text-foreground">Pinned</span>
        <span>{count}</span>
        <span className="flex-1" />
        <span className="flex items-center gap-0.5 max-md:hidden">
          {sideButton("float", "Float the pinned views over the text", <PictureInPicture2 />)}
          {sideButton("left", "Dock pinned views left of the text", <PanelLeft />)}
        </span>
      </div>
      <div ref={listRef} className="mp-dock__list" data-testid="pinned-list" />
    </section>
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
function InlineAids({ articleRef, dock }: { articleRef: React.RefObject<HTMLElement | null>; dock: HTMLElement | null }) {
  const { visible, pinned } = useVisiblePlacements();
  const activeLine = useStore((s) => s.activeLine);
  const originalList = useStore(docPrefs).original;
  const original = new Set(originalList ?? []);
  const slots = useRef(new Map<string, HTMLDivElement>());
  const [, force] = useState(0);
  // Aids that actually stand in for their block right now (#63).
  const [replacing, setReplacing] = useState("");
  // Source lines are part of the key: an edit that grows a table must place
  // (and replace, #63) again once the new layout arrives.
  const keys = visible
    .map((p) => {
      const r = p.node.sourceRange;
      return `${widgetKey(p)}${pinned.has(widgetKey(p)) ? "^" : ""}${original.has(widgetKey(p)) ? "o" : ""}@${r ? `${r.start_line}-${r.end_line}` : ""}`;
    })
    .join("|");
  const entering = useEntering(visible, (k) => slots.current.has(k));

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
    const place = () => {
      const r = placeSlots(article, visible, pinned, original, map, dock);
      const joined = [...r].sort().join("|");
      setReplacing((prev) => (prev === joined ? prev : joined));
    };
    place();
    window.addEventListener(BODY_EVENT, place);
    if (created) force((n) => n + 1);
    return () => window.removeEventListener(BODY_EVENT, place);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [keys, dock]);

  useEffect(
    () => () => {
      for (const el of slots.current.values()) el.remove();
      slots.current.clear();
      const article = articleRef.current;
      // Reading aids off: the text shows its own tables and lists again.
      if (article) unreplace(article);
    },
    [],
  );

  const replacingSet = new Set(replacing.split("|"));
  return (
    <>
      {visible.map((p) => {
        const el = slots.current.get(widgetKey(p));
        if (!el) return null;
        const pin = pinned.has(widgetKey(p));
        return createPortal(
          <WidgetNote
            p={p}
            pinned={pin}
            active={isActive(p, activeLine)}
            atTop={pin}
            enter={entering.get(widgetKey(p))}
            replaces={!pin && replacingSet.has(widgetKey(p)) ? (original.has(widgetKey(p)) ? "original" : "aid") : undefined}
          />,
          el,
          widgetKey(p),
        );
      })}
    </>
  );
}

/**
 * Aids that just arrived from the model animate in (a short overshoot, staggered
 * within one batch), so the reader sees what was added. Aids already known when
 * the document opened, or reused from the cache, appear without motion.
 * Returns the stagger index of each entering aid.
 */
function useEntering(visible: Placement[], mounted: (key: string) => boolean): Map<string, number> {
  const docPath = useStore((s) => s.doc.path);
  // A layout fetched over HTTP is what was already there (page load, file
  // switch); only pushes from a run bring new aids.
  const pushed = useStore((s) => s.layoutFrom === "ws");
  const seen = useRef<{ path: string; keys: Set<string> }>({ path: docPath, keys: new Set() });
  if (seen.current.path !== docPath) seen.current = { path: docPath, keys: new Set() };
  const out = new Map<string, number>();
  let i = 0;
  if (pushed)
    for (const p of visible) {
      const k = widgetKey(p);
      if (!seen.current.keys.has(k) && !p.why.cached) out.set(k, i++);
    }
  // Seen once actually rendered: a new aid's slot is created after the render
  // that first lists it, so only keys that had a slot during this render count.
  const rendered = visible.map(widgetKey).filter(mounted);
  useEffect(() => {
    for (const k of rendered) seen.current.keys.add(k);
  });
  return out;
}

/**
 * Where each slot goes; slots aimed at the same spot keep their order. Pinned
 * slots go into the dock outside the text. Returns the views that stand in for
 * their block (#63).
 */
function placeSlots(
  article: HTMLElement,
  visible: Placement[],
  pinned: Set<string>,
  original: Set<string>,
  map: Map<string, HTMLDivElement>,
  dock: HTMLElement | null,
): Set<string> {
  unreplace(article);
  const replacing = new Set<string>();
  const blocks = bodyBlocks(article);
  const secondHeading = blocks.find((b, i) => i > 0 && /^H[1-6]$/.test(b.tagName)) ?? null;
  for (const p of visible) {
    const el = map.get(widgetKey(p));
    if (!el) continue;
    el.dataset.place = pinned.has(widgetKey(p)) ? "top" : p.scope;
    if (pinned.has(widgetKey(p)) && dock) {
      dock.appendChild(el);
      continue;
    }
    // A table / task list shown as an aid takes the place of its block (#63);
    // the block is only hidden (display), and comes back with "Original".
    const replaced = canReplace(p) ? replacedBlocks(blocks, p) : [];
    if (replaced.length) {
      replacing.add(widgetKey(p));
      el.dataset.place = "replace";
      article.insertBefore(el, replaced[0]);
      if (!original.has(widgetKey(p)))
        for (const b of replaced) {
          b.classList.add(REPLACED_CLASS);
          b.dataset.replacedBy = widgetKey(p);
        }
      continue;
    }
    let before: Element | null;
    if (p.scope === "document" || blocks.length === 0) before = secondHeading;
    else before = afterSource(blocks, p);
    article.insertBefore(el, before);
  }
  return replacing;
}

/** Aids that stand in for the block they were built from (#63). */
const REPLACING_KINDS = new Set(["DataTable", "Checklist"]);
const REPLACED_CLASS = "mp-replaced";

function canReplace(p: Placement): boolean {
  return p.scope === "section" && REPLACING_KINDS.has(p.node.kind);
}

/** Show every block an aid stood in for. */
function unreplace(article: HTMLElement) {
  for (const b of Array.from(article.querySelectorAll<HTMLElement>(`:scope > .${REPLACED_CLASS}`))) {
    b.classList.remove(REPLACED_CLASS);
    delete b.dataset.replacedBy;
  }
}

/**
 * The body blocks an aid shows in full: blocks starting inside the aid's
 * source lines, of the right element, whose rows / items are exactly the
 * aid's. Anything else (a list mixing tasks and plain items, a table the aid
 * shows only part of) keeps its block, and the aid sits after it as before.
 */
function replacedBlocks(blocks: HTMLElement[], p: Placement): HTMLElement[] {
  const r = p.node.sourceRange;
  if (!r) return [];
  const last = r.end_column === 1 && r.end_line > r.start_line ? r.end_line - 1 : r.end_line;
  const inside = blocks.filter((b) => {
    const s = Number(b.dataset.lineStart);
    const e = Number(b.dataset.lineEnd);
    return Number.isFinite(s) && s >= r.start_line && s <= last && e <= last + 1;
  });
  if (inside.length === 0) return [];
  if (p.node.kind === "DataTable") {
    if (inside.length !== 1) return [];
    const table = inside[0].matches("table") ? inside[0] : inside[0].querySelector(":scope > table");
    if (!table) return [];
    const rows = table.querySelectorAll(":scope > tbody > tr").length;
    return rows === p.node.rows.length ? inside : [];
  }
  if (p.node.kind === "Checklist") {
    let items = 0;
    for (const b of inside) {
      if (!b.matches("ul, ol")) return [];
      const lis = Array.from(b.querySelectorAll(":scope > li"));
      if (lis.some((li) => !li.querySelector(':scope > input[type="checkbox"], :scope > p > input[type="checkbox"]'))) return [];
      items += lis.length;
    }
    return items === p.node.items.length ? inside : [];
  }
  return [];
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
