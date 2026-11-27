// Files (the current working tree's Markdown, left rail) and Outline (fuzzy
// filter + scrollspy; shown on the right by OutlinePane, and in the mobile sheet).
import { useMemo, useState } from "react";
import { FileText, Folder, GitBranch, Search } from "lucide-react";
import { cn } from "cn";
import { Input } from "@/components/ui/input";
import { useStore } from "../lib/store";
import { selectFile } from "../lib/actions";
import { jumpToHeading } from "../lib/nav";

export function fuzzyMatch(query: string, text: string): boolean {
  const q = query.toLowerCase().replace(/\s+/g, "");
  if (!q) return true;
  const t = text.toLowerCase();
  let i = 0;
  for (const ch of t) if (ch === q[i]) i++;
  return i >= q.length;
}

/** "repo (branch)", or the directory name outside git / when detached. */
export function repoLabel(g: { name: string; branch?: string | null }) {
  return g.branch ? `${g.name} (${g.branch})` : g.name;
}

/** Left rail (desktop): the current branch's Markdown files. */
export function Rail() {
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex min-h-11 items-center border-b px-4">
        <h2 className="text-[13px] font-semibold">Files</h2>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        <Files />
      </div>
    </div>
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

/** Markdown files of the current working tree (the branch you are on). */
export function Files({ touch }: { touch?: boolean }) {
  const tree = useStore((s) => s.tree);
  const current = useStore((s) => s.doc.path);
  const group = tree?.tree.groups[0];
  const files = group?.files ?? [];
  return (
    <nav aria-label="Files" className="px-2 py-2">
      {group && (
        <p className="flex items-center gap-1.5 px-2 pb-2 text-[12px] text-muted-foreground" title={group.root}>
          {group.branch ? <GitBranch className="size-3.5" aria-hidden="true" /> : <Folder className="size-3.5" aria-hidden="true" />}
          <span className="truncate">{repoLabel(group)}</span>
        </p>
      )}
      {files.length === 0 && <p className="px-2 text-[13px] text-muted-foreground">No Markdown files found.</p>}
      <ul>
        {files.map((f) => {
          const active = f.path === current;
          return (
            <li key={f.path}>
              <button
                type="button"
                onClick={() => selectFile(f.path)}
                aria-current={active ? "page" : undefined}
                title={f.path}
                data-path={f.path}
                className={cn(
                  "flex w-full min-w-0 items-center gap-2 rounded-sm px-2 py-1 text-left text-[13px] hover:bg-muted",
                  active ? "bg-muted font-medium text-foreground" : "text-muted-foreground",
                  touch && "min-h-11",
                )}
              >
                <FileText className="size-3.5 shrink-0" aria-hidden="true" />
                <span className="truncate">{f.rel}</span>
              </button>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
