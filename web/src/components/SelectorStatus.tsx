// Always-visible selector state (R7): which selector (rules / jev / fake),
// whether it is choosing, fell back, or failed. Details in a popover.
import { useId } from "react";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Button } from "@/components/ui/button";
import { RefreshCw } from "lucide-react";
import { cn } from "cn";
import { useStore } from "../lib/store";
import { fetchAdaptive } from "../lib/actions";
import { fallbackText } from "../lib/types";

export function SelectorStatus({ compact }: { compact?: boolean }) {
  const layout = useStore((s) => s.layout);
  const sel = useStore((s) => s.select);
  const err = useStore((s) => s.selectError);
  const provider = layout?.selector.provider ?? "…";
  const fallback = layout?.selector.fallback ?? null;
  const titleId = useId();
  const state =
    sel === "error" ? "error" : sel === "selecting" ? "selecting" : fallback ? "fallback" : "ready";
  const label =
    state === "error"
      ? "Reading aids unavailable"
      : state === "selecting"
        ? layout
          ? `${provider}, updating`
          : "Choosing"
        : state === "fallback"
          ? `${provider}, fallback`
          : provider;
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className={cn("h-8 gap-2 px-2 font-normal text-muted-foreground", compact && "size-11 px-0")}
          data-testid="selector-status"
          data-state={state}
          data-provider={layout?.selector.provider ?? ""}
          aria-label={`Selector: ${label}. Show details`}
        >
          <span
            aria-hidden="true"
            className={cn(
              "size-2 shrink-0 rounded-full",
              state === "ready" && "bg-ok",
              state === "fallback" && "bg-change ring-1 ring-muted-foreground/40",
              state === "error" && "bg-danger",
              state === "selecting" && "bg-primary motion-safe:animate-pulse",
            )}
          />
          {!compact && <span className="max-w-40 truncate">{label}</span>}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-80 text-[13px] leading-relaxed" aria-labelledby={titleId}>
        <h3 id={titleId} className="mb-2 font-semibold">Reading-aid selector</h3>
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
          <dt className="text-muted-foreground">Status</dt>
          <dd>{state === "selecting" ? "Choosing (previous result stays visible)" : state === "error" ? "Failed" : state === "fallback" ? "Fell back" : "Ready"}</dd>
          <dt className="text-muted-foreground">Decided by</dt>
          <dd>{layout ? `${layout.selector.provider}, model ${layout.selector.model}` : "not yet"}</dd>
          <dt className="text-muted-foreground">Configured</dt>
          <dd>{layout?.selector.requested ?? "unknown"}</dd>
          {fallback && (
            <>
              <dt className="text-muted-foreground">Fallback</dt>
              <dd data-testid="selector-fallback">{fallbackText(fallback)}. Showing the rules choice.</dd>
            </>
          )}
          {layout && (
            <>
              <dt className="text-muted-foreground">Sent to Jev</dt>
              <dd>{layout.selector.requests === 0 ? "nothing" : `${layout.selector.requests} request(s)`}</dd>
              <dt className="text-muted-foreground">Revision</dt>
              <dd>{layout.revision}</dd>
            </>
          )}
        </dl>
        {err && <p className="mt-2 text-danger">{err}</p>}
        <Button variant="outline" size="sm" className="mt-3" onClick={() => fetchAdaptive("reselect")}>
          <RefreshCw /> Choose again
        </Button>
      </PopoverContent>
    </Popover>
  );
}
