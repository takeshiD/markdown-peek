// UI IR component implementations (design doc §5.1).
//
// One component per `UiNode` kind. Security invariants (§8): never use
// `dangerouslySetInnerHTML`; all model-provided strings render as JSX text
// nodes (auto-escaped) and code/config renders inside <pre>. Nothing here
// evaluates content.

import type {
  UiNode,
  NodeMeta,
  SourceRange,
  Severity,
} from "../ir";

/** Emitted when a node (or item) wants to scroll the Content pane to a range. */
export type OnJump = (range: SourceRange) => void;

interface NodeProps<T> {
  node: T;
  onJump?: OnJump;
}

// --- shared bits ---------------------------------------------------------

/** "generated / verify" + "low confidence" badges (design §5.1). */
function MetaBadges({ meta }: { meta: NodeMeta }) {
  return (
    <span class="gui-badges">
      {meta.origin === "llm" && <span class="gui-badge gui-badge--llm">generated · verify</span>}
      {meta.lowConfidence && <span class="gui-badge gui-badge--low">low confidence</span>}
    </span>
  );
}

/** Jump-to-source affordance (design §5.1 SourceRangeLink). */
function SourceLink({ range, onJump }: { range?: SourceRange; onJump?: OnJump }) {
  if (!range) return null;
  return (
    <button
      class="gui-srclink"
      title={`Source: lines ${range.start_line}–${range.end_line}`}
      onClick={() => onJump?.(range)}
    >
      L{range.start_line}
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
  children: preact.ComponentChildren;
}) {
  return (
    <section class="gui-panel">
      <header class="gui-panel__head">
        <h3>{title}</h3>
        <MetaBadges meta={meta} />
      </header>
      <div class="gui-panel__body">{children}</div>
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
        <details key={i} open={i === 0} class="gui-tab">
          <summary>{t.title}</summary>
          <div class="gui-tab__body">
            {t.children.map((child, j) => (
              <Render key={j} node={child} onJump={onJump} />
            ))}
          </div>
        </details>
      ))}
    </Panel>
  );
}

export function Checklist({ node, onJump }: NodeProps<Extract<UiNode, { kind: "Checklist" }>>) {
  const done = node.items.filter((i) => i.checked).length;
  return (
    <Panel title={`Checklist (${done}/${node.items.length})`} meta={node}>
      <ul class="gui-checklist">
        {node.items.map((item, i) => (
          <li key={i} class={item.checked ? "is-checked" : ""}>
            <input type="checkbox" checked={item.checked} readonly />
            <span>{item.title}</span>
            {item.category && <span class="gui-tag">{item.category}</span>}
            <SourceLink range={item.sourceRange} onJump={onJump} />
          </li>
        ))}
      </ul>
    </Panel>
  );
}

export function DataTable({ node, onJump }: NodeProps<Extract<UiNode, { kind: "DataTable" }>>) {
  return (
    <Panel title="Table" meta={node}>
      <div class="gui-tablewrap">
        <table class="gui-table">
          <thead>
            <tr>
              {node.columns.map((c) => (
                <th key={c.key}>{c.label}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {node.rows.map((row, i) => (
              <tr key={i}>
                {node.columns.map((c) => (
                  <td key={c.key}>{String(row[c.key] ?? "")}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <SourceLink range={node.sourceRange} onJump={onJump} />
    </Panel>
  );
}

export function Callout({ node, onJump }: NodeProps<Extract<UiNode, { kind: "Callout" }>>) {
  return (
    <section class={`gui-panel gui-callout ${sevClass(node.severity)}`}>
      <header class="gui-panel__head">
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
      <pre class="gui-code" data-diagram={node.format}>
        {node.code}
      </pre>
      <SourceLink range={node.sourceRange} onJump={onJump} />
    </Panel>
  );
}

export function ConfigViewer({ node, onJump }: NodeProps<Extract<UiNode, { kind: "ConfigViewer" }>>) {
  return (
    <Panel title={node.title ?? `Config (${node.format})`} meta={node}>
      <pre class="gui-code">{node.content}</pre>
      <SourceLink range={node.sourceRange} onJump={onJump} />
    </Panel>
  );
}

function ConfBadge({ c }: { c?: import("../ir").Confidence }) {
  if (!c || c === "high") return null;
  return <span class={`gui-conf gui-conf--${c}`}>{c}</span>;
}

export function RiskPanel({ node, onJump }: NodeProps<Extract<UiNode, { kind: "RiskPanel" }>>) {
  return (
    <Panel title="Risks & Assumptions" meta={node}>
      <ul class="gui-risklist">
        {node.risks.map((r, i) => (
          <li key={i} class={sevClass(r.severity)}>
            <strong>{r.title}</strong>
            <ConfBadge c={r.confidence} />
            {r.note && <span> — {r.note}</span>}
            {r.mitigation && <div class="gui-sub">Mitigation: {r.mitigation}</div>}
            <SourceLink range={r.sourceRange} onJump={onJump} />
          </li>
        ))}
      </ul>
      {node.assumptions && node.assumptions.length > 0 && (
        <ul class="gui-risklist gui-assumptions">
          {node.assumptions.map((a, i) => (
            <li key={i}>
              <span class="gui-tag">assumption</span> {a.statement}
              <ConfBadge c={a.confidence} />
              {a.impactIfFalse && <div class="gui-sub">If false: {a.impactIfFalse}</div>}
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
        <div key={i} class="gui-outgroup">
          <h4>{g.label}</h4>
          {g.description && <p class="gui-sub">{g.description}</p>}
          <ul class="gui-outline">
            {g.items.map((it, j) => (
              <li key={j}>
                <button class="gui-linkish" onClick={() => it.sourceRange && onJump?.(it.sourceRange)}>
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
        <div key={i} class="gui-card">
          <div class="gui-card__head">
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
        <div key={i} class="gui-card">
          <div class="gui-card__head">
            <strong>{d.title}</strong>
            <span class={`gui-status gui-status--${d.status}`}>{d.status}</span>
            <ConfBadge c={d.confidence} />
            <SourceLink range={d.sourceRange} onJump={onJump} />
          </div>
          {d.reason && <div class="gui-sub">Reason: {d.reason}</div>}
          {d.alternatives && d.alternatives.length > 0 && (
            <div class="gui-sub">Alternatives: {d.alternatives.join(", ")}</div>
          )}
          {d.impact && <div class="gui-sub">Impact: {d.impact}</div>}
        </div>
      ))}
    </Panel>
  );
}

export function ActionItems({ node, onJump }: NodeProps<Extract<UiNode, { kind: "ActionItems" }>>) {
  const done = node.items.filter((i) => i.status === "done").length;
  return (
    <Panel title={`Action Items (${done}/${node.items.length})`} meta={node}>
      <ul class="gui-checklist">
        {node.items.map((it, i) => (
          <li key={i} class={it.status === "done" ? "is-checked" : ""}>
            <input type="checkbox" checked={it.status === "done"} readonly />
            <span>{it.task}</span>
            {it.assignee && <span class="gui-tag">{it.assignee}</span>}
            {it.dueDate && <span class="gui-tag">{it.dueDate}</span>}
            <ConfBadge c={it.confidence} />
            <SourceLink range={it.sourceRange} onJump={onJump} />
          </li>
        ))}
      </ul>
    </Panel>
  );
}

export function OpenQuestions({ node, onJump }: NodeProps<Extract<UiNode, { kind: "OpenQuestions" }>>) {
  return (
    <Panel title="Open Questions" meta={node}>
      <ul class="gui-risklist">
        {node.questions.map((q, i) => (
          <li key={i} class={sevClass(q.severity)}>
            <strong>{q.question}</strong>
            <ConfBadge c={q.confidence} />
            {q.context && <div class="gui-sub">{q.context}</div>}
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
      <ol class="gui-timeline">
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
      <ul class="gui-api">
        {node.endpoints.map((e, i) => (
          <li key={i}>
            <span class={`gui-method gui-method--${e.method.toLowerCase()}`}>{e.method}</span>
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
      <ul class="gui-edges">
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
      <ul class="gui-log">
        {node.entries.map((e, i) => (
          <li key={i} class={sevClass(e.severity)}>
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
      <ul class="gui-commits">
        {node.commits.map((c, i) => (
          <li key={i}>
            <code>{c.hash.slice(0, 7)}</code>
            {c.kind && <span class="gui-tag">{c.kind}</span>}
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
      <dl class="gui-glossary">
        {node.terms.map((t, i) => (
          <div key={i}>
            <dt>
              {t.term}
              {t.aliases && t.aliases.length > 0 && <span class="gui-sub"> ({t.aliases.join(", ")})</span>}
              <SourceLink range={t.sourceRange} onJump={onJump} />
            </dt>
            <dd>
              {t.definition ?? t.inferredDefinition ?? <em class="gui-sub">defined elsewhere / not defined in this document</em>}
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
      <ul class="gui-roster">
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

export function StepNavigator({ node, onJump }: NodeProps<Extract<UiNode, { kind: "StepNavigator" }>>) {
  return (
    <Panel title="Steps" meta={node}>
      <ol class="gui-steps">
        {node.steps.map((s, i) => (
          <li key={i}>
            <strong>{s.title}</strong>
            {s.duration && <span class="gui-tag">{s.duration}</span>}
            {s.body && <p>{s.body}</p>}
            {s.prerequisites && s.prerequisites.length > 0 && (
              <small>prereq: {s.prerequisites.join(", ")}</small>
            )}
            <SourceLink range={s.sourceRange} onJump={onJump} />
          </li>
        ))}
      </ol>
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
      <div class="gui-meter">
        <div class="gui-meter__track">
          <div class="gui-meter__fill" style={`width:${pct}%`} />
        </div>
        <div class="gui-meter__labels">
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
      <table class="gui-table">
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
      <table class="gui-table">
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
