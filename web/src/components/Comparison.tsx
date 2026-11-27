// Comparison reading aid: the options of a comparison table side by side.
// Scores and facts are the table's cells; the average is plain arithmetic on
// them (each score scaled to its own maximum) and is labelled as such. A
// "Recommended" badge appears only when the document says so.
import { useState } from "react";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import type { UiNode } from "../ir";

type Cmp = Extract<UiNode, { kind: "Comparison" }>;

function cellText(c: string): string {
  return c.replace(/^(\*\*|__|`|\*|_|~~)+|(\*\*|__|`|\*|_|~~)+$/g, "").trim();
}

/** Mean of the option's scores, each scaled to 0–100 by its criterion's max. */
function average(node: Cmp, i: number): { value: number; n: number } | null {
  const xs = node.criteria.map((c) => (c.scores[i] == null ? null : (c.scores[i]! / c.max) * 100)).filter((v): v is number => v != null);
  if (!xs.length) return null;
  return { value: xs.reduce((a, b) => a + b, 0) / xs.length, n: xs.length };
}

export function Comparison({ node }: { node: Cmp }) {
  const [focus, setFocus] = useState<string>("avg");
  const fi = focus === "avg" ? -1 : Number(focus);
  const avgs = node.options.map((_, i) => average(node, i));
  const sameScale = node.criteria.every((c) => c.max === node.criteria[0].max);
  const top = (vals: (number | null)[]) => {
    const m = Math.max(...vals.filter((v): v is number => v != null));
    return vals.map((v) => v != null && v === m && vals.filter((x) => x === m).length < vals.length);
  };
  const best = fi < 0 ? top(avgs.map((a) => (a ? Math.round(a.value) : null))) : top(node.criteria[fi].scores);
  const cols = node.options.length;

  return (
    <div className="gui-cmp" data-testid="comparison">
      <div className="gui-cmp__focus">
        <span className="gui-cmp__focus-label">
          Focus
        </span>
        <ToggleGroup
          type="single"
          variant="outline"
          size="sm"
          value={focus}
          onValueChange={(v) => v && setFocus(v)}
          aria-label="Focus on a criterion"
          className="flex-wrap justify-start"
        >
          <ToggleGroupItem value="avg" className="h-7 px-2.5 text-[12.5px] max-md:h-11">
            Average
          </ToggleGroupItem>
          {node.criteria.map((c, i) => (
            <ToggleGroupItem key={i} value={String(i)} className="h-7 px-2.5 text-[12.5px] max-md:h-11">
              {c.name}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </div>

      <ul className="gui-cmp__cards" style={{ gridTemplateColumns: `repeat(auto-fit, minmax(${cols > 2 ? 180 : 150}px, 1fr))` }}>
        {node.options.map((o, i) => {
          const a = avgs[i];
          const rec = node.recommended === i;
          return (
            <li key={i} className="gui-cmp__card" data-best={best[i] ? "true" : undefined} data-recommended={rec ? "true" : undefined}>
              <div className="gui-cmp__head">
                <h4>{o.name}</h4>
                {rec && (
                  <span className="gui-cmp__badge" title={node.recommended_by ? `From the document: ${node.recommended_by}` : undefined}>
                    Recommended
                  </span>
                )}
              </div>
              {o.note && <p className="gui-cmp__note">{o.note}</p>}

              <div className="gui-cmp__figure">
                {fi < 0 ? (
                  <>
                    <span className="gui-cmp__big">{a ? Math.round(a.value) : "—"}</span>
                    <span className="gui-cmp__of">/ 100</span>
                  </>
                ) : (
                  <>
                    <span className="gui-cmp__big">{cellText(node.criteria[fi].cells[i]) || "—"}</span>
                    {node.criteria[fi].scores[i] != null && <span className="gui-cmp__of">/ {node.criteria[fi].max}</span>}
                  </>
                )}
              </div>
              <p className="gui-cmp__figcap">
                {fi < 0
                  ? a
                    ? `Average of ${a.n} score${a.n > 1 ? "s" : ""}${sameScale ? "" : ", each scaled to 100"}`
                    : "No scores"
                  : node.criteria[fi].name}
                {best[i] && <span className="gui-cmp__best">{fi < 0 ? "Highest" : "Best"}</span>}
              </p>

              <ul className="gui-cmp__rows">
                {node.criteria.map((c, j) => {
                  const v = c.scores[i];
                  return (
                    <li key={j} data-focus={fi === j ? "true" : fi >= 0 ? "false" : undefined}>
                      <span className="gui-cmp__crit">{c.name}</span>
                      <span className="gui-cmp__val">{cellText(c.cells[i]) || "—"}</span>
                      <span className="gui-cmp__track" aria-hidden="true">
                        {v != null && <span className="gui-cmp__fill" style={{ width: `${(v / c.max) * 100}%` }} />}
                      </span>
                    </li>
                  );
                })}
              </ul>

              {(node.facts ?? []).length > 0 && (
                <dl className="gui-cmp__facts">
                  {(node.facts ?? []).map((f, j) => (
                    <div key={j}>
                      <dt>{f.name}</dt>
                      <dd>{cellText(f.cells[i]) || "—"}</dd>
                    </div>
                  ))}
                </dl>
              )}
            </li>
          );
        })}
      </ul>
      {node.recommended != null && node.recommended_by && (
        <p className="gui-cmp__source">
          Recommendation from the document: “{node.recommended_by}”
        </p>
      )}
    </div>
  );
}
