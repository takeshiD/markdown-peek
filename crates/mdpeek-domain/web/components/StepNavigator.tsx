// ステップナビ (§9.2 手順書「どこまでやった」)。1 ステップずつ + 進捗 + 前提物。
// 注意/ロールバックは表示のみ (自動実行しない — セキュリティ §8)。
import { h } from "preact";
import { useSignal } from "@preact/signals";
import type { StepNavigatorNode } from "../ir";
import { fmt } from "../quantity";
import { SourceRangeLink } from "./SourceRangeLink";

export function StepNavigator({ node }: { node: StepNavigatorNode }) {
  const cur = useSignal(0);
  const step = node.steps[cur.value];
  const total = node.steps.length;

  return (
    <div class="mp-step">
      {node.prerequisites && node.prerequisites.length > 0 && (
        <details class="mp-step__prereq" open>
          <summary>必要なもの ({node.prerequisites.length})</summary>
          <ul>{node.prerequisites.map((p, i) => <li key={i}>{p}</li>)}</ul>
        </details>
      )}

      <div class="mp-step__progress">
        ステップ {step.index} / {total}
        <div class="mp-step__bar">
          <div class="mp-step__fill" style={{ width: `${((cur.value + 1) / total) * 100}%` }} />
        </div>
      </div>

      <div class="mp-step__card">
        <h4>{step.title}</h4>
        {step.detail && <p class="mp-step__detail">{step.detail}</p>}
        {step.duration && (
          <div class="mp-step__time">所要 {fmt(step.duration.value)}{step.duration.unit ?? ""}</div>
        )}
        {step.caution && <div class="mp-step__caution">⚠ {step.caution}</div>}
        {step.rollback && (
          <details class="mp-step__rollback">
            <summary>失敗したら</summary>
            <p>{step.rollback}</p>
          </details>
        )}
        <SourceRangeLink meta={{ source_range: step.source_range }} />
      </div>

      <div class="mp-step__nav">
        <button disabled={cur.value === 0} onClick={() => (cur.value -= 1)}>← 前</button>
        <button disabled={cur.value >= total - 1} onClick={() => (cur.value += 1)}>次 →</button>
      </div>
    </div>
  );
}
