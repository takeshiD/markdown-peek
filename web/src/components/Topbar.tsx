// Top bar: breadcrumb (repository / file), reading-aid controls (toggle,
// model, stop / resume, status), command palette and theme. Guided reading, following live changes and the
// shortcut list live in the command palette (and on keys g / ?), #66.
import {
  BookOpenText,
  ChevronDown,
  Command as CommandIcon,
  FileText,
  Folder,
  GitBranch,
  ListTree,
  LocateFixed,
  Menu,
  Monitor,
  Moon,
  MoreHorizontal,
  PanelLeft,
  Play,
  Sparkles,
  Square,
  Star,
  Sun,
} from "lucide-react";
import { cn } from "cn";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { Kbd } from "@/components/ui/kbd";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { aidsOn, getState, setPrefs, setState, useStore } from "../lib/store";
import { keepReadingPosition } from "../lib/nav";
import { makeDefaultModel, runAids, selectFile, setTheme, stopAids, toggleAids } from "../lib/actions";
import { CONFIG_ERRORS, type ThemePref } from "../lib/types";
import { AidsControls, AidsStatus, AidsToggle, ModelRadioItems, useAidsView } from "./AidsControls";
import { toggleOutline } from "./OutlinePane";
import { repoLabel } from "./Rail";

export const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);
export const MOD = isMac ? "⌘" : "Ctrl";

function Breadcrumb({ touch }: { touch?: boolean }) {
  const tree = useStore((s) => s.tree);
  const path = useStore((s) => s.doc.path);
  const title = useStore((s) => s.doc.title);
  const group = tree?.tree.groups[0] ?? null;
  const rel = group?.files.find((f) => f.path === path)?.rel ?? title;
  return (
    <nav aria-label="Breadcrumb" className="flex min-w-0 flex-1 items-center gap-0.5 overflow-hidden text-[13px]">
      {group && !touch && (
        <div className="flex min-w-0 shrink items-center gap-0.5 max-lg:hidden">
          <span className="flex h-8 min-w-0 max-w-56 shrink items-center gap-1.5 px-2 font-medium" title={group.root} data-testid="breadcrumb-repo">
            {group.branch ? <GitBranch className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" /> : <Folder className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />}
            <span className="truncate">{repoLabel(group)}</span>
          </span>
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
          <DropdownMenuLabel>Switch file</DropdownMenuLabel>
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
        <Button variant="ghost" size="icon-sm" aria-label={label} aria-pressed={pressed} onClick={onClick} className={cn(pressed && "bg-muted text-foreground")}>
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

export function Topbar({ mobile }: { mobile: boolean }) {
  const aids = useStore(aidsOn);
  const models = useStore((s) => s.models);
  const view = useAidsView();
  const layoutError = useStore((s) => s.layout?.selector.error ?? "");
  const railCollapsed = useStore((s) => s.prefs.railCollapsed);
  const outlineCollapsed = useStore((s) => s.prefs.outlineCollapsed);
  const autoscroll = useStore((s) => s.prefs.autoscroll);
  const guided = useStore((s) => s.guidedOpen);
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
        <AidsToggle compact />
        <AidsStatus compact />
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" className="size-11" aria-label="More">
              <MoreHorizontal />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-60">
            <DropdownMenuItem className="min-h-11" onSelect={toggleAids}>
              <Sparkles /> {aids ? "Turn GenUI off" : "Turn GenUI on"}
            </DropdownMenuItem>
            {view === "running" && (
              <DropdownMenuItem className="min-h-11" onSelect={() => void stopAids()}>
                <Square /> Stop GenUI
              </DropdownMenuItem>
            )}
            {(view === "stopped" || view === "idle" || (view === "error" && !CONFIG_ERRORS.has(layoutError))) && (
              <DropdownMenuItem className="min-h-11" onSelect={() => void runAids()}>
                <Play /> {view === "stopped" ? "Resume GenUI" : view === "error" ? "Retry" : "Run GenUI"}
              </DropdownMenuItem>
            )}
            {models && (
              <>
                <DropdownMenuSeparator />
                <DropdownMenuLabel>Model for this session</DropdownMenuLabel>
                <ModelRadioItems touch />
                {models.active !== models.default && (
                  <DropdownMenuItem className="min-h-11" onSelect={() => void makeDefaultModel()}>
                    <Star /> Make “{models.active}” the default
                  </DropdownMenuItem>
                )}
              </>
            )}
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
      <IconToggle
        label={railCollapsed ? "Show files" : "Hide files"}
        pressed={!railCollapsed}
        onClick={() => keepReadingPosition(() => setPrefs({ railCollapsed: !railCollapsed }))}
        icon={PanelLeft}
        shortcut="["
      />
      <Breadcrumb />
      <AidsControls />
      <IconToggle
        label={outlineCollapsed ? "Show outline" : "Hide outline"}
        pressed={!outlineCollapsed}
        onClick={toggleOutline}
        icon={ListTree}
        shortcut="]"
      />
      <span className="mx-1 h-5 w-px bg-border" aria-hidden="true" />
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
