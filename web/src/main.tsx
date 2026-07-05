// Preact island entry point (design doc §1 / §5).
//
// The server injects the validated UI IR + source markdown into the page (per
// 論点 A, as the Generated UI island alongside the Layer-1 SSR content). In the
// standalone dev harness we fall back to a bundled fixture so `vite dev` works
// without the Rust server.

import { render } from "preact";
import type { UiNode } from "./ir";
import { ThreePane } from "./layout/ThreePane";
import "./styles.css";

declare global {
  interface Window {
    __MDPEEK_GUI__?: { nodes: UiNode[]; markdown: string };
  }
}

async function bootstrap() {
  const root = document.getElementById("app");
  if (!root) return;

  let data = window.__MDPEEK_GUI__;
  if (!data) {
    // Dev fallback fixture.
    try {
      const res = await fetch("/gui.sample.json");
      if (res.ok) data = await res.json();
    } catch {
      /* offline dev: leave empty */
    }
  }

  render(
    <ThreePane nodes={data?.nodes ?? []} markdown={data?.markdown ?? "# No document loaded"} />,
    root,
  );
}

bootstrap();
