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

main() {
  if install_prebuilt; then
    exit 0
  fi

  info "Prebuilt release not available for this platform."

  if install_with_cargo; then
    exit 0
  fi

  if install_from_source; then
    exit 0
  fi

  err "Could not install $BIN_NAME automatically."
  err "Install Rust and run: cargo install osmogrep"
  exit 1
}

main "$@"
