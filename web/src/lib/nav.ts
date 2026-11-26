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
  setState({ sheet: null });
  els[0].scrollIntoView({ behavior: behavior(), block: "start" });
  flash(els, "mp-source-hit", 2400);
  // Move focus for keyboard and screen-reader users without scrolling again.
  els[0].setAttribute("tabindex", "-1");
  els[0].focus({ preventScroll: true });
  announce(`Showing lines ${range.start_line} to ${range.end_line}`);
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
