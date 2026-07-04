# mdpeek.nvim

Drive the [`mdpeek`](../../README.md) Markdown previewer from Neovim (Layer 5).
The plugin never renders Markdown itself — it shells out to the `mdpeek` binary,
which is the single source of truth for rendering.

Requires Neovim ≥ 0.7 and the `mdpeek` binary on `PATH`.

## Install

**lazy.nvim**

```lua
{
  "takeshid/markdown-peek",
  -- the plugin lives in the editors/nvim subdirectory of the repo:
  dir = vim.fn.stdpath("data") .. "/lazy/markdown-peek/editors/nvim",
  ft = { "markdown" },
  config = function()
    require("mdpeek").setup({
      -- all optional; defaults shown
      bin = "mdpeek",
      host = "127.0.0.1",
      port = 3030,
      auto_open = true,     -- open the browser on :MdPeek
      theme = nil,          -- e.g. "nord" for :MdPeekTerm (mdpeek term --theme)
      term_split = "vsplit", -- or "split"
    })
  end,
}
```

Or just copy `editors/nvim/` onto your `runtimepath`. Because the plugin is a
subdirectory of the repo, most plugin managers need to be pointed at
`editors/nvim` specifically (as above) rather than the repo root.

## Commands

| Command | Action |
|---|---|
| `:MdPeek` | Start the live-reload **browser** preview for the current file (`mdpeek serve`) and open it. Saving the buffer live-reloads. |
| `:MdPeekStop` | Stop the preview server. |
| `:MdPeekTerm` | Render the current file in a **terminal split** (`mdpeek term`). |
| `:MdPeekStatus` | Show whether a server is running and its URL. |

The server is automatically stopped when you quit Neovim (`VimLeavePre`), so no
orphan process is left behind.

## Notes

- `mdpeek serve` watches the file on disk, so the plugin does not need to push
  updates on every keystroke — a `:w` is enough to refresh the browser.
- The browser is opened with `vim.ui.open` on Neovim ≥ 0.10, falling back to
  `xdg-open` / `open` / `start`.
