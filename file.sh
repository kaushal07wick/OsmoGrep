#!/usr/bin/env bash
set -euo pipefail

ROOT="src"
OUT="call_graph.md"

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
    echo "**Calls**"

    case "$file" in
      *.rs)
        grep -oE "[A-Za-z_][A-Za-z0-9_]*::[A-Za-z_][A-Za-z0-9_]*\s*\(" "$file" \
          | sed 's/[[:space:]]*(//' \
          | sort -u \
          | sed 's/^/- /' || true
        ;;
      *.py)
        grep -oE "[A-Za-z_][A-Za-z0-9_]*\.[A-Za-z_][A-Za-z0-9_]*\s*\(" "$file" \
          | sed 's/[[:space:]]*(//' \
          | sort -u \
          | sed 's/^/- /' || true
        ;;
    esac

    echo ""
  done

  # ==========================================================
  # Global Hotspots (ONE TIME, AFTER FILE LOOP)
  # ==========================================================

  echo "## Global Hotspots"
  echo ""

  echo "### Thread creation"
  grep -R "thread::spawn" "$ROOT" 2>/dev/null \
    | awk -F: '{print "- " $1 " → thread::spawn"}' \
    | sort -u
  echo ""

  echo "### Process execution"
  grep -R "Command::new" "$ROOT" 2>/dev/null \
    | awk -F: '{print "- " $1 " → Command::new"}' \
    | sort -u
  echo ""

  echo "### AgentEvent fan-out"
  grep -R "AgentEvent::" "$ROOT" 2>/dev/null \
    | awk -F: '{print "- " $1}' \
    | sort -u
  echo ""

} > "$OUT"
