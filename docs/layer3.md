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
| §7 LLM | `src/generator/llm/` | ✅ 3 backends: `claude_code` (`claude` CLI) + `codex` (`codex` CLI) in the default build, `anthropic_api` (HTTP) behind `feature = "llm"`; model + effort per backend; validates output; rules fallback |
| §6 cache | `src/cache/` | ✅ content-hash key (markdown + generator + schema version) + `.cache/mdpeek/*.gui.json` store |
| §1 pipeline | `src/gui.rs` | ✅ generate → validate → cache facade (rules or LLM) |
| CLI | `mdpeek gen <file>` | ✅ emits validated IR JSON; `--no-cache`, `--llm`, `--provider`, `--model`, `--effort` |
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
# Deterministic, offline IR generation (rules):
mdpeek gen README.md              # prints validated UI IR JSON, caches under .cache/mdpeek/
mdpeek gen README.md --no-cache   # always regenerate

# LLM-backed generation. Backend + model + effort come from [llm] in config.toml,
# or from CLI flags (which override config). Falls back to rules on any failure.
mdpeek gen README.md --llm --provider claude_code --model claude-sonnet-5 --effort high
mdpeek gen README.md --llm --provider codex        --model gpt-5-codex     --effort medium

# The `anthropic_api` backend (direct HTTP) needs a feature build + API key:
cargo build --features llm
ANTHROPIC_API_KEY=... mdpeek gen README.md --llm --provider anthropic_api

# Web frontend (Generated UI island):
cd web && npm install && npm run dev     # dev harness with a bundled fixture
cd web && npm run build                  # → web/dist (embedded by the server later)
```

### LLM backends

| provider | build | needs | model flag | effort mapping |
|---|---|---|---|---|
| `claude_code` | default | `claude` CLI on PATH | `claude --model` | prompt keyword (`think`/`ultrathink`) |
| `codex` | default | `codex` CLI on PATH | `codex --model` | `-c model_reasoning_effort="…"` |
| `anthropic_api` | `--features llm` | `ANTHROPIC_API_KEY` | request `model` | advisory only |

## Security invariants (design §8)

- LLM output is **UI IR only** — enforced structurally by serde types + the
  registry allowlist + sourceRange verification in `ir::validate`. An LLM cannot
  introduce a component outside the registry or a fabricated range.
- Renderers select from a **fixed registry**; unknown `kind` renders nothing.
- No `dangerouslySetInnerHTML`; code/config render as escaped `<pre>` text.
- Low-confidence / LLM-origin nodes are badged in the UI (judgement stays human).
