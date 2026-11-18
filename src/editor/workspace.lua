-- Native Neovim windows: one project tree, a live agent file, and optional inspection windows.
local api = vim.api
local root = vim.fn.getcwd()
local state = { expanded = { [root] = true }, rows = {}, live = nil, line = 1, phase = 'Ready', following = true }
local main_window, tree_window, tree_buffer, inspect_window
local M = {}
_G.osmogrep_workspace = M

local function valid(window) return window and api.nvim_win_is_valid(window) end
local function plain(text) return text:gsub('%%', '%%%%'):gsub('%c', '?') end
local function relative(path) return vim.fn.fnamemodify(path, ':~:.') end
local function document_window()
  if valid(inspect_window) then return inspect_window end
  return main_window
end
local function ensure_main()
  if valid(main_window) then return end
  for _, window in ipairs(api.nvim_tabpage_list_wins(0)) do
    if window ~= tree_window and api.nvim_win_get_config(window).relative == '' then
      main_window = window
      if inspect_window == window then inspect_window = nil end
      vim.wo[window].winbar = '%!v:lua.osmogrep_workspace.ticker()'
      return
    end
  end
end
local function document_buffers()
  local buffers = {}
  for _, buffer in ipairs(api.nvim_list_bufs()) do
    if api.nvim_buf_is_loaded(buffer) and vim.bo[buffer].buflisted and not vim.b[buffer].osmogrep_ui then
      table.insert(buffers, buffer)
    end
  end
  table.sort(buffers, function(a, b)
    if a == state.live then return b ~= state.live end
    if b == state.live then return false end
    return a < b
  end)
  return buffers
end

function M.tabs()
  local parts = {}
  for _, buffer in ipairs(document_buffers()) do
    local selected = buffer == api.nvim_get_current_buf()
    local name = vim.fn.fnamemodify(api.nvim_buf_get_name(buffer), ':t')
    if name == '' then name = 'New file' end
    table.insert(parts, (selected and '%#TabLineSel#' or '%#TabLine#') .. '%' .. buffer .. '@v:lua.osmogrep_workspace.tab_click@ '
      .. (buffer == state.live and '● ' or '') .. plain(name) .. (vim.bo[buffer].modified and ' +' or '') .. ' %T')
  end
  return table.concat(parts) .. '%#TabLineFill#%='
end

function M.ticker()
  local label = state.live and api.nvim_buf_is_valid(state.live) and (state.phase .. ' · ' .. relative(api.nvim_buf_get_name(state.live)) .. ':' .. state.line) or 'Waiting for agent edits'
  return '%#OsmoBrand#%@v:lua.osmogrep_workspace.jump_click@ ● %<' .. plain(label) .. ' %T%=%#Comment#%@v:lua.osmogrep_workspace.toggle_follow@'
    .. (state.blocked or (state.following and 'Follow ' or 'Paused ')) .. '%T%@v:lua.osmogrep_workspace.split@ Split %T'
end

function M.status()
  return { live = state.live, line = state.line, following = state.following, main = main_window,
    tree = valid(tree_window) and tree_window or nil, inspection = valid(inspect_window) and inspect_window or nil }
end

local function tree_entries(directory, depth, output)
  -- shortcut: cap expanded trees at 2,000 entries and 12 levels; add paging for larger expanded views.
  local scanner = vim.uv.fs_scandir(directory)
  if not scanner then return end
  local children = {}
  while #children < 2000 do
    local name, kind = vim.uv.fs_scandir_next(scanner)
    if not name then break end
    if name ~= '.git' then table.insert(children, { name = name, directory = kind == 'directory', path = directory .. '/' .. name, depth = depth }) end
  end
  table.sort(children, function(a, b)
    if a.directory ~= b.directory then return a.directory end
    return a.name:lower() < b.name:lower()
  end)
  for _, item in ipairs(children) do
    if #output >= 2000 then return end
    table.insert(output, item)
    if item.directory and state.expanded[item.path] and depth < 12 then tree_entries(item.path, depth + 1, output) end
  end
end

function M.refresh_tree()
  if not tree_buffer or not api.nvim_buf_is_valid(tree_buffer) then return end
  local cursor = valid(tree_window) and api.nvim_win_get_cursor(tree_window) or { 1, 0 }
  local entries = {}
  tree_entries(root, 0, entries)
  state.rows = entries
  local lines = {}
  for _, item in ipairs(entries) do
    local live = state.live and api.nvim_buf_get_name(state.live) == item.path
    local glyph = item.directory and (state.expanded[item.path] and '▾ ' or '▸ ') or (live and '● ' or '  ')
    table.insert(lines, ' ' .. string.rep('  ', item.depth) .. glyph .. item.name:gsub('%c', '?'))
  end
  if #lines == 0 then lines = { ' No files yet' } end
  if #entries >= 2000 then table.insert(lines, ' … tree limit; collapse folders') end
  vim.bo[tree_buffer].modifiable = true
  api.nvim_buf_set_lines(tree_buffer, 0, -1, false, lines)
  vim.bo[tree_buffer].modifiable = false
  local namespace = api.nvim_create_namespace('osmogrep-tree')
  api.nvim_buf_clear_namespace(tree_buffer, namespace, 0, -1)
  for index, item in ipairs(entries) do
    local live = state.live and api.nvim_buf_get_name(state.live) == item.path
    if live then api.nvim_buf_add_highlight(tree_buffer, namespace, 'OsmoBrand', index - 1, 0, -1) end
  end
  if valid(tree_window) then api.nvim_win_set_cursor(tree_window, { math.min(cursor[1], #lines), 0 }) end
end

function M.open_path(path)
  ensure_main()
  if not path or path == '' then return true end
  path = vim.fn.fnamemodify(path, ':p'):gsub('/$', '')
  if vim.fn.isdirectory(path) == 1 then
    state.expanded[path] = true
    M.refresh_tree()
    return true
  end
  local buffer = vim.fn.bufadd(path)
  vim.fn.bufload(buffer)
  vim.bo[buffer].buflisted = true
  if buffer == state.live then M.jump_click(); return true end
  state.following = valid(inspect_window) or false
  local window = document_window()
  if valid(window) then api.nvim_set_current_win(window); api.nvim_win_set_buf(window, buffer) end
  vim.cmd('redrawtabline')
  return true
end

function M.tab_click(buffer, _, button)
  if button == 'm' then
    if buffer ~= state.live then
      local ok, error = pcall(api.nvim_buf_delete, buffer, {})
      if not ok then vim.notify(tostring(error), vim.log.levels.WARN) end
    end
  elseif buffer == state.live then
    M.jump_click()
  else
    M.open_path(api.nvim_buf_get_name(buffer))
  end
end

function M.jump_click()
  ensure_main()
  state.following = true
  if state.live and api.nvim_buf_is_valid(state.live) and valid(main_window) then
    if vim.bo[state.live].modified or vim.bo[api.nvim_win_get_buf(main_window)].modified then
      state.blocked = 'Unsaved · paused '
      return
    end
    state.blocked = nil
    vim.cmd('silent! checktime ' .. state.live)
    api.nvim_set_current_win(main_window)
    api.nvim_win_set_buf(main_window, state.live)
    api.nvim_win_set_cursor(main_window, { math.min(state.line, api.nvim_buf_line_count(state.live)), 0 })
  end
end

function M.toggle_follow()
  if state.following then state.following = false else M.jump_click() end
  vim.cmd('redrawstatus')
end

function M.split()
  if valid(inspect_window) then
    if vim.bo[api.nvim_win_get_buf(inspect_window)].modified then
      vim.notify('Save or discard your inspection edits before closing this window.', vim.log.levels.WARN)
      return
    end
    api.nvim_win_close(inspect_window, false)
    inspect_window = nil
    return
  end
  if not valid(main_window) then return end
  local current = api.nvim_get_current_buf()
  if vim.b[current].osmogrep_ui then current = api.nvim_win_get_buf(main_window) end
  api.nvim_set_current_win(main_window)
  if state.live and api.nvim_buf_is_valid(state.live) and not vim.bo[api.nvim_win_get_buf(main_window)].modified then api.nvim_win_set_buf(main_window, state.live) end
  vim.cmd('belowright split')
  inspect_window = api.nvim_get_current_win()
  api.nvim_win_set_buf(inspect_window, current)
  vim.wo[inspect_window].winbar = '%#Comment# Inspect · %t%=%#OsmoBrand# YOUR VIEW '
  state.following = true
end

function M.agent_focus(event)
  ensure_main()
  local buffer = vim.fn.bufadd(event.path)
  vim.fn.bufload(buffer)
  vim.bo[buffer].buflisted = true
  state.live, state.line = buffer, math.max(1, math.floor(tonumber(event.line) or 1))
  state.phase = ({ reading = 'Reading', editing = 'Editing', edited = 'Saved', writing = 'Writing' })[event.phase] or 'Working'
  local parent = vim.fn.fnamemodify(event.path, ':h')
  while parent ~= root and parent:sub(1, #root + 1) == root .. '/' do
    state.expanded[parent] = true
    parent = vim.fn.fnamemodify(parent, ':h')
  end
  M.refresh_tree()
  vim.cmd('redrawtabline')
  state.blocked = nil
  if not state.following or not valid(main_window) then return nil end
  if vim.bo[buffer].modified or vim.bo[api.nvim_win_get_buf(main_window)].modified then state.blocked = 'Unsaved · paused '; return nil end
  if api.nvim_get_current_win() == main_window and vim.fn.mode() ~= 'n' then return nil end
  return main_window
end

function M.toggle_tree()
  if valid(tree_window) then api.nvim_win_close(tree_window, false); tree_window = nil; return end
  if not valid(main_window) then return end
  local previous = api.nvim_get_current_win()
  api.nvim_set_current_win(main_window)
  vim.cmd('topleft vsplit')
  tree_window = api.nvim_get_current_win()
  tree_buffer = tree_buffer and api.nvim_buf_is_valid(tree_buffer) and tree_buffer or api.nvim_create_buf(false, true)
  vim.b[tree_buffer].osmogrep_ui = true
  api.nvim_win_set_buf(tree_window, tree_buffer)
  api.nvim_win_set_width(tree_window, math.max(14, math.min(23, math.floor(vim.o.columns * .24))))
  vim.wo[tree_window].winfixwidth = true
  vim.wo[tree_window].number = false
  vim.wo[tree_window].relativenumber = false
  vim.wo[tree_window].wrap = false
  vim.wo[tree_window].winbar = '%#Comment# PROJECT FILES '
  vim.bo[tree_buffer].bufhidden = 'hide'
  local function select()
    local item = state.rows[api.nvim_win_get_cursor(tree_window)[1]]
    if not item then return end
    if item.directory then state.expanded[item.path] = not state.expanded[item.path]; M.refresh_tree() else M.open_path(item.path) end
  end
  vim.keymap.set('n', '<CR>', select, { buffer = tree_buffer })
  vim.keymap.set('n', '<2-LeftMouse>', select, { buffer = tree_buffer })
  vim.keymap.set('n', '<LeftRelease>', select, { buffer = tree_buffer })
  M.refresh_tree()
  if valid(previous) then api.nvim_set_current_win(previous) else api.nvim_set_current_win(main_window) end
end

vim.opt.mouse = 'a'
vim.opt.showtabline = 2
vim.opt.tabline = '%!v:lua.osmogrep_workspace.tabs()'
api.nvim_set_hl(0, 'TabLine', { fg = '#928c80', bg = '#080808' })
api.nvim_set_hl(0, 'TabLineSel', { fg = '#eb9b69', bg = '#211a12', underline = true })
api.nvim_set_hl(0, 'TabLineFill', { bg = '#080808' })
vim.keymap.set('n', '<leader>e', M.toggle_tree, { desc = 'Toggle project tree' })
vim.keymap.set('n', '<leader>f', M.toggle_follow, { desc = 'Follow the agent' })
vim.keymap.set('n', '<leader>g', M.jump_click, { desc = 'Jump to live edit' })
vim.keymap.set('n', '<leader>s', M.split, { desc = 'Split live and inspected files' })
api.nvim_create_autocmd('VimResized', { callback = function()
  if valid(tree_window) then api.nvim_win_set_width(tree_window, math.max(14, math.min(23, math.floor(vim.o.columns * .24)))) end
end })
api.nvim_create_autocmd('VimEnter', { once = true, callback = function()
  main_window = api.nvim_get_current_win()
  if api.nvim_buf_get_name(0) == '' then
    vim.b.osmogrep_ui = true
    api.nvim_buf_set_lines(0, 0, -1, false, { '', '  Your code appears here.', '', '  The agent’s active file opens automatically.', '  Choose a file in the tree to inspect it.', '', '  Space a  chat     Space s  split view', '  Space e  files    Space g  live edit' })
    vim.bo.buftype, vim.bo.bufhidden, vim.bo.swapfile, vim.bo.modified = 'nofile', 'hide', false, false
  end
  vim.wo.winbar = '%!v:lua.osmogrep_workspace.ticker()'
  M.toggle_tree()
end })
api.nvim_create_autocmd('WinClosed', { callback = function()
  vim.schedule(function()
    local windows = api.nvim_tabpage_list_wins(0)
    if #windows == 1 and windows[1] == tree_window then vim.cmd('confirm qall') end
  end)
end })

-- Capture exactly what the user selected, including unsaved buffer text.
vim.keymap.set('x', '<leader>a', function()
  local first, last = vim.fn.getpos('v'), vim.fn.getpos('.')
  local text = table.concat(vim.fn.getregion(first, last, { type = vim.fn.mode() }), '\n')
  if #text == 0 or #text > 256 * 1024 then vim.notify('Select between 1 byte and 256 KiB.', vim.log.levels.WARN); return end
  local destination = vim.env.OSMOGREP_EDITOR_BRIDGE
  if not destination then return end
  destination = destination:gsub('%.json$', '.selection.json')
  local temporary = destination .. '.' .. vim.fn.getpid()
  local data = vim.json.encode({ path = api.nvim_buf_get_name(0), start = math.min(first[2], last[2]), ['end'] = math.max(first[2], last[2]), text = text })
  local ok = vim.fn.writefile({data}, temporary)
  if ok ~= 0 then vim.notify('Could not attach selection.', vim.log.levels.ERROR); return end
  vim.uv.fs_chmod(temporary, 384)
  local renamed, err = vim.uv.fs_rename(temporary, destination)
  if not renamed then vim.notify(tostring(err), vim.log.levels.ERROR); return end
  api.nvim_feedkeys(api.nvim_replace_termcodes('<Esc>', true, false, true), 'n', false)
  vim.schedule(osmogrep_focus_chat)
end, { desc = 'Attach selection to OsmoGrep chat' })
