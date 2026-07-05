#!/usr/bin/env bash
#
# render.sh — render Markdown files to static HTML with mdpeek (Layer 5).
#
# `mdpeek serve` renders a file to HTML and serves it at `/`. This script spins
# the server up for each file, captures that HTML, and writes it to an output
# directory — turning the live previewer into a static export suitable for a
# GitHub PR preview artifact (or GitHub Pages). Rendering stays in mdpeek; this
# is only orchestration (AGENTS.md §1.1: don't re-implement the renderer).
#
# Usage:
#   render.sh <outdir> <file.md> [file2.md ...]
#
# Environment:
#   MDPEEK_BIN   mdpeek binary to use          (default: mdpeek)
#   MDPEEK_PORT  port to serve on              (default: 38080)
#   MDPEEK_HOST  host to bind/curl             (default: 127.0.0.1)
set -euo pipefail

MDPEEK="${MDPEEK_BIN:-mdpeek}"
PORT="${MDPEEK_PORT:-38080}"
HOST="${MDPEEK_HOST:-127.0.0.1}"

if [ "$#" -lt 2 ]; then
  echo "usage: render.sh <outdir> <file.md> [file.md ...]" >&2
  exit 2
fi

OUTDIR="$1"
shift
mkdir -p "$OUTDIR"

# The rendered HTML links assets by absolute path (`/static/...`), so for the
# preview to work when the artifact is served at a web root, copy the static
# asset dir alongside the pages. Set MDPEEK_STATIC_DIR=static to enable.
# (Absolute `/static/` paths mean the artifact must be served from the domain
# root, not a subpath — see editors/github/README.md.)
if [ -n "${MDPEEK_STATIC_DIR:-}" ] && [ -d "${MDPEEK_STATIC_DIR}" ]; then
  cp -r "${MDPEEK_STATIC_DIR}" "$OUTDIR/static"
  echo "assets: copied ${MDPEEK_STATIC_DIR} -> $OUTDIR/static"
fi

if ! command -v "$MDPEEK" >/dev/null 2>&1; then
  echo "error: mdpeek binary '$MDPEEK' not found (set MDPEEK_BIN)" >&2
  exit 1
fi

rendered=()

for f in "$@"; do
  if [ ! -f "$f" ]; then
    echo "skip: $f (not a file)" >&2
    continue
  fi

  # Serve the single file in the background.
  "$MDPEEK" serve "$f" --host "$HOST" --port "$PORT" >/dev/null 2>&1 &
  pid=$!

  # Wait for the server to bind (up to ~5s), then capture the rendered HTML.
  url="http://${HOST}:${PORT}/"
  ok=0
  for _ in $(seq 1 50); do
    if curl -sf -o /dev/null --max-time 1 "$url"; then
      ok=1
      break
    fi
    sleep 0.1
  done

  if [ "$ok" -eq 1 ]; then
    dest="$OUTDIR/${f%.md}.html"
    mkdir -p "$(dirname "$dest")"
    curl -s "$url" -o "$dest"
    echo "rendered: $f -> $dest"
    rendered+=("${f%.md}.html")
  else
    echo "error: server did not come up for $f" >&2
  fi

  # Stop the server before moving to the next file.
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
done

# Emit a small index linking every rendered page.
{
  echo "<!doctype html><meta charset=utf-8><title>mdpeek preview</title>"
  echo "<h1>mdpeek preview</h1><ul>"
  for rel in "${rendered[@]}"; do
    printf '<li><a href="%s">%s</a></li>\n' "$rel" "$rel"
  done
  echo "</ul>"
} >"$OUTDIR/index.html"

echo "index: $OUTDIR/index.html (${#rendered[@]} file(s))"
