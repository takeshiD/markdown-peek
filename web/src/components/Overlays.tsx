// Overlays: command palette, keyboard shortcuts, guided reading panel, and
// the mobile Files / Outline sheet.
import { useEffect, useRef, useState } from "react";
import { BookOpenText, Bot, EyeOff, FileText, Hash, Keyboard, LocateFixed, Monitor, Moon, PanelLeft, PanelTop, PictureInPicture2, Pin, Play, Sparkles, Square, Star, Sun, Undo2 } from "lucide-react";
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
import { aidsOn, getState, setPrefs, setState, useStore, type PinSide } from "../lib/store";
import { keepReadingPosition } from "../lib/nav";
import { hideWidget, makeDefaultModel, runAids, selectFile, setActiveModel, setPinSide, setTheme, showWidget, stopAids, toggleAids, togglePin } from "../lib/actions";
import { jumpToHeading } from "../lib/nav";
import { kindLabel, widgetKey } from "../lib/types";
import { paletteFilter, STRICT } from "../lib/paletteFilter";
import { enterScrolly, exitScrolly } from "../scrolly";
import { MOD } from "./Topbar";
import { Files, Outline, repoLabel } from "./Rail";
import { useVisiblePlacements } from "./Widgets";
import { setOutlineFloating, toggleOutline } from "./OutlinePane";

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
  const headings = useStore((s) => s.headings);
  const layout = useStore((s) => s.layout);
  const models = useStore((s) => s.models);
  const aids = useStore(aidsOn);
  const guided = useStore((s) => s.guidedOpen);
  const autoscroll = useStore((s) => s.prefs.autoscroll);
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
  entries.push({ id: "a:toggle", group: "GenUI", label: aids ? "Turn GenUI off" : "Turn GenUI on", extras: ["aids", "reading aids", "generate", "adaptive", "read"], icon: <Sparkles />, shortcut: "a", action: toggleAids });
  const st = layout?.progress.state;
  if (aids && st === "running") entries.push({ id: "a:stop", group: "GenUI", label: "Stop GenUI", extras: ["cancel"], icon: <Square />, action: () => void stopAids() });
  if (aids && (st === "stopped" || st === "idle" || st === "error"))
    entries.push({ id: "a:run", group: "GenUI", label: st === "stopped" ? "Resume GenUI" : "Run GenUI", extras: ["resume", "retry", "generate"], icon: <Play />, action: () => void runAids() });
  for (const m of models?.models ?? [])
    if (m.name !== models!.active && !m.problem)
      entries.push({ id: `model:${m.name}`, group: "GenUI", label: `Use model ${m.name}`, extras: [m.model, "switch model"], icon: <Bot />, action: () => void setActiveModel(m.name) });
  if (models && models.active !== models.default)
    entries.push({ id: "model:default", group: "GenUI", label: `Make ${models.active} the default model`, extras: ["save", "config"], icon: <Star />, action: () => void makeDefaultModel() });
  entries.push(
    { id: "t:light", group: "View", label: "Light theme", icon: <Sun />, action: () => setTheme("light") },
    { id: "t:dark", group: "View", label: "Dark theme", icon: <Moon />, action: () => setTheme("dark") },
    { id: "t:system", group: "View", label: "System theme", icon: <Monitor />, action: () => setTheme("system") },
    { id: "v:rail", group: "View", label: "Show or hide files", shortcut: "[", action: () => keepReadingPosition(() => setPrefs({ railCollapsed: !getState().prefs.railCollapsed })) },
    { id: "v:outline", group: "View", label: "Show or hide the outline", shortcut: "]", action: toggleOutline },
    { id: "v:float", group: "View", label: "Float or dock the outline", extras: ["floating", "pin outline", "picture in picture"], action: () => setOutlineFloating(!getState().prefs.outlineFloating) },
    { id: "v:guided", group: "View", label: guided ? "Close guided reading" : "Guided reading", extras: ["scrolly", "explain"], icon: <BookOpenText />, shortcut: "g", action: () => setState({ guidedOpen: !getState().guidedOpen }) },
    { id: "v:follow", group: "View", label: autoscroll ? "Stop following live changes" : "Follow live changes", extras: ["autoscroll", "live", "on", "off"], icon: <LocateFixed />, action: () => setPrefs({ autoscroll: !getState().prefs.autoscroll }) },
    { id: "v:shortcuts", group: "View", label: "Keyboard shortcuts", extras: ["keys", "help", "keybindings"], icon: <Keyboard />, shortcut: "?", action: () => setState({ shortcutsOpen: true }) },
  );
  if (layout) {
    const where = (p: (typeof visible)[number]) => (p.scope === "section" ? ` in ${p.anchor.heading}` : "");
    for (const p of visible)
      entries.push({ id: `pin:${widgetKey(p)}`, group: "GenUI", label: `${pinned.has(widgetKey(p)) ? "Unpin" : "Pin"} ${kindLabel(p.node.kind)}${where(p)}`, strict: true, icon: <Pin />, action: () => togglePin(p) });
    for (const p of visible)
      entries.push({ id: `hide:${widgetKey(p)}`, group: "GenUI", label: `Hide ${kindLabel(p.node.kind)}${where(p)}`, strict: true, icon: <EyeOff />, action: () => hideWidget(p) });
    const side = getState().prefs.pinSide;
    const places: [PinSide, string, React.ReactNode][] = [
      ["float", "Float the pinned views over the text", <PictureInPicture2 />],
      ["top", "Dock pinned views above the text", <PanelTop />],
      ["left", "Dock pinned views left of the text", <PanelLeft />],
    ];
    for (const [s, label, icon] of places)
      if (s !== side) entries.push({ id: `pin:side:${s}`, group: "GenUI", label, extras: ["pin", "dock", "pinned", "floating", s], icon, action: () => setPinSide(s) });
    for (const p of hidden)
      entries.push({ id: `show:${widgetKey(p)}`, group: "GenUI", label: `Show ${kindLabel(p.node.kind)}${where(p)}`, extras: ["hidden", "restore"], strict: true, icon: <Undo2 />, action: () => showWidget(widgetKey(p)) });
  }
  const repo = tree?.tree.groups[0];
  for (const f of repo?.files ?? [])
    entries.push({ id: `f:${f.path}`, group: `Files in ${repoLabel(repo!)}`, label: f.rel, extras: ["open file"], icon: <FileText />, action: () => void selectFile(f.path) });

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
  [["a"], "GenUI on or off"],
  [["["], "Show or hide files"],
  [["]"], "Show or hide the outline (dock or float it from its header)"],
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

/** Mobile sheet for Files / Outline (R4). */
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
    </>
  );
}
