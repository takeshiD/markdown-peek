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

async function loadInto(panel: HTMLElement) {
  render(<p class="gui-status">Generating UI…</p>, panel);
  try {
    const res = await fetch("/api/gui");
    if (!res.ok) throw new Error(`/api/gui returned ${res.status}`);
    const data: GuiResponse = await res.json();
    if (!data.nodes || data.nodes.length === 0) {
      render(<p class="gui-status">No generated components for this document.</p>, panel);
      return;
    }
    render(<RenderList nodes={data.nodes} onJump={jump} />, panel);
  } catch (e) {
    render(<p class="gui-status gui-status--error">Failed to generate UI: {String(e)}</p>, panel);
  }
}

function init() {
  const toggle = document.getElementById(TOGGLE_ID);
  const panel = document.getElementById(PANEL_ID);
  if (!toggle || !panel) return;

  // Reveal the toggle (server ships it hidden so it never flashes without JS).
  toggle.hidden = false;

  toggle.addEventListener("click", () => {
    const open = document.body.classList.toggle(OPEN_CLASS);
    toggle.setAttribute("aria-pressed", String(open));
    // Re-fetch on every open so switching files (explorer) shows fresh output.
    if (open) loadInto(panel);
  });
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", init);
} else {
  init();
}
