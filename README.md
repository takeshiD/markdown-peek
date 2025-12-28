# Markdown Peek
Markdown Peek (`mdpeek`) is a lightweight CLI tool that watches a markdown file and renders it either in your browser (live preview) or directly in the terminal.

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
 INFO mdpeek::server: Listening on http://127.0.0.1:3000NFO mdpeek::watcher Watching: "README.md"
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


# Flavor Status
## GFM
- [x] [Table](https://github.github.com/gfm/#tables-extension-)
- [x] [TaskList](https://github.github.com/gfm/#task-list-items-extension-)
- [ ] [Strike throough](https://github.github.com/gfm/#strikethrough-extension-)
- [x] [Fenced Code](https://github.github.com/gfm/#fenced-code-blocks)
- [ ] Syntax Hightlight
- [ ] [Emoji](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#using-emojis)
- [ ] [Alert](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#alerts)
- [ ] MathJax
- [ ] [Color Model](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#supported-color-models)
- [ ] Auto Link
- [ ] InPage Link
- [ ] Footnote
- [ ] Table of Contents
- [ ] Theme Switch(Light/Dark)

## GLFM(Planed)
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
- [ ] [Table](https://github.github.com/gfm/#tables-extension-)
- [ ] [TaskList](https://github.github.com/gfm/#task-list-items-extension-)
- [ ] [Strike throough](https://github.github.com/gfm/#strikethrough-extension-)
- [ ] [Fenced Code](https://github.github.com/gfm/#fenced-code-blocks)
- [ ] Syntax Hightlight
- [ ] [Emoji](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#using-emojis)
- [ ] [Alert](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#alerts)
- [ ] MathJax
- [ ] [Color Model](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#supported-color-models)
- [ ] Auto Link
- [ ] InPage Link
- [ ] Footnote
- [ ] Table of Contents

# License
MIT License.  
See [LICENSE](LICENSE).
