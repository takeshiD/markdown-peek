-- Command registration for mdpeek.nvim (Layer 5).
-- Loaded automatically by Neovim from any `plugin/` directory on the runtimepath.
if vim.g.loaded_mdpeek then
  return
end
vim.g.loaded_mdpeek = true

-- Lazily require the module so startup cost is a single `require`.
local function mdpeek()
  return require("mdpeek")
end

vim.api.nvim_create_user_command("MdPeek", function()
  mdpeek().start()
end, { desc = "Start mdpeek live browser preview for the current file" })

vim.api.nvim_create_user_command("MdPeekStop", function()
  mdpeek().stop()
end, { desc = "Stop the mdpeek preview server" })

vim.api.nvim_create_user_command("MdPeekTerm", function()
  mdpeek().term()
end, { desc = "Render the current file with `mdpeek term` in a split" })

vim.api.nvim_create_user_command("MdPeekStatus", function()
  mdpeek().status()
end, { desc = "Show mdpeek preview server status" })

-- Stop the server when Neovim exits so no orphan process is left behind.
vim.api.nvim_create_autocmd("VimLeavePre", {
  group = vim.api.nvim_create_augroup("mdpeek_cleanup", { clear = true }),
  callback = function()
    pcall(function()
      mdpeek().stop()
    end)
  end,
})
