// Node graph reading aid: the flow a section writes as arrows (or a from/to
// table), laid out top-down. Pick a node to see what it is, where it comes
// from and where it goes; a node with labelled exits (PASS / RETRY …) gets a
// switch that follows one exit. Everything shown is text from the document.
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import type { SourceRange, UiNode } from "../ir";
import { SourceLink, type OnJump } from "./index";

type Graph = Extract<UiNode, { kind: "NodeGraph" }>;
type Edge = Graph["edges"][number];

interface Layout {
  layer: Map<string, number>;
  order: string[][];
  back: Set<number>;
}

/**
 * Back edges: in document order, an edge that would close a cycle with the
 * edges written before it (a "retry" written after the main flow). Layers by
 * longest path over the remaining forward edges.
 */
function layout(g: Graph): Layout {
  const ids = g.nodes.map((n) => n.id);
  const fwd = new Map<string, string[]>(ids.map((i) => [i, []]));
  const reaches = (from: string, to: string) => {
    const seen = new Set<string>();
    const stack = [from];
    while (stack.length) {
      const v = stack.pop()!;
      if (v === to) return true;
      if (seen.has(v)) continue;
      seen.add(v);
      stack.push(...(fwd.get(v) ?? []));
    }
    return false;
  };
  const back = new Set<number>();
  g.edges.forEach((e, i) => {
    if (reaches(e.to, e.from)) back.add(i);
    else fwd.get(e.from)?.push(e.to);
  });

  const layer = new Map<string, number>(ids.map((i) => [i, 0]));
  // Relax forward edges until stable (graphs are small).
  for (let k = 0; k < ids.length; k++) {
    let changed = false;
    g.edges.forEach((e, i) => {
      if (back.has(i)) return;
      const want = (layer.get(e.from) ?? 0) + 1;
      if ((layer.get(e.to) ?? 0) < want) {
        layer.set(e.to, want);
        changed = true;
      }
    });
    if (!changed) break;
  }
  const depth = Math.max(0, ...layer.values());
  const order: string[][] = Array.from({ length: depth + 1 }, () => []);
  for (const v of ids) order[layer.get(v)!].push(v);
  // One barycenter pass keeps children under their parents.
  for (let l = 1; l < order.length; l++) {
    const pos = new Map(order[l - 1].map((v, i) => [v, i]));
    const bary = (v: string) => {
      const ps = g.edges.filter((e, i) => !back.has(i) && e.to === v && pos.has(e.from)).map((e) => pos.get(e.from)!);
      return ps.length ? ps.reduce((a, b) => a + b, 0) / ps.length : Number.MAX_SAFE_INTEGER;
    };
    order[l].sort((a, b) => bary(a) - bary(b));
  }
  return { layer, order, back };
}

function useWidth<T extends HTMLElement>(): [React.RefObject<T | null>, number] {
  const ref = useRef<T>(null);
  const [w, setW] = useState(0);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setW(el.clientWidth));
    ro.observe(el);
    setW(el.clientWidth);
    return () => ro.disconnect();
  }, []);
  return [ref, w];
}

const VGAP = 56;
const PAD = 12;

export function NodeGraph({ node: g, onJump }: { node: Graph; onJump?: OnJump }) {
  const lay = useMemo(() => layout(g), [g]);
  const labelled = (id: string) => g.edges.filter((e) => e.from === id && e.label);
  const decision = g.nodes.find((n) => new Set(labelled(n.id).map((e) => e.label)).size >= 2);
  const [sel, setSel] = useState<string | null>(decision?.id ?? null);
  const [outcome, setOutcome] = useState<Record<string, string>>({});
  const [ref, width] = useWidth<HTMLDivElement>();
  const uid = useId().replace(/:/g, "");

  const byId = new Map(g.nodes.map((n) => [n.id, n]));
  const exits = sel ? labelled(sel) : [];
  const exitLabels = Array.from(new Set(exits.map((e) => e.label!)));
  const chosen = sel && exitLabels.length >= 2 ? (outcome[sel] ?? exitLabels[0]) : null;

  // Geometry.
  const gutter = lay.back.size ? 28 + 14 * lay.back.size : 0;
  const W = Math.max(width, 260);
  const k = Math.max(...lay.order.map((l) => l.length));
  const hasNotes = g.nodes.some((n) => n.note);
  const room = W - gutter - PAD * 2;
  const nw = Math.max(92, Math.min(220, (room - 16 * (k - 1)) / k));
  // A name that does not fit widens its node into the row's spare room, and
  // wraps onto a second line when even that is not enough (narrow screens).
  const byNode = new Map(g.nodes.map((n) => [n.id, n]));
  const size = new Map<string, { w: number; name: string[] }>();
  for (const ids of lay.order) {
    const most = Math.max(nw, Math.min(260, (room - 16 * (ids.length - 1)) / ids.length));
    for (const id of ids) {
      const label = byNode.get(id)!.label;
      const want = textWidth(label, 7.6) + 16;
      const w = want > nw ? Math.min(most, want) : nw;
      size.set(id, { w, name: wrap(label, w, 7.6) });
    }
  }
  const nameLines = Math.max(...[...size.values()].map((s) => s.name.length));
  const nh = (hasNotes ? 52 : 38) + 16 * (nameLines - 1);
  const pos = new Map<string, { x: number; y: number; w: number }>();
  lay.order.forEach((ids, l) => {
    const total = ids.reduce((s, id) => s + size.get(id)!.w, 0) + (ids.length - 1) * 16;
    let x = PAD + (room - total) / 2;
    for (const id of ids) {
      const w = size.get(id)!.w;
      pos.set(id, { x, y: PAD + l * (nh + VGAP), w });
      x += w + 16;
    }
  });
  const H = PAD * 2 + lay.order.length * nh + (lay.order.length - 1) * VGAP;
  const maxRight = Math.max(...[...pos.values()].map((p) => p.x + p.w));

  const edgeClass = (e: Edge) => {
    if (chosen && e.from === sel && e.label) return e.label === chosen ? "is-chosen" : "is-dim";
    if (sel && (e.from === sel || e.to === sel)) return "is-near";
    return "";
  };
  let backIdx = 0;
  const paths = g.edges.map((e, i) => {
    const a = pos.get(e.from)!;
    const b = pos.get(e.to)!;
    if (lay.back.has(i) || (lay.layer.get(e.to) ?? 0) <= (lay.layer.get(e.from) ?? 0)) {
      const gx = maxRight + 18 + 14 * backIdx++;
      const ay = a.y + nh / 2;
      const by = b.y + nh / 2;
      return {
        e,
        back: true,
        d: `M${a.x + a.w},${ay} H${gx} V${by} H${b.x + b.w + 3}`,
        lx: gx - 6,
        ly: (ay + by) / 2,
        anchor: "end" as const,
      };
    }
    const sx = a.x + a.w / 2;
    const tx = b.x + b.w / 2;
    const sy = a.y + nh;
    const ty = b.y - 3;
    const my = b.y - VGAP / 2;
    return {
      e,
      back: false,
      d: Math.abs(sx - tx) < 1 ? `M${sx},${sy} V${ty}` : `M${sx},${sy} V${my} H${tx} V${ty}`,
      lx: tx + 8,
      ly: (my + b.y) / 2 + 4,
      anchor: "start" as const,
    };
  });

  const s = sel ? byId.get(sel) : null;
  const incoming = sel ? g.edges.filter((e) => e.to === sel) : [];
  const outgoing = sel ? g.edges.filter((e) => e.from === sel) : [];
  const name = (id: string) => byId.get(id)?.label ?? id;
  const jump = (r?: SourceRange) => r && onJump?.(r);

  return (
    <div className="gui-graph" data-testid="node-graph">
      <p className="gui-graph__caption">
        {g.nodes.length} nodes, {g.edges.length} links
      </p>
      <div ref={ref} className="gui-graph__canvas" aria-hidden="true">
        {width > 0 && (
          <svg width={W} height={H} viewBox={`0 0 ${W} ${H}`}>
            <defs>
              <marker id={`${uid}-a`} viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7" markerUnits="userSpaceOnUse" orient="auto-start-reverse">
                <path d="M0,0 L8,4 L0,8 z" className="gui-graph__head" />
              </marker>
              <marker id={`${uid}-s`} viewBox="0 0 8 8" refX="7" refY="4" markerWidth="8" markerHeight="8" markerUnits="userSpaceOnUse" orient="auto-start-reverse">
                <path d="M0,0 L8,4 L0,8 z" className="gui-graph__head is-chosen" />
              </marker>
            </defs>
            {paths.map((p, i) => (
              <g key={i} className={`gui-graph__edge ${p.back ? "is-back" : ""} ${edgeClass(p.e)}`}>
                <path d={p.d} markerEnd={edgeClass(p.e) === "is-chosen" ? `url(#${uid}-s)` : `url(#${uid}-a)`} />
                {p.e.label && (
                  <text x={p.lx} y={p.ly} textAnchor={p.anchor} className="gui-graph__label">
                    {clip(p.e.label, (p.anchor === "end" ? p.lx : W - p.lx) + 12, 6.8)}
                  </text>
                )}
              </g>
            ))}
            {g.nodes.map((n) => {
              const p = pos.get(n.id)!;
              const rows = size.get(n.id)!.name;
              const top = p.y + (nh - (16 * rows.length + (n.note ? 15 : 0))) / 2;
              const on = n.id === sel;
              const reached = chosen != null && g.edges.some((e) => e.from === sel && e.label === chosen && e.to === n.id);
              return (
                <g
                  key={n.id}
                  className={`gui-graph__node ${on ? "is-selected" : ""} ${reached ? "is-reached" : ""}`}
                  onClick={() => setSel(on ? null : n.id)}
                  data-node={n.id}
                >
                  <rect x={p.x} y={p.y} width={p.w} height={nh} rx={7} />
                  {rows.map((t, i) => (
                    <text key={i} x={p.x + p.w / 2} y={top + 12 + 16 * i} textAnchor="middle" className="gui-graph__name">
                      {t}
                    </text>
                  ))}
                  {n.note && (
                    <text x={p.x + p.w / 2} y={top + 16 * rows.length + 12} textAnchor="middle" className="gui-graph__note">
                      {clip(n.note, p.w, 6.6)}
                    </text>
                  )}
                </g>
              );
            })}
          </svg>
        )}
      </div>

      {chosen && s && (
        <div className="gui-graph__row">
          <span className="gui-graph__row-label">{s.label} result</span>
          <ToggleGroup
            type="single"
            variant="outline"
            size="sm"
            value={chosen}
            onValueChange={(v) => v && setOutcome({ ...outcome, [s.id]: v })}
            aria-label={`${s.label} result`}
          >
            {exitLabels.map((l) => (
              <ToggleGroupItem key={l} value={l} className="h-7 px-3 text-[12.5px] max-md:h-11">
                {l}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
          <span className="gui-graph__row-label" aria-live="polite">
            → {exits.filter((e) => e.label === chosen).map((e) => name(e.to)).join(", ")}
          </span>
        </div>
      )}

      <div className="gui-graph__chips" role="group" aria-label="Select a node">
        {g.nodes.map((n) => (
          <button
            key={n.id}
            type="button"
            className="gui-graph__chip"
            aria-pressed={n.id === sel}
            onClick={() => setSel(n.id === sel ? null : n.id)}
          >
            {n.label}
          </button>
        ))}
      </div>

      {s && (
        <div className="gui-graph__detail" data-testid="node-detail">
          <div className="gui-graph__detail-head">
            <strong>{s.label}</strong>
            <SourceLink range={s.sourceRange} onJump={() => jump(s.sourceRange)} />
          </div>
          {s.note && <p>{s.note}</p>}
          <dl>
            <dt>From</dt>
            <dd>{incoming.length ? incoming.map((e) => name(e.from) + (e.label ? ` (${e.label})` : "")).join(", ") : "start"}</dd>
            <dt>To</dt>
            <dd>{outgoing.length ? outgoing.map((e) => name(e.to) + (e.label ? ` (${e.label})` : "")).join(", ") : "end"}</dd>
          </dl>
        </div>
      )}

      <ul className="sr-only">
        {g.edges.map((e, i) => (
          <li key={i}>
            {name(e.from)} to {name(e.to)}
            {e.label ? `, ${e.label}` : ""}
          </li>
        ))}
      </ul>
    </div>
  );
}

/** Shorten text to fit `width` px at about `px` per character (CJK counts double). */
function clip(t: string, width: number, px: number): string {
  const room = Math.floor((width - 16) / px);
  let used = 0;
  let out = "";
  for (const ch of t) {
    used += wide(ch) ? 2 : 1;
    if (used > room) return out.trimEnd() + "…";
    out += ch;
  }
  return out;
}

const wide = (ch: string) => /[　-鿿＀-￯]/.test(ch);

/** Width of `t` in px at about `px` per character (CJK counts double). */
function textWidth(t: string, px: number): number {
  let n = 0;
  for (const ch of t) n += wide(ch) ? 2 : 1;
  return n * px;
}

/**
 * Break a name onto at most two lines that fit `width`: at the last space
 * that fits (anywhere for CJK), the rest clipped.
 */
function wrap(t: string, width: number, px: number): string[] {
  if (textWidth(t, px) <= width - 16) return [t];
  const chars = Array.from(t);
  let cut = 0;
  let space = -1;
  for (let i = 1; i <= chars.length; i++) {
    if (textWidth(chars.slice(0, i).join(""), px) > width - 16) break;
    cut = i;
    if (chars[i] === " ") space = i;
  }
  const at = space > 0 ? space : cut;
  if (at === 0) return [clip(t, width, px)];
  return [chars.slice(0, at).join("").trimEnd(), clip(chars.slice(at).join("").trimStart(), width, px)];
}
