// Reading-aid widgets: margin notes (Adaptive), the Split list, the "why this
// widget" popover, hidden-widget restore and the selector status.
import { useId, useState } from "react";
import { ChevronRight, EyeOff, Info, Pin, PinOff, RefreshCw, Undo2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { cn } from "cn";
import { PanelChrome, SourceLink } from "./index";
import { Render } from "../registry";
import { docPrefs, useStore } from "../lib/store";
import { fetchAdaptive, hideWidget, showWidget, togglePin } from "../lib/actions";
import { jumpToRange } from "../lib/nav";
import { WidgetKeyCtx } from "../lib/progress";
import type { AdaptiveLayout, Placement } from "../lib/types";
import { fallbackText, kindLabel, widgetKey } from "../lib/types";

const pct = (n: number) => `${Math.round(n * 100)}%`;

/** Placements to show, in reading order, pinned first; hidden ones removed. */
export function useVisiblePlacements(): { visible: Placement[]; hidden: Placement[]; pinned: Set<string> } {
  const layout = useStore((s) => s.layout);
  const dp = useStore(docPrefs);
  if (!layout) return { visible: [], hidden: [], pinned: new Set() };
  const pinned = new Set(dp.pinned);
  const hiddenSet = new Set(dp.hidden);
  const order = (p: Placement) => (p.scope === "document" ? 0 : p.anchor.range.start_line);
  const all = [...layout.placements].sort((a, b) => order(a) - order(b));
  const visible = all.filter((p) => !hiddenSet.has(widgetKey(p)));
  visible.sort((a, b) => Number(pinned.has(widgetKey(b))) - Number(pinned.has(widgetKey(a))));
  return { visible, hidden: all.filter((p) => hiddenSet.has(widgetKey(p))), pinned };
}

export function WhyPopover({ p, layout }: { p: Placement; layout: AdaptiveLayout }) {
  const sel = layout.selector;
  const alts = p.why.alternatives ?? [];
  const titleId = useId();
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button variant="ghost" size="icon-sm" className="size-7 text-muted-foreground max-md:size-11" aria-label={`Why this widget: ${kindLabel(p.node.kind)}`}>
          <Info />
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-80 text-[13px] leading-relaxed" aria-labelledby={titleId}>
        <h3 id={titleId} className="mb-2 font-semibold">Why {kindLabel(p.node.kind).toLowerCase()} here</h3>
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
          <dt className="text-muted-foreground">Chosen by</dt>
          <dd data-testid="why-provider">
            {p.why.by === "jev" || p.why.by === "fake" ? `${sel.provider}, model ${sel.model}` : p.why.by === "rules" ? `rules (${sel.model})` : p.why.by}
          </dd>
          <dt className="text-muted-foreground">Confidence</dt>
          <dd data-testid="why-confidence">{pct(p.why.confidence)}</dd>
          <dt className="text-muted-foreground">Helps</dt>
          <dd data-testid="why-helps">{pct(p.why.helps)}</dd>
          <dt className="text-muted-foreground">Reason</dt>
          <dd>{p.why.reason_code.replace(/_/g, " ")}{p.why.cached ? " (reused)" : ""}</dd>
          <dt className="text-muted-foreground">Other candidates</dt>
          <dd data-testid="why-alternatives">
            {alts.length === 0 ? "none" : alts.map(([k, v]) => `${kindLabel(k.split(":")[0])} ${pct(v)}`).join(", ")}
          </dd>
          <dt className="text-muted-foreground">Fallback</dt>
          <dd data-testid="why-fallback">{sel.fallback ? fallbackText(sel.fallback) : "none"}</dd>
          <dt className="text-muted-foreground">Source</dt>
          <dd>{p.scope === "document" ? "whole document" : `section “${p.anchor.heading}”`}</dd>
        </dl>
      </PopoverContent>
    </Popover>
  );
}

/** One widget as a margin note: source chip + title, then the content. */
export function WidgetNote({
  p,
  pinned,
  active,
  compact,
  collapsible,
}: {
  p: Placement;
  pinned: boolean;
  active: boolean;
  compact?: boolean;
  /** Start folded (whole-document aids in the margin stay out of the way). */
  collapsible?: boolean;
}) {
  const layout = useStore((s) => s.layout)!;
  const [open, setOpen] = useState(!collapsible);
  // A shell block reads better as "Commands" than the generic kind label.
  const title =
    p.node.kind === "ConfigViewer" && (p.node as { format?: string }).format === "shell" ? "Commands" : kindLabel(p.node.kind);
  const r = p.node.sourceRange ?? p.anchor.range;
  return (
    <section
      className={cn(
        "group/note relative border-l-2 pl-3 transition-colors",
        active ? "border-primary" : "border-border",
        compact ? "py-1" : "py-1.5",
      )}
      aria-label={`${title}${p.scope === "section" ? ` for ${p.anchor.heading}` : ""}`}
      aria-current={active ? "true" : undefined}
      data-widget={widgetKey(p)}
      data-kind={p.node.kind}
      data-active={active ? "true" : "false"}
    >
      <header className="mb-1 flex min-h-7 items-center gap-1">
        {collapsible && (
          <Button
            variant="ghost"
            size="icon-sm"
            className="-ml-1 size-7 text-muted-foreground max-md:size-11"
            aria-expanded={open}
            aria-label={open ? `Fold ${title}` : `Unfold ${title}`}
            onClick={() => setOpen(!open)}
          >
            <ChevronRight className={cn("transition-transform", open && "rotate-90")} />
          </Button>
        )}
        <SourceLink range={r} onJump={(rg) => jumpToRange(rg, p.anchor.heading)} />
        <h3 className="min-w-0 flex-1 truncate text-[13px] font-semibold">
          {title}
          {p.scope === "section" && <span className="font-normal text-muted-foreground"> in {p.anchor.heading}</span>}
        </h3>
        <WhyPopover p={p} layout={layout} />
        <Button
          variant="ghost"
          size="icon-sm"
          className={cn("size-7 max-md:size-11", pinned ? "text-primary" : "text-muted-foreground")}
          aria-pressed={pinned}
          aria-label={pinned ? `Unpin ${title}` : `Pin ${title}`}
          onClick={() => togglePin(p)}
        >
          {pinned ? <PinOff /> : <Pin />}
        </Button>
        <Button variant="ghost" size="icon-sm" className="size-7 text-muted-foreground max-md:size-11" aria-label={`Hide ${title}`} onClick={() => hideWidget(p)}>
          <EyeOff />
        </Button>
      </header>
      {open && (
        <PanelChrome.Provider value="bare">
          <WidgetKeyCtx.Provider value={widgetKey(p)}>
            <Render node={p.node} onJump={(rg) => jumpToRange(rg, p.anchor.heading)} />
          </WidgetKeyCtx.Provider>
        </PanelChrome.Provider>
      )}
    </section>
  );
}

export function HiddenWidgetsMenu() {
  const { hidden } = useVisiblePlacements();
  if (hidden.length === 0) return null;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="sm" className="h-7 px-2 text-muted-foreground max-md:h-11">
          <Undo2 /> Hidden ({hidden.length})
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuLabel>Show a hidden widget</DropdownMenuLabel>
        {hidden.map((p) => (
          <DropdownMenuItem key={widgetKey(p)} onSelect={() => showWidget(widgetKey(p))}>
            {kindLabel(p.node.kind)}
            {p.scope === "section" ? ` in ${p.anchor.heading}` : ""}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** Is `p` the widget for the section currently being read? */
export function isActive(p: Placement, activeLine: number | null): boolean {
  if (activeLine == null || p.scope !== "section") return false;
  return p.anchor.range.start_line <= activeLine && activeLine < p.anchor.range.end_line;
}

/** Empty / error / selecting messages shared by the Split pane and sheets. */
function WidgetsMessage() {
  const layout = useStore((s) => s.layout);
  const sel = useStore((s) => s.select);
  const err = useStore((s) => s.selectError);
  if (sel === "error" && err) {
    return (
      <div role="alert" className="text-[13px] text-danger">
        <p>{err}</p>
        <Button variant="outline" size="sm" className="mt-2 max-md:h-11" onClick={() => fetchAdaptive("reselect")}>
          <RefreshCw /> Try again
        </Button>
      </div>
    );
  }
  if (!layout) return <p className="text-[13px] text-muted-foreground">Choosing reading aids for this document…</p>;
  if (layout.placements.length === 0)
    return (
      <p className="text-[13px] text-muted-foreground" data-testid="widgets-empty">
        This document has no extra reading aids. The {layout.selector.provider} selector found nothing that would help beyond the text itself.
      </p>
    );
  return null;
}

/** The Split-mode widget list (also used in the mobile sheet). */
export function WidgetList() {
  const { visible, pinned } = useVisiblePlacements();
  const activeLine = useStore((s) => s.activeLine);
  const layout = useStore((s) => s.layout);
  return (
    <div className="flex flex-col gap-5">
      <WidgetsMessage />
      {layout &&
        visible.map((p) => <WidgetNote key={widgetKey(p)} p={p} pinned={pinned.has(widgetKey(p))} active={isActive(p, activeLine)} />)}
      {layout && layout.placements.length > 0 && visible.length === 0 && (
        <p className="text-[13px] text-muted-foreground">All widgets are hidden. Use Hidden to bring one back.</p>
      )}
    </div>
  );
}

/** Header row of the widget pane: doc type, re-select, hidden restore. */
/** `className` lets a sheet reserve room for its own Close button. */
export function WidgetPaneHeader({ className }: { className?: string } = {}) {
  const layout = useStore((s) => s.layout);
  const sel = useStore((s) => s.select);
  return (
    <div className={cn("flex min-h-11 items-center gap-2 border-b px-4", className)}>
      <h2 className="text-[13px] font-semibold">Reading aids</h2>
      {layout && (
        <span className="truncate text-[12px] text-muted-foreground" title="Document type">
          {layout.doc_type.value} ({pct(layout.doc_type.confidence)}, {layout.doc_type.by})
        </span>
      )}
      <span className="flex-1" />
      <HiddenWidgetsMenu />
      <Button
        variant="ghost"
        size="icon-sm"
        className="size-7 text-muted-foreground max-md:size-11"
        aria-label="Choose reading aids again"
        disabled={sel === "selecting"}
        onClick={() => fetchAdaptive("reselect")}
      >
        <RefreshCw className={sel === "selecting" ? "motion-safe:animate-spin" : ""} />
      </Button>
    </div>
  );
}
