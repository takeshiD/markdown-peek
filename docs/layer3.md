# Layer 3 — Generated UI (implementation notes)

This document tracks the Layer 3 implementation (design: [`AGENTS.md`](../AGENTS.md)
§10 "Layer 3 — Generated UI"). It records what shipped, the deliberate scope
boundaries chosen to avoid colliding with the in-flight Layer 1 / Layer 2
worktrees, and what remains.

## What shipped

Layer 3 is built as **additive modules** on the existing single-binary crate —
no workspace restructuring (per 論点 B, that happens with Layer 2) and no edits
to the `parser` / `analyzer` / `model` areas that Layer 1 / 2 own.

| Design section | Module | Status |
|---|---|---|
| §4.1 UI IR (source of truth) | `src/ir/node.rs` | ✅ all 12 core + 6 domain nodes, `NodeMeta` flattened, `Quantity`/`Visibility`/`Origin` |
| §1 sourceRange | `src/ir/range.rs` | ✅ `SourceRange` + `LineIndex` (byte offset → line/col) |
| §3.5 / §8 allowlist | `src/ir/registry.rs` | ✅ 2-layer allowlist (core + domain) |
| §3.5 validation | `src/ir/validate.rs` | ✅ schema (serde) + allowlist + sourceRange bounds + low-confidence flagging |
| §3.4 generator | `src/generator/rules.rs` | ✅ `RulesGenerator`: task lists→`Checklist`, tables→`DataTable`, mermaid→`Diagram`, json/yaml/toml/env→`ConfigViewer`, GFM alerts→`Callout` |
| §7 LLM | `src/generator/llm/` (`feature = "llm"`) | ✅ `ClaudeGenerator` + prompt; offline fallback to rules; **not yet driven** (see below) |
| §6 cache | `src/cache/` | ✅ content-hash key (markdown + generator + schema version) + `.cache/mdpeek/*.gui.json` store |
| §1 pipeline | `src/gui.rs` | ✅ generate → validate → cache facade |
| CLI | `mdpeek gen <file>` | ✅ emits validated IR JSON; `--no-cache` |
| §5.1 web registry | `web/src/registry.tsx` | ✅ 2-layer registry + `Render` dispatcher |
| §5.1 components | `web/src/components/` | ✅ all 18 node kinds |
| §5.3 layout | `web/src/layout/ThreePane.tsx` | ✅ Outline / Content / Generated UI, SourceRangeLink jump |
| §4.1 TS types | `web/src/ir.ts` | ✅ hand-maintained mirror of Rust IR |

Tests: `cargo test` (18 Layer-3 unit tests + `tests/gen_output.rs` integration)
and `cd web && npm run build` (tsc + vite) both pass.

## Deliberately deferred (to avoid worktree interference)

These Layer 3 items touch files owned by other in-flight worktrees, or depend on
their outputs, so they are left as clean integration points:

- **Server `/api/gui` route + Preact island mount** (論点 A). Wiring the
  Generated UI pane into the live server edits `src/server.rs` and the static
  HTML — shared with Layer 1 (#12/#16). `web/dist` embedding via `include_bytes!`
  (論点 C) waits on that.
- **Layer 2 `DocumentModel` / `planner`.** `generator::traits::GenInput` is a
  lightweight stand-in (raw markdown + `DocType` hint). When Layer 2 lands,
  `GenInput` becomes a thin adapter over `DocumentModel` — the `Generator`
  contract (`-> Vec<UiNode>`) and everything downstream are unchanged.
- **`ts-rs` auto-generation of `ir.ts`.** Needs the workspace split; `ir.ts` is
  hand-kept in lockstep meanwhile.
- **#16 live diff regeneration.** Depends on the watcher channelization from
  Layer 1.

## Usage

```sh
# Deterministic, offline IR generation:
mdpeek gen README.md              # prints validated UI IR JSON, caches under .cache/mdpeek/
mdpeek gen README.md --no-cache   # always regenerate

# LLM-backed generation (opt-in; falls back to rules if ANTHROPIC_API_KEY unset):
cargo build --features llm

# Web frontend (Generated UI island):
cd web && npm install && npm run dev     # dev harness with a bundled fixture
cd web && npm run build                  # → web/dist (embedded by the server later)
```

## Security invariants (design §8)

- LLM output is **UI IR only** — enforced structurally by serde types + the
  registry allowlist + sourceRange verification in `ir::validate`. An LLM cannot
  introduce a component outside the registry or a fabricated range.
- Renderers select from a **fixed registry**; unknown `kind` renders nothing.
- No `dangerouslySetInnerHTML`; code/config render as escaped `<pre>` text.
- Low-confidence / LLM-origin nodes are badged in the UI (judgement stays human).
