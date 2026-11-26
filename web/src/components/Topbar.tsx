// Top bar: breadcrumb (repository / file), mode switch, selector status,
// command palette, theme, and view toggles.
import {
  BookOpenText,
  ChevronDown,
  Eye,
  Columns2,
  Command as CommandIcon,
  FileText,
  Folder,
  GitBranch,
  Keyboard,
  LocateFixed,
  Menu,
  Monitor,
  Moon,
  MoreHorizontal,
  PanelLeft,
  PanelRight,
  Rows3,
  Sparkles,
  Sun,
  TextQuote,
} from "lucide-react";
import { cn } from "cn";
import { Button } from "@/components/ui/button";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { Kbd } from "@/components/ui/kbd";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuShortcut,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { getState, setPrefs, setState, useStore } from "../lib/store";
import { keepReadingPosition } from "../lib/nav";
import { selectFile, setMode, setTheme } from "../lib/actions";
import type { Group, Mode, ThemePref } from "../lib/types";
import { SelectorStatus } from "./SelectorStatus";
import { groupLabel } from "./Rail";

export const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);
export const MOD = isMac ? "⌘" : "Ctrl";

export const MODES: { value: Mode; label: string; key: string; icon: typeof TextQuote; hint: string }[] = [
  { value: "read", label: "Read", key: "1", icon: TextQuote, hint: "Document only" },
  { value: "split", label: "Split", key: "2", icon: Columns2, hint: "Widgets in a side pane" },
  { value: "adaptive", label: "Adaptive", key: "3", icon: Rows3, hint: "Widgets beside their sections" },
];

export function groupForPath(groups: Group[], path: string): Group | null {
  let best: Group | null = null;
  for (const g of groups) if ((path === g.root || path.startsWith(g.root + "/")) && (!best || g.root.length > best.root.length)) best = g;
  return best;
}

function Breadcrumb({ touch }: { touch?: boolean }) {
  const tree = useStore((s) => s.tree);
  const path = useStore((s) => s.doc.path);
  const title = useStore((s) => s.doc.title);
  const by = useStore((s) => s.prefs.groupBy);
  const groups = tree?.tree.groups.filter((g) => g.files.length) ?? [];
  const group = groupForPath(groups, path);
  const rel = group?.files.find((f) => f.path === path)?.rel ?? title;
  const switchGroup = (g: Group) => {
    const same = g.files.find((f) => f.rel === rel) ?? g.files[0];
    if (same) void selectFile(same.path);
  };
  return (
    <nav aria-label="Breadcrumb" className="flex min-w-0 flex-1 items-center gap-0.5 overflow-hidden text-[13px]">
      {group && !touch && (
        <div className="flex min-w-0 shrink items-center gap-0.5 max-lg:hidden">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="sm" className="h-8 min-w-0 max-w-48 shrink gap-1.5 px-2 font-medium" aria-label={`Repository: ${groupLabel(group, by)}. Switch ${by}`}>
                {by === "branch" ? <GitBranch aria-hidden="true" /> : <Folder aria-hidden="true" />}
                <span className="truncate">{groupLabel(group, by)}</span>
                <ChevronDown className="size-3 opacity-60" aria-hidden="true" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="max-h-80 overflow-auto">
              <DropdownMenuLabel>Switch {by}</DropdownMenuLabel>
              <DropdownMenuRadioGroup value={group.root} onValueChange={(v) => { const g = groups.find((x) => x.root === v); if (g) switchGroup(g); }}>
                {groups.map((g) => (
                  <DropdownMenuRadioItem key={g.root} value={g.root}>
                    {groupLabel(g, by)}
                  </DropdownMenuRadioItem>
                ))}
              </DropdownMenuRadioGroup>
            </DropdownMenuContent>
          </DropdownMenu>
          <span className="text-muted-foreground/60" aria-hidden="true">
            /
          </span>
        </div>
      )}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="sm"
            className={cn("h-8 min-w-0 max-w-full shrink gap-1.5 px-2 font-normal", touch && "h-11")}
            aria-label={`File: ${rel}. Switch file`}
            data-testid="breadcrumb-file"
          >
            <FileText className="text-muted-foreground" aria-hidden="true" />
            <span className="truncate">{rel}</span>
            <ChevronDown className="size-3 opacity-60" aria-hidden="true" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="max-h-96 max-w-[90vw] overflow-auto">
          <DropdownMenuLabel>Switch file{group ? ` in ${groupLabel(group, by)}` : ""}</DropdownMenuLabel>
          <DropdownMenuRadioGroup value={path} onValueChange={(v) => void selectFile(v)}>
            {(group?.files ?? []).map((f) => (
              <DropdownMenuRadioItem key={f.path} value={f.path} className={cn(touch && "min-h-11")}>
                {f.rel}
              </DropdownMenuRadioItem>
            ))}
          </DropdownMenuRadioGroup>
        </DropdownMenuContent>
      </DropdownMenu>
    </nav>
  );
}

export function ModeSwitch() {
  const mode = useStore((s) => s.prefs.mode);
  return (
    <ToggleGroup
      type="single"
      variant="outline"
      size="sm"
      value={mode}
      onValueChange={(v) => v && setMode(v as Mode)}
      aria-label="View mode"
      data-testid="mode-switch"
    >
      {MODES.map((m) => (
        <Tooltip key={m.value}>
          <TooltipTrigger asChild>
            <ToggleGroupItem value={m.value} aria-label={`${m.label} mode`} className="h-8 px-2.5 text-[13px]">
              <m.icon aria-hidden="true" /> <span className="max-lg:sr-only">{m.label}</span>
            </ToggleGroupItem>
          </TooltipTrigger>
          <TooltipContent>
            {m.hint} <Kbd>{m.key}</Kbd>
          </TooltipContent>
        </Tooltip>
      ))}
    </ToggleGroup>
  );
}

const THEME_ICON: Record<ThemePref, typeof Sun> = { light: Sun, dark: Moon, system: Monitor };

function ThemeMenu() {
  const theme = useStore((s) => s.prefs.theme);
  const Icon = THEME_ICON[theme];
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="icon-sm" aria-label={`Theme: ${theme}`} data-testid="theme-menu">
          <Icon />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuLabel>Theme</DropdownMenuLabel>
        <DropdownMenuRadioGroup value={theme} onValueChange={(v) => setTheme(v as ThemePref)}>
          <DropdownMenuRadioItem value="light">Light</DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="dark">Dark</DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="system">System</DropdownMenuRadioItem>
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function IconToggle({ label, pressed, onClick, icon: Icon, shortcut }: { label: string; pressed: boolean; onClick: () => void; icon: typeof Sun; shortcut?: string }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="icon-sm" aria-label={label} aria-pressed={pressed} onClick={onClick} className={cn(pressed && "bg-muted text-primary")}>
          <Icon />
        </Button>
      </TooltipTrigger>
      <TooltipContent>
        {label}
        {shortcut && <Kbd>{shortcut}</Kbd>}
      </TooltipContent>
    </Tooltip>
  );
}

/** Secondary views and toggles, grouped so the bar stays quiet. */
function ViewMenu({ autoscroll, guided, lens }: { autoscroll: boolean; guided: boolean; lens: boolean }) {
  const active = autoscroll || guided || lens;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="sm" className={cn("h-8 gap-1.5 px-2 font-normal", active ? "text-foreground" : "text-muted-foreground")} data-testid="view-menu">
          <Eye aria-hidden="true" /> <span className="max-lg:sr-only">View</span>
          <ChevronDown className="size-3 opacity-60" aria-hidden="true" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-64">
        <DropdownMenuCheckboxItem checked={guided} onCheckedChange={(v) => setState({ guidedOpen: !!v, lensOpen: false })}>
          <BookOpenText aria-hidden="true" /> Guided reading
          <DropdownMenuShortcut>G</DropdownMenuShortcut>
        </DropdownMenuCheckboxItem>
        <DropdownMenuCheckboxItem checked={lens} onCheckedChange={(v) => setState({ lensOpen: !!v })}>
          <Sparkles aria-hidden="true" /> Generated UI
        </DropdownMenuCheckboxItem>
        <DropdownMenuCheckboxItem checked={autoscroll} onCheckedChange={(v) => setPrefs({ autoscroll: !!v })}>
          <LocateFixed aria-hidden="true" /> Follow live changes
        </DropdownMenuCheckboxItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={() => setState({ shortcutsOpen: true })}>
          <Keyboard aria-hidden="true" /> Keyboard shortcuts
          <DropdownMenuShortcut>?</DropdownMenuShortcut>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export function Topbar({ mobile }: { mobile: boolean }) {
  const mode = useStore((s) => s.prefs.mode);
  const railCollapsed = useStore((s) => s.prefs.railCollapsed);
  const widgetsCollapsed = useStore((s) => s.prefs.widgetsCollapsed);
  const autoscroll = useStore((s) => s.prefs.autoscroll);
  const guided = useStore((s) => s.guidedOpen);
  const lens = useStore((s) => s.lensOpen);
  const theme = useStore((s) => s.prefs.theme);

  if (mobile) {
    return (
      <header className="flex h-14 shrink-0 items-center gap-1 border-b px-1">
        <Button variant="ghost" className="size-11" aria-label="Files and outline" onClick={() => setState({ sheet: "outline" })}>
          <Menu />
        </Button>
        <div className="min-w-0 flex-1">
          <Breadcrumb touch />
        </div>
        <SelectorStatus compact />
        <Button variant="ghost" className="size-11" aria-label="Reading aids" onClick={() => setState({ sheet: "widgets" })} data-testid="open-widgets">
          <PanelRight />
        </Button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" className="size-11" aria-label="More">
              <MoreHorizontal />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-60">
            <DropdownMenuLabel>View mode</DropdownMenuLabel>
            <DropdownMenuRadioGroup value={mode} onValueChange={(v) => setMode(v as Mode)}>
              {MODES.map((m) => (
                <DropdownMenuRadioItem key={m.value} value={m.value} className="min-h-11">
                  {m.label}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
            <DropdownMenuSeparator />
            <DropdownMenuLabel>Theme</DropdownMenuLabel>
            <DropdownMenuRadioGroup value={theme} onValueChange={(v) => setTheme(v as ThemePref)}>
              <DropdownMenuRadioItem value="light" className="min-h-11">Light</DropdownMenuRadioItem>
              <DropdownMenuRadioItem value="dark" className="min-h-11">Dark</DropdownMenuRadioItem>
              <DropdownMenuRadioItem value="system" className="min-h-11">System</DropdownMenuRadioItem>
            </DropdownMenuRadioGroup>
            <DropdownMenuSeparator />
            <DropdownMenuItem className="min-h-11" onSelect={() => setState({ paletteOpen: true })}>
              <CommandIcon /> Command palette
            </DropdownMenuItem>
            <DropdownMenuItem className="min-h-11" onSelect={() => setState({ guidedOpen: !getState().guidedOpen })}>
              <BookOpenText /> {guided ? "Stop guided reading" : "Guided reading"}
            </DropdownMenuItem>
            <DropdownMenuItem className="min-h-11" onSelect={() => setState({ lensOpen: true })}>
              <Sparkles /> Generated UI
            </DropdownMenuItem>
            <DropdownMenuItem className="min-h-11" onSelect={() => setPrefs({ autoscroll: !autoscroll })}>
              <LocateFixed /> {autoscroll ? "Stop following changes" : "Follow changes"}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </header>
    );
  }

  return (
    <header className="flex h-11 shrink-0 items-center gap-1 border-b px-2">
      {mode !== "read" && (
        <IconToggle
          label={railCollapsed ? "Show files and outline" : "Hide files and outline"}
          pressed={!railCollapsed}
          onClick={() => keepReadingPosition(() => setPrefs({ railCollapsed: !railCollapsed }))}
          icon={PanelLeft}
          shortcut="["
        />
      )}
      <Breadcrumb />
      <SelectorStatus />
      <ModeSwitch />
      {mode === "split" && (
        <IconToggle
          label={widgetsCollapsed ? "Show reading aids" : "Hide reading aids"}
          pressed={!widgetsCollapsed}
          onClick={() => keepReadingPosition(() => setPrefs({ widgetsCollapsed: !widgetsCollapsed }))}
          icon={PanelRight}
          shortcut="]"
        />
      )}
      <span className="mx-1 h-5 w-px bg-border" aria-hidden="true" />
      <ViewMenu autoscroll={autoscroll} guided={guided} lens={lens} />
      <Button
        variant="outline"
        size="sm"
        className="ml-1 h-8 gap-2 px-2 font-normal text-muted-foreground"
        onClick={() => setState({ paletteOpen: true })}
        aria-keyshortcuts={isMac ? "Meta+K" : "Control+K"}
        data-testid="open-palette"
      >
        <CommandIcon /> <span className="sr-only lg:not-sr-only">Commands</span>
        <Kbd>{MOD} K</Kbd>
      </Button>
      <ThemeMenu />
    </header>
  );
}
