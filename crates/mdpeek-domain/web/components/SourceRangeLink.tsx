// 原文へジャンプするリンク (AGENTS.md §5.1 SourceRangeLink)。
// Content ペインの該当行へスクロール&ハイライトする。全 UI は sourceRange に
// 紐づく (DESIGN.md 思想)。ここでは行番号へのアンカーを張るだけの薄い実装。
import { h } from "preact";
import type { SourceRange } from "../ir";

export function SourceRangeLink({
  meta,
  label = "原文",
}: {
  meta: { source_range?: SourceRange };
  label?: string;
}) {
  const sr = meta.source_range;
  if (!sr) return null;
  const onClick = (e: Event) => {
    e.preventDefault();
    // Content ペインは Layer 1 の SSR HTML。行アンカー規約は Layer 3 側の
    // scrollToLine と統合する (README「統合手順」)。
    document.dispatchEvent(
      new CustomEvent("mp:jump-to-line", { detail: { line: sr.start_line } }),
    );
  };
  return (
    <a class="mp-srclink" href={`#L${sr.start_line}`} onClick={onClick} title={`L${sr.start_line}`}>
      {label}
    </a>
  );
}
