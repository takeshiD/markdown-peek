// Reading-aid controls in the top bar: one on/off toggle (a spinner while a
// model is choosing), the model for this session, stop / resume, "make
// default" (only while a non-default model is in use), and a status chip
// whose popover says who decided what.
import { useId } from "react";
import { AlertTriangle, ChevronDown, Loader2, Play, RotateCw, Sparkles, Square, Star } from "lucide-react";
import { cn } from "cn";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { Kbd } from "@/components/ui/kbd";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { aidsOn, useStore } from "../lib/store";
import { makeDefaultModel, runAids, setActiveModel, stopAids, toggleAids } from "../lib/actions";
import { CONFIG_ERRORS, errorShort, type JobState } from "../lib/types";

/** What the bar shows, from the toggle, the request state and the run. */
export type AidsView = "off" | "loading" | JobState;

export function useAidsView(): AidsView {
  const on = useStore(aidsOn);
  const sel = useStore((s) => s.select);
  const state = useStore((s) => s.layout?.progress.state);
  if (!on) return "off";
  if (!state) return sel === "error" ? "error" : "loading";
  return state;
}

/** `data-state` kept compatible with the earlier status chip. */
function chipState(v: AidsView): string {
  if (v === "running" || v === "loading") return "selecting";
  if (v === "done") return "ready";
  return v;
}

export function AidsToggle({ compact }: { compact?: boolean }) {
  const on = useStore(aidsOn);
  const view = useAidsView();
  const busy = view === "running";
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className={cn("h-8 gap-1.5 px-2 text-[13px] font-normal", on ? "bg-muted text-primary" : "text-muted-foreground", compact && "size-11 px-0")}
          aria-pressed={on}
          aria-busy={busy || undefined}
          aria-label={busy ? "Reading aids (choosing…)" : "Reading aids"}
          data-testid="aids-toggle"
          data-busy={busy ? "true" : "false"}
          onClick={toggleAids}
        >
          {busy ? <Loader2 className="motion-safe:animate-spin" aria-hidden="true" /> : <Sparkles aria-hidden="true" />}
          {!compact && <span className="max-lg:sr-only">Aids</span>}
        </Button>
      </TooltipTrigger>
      <TooltipContent>
        {on ? "Turn reading aids off" : "Turn reading aids on"} <Kbd>A</Kbd>
      </TooltipContent>
    </Tooltip>
  );
}

/** The models from the config; picking one applies to this session only. */
export function ModelMenu() {
  const models = useStore((s) => s.models);
  if (!models) return null;
  const active = models.models.find((m) => m.name === models.active);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className="h-8 min-w-0 shrink gap-1 px-2 text-[13px] font-normal text-muted-foreground"
          aria-label={`Model: ${models.active}${models.active === models.default ? " (default)" : ", for this session"}. Change model`}
          data-testid="model-menu"
        >
          {active?.problem && <AlertTriangle className="text-danger" aria-hidden="true" />}
          <span className="max-w-32 truncate">{models.active}</span>
          <ChevronDown className="size-3 opacity-60" aria-hidden="true" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-80">
        <DropdownMenuLabel>Model for this session</DropdownMenuLabel>
        <ModelRadioItems />
        <DropdownMenuSeparator />
        <p className="px-2 py-1.5 text-[12px] leading-snug text-muted-foreground">
          The default ({models.default}) is <code>[model] use</code> in the config. Switching here lasts until mdpeek restarts.
        </p>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** Radio items for the models (also used by the mobile menu). */
export function ModelRadioItems({ touch }: { touch?: boolean }) {
  const models = useStore((s) => s.models);
  if (!models) return null;
  return (
    <DropdownMenuRadioGroup value={models.active} onValueChange={(v) => void setActiveModel(v)}>
      {models.models.map((m) => (
        <DropdownMenuRadioItem
          key={m.name}
          value={m.name}
          disabled={!!m.problem}
          className={cn("items-start", touch && "min-h-11")}
          data-testid={`model-${m.name}`}
          aria-label={`${m.name} (${m.kind === "decision" ? "decision model" : "LLM"} ${m.model}${m.name === models.default ? ", default" : ""})${m.problem ? `: cannot be used, ${m.problem}` : ""}`}
        >
          <span className="flex min-w-0 flex-col">
            <span className="flex items-center gap-2">
              <span className="font-medium">{m.name}</span>
              <span className="text-[11.5px] text-muted-foreground">{m.kind === "decision" ? "decision model" : "LLM"}</span>
              {m.name === models.default && <span className="rounded border px-1 text-[11px] text-muted-foreground">default</span>}
            </span>
            <span className="truncate text-[12px] text-muted-foreground">{m.model}</span>
            {m.problem && (
              <span className="flex items-start gap-1 text-[12px] text-danger" data-testid={`model-problem-${m.name}`}>
                <AlertTriangle className="mt-0.5 size-3" aria-hidden="true" /> {m.problem}
              </span>
            )}
          </span>
        </DropdownMenuRadioItem>
      ))}
    </DropdownMenuRadioGroup>
  );
}

/** Shown only while a model other than the default is in use. */
export function MakeDefaultButton() {
  const models = useStore((s) => s.models);
  if (!models || models.active === models.default) return null;
  // A native title, not a Radix tooltip: this button disappears right after
  // it is pressed, and a tooltip closing under an open menu would close it.
  return (
    <Button
      variant="outline"
      size="sm"
      className="h-7 gap-1 px-2 text-[12.5px] font-normal"
      data-testid="make-default"
      title={`Save “${models.active}” as the default model${models.configPath ? ` in ${models.configPath}` : ""}`}
      aria-label={`Make ${models.active} the default model`}
      onClick={async () => {
        await makeDefaultModel();
      }}
    >
      <Star aria-hidden="true" /> <span className="max-xl:sr-only">Make default</span>
    </Button>
  );
}

/** Stop while choosing; resume / start / retry otherwise. */
export function RunButton({ touch }: { touch?: boolean }) {
  const view = useAidsView();
  const code = useStore((s) => s.layout?.selector.error ?? "");
  // Retrying cannot fix settings: the notification says what to change.
  if (view === "error" && CONFIG_ERRORS.has(code)) return null;
  const cls = cn("h-7 gap-1 px-2 text-[12.5px] font-normal", touch && "h-11");
  if (view === "running")
    return (
      <Button variant="outline" size="sm" className={cls} data-testid="aids-stop" aria-label="Stop choosing reading aids" onClick={() => void stopAids()}>
        <Square className="size-3 fill-current" aria-hidden="true" /> <span className="max-lg:sr-only">Stop</span>
      </Button>
    );
  if (view === "stopped" || view === "idle" || view === "error") {
    const label = view === "stopped" ? "Resume" : view === "idle" ? "Choose aids" : "Retry";
    const Icon = view === "error" ? RotateCw : Play;
    return (
      <Button variant="outline" size="sm" className={cls} data-testid="aids-resume" aria-label={`${label}: choose reading aids`} onClick={() => void runAids()}>
        <Icon aria-hidden="true" /> <span className="max-lg:sr-only">{label}</span>
      </Button>
    );
  }
  return null;
}

const STATUS_TEXT: Record<AidsView, string> = {
  off: "Off",
  loading: "Loading",
  idle: "Not started",
  running: "Choosing",
  stopped: "Stopped",
  done: "Ready",
  error: "Failed",
};

export function AidsStatus({ compact }: { compact?: boolean }) {
  const layout = useStore((s) => s.layout);
  const models = useStore((s) => s.models);
  const err = useStore((s) => s.selectError);
  const view = useAidsView();
  const titleId = useId();
  const p = layout?.progress;
  const sel = layout?.selector;
  const label =
    view === "running" && p
      ? `Choosing ${p.done}/${p.total}`
      : view === "error" && sel?.error
        ? `Failed (${errorShort(sel.error)})`
        : STATUS_TEXT[view];
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className={cn("h-8 min-w-0 shrink gap-2 px-2 font-normal text-muted-foreground", compact && "size-11 px-0")}
          data-testid="selector-status"
          data-state={chipState(view)}
          data-provider={sel?.name ?? ""}
          aria-label={`Reading aids: ${label}. Show details`}
        >
          <span
            aria-hidden="true"
            className={cn(
              "size-2 shrink-0 rounded-full",
              view === "done" && "bg-ok",
              (view === "stopped" || view === "idle") && "bg-change ring-1 ring-muted-foreground/40",
              view === "error" && "bg-danger",
              (view === "running" || view === "loading") && "bg-primary motion-safe:animate-pulse",
              view === "off" && "bg-muted-foreground/40",
            )}
          />
          {!compact && <span className="max-w-40 truncate tabular-nums max-xl:sr-only">{label}</span>}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-80 text-[13px] leading-relaxed" aria-labelledby={titleId}>
        <h3 id={titleId} className="mb-2 font-semibold">Reading aids</h3>
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
          <dt className="text-muted-foreground">Status</dt>
          <dd data-testid="status-text">
            {label}
            {view === "running" ? " (aids appear as they are chosen)" : ""}
          </dd>
          <dt className="text-muted-foreground">Model</dt>
          <dd data-testid="why-provider">
            {sel ? `${sel.name}, ${sel.kind === "decision" ? "decision model" : "LLM"} ${sel.model}` : (models?.active ?? "unknown")}
          </dd>
          {sel?.message && (
            <>
              <dt className="text-muted-foreground">Problem</dt>
              <dd className="text-danger" data-testid="selector-fallback">{sel.message}</dd>
            </>
          )}
          {layout && (
            <>
              <dt className="text-muted-foreground">Sent</dt>
              <dd>
                {sel!.requests === 0
                  ? "nothing"
                  : `${sel!.requests} request${sel!.requests > 1 ? "s" : ""} (headings, short excerpts, counts)`}
              </dd>
              <dt className="text-muted-foreground">Reused</dt>
              <dd>{layout.cache ? `${layout.cache.hits} cached decision${layout.cache.hits === 1 ? "" : "s"}` : "none"}</dd>
              <dt className="text-muted-foreground">Document type</dt>
              <dd>
                {layout.doc_type.value} ({Math.round(layout.doc_type.confidence * 100)}%, {layout.doc_type.by})
              </dd>
              <dt className="text-muted-foreground">Revision</dt>
              <dd>{layout.revision}</dd>
            </>
          )}
          {models?.cacheDir && (
            <>
              <dt className="text-muted-foreground">Cache</dt>
              <dd className="break-all">{models.cacheDir}</dd>
            </>
          )}
        </dl>
        {layout && layout.placements.length === 0 && view === "done" && (
          <p className="mt-2 text-muted-foreground" data-testid="widgets-empty">
            No reading aids for this document: {sel?.name ?? "the model"} found nothing that would help beyond the text itself.
          </p>
        )}
        {err && <p className="mt-2 text-danger">{err}</p>}
      </PopoverContent>
    </Popover>
  );
}

/** Everything for the desktop bar, in one group. */
export function AidsControls() {
  return (
    <div className="flex min-w-0 shrink items-center gap-1" role="group" aria-label="Reading aids" data-testid="aids-controls">
      <AidsToggle />
      <ModelMenu />
      <MakeDefaultButton />
      <AidsStatus />
      <RunButton />
    </div>
  );
}
