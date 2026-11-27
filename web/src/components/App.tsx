// App shell: skip link, landmarks, top bar, resizable Files rail | document |
// Outline sidebar (desktop; the outline can also float over the document), or
// a single pane with sheets (mobile). Reading aids live inside the document.
import { useEffect, useRef, useSyncExternalStore } from "react";
import type { PanelImperativeHandle } from "react-resizable-panels";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { TooltipProvider } from "@/components/ui/tooltip";
import { aidsOn, getState, setPrefs, setState, useStore, type Prefs } from "../lib/store";
import { keepReadingPosition } from "../lib/nav";
import { applyTheme, setTheme, toggleAids } from "../lib/actions";
import { Topbar } from "./Topbar";
import { Rail } from "./Rail";
import { DocumentView } from "./DocumentView";
import { DockedOutline, FloatingOutline, toggleOutline } from "./OutlinePane";
import { CommandPalette, GuidedPanel, MobileSheets, ShortcutsDialog } from "./Overlays";
import { Toaster } from "./Toaster";

const MOBILE_QUERY = "(max-width: 767px)";
function useMobile(): boolean {
  return useSyncExternalStore(
    (cb) => {
      const m = window.matchMedia(MOBILE_QUERY);
      m.addEventListener("change", cb);
      return () => m.removeEventListener("change", cb);
    },
    () => window.matchMedia(MOBILE_QUERY).matches,
  );
}

function isTyping(t: EventTarget | null): boolean {
  const el = t as HTMLElement | null;
  if (!el) return false;
  return el.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName);
}

function useShortcuts(mobile: boolean) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setState({ paletteOpen: !getState().paletteOpen });
        return;
      }
      if (e.ctrlKey || e.metaKey || e.altKey || isTyping(e.target) || e.defaultPrevented) return;
      const s = getState();
      if (s.paletteOpen || s.shortcutsOpen || s.sheet) return;
      // Leave keys alone inside open menus / popovers (Radix handles them).
      if ((e.target as HTMLElement | null)?.closest?.("[role=menu],[role=dialog],[role=listbox],[data-radix-popper-content-wrapper]")) return;
      switch (e.key) {
        case "a":
          toggleAids();
          break;
        case "[":
          keepReadingPosition(() => setPrefs({ railCollapsed: !s.prefs.railCollapsed }));
          break;
        case "]":
          toggleOutline();
          break;
        case "g":
          setState({ guidedOpen: !s.guidedOpen });
          break;
        case "t": {
          const order = ["light", "dark", "system"] as const;
          setTheme(order[(order.indexOf(s.prefs.theme) + 1) % 3]);
          break;
        }
        case "?":
          setState({ shortcutsOpen: true });
          break;
        case "/": {
          if (mobile) {
            setState({ sheet: "outline" });
          } else {
            keepReadingPosition(() => setPrefs({ outlineCollapsed: false }));
          }
          window.setTimeout(() => document.querySelector<HTMLInputElement>("[data-testid=outline-filter]")?.focus(), 60);
          break;
        }
        default:
          return;
      }
      e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [mobile]);
}

function useTheme() {
  const theme = useStore((s) => s.prefs.theme);
  useEffect(() => {
    applyTheme();
    if (theme !== "system") return;
    const m = window.matchMedia("(prefers-color-scheme: dark)");
    m.addEventListener("change", applyTheme);
    return () => m.removeEventListener("change", applyTheme);
  }, [theme]);
}

function LiveRegion() {
  const text = useStore((s) => s.announce);
  return (
    <div className="sr-only" role="status" aria-live="polite" aria-atomic="true" data-testid="live-region">
      {text}
    </div>
  );
}

/** Keep a collapsible panel in sync with its `collapsed` preference. */
function useCollapse(ref: React.RefObject<PanelImperativeHandle | null>, collapsed: boolean, size: () => number, mode: string) {
  useEffect(() => {
    const p = ref.current;
    if (!p) return;
    if (collapsed && !p.isCollapsed()) p.collapse();
    if (!collapsed && (p.isCollapsed() || p.getSize().inPixels < 20)) {
      p.expand();
      if (p.getSize().inPixels < 20) p.resize(size());
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [collapsed, mode]);
}

function DesktopPanes() {
  const mode = useStore((s) => (aidsOn(s) ? "aids" : "plain"));
  const prefs = useStore((s) => s.prefs);
  // Initial sizes are read once per mount: changing defaultSize mid-drag
  // would fight the user's pointer.
  const initial = useRef<Prefs>(prefs);
  const rail = useRef<PanelImperativeHandle | null>(null);
  const outline = useRef<PanelImperativeHandle | null>(null);
  const floating = prefs.outlineFloating;

  useCollapse(rail, prefs.railCollapsed, () => getState().prefs.railSize, mode);
  // Floating: the docked panel folds away and the outline sits over the text.
  useCollapse(outline, prefs.outlineCollapsed || floating, () => getState().prefs.outlineSize, mode);

  // Persist sizes from the layout the group reports (percentages of the
  // group width). defaultSize is fixed per mount, so re-rendering during a
  // drag does not disturb it.
  const groupEl = useRef<HTMLDivElement | null>(null);
  const persist = (layout: Record<string, number>, meta: { isUserInteraction: boolean }) => {
    // Only user drags / separator keys are persisted here; collapse buttons
    // and `[` write prefs directly, and mount/constraint passes must not
    // overwrite the saved state.
    if (!meta.isUserInteraction) return;
    const width = groupEl.current?.offsetWidth ?? 0;
    if (!width) return;
    const patch: Partial<Prefs> = {};
    const px = (id: string) => Math.round(((layout[id] ?? 0) * width) / 100);
    const r = px("rail-panel");
    patch.railCollapsed = r < 20;
    if (r >= 20) patch.railSize = r;
    const o = px("outline-panel");
    if (o >= 20) {
      patch.outlineSize = o;
      patch.outlineCollapsed = false;
      // Dragging the docked panel open while floating docks it again.
      patch.outlineFloating = false;
    } else if (!getState().prefs.outlineFloating) patch.outlineCollapsed = true;
    const cur = getState().prefs;
    if (Object.entries(patch).some(([k, v]) => cur[k as keyof Prefs] !== v)) setPrefs(patch);
  };

  return (
    <ResizablePanelGroup
      orientation="horizontal"
      className="min-h-0 flex-1"
      elementRef={groupEl}
      onLayoutChanged={persist}
    >
      <ResizablePanel
        id="rail-panel"
        panelRef={rail}
        defaultSize={initial.current.railCollapsed ? 0 : initial.current.railSize}
        minSize={180}
        maxSize={480}
        collapsible
        collapsedSize={0}
        groupResizeBehavior="preserve-pixel-size"
      >
        <aside aria-label="Files" className="h-full min-w-0 overflow-hidden" data-testid="rail">
          <Rail />
        </aside>
      </ResizablePanel>
      <ResizableHandle aria-label="Resize files" data-testid="rail-handle" className="w-px hover:bg-primary/50 focus-visible:bg-primary" />
      <ResizablePanel id="doc-panel" minSize={320}>
        <div className="relative h-full">
          <DocumentView />
          {floating && !prefs.outlineCollapsed && <FloatingOutline />}
        </div>
      </ResizablePanel>
      <ResizableHandle aria-label="Resize outline" data-testid="outline-handle" className="w-px hover:bg-primary/50 focus-visible:bg-primary" />
      <ResizablePanel
        id="outline-panel"
        panelRef={outline}
        defaultSize={initial.current.outlineCollapsed || initial.current.outlineFloating ? 0 : initial.current.outlineSize}
        minSize={180}
        maxSize={420}
        collapsible
        collapsedSize={0}
        groupResizeBehavior="preserve-pixel-size"
      >
        <DockedOutline />
      </ResizablePanel>
    </ResizablePanelGroup>
  );
}

/** Keep the tab title on the open file (it changes without a page load). */
function useDocTitle() {
  const title = useStore((s) => s.doc.title);
  useEffect(() => {
    if (title) document.title = `${title} - mdpeek`;
  }, [title]);
}

export function App() {
  const mobile = useMobile();
  useTheme();
  useDocTitle();
  useShortcuts(mobile);
  return (
    <TooltipProvider delayDuration={400}>
      <div className="flex h-full flex-col" data-aids={useStore(aidsOn) ? "on" : "off"}>
        <a
          href="#main"
          className="sr-only z-50 rounded-md bg-primary px-3 py-2 text-primary-foreground focus:not-sr-only focus:fixed focus:top-2 focus:left-2"
          onClick={(e) => {
            e.preventDefault();
            document.getElementById("main")?.focus();
          }}
        >
          Skip to document
        </a>
        <Topbar mobile={mobile} />
        {mobile ? (
          <div className="min-h-0 flex-1">
            <DocumentView />
          </div>
        ) : (
          <DesktopPanes />
        )}
        {mobile && <MobileSheets />}
        <CommandPalette />
        <ShortcutsDialog />
        <GuidedPanel mobile={mobile} />
        <LiveRegion />
        <Toaster />
      </div>
    </TooltipProvider>
  );
}
