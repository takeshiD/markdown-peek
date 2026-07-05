# mdpeek-tui-ir — IR-driven TUI renderer

A `ratatui` renderer for the mdpeek Generative-UI **IR** (see `../../AGENTS.md`
§4). It consumes the *same* IR JSON the web (Preact) renderer consumes and draws
it in the terminal, so document analysis is never re-implemented per frontend.

This is a **standalone crate** (its own `[workspace]`), decoupled from the root
`markdown-peek` package so Layer 5 can be built without touching the root
manifest. When Layer 3 lands `mdpeek-core::ir`, depend on it and delete
`src/ir.rs`.

## Build & run

```sh
cargo build --release           # produces ./target/release/mdpeek-tui-ir

# Interactive viewer (reads a file or stdin):
mdpeek-tui-ir fixtures/design-doc.json

# Non-interactive render to stdout (no TTY; great for pipes / CI / snapshots):
mdpeek-tui-ir --print fixtures/design-doc.json

# From a generator pipeline:
some-generator | mdpeek-tui-ir --print
```

### Reading-position (spoiler) control

Nodes marked `"visibility": { "until_read": { "reveal_after_line": N } }` stay
hidden until the reader has passed line `N` (AGENTS.md §9.3):

```sh
mdpeek-tui-ir --print --reveal-line 120 fixtures/design-doc.json
```

## Keys (interactive)

| Key | Action |
|---|---|
| `q` / `Esc` | quit |
| `↓` / `j`, `↑` / `k` | scroll |
| `PgDn` / `PgUp` | page |
| `g` / `G` | top / bottom |

## Input format

Either a bare array of nodes or a `{ "ui_ir": [ ... ] }` envelope (matching
`GuiCacheEntry.ui_ir`, §4.3). Unknown `kind`s are **rejected** — the renderer
only knows the fixed registry, enforcing the allowlist (DESIGN.md).

Supported node kinds: `Tabs`, `Timeline`, `Checklist`, `DataTable`, `Callout`,
`RiskPanel`, `LogTimeline`, `CommitGraph`, `Glossary`, `StepNavigator`, and
graphical `Diagram` / `DependencyGraph` (rendered as a text summary + "open in
web" hint, per §5.2).

## Known limitations (v0)

- Tables pad by character count, so rows mixing full-width (CJK) and ASCII text
  can be visually misaligned. A follow-up can switch to display-width padding
  (ratatui already pulls in `unicode-width`).
- Only the drawable subset of node kinds is implemented; the rest fall back to
  text. This matches the staged "部分集合を段階対応" plan in §5.2.

## Tests

```sh
cargo test
```

Covers each node kind's rendering, column alignment, severity glyphs, the
graphical→web fallback, spoiler control (hide then reveal), unknown-kind
rejection, envelope/bare parsing, the bundled fixture, and a headless
`TestBackend` paint.
