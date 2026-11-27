// The document outline on the right: docked as a resizable sidebar, or
// floating over the document (drag it by its grip, or move it with the arrow
// keys). The left rail keeps Files.
import { useRef, useState } from "react";
import { GripVertical, PanelRightClose, PictureInPicture2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { Kbd } from "@/components/ui/kbd";
import { announce, getState, setPrefs, useStore } from "../lib/store";
import { keepReadingPosition } from "../lib/nav";
import { Outline } from "./Rail";

export function setOutlineFloating(floating: boolean) {
  keepReadingPosition(() => setPrefs({ outlineFloating: floating, outlineCollapsed: false }));
  announce(floating ? "Outline floats over the document" : "Outline docked on the right");
}

export function toggleOutline() {
  keepReadingPosition(() => setPrefs({ outlineCollapsed: !getState().prefs.outlineCollapsed }));
}

function HeadButton({ label, onClick, children, kbd }: { label: string; onClick: () => void; children: React.ReactNode; kbd?: string }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="icon-sm" className="size-7 text-muted-foreground" aria-label={label} onClick={onClick}>
          {children}
        </Button>
      </TooltipTrigger>
      <TooltipContent>
        {label}
        {kbd && <Kbd>{kbd}</Kbd>}
      </TooltipContent>
    </Tooltip>
  );
}

/** Docked: the right sidebar of the desktop layout. */
export function DockedOutline() {
  return (
    <aside aria-label="Document outline" className="flex h-full min-w-0 flex-col overflow-hidden" data-testid="outline-pane">
      <div className="flex min-h-11 items-center gap-1 border-b pr-2 pl-4">
        <h2 className="flex-1 text-[13px] font-semibold">Outline</h2>
        <HeadButton label="Float the outline over the document" onClick={() => setOutlineFloating(true)}>
          <PictureInPicture2 />
        </HeadButton>
        <HeadButton label="Hide the outline" kbd="]" onClick={() => keepReadingPosition(() => setPrefs({ outlineCollapsed: true }))}>
          <PanelRightClose />
        </HeadButton>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        <Outline />
      </div>
    </aside>
  );
}

const W = 264;
const STEP = 24;

/** Floating: a small panel over the document column, placed where the reader drops it. */
export function FloatingOutline() {
  const saved = useStore((s) => s.prefs.outlineFloatPos);
  const [pos, setPos] = useState(saved);
  const box = useRef<HTMLElement>(null);
  const drag = useRef<{ x: number; y: number; right: number; top: number } | null>(null);

  const clamp = (p: { right: number; top: number }) => {
    const parent = box.current?.parentElement;
    const pw = parent?.clientWidth ?? 1000;
    const ph = parent?.clientHeight ?? 800;
    const bh = box.current?.offsetHeight ?? 200;
    return {
      right: Math.round(Math.max(8, Math.min(p.right, pw - W - 8))),
      top: Math.round(Math.max(8, Math.min(p.top, ph - Math.min(bh, 120)))),
    };
  };
  const commit = (p: { right: number; top: number }) => {
    const c = clamp(p);
    setPos(c);
    setPrefs({ outlineFloatPos: c });
  };

  return (
    <aside
      ref={box}
      aria-label="Document outline (floating)"
      data-testid="outline-float"
      className="absolute z-30 flex max-h-[min(60vh,520px)] flex-col overflow-hidden rounded-lg border bg-popover/95 shadow-lg backdrop-blur supports-[backdrop-filter]:bg-popover/85"
      style={{ width: W, right: `max(8px, min(${pos.right}px, calc(100% - ${W + 8}px)))`, top: `max(8px, min(${pos.top}px, calc(100% - 120px)))` }}
    >
      <div
        className="flex min-h-10 cursor-grab touch-none items-center gap-1 border-b pr-1.5 pl-1 select-none active:cursor-grabbing"
        onPointerDown={(e) => {
          if (e.button !== 0 || (e.target as HTMLElement).closest("button:not([data-grip])")) return;
          e.currentTarget.setPointerCapture(e.pointerId);
          drag.current = { x: e.clientX, y: e.clientY, right: pos.right, top: pos.top };
        }}
        onPointerMove={(e) => {
          const d = drag.current;
          if (!d) return;
          setPos(clamp({ right: d.right - (e.clientX - d.x), top: d.top + (e.clientY - d.y) }));
        }}
        onPointerUp={() => {
          if (!drag.current) return;
          drag.current = null;
          commit(pos);
        }}
        data-testid="outline-float-handle"
      >
        <Button
          variant="ghost"
          size="icon-sm"
          data-grip
          className="size-7 cursor-grab text-muted-foreground"
          aria-label="Move the outline (arrow keys)"
          onKeyDown={(e) => {
            const d = { ArrowLeft: [STEP, 0], ArrowRight: [-STEP, 0], ArrowUp: [0, -STEP], ArrowDown: [0, STEP] }[e.key];
            if (!d) return;
            e.preventDefault();
            commit({ right: pos.right + d[0], top: pos.top + d[1] });
          }}
        >
          <GripVertical />
        </Button>
        <h2 className="flex-1 text-[13px] font-semibold">Outline</h2>
        <HeadButton label="Dock the outline on the right" onClick={() => setOutlineFloating(false)}>
          <PanelRightClose className="rotate-180" />
        </HeadButton>
        <HeadButton label="Hide the outline" kbd="]" onClick={() => setPrefs({ outlineCollapsed: true })}>
          <X />
        </HeadButton>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        <Outline />
      </div>
    </aside>
  );
}
