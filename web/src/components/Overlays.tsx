// Overlays: command palette, keyboard shortcuts, Generated UI lens, guided
// reading panel, and the mobile sheets.
import { useEffect, useRef, useState } from "react";
import { EyeOff, FileText, Hash, Monitor, Moon, Pin, RefreshCw, Sun, Undo2 } from "lucide-react";
import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandShortcut,
} from "@/components/ui/command";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/components/ui/sheet";
import { Kbd } from "@/components/ui/kbd";
import { getState, setPrefs, setState, useStore } from "../lib/store";
import { fetchAdaptive, hideWidget, selectFile, setMode, setTheme, showWidget, togglePin } from "../lib/actions";
import { jumpToHeading } from "../lib/nav";
import { kindLabel, widgetKey } from "../lib/types";
import { RenderList } from "../registry";
import { paletteFilter, STRICT } from "../lib/paletteFilter";
import { enterScrolly, exitScrolly } from "../scrolly";
import { MOD, MODES } from "./Topbar";
import { Files, Outline, groupLabel } from "./Rail";
import { useVisiblePlacements, WidgetList, WidgetPaneHeader } from "./Widgets";
import type { UiNode } from "../ir";

interface PaletteEntry {
  id: string;
  group: string;
  /** Visible text; queries are ranked against it first (lib/paletteFilter). */
  label: string;
  extras?: string[];
  /** State-changing widget actions: never matched on scattered letters. */
  strict?: boolean;
  icon?: React.ReactNode;
  shortcut?: string;
  action: () => void;
}

export function CommandPalette() {
  const open = useStore((s) => s.paletteOpen);
  const tree = useStore((s) => s.tree);
  const by = useStore((s) => s.prefs.groupBy);
  const headings = useStore((s) => s.headings);
  const layout = useStore((s) => s.layout);
  const { visible, hidden, pinned } = useVisiblePlacements();
  const [query, setQuery] = useState("");
  const returnFocus = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (open) {
      returnFocus.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      setQuery("");
    }
  }, [open]);

  /** Put focus back where it was when the palette opened (or on the document). */
  const restoreFocus = () => {
    const el = returnFocus.current;
    if (el && el.isConnected && el !== document.body) el.focus();
    else document.getElementById("main")?.focus();
  };
  const run = (fn: () => void) => {
    setState({ paletteOpen: false });
    // Let the dialog close before acting; actions that move focus win.
    window.setTimeout(() => {
      restoreFocus();
      fn();
    }, 0);
  };

  const entries: PaletteEntry[] = [];
  for (const h of headings)
    entries.push({ id: `h:${h.id}`, group: "Headings", label: h.text, extras: ["heading", "go to"], icon: <Hash />, action: () => jumpToHeading(h.id) });
  for (const m of MODES)
    entries.push({ id: `m:${m.value}`, group: "View", label: `${m.label} mode`, extras: [m.hint], icon: <m.icon />, shortcut: m.key, action: () => setMode(m.value) });
  entries.push(
    { id: "t:light", group: "View", label: "Light theme", icon: <Sun />, action: () => setTheme("light") },
    { id: "t:dark", group: "View", label: "Dark theme", icon: <Moon />, action: () => setTheme("dark") },
    { id: "t:system", group: "View", label: "System theme", icon: <Monitor />, action: () => setTheme("system") },
    { id: "v:rail", group: "View", label: "Show or hide files and outline", shortcut: "[", action: () => setPrefs({ railCollapsed: !getState().prefs.railCollapsed }) },
    { id: "v:widgets", group: "View", label: "Show or hide the reading-aid pane", shortcut: "]", action: () => setPrefs({ widgetsCollapsed: !getState().prefs.widgetsCollapsed }) },
    { id: "v:guided", group: "View", label: "Guided reading", shortcut: "g", action: () => setState({ guidedOpen: !getState().guidedOpen }) },
    { id: "v:lens", group: "View", label: "Generated UI", action: () => setState({ lensOpen: true }) },
    { id: "v:follow", group: "View", label: "Follow live changes on or off", extras: ["autoscroll"], action: () => setPrefs({ autoscroll: !getState().prefs.autoscroll }) },
    { id: "w:reselect", group: "Reading aids", label: "Choose reading aids again", extras: ["reselect", "refresh"], icon: <RefreshCw />, action: () => void fetchAdaptive("reselect") },
  );
  if (layout) {
    const where = (p: (typeof visible)[number]) => (p.scope === "section" ? ` in ${p.anchor.heading}` : "");
    for (const p of visible)
      entries.push({ id: `pin:${widgetKey(p)}`, group: "Reading aids", label: `${pinned.has(widgetKey(p)) ? "Unpin" : "Pin"} ${kindLabel(p.node.kind)}${where(p)}`, strict: true, icon: <Pin />, action: () => togglePin(p) });
    for (const p of visible)
      entries.push({ id: `hide:${widgetKey(p)}`, group: "Reading aids", label: `Hide ${kindLabel(p.node.kind)}${where(p)}`, strict: true, icon: <EyeOff />, action: () => hideWidget(p) });
    for (const p of hidden)
      entries.push({ id: `show:${widgetKey(p)}`, group: "Reading aids", label: `Show ${kindLabel(p.node.kind)}${where(p)}`, extras: ["hidden", "restore"], strict: true, icon: <Undo2 />, action: () => showWidget(widgetKey(p)) });
  }
  for (const g of tree?.tree.groups.filter((x) => x.files.length) ?? [])
    for (const f of g.files)
      entries.push({ id: `f:${f.path}`, group: `Files in ${groupLabel(g, by)}`, label: f.rel, extras: [groupLabel(g, by), "open file"], icon: <FileText />, action: () => void selectFile(f.path) });

  // While typing: one list ranked by how well the label matches (prefix >
  // word start > substring > extras > scattered letters). Empty: grouped.
  const q = query.trim();
  const ranked = q
    ? entries
        .map((e, i) => ({ e, i, s: paletteFilter(e.label, q, [e.label, ...(e.extras ?? []), ...(e.strict ? [STRICT] : [])]) }))
        .filter((x) => x.s > 0)
        .sort((a, b) => b.s - a.s || a.i - b.i)
        .map((x) => x.e)
    : entries;
  const groups = q ? [["Results", ranked] as const] : Array.from(new Set(entries.map((e) => e.group))).map((g) => [g, entries.filter((e) => e.group === g)] as const);

  return (
    <CommandDialog
      open={open}
      onOpenChange={(o) => {
        setState({ paletteOpen: o });
        if (!o) window.setTimeout(restoreFocus, 0);
      }}
      shouldFilter={false}
      title="Command palette"
      description="Type to filter files, headings and commands; press Enter to run."
      className="top-[18%] translate-y-0 sm:max-w-xl"
    >
      <CommandInput placeholder="Go to a file or heading, or run a command…" data-testid="palette-input" value={query} onValueChange={setQuery} />
      <CommandList className="max-h-[60vh]">
        <CommandEmpty>Nothing matches.</CommandEmpty>
        {groups.map(([name, items]) =>
          items.length ? (
            <CommandGroup key={name} heading={name}>
              {items.map((e) => (
                <CommandItem key={e.id} value={e.id} onSelect={() => run(e.action)}>
                  {e.icon} <span className="truncate">{e.label}</span>
                  {q && e.group !== "View" && e.group !== "Results" && <span className="ml-auto truncate pl-2 text-[12px] text-muted-foreground">{e.group}</span>}
                  {e.shortcut && <CommandShortcut>{e.shortcut}</CommandShortcut>}
                </CommandItem>
              ))}
            </CommandGroup>
          ) : null,
        )}
      </CommandList>
    </CommandDialog>
  );
}

const SHORTCUTS: [string[], string][] = [
  [[`${MOD}`, "K"], "Command palette"],
  [["1"], "Read mode"],
  [["2"], "Split mode"],
  [["3"], "Adaptive mode"],
  [["["], "Show or hide files and outline"],
  [["]"], "Show or hide the reading-aid pane"],
  [["/"], "Filter headings"],
  [["g"], "Guided reading"],
  [["t"], "Next theme (light, dark, system)"],
  [["?"], "This list"],
  [["←", "→"], "Resize a focused pane divider"],
  [["Esc"], "Close a dialog, sheet or popover"],
];

export function ShortcutsDialog() {
  const open = useStore((s) => s.shortcutsOpen);
  return (
    <Dialog open={open} onOpenChange={(o) => setState({ shortcutsOpen: o })}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Keyboard shortcuts</DialogTitle>
          <DialogDescription>Single-key shortcuts work when the focus is not in a text field.</DialogDescription>
        </DialogHeader>
        <dl className="grid grid-cols-[auto_1fr] items-center gap-x-6 gap-y-2 text-[13px]">
          {SHORTCUTS.map(([keys, what]) => (
            <div key={what} className="contents">
              <dt className="flex gap-1">
                {keys.map((k) => (
                  <Kbd key={k}>{k}</Kbd>
                ))}
              </dt>
              <dd>{what}</dd>
            </div>
          ))}
        </dl>
      </DialogContent>
    </Dialog>
  );
}

/** Generated UI (`/api/gui` lens), unchanged backend, shown in a sheet. */
export function LensSheet() {
  const open = useStore((s) => s.lensOpen);
  const path = useStore((s) => s.doc.path);
  const [state, setLocal] = useState<{ status: "idle" | "loading" | "done" | "error"; nodes: UiNode[]; error?: string }>({ status: "idle", nodes: [] });
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setLocal({ status: "loading", nodes: [] });
    fetch("/api/gui", { cache: "no-store" })
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error(`server answered ${r.status}`))))
      .then((d: { nodes: UiNode[] }) => !cancelled && setLocal({ status: "done", nodes: d.nodes ?? [] }))
      .catch((e: Error) => !cancelled && setLocal({ status: "error", nodes: [], error: e.message }));
    return () => {
      cancelled = true;
    };
  }, [open, path]);
  return (
    <Sheet open={open} onOpenChange={(o) => setState({ lensOpen: o })}>
      <SheetContent side="right" className="w-full gap-0 sm:max-w-lg">
        <SheetHeader className="border-b pr-15">
          <SheetTitle>Generated UI</SheetTitle>
          <SheetDescription>Components generated from this document by the configured generator.</SheetDescription>
        </SheetHeader>
        <div className="min-h-0 flex-1 overflow-y-auto p-4" data-testid="lens-body" aria-busy={state.status === "loading"}>
          {state.status === "loading" && <p role="status">Generating components…</p>}
          {state.status === "error" && <p role="alert" className="text-danger">Could not generate components ({state.error}).</p>}
          {state.status === "done" && state.nodes.length === 0 && <p>No generated components for this document.</p>}
          {state.status === "done" && <RenderList nodes={state.nodes} onJump={() => setState({ lensOpen: false })} />}
        </div>
      </SheetContent>
    </Sheet>
  );
}

/** Guided reading: a non-modal panel (the reader keeps scrolling the text). */
export function GuidedPanel({ mobile }: { mobile: boolean }) {
  const open = useStore((s) => s.guidedOpen);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open || !ref.current) return;
    void enterScrolly(ref.current, () => setState({ guidedOpen: false }));
    return () => exitScrolly();
  }, [open]);
  if (!open) return null;
  return (
    <aside
      aria-label="Guided reading"
      data-testid="guided-panel"
      className={
        mobile
          ? "fixed inset-x-0 bottom-0 z-40 flex max-h-[55vh] flex-col border-t bg-background shadow-lg"
          : "fixed top-14 right-3 bottom-3 z-40 flex w-[360px] flex-col rounded-lg border bg-background shadow-lg"
      }
    >
      <div ref={ref} className="flex min-h-0 flex-1 flex-col" />
    </aside>
  );
}

/** Mobile sheets for Files / Outline / Widgets (R4). */
export function MobileSheets() {
  const sheet = useStore((s) => s.sheet);
  const close = (o: boolean) => !o && setState({ sheet: null });
  return (
    <>
      <Sheet open={sheet === "files" || sheet === "outline"} onOpenChange={close}>
        <SheetContent side="left" className="w-[88vw] max-w-sm gap-0 p-0">
          <SheetHeader className="border-b pr-15">
            <SheetTitle>{sheet === "files" ? "Files" : "Outline"}</SheetTitle>
            <SheetDescription className="sr-only">Switch between the outline of this document and other files.</SheetDescription>
            <div className="flex gap-2" role="group" aria-label="Show">
              <button
                type="button"
                className="min-h-11 flex-1 rounded-md border text-[14px] aria-pressed:bg-muted aria-pressed:font-medium"
                aria-pressed={sheet === "outline"}
                onClick={() => setState({ sheet: "outline" })}
              >
                Outline
              </button>
              <button
                type="button"
                className="min-h-11 flex-1 rounded-md border text-[14px] aria-pressed:bg-muted aria-pressed:font-medium"
                aria-pressed={sheet === "files"}
                onClick={() => setState({ sheet: "files" })}
              >
                Files
              </button>
            </div>
          </SheetHeader>
          <div className="min-h-0 flex-1 overflow-y-auto">{sheet === "files" ? <Files touch /> : <Outline touch />}</div>
        </SheetContent>
      </Sheet>
      <Sheet open={sheet === "widgets"} onOpenChange={close}>
        <SheetContent side="bottom" className="max-h-[85vh] gap-0 p-0">
          <SheetHeader className="sr-only">
            <SheetTitle>Reading aids</SheetTitle>
            <SheetDescription>Widgets chosen for this document, each linked to its source lines.</SheetDescription>
          </SheetHeader>
          <WidgetPaneHeader className="min-h-15 pr-15" />
          <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
            <WidgetList />
          </div>
        </SheetContent>
      </Sheet>
    </>
  );
}
