// Directory tree reading aid (#64): a layout drawn with `├──` / `└──` in a
// code block, redrawn as a tree whose folders open and close. Names and
// comments are the code block's text (rendered as text nodes); every entry
// keeps its source line, so Enter or the line chip jumps back to it.
//
// Keyboard (WAI-ARIA tree pattern): Up / Down move, Right opens a folder or
// moves to its first child, Left closes it or moves to the parent, Home / End,
// Enter shows the entry's line in the document, Space opens / closes.
import { useId, useMemo, useRef, useState } from "react";
import { ChevronRight, File, Folder, FolderOpen } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "cn";
import type { SourceRange, UiNode } from "../ir";
import { InlineText, type OnJump } from "./index";

type Tree = Extract<UiNode, { kind: "DirectoryTree" }>;
type Entry = Tree["entries"][number];

/** Above this many entries only the first two levels start open. */
const OPEN_ALL_UP_TO = 40;

interface Shape {
  parent: number[];
  children: number[][];
  roots: number[];
  dirs: number[];
}

function shapeOf(entries: Entry[]): Shape {
  const parent: number[] = [];
  const children: number[][] = entries.map(() => []);
  const roots: number[] = [];
  const stack: number[] = [];
  entries.forEach((e, i) => {
    while (stack.length > e.depth) stack.pop();
    const p = stack.length ? stack[stack.length - 1] : -1;
    parent.push(p);
    if (p < 0) roots.push(i);
    else children[p].push(i);
    stack.push(i);
  });
  const dirs = entries.map((e, i) => (e.is_dir || children[i].length > 0 ? i : -1)).filter((i) => i >= 0);
  return { parent, children, roots, dirs };
}

function lineOf(r?: SourceRange): string {
  return r ? `L${r.start_line}` : "";
}

export function DirectoryTree({ node, onJump }: { node: Tree; onJump?: OnJump }) {
  const entries = node.entries;
  const shape = useMemo(() => shapeOf(entries), [entries]);
  const uid = useId();
  const [open, setOpen] = useState<Set<number>>(
    () => new Set(shape.dirs.filter((i) => entries.length <= OPEN_ALL_UP_TO || entries[i].depth < 2)),
  );
  const [focus, setFocus] = useState(0);
  const items = useRef(new Map<number, HTMLLIElement>());

  const hasKids = (i: number) => shape.children[i].length > 0;
  const isDir = (i: number) => entries[i].is_dir || hasKids(i);

  // Entries currently shown, in order (for Up / Down / Home / End).
  const visible = useMemo(() => {
    const out: number[] = [];
    const walk = (ids: number[]) => {
      for (const i of ids) {
        out.push(i);
        if (open.has(i)) walk(shape.children[i]);
      }
    };
    walk(shape.roots);
    return out;
  }, [open, shape]);

  // The focused entry may have been hidden by a fold: fall back to its
  // nearest shown ancestor.
  let current = focus;
  while (current >= 0 && !visible.includes(current)) current = shape.parent[current];
  if (current < 0) current = visible[0] ?? 0;

  const moveTo = (i: number) => {
    setFocus(i);
    items.current.get(i)?.focus();
  };
  const toggle = (i: number, to?: boolean) =>
    setOpen((s) => {
      const n = new Set(s);
      if (to ?? !n.has(i)) n.add(i);
      else n.delete(i);
      return n;
    });
  const jump = (i: number) => {
    const r = entries[i].sourceRange;
    if (r) onJump?.(r);
  };

  const onKey = (e: React.KeyboardEvent, i: number) => {
    const pos = visible.indexOf(i);
    switch (e.key) {
      case "ArrowDown":
        if (pos < visible.length - 1) moveTo(visible[pos + 1]);
        break;
      case "ArrowUp":
        if (pos > 0) moveTo(visible[pos - 1]);
        break;
      case "Home":
        moveTo(visible[0]);
        break;
      case "End":
        moveTo(visible[visible.length - 1]);
        break;
      case "ArrowRight":
        if (!hasKids(i)) break;
        if (!open.has(i)) toggle(i, true);
        else moveTo(shape.children[i][0]);
        break;
      case "ArrowLeft":
        if (hasKids(i) && open.has(i)) toggle(i, false);
        else if (shape.parent[i] >= 0) moveTo(shape.parent[i]);
        break;
      case "Enter":
        jump(i);
        break;
      case " ":
        if (hasKids(i)) toggle(i);
        break;
      default:
        return;
    }
    e.preventDefault();
    e.stopPropagation();
  };

  const folders = shape.dirs.length;
  const files = entries.length - folders;
  const foldable = shape.dirs.filter(hasKids);

  const renderItem = (i: number, setSize: number, posInSet: number): React.ReactNode => {
    const e = entries[i];
    const dir = isDir(i);
    const kids = hasKids(i);
    const expanded = kids ? open.has(i) : undefined;
    const nameId = `${uid}-n${i}`;
    const Icon = dir ? (expanded ? FolderOpen : Folder) : File;
    return (
      <li
        key={i}
        ref={(el) => {
          if (el) items.current.set(i, el);
          else items.current.delete(i);
        }}
        role="treeitem"
        aria-level={e.depth + 1}
        aria-setsize={setSize}
        aria-posinset={posInSet}
        aria-expanded={expanded}
        aria-labelledby={nameId}
        tabIndex={i === current ? 0 : -1}
        className="gui-dtree__item"
        data-dir={dir ? "true" : undefined}
        data-marked={e.marked ? "true" : undefined}
        onKeyDown={(ev) => onKey(ev, i)}
        onFocus={(ev) => {
          if (ev.target === ev.currentTarget) setFocus(i);
        }}
      >
        <div
          className="gui-dtree__row"
          onClick={() => {
            moveTo(i);
            if (kids) toggle(i);
          }}
        >
          <span className="gui-dtree__twisty" aria-hidden="true">
            {kids && <ChevronRight className={cn("size-3.5 transition-transform motion-reduce:transition-none", expanded && "rotate-90")} />}
          </span>
          <Icon className="gui-dtree__icon" aria-hidden="true" />
          <span id={nameId} className="gui-dtree__label">
            <span className="gui-dtree__name">{e.name}</span>{" "}
            {e.comment && (
              <span className="gui-dtree__comment">
                <InlineText text={e.comment} />
              </span>
            )}
          </span>
          {e.sourceRange && (
            <button
              type="button"
              tabIndex={-1}
              className="gui-srclink gui-dtree__line"
              aria-label={`${lineOf(e.sourceRange)}: show source line ${e.sourceRange.start_line}`}
              onClick={(ev) => {
                ev.stopPropagation();
                jump(i);
              }}
            >
              {lineOf(e.sourceRange)}
            </button>
          )}
        </div>
        {kids && expanded && (
          <ul role="group" className="gui-dtree__group">
            {shape.children[i].map((c, k) => renderItem(c, shape.children[i].length, k + 1))}
          </ul>
        )}
      </li>
    );
  };

  const label = node.title ? `Directory tree: ${node.title}` : "Directory tree";
  return (
    <div className="gui-dtree" data-testid="directory-tree">
      <div className="gui-dtree__bar">
        <p className="gui-dtree__count">
          {folders} folder{folders === 1 ? "" : "s"}, {files} file{files === 1 ? "" : "s"}
        </p>
        {foldable.length > 0 && (
          <span className="flex gap-1">
            <Button
              variant="ghost"
              size="sm"
              className="h-7 px-2 text-[12.5px] text-muted-foreground max-md:h-11"
              disabled={foldable.every((i) => open.has(i))}
              onClick={() => setOpen(new Set(foldable))}
            >
              Expand all
            </Button>
            <Button
              variant="ghost"
              size="sm"
              className="h-7 px-2 text-[12.5px] text-muted-foreground max-md:h-11"
              disabled={foldable.every((i) => !open.has(i))}
              onClick={() => setOpen(new Set())}
            >
              Collapse all
            </Button>
          </span>
        )}
      </div>
      <ul role="tree" aria-label={label} className="gui-dtree__tree">
        {shape.roots.map((r, k) => renderItem(r, shape.roots.length, k + 1))}
      </ul>
    </div>
  );
}
