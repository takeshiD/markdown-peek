-- Configuration for the mdpeek Neovim plugin (Layer 5).
--
-- All values can be overridden through `require("mdpeek").setup({ ... })`.
local M = {}

---@class MdpeekConfig
---@field bin string            Path/name of the mdpeek binary.
---@field host string           Host the preview server binds to.
---@field port integer          Port the preview server binds to.
---@field auto_open boolean      Open the browser automatically on :MdPeek.
---@field theme string|nil      Terminal theme passed to `mdpeek term --theme`.
---@field term_split string     "split" | "vsplit" for :MdPeekTerm.
M.defaults = {
  bin = "mdpeek",
  host = "127.0.0.1",
  port = 3030,
  auto_open = true,
  theme = nil,
  term_split = "vsplit",
}

---@type MdpeekConfig
M.options = vim.deepcopy(M.defaults)

---Merge user options over the defaults.
---@param opts table|nil
function M.setup(opts)
  M.options = vim.tbl_deep_extend("force", vim.deepcopy(M.defaults), opts or {})
  return M.options
end

---URL of the running preview server.
function M.url()
  return string.format("http://%s:%d", M.options.host, M.options.port)
end

return M
