vim.g.mapleader = ' '
vim.g.netrw_banner = 0
vim.g.netrw_liststyle = 3
vim.g.netrw_browse_split = 0
vim.opt.termguicolors = true
vim.opt.background = 'dark'
vim.opt.number = true
vim.opt.autoread = true
vim.opt.hidden = true
vim.opt.cursorline = true
vim.opt.showmode = false
vim.opt.laststatus = 2
vim.opt.shortmess:append('I')
vim.opt.fillchars:append({ eob = ' ' })
vim.cmd('colorscheme default')
local colors = {
  Normal = { fg = '#ddd9d0', bg = '#000000' },
  NormalNC = { fg = '#ccc8bf', bg = '#000000' },
  NormalFloat = { fg = '#ddd9d0', bg = '#101010' },
  EndOfBuffer = { fg = '#000000', bg = '#000000' },
  LineNr = { fg = '#625f59', bg = '#000000' },
  CursorLineNr = { fg = '#eb9b69', bg = '#000000' },
  CursorLine = { bg = '#0b0b0b' },
  SignColumn = { bg = '#000000' },
  StatusLine = { fg = '#aaa59b', bg = '#171614' },
  StatusLineNC = { fg = '#76716a', bg = '#111110' },
  OsmoBrand = { fg = '#eb9b69', bg = '#171614', bold = true },
  OsmoAgentLine = { bg = '#21180f' },
  OsmoAdded = { bg = '#082317', fg = '#78bd91' },
  OsmoRemoved = { bg = '#2b1116', fg = '#e999a5' },
  VertSplit = { fg = '#292724', bg = '#000000' },
  WinSeparator = { fg = '#292724', bg = '#000000' },
  Directory = { fg = '#eb9b69' },
  Comment = { fg = '#77756e' },
  Visual = { bg = '#38322a' },
}
for group, color in pairs(colors) do vim.api.nvim_set_hl(0, group, color) end
vim.opt.statusline = '%#OsmoBrand# OsmoGrep %#StatusLine# %<%f %m%= space a: chat  %l:%c '
vim.keymap.set('n', '<leader>e', '<cmd>Explore<CR>', { desc = 'Browse project files' })
vim.keymap.set('n', '<leader>a', function()
  local pane = vim.env.OSMOGREP_AGENT_PANE
  if not pane then return end
  local window = vim.fn.system({ 'tmux', 'display-message', '-p', '-t', pane, '#{window_id}' }):gsub('%s+$', '')
  if vim.v.shell_error ~= 0 then return end
  vim.fn.system({ 'tmux', 'select-window', '-t', window })
  vim.fn.system({ 'tmux', 'select-pane', '-t', pane })
end, { desc = 'Return to OsmoGrep chat' })

local bridge = vim.env.OSMOGREP_EDITOR_BRIDGE
local namespace = vim.api.nvim_create_namespace('osmogrep-agent')
local following, last_sequence, agent_status = true, nil, 'Follow on'
local previous_path, previous_text
local watched = {}

function _G.osmogrep_editor_status()
  return agent_status
end
vim.opt.statusline = '%#OsmoBrand# OsmoGrep %#StatusLine# %<%f %m%= %{v:lua.osmogrep_editor_status()}  space a: chat  %l:%c '
vim.keymap.set('n', '<leader>f', function()
  following = not following
  agent_status = following and 'Follow on' or 'Follow paused'
  last_sequence = nil
end, { desc = 'Toggle following agent edits' })

local function show_changes(buffer, before)
  local after = table.concat(vim.api.nvim_buf_get_lines(buffer, 0, -1, false), '\n') .. '\n'
  if before == after then return end
  local old_lines = vim.split(before, '\n', { plain = true })
  for _, change in ipairs(vim.diff(before, after, { result_type = 'indices' })) do
    local old_start, old_count, new_start, new_count = unpack(change)
    local row = math.max(0, new_start - 1)
    for offset = 0, math.min(new_count, 200) - 1 do
      if row + offset < vim.api.nvim_buf_line_count(buffer) then
        vim.api.nvim_buf_set_extmark(buffer, namespace, row + offset, 0, { line_hl_group = 'OsmoAdded' })
      end
    end
    if old_count > 0 then
      local removed = {}
      for offset = 0, math.min(old_count, 3) - 1 do
        table.insert(removed, { { '- ' .. (old_lines[old_start + offset] or ''), 'OsmoRemoved' } })
      end
      if old_count > 3 then table.insert(removed, { { '… ' .. (old_count - 3) .. ' removed lines', 'OsmoRemoved' } }) end
      vim.api.nvim_buf_set_extmark(buffer, namespace, math.min(row, vim.api.nvim_buf_line_count(buffer) - 1), 0,
        { virt_lines = removed, virt_lines_above = true })
    end
  end
end

local function follow_agent()
  if not following or not bridge or vim.fn.mode() ~= 'n' then return end
  if vim.bo.modified then
    agent_status = 'Follow paused · unsaved edits'
    return
  end
  local buffer = vim.api.nvim_get_current_buf()
  local name = vim.api.nvim_buf_get_name(buffer)
  local stat = vim.uv.fs_stat(name)
  local stamp = stat and (tostring(stat.mtime.sec) .. ':' .. tostring(stat.mtime.nsec) .. ':' .. stat.size)
  if vim.bo.buftype == '' and stamp ~= watched[name] then
    watched[name] = stamp
    local before = table.concat(vim.api.nvim_buf_get_lines(buffer, 0, -1, false), '\n') .. '\n'
    vim.cmd('silent! checktime ' .. buffer)
    show_changes(buffer, before)
  end
  local file = io.open(bridge, 'r')
  if not file then return end
  local raw = file:read(8192)
  file:close()
  local ok, event = pcall(vim.json.decode, raw)
  if not ok or type(event) ~= 'table' or event.sequence == last_sequence or type(event.path) ~= 'string' then return end
  last_sequence = event.sequence
  local target = vim.fn.bufadd(event.path)
  if vim.bo[target].modified then
    agent_status = 'Follow paused · unsaved target'
    last_sequence = nil
    return
  end
  vim.fn.bufload(target)
  local before = (previous_path == event.path and previous_text) or (table.concat(vim.api.nvim_buf_get_lines(target, 0, -1, false), '\n') .. '\n')
  vim.cmd('silent! checktime ' .. target)
  vim.api.nvim_set_current_buf(target)
  vim.api.nvim_buf_clear_namespace(target, namespace, 0, -1)
  local row = math.min(math.max(tonumber(event.line) or 1, 1), vim.api.nvim_buf_line_count(target))
  vim.api.nvim_win_set_cursor(0, { row, 0 })
  vim.api.nvim_buf_set_extmark(target, namespace, row - 1, 0, { line_hl_group = 'OsmoAgentLine' })
  show_changes(target, before)
  previous_path = event.path
  previous_text = table.concat(vim.api.nvim_buf_get_lines(target, 0, -1, false), '\n') .. '\n'
  agent_status = 'Agent ' .. (event.phase or 'working') .. ' · space f: pause'
end

if bridge then
  local timer = vim.uv.new_timer()
  timer:start(0, 250, vim.schedule_wrap(function()
    local ok, err = pcall(follow_agent)
    if not ok then agent_status = 'Follow error: ' .. tostring(err):sub(1, 80) end
  end))
  vim.api.nvim_create_autocmd('VimLeavePre', { callback = function() timer:stop(); timer:close() end })
end
