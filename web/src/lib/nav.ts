// Navigation inside the document column: source jumps and heading jumps.
import type { SourceRange } from "../ir";
import { blocksForRange, flash, headingFor, prefersReducedMotion } from "./body";
import { announce, setState } from "./store";

let article: HTMLElement | null = null;
let scroller: HTMLElement | null = null;

export function registerDocument(a: HTMLElement | null, s: HTMLElement | null) {
  article = a;
  scroller = s;
}

export function getScroller() {
  return scroller;
}

export function getArticle() {
  return article;
}

function behavior(): ScrollBehavior {
  return prefersReducedMotion() ? "auto" : "smooth";
}

/** Scroll the body to the blocks covering `range` and briefly mark them. */
export function jumpToRange(range: SourceRange, heading?: string) {
  if (!article) return;
  let els = blocksForRange(article, range.start_line, range.end_line);
  if (els.length === 0 && heading) {
    const h = headingFor(article, range.start_line, heading);
    if (h) els = [h];
  }
  if (els.length === 0) {
    announce(`Lines ${range.start_line} to ${range.end_line} are not in the rendered document`);
    return;
  }
  // A block an aid stands in for (#63) is hidden: show its aid instead.
  els = els.map((e) => replacementFor(e) ?? e).filter((e, i, a) => a.indexOf(e) === i);
  setState({ sheet: null });
  els[0].scrollIntoView({ behavior: behavior(), block: "start" });
  flash(els, "mp-source-hit", 2400);
  // Move focus for keyboard and screen-reader users without scrolling again.
  els[0].setAttribute("tabindex", "-1");
  els[0].focus({ preventScroll: true });
  announce(`Showing lines ${range.start_line} to ${range.end_line}`);
}

/** The aid slot standing in for a hidden body block, if any. */
function replacementFor(el: HTMLElement): HTMLElement | null {
  const key = el.dataset.replacedBy;
  if (!key || !article) return null;
  return article.querySelector<HTMLElement>(`:scope > [data-slot="${CSS.escape(key)}"]`);
}

export function jumpToHeading(id: string) {
  const el = article?.querySelector<HTMLElement>(`#${CSS.escape(id)}`);
  if (!el) return;
  setState({ sheet: null });
  el.scrollIntoView({ behavior: behavior(), block: "start" });
  el.setAttribute("tabindex", "-1");
  el.focus({ preventScroll: true });
  flash([el], "mp-source-hit", 1600);
}

/**
 * Run `change` (a mode switch, a pane collapse) without losing the reader's
 * place: remember the first source block at the top of the document column
 * and its offset, then put it back once the layout has settled. The document
 * column may be re-mounted by the change, so blocks are found again by their
 * source line.
 */
export function keepReadingPosition(change: () => void) {
  const a = article;
  const s = scroller;
  let restore = () => {};
  if (a && s) {
    const top = s.getBoundingClientRect().top;
    const blocks = Array.from(a.querySelectorAll<HTMLElement>("[data-line-start]"));
    const el = blocks.find((b) => b.getBoundingClientRect().bottom > top + 1);
    if (el && s.scrollTop > 0) {
      const line = el.dataset.lineStart;
      const offset = el.getBoundingClientRect().top - top;
      restore = () => {
        const a2 = article;
        const s2 = scroller;
        const t = a2?.querySelector<HTMLElement>(`[data-line-start="${line}"]`);
        if (!a2 || !s2 || !t) return;
        s2.scrollTop += t.getBoundingClientRect().top - s2.getBoundingClientRect().top - offset;
      };
    }
  }
  change();
  // Two frames for React to commit and layout; once more after pane
  // animations / container queries have settled.
  requestAnimationFrame(() => requestAnimationFrame(restore));
  setTimeout(restore, 260);
}
