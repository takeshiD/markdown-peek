// Chart reading aid: a bar or line chart of one numeric column of a Markdown
// table. Every value is a parsed table cell (server side, select/chart.rs);
// tooltips show the cell text exactly as written. One measure at a time (no
// second axis): a table with several measures gets a switch.
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import type { ChartSeries, UiNode } from "../ir";

type ChartNode = Extract<UiNode, { kind: "Chart" }>;

const compact = new Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 });
const full = new Intl.NumberFormat(undefined, { maximumFractionDigits: 2 });

function fmt(v: number, s: ChartSeries, axis = false): string {
  const n = axis && Math.abs(v) >= 10_000 ? compact.format(v) : full.format(v);
  if (!s.unit) return n;
  if (s.unit_prefix) return `${s.unit}${n}`;
  return s.unit === "%" ? `${n}%` : `${n} ${s.unit}`;
}

/** Strip Markdown emphasis around a cell for display. */
function cellText(c: string): string {
  return c.replace(/^(\*\*|__|`|\*|_|~~)+|(\*\*|__|`|\*|_|~~)+$/g, "").trim();
}

/** Round tick values covering [lo, hi] (about `count` steps). */
function niceTicks(lo: number, hi: number, count = 4): number[] {
  if (lo === hi) {
    const d = Math.abs(lo) || 1;
    lo -= d / 2;
    hi += d / 2;
  }
  const raw = (hi - lo) / count;
  const mag = 10 ** Math.floor(Math.log10(raw));
  const step = [1, 2, 2.5, 5, 10].map((m) => m * mag).find((s) => s >= raw) ?? 10 * mag;
  const start = Math.floor(lo / step) * step;
  const out: number[] = [];
  for (let v = start; v <= hi + step * 1e-9; v += step) out.push(Number(v.toPrecision(12)));
  if (out[out.length - 1] < hi) out.push(Number((out[out.length - 1] + step).toPrecision(12)));
  return out;
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

export function Chart({ node }: { node: ChartNode }) {
  const [idx, setIdx] = useState(0);
  const s = node.series[Math.min(idx, node.series.length - 1)];
  const captionId = useId();
  const caption = `${s.name}${s.unit ? ` (${s.unit})` : ""} by ${node.label_header}`;
  return (
    <figure className="gui-chart" data-chart={node.chart}>
      <div className="gui-chart__top">
        {node.series.length > 1 ? (
          <ToggleGroup
            type="single"
            variant="outline"
            size="sm"
            value={String(idx)}
            onValueChange={(v) => v && setIdx(Number(v))}
            aria-label="Measure"
            className="flex-wrap justify-start"
          >
            {node.series.map((x, i) => (
              <ToggleGroupItem key={i} value={String(i)} className="h-7 px-2.5 text-[12.5px] max-md:h-11">
                {x.name}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        ) : null}
        <figcaption id={captionId} className="gui-chart__caption">
          {caption}
        </figcaption>
      </div>
      {node.chart === "line" ? <LineChart node={node} s={s} /> : <BarChart node={node} s={s} />}
      <table className="sr-only">
        <caption>{caption}</caption>
        <thead>
          <tr>
            <th scope="col">{node.label_header}</th>
            <th scope="col">{s.name}</th>
          </tr>
        </thead>
        <tbody>
          {node.labels.map((l, i) => (
            <tr key={i}>
              <th scope="row">{l}</th>
              <td>{cellText(s.cells[i] ?? "") || "no value"}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </figure>
  );
}

function BarChart({ node, s }: { node: ChartNode; s: ChartSeries }) {
  const vals = s.values.filter((v): v is number => v != null);
  const lo = Math.min(0, ...vals);
  const hi = Math.max(0, ...vals);
  const span = hi - lo || 1;
  const zero = ((0 - lo) / span) * 100;
  return (
    <div className="gui-bars" aria-hidden="true" data-testid="chart-bars">
      {node.labels.map((label, i) => {
        const v = s.values[i];
        const left = v == null ? zero : (Math.min(v, 0) - lo) / span * 100;
        const width = v == null ? 0 : (Math.abs(v) / span) * 100;
        const neg = v != null && v < 0;
        return (
          <div key={i} className="gui-bars__row" title={`${label}: ${cellText(s.cells[i] ?? "") || "no value"}`}>
            <span className="gui-bars__label">{label}</span>
            <span className="gui-bars__track">
              {v != null && (
                <span className="gui-bars__bar" data-neg={neg ? "true" : undefined} style={{ left: `${left}%`, width: `max(${width}%, 2px)` }} />
              )}
              <span
                className="gui-bars__value"
                style={neg ? { right: `${100 - left}%`, paddingRight: 6 } : { left: `${v == null ? zero : left + width}%`, paddingLeft: 6 }}
              >
                {v == null ? "—" : fmt(v, s)}
              </span>
            </span>
          </div>
        );
      })}
    </div>
  );
}

const H = 196;
const PAD = { top: 12, right: 16, bottom: 26, left: 8 };

function LineChart({ node, s }: { node: ChartNode; s: ChartSeries }) {
  const [ref, width] = useWidth<HTMLDivElement>();
  const [hover, setHover] = useState<number | null>(null);
  const n = node.labels.length;
  const vals = s.values.filter((v): v is number => v != null);
  const ticks = useMemo(() => niceTicks(Math.min(...vals), Math.max(...vals)), [vals.join(",")]); // eslint-disable-line react-hooks/exhaustive-deps
  const t0 = ticks[0];
  const t1 = ticks[ticks.length - 1];
  const tickW = Math.max(...ticks.map((t) => fmt(t, s, true).length)) * 7 + 8;
  const left = PAD.left + tickW;
  const W = Math.max(width, 240);
  const plotW = W - left - PAD.right;
  const plotH = H - PAD.top - PAD.bottom;
  const x = (i: number) => left + (n === 1 ? plotW / 2 : (i / (n - 1)) * plotW);
  const y = (v: number) => PAD.top + plotH - ((v - t0) / (t1 - t0 || 1)) * plotH;

  // Path with gaps where a value is missing.
  let line = "";
  let area = "";
  let run: number[] = [];
  const flush = () => {
    if (run.length > 0) {
      line += run.map((i, k) => `${k ? "L" : "M"}${x(i)},${y(s.values[i]!)}`).join("");
      if (run.length > 1)
        area += `M${x(run[0])},${y(t0)}` + run.map((i) => `L${x(i)},${y(s.values[i]!)}`).join("") + `L${x(run[run.length - 1])},${y(t0)}Z`;
    }
    run = [];
  };
  s.values.forEach((v, i) => (v == null ? flush() : run.push(i)));
  flush();

  const every = Math.max(1, Math.ceil(n / Math.max(2, Math.floor(plotW / 72))));
  const lastIdx = s.values.map((v, i) => (v == null ? -1 : i)).filter((i) => i >= 0).pop() ?? -1;
  const at = hover ?? null;
  const pick = (clientX: number) => {
    const el = ref.current;
    if (!el) return;
    const px = clientX - el.getBoundingClientRect().left;
    const i = Math.round(((px - left) / (plotW || 1)) * (n - 1));
    setHover(Math.max(0, Math.min(n - 1, i)));
  };
  const tip = at != null ? `${node.labels[at]}: ${cellText(s.cells[at] ?? "") || "no value"}` : "";

  return (
    <div
      ref={ref}
      className="gui-line"
      tabIndex={0}
      role="group"
      aria-label={`Line chart. ${s.name} by ${node.label_header}. Use left and right arrow keys to read values.`}
      onPointerMove={(e) => pick(e.clientX)}
      onPointerLeave={() => setHover(null)}
      onBlur={() => setHover(null)}
      onKeyDown={(e) => {
        if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
          e.preventDefault();
          const d = e.key === "ArrowRight" ? 1 : -1;
          setHover((h) => Math.max(0, Math.min(n - 1, (h ?? (d > 0 ? -1 : n)) + d)));
        } else if (e.key === "Home" || e.key === "End") {
          e.preventDefault();
          setHover(e.key === "Home" ? 0 : n - 1);
        }
      }}
      data-testid="chart-line"
    >
      {width > 0 && (
        <svg width={W} height={H} viewBox={`0 0 ${W} ${H}`} aria-hidden="true">
          {ticks.map((t) => (
            <g key={t}>
              <line x1={left} x2={W - PAD.right} y1={y(t)} y2={y(t)} className="gui-line__grid" />
              <text x={left - 8} y={y(t)} dy="0.32em" textAnchor="end" className="gui-line__tick">
                {fmt(t, s, true)}
              </text>
            </g>
          ))}
          {node.labels.map((l, i) =>
            i % every === 0 || i === n - 1 ? (
              <text key={i} x={x(i)} y={H - 8} textAnchor={i === 0 && n > 1 ? "start" : i === n - 1 && n > 1 ? "end" : "middle"} className="gui-line__tick">
                {(i === n - 1 || i + every <= n - 1) ? l : ""}
              </text>
            ) : null,
          )}
          <path d={area} className="gui-line__area" />
          <path d={line} className="gui-line__path" />
          {at != null && <line x1={x(at)} x2={x(at)} y1={PAD.top} y2={PAD.top + plotH} className="gui-line__cross" />}
          {lastIdx >= 0 && at == null && (
            <>
              <circle cx={x(lastIdx)} cy={y(s.values[lastIdx]!)} r={4} className="gui-line__dot" />
              <text x={x(lastIdx)} y={y(s.values[lastIdx]!) - 10} textAnchor="end" className="gui-line__end">
                {fmt(s.values[lastIdx]!, s)}
              </text>
            </>
          )}
          {at != null && s.values[at] != null && <circle cx={x(at)} cy={y(s.values[at]!)} r={4} className="gui-line__dot" />}
        </svg>
      )}
      {at != null && (
        <div
          className="gui-line__tip"
          style={{ left: Math.min(Math.max(x(at), 70), W - 70), top: s.values[at] != null ? y(s.values[at]!) - 12 : PAD.top }}
          data-testid="chart-tip"
        >
          {tip}
        </div>
      )}
      <span className="sr-only" aria-live="polite">
        {tip}
      </span>
    </div>
  );
}
