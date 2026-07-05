// Generated-UI island entry for the live server (design §5, 論点 A co-existence).
//
// Unlike the standalone `main.tsx` (full 3-pane dev harness), this mounts ONLY
// the Generated UI pane next to the existing server-rendered content. It fetches
// validated UI IR from `/api/gui` for the currently active file and renders it
// via the shared component registry. Built as a self-contained IIFE and
// embedded by mdpeek-server.

import { render } from "preact";
import type { SourceRange } from "./ir";
import { RenderList } from "./registry";
import { initScrolly } from "./scrolly";
import "./panel.css";

const PANEL_ID = "mdpeek-gui-panel";
const TOGGLE_ID = "mdpeek-gui-toggle";
const OPEN_CLASS = "mdpeek-gui-open";

interface GuiResponse {
  nodes: Parameters<typeof RenderList>[0]["nodes"];
  markdown: string;
}

/** Best-effort jump: scroll the server-rendered content to the nearest heading.
 * The SSR HTML has heading ids but no per-line anchors, so exact source lines
 * aren't addressable yet (that arrives with the #16 live-diff work). */
function jump(_range: SourceRange) {
  /* no-op for now; kept so components render their SourceRangeLink affordance */
}

/** Close the panel and reset the toolbar toggle's pressed state. */
function closePanel() {
  document.body.classList.remove(OPEN_CLASS);
  document.getElementById(TOGGLE_ID)?.setAttribute("aria-pressed", "false");
}

/** Panel chrome: a sticky header with a title + close button, wrapping content.
 * `loading` shows an indeterminate progress bar under the header. */
function Island({
  children,
  loading = false,
}: {
  children: preact.ComponentChildren;
  loading?: boolean;
}) {
  return (
    <>
      <div class={loading ? "gui-island__head is-loading" : "gui-island__head"}>
        <span class="gui-island__title">✨ Generated UI</span>
        <button class="gui-island__close" type="button" aria-label="Close generated UI" onClick={closePanel}>
          <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>
        </button>
      </div>
      <div class="gui-island__body">{children}</div>
    </>
  );
}

/** Indeterminate loading state (a spinner + label). The request is a single
 * fetch with no progress events, so a spinner rather than a % bar. */
function Loading() {
  return (
    <div class="gui-loading" role="status" aria-live="polite">
      <span class="gui-spinner" aria-hidden="true" />
      <span>Generating UI…</span>
    </div>
  );
}

async function loadInto(panel: HTMLElement) {
  render(<Island loading><Loading /></Island>, panel);
  try {
    const res = await fetch("/api/gui");
    if (!res.ok) throw new Error(`/api/gui returned ${res.status}`);
    const data: GuiResponse = await res.json();
    const body =
      !data.nodes || data.nodes.length === 0 ? (
        <p class="gui-status">No generated components for this document.</p>
      ) : (
        <RenderList nodes={data.nodes} onJump={jump} />
      );
    render(<Island>{body}</Island>, panel);
  } catch (e) {
    render(
      <Island>
        <p class="gui-status gui-status--error">Failed to generate UI: {String(e)}</p>
      </Island>,
      panel,
    );
  }
}

function init() {
  const toggle = document.getElementById(TOGGLE_ID);
  const panel = document.getElementById(PANEL_ID);
  if (!toggle || !panel) return;

  // Reveal the toggle (server ships it hidden so it never flashes without JS).
  toggle.hidden = false;

  // Sibling mode: reader-paced Generative Scrollytelling (design §5).
  initScrolly();

  toggle.addEventListener("click", () => {
    const open = document.body.classList.toggle(OPEN_CLASS);
    toggle.setAttribute("aria-pressed", String(open));
    // Re-fetch on every open so switching files (explorer) shows fresh output.
    if (open) loadInto(panel);
  });

  // Escape closes the panel when it's open.
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && document.body.classList.contains(OPEN_CLASS)) {
      closePanel();
    }
  });
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", init);
} else {
  init();
}
