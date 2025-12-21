#!/usr/bin/env bash
set -euo pipefail

PID_FILE=".agents.pid"
LOG_FILE=".agents.log"

start() {
  if [ -f "$PID_FILE" ] && kill -0 "$(cat "$PID_FILE")" 2>/dev/null; then
    echo "agents watcher already running"
    exit 0
  fi

  nohup bash -c '
    ./file.sh
    while true; do
      inotifywait -r -e modify,create,delete,move src >/dev/null 2>&1
      ./file.sh
    done
  ' > "$LOG_FILE" 2>&1 &

  echo $! > "$PID_FILE"
  echo "agents watcher started"
}

stop() {
  if [ -f "$PID_FILE" ]; then
    kill "$(cat "$PID_FILE")" 2>/dev/null || true
    rm -f "$PID_FILE"
    echo "agents watcher stopped"
  else
    echo "agents watcher not running"
  fi
}

status() {
  if [ -f "$PID_FILE" ] && kill -0 "$(cat "$PID_FILE")" 2>/dev/null; then
    echo "agents watcher running (PID $(cat "$PID_FILE"))"
  else
    echo "agents watcher not running"
  fi
}

case "${1:-start}" in
  start) start ;;
  stop) stop ;;
  status) status ;;
  *) echo "usage: $0 [start|stop|status]" ;;
esac
