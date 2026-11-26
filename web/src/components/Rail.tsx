// Left rail: Files (worktree / branch explorer + compare) and Outline
// (fuzzy filter + scrollspy).
import { useMemo, useState } from "react";
import { FileText, Folder, GitBranch, GitCompareArrows, Search } from "lucide-react";
import { cn } from "cn";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { setPrefs, useStore } from "../lib/store";
import { compareWith, selectFile } from "../lib/actions";
import { jumpToHeading } from "../lib/nav";

export function fuzzyMatch(query: string, text: string): boolean {
  const q = query.toLowerCase().replace(/\s+/g, "");
  if (!q) return true;
  const t = text.toLowerCase();
  let i = 0;
  for (const ch of t) if (ch === q[i]) i++;
  return i >= q.length;
}

export function groupLabel(g: { name: string; branch?: string | null }, by: "worktree" | "branch") {
  return by === "branch" ? g.branch || g.name : g.name;
}

export function Rail({ touch }: { touch?: boolean }) {
  const tab = useStore((s) => s.prefs.railTab);
  return (
    <Tabs
      value={tab}
      onValueChange={(v) => setPrefs({ railTab: v as "files" | "outline" })}
      className="flex h-full min-h-0 flex-col gap-0"
    >
      <div className="flex min-h-11 items-center border-b px-3">
        <TabsList className="h-8 bg-transparent p-0" aria-label="Rail">
          <TabsTrigger value="outline" className={cn("px-3 text-muted-foreground data-[state=active]:bg-muted data-[state=active]:text-foreground data-[state=active]:shadow-none", touch && "min-h-11")}>
            Outline
          </TabsTrigger>
          <TabsTrigger value="files" className={cn("px-3 text-muted-foreground data-[state=active]:bg-muted data-[state=active]:text-foreground data-[state=active]:shadow-none", touch && "min-h-11")}>
            Files
          </TabsTrigger>
        </TabsList>
      </div>
      <TabsContent value="outline" className="min-h-0 flex-1 overflow-y-auto">
        <Outline touch={touch} />
      </TabsContent>
      <TabsContent value="files" className="min-h-0 flex-1 overflow-y-auto">
        <Files touch={touch} />
      </TabsContent>
    </Tabs>
  );
}

export function Outline({ touch }: { touch?: boolean }) {
  const headings = useStore((s) => s.headings);
  const active = useStore((s) => s.activeLine);
  const [q, setQ] = useState("");
  const shown = useMemo(() => headings.filter((h) => fuzzyMatch(q, h.text)), [headings, q]);
  const minLevel = Math.min(...headings.map((h) => h.level), 6);
  return (
    <nav aria-label="Outline" className="px-2 py-2">
      <div className="relative mb-2 px-1">
        <Search className="pointer-events-none absolute top-1/2 left-3 size-3.5 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
        <Input
          type="search"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => {
            // Enter jumps to the first matching heading.
            if (e.key === "Enter" && shown.length) {
              e.preventDefault();
              jumpToHeading(shown[0].id);
            }
          }}
          placeholder="Filter headings"
          aria-label="Filter headings"
          data-testid="outline-filter"
          className={cn("h-8 pl-7 text-[13px]", touch && "h-11")}
        />
      </div>
      {shown.length === 0 ? (
        <p className="px-2 py-1 text-[13px] text-muted-foreground">{headings.length ? "No heading matches." : "This document has no headings."}</p>
      ) : (
        <ul className="flex flex-col">
          {shown.map((h) => {
            const isActive = h.line != null && h.line === active;
            return (
              <li key={h.id}>
                <a
                  href={`#${h.id}`}
                  onClick={(e) => {
                    e.preventDefault();
                    jumpToHeading(h.id);
                  }}
                  aria-current={isActive ? "location" : undefined}
                  className={cn(
                    "block truncate rounded-sm border-l-2 py-1 pr-2 text-[13px] leading-5 hover:bg-muted",
                    isActive ? "border-primary font-medium text-foreground" : "border-transparent text-muted-foreground",
                    touch && "py-3",
                  )}
                  style={{ paddingLeft: `${8 + (h.level - minLevel) * 12}px` }}
                  title={h.text}
                >
                  {h.text}
                </a>
              </li>
            );
          })}
        </ul>
      )}
    </nav>
  );
}

export function Files({ touch }: { touch?: boolean }) {
  const tree = useStore((s) => s.tree);
  const by = useStore((s) => s.prefs.groupBy);
  const current = useStore((s) => s.doc.path);
  const compareA = useStore((s) => s.compareA);
  const groups = tree?.tree.groups.filter((g) => g.files.length) ?? [];
  return (
    <nav aria-label="Files" className="px-2 py-2">
      <div className="mb-2 flex items-center gap-2 px-1">
        <span className="text-[12px] text-muted-foreground" id="groupby-label">
          Group by
        </span>
        <ToggleGroup
          type="single"
          size="sm"
          variant="outline"
          value={by}
          aria-labelledby="groupby-label"
          onValueChange={(v) => v && setPrefs({ groupBy: v as "worktree" | "branch" })}
        >
          <ToggleGroupItem value="worktree" className={cn("h-7 text-[12px]", touch && "h-11")}>
            <Folder /> Worktree
          </ToggleGroupItem>
          <ToggleGroupItem value="branch" className={cn("h-7 text-[12px]", touch && "h-11")}>
            <GitBranch /> Branch
          </ToggleGroupItem>
        </ToggleGroup>
      </div>
      {compareA && (
        <p className="mx-1 mb-2 rounded-sm bg-muted px-2 py-1 text-[12px]" role="status">
          Comparing from {compareA.split("/").pop()}. Pick a second file.
        </p>
      )}
      {groups.length === 0 && <p className="px-2 text-[13px] text-muted-foreground">No Markdown files found.</p>}
      {groups.map((g) => (
        <section key={g.root} className="mb-3" aria-label={groupLabel(g, by)}>
          <h3 className="flex items-center gap-1.5 px-2 py-1 text-[12px] font-semibold text-muted-foreground" title={g.root}>
            {by === "branch" ? <GitBranch className="size-3.5" aria-hidden="true" /> : <Folder className="size-3.5" aria-hidden="true" />}
            <span className="truncate">{groupLabel(g, by)}</span>
          </h3>
          <ul>
            {g.files.map((f) => {
              const active = f.path === current;
              const picked = compareA === f.path;
              return (
                <li key={f.path} className="group/file flex items-center">
                  <button
                    type="button"
                    onClick={() => selectFile(f.path)}
                    aria-current={active ? "page" : undefined}
                    title={f.path}
                    data-path={f.path}
                    className={cn(
                      "flex min-w-0 flex-1 items-center gap-2 rounded-sm px-2 py-1 text-left text-[13px] hover:bg-muted",
                      active ? "bg-muted font-medium text-foreground" : "text-muted-foreground",
                      touch && "min-h-11",
                    )}
                  >
                    <FileText className="size-3.5 shrink-0" aria-hidden="true" />
                    <span className="truncate">{f.rel}</span>
                  </button>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    className={cn(
                      "size-7 shrink-0",
                      picked ? "text-primary" : "text-muted-foreground opacity-60 group-hover/file:opacity-100 focus-visible:opacity-100",
                      touch && "size-11 opacity-100",
                    )}
                    aria-pressed={picked}
                    aria-label={compareA && !picked ? `Compare with ${f.rel}` : `Compare ${f.rel}`}
                    onClick={() => compareWith(f.path)}
                  >
                    <GitCompareArrows />
                  </Button>
                </li>
              );
            })}
          </ul>
        </section>
      ))}
    </nav>
  );
}
