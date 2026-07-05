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
| §3.4 generator | `crates/mdpeek-gui/src/generator/rules.rs` | ✅ `RulesGenerator` (structural): task lists→`Checklist`, tables→`DataTable`, mermaid→`Diagram`, json/yaml/toml/env→`ConfigViewer`, GFM alerts→`Callout` |
| §3.3 / §9 planner + Layer 2 | `crates/mdpeek-gui/src/planner` | ✅ consumes `mdpeek_analyzer::analyze` (DocumentModel + SemanticPanel); doctype-aware semantic nodes: risks→`RiskPanel`, open questions→`Checklist`, DesignDoc/Readme→missing-section review `Checklist`, Adr/Changelog/Minutes→`Timeline` |
| §7 LLM | `src/generator/llm/` | ✅ 3 backends: `claude_code` (`claude` CLI) + `codex` (`codex` CLI) in the default build, `anthropic_api` (HTTP) behind `feature = "llm"`; model + effort per backend; validates output; rules fallback |
| §6 cache | `src/cache/` | ✅ content-hash key (markdown + generator + schema version) + `.cache/mdpeek/*.gui.json` store |
| §1 pipeline | `src/gui.rs` | ✅ generate → validate → cache facade (rules or LLM) |
| CLI | `mdpeek gen <file>` | ✅ emits validated IR JSON; `--no-cache`, `--llm`, `--provider`, `--model`, `--effort` |
| §5.1 web registry | `web/src/registry.tsx` | ✅ 2-layer registry + `Render` dispatcher |
| §5.1 components | `web/src/components/` | ✅ all 18 node kinds |
| §5.3 layout | `web/src/layout/ThreePane.tsx` | ✅ Outline / Content / Generated UI, SourceRangeLink jump (standalone dev harness) |
| §5.3 / 論点 A server integration | `crates/mdpeek-server` + `web/src/panel.tsx` | ✅ `/api/gui` endpoint + Preact island co-existing with SSR content, toggled from the toolbar; `web/dist` embedded via `include_bytes!` (論点 C) |
| §4.1 TS types | `web/src/ir.ts` | ✅ hand-maintained mirror of Rust IR |

Layer 3 core now lives in its own crate (`crates/mdpeek-gui`, design §2 `mdpeek-core`)
so both the CLI and the server share one implementation.

Tests: `cargo test --workspace` (mdpeek-gui unit tests + `tests/gen_output.rs`
integration) and `cd web && npm run build` (tsc + vite) all pass.

## Deliberately deferred (to avoid worktree interference)

These Layer 3 items depend on other layers' outputs, so they are left as clean
integration points:

- **Deeper Layer 2 use.** The planner now consumes `mdpeek_analyzer::analyze`
  for doctype-aware nodes (risks, open questions, review checklist, timeline).
  Still to do: block-class-driven `Tabs` (group content by section), ADR
  decision graphs, and richer per-doctype layouts (§9 tables).
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

# Live server: open the preview, then click the ✨ toolbar button to reveal the
# Generated UI pane (fetches /api/gui for the active file; rules by default,
# LLM when [llm] enabled = true).
mdpeek serve README.md

# Web frontend build (rebuild after changing web/src; commit web/dist):
cd web && npm install
cd web && npm run build                  # → web/dist/mdpeek-gui.{js,css} (embedded by the server)
cd web && npm run dev                    # standalone 3-pane dev harness with a fixture
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
