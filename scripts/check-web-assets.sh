#!/bin/sh
# Static checks for the web UI (R11): old viewer files are gone, design tokens
# live only in web/src/app.css, shadcn/ui + lucide are really used, and the
# built bundle loads nothing from the network.
set -u
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
fail=0
ok() { echo "ok   $*"; }
ng() { echo "FAIL $*" >&2; fail=1; }

for f in static/js/main.js static/css/github-light.css static/css/github-dark.css web/src/styles.css web/src/panel.css web/src/panel.tsx web/src/scrolly.css; do
  [ -e "$f" ] && ng "old file still present: $f" || ok "removed: $f"
done

# Colour tokens: defined once (app.css); no raw hex colours in components.
n=$(grep -rlE -- '--background:|--primary:|--foreground:' web/src | wc -l)
[ "$n" -eq 1 ] && ok "tokens defined in exactly one file: $(grep -rlE -- '--background:' web/src)" || ng "tokens defined in $n files"
hex=$(grep -rnE '#[0-9a-fA-F]{6}\b|#[0-9a-fA-F]{3}\b' web/src --include=*.tsx --include=*.ts | grep -v '^\s*//' || true)
[ -z "$hex" ] && ok "no raw hex colours in .ts/.tsx" || ng "raw hex colours in components: $hex"

# shadcn/ui source components exist and are imported by the app.
for c in button dialog sheet command popover tooltip dropdown-menu toggle-group tabs resizable input kbd; do
  [ -f "web/src/components/ui/$c.tsx" ] || { ng "missing shadcn component $c"; continue; }
  grep -rq "@/components/ui/$c\"" web/src --include=*.tsx && ok "shadcn $c used" || ng "shadcn $c not imported"
done
grep -rq 'from "lucide-react"' web/src/components/Topbar.tsx && ok "lucide-react icons used" || ng "lucide-react unused"
grep -q '"tailwindcss"' web/src/app.css && grep -q '@tailwindcss/vite' web/vite.config.ts && ok "Tailwind v4 via @tailwindcss/vite" || ng "Tailwind v4 not wired"
grep -q '"react": "19' web/package.json && ok "React 19" || ng "React 19 missing"

# Built assets: present, embedded, and free of network loads.
for f in web/dist/app.js web/dist/app.css; do [ -s "$f" ] && ok "built $f" || ng "missing $f"; done
grep -q 'web/dist/app.js' crates/mdpeek-server/src/lib.rs && grep -q 'include_bytes!("../../../web/dist/app.css")' crates/mdpeek-server/src/lib.rs && ok "dist embedded with include_bytes!" || ng "dist not embedded"
urls=$(grep -oE "(https?:)?//[a-zA-Z0-9.-]+\.[a-z]{2,}[^\"' )]*" web/dist/app.js web/dist/app.css | grep -vE 'www\.w3\.org|react\.dev/errors' || true)
[ -z "$urls" ] && ok "no external URLs in web/dist (besides XML namespaces / React error links)" || ng "external URLs in bundle: $urls"
grep -qE 'url\((https?:)?//' web/dist/app.css && ng "remote url() in CSS" || ok "no remote url() in app.css"
ext=$(grep -oE '(src|href)="https?://[^"]+"' static/index.html | grep -v 'cdn.jsdelivr.net/npm/mathjax' || true)
[ -z "$ext" ] && ok "shell loads only embedded assets (MathJax CDN pre-existing, out of scope)" || ng "new external loads in shell: $ext"
exit $fail
