-- mdpeek.nvim — drive the `mdpeek` binary from Neovim (Layer 5: editor 統合).
--
-- Commands (registered in plugin/mdpeek.lua):
--   :MdPeek       start the live-reload browser preview for the current file
--   :MdPeekStop   stop the preview server
--   :MdPeekTerm   render the current file in a terminal split (`mdpeek term`)
--   :MdPeekStatus show whether a server is running and its URL
--
-- The plugin never re-implements rendering: it shells out to `mdpeek`, which is
-- the single source of truth (AGENTS.md §1.1). `serve` watches the file itself,
-- so saving the buffer live-reloads the browser with no extra wiring here.
local config = require("mdpeek.config")

local M = {}

-- Job id of the running `mdpeek serve` process, or nil when stopped.
---@type integer|nil
M._job = nil

local function notify(msg, level)
  vim.notify("[mdpeek] " .. msg, level or vim.log.levels.INFO)
end

---Absolute path of the current buffer, or nil if the buffer has no file.
local function current_file()
  local name = vim.api.nvim_buf_get_name(0)
  if name == nil or name == "" then
    return nil
  end
  return name
end

---Is the mdpeek binary reachable?
local function has_bin()
  return vim.fn.executable(config.options.bin) == 1
end

---Open a URL in the system browser (nvim 0.10 `vim.ui.open`, else fallbacks).
local function open_url(url)
  if vim.ui and type(vim.ui.open) == "function" then
    vim.ui.open(url)
    return
  end
  local opener
  if vim.fn.has("mac") == 1 then
    opener = "open"
  elseif vim.fn.has("win32") == 1 then
    opener = "start"
  else
    opener = "xdg-open"
  end
  vim.fn.jobstart({ opener, url }, { detach = true })
end

---Start the live-reload preview server for the current file.
function M.start()
  if not has_bin() then
    notify("binary '" .. config.options.bin .. "' not found on PATH", vim.log.levels.ERROR)
    return
  end
  local file = current_file()
  if not file then
    notify("current buffer has no file to preview", vim.log.levels.WARN)
    return
  end
  if M._job then
    notify("already running at " .. config.url() .. " (reusing)")
    if config.options.auto_open then
      open_url(config.url())
    end
    return
  end

  local cmd = {
    config.options.bin,
    "serve",
    file,
    "--host",
    config.options.host,
    "--port",
    tostring(config.options.port),
  }

  M._job = vim.fn.jobstart(cmd, {
    -- The server is long-lived; keep stderr for diagnostics only.
    on_exit = function(_, code, _)
      M._job = nil
      if code ~= 0 and code ~= 143 then -- 143 = SIGTERM from :MdPeekStop
        notify("server exited with code " .. code, vim.log.levels.WARN)
      end
    end,
  })

  if not M._job or M._job <= 0 then
    M._job = nil
    notify("failed to launch mdpeek serve", vim.log.levels.ERROR)
    return
  end

  notify("serving " .. vim.fn.fnamemodify(file, ":t") .. " at " .. config.url())
  if config.options.auto_open then
    -- Give the server a moment to bind before opening the browser.
    vim.defer_fn(function()
      open_url(config.url())
    end, 400)
  end
end

---Stop the running preview server, if any.
function M.stop()
  if not M._job then
    notify("no server running")
    return
  end
  vim.fn.jobstop(M._job)
  M._job = nil
  notify("server stopped")
end

---Render the current file in a terminal split via `mdpeek term`.
function M.term()
  if not has_bin() then
    notify("binary '" .. config.options.bin .. "' not found on PATH", vim.log.levels.ERROR)
    return
  end
  local file = current_file()
  if not file then
    notify("current buffer has no file to preview", vim.log.levels.WARN)
    return
  end

  local cmd = { config.options.bin, "term", file }
  if config.options.theme then
    table.insert(cmd, "--theme")
    table.insert(cmd, config.options.theme)
  end

  local split = config.options.term_split == "split" and "split" or "vsplit"
  vim.cmd(split)
  vim.fn.termopen(cmd)
  vim.cmd("startinsert")
end

---Report server status.
function M.status()
  if M._job then
    notify("running at " .. config.url())
  else
    notify("stopped")
  end
end

---Entry point for user configuration.
---@param opts table|nil
function M.setup(opts)
  config.setup(opts)
end

return M
