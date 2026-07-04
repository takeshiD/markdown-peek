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
    outDir: "dist",
    emptyOutDir: true,
    target: "es2020",
  },
});
