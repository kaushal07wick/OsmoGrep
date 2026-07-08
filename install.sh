#!/usr/bin/env sh
set -eu

REPO="kaushal07wick/OsmoGrep"
BIN_NAME="osmogrep"

info() {
  printf '%s\n' "$*"
}

err() {
  printf 'error: %s\n' "$*" >&2
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1
}

has_sudo() {
  need_cmd sudo || [ "$(id -u)" -eq 0 ]
}

run_pkg() {
  if [ "$(id -u)" -eq 0 ]; then
    "$@"
  elif need_cmd sudo; then
    sudo "$@"
  else
    return 1
  fi
}

os() {
  case "$(uname -s)" in
    Linux) echo "unknown-linux-gnu" ;;
    Darwin) echo "apple-darwin" ;;
    *) err "unsupported OS: $(uname -s)"; exit 1 ;;
  esac
}

arch() {
  case "$(uname -m)" in
    x86_64|amd64) echo "x86_64" ;;
    arm64|aarch64) echo "aarch64" ;;
    *) err "unsupported architecture: $(uname -m)"; exit 1 ;;
  esac
}

install_dir() {
  if [ "$(id -u)" -eq 0 ]; then
    echo "/usr/local/bin"
  else
    echo "$HOME/.local/bin"
  fi
}

install_prebuilt() {
  target="$(arch)-$(os)"
  api="https://api.github.com/repos/$REPO/releases/latest"

  if ! need_cmd curl; then
    return 1
  fi

  download_url="$(
    curl -fsSL "$api" |
      sed -n "s/.*\"browser_download_url\": \"\([^\"]*${target}[^\"]*\.tar\.gz\)\".*/\1/p" |
      head -n 1
  )"

  if [ -z "$download_url" ]; then
    return 1
  fi

  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "$tmp_dir"' EXIT INT TERM

  info "Downloading prebuilt binary for $target..."
  curl -fsSL "$download_url" -o "$tmp_dir/osmogrep.tar.gz"
  tar -xzf "$tmp_dir/osmogrep.tar.gz" -C "$tmp_dir"

  bin_path="$(find "$tmp_dir" -type f -name "$BIN_NAME" | head -n 1)"
  if [ -z "$bin_path" ]; then
    err "downloaded release did not contain $BIN_NAME"
    return 1
  fi

  dst="$(install_dir)"
  mkdir -p "$dst"
  cp "$bin_path" "$dst/$BIN_NAME"
  chmod +x "$dst/$BIN_NAME"

  info "Installed $BIN_NAME to $dst/$BIN_NAME"
  if [ "$dst" = "$HOME/.local/bin" ]; then
    info "Add this to PATH if needed: export PATH=\"$HOME/.local/bin:\$PATH\""
  fi
  return 0
}

install_with_cargo() {
  if ! need_cmd cargo; then
    return 1
  fi
  info "Installing via cargo..."
  cargo install osmogrep
}

install_from_source() {
  if ! need_cmd cargo || ! need_cmd git; then
    return 1
  fi

  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "$tmp_dir"' EXIT INT TERM
  info "Building from source..."
  git clone "https://github.com/$REPO.git" "$tmp_dir/OsmoGrep"
  (cd "$tmp_dir/OsmoGrep" && cargo install --path .)
}

install_runtime_deps() {
  missing_tmux=0
  missing_nvim=0
  need_cmd tmux || missing_tmux=1
  need_cmd nvim || missing_nvim=1

  if [ "$missing_tmux" -eq 0 ] && [ "$missing_nvim" -eq 0 ]; then
    return 0
  fi

  info "Installing runtime dependencies for /nv (tmux + neovim)..."

  if [ "$(uname -s)" = "Darwin" ] && need_cmd brew; then
    brew install tmux neovim || true
    brew install --cask font-jetbrains-mono-nerd-font || true
    return 0
  fi

  if need_cmd apt-get; then
    run_pkg apt-get update || true
    run_pkg apt-get install -y tmux neovim || true
    return 0
  fi

  if need_cmd dnf; then
    run_pkg dnf install -y tmux neovim || true
    return 0
  fi

  if need_cmd pacman; then
    run_pkg pacman -Sy --noconfirm tmux neovim || true
    return 0
  fi

  info "Could not auto-install tmux/neovim (no supported package manager found)."
  info "Install manually to use /nv."
  return 0
}

setup_nvim_ux() {
  cfg_dir="${HOME}/.config/nvim"
  init_lua="${cfg_dir}/init.lua"
  marker="-- osmogrep-managed-nvim"

  mkdir -p "$cfg_dir"

  if [ -f "$init_lua" ] && ! grep -q "$marker" "$init_lua"; then
    info "Existing nvim config found at $init_lua (leaving unchanged)."
    return 0
  fi

  cat >"$init_lua" <<'EOF'
-- osmogrep-managed-nvim
vim.g.mapleader = ' '
vim.g.loaded_netrw = 1
vim.g.loaded_netrwPlugin = 1
vim.opt.termguicolors = true
vim.opt.number = true
vim.opt.relativenumber = true
vim.opt.signcolumn = 'yes'
vim.opt.cursorline = true
vim.opt.updatetime = 200
vim.opt.splitright = true
vim.opt.splitbelow = true
vim.opt.clipboard = 'unnamedplus'

local lazypath = vim.fn.stdpath('data') .. '/lazy/lazy.nvim'
local stat = vim.loop.fs_stat(lazypath)
if stat and stat.type ~= 'directory' then
  pcall(vim.fn.delete, lazypath)
  stat = nil
end
if not stat then
  vim.fn.system({
    'git', 'clone', '--filter=blob:none',
    'https://github.com/folke/lazy.nvim.git',
    '--branch=stable', lazypath
  })
end
vim.opt.rtp:prepend(lazypath)

require('lazy').setup({
  { 'catppuccin/nvim', name = 'catppuccin', priority = 1000 },
  { 'nvim-tree/nvim-web-devicons' },
  {
    'nvim-tree/nvim-tree.lua',
    config = function()
      require('nvim-tree').setup({
        hijack_netrw = true,
        sync_root_with_cwd = true,
        view = { width = 34 },
      })
    end
  },
  { 'nvim-treesitter/nvim-treesitter', build = ':TSUpdate' },
  { 'nvim-telescope/telescope.nvim', dependencies = { 'nvim-lua/plenary.nvim' } },
  { 'neovim/nvim-lspconfig' },
  { 'williamboman/mason.nvim' },
  { 'williamboman/mason-lspconfig.nvim', dependencies = { 'williamboman/mason.nvim', 'neovim/nvim-lspconfig' } },
})

vim.cmd.colorscheme('catppuccin-mocha')
pcall(function() require('nvim-tree').setup({ view = { width = 34 } }) end)
pcall(function()
  require('nvim-treesitter.configs').setup({
    highlight = { enable = true },
    indent = { enable = true },
    ensure_installed = { 'lua', 'vim', 'vimdoc', 'rust', 'python', 'javascript', 'typescript', 'go', 'json', 'toml', 'bash' },
  })
end)
pcall(function() require('mason').setup() end)
pcall(function()
  require('mason-lspconfig').setup({
    ensure_installed = { 'lua_ls', 'rust_analyzer', 'pyright', 'ts_ls', 'gopls', 'clangd' },
  })
end)

local lspconfig = require('lspconfig')
for _, server in ipairs({ 'lua_ls', 'rust_analyzer', 'pyright', 'ts_ls', 'gopls', 'clangd' }) do
  pcall(function() lspconfig[server].setup({}) end)
end

vim.keymap.set('n', '<leader>e', '<cmd>NvimTreeToggle<CR>', { silent = true })
vim.keymap.set('n', '<C-b>', '<cmd>NvimTreeToggle<CR>', { silent = true })
vim.keymap.set('n', '<leader>ff', '<cmd>Telescope find_files<CR>', { silent = true })
vim.api.nvim_create_autocmd('FileType', {
  pattern = 'lazy',
  callback = function()
    vim.keymap.set('n', 'q', '<cmd>close<CR>', { buffer = true, silent = true })
    vim.keymap.set('n', '<Esc>', '<cmd>close<CR>', { buffer = true, silent = true })
  end,
})
vim.api.nvim_create_autocmd('VimEnter', {
  callback = function()
    pcall(vim.cmd, 'NvimTreeOpen')
    pcall(vim.cmd, 'wincmd l')
  end,
})
EOF

  info "Installed managed nvim UX config at $init_lua"
}

main() {
  if install_prebuilt; then
    install_runtime_deps || true
    setup_nvim_ux || true
    exit 0
  fi

  info "Prebuilt release not available for this platform."

  if install_with_cargo; then
    install_runtime_deps || true
    setup_nvim_ux || true
    exit 0
  fi

  if install_from_source; then
    install_runtime_deps || true
    setup_nvim_ux || true
    exit 0
  fi

  err "Could not install $BIN_NAME automatically."
  err "Install Rust and run: cargo install osmogrep"
  exit 1
}

main "$@"
