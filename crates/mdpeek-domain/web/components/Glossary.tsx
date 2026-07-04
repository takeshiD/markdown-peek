// 用語集 (§9.2 小説「独自世界観」/ 契約「定義語」)。造語 + 初出定義 + 初出ジャンプ。
import { h } from "preact";
import type { GlossaryNode } from "../ir";
import { SourceRangeLink } from "./SourceRangeLink";

export function Glossary({ node }: { node: GlossaryNode }) {
  return (
    <dl class="mp-glossary">
      {node.entries.map((e) => (
        <div class="mp-glossary__entry" key={e.term}>
          <dt>
            {e.term}
            {e.first_occurrence && <SourceRangeLink meta={{ source_range: e.first_occurrence }} label="初出" />}
          </dt>
          <dd>{e.definition}</dd>
        </div>
      ))}
    </dl>
  );
}
