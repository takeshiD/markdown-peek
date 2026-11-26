#!/bin/sh
# Rebuild the web UI from a clean copy (`npm ci && npm run build`) and check
# that the result is byte-identical to the committed web/dist (design 論点 C).
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/web"
( cd "$root/web" && tar cf - --exclude=./node_modules --exclude=./dist . ) | ( cd "$tmp/web" && tar xf - )
cd "$tmp/web"
npm ci --no-audit --no-fund --loglevel=error >/dev/null
npm run build --silent >/dev/null
if diff -r "$tmp/web/dist" "$root/web/dist"; then
  echo "web/dist is reproducible: $(ls "$tmp/web/dist" | tr '\n' ' ')"
else
  echo "web/dist differs from a clean npm ci && npm run build" >&2
  exit 1
fi
