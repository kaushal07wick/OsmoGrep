#!/usr/bin/env bash
set -euo pipefail

ROOT="src"
OUT="agents.md"

{
  echo "# Osmogrep Agent Map"
  echo ""
  echo "> Auto-generated. Do not edit manually."
  echo ""
  echo "## Files"
  echo ""

  find "$ROOT" -type f \( -name "*.rs" -o -name "*.py" \) | sort | while read -r file; do
    echo "### $file"
    echo ""
    echo "**Functions**"

    case "$file" in
      *.rs)
        awk '
          BEGIN { in_sig=0; sig="" }
          /^[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?(async[[:space:]]+)?(unsafe[[:space:]]+)?(const[[:space:]]+)?fn[[:space:]]+[A-Za-z0-9_]+/ {
            in_sig=1
            sig=$0
            next
          }
          in_sig {
            sig = sig " " $0
            if ($0 ~ /\{/) {
              gsub(/[[:space:]]*\{.*/, "", sig)
              gsub(/[[:space:]]+/, " ", sig)
              print "- " sig
              sig=""
              in_sig=0
            }
          }
        ' "$file"
        ;;
      *.py)
        awk '
          /^[[:space:]]*(async[[:space:]]+)?def[[:space:]]+[A-Za-z0-9_]+[[:space:]]*\(/ {
            sig=$0
            gsub(/[[:space:]]*:.*/, "", sig)
            gsub(/[[:space:]]+/, " ", sig)
            print "- " sig
          }
        ' "$file"
        ;;
    esac

    echo ""
  done
} > "$OUT"
