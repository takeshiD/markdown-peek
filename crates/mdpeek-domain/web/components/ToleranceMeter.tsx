// 公差メーター (§9.2 生産指示書「公差が数値の羅列」)。
// 上下限を視覚バーに、規格中心からの位置を示す。判定は evaluateTolerance に委譲。
import { h } from "preact";
import type { ToleranceMeterNode } from "../ir";
import { evaluateTolerance, fmt } from "../quantity";
import { SourceRangeLink } from "./SourceRangeLink";

const STATUS_COLOR: Record<string, string> = {
  in_spec: "var(--ok, #2e7d32)",
  below_min: "var(--warn, #ef6c00)",
  above_max: "var(--warn, #ef6c00)",
  unknown: "var(--muted, #9e9e9e)",
};

export function ToleranceMeter({ node }: { node: ToleranceMeterNode }) {
  return (
    <div class="mp-tolerance">
      {node.meters.map((m) => {
        const ev = evaluateTolerance(m.quantity);
        const pct = ev.position === null ? null : Math.max(0, Math.min(1, ev.position)) * 100;
        const unit = m.quantity.unit ?? "";
        return (
          <div class="mp-tolerance__row" key={m.label}>
            <div class="mp-tolerance__label">
              {m.label}
              <span class="mp-tolerance__value" style={{ color: STATUS_COLOR[ev.status] }}>
                {fmt(m.quantity.value)}{unit}
              </span>
            </div>
            <div class="mp-tolerance__bar">
              {m.quantity.min !== undefined && <span class="mp-tolerance__min">{fmt(m.quantity.min)}</span>}
              <div class="mp-tolerance__track">
                {pct !== null && (
                  <div
                    class="mp-tolerance__marker"
                    style={{ left: `${pct}%`, background: STATUS_COLOR[ev.status] }}
                    title={ev.status}
                  />
                )}
                {m.quantity.nominal !== undefined && ev.position !== null && (
                  <div class="mp-tolerance__center" />
                )}
              </div>
              {m.quantity.max !== undefined && <span class="mp-tolerance__max">{fmt(m.quantity.max)}</span>}
            </div>
            {ev.status !== "in_spec" && ev.status !== "unknown" && (
              <div class="mp-tolerance__flag">規格外 (要確認)</div>
            )}
          </div>
        );
      })}
      <SourceRangeLink meta={node} />
    </div>
  );
}
