// 登場人物パネル (§9.2 小説「人物を見失う」)。名前 + 一言要約 + 初出ジャンプ。
// 要約は断定せず候補・要確認 (DESIGN.md 思想 8: 判断は読者)。
import { h } from "preact";
import type { CharacterRosterNode } from "../ir";
import { SourceRangeLink } from "./SourceRangeLink";

export function CharacterRoster({ node }: { node: CharacterRosterNode }) {
  return (
    <ul class="mp-roster">
      {node.characters.map((c) => (
        <li class="mp-roster__card" key={c.name}>
          <div class="mp-roster__name">
            {c.name}
            {c.aliases && c.aliases.length > 0 && (
              <span class="mp-roster__aliases">（{c.aliases.join("・")}）</span>
            )}
          </div>
          {c.summary && <div class="mp-roster__summary">{c.summary}</div>}
          {c.first_occurrence && (
            <SourceRangeLink meta={{ source_range: c.first_occurrence }} label="初出へ" />
          )}
        </li>
      ))}
    </ul>
  );
}
