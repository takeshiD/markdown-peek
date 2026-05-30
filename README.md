# Markdown Peek
Markdown Peek (`mdpeek`) is a lightweight CLI tool that watches a markdown file and renders it either in your browser (live preview) or directly in the terminal.

- [Features](#features)
- [Quick Start](#quick-start)
- [Installation](#installation)
- [Configuration](#configuration)
- [Status](#status)
- [License](#license)

# Features
- ⚡ Fast
- 🔄 Live Reload
- ⚙️ Easy Configure
- 🌐🖥️ Preview in Browser or in Terminal
- 📝 Supported Github Flavored Markdown(GFM), GitLab Flavored Markdown(GLFM)

# Quick start

## Preview on your browser
`mdpeek` detects `README.md` on default and previews markdown on browser.
```sh
mdpeek
 INFO mdpeek::watcher: Watching: "README.md"
 INFO mdpeek::server: Listening on http://127.0.0.1:3030
```

## Preview on your terminal
`mdpeek` detects `README.md` on default.
If you want preview on termianl, using `term` subcommand.
```sh
mdpeek term
```

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


# Status
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
See [LICENSE](LICENSE).

