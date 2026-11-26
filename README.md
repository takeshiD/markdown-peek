# Markdown Peek
Markdown Peek (`mdpeek`) is a lightweight, repository-aware CLI tool that watches your markdown and renders it live — either in your browser or in an interactive terminal viewer.

- [Features](#features)
- [Quick Start](#quick-start)
- [Installation](#installation)
- [Configuration](#configuration)
- [Status](#status)
- [License](#license)

# Features

### Available now
- ⚡ Fast — a single binary with all assets embedded
- 🔄 Live, in-place updates — only the blocks you changed re-render (no full reload), your scroll position is kept, and changed blocks are briefly highlighted
- 🌐 Browser preview with a repository/worktree file explorer, outline + fuzzy heading search, a front matter panel, and light/dark themes
- 🔍 Two-file diff — source or rendered, unified or side-by-side, including the same file across worktrees/branches
- 🖥️ Interactive terminal viewer (TUI) — scrolling, wrapping, vim-style search, and live updates without flicker
- 📝 GitHub Flavored Markdown (GFM); GitLab Flavored Markdown (GLFM) planned
- ⚙️ Easy to configure via TOML (XDG)

### Planned (roadmap)
mdpeek is growing from a strong markdown viewer into a tool that understands your
documents and renders a UI tailored to each. Upcoming layers:

- 🧠 **Semantic view** — a document model with document-type detection and a side panel of outline / TODOs / risks / open questions.
- 🎛️ **Generated UI** — a UI generated per document type (design doc, README, ADR, meeting notes, runbook, changelog, …), first by rules and then by Claude (LLM).
- 📚 **Non-developer document types** — domain widgets for novels, contracts, and manufacturing/procedure docs (character roster, tolerance meter, step navigator, obligation matrix, glossary), with spoiler control.
- 🔗 **Repository-aware checks** — README ↔ code links, docs/code consistency, ADR ↔ git history, and TODO ↔ issue cross-references.
- 🧩 **Editor & tooling integration** — an IR-aware TUI renderer, a Neovim plugin, and GitHub preview integration.


# Quick start

## Preview in your browser
Run `mdpeek` with no arguments. It discovers the markdown in your current git repository (and any linked worktrees) and opens a browser preview with a file-explorer sidebar:
```sh
mdpeek
 INFO mdpeek::server: Listening on http://127.0.0.1:3030
```
Or point it at a single file: `mdpeek serve path/to/file.md`.

The preview updates **in place** as you edit — only the blocks that changed re-render (your scroll position is kept) and they are briefly highlighted. From the sidebar you can:
- switch between files across worktrees/branches (toggle grouping by worktree or branch),
- open a **two-file diff** with the ⇄ compare buttons — source or rendered, unified or side-by-side,
- toggle the outline (with fuzzy heading search), the color theme, and auto-scroll-to-change.

A breadcrumb shows which worktree/branch the open file belongs to.

## Preview in your terminal
Use the `term` subcommand. On a TTY it opens an interactive full-screen viewer that live-updates as the file changes:
```sh
mdpeek term             # interactive viewer (watches by default on a TTY)
mdpeek term --no-watch  # render once and exit (also the default when piped)
```

### Terminal viewer keybindings
| Key                  | Action                      |
| -----                | --------                    |
| `q` / `Ctrl-c`       | quit                        |
| `j` / `k`, `↓` / `↑` | scroll one line             |
| `Ctrl-d` / `Ctrl-u`  | half page down / up         |
| `PgDn` / `PgUp`      | page down / up              |
| `g` / `G`            | jump to top / bottom        |
| `/`                  | search                      |
| `n` / `N`            | next / previous match       |
| `Esc`                | clear search                |
| `?`                  | toggle the keybindings help |

# Installation
## `cargo`
```
cargo install markdown-peek
```

## `nix`(Planed)
```
nix-shell -p markdown-peek --command mdpeek
```

## `npm`(Planed)
```
npm install -g markdown-peek
```

## download prebuild-binary
```
curl -SL https://github.com/takeshid/markdown-peek
```


# Configuration
`mdpeek` reads an optional configuration file following the [XDG Base Directory](https://specifications.freedesktop.org/basedir-spec/latest/) specification:

```
$XDG_CONFIG_HOME/mdpeek/config.toml
```

When `XDG_CONFIG_HOME` is unset, it falls back to `~/.config/mdpeek/config.toml`. The file is optional; any missing key uses its built-in default. Settings are resolved with the following precedence:

```
CLI arguments  >  config file  >  built-in defaults
```

A different config file can be loaded with `-c`/`--config`, which overrides the default XDG location:

```sh
mdpeek --config ./my-config.toml
mdpeek -c ./my-config.toml term README.md
```

## Options

| Key | Values | Default | Description |
|-----|--------|---------|-------------|
| `default_mode` | `serve` \| `term` | auto (`serve` on a TTY, otherwise `term`) | Mode used when `mdpeek` is run without a subcommand |
| `server.host` | IP string | `127.0.0.1` | Address the browser preview binds to |
| `server.port` | port string | `3030` | Port the browser preview listens on |
| `server.theme` | `light` \| `dark` | `light` | Default browser preview theme |
| `term.theme` | `glow` \| `mono` \| `catputtin` \| `dracura` \| `solarized` \| `nord` \| `ayu` | `glow` | Default terminal color theme |
| `term.pager` | command string | `$PAGER`, else `less -R` | Pager for long terminal output; set to `""` to disable paging |

## Example
See [`config.example.toml`](config.example.toml) for a complete, commented example. To get started:

```sh
mkdir -p ~/.config/mdpeek
cp config.example.toml ~/.config/mdpeek/config.toml
```

```toml
default_mode = "serve"

[server]
host = "127.0.0.1"
port = "3030"
theme = "light"

[term]
theme = "glow"
pager = "less -R"
```

## Adaptive reading aids (`[select]`)

`mdpeek select <file>` prints an **AdaptiveLayout** JSON: for each section, which
reading-aid widget to show (step navigator, checklist, table view, command list,
timeline, risk callout, …) or why nothing is shown (`skipped`). `mdpeek serve`
exposes the same layout at `GET /api/adaptive` and pushes
`{"type":"adaptive","revision":N,"layout":{…}}` over the WebSocket after each file
change (debounced, default 800 ms; results for outdated revisions are dropped,
unchanged sections reuse in-memory decisions — nothing is written to disk).

Widget *content* is always extracted deterministically from your document and
carries its source line range; a selector only picks among candidates that
already fit the section.

| Key (`[select]`) | Default | Description |
|---|---|---|
| `provider` | `rules` | `rules` (offline), `jev` (TypeSafe AI Jev), `fake` (scripted, for tests / E2E) |
| `allow_network` | `false` | Must be `true` for `jev` to send anything |
| `model` / `base_url` | `jev-1.14.0` / `https://api.typesafe.ai` | Jev model and endpoint |
| `timeout_ms` | `4000` | Whole-request deadline (≤ 1 retry on 429/529, honouring `retry-after`) |
| `helps_threshold` / `confidence_threshold` | `0.5` / `0.6` | Hide widgets Jev considers unhelpful / is unsure about |
| `debounce_ms` | `800` | `serve`: delay before re-selecting after a change |
| `fake_script` | – | JSON script for `provider = "fake"` |

**External sending is opt-in.** Nothing leaves your machine unless
`provider = "jev"` **and** `allow_network = true` (or `mdpeek select --provider jev
--allow-network`). Jev then receives one request per document with headings, a
short excerpt per section (≤ 600 chars) and numeric features — never the full text.
Keep confidential documents on `rules`.

**The API key is read only from the `TYPESAFE_API_KEY` environment variable** of
the `mdpeek` process. It cannot be set in `config.toml` or on the command line,
and it is never printed, logged or returned to the browser. Without a key — or on
timeout, 401, 429/529, 5xx, malformed responses or no network — `mdpeek` falls
back to `rules` and reports why in `selector.fallback`.

```sh
mdpeek select README.md                                  # rules, offline
TYPESAFE_API_KEY=… mdpeek select README.md --provider jev --allow-network
mdpeek select README.md --provider fake --fake-script tests/fake-select.json   # scripted
mdpeek select-eval tests/fixtures/select                 # accuracy / none / p50-p95
```

### Fake provider script (`provider = "fake"`)

For tests and web E2E without a key, `fake` answers from a JSON script
(example: [`tests/fake-select.json`](tests/fake-select.json)). It never touches the
network and gives the same answer for the same document every time.

```json
{
  "model": "fake-e2e",
  "delay_ms": 0,
  "fail": null,
  "doc_type": { "value": "runbook", "confidence": 0.95 },
  "document": { "widget": "ActionItems", "confidence": 0.9, "helps": 0.9 },
  "sections": {
    "Steps": { "widget": "StepNavigator", "confidence": 0.9, "helps": 0.9 },
    "Commands": { "helps": 0.1 }
  },
  "default": "rules"
}
```

- `sections` keys are section headings (or positional ids `s0`, `s1`, …). Each entry
  has optional `widget` (a candidate id such as `StepNavigator`,
  `ConfigViewer:commands`, `DataTable:status`, `Callout:risk`; omitted = the rules
  choice), `helps` and `confidence` (0–1, default 1). The usual thresholds apply, so a
  low `helps` / `confidence` hides the widget; an id that is not a candidate for that
  section falls back to rules (`fallback_invalid_answer`).
- `document`: the document-level panel (`SemanticOutline`, `ActionItems`, `RiskPanel`, …).
- `default`: for unlisted sections — `"rules"` (default), `"missing"` (treated as an
  invalid answer), or an entry object.
- `fail`: make every call fail with that fallback code (e.g. `"timeout"`);
  `delay_ms`: simulated latency (exceeding `timeout_ms` yields `timeout`).
- A missing or malformed script gives `selector.fallback = "fake_script_error"`, and
  the reason (I/O error kind or JSON error position) is printed on stderr.


# Status
## Viewer
- [x] Live in-place updates with changed-block highlighting
- [x] Repository + worktree file explorer sidebar (group by worktree / branch)
- [x] Breadcrumb showing the active file's worktree/branch
- [x] Outline panel with fuzzy heading search
- [x] Front matter panel
- [x] Two-file diff (source / rendered, unified / split; across worktrees)
- [x] Interactive terminal viewer (scroll, wrap, vim-style search, live update)

## [GFM](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax)
- [x] [Table](https://github.github.com/gfm/#tables-extension-)
- [x] [TaskList](https://github.github.com/gfm/#task-list-items-extension-)
- [x] [Strike throough](https://github.github.com/gfm/#strikethrough-extension-)
- [x] [Fenced Code](https://github.github.com/gfm/#fenced-code-blocks)
- [x] Syntax Hightlight
- [x] [Emoji](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#using-emojis)
- [x] [Alert](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#alerts)
- [x] MathJax
- [x] [Color Model](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#supported-color-models)
- [x] Auto Link
- [x] InPage Link
- [x] Footnote
- [x] Table of Contents
- [x] Theme Switch(Light/Dark)

## [GLFM(Planed)](https://docs.gitlab.com/user/markdown/)
- [ ] Table
- [ ] Strike throough
- [ ] TaskList
- [ ] Fence Code
- [ ] Syntax Hightlight
- [ ] Auto Link
- [ ] Emoji
- [ ] [Alert](https://docs.gitlab.com/user/markdown/#alerts)
- [ ] Math equation

## Terminal
- [x] [Table](https://github.github.com/gfm/#tables-extension-)
- [x] [TaskList](https://github.github.com/gfm/#task-list-items-extension-)
- [x] [Strike throough](https://github.github.com/gfm/#strikethrough-extension-)
- [x] [Fenced Code](https://github.github.com/gfm/#fenced-code-blocks)
- [x] Syntax Hightlight
- [x] [Emoji](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#using-emojis)
- [x] [Alert](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#alerts)
- [x] Math Equation
- [x] [Color Model](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#supported-color-models)
- [x] Footnote

# License
MIT License.
[LICENSE](LICENSE).
