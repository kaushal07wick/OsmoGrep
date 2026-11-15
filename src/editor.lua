vim.g.mapleader = ' '
vim.g.netrw_banner = 0
vim.g.netrw_liststyle = 3
vim.g.netrw_browse_split = 0
vim.opt.termguicolors = true
vim.opt.background = 'dark'
vim.opt.number = true
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
