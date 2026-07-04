// Three-pane layout (design doc §5.3 / DESIGN.md "複数ビュー表示").
//
// Outline | Content | Generated UI. Per 論点 A the Content pane is the existing
// Layer-1 SSR HTML and the Generated UI pane is this Preact island; in the
// standalone dev harness Content falls back to the raw markdown source so the
// island can be exercised without the server.

import { useSignal } from "@preact/signals";
import type { UiNode, SourceRange } from "../ir";
import { RenderList } from "../registry";

interface Props {
  nodes: UiNode[];
  markdown: string;
}

interface OutlineItem {
  label: string;
  line: number;
}

/** Derive a lightweight outline from markdown ATX headings. */
function outline(markdown: string): OutlineItem[] {
  const items: OutlineItem[] = [];
  markdown.split("\n").forEach((raw, i) => {
    const m = /^(#{1,6})\s+(.*)$/.exec(raw);
    if (m) items.push({ label: `${"·".repeat(m[1].length - 1)}${m[2]}`, line: i + 1 });
  });
  return items;
}

export function ThreePane({ nodes, markdown }: Props) {
  const activeLine = useSignal<number | null>(null);
  const lines = markdown.split("\n");
  const items = outline(markdown);

  const jump = (r: SourceRange) => {
    activeLine.value = r.start_line;
    const el = document.getElementById(`ln-${r.start_line}`);
    el?.scrollIntoView({ behavior: "smooth", block: "center" });
  };

  return (
    <div class="gui-3pane">
      <nav class="gui-outline">
        <h2>Outline</h2>
        <ul>
          {items.map((it) => (
            <li key={it.line}>
              <button onClick={() => jump({ start_line: it.line, start_column: 1, end_line: it.line, end_column: 1 })}>
                {it.label}
              </button>
            </li>
          ))}
        </ul>
      </nav>

      <main class="gui-content">
        <h2>Content</h2>
        <pre class="gui-source">
          {lines.map((l, i) => (
            <div
              key={i}
              id={`ln-${i + 1}`}
              class={activeLine.value === i + 1 ? "gui-line is-active" : "gui-line"}
            >
              <span class="gui-lineno">{i + 1}</span>
              {l || " "}
            </div>
          ))}
        </pre>
      </main>

      <aside class="gui-generated">
        <h2>Generated UI</h2>
        {nodes.length === 0 ? (
          <p class="gui-empty">No generated components for this document.</p>
        ) : (
          <RenderList nodes={nodes} onJump={jump} />
        )}
      </aside>
    </div>
  );
}
