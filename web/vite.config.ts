import { defineConfig } from "vite";

// Preact via aliasing react/jsx-runtime to preact (no @preact/preset-vite dep
// needed; keeps the toolchain minimal). Build emits to `dist/`, which the
// server embeds with `include_bytes!` (design 論点 C: commit dist in-tree).
export default defineConfig({
  esbuild: {
    jsx: "automatic",
    jsxImportSource: "preact",
  },
  resolve: {
    alias: {
      "react/jsx-runtime": "preact/jsx-runtime",
    },
  },
  build: {
    // Build the Generated-UI island as a self-contained IIFE with stable file
    // names, so mdpeek-server can embed it via include_bytes! (論点 C). The
    // standalone dev harness (index.html → main.tsx) still runs via `vite dev`.
    lib: {
      entry: "src/panel.tsx",
      formats: ["iife"],
      name: "MdpeekGui",
      fileName: () => "mdpeek-gui.js",
    },
    outDir: "dist",
    emptyOutDir: true,
    target: "es2020",
    rollupOptions: {
      output: {
        // Stable CSS name for include_bytes! (default lib output is style.css).
        assetFileNames: "mdpeek-gui.[ext]",
      },
    },
  },
});
