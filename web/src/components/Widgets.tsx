// Reading aids shown inside the document: the aid block itself, the "why this
// aid" popover, and the strip above the text (errors, hidden-aid restore).
import { useId, useState, type ReactElement } from "react";
import {
  CalendarRange,
  ChartColumn,
  ChartLine,
  ChevronRight,
  Columns2,
  Workflow,
  EyeOff,
  FileText,
  FolderTree,
  Footprints,
  Info,
  ListChecks,
  ListTodo,
  ListTree,
  type LucideIcon,
  MessageCircleQuestion,
  Loader2,
  Pin,
  PinOff,
  RefreshCw,
  Square,
  Scale,
  Sparkles,
  SquareTerminal,
  Table2,
  TextQuote,
  TriangleAlert,
  BookA,
  Undo2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
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
import { bootAids, hideWidget, showWidget, stopAids, toggleOriginal, togglePin } from "../lib/actions";
import { jumpToRange } from "../lib/nav";
import { WidgetKeyCtx } from "../lib/progress";
import type { AdaptiveLayout, Placement } from "../lib/types";
import { kindLabel, widgetKey } from "../lib/types";

const pct = (n: number) => `${Math.round(n * 100)}%`;

/** A short hover / focus hint for an icon action in a view's header. */
function ActionTip({ tip, children }: { tip: string; children: ReactElement }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>{children}</TooltipTrigger>
      <TooltipContent side="bottom">{tip}</TooltipContent>
    </Tooltip>
  );
}

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

const KIND_ICON: Record<string, LucideIcon> = {
  StepNavigator: Footprints,
  Checklist: ListChecks,
  ActionItems: ListTodo,
  DataTable: Table2,
  ConfigViewer: SquareTerminal,
  Timeline: CalendarRange,
  Callout: TriangleAlert,
  RiskPanel: TriangleAlert,
  SemanticOutline: ListTree,
  OpenQuestions: MessageCircleQuestion,
  DecisionLog: Scale,
  Glossary: BookA,
  Chart: ChartColumn,
  NodeGraph: Workflow,
  Comparison: Columns2,
  DirectoryTree: FolderTree,
  Verdicts: ListChecks,
};

function iconFor(p: Placement): LucideIcon {
  if (p.node.kind === "Chart") return p.node.chart === "line" ? ChartLine : ChartColumn;
  if (p.node.kind === "Callout") return TextQuote;
  return KIND_ICON[p.node.kind] ?? Sparkles;
}

export function WhyPopover({ p, layout }: { p: Placement; layout: AdaptiveLayout }) {
  const sel = layout.selector;
  const alts = p.why.alternatives ?? [];
  const titleId = useId();
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button variant="ghost" size="icon-sm" className="mp-note-action size-7 text-muted-foreground max-md:size-11" aria-label={`Why this view: ${kindLabel(p.node.kind)}`}>
          <Info />
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-80 text-[13px] leading-relaxed" aria-labelledby={titleId}>
        <h3 id={titleId} className="mb-2 font-semibold">Why {kindLabel(p.node.kind).toLowerCase()} here</h3>
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
          <dt className="text-muted-foreground">Chosen by</dt>
          <dd data-testid="why-provider">
            {p.why.by === sel.name ? `${sel.name}, model ${sel.model}` : p.why.by}
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
          <dt className="text-muted-foreground">Source</dt>
          <dd>{p.scope === "document" ? "whole document" : `section “${p.anchor.heading}”`}</dd>
        </dl>
      </PopoverContent>
    </Popover>
  );
}

/**
 * One reading aid, set into the text right after the lines it was built from
 * (or at the top when pinned). Whole-document aids start folded.
 */
export function WidgetNote({
  p,
  pinned,
  active,
  atTop,
  enter,
  replaces,
}: {
  p: Placement;
  pinned: boolean;
  active: boolean;
  /** Shown in the pinned strip at the top, away from its section. */
  atTop?: boolean;
  /** Just chosen by the model: animate in (value = stagger index). */
  enter?: number;
  /**
   * The aid stands in for its table / task list in the text (#63): "aid" while
   * it does, "original" when the reader switched back to the Markdown block.
   */
  replaces?: "aid" | "original";
}) {
  const layout = useStore((s) => s.layout)!;
  const [open, setOpen] = useState(p.scope !== "document");
  // A shell block reads better as "Commands" than the generic kind label.
  const title =
    p.node.kind === "ConfigViewer" && (p.node as { format?: string }).format === "shell" ? "Commands" : kindLabel(p.node.kind);
  const r = p.node.sourceRange ?? p.anchor.range;
  const Icon = iconFor(p);
  const where = p.scope === "section" && atTop ? p.anchor.heading : p.scope === "document" ? "whole document" : "";
  const showingOriginal = replaces === "original";
  const bodyOpen = open && !showingOriginal;
  return (
    <section
      className="mp-aid group/note"
      aria-label={`${title}${p.scope === "section" ? ` for ${p.anchor.heading}` : ""}`}
      aria-current={active ? "true" : undefined}
      data-widget={widgetKey(p)}
      data-kind={p.node.kind}
      data-scope={p.scope}
      data-active={active ? "true" : "false"}
      data-open={bodyOpen ? "true" : "false"}
      data-replaces={replaces}
      data-enter={enter != null ? "true" : undefined}
      style={enter != null ? ({ "--enter-delay": `${Math.min(enter, 6) * 70}ms` } as React.CSSProperties) : undefined}
      onAnimationEnd={(e) => {
        if (e.target === e.currentTarget && e.animationName === "mp-aid-glow") e.currentTarget.removeAttribute("data-enter");
      }}
    >
      <header className="mp-aid__head">
        {/* Accordion pattern: the heading holds the fold button. */}
        <h3 className="flex min-w-0 shrink">
          <Button
            variant="ghost"
            size="sm"
            className="mp-aid__toggle -ml-1.5 h-7 min-w-0 shrink overflow-hidden gap-1.5 px-1.5 text-[13px] font-semibold max-md:h-11"
            aria-expanded={bodyOpen}
            onClick={() => (showingOriginal ? toggleOriginal(p) : setOpen(!open))}
          >
            <ChevronRight className={cn("size-3.5 text-muted-foreground transition-transform motion-reduce:transition-none", bodyOpen && "rotate-90")} aria-hidden="true" />
            <Icon className="size-3.5 text-muted-foreground" aria-hidden="true" />
            <span className="shrink-0">{title}</span>
            {where && <span className="min-w-0 truncate text-[12.5px] font-normal text-muted-foreground">{where}</span>}
            {showingOriginal && <span className="truncate text-[12.5px] font-normal text-muted-foreground">original shown below</span>}
          </Button>
        </h3>
        <SourceLink range={r} onJump={(rg) => jumpToRange(rg, p.anchor.heading)} />
        <span className="flex-1" />
        {replaces && (
          <ActionTip tip={showingOriginal ? "Show as GenUI again" : "Show the original Markdown"}>
          <Button
            variant="ghost"
            size="sm"
            className={cn("mp-note-action h-7 gap-1 px-1.5 text-[12.5px] font-normal max-md:h-11 max-md:min-w-11", showingOriginal ? "text-primary" : "text-muted-foreground")}
            aria-pressed={showingOriginal}
            // A toggle keeps one name; aria-pressed carries the state (APG).
            aria-label={`Show the original ${title.toLowerCase()}`}
            onClick={() => toggleOriginal(p)}
            data-testid="show-original"
          >
            <FileText aria-hidden="true" /> <span className="max-sm:sr-only">Original</span>
          </Button>
          </ActionTip>
        )}
        <WhyPopover p={p} layout={layout} />
        <ActionTip tip={pinned ? "Unpin: back to its section" : "Pin: keep in view while you scroll"}>
        <Button
          variant="ghost"
          size="icon-sm"
          className={cn("mp-note-action size-7 max-md:size-11", pinned ? "text-primary" : "text-muted-foreground")}
          aria-pressed={pinned}
          aria-label={pinned ? `Unpin ${title}` : `Pin ${title}`}
          onClick={() => togglePin(p)}
        >
          {pinned ? <PinOff /> : <Pin />}
        </Button>
        </ActionTip>
        <ActionTip tip="Hide: restore later from “hidden”">
        <Button variant="ghost" size="icon-sm" className="mp-note-action size-7 text-muted-foreground max-md:size-11" aria-label={`Hide ${title}`} onClick={() => hideWidget(p)}>
          <EyeOff />
        </Button>
        </ActionTip>
      </header>
      {bodyOpen && (
        <div className="mp-aid__body">
          <PanelChrome.Provider value="bare">
            <WidgetKeyCtx.Provider value={widgetKey(p)}>
              <Render node={p.node} onJump={(rg) => jumpToRange(rg, p.anchor.heading)} />
            </WidgetKeyCtx.Provider>
          </PanelChrome.Provider>
        </div>
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
        <Button variant="ghost" size="sm" className="h-7 px-2 text-muted-foreground max-md:h-11" data-testid="hidden-aids">
          <Undo2 /> {hidden.length} hidden
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        <DropdownMenuLabel>Show a hidden view</DropdownMenuLabel>
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

/**
 * The strip above the text while aids are on: a spinner (with stop) while the
 * model is choosing, a failed fetch (with retry), and the way back to hidden
 * aids. Renders nothing when there is nothing to say.
 */
export function AidsStrip() {
  const layout = useStore((s) => s.layout);
  const sel = useStore((s) => s.select);
  const err = useStore((s) => s.selectError);
  const { hidden } = useVisiblePlacements();
  if (sel === "error" && err) {
    return (
      <div role="alert" className="mb-6 flex flex-wrap items-center gap-x-3 gap-y-1 border-l-2 border-danger pl-3 text-[13px] text-danger" data-testid="aids-error">
        <p className="min-w-0 flex-1">
          {err}
          {layout ? " The GenUI shown may be out of date." : ""}
        </p>
        <Button variant="outline" size="sm" className="max-md:h-11" onClick={() => void bootAids()}>
          <RefreshCw /> Try again
        </Button>
      </div>
    );
  }
  const running = layout?.progress.state === "running";
  if (!running && hidden.length === 0) return null;
  return (
    <div className="mb-4 flex flex-wrap items-center gap-x-3 gap-y-1 text-[12.5px] text-muted-foreground" data-testid="aids-strip">
      {running && (
        <span className="flex items-center gap-2" data-testid="aids-progress">
          <Loader2 className="size-3.5 text-primary motion-safe:animate-spin" aria-hidden="true" />
          <span className="tabular-nums">
            Choosing GenUI with {layout!.selector.name}… {layout!.progress.done} of {layout!.progress.total}
          </span>
          <Button variant="ghost" size="sm" className="h-7 gap-1 px-2 max-md:h-11" onClick={() => void stopAids()}>
            <Square className="size-3 fill-current" aria-hidden="true" /> Stop
          </Button>
        </span>
      )}
      <HiddenWidgetsMenu />
    </div>
  );
}
