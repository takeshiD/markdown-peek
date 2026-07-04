// 数量連動テーブル (§9.2 レシピ「人数で分量が変わる」)。
// スライダで人数を変えると scalable セルが連動再計算される (§9.3-3 operable)。
import { h } from "preact";
import { useSignal } from "@preact/signals";
import type { ScalableTableNode } from "../ir";
import { isAmount } from "../ir";
import { fmt, scaleFactor, scaleQuantity } from "../quantity";
import { SourceRangeLink } from "./SourceRangeLink";

export function ScalableTable({ node }: { node: ScalableTableNode }) {
  const base = node.base_scale?.value ?? 1;
  const target = useSignal(base);
  const factor = scaleFactor(base, target.value);
  const scaleUnit = node.base_scale?.unit ?? "";

  return (
    <div class="mp-scalable">
      {node.base_scale && (
        <div class="mp-scalable__control">
          <label>
            {fmt(target.value)}{scaleUnit}
            <input
              type="range"
              min={1}
              max={Math.max(base * 6, 12)}
              value={target.value}
              onInput={(e) => (target.value = Number((e.target as HTMLInputElement).value))}
            />
          </label>
          <span class="mp-scalable__base">基準 {fmt(base)}{scaleUnit}</span>
        </div>
      )}
      <table class="mp-scalable__table">
        <thead>
          <tr>{node.columns.map((c) => <th key={c.key}>{c.label}</th>)}</tr>
        </thead>
        <tbody>
          {node.rows.map((row, ri) => (
            <tr key={ri}>
              {row.cells.map((cell, ci) => {
                if (isAmount(cell)) {
                  const q = scaleQuantity(cell, factor);
                  return <td key={ci} class="mp-scalable__amount">{fmt(q.value)}{q.unit ?? ""}</td>;
                }
                return <td key={ci}>{cell}</td>;
              })}
            </tr>
          ))}
        </tbody>
      </table>
      <SourceRangeLink meta={node} />
    </div>
  );
}
