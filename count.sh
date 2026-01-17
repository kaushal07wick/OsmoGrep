#!/bin/sh

SRC_DIR=${1:-src}

if [ ! -d "$SRC_DIR" ]; then
  echo "Directory not found: $SRC_DIR"
  exit 1
fi

find "$SRC_DIR" -type f \
  ! -path "*/target/*" \
  ! -path "*/.git/*" \
  -print0 \
| xargs -0 wc -l
