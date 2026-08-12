#!/bin/bash
# Reference capture harness for the Tiller Swift app.
# Usage: capture.sh <name>            -> screenshot window into shots/<name>.png
#        capture.sh --click X Y       -> click at window-relative coords
#        capture.sh --key <keys>      -> send keystroke via cliclick
set -euo pipefail

SHOTS="$(cd "$(dirname "$0")" && pwd)/shots"
mkdir -p "$SHOTS"

win() {
  osascript -e 'tell application "System Events" to tell process "Tiller" to get {position, size} of window 1' \
    | tr -d ' ' | tr ',' ' '
}

case "${1:-}" in
  --click)
    read -r X Y W H <<<"$(win)"
    cliclick "m:$((X + $2)),$((Y + $3))" w:120 "c:$((X + $2)),$((Y + $3))"
    sleep "${4:-1}"
    ;;
  --key)
    shift; cliclick "$@"; sleep 1
    ;;
  --geom)
    win
    ;;
  *)
    name="${1:?usage: capture.sh <name>}"
    read -r X Y W H <<<"$(win)"
    screencapture -x -R"$X,$Y,$W,$H" "$SHOTS/$name.png"
    echo "$SHOTS/$name.png ($W x $H)"
    ;;
esac
