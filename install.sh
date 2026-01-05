#!/bin/sh
set -e

REPO="kaushal07wick/osmogrep"
VERSION="v0.1.0"
BIN="osmogrep-linux-x86_64"
INSTALL_DIR="/usr/local/bin"

echo "Installing osmogrep..."

curl -fsSL \
  "https://github.com/$REPO/releases/download/$VERSION/$BIN" \
  -o osmogrep

chmod +x osmogrep

if [ -w "$INSTALL_DIR" ]; then
  mv osmogrep "$INSTALL_DIR/osmogrep"
else
  sudo mv osmogrep "$INSTALL_DIR/osmogrep"
fi

echo "osmogrep installed successfully"
echo "Run: osmogrep"
