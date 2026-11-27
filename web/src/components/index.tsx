// UI IR component implementations (design doc §5.1).
//
// One component per `UiNode` kind. Security invariants (§8): never use
// `dangerouslySetInnerHTML`; all model-provided strings render as JSX text
// nodes (auto-escaped) and code/config renders inside <pre>. Nothing here
// evaluates content.

import { createContext, useContext, useMemo, useState } from "react";
import { ArrowDown, ArrowUp, ArrowUpDown, Check, ChevronLeft, ChevronRight, Copy } from "lucide-react";
import { WidgetKeyCtx, copyText, useMark, useStep, useWidgetMarks } from "../lib/progress";
import { announce, docPrefs, useStore } from "../lib/store";
import type {
  UiNode,
  NodeMeta,
  SourceRange,
  Severity,
} from "../ir";

export { Chart } from "./Chart";
export { NodeGraph } from "./NodeGraph";
export { Comparison } from "./Comparison";
export { DirectoryTree } from "./DirectoryTree";

/** Emitted when a node (or item) wants to scroll the Content pane to a range. */
export type OnJump = (range: SourceRange) => void;

interface NodeProps<T> {
  node: T;
  onJump?: OnJump;
}

/**
 * "bare": the widget is shown inside a reading aid that already has its own
 * header (title, source chip, why, pin/hide), so Panel drops its title row.
 */
export const PanelChrome = createContext<"full" | "bare">("full");

// --- shared bits ---------------------------------------------------------

/** "generated / verify" + "low confidence" badges (design §5.1). */
function MetaBadges({ meta }: { meta: NodeMeta }) {
  if (meta.origin !== "llm" && !meta.lowConfidence) return null;
  return (
    <span className="gui-badges">
      {meta.origin === "llm" && <span className="gui-badge">generated, verify</span>}
      {meta.lowConfidence && <span className="gui-badge">low confidence</span>}
    </span>
  );
}

/** Source text with `code` spans shown as code (still plain text nodes, no HTML). */
export function InlineText({ text }: { text: string }) {
  const parts = text.split(/(`[^`]+`)/g);
  return (
    <>
      {parts.map((p, i) => (p.length > 2 && p.startsWith("`") && p.endsWith("`") ? <code key={i}>{p.slice(1, -1)}</code> : p))}
    </>
  );
}

export function rangeLabel(r: SourceRange): string {
  return r.end_line > r.start_line ? `L${r.start_line}–${r.end_line}` : `L${r.start_line}`;
}

/** Jump-to-source affordance (design §5.1 SourceRangeLink). */
export function SourceLink({ range, onJump }: { range?: SourceRange; onJump?: OnJump }) {
  if (!range) return null;
  const end = Math.max(range.start_line, range.end_column === 1 && range.end_line > range.start_line ? range.end_line - 1 : range.end_line);
  const r = { ...range, end_line: end };
  return (
    <button
      type="button"
      className="gui-srclink"
      aria-label={`${rangeLabel(r)}: show source lines ${r.start_line} to ${r.end_line}`}
      onClick={() => onJump?.(r)}
    >
      {rangeLabel(r)}
    </button>
  );
}

function Panel({
  title,
  meta,
  children,
}: {
  title: string;
  meta: NodeMeta;
  children: React.ReactNode;
}) {
  const chrome = useContext(PanelChrome);
  if (chrome === "bare") {
    return (
      <div className="gui-panel gui-panel--bare">
        <MetaBadges meta={meta} />
        {children}
      </div>
    );
  }
  return (
    <section className="gui-panel">
      <header className="gui-panel__head">
        <h3>{title}</h3>
        <MetaBadges meta={meta} />
      </header>
      <div className="gui-panel__body">{children}</div>
    </section>
  );
}

function sevClass(s: Severity): string {
  return `gui-sev gui-sev--${s}`;
}

// --- core registry -------------------------------------------------------

export function Tabs({ node, onJump }: NodeProps<Extract<UiNode, { kind: "Tabs" }>>) {
  // Signals could drive the active tab; kept as details/summary for zero-state.
  return (
    <Panel title="Tabs" meta={node}>
      {node.tabs.map((t, i) => (
        <details key={i} open={i === 0} className="gui-tab">
          <summary>{t.title}</summary>
          <div className="gui-tab__body">
            {t.children.map((child, j) => (
              <Render key={j} node={child} onJump={onJump} />
            ))}
          </div>
        </details>
      ))}
    </Panel>
  );
}

export function Callout({ node, onJump }: NodeProps<Extract<UiNode, { kind: "Callout" }>>) {
  const chrome = useContext(PanelChrome);
  if (chrome === "bare") {
    return (
      <div className={`gui-panel gui-panel--bare gui-callout ${sevClass(node.severity)}`}>
        <MetaBadges meta={node} />
        {node.title && <strong>{node.title}</strong>}
        <p>{node.body}</p>
      </div>
    );
  }
  return (
    <section className={`gui-panel gui-callout ${sevClass(node.severity)}`}>
      <header className="gui-panel__head">
        <h3>{node.title ?? node.severity}</h3>
        <MetaBadges meta={node} />
        <SourceLink range={node.sourceRange} onJump={onJump} />
      </header>
      <p>{node.body}</p>
    </section>
  );
}

export function Diagram({ node, onJump }: NodeProps<Extract<UiNode, { kind: "Diagram" }>>) {
  // Mermaid is rendered in a sandbox by the live layer; the safe zero-state is
  // the escaped source (design §8: embedded content is sandboxed, not eval'd).
  return (
    <Panel title={node.title ?? "Diagram"} meta={node}>
      <pre className="gui-code" data-diagram={node.format}>
        {node.code}
      </pre>
      <SourceLink range={node.sourceRange} onJump={onJump} />
    </Panel>
  );
}

// --- operable widgets: progress, stepper, copy, sort ---------------------
// These add an action the plain text cannot offer. Ticks and the current
// step are kept per document in this browser (lib/progress); the Markdown
// file is never modified.

/** A tick-able row: the whole label toggles; the source chip stays separate. */
function TickRow({
  text,
  fromSource,
  extra,
  range,
  onJump,
}: {
  text: string;
  fromSource: boolean;
  extra?: React.ReactNode;
  range?: SourceRange;
  onJump?: OnJump;
}) {
  const [on, toggle] = useMark(text, fromSource);
  return (
    <li className={on ? "is-checked" : ""}>
      <button type="button" role="checkbox" aria-checked={on} className="gui-tick" onClick={toggle}>
        <span className={on ? "gui-check is-on" : "gui-check"} aria-hidden="true">
          {on ? <Check /> : null}
        </span>
        <span className="gui-tick__label">
          <InlineText text={text} />
        </span>
      </button>
      {extra}
      <SourceLink range={range} onJump={onJump} />
    </li>
  );
}

/** "2 of 4 done" with a thin meter, plus a reset when ticks differ from the file. */
function Progress({ done, total }: { done: number; total: number }) {
  const { changed, reset } = useWidgetMarks();
  return (
    <div className="gui-progress">
      <span className="gui-progress__text">
        {done} of {total} done
      </span>
      <span className="gui-meter" aria-hidden="true">
        <span className="gui-meter__fill" style={{ width: `${total ? (done / total) * 100 : 0}%` }} />
      </span>
      {changed && (
        <button type="button" className="gui-linkish gui-progress__reset" onClick={reset} title="Ticks are kept in this browser only. Reset returns to what the file says.">
          Reset to file
        </button>
      )}
    </div>
  );
}

function useDoneCount(items: { text: string; fromSource: boolean }[]): number {
  const wk = useContext(WidgetKeyCtx);
  return useStore((s) => {
    const marks = docPrefs(s).marks ?? {};
    return items.filter((it) => (wk ? (marks[`${wk}|${it.text}`] ?? it.fromSource) : it.fromSource)).length;
  });
}

export function Checklist({ node, onJump }: NodeProps<Extract<UiNode, { kind: "Checklist" }>>) {
  const bare = useContext(PanelChrome) === "bare";
  const done = useDoneCount(node.items.map((i) => ({ text: i.title, fromSource: i.checked })));
  return (
    <Panel title="Checklist" meta={node}>
      <Progress done={done} total={node.items.length} />
      <ul className="gui-checklist">
        {node.items.map((item, i) => (
          <TickRow
            key={i}
            text={item.title}
            fromSource={item.checked}
            extra={item.category && !bare ? <span className="gui-tag">{item.category}</span> : null}
            range={item.sourceRange}
            onJump={onJump}
          />
        ))}
      </ul>
    </Panel>
  );
}

export function ActionItems({ node, onJump }: NodeProps<Extract<UiNode, { kind: "ActionItems" }>>) {
  const done = useDoneCount(node.items.map((i) => ({ text: i.task, fromSource: i.status === "done" })));
  return (
    <Panel title="Action items" meta={node}>
      <Progress done={done} total={node.items.length} />
      <ul className="gui-checklist">
        {node.items.map((it, i) => (
          <TickRow
            key={i}
            text={it.task}
            fromSource={it.status === "done"}
            extra={
              <>
                {it.assignee && <span className="gui-tag">{it.assignee}</span>}
                {it.dueDate && <span className="gui-tag">{it.dueDate}</span>}
                <ConfBadge c={it.confidence} />
              </>
            }
            range={it.sourceRange}
            onJump={onJump}
          />
        ))}
      </ul>
    </Panel>
  );
}

export function StepNavigator({ node, onJump }: NodeProps<Extract<UiNode, { kind: "StepNavigator" }>>) {
  const n = node.steps.length;
  const [cur, setCur] = useStep(n);
  return (
    <Panel title="Steps" meta={node}>
      <div className="gui-stepper" role="group" aria-label={`Step ${cur + 1} of ${n}`}>
        <span className="gui-progress__text" aria-live="polite">
          Step {cur + 1} of {n}
        </span>
        <span className="gui-meter" aria-hidden="true">
          <span className="gui-meter__fill" style={{ width: `${((cur + 1) / n) * 100}%` }} />
        </span>
        <span className="gui-stepper__nav">
          <button type="button" className="gui-iconbtn" aria-label="Previous step" disabled={cur === 0} onClick={() => setCur(cur - 1)}>
            <ChevronLeft aria-hidden="true" />
          </button>
          <button type="button" className="gui-iconbtn" aria-label="Next step" disabled={cur >= n - 1} onClick={() => setCur(cur + 1)}>
            <ChevronRight aria-hidden="true" />
          </button>
        </span>
      </div>
      <ol className="gui-steps">
        {node.steps.map((s, i) => (
          <li key={i} className={i === cur ? "is-current" : i < cur ? "is-done" : ""} aria-current={i === cur ? "step" : undefined}>
            <button type="button" className="gui-step" onClick={() => setCur(i)}>
              <span className="gui-step__num" aria-hidden="true">
                {i < cur ? <Check /> : i + 1}
              </span>
              <span className="gui-step__title">
                <InlineText text={s.title} />
                {i < cur && <span className="sr-only"> (done)</span>}
              </span>
            </button>
            {s.duration && <span className="gui-tag">{s.duration}</span>}
            <SourceLink range={s.sourceRange} onJump={onJump} />
            {i === cur && s.body && <p className="gui-step__body">{s.body}</p>}
            {i === cur && s.prerequisites && s.prerequisites.length > 0 && <small>Before this: {s.prerequisites.join(", ")}</small>}
          </li>
        ))}
      </ol>
    </Panel>
  );
}

function CopyButton({ text, label }: { text: string; label: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      className="gui-iconbtn"
      aria-label={copied ? `Copied ${label}` : `Copy ${label}`}
      onClick={async () => {
        const ok = await copyText(text);
        setCopied(ok);
        announce(ok ? `Copied ${label}` : "Copy failed");
        if (ok) setTimeout(() => setCopied(false), 1500);
      }}
    >
      {copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
    </button>
  );
}

export function ConfigViewer({ node, onJump }: NodeProps<Extract<UiNode, { kind: "ConfigViewer" }>>) {
  const isShell = (node.format as string) === "shell";
  const lines = node.content.split("\n").filter((l) => l.trim().length > 0);
  if (isShell) {
    // Commands are shown and copied, never run (design §8).
    return (
      <Panel title={node.title ?? "Commands"} meta={node}>
        <div className="gui-commands">
          <div className="gui-commands__head">
            <span className="gui-progress__text">{lines.length} commands</span>
            <CopyButton text={lines.join("\n")} label="all commands" />
          </div>
          <ol className="gui-commands__list">
            {lines.map((l, i) => (
              <li key={i}>
                <code>{l}</code>
                <CopyButton text={l} label={`command ${i + 1}`} />
              </li>
            ))}
          </ol>
        </div>
      </Panel>
    );
  }
  return (
    <Panel title={node.title ?? `Config (${node.format})`} meta={node}>
      <div className="gui-commands__head">
        <span className="gui-progress__text">{node.format}</span>
        <CopyButton text={node.content} label={node.format} />
      </div>
      <pre className="gui-code">{node.content}</pre>
      <SourceLink range={node.sourceRange} onJump={onJump} />
    </Panel>
  );
}

export function DataTable({ node, onJump }: NodeProps<Extract<UiNode, { kind: "DataTable" }>>) {
  const [sort, setSort] = useState<{ key: string; dir: 1 | -1 } | null>(null);
  const [q, setQ] = useState("");
  const rows = useMemo(() => {
    const needle = q.trim().toLowerCase();
    let r = node.rows.filter((row) => !needle || node.columns.some((c) => String(row[c.key] ?? "").toLowerCase().includes(needle)));
    if (sort) {
      const num = (v: unknown) => Number(String(v ?? "").replace(/[^0-9.+-]/g, ""));
      r = [...r].sort((a, b) => {
        const x = a[sort.key], y = b[sort.key];
        const nx = num(x), ny = num(y);
        const cmp = !isNaN(nx) && !isNaN(ny) && String(x ?? "").trim() !== "" && String(y ?? "").trim() !== ""
          ? nx - ny
          : String(x ?? "").localeCompare(String(y ?? ""), undefined, { numeric: true });
        return cmp * sort.dir;
      });
    }
    return r;
  }, [node.rows, node.columns, sort, q]);
  const cycle = (key: string) => setSort((s) => (!s || s.key !== key ? { key, dir: 1 } : s.dir === 1 ? { key, dir: -1 } : null));
  return (
    <Panel title="Table" meta={node}>
      {node.rows.length > 5 && (
        <input className="gui-filter" type="search" placeholder="Filter rows" aria-label="Filter table rows" value={q} onChange={(e) => setQ(e.target.value)} />
      )}
      <div className="gui-tablewrap">
        <table className="gui-table">
          <thead>
            <tr>
              {node.columns.map((c) => {
                const dir = sort?.key === c.key ? sort.dir : 0;
                return (
                  <th key={c.key} aria-sort={dir === 1 ? "ascending" : dir === -1 ? "descending" : "none"}>
                    <button type="button" className="gui-sortbtn" onClick={() => cycle(c.key)}>
                      {c.label}
                      {dir === 1 ? <ArrowUp aria-hidden="true" /> : dir === -1 ? <ArrowDown aria-hidden="true" /> : <ArrowUpDown aria-hidden="true" className="opacity-40" />}
                    </button>
                  </th>
                );
              })}
            </tr>
          </thead>
          <tbody>
            {rows.map((row, i) => (
              <tr key={i}>
                {node.columns.map((c) => (
                  <td key={c.key}>
                    <InlineText text={String(row[c.key] ?? "")} />
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {q && <p className="gui-tag">{rows.length} of {node.rows.length} rows</p>}
      <SourceLink range={node.sourceRange} onJump={onJump} />
    </Panel>
  );
}

function ConfBadge({ c }: { c?: import("../ir").Confidence }) {
  if (!c || c === "high") return null;
  return <span className={`gui-conf gui-conf--${c}`}>{c}</span>;
}

export function RiskPanel({ node, onJump }: NodeProps<Extract<UiNode, { kind: "RiskPanel" }>>) {
  return (
    <Panel title="Risks & Assumptions" meta={node}>
      <ul className="gui-risklist">
        {node.risks.map((r, i) => (
          <li key={i} className={sevClass(r.severity)}>
            <strong>{r.title}</strong>
            <ConfBadge c={r.confidence} />
            {r.note && <span> — {r.note}</span>}
            {r.mitigation && <div className="gui-sub">Mitigation: {r.mitigation}</div>}
            <SourceLink range={r.sourceRange} onJump={onJump} />
          </li>
        ))}
      </ul>
      {node.assumptions && node.assumptions.length > 0 && (
        <ul className="gui-risklist gui-assumptions">
          {node.assumptions.map((a, i) => (
            <li key={i}>
              <span className="gui-tag">assumption</span> {a.statement}
              <ConfBadge c={a.confidence} />
              {a.impactIfFalse && <div className="gui-sub">If false: {a.impactIfFalse}</div>}
              <SourceLink range={a.sourceRange} onJump={onJump} />
            </li>
          ))}
        </ul>
      )}
    </Panel>
  );
}

// --- reading lenses (design §8) ------------------------------------------

export function SemanticOutline({ node, onJump }: NodeProps<Extract<UiNode, { kind: "SemanticOutline" }>>) {
  return (
    <Panel title="Semantic Outline" meta={node}>
      {node.groups.map((g, i) => (
        <div key={i} className="gui-outgroup">
          <h4>{g.label}</h4>
          {g.description && <p className="gui-sub">{g.description}</p>}
          <ul className="gui-outline">
            {g.items.map((it, j) => (
              <li key={j}>
                <button className="gui-linkish" onClick={() => it.sourceRange && onJump?.(it.sourceRange)}>
                  {it.title}
                </button>
              </li>
            ))}
          </ul>
        </div>
      ))}
    </Panel>
  );
}

export function SummaryCards({ node, onJump }: NodeProps<Extract<UiNode, { kind: "SummaryCards" }>>) {
  return (
    <Panel title="Summary" meta={node}>
      {node.cards.map((c, i) => (
        <div key={i} className="gui-card">
          <div className="gui-card__head">
            <strong>{c.title}</strong>
            <ConfBadge c={c.confidence} />
            <SourceLink range={c.sourceRange} onJump={onJump} />
          </div>
          <p>{c.summary}</p>
          {c.keyPoints && c.keyPoints.length > 0 && (
            <ul>{c.keyPoints.map((k, j) => <li key={j}>{k}</li>)}</ul>
          )}
        </div>
      ))}
    </Panel>
  );
}

export function DecisionLog({ node, onJump }: NodeProps<Extract<UiNode, { kind: "DecisionLog" }>>) {
  return (
    <Panel title="Decisions" meta={node}>
      {node.decisions.map((d, i) => (
        <div key={i} className="gui-card">
          <div className="gui-card__head">
            <strong>{d.title}</strong>
            <span className={`gui-status gui-status--${d.status}`}>{d.status}</span>
            <ConfBadge c={d.confidence} />
            <SourceLink range={d.sourceRange} onJump={onJump} />
          </div>
          {d.reason && <div className="gui-sub">Reason: {d.reason}</div>}
          {d.alternatives && d.alternatives.length > 0 && (
            <div className="gui-sub">Alternatives: {d.alternatives.join(", ")}</div>
          )}
          {d.impact && <div className="gui-sub">Impact: {d.impact}</div>}
        </div>
      ))}
    </Panel>
  );
}

export function OpenQuestions({ node, onJump }: NodeProps<Extract<UiNode, { kind: "OpenQuestions" }>>) {
  return (
    <Panel title="Open Questions" meta={node}>
      <ul className="gui-risklist">
        {node.questions.map((q, i) => (
          <li key={i} className={sevClass(q.severity)}>
            <strong>{q.question}</strong>
            <ConfBadge c={q.confidence} />
            {q.context && <div className="gui-sub">{q.context}</div>}
            <SourceLink range={q.sourceRange} onJump={onJump} />
          </li>
        ))}
      </ul>
    </Panel>
  );
}

export function Timeline({ node, onJump }: NodeProps<Extract<UiNode, { kind: "Timeline" }>>) {
  return (
    <Panel title="Timeline" meta={node}>
      <ol className="gui-timeline">
        {node.events.map((e, i) => (
          <li key={i}>
            {e.timestamp && <time>{e.timestamp}</time>}
            <strong>{e.title}</strong>
            {e.description && <p>{e.description}</p>}
            <SourceLink range={e.sourceRange} onJump={onJump} />
          </li>
        ))}
      </ol>
    </Panel>
  );
}

export function ApiExplorer({ node }: NodeProps<Extract<UiNode, { kind: "ApiExplorer" }>>) {
  return (
    <Panel title="API" meta={node}>
      <ul className="gui-api">
        {node.endpoints.map((e, i) => (
          <li key={i}>
            <span className={`gui-method gui-method--${e.method.toLowerCase()}`}>{e.method}</span>
            <code>{e.path}</code>
            {e.description && <span> — {e.description}</span>}
          </li>
        ))}
      </ul>
    </Panel>
  );
}

export function DependencyGraph({ node }: NodeProps<Extract<UiNode, { kind: "DependencyGraph" }>>) {
  return (
    <Panel title="Dependencies" meta={node}>
      <ul className="gui-edges">
        {node.edges.map((e, i) => (
          <li key={i}>
            {e.from} → {e.to}
            {e.label && <em> ({e.label})</em>}
          </li>
        ))}
      </ul>
    </Panel>
  );
}

export function LogTimeline({ node }: NodeProps<Extract<UiNode, { kind: "LogTimeline" }>>) {
  return (
    <Panel title="Log" meta={node}>
      <ul className="gui-log">
        {node.entries.map((e, i) => (
          <li key={i} className={sevClass(e.severity)}>
            {e.timestamp && <time>{e.timestamp}</time>}
            <span>{e.message}</span>
          </li>
        ))}
      </ul>
    </Panel>
  );
}

export function CommitGraph({ node }: NodeProps<Extract<UiNode, { kind: "CommitGraph" }>>) {
  return (
    <Panel title="Commits" meta={node}>
      <ul className="gui-commits">
        {node.commits.map((c, i) => (
          <li key={i}>
            <code>{c.hash.slice(0, 7)}</code>
            {c.kind && <span className="gui-tag">{c.kind}</span>}
            <span>{c.subject}</span>
          </li>
        ))}
      </ul>
    </Panel>
  );
}

// --- domain primitives ---------------------------------------------------

export function Glossary({ node, onJump }: NodeProps<Extract<UiNode, { kind: "Glossary" }>>) {
  return (
    <Panel title="Glossary" meta={node}>
      <dl className="gui-glossary">
        {node.terms.map((t, i) => (
          <div key={i}>
            <dt>
              {t.term}
              {t.aliases && t.aliases.length > 0 && <span className="gui-sub"> ({t.aliases.join(", ")})</span>}
              <SourceLink range={t.sourceRange} onJump={onJump} />
            </dt>
            <dd>
              {t.definition ?? t.inferredDefinition ?? <em className="gui-sub">defined elsewhere / not defined in this document</em>}
              {!t.definition && t.inferredDefinition && <ConfBadge c={t.confidence ?? "low"} />}
            </dd>
          </div>
        ))}
      </dl>
    </Panel>
  );
}

export function CharacterRoster({ node, onJump }: NodeProps<Extract<UiNode, { kind: "CharacterRoster" }>>) {
  return (
    <Panel title="Characters" meta={node}>
      <ul className="gui-roster">
        {node.characters.map((c, i) => (
          <li key={i}>
            <strong>{c.name}</strong>
            {c.summary && <span> — {c.summary}</span>}
            <SourceLink range={c.firstSeen} onJump={onJump} />
          </li>
        ))}
      </ul>
    </Panel>
  );
}

export function ToleranceMeter({ node }: NodeProps<Extract<UiNode, { kind: "ToleranceMeter" }>>) {
  const { min, max, nominal, value, unit } = node.quantity;
  let pct = 50;
  if (min != null && max != null && max > min) {
    pct = Math.max(0, Math.min(100, ((value - min) / (max - min)) * 100));
  }
  return (
    <Panel title={node.label} meta={node}>
      <div className="gui-meter">
        <div className="gui-meter__track">
          <div className="gui-meter__fill" style={{ width: `${pct}%` }} />
        </div>
        <div className="gui-meter__labels">
          <span>{min != null ? `${min}${unit ?? ""}` : ""}</span>
          <strong>
            {value}
            {unit ?? ""}
            {nominal != null && ` (nom ${nominal})`}
          </strong>
          <span>{max != null ? `${max}${unit ?? ""}` : ""}</span>
        </div>
      </div>
    </Panel>
  );
}

export function ScalableTable({ node }: NodeProps<Extract<UiNode, { kind: "ScalableTable" }>>) {
  // Scaling is interactive in the full build (@preact/signals); zero-state
  // shows base quantities.
  return (
    <Panel title={`Scalable (base ${node.baseScale})`} meta={node}>
      <table className="gui-table">
        <tbody>
          {node.rows.map((r, i) => (
            <tr key={i}>
              <td>{r.label}</td>
              <td>
                {r.quantity.value}
                {r.quantity.unit ?? ""}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </Panel>
  );
}

export function ObligationMatrix({ node, onJump }: NodeProps<Extract<UiNode, { kind: "ObligationMatrix" }>>) {
  return (
    <Panel title="Obligations" meta={node}>
      <table className="gui-table">
        <thead>
          <tr>
            <th>Party</th>
            <th>Duty</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {node.obligations.map((o, i) => (
            <tr key={i}>
              <td>{o.party}</td>
              <td>{o.duty}</td>
              <td>
                <SourceLink range={o.sourceRange} onJump={onJump} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </Panel>
  );
}

// Re-import Render lazily to avoid a cycle in module init order.
import { Render } from "../registry";
