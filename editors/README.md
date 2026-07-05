# Layer 5 — Editor / TUI 統合

This directory implements **Layer 5** of the roadmap in [`../AGENTS.md`](../AGENTS.md) §10:

> **Layer 5 — Editor/TUI 統合**: TUI renderer (#12 の上に IR 対応)、Neovim plugin、GitHub preview 連携。

Everything here is **additive and self-contained**: it lives entirely under
`editors/` and does not modify any existing Rust source (`src/`), so it can land
in parallel with the Layer 1–4 work happening in other worktrees without merge
conflicts.

| Sub-deliverable | Path | What it is |
|---|---|---|
| TUI renderer (IR 対応) | [`tui-ir/`](tui-ir/) | A standalone `ratatui` binary that renders the Generative-UI **IR** (AGENTS.md §4) in the terminal — the same IR JSON the web (Preact) renderer consumes. |
| Neovim plugin | [`nvim/`](nvim/) | Drives the `mdpeek` binary from Neovim: live browser preview, terminal render, all via `:MdPeek*` commands. |
| GitHub preview 連携 | [`github/`](github/) | A composite Action + example workflow that renders a PR's changed Markdown to a static HTML preview artifact. |

## Status & dependency notes

- **TUI renderer**: fully working today. The IR types in `tui-ir/src/ir.rs` are a
  **provisional mirror** of `mdpeek-core::ir` (Layer 3). The wire format is
  identical serde JSON, so when Layer 3 lands its `ir` crate, `tui-ir` should
  depend on it and delete its local `ir.rs` — the renderer (`render.rs`) is
  unaffected. Per AGENTS.md §5.2 the TUI renders the drawable subset of nodes and
  falls back to a text summary + "open in web" hint for graphical ones
  (Diagram / DependencyGraph). Reading-position spoiler control (§9.3,
  `Visibility::UntilRead`) is implemented.
- **Neovim plugin** and **GitHub integration**: work against the *current*
  `mdpeek serve` / `mdpeek term` CLI — no dependency on Layer 1–4.

## Design invariants honoured

From DESIGN.md's "重要な設計思想", carried into every piece here:

- **The renderer is deterministic** — same IR ⇒ same output (that's why `tui-ir`
  is unit-testable with a headless backend).
- **Unknown component `kind`s are rejected** — `tui-ir` only knows a fixed
  registry; serde rejects anything else.
- **No arbitrary code execution** — the IR is pure data; nothing here evals it.
- **Rendering is never re-implemented per frontend** — the Neovim plugin and the
  GitHub action both shell out to the one `mdpeek` binary (AGENTS.md §1.1).
