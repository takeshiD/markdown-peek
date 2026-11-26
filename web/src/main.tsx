// Entry point of the mdpeek web UI (served by `mdpeek serve`).
//
// The server embeds the first document in #mdpeek-boot, so the body renders
// on the first frame; reading-aid selection, the file tree and the WebSocket
// follow asynchronously and never block the text.
import { createRoot } from "react-dom/client";
import "./app.css";
import { App } from "./components/App";
import { initStore } from "./lib/store";
import { connectSocket, fetchAdaptive, fetchTree } from "./lib/actions";
import type { DocPayload } from "./lib/types";

function readBoot(): DocPayload {
  const el = document.getElementById("mdpeek-boot");
  try {
    if (el?.textContent) return JSON.parse(el.textContent) as DocPayload;
  } catch {
    /* fall through */
  }
  return { path: "", title: "markdown-peek", html: "", frontmatter: "", blocks: [], revision: 0, error: "No document was provided by the server." };
}

initStore(readBoot());
createRoot(document.getElementById("root")!).render(<App />);
connectSocket();
void fetchTree();
void fetchAdaptive("initial");
