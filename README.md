# Markdown Peek

English | [日本語](README.ja.md)

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
- 🌐 Browser preview with source-linked GenUI (**charts, node graphs and comparison cards set into the text**) behind one on/off toggle, chosen by the model you pick — **Jev** (recommended), another decision model (GPT-6 Luna Decision, Gemini Decision, Microsoft Decision) or a general LLM (Claude, GPT, GLM, DeepSeek…) — with a spinner, stop / resume and an on-disk decision cache, a command palette (Ctrl/⌘K), a file explorer for the current branch, outline + fuzzy heading search, a front matter panel, light/dark/system themes, and full keyboard support
- 🖥️ Interactive terminal viewer (TUI) — scrolling, wrapping, vim-style search, and live updates without flicker
- 📝 GitHub Flavored Markdown (GFM); GitLab Flavored Markdown (GLFM) planned
- ⚙️ Easy to configure via TOML (XDG)

### Planned (roadmap)
mdpeek is growing from a strong markdown viewer into a tool that understands your
documents and renders a UI tailored to each. Upcoming layers:

- 🧠 **Semantic view** — a document model with document-type detection and a side panel of outline / TODOs / risks / open questions.
- 🎛️ **Generated UI** — letting an LLM write GenUI views, not only choose them (being considered in [#58](https://github.com/takeshiD/markdown-peek/issues/58); the earlier `mdpeek gen` / `/api/gui` were removed until then).
- 📚 **Non-developer document types** — domain widgets for novels, contracts, and manufacturing/procedure docs (character roster, tolerance meter, step navigator, obligation matrix, glossary), with spoiler control.
- 🔗 **Repository-aware checks** — README ↔ code links, docs/code consistency, ADR ↔ git history, and TODO ↔ issue cross-references.
- 🧩 **Editor & tooling integration** — an IR-aware TUI renderer, a Neovim plugin, and GitHub preview integration.


# Quick start

## Preview in your browser
Run `mdpeek` with no arguments. It discovers the markdown in the current branch's working tree (other worktrees are not listed) and opens a browser preview with a file-explorer sidebar:
```sh
mdpeek
 INFO mdpeek::server: Listening on http://127.0.0.1:3030
```
Or point it at a single file: `mdpeek serve path/to/file.md`.

The preview updates **in place** as you edit — only the blocks that changed re-render (your reading position is kept) and they are briefly marked.

### The browser UI

![GenUI views arriving one by one with a spinner, ticking a checklist and stepping through steps inside the text, following a node graph's RETRY exit, focusing comparison cards on one criterion, switching the model for the session and making it the default, and turning GenUI off and on](docs/design/media/adaptive-tour.gif)

The window has a top bar (repository / file breadcrumb, the **GenUI controls**, command palette, theme), **Files** on the left, the document, and the **Outline** on the right. With GenUI on, the document carries small widgets (steps, checklists, tables, charts, node graphs, comparison cards, directory trees, risks, action items, …) built from your document and **set into the text right after the lines they came from**, each linked to those source lines: a chart under its table, a step navigator under its list, a stage flow under a section that walks through stages. A **sortable table or an interactive checklist takes the place of the table / task list it was built from** (so it is not shown twice); its *Original* button switches that block back to the Markdown rendering, and turning GenUI off restores every block. Code blocks in the text have a **copy button**. Whole-document views (outline by role, glossary, …) sit folded after the opening section. There is no GenUI side pane.

The GenUI controls, left to right:

| Control | What it does |
|---|---|
| **GenUI** toggle (`a`) | GenUI on or off. Off shows the text alone. While a model is choosing, the toggle shows a **spinner**. |
| **Model** menu | The models from your config (`[models.<name>]`). Picking one applies **to this session only**; a model whose settings cannot work is shown with the reason and cannot be picked. |
| **Make default** | Appears only while a model other than the default is in use; saves it as `[model] use` in your config file (comments and the rest of the file are kept). |
| Status | Choosing *n/N*, Ready, Stopped or Failed; its popover says which model decided, what was sent, the document type and the cache directory. |
| **Stop** / **Resume** | Stop a running choice (views chosen so far stay); Resume asks only what is still missing (decisions are cached per section). |

Settings that cannot work (an unset key variable, a key written in the config, an unknown `api`, …) are reported in a **notification** that names the problem and the config file.

- **Charts**: a table whose columns are measures (`120 ms`, `$4.20`, `35%`, `1,200`) becomes a bar chart, or a line chart when its rows are dates, months, quarters or versions. Every value is a parsed table cell — nothing is estimated — and the tooltip and the screen-reader table show the cell text exactly as written. A column with mixed units, row numbers / IDs, or text is never plotted; a table with several measures shows one at a time with a switch (never two axes). The line chart can be read with ← / → when focused.
- **Node graphs**: a flow written as arrows in a list (`Planning (Orchestrator) → Frontend, Backend`, `Verification --PASS--> Release`, `A -->|RETRY| B`) or as a table with *from* / *to* columns is drawn top-down; an edge written after the main flow that closes a loop (a retry) is dashed. Pick a node (chips below the graph, or click it) to see its description from the document (`- **Name**: …` or `Name (subtitle)`), where it comes from and where it goes; a node with labelled exits gets a switch (e.g. *PASS / RETRY*) that highlights where that exit leads.
- **Stage flows**: a section whose subsections walk through the stages of a process one by one (`### 3.1 parser — markdown -> BlockTree`, `### 3.2 analyzer — …`) gets a top-down flow of those stages under its own text; the input / output a heading states (`A -> B`) labels the arrow, and each node links to its subsection.
- **Directory trees**: a code block drawing a layout with `├──` / `└──` / `│` (or `|--`, `` `-- ``) becomes a tree whose folders open and close (keyboard: arrows, Home / End, Enter to jump to the line), with the `# comments` and ★ marks of the original.
- **Comparison cards**: a table comparing 2–4 options on scored criteria (numbers out of 5 / 10 / 100, ★★★☆☆, or ◎ ○ △ ×) becomes one card per option with the scores as bars; other rows (durations, prices, …) are shown verbatim. The figure is the plain average of the option's scores (labelled as such); *Focus* on one criterion shows that cell and marks the best option. A **Recommended** badge appears only when the document says which option it recommends (e.g. `… Agent Swarm を推奨する` or `(推奨)` in the header), and the sentence is quoted under the cards.

- GenUI views are **operable**, not copies of the text: tick checklist / action items (with a progress meter), step through a procedure (Step n of N, previous / next), sort and filter tables. Code is copied with the copy button on each code block (commands are never run). Ticks and the current step are kept per document in this browser only; **the Markdown file is never modified**, and *Reset to file* returns to what the file says.
- Click a widget's line range (e.g. `L21–26`) to scroll to those lines and highlight them. While you scroll, the current section is marked in the Outline and on its view. Fold any view from its title.
- The ⓘ button on a widget explains **why it was chosen**: the model, confidence, how much it helps and the other candidates.
- Views a model has just chosen **rise into the text with a short overshoot** and a fading ring, so you can see what was added; views reused from the cache appear without motion, and *reduce motion* keeps only the ring.
- **Pin** a view to keep it in view: it moves to a strip under the title that **sticks to the top of the document while you read** (capped in height; it scrolls inside). **Hide** a view and a notification offers **Undo**; later, bring hidden views back from *n hidden* above the text or from the command palette. Both are remembered per document.
- While the model is choosing after an edit, the previous widgets stay visible; results for an older version of the file are never shown.
- **Outline** (right): fuzzy heading filter and the current section marked as you scroll. Its header can **float** it over the document — drag the floating panel by its header (or focus its grip and use the arrow keys) — or **dock** it back as a sidebar; `]` hides or shows it.
- Both sidebars can be resized by dragging the divider or by focusing it and pressing ← / →; `[` folds the Files rail. The GenUI toggle, theme, sizes, folds and the floating outline's place are remembered across reloads.
- **Files** lists the Markdown of the branch you are on; the breadcrumb shows the repository and branch.
- **Guided reading** (`g`), *Follow live changes* and the shortcut list (`?`) are in the command palette.
- Turning GenUI on or off or folding a sidebar keeps your place in the document.
- On a phone (narrow screens) the document fills the screen with the views in it; Files/Outline open as a sheet.

**Keyboard shortcuts** (`?` shows them in the app; single keys work when no text field has focus):

| Key | Action |
|---|---|
| `Ctrl`+`K` (`⌘K` on macOS) | Command palette: open a file, jump to a heading, GenUI on / off, stop / resume, use another model, make it the default, theme, pin / hide / show views, guided reading, follow live changes, keyboard shortcuts |
| `a` | GenUI on or off |
| `[` | Fold or unfold the Files rail |
| `]` | Show or hide the Outline (docked or floating) |
| `/` | Filter headings in the Outline |
| `g` | Guided reading |
| `t` | Next theme (light → dark → system) |
| `?` | Shortcut list |
| `←` / `→` on a focused divider | Resize the pane |
| `Esc` | Close a dialog, sheet or popover |

GenUI views are chosen by a **model** (Jev by default; see [GenUI and models](#genui-and-models)). With GenUI on, `mdpeek` sends the chosen model headings, a short excerpt per section and counts — never the whole file; turn GenUI off (or set `[genui] default = "off"`) for documents that must not leave your machine. If the model cannot answer, no views are shown and the UI says why.

### Building the web UI

The UI is a React 19 + Tailwind CSS v4 + shadcn/ui app in `web/`, built to `web/dist/app.{js,css}` and embedded in the binary (the built files are committed, so `cargo install` needs no Node). After changing `web/src`:

```sh
cd web && npm ci && npm run build     # rebuild web/dist (reproducible: scripts/check-web-dist.sh)
npm run test:e2e                      # Playwright E2E against a real mdpeek (test-only fake model, no key, no network)
```

The E2E suite uses the system Chrome (`MDPEEK_CHROME` overrides the path; Playwright's own browser download is not used) and also refreshes the reference screenshots in `docs/design/screenshots/`.

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
width = "88ch"   # text column: a length ("88ch", "850px", "60em") or a share of the pane ("95%", "3/4")

[term]
theme = "glow"
pager = "less -R"
```

## GenUI and models

Every GenUI component, with the Markdown it is built from and a picture (a GIF for the interactive ones), is in [docs/components.md](docs/components.md) (in Japanese).

`mdpeek serve` shows GenUI views chosen by a model; `mdpeek select <file>` prints
the same choice as an **AdaptiveLayout** JSON (for each section, which widget to
show — step navigator, checklist, table view, chart, node graph, stage flow,
comparison cards, directory tree, timeline, risk callout, … — or why nothing is shown).

**A model only picks; it never writes content.** Each widget is extracted from your
document deterministically and carries its source line range; the model chooses
among the candidates that already fit each section and says how much they help.

### Kinds of models

| `api` | Kind | Examples | Endpoint (`base_url` + …) | Key header |
|---|---|---|---|---|
| `systemone` | decision model | **TypeSafe Jev (recommended)**, GPT-6 Luna Decision, Gemini Decision 1, Microsoft Decision | `/v1/systemone` | `Authorization: Bearer` (`auth = "api-key"` for Azure) |
| `anthropic` | LLM | Claude Haiku / Sonnet / Opus | `/v1/messages` | `x-api-key` (`auth = "bearer"` through OpenCode) |
| `openai` | LLM | GPT-6 Luna / Sol (Responses API) | `/v1/responses` | `Authorization: Bearer` |
| `openai_chat` | LLM | GLM, DeepSeek, Kimi… (OpenAI-compatible) | `/v1/chat/completions` | `Authorization: Bearer` |
| `claude_code` / `codex` | LLM | the `claude` / `codex` CLIs signed in on this machine | – | – |

Decision models answer the questions directly (a probability per yes/no question,
a choice with its confidence). LLMs get the same questions as a JSON prompt and
must answer with the same JSON; prose without JSON counts as an invalid answer.
Letting an LLM *write* views is not supported (see issue #58).

### Configuration

```toml
[genui]
default = "on"           # the toggle's initial state: "on" | "off"
auto_generate = true     # start choosing when a document opens; false = wait for the reader

[model]
use = "jev"              # the default model: a key of [models]
timeout_ms = 30000       # per request
batch_sections = 8       # sections per request (also the unit of stop / resume)
excerpt_chars = 600      # excerpt per section sent to the model
max_state_chars = 24000  # total sent per request
helps_threshold = 0.5    # hide views the model considers unhelpful…
confidence_threshold = 0.6  # …or is unsure about
debounce_ms = 800        # serve: wait after an edit before choosing again

[models.jev]                       # TypeSafe directly
enable = true                      # false: keep the entry, but do not offer or use it
api = "systemone"
model = "jev-1.14.0"
api_key_env = "TYPESAFE_API_KEY"   # the NAME of the variable, never the key

# Through OpenCode (one key for every model below)
[models.gpt-luna]
api = "systemone"
model = "gpt-6-luna-decision"
base_url = "https://opencode.ai/inference/systemone"
api_key_env = "OPENCODE_API_KEY"

[models.gemini]
api = "systemone"
model = "gemini-decision-1"      # OpenCode also has the free "jev-1.13-free"
base_url = "https://opencode.ai/inference/systemone"
api_key_env = "OPENCODE_API_KEY"

[models.haiku]
api = "anthropic"
model = "claude-haiku-5-5"
base_url = "https://opencode.ai/inference/anthropic"
api_key_env = "OPENCODE_API_KEY"
auth = "bearer"

[models.glm]
api = "openai_chat"
model = "glm-5.3"
base_url = "https://opencode.ai/inference/openai"
api_key_env = "OPENCODE_API_KEY"

# Microsoft Decision on Azure Foundry (your deployment name as `model`)
[models.microsoft]
api = "systemone"
model = "<your deployment>"
base_url = "https://<your-resource>.services.ai.azure.com/providers/microsoft"
api_key_env = "AZURE_API_KEY"
auth = "api-key"

[cache]
enabled = true
dir = "~/.cache/mdpeek"  # default: $XDG_CACHE_HOME/mdpeek, else ~/.cache/mdpeek
max_size_mb = 100        # oldest entries go first beyond this
ttl_days = 30            # older decisions are not used
memory_entries = 256     # in-process cache in front of the disk
```

Without a `[models]` table, a single `jev` entry (TypeSafe, `TYPESAFE_API_KEY`) is
used. Each entry also accepts `effort` (`low` … `max`; Anthropic `output_config.effort`,
OpenAI `reasoning.effort`) and `kind` (`decision` / `llm`, checked against `api`).
`enable = false` turns an entry off without deleting it: it is not in the Model menu and
is never called. If `[model] use` names it, the first enabled entry is used (and the
UI says so); if every entry is off, no model is called.

**Keys.** An entry names the environment variable that holds its key
(`api_key_env`); the key is read from it when a request is sent and is never
printed, logged, cached or sent to the browser. A key written in the config
(`api_key = …`) makes that entry invalid. Plain `http://` is refused except for
`localhost`.

**What is sent.** Headings, an excerpt per section (≤ `excerpt_chars`) and numeric
features — never the whole file.

**Cache.** Decisions are cached per section (memory, then disk), keyed by the
section's content hash, the model (api, endpoint, id, effort) and the candidate
list, so an unchanged section is never asked twice and a stopped run resumes
where it stopped. Only option ids and numbers are written to disk (file names are
SHA-256 digests) — no document text, heading or key. `[cache] enabled = false`
keeps decisions in memory only.

**Failures.** A model that cannot be used (bad entry, unset key, 401/402/404, CLI
missing) shows no views, and the UI notifies you with the reason and the config
file. Timeouts, 429/529 (retried once, honouring `retry-after`), 5xx, malformed
answers and no network end the run with an error you can retry.

```sh
mdpeek select README.md                     # the default model ([model] use)
mdpeek select README.md --model gemini      # another entry
mdpeek serve README.md --model haiku        # use another model for this session
mdpeek select-eval tests/fixtures/select --model jev --model haiku   # accuracy / none / p50-p95
```

`mdpeek select` keeps decisions in memory only, so every run asks the model.

# Status
## Viewer
- [x] Live in-place updates with changed-block highlighting
- [x] File explorer sidebar for the current branch's working tree
- [x] Breadcrumb showing the repository and branch
- [x] Outline panel with fuzzy heading search
- [x] Front matter panel
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
