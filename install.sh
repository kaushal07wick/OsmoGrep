#!/bin/sh
set -e

REPO="kaushal07wick/OsmoGrep"
VERSION="v0.1.0"
BIN="osmogrep-linux-x86_64"
INSTALL_DIR="/usr/local/bin"

echo "Installing osmogrep..."

tmp="$(mktemp)"

curl -fsSL \
  "https://github.com/$REPO/releases/download/$VERSION/$BIN" \
  -o "$tmp"

chmod +x "$tmp"

if [ -w "$INSTALL_DIR" ]; then
  mv "$tmp" "$INSTALL_DIR/osmogrep"
else
  sudo mv "$tmp" "$INSTALL_DIR/osmogrep"
fi

echo "osmogrep installed successfully"
echo "Run: osmogrep"
