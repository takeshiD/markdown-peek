import { defineConfig } from "vite";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath } from "node:url";

// The web UI is built to two stable files, dist/app.js + dist/app.css, which
// mdpeek-server embeds with include_bytes! (design 論点 C: dist is committed
// and must be reproducible with `npm ci && npm run build`). No code splitting,
// no hashed names, no external URLs.
export default defineConfig({
  plugins: [tailwindcss()],
  esbuild: { jsx: "automatic" },
  resolve: {
    alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2022",
    cssCodeSplit: false,
    modulePreload: false,
    chunkSizeWarningLimit: 800,
    rollupOptions: {
      // "use client" directives (radix, cmdk, lucide) are meaningless in a
      // client-only bundle; keep the build log readable.
      onwarn(w, warn) {
        if (w.code === "MODULE_LEVEL_DIRECTIVE" || w.code === "SOURCEMAP_ERROR") return;
        warn(w);
      },
      input: "src/main.tsx",
      output: {
        format: "es",
        entryFileNames: "app.js",
        assetFileNames: "app.[ext]",
        inlineDynamicImports: true,
      },
    },
  },
});
