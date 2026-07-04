// 数値 operable ロジックの web 版 (Rust crate の src/quantity.rs と同じ規則)。
// 重い判断は Rust core に集約する方針 (§1) だが、スケール係数の変更はクライアント
// 側インタラクション (人数スライダ) なので、決定論的な再計算だけここに持つ。
import type { Quantity } from "./ir";

export type ToleranceStatus = "in_spec" | "below_min" | "above_max" | "unknown";

export interface ToleranceEval {
  status: ToleranceStatus;
  position: number | null; // min..max を 0..1 に写像 (範囲外は 0 未満/1 超も返す)
  deviation: number | null; // value - nominal
}

export function evaluateTolerance(q: Quantity): ToleranceEval {
  let status: ToleranceStatus;
  if (q.min !== undefined && q.value < q.min) status = "below_min";
  else if (q.max !== undefined && q.value > q.max) status = "above_max";
  else if (q.min !== undefined || q.max !== undefined) status = "in_spec";
  else status = "unknown";

  const position =
    q.min !== undefined && q.max !== undefined && q.max > q.min
      ? (q.value - q.min) / (q.max - q.min)
      : null;

  const deviation = q.nominal !== undefined ? q.value - q.nominal : null;
  return { status, position, deviation };
}

export function scaleFactor(base: number, target: number): number {
  return base === 0 ? 1 : target / base;
}

export function scaleQuantity(q: Quantity, factor: number): Quantity {
  if (!q.scalable) return q;
  return {
    ...q,
    value: q.value * factor,
    min: q.min !== undefined ? q.min * factor : undefined,
    max: q.max !== undefined ? q.max * factor : undefined,
    nominal: q.nominal !== undefined ? q.nominal * factor : undefined,
  };
}

// 表示用に有効数字を整える (整数ならそのまま)。
export function fmt(n: number): string {
  return Number.isInteger(n) ? String(n) : n.toFixed(2).replace(/\.?0+$/, "");
}
