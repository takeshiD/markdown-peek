// Verdicts reading aid: a list written `R1: satisfied — C1 and C3 passed …`
// (acceptance records, review results), as one row per item: its verdict,
// the checks it cites, and whether it was re-checked or left partly untested.
// The prose stays one click away. Everything shown is the item's own text;
// the outcome comes from the verdict word.
//
// Filters: by outcome / note, and by a cited check (C7 also matches "C6〜C8").
import { useMemo, useState } from "react";
import { Check, ChevronRight, CircleDashed, CircleHelp, RotateCcw, TriangleAlert, X } from "lucide-react";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { cn } from "cn";
import type { UiNode } from "../ir";
import { InlineText, SourceLink, type OnJump } from "./index";

type Node = Extract<UiNode, { kind: "Verdicts" }>;
type Item = Node["items"][number];
type Outcome = Item["outcome"];

const OUTCOME: Record<Outcome, { label: string; Icon: typeof Check }> = {
  pass: { label: "Met", Icon: Check },
  fail: { label: "Not met", Icon: X },
  partial: { label: "Partly met", Icon: CircleDashed },
  unknown: { label: "No verdict", Icon: CircleHelp },
};

type Filter = "all" | Outcome | "rechecked" | "untested";

/** `C6〜C8` → prefix C, 6..8; `REG1` → REG, 1..1. */
function parseCite(c: string): { prefix: string; from: number; to: number } | null {
  const m = /^([A-Z]+)(\d+)(?:[〜~–]([A-Z]*)(\d+))?$/.exec(c);
  if (!m) return null;
  const from = Number(m[2]);
  const to = m[4] ? Number(m[4]) : from;
  return { prefix: m[1], from: Math.min(from, to), to: Math.max(from, to) };
}

/** Does `cite` (as written in an item) cover the single check `id`? */
function covers(cite: string, id: string): boolean {
  if (cite === id) return true;
  const a = parseCite(cite);
  const b = parseCite(id);
  return !!a && !!b && a.prefix === b.prefix && b.from >= a.from && b.to <= a.to;
}

/** Every single check cited anywhere, ranges spread out, in id order. */
function allChecks(items: Item[]): string[] {
  const seen = new Map<string, { prefix: string; n: number }>();
  const order: string[] = [];
  for (const it of items) {
    for (const c of it.cites ?? []) {
      const p = parseCite(c);
      if (!p) continue;
      if (!order.includes(p.prefix)) order.push(p.prefix);
      // Spread a range only when it is short; a long one stays as written.
      if (p.to - p.from <= 20) {
        for (let n = p.from; n <= p.to; n++) seen.set(`${p.prefix}${n}`, { prefix: p.prefix, n });
      } else {
        seen.set(c, { prefix: p.prefix, n: p.from });
      }
    }
  }
  return [...seen.entries()]
    .sort(([, a], [, b]) => order.indexOf(a.prefix) - order.indexOf(b.prefix) || a.n - b.n)
    .map(([k]) => k);
}

/** The item text with one sentence marked (a re-check or an untested part). */
function Marked({ text, marks }: { text: string; marks: { s: string; kind: string }[] }) {
  const hits = marks
    .map((m) => ({ ...m, at: text.indexOf(m.s) }))
    .filter((m) => m.at >= 0)
    .sort((a, b) => a.at - b.at);
  const out: React.ReactNode[] = [];
  let pos = 0;
  hits.forEach((h, i) => {
    if (h.at < pos) return;
    out.push(<InlineText key={`t${i}`} text={text.slice(pos, h.at)} />);
    out.push(
      <mark key={`m${i}`} className="gui-vd__mark" data-kind={h.kind}>
        <InlineText text={h.s} />
      </mark>,
    );
    pos = h.at + h.s.length;
  });
  out.push(<InlineText key="rest" text={text.slice(pos)} />);
  return <>{out}</>;
}

export function Verdicts({ node, onJump }: { node: Node; onJump?: OnJump }) {
  const items = node.items;
  const [filter, setFilter] = useState<Filter>("all");
  const [check, setCheck] = useState<string | null>(null);
  const [open, setOpen] = useState<Set<number>>(() => new Set());
  const checks = useMemo(() => allChecks(items), [items]);

  const count = (f: Filter) =>
    f === "all"
      ? items.length
      : f === "rechecked"
        ? items.filter((i) => i.rechecked).length
        : f === "untested"
          ? items.filter((i) => i.untested).length
          : items.filter((i) => i.outcome === f).length;
  const all: { value: Filter; label: string }[] = [
    { value: "all", label: "All" },
    ...(["pass", "partial", "fail", "unknown"] as const).map((o) => ({ value: o as Filter, label: OUTCOME[o].label })),
    { value: "rechecked", label: "Re-checked" },
    { value: "untested", label: "Partly untested" },
  ];
  const filters = all.filter((f) => f.value === "all" || count(f.value) > 0);

  const shown = items
    .map((it, i) => ({ it, i }))
    .filter(({ it }) =>
      filter === "all"
        ? true
        : filter === "rechecked"
          ? !!it.rechecked
          : filter === "untested"
            ? !!it.untested
            : it.outcome === filter,
    )
    .filter(({ it }) => !check || (it.cites ?? []).some((c) => covers(c, check)));

  const toggle = (i: number) =>
    setOpen((s) => {
      const n = new Set(s);
      if (n.has(i)) n.delete(i);
      else n.add(i);
      return n;
    });

  return (
    <div className="gui-vd" data-testid="verdicts">
      <ToggleGroup
        type="single"
        variant="outline"
        size="sm"
        value={filter}
        onValueChange={(v) => v && setFilter(v as Filter)}
        aria-label="Show items"
        className="gui-vd__filters"
      >
        {filters.map((f) => (
          <ToggleGroupItem key={f.value} value={f.value} className="gui-vd__filter" data-outcome={f.value}>
            {f.label}
            <span className="gui-vd__n">{count(f.value)}</span>
          </ToggleGroupItem>
        ))}
      </ToggleGroup>

      {checks.length > 0 && (
        <div className="gui-vd__checks" role="group" aria-label="Show the items that cite a check">
          <span className="gui-vd__checks-label">Checks</span>
          {checks.map((c) => (
            <button
              key={c}
              type="button"
              className="gui-vd__cite"
              aria-pressed={check === c}
              onClick={() => setCheck(check === c ? null : c)}
            >
              {c}
            </button>
          ))}
        </div>
      )}

      {check && (
        <p className="gui-vd__status" aria-live="polite">
          {shown.length} of {items.length} cite {check}
          <button type="button" className="gui-vd__clear" onClick={() => setCheck(null)}>
            Clear
          </button>
        </p>
      )}

      <ul className="gui-vd__list">
        {shown.map(({ it, i }) => {
          const o = OUTCOME[it.outcome];
          const isOpen = open.has(i);
          const marks = [
            ...(it.rechecked ? [{ s: it.rechecked, kind: "rechecked" }] : []),
            ...(it.untested ? [{ s: it.untested, kind: "untested" }] : []),
          ];
          return (
            <li key={i} className="gui-vd__item" data-outcome={it.outcome} data-open={isOpen || undefined}>
              <div className="gui-vd__row">
                <button
                  type="button"
                  className="gui-vd__head"
                  aria-expanded={isOpen}
                  aria-label={`${it.id}: ${it.verdict}${it.rechecked ? ", re-checked" : ""}${it.untested ? ", partly untested" : ""}. ${isOpen ? "Hide" : "Show"} the text`}
                  onClick={() => toggle(i)}
                >
                  <ChevronRight className="gui-vd__twisty" aria-hidden="true" />
                  <span className="gui-vd__id">{it.id}</span>
                  <span className="gui-vd__verdict" data-outcome={it.outcome}>
                    <o.Icon aria-hidden="true" />
                    {it.verdict}
                  </span>
                </button>
                <span className="gui-vd__cites">
                  {(it.cites ?? []).map((c) => (
                    <button
                      key={c}
                      type="button"
                      className="gui-vd__cite"
                      aria-pressed={!!check && covers(c, check)}
                      title={`Show the items that cite ${c}`}
                      onClick={() => setCheck(check === c ? null : c)}
                    >
                      {c}
                    </button>
                  ))}
                </span>
                <span className="gui-vd__tags">
                  {it.rechecked && (
                    <span className="gui-vd__tag" data-kind="rechecked">
                      <RotateCcw aria-hidden="true" />
                      Re-checked
                    </span>
                  )}
                  {it.untested && (
                    <span className="gui-vd__tag" data-kind="untested">
                      <TriangleAlert aria-hidden="true" />
                      Partly untested
                    </span>
                  )}
                </span>
                <span className="gui-vd__line">
                  <SourceLink range={it.sourceRange} onJump={onJump} />
                </span>
              </div>
              {isOpen && (
                <p className={cn("gui-vd__text")}>
                  <Marked text={it.text} marks={marks} />
                </p>
              )}
            </li>
          );
        })}
      </ul>
      {shown.length === 0 && <p className="gui-vd__status">No item matches.</p>}
    </div>
  );
}
