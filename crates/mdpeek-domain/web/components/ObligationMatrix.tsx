// 当事者 × 義務/権利マトリクス (§9.2 契約・規程)。当事者ごとに義務/権利を整理。
import { h } from "preact";
import type { ObligationMatrixNode } from "../ir";
import { SourceRangeLink } from "./SourceRangeLink";

export function ObligationMatrix({ node }: { node: ObligationMatrixNode }) {
  return (
    <table class="mp-obligation">
      <thead>
        <tr>
          <th>当事者</th>
          <th>義務</th>
          <th>権利</th>
        </tr>
      </thead>
      <tbody>
        {node.parties.map((party) => {
          const rows = node.obligations.filter((o) => o.party === party);
          const duties = rows.filter((o) => o.kind === "obligation");
          const rights = rows.filter((o) => o.kind === "right");
          return (
            <tr key={party}>
              <th scope="row">{party}</th>
              <td>
                <ul>
                  {duties.map((o, i) => (
                    <li key={i}>
                      {o.description}
                      {o.source_range && <SourceRangeLink meta={{ source_range: o.source_range }} />}
                    </li>
                  ))}
                </ul>
              </td>
              <td>
                <ul>
                  {rights.map((o, i) => (
                    <li key={i}>
                      {o.description}
                      {o.source_range && <SourceRangeLink meta={{ source_range: o.source_range }} />}
                    </li>
                  ))}
                </ul>
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
