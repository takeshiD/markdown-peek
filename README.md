# makrdown-peek
`markdown-peek` is a markdown previewer.
Using both browser rendering and render in terminal(like glow).

# Features
- Rendering GitHub/GitLab Flavored Markdown(and your theme)
- mermaid.js rendering in browser and terminal
- katext.js rendering in browser and terminal

# Installation
## cargo
```bash
cargo install markdown-peek --locked
```

## Nix
if you use flake, execute following.
```bash
nix run github::takeshid/markdown-peek
```

## Source Build
```bash
git clone https:/github.com/takeshid/markdown-peek.git
cd markdown-peek
cargo install --path .
```

# Usage
```bash
# Preview from README.md
mdpeek

# Preview from file
mdpeek PLAN.md

# Preview from stdin
echo "# Hello Markdown Peek\nVery nice command" | mdpeek -
```
