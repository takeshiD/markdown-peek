# GitHub preview 連携

Render a pull request's Markdown to a static HTML **preview** in CI (Layer 5).
`mdpeek serve` renders a file to HTML; this integration captures that HTML for
each file and publishes it as a PR artifact — turning the live previewer into a
static export. Rendering stays in `mdpeek` (AGENTS.md §1.1); everything here is
orchestration.

## Contents

| File | Purpose |
|---|---|
| `render.sh` | Renders a list of Markdown files to `<outdir>/*.html` + `index.html`. Locally runnable. |
| `action.yml` | A composite GitHub Action wrapping `render.sh`. |
| `workflows/md-preview.yml` | An example PR workflow. **Copy it to `.github/workflows/` to enable.** |

## Local use

```sh
cargo build --release --bin mdpeek
MDPEEK_BIN=target/release/mdpeek MDPEEK_STATIC_DIR=static \
  bash editors/github/render.sh out README.md SAMPLE.md

# serve the export at a web root and open http://localhost:8000
cd out && python3 -m http.server 8000     # or: npx serve .
```

Environment variables: `MDPEEK_BIN` (default `mdpeek`), `MDPEEK_PORT`
(default `38080`), `MDPEEK_HOST` (default `127.0.0.1`), `MDPEEK_STATIC_DIR`
(copy an asset dir next to the pages; unset to skip).

## CI use

Copy `workflows/md-preview.yml` to `.github/workflows/`. On every PR that
touches `*.md` it builds `mdpeek`, renders the **changed** files with the
composite action, uploads a `mdpeek-preview` artifact, and posts/updates a
sticky PR comment listing the previewed files with a link to download.

Using the composite action from another repo's workflow:

```yaml
- uses: takeshid/markdown-peek/editors/github@main
  with:
    files: "docs/guide.md README.md"
    mdpeek-bin: mdpeek        # must be installed on the runner
    static-dir: static
    output-dir: _mdpeek_preview
```

## Limitation: absolute asset paths

The rendered HTML links assets by **absolute** path (`/static/...`). The preview
therefore works when served from a web **root** (the local `http.server` above,
or GitHub Pages on a custom domain at `/`). Served from a sub-path it would need
the assets rehosted at `/static/` or the HTML rewritten to relative paths — a
reasonable follow-up if we later add a first-class `mdpeek export` subcommand
that emits relocatable HTML.
