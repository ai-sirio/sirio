#!/usr/bin/env bash
# P72: run the real GPUI + WebKitGTK child-window spike and capture it.
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$ROOT/artifacts/p72-browser-spike.png}"
DISP="${2:-:1}"
LOG="${OUT%.png}.log"

mkdir -p "$(dirname "$OUT")"
source "$HOME/.cargo/env"
export DISPLAY="$DISP"
unset WAYLAND_DISPLAY
export GPUI_X11_SCALE_FACTOR=1

(cd "$ROOT/rust" && cargo run -p sirio_ui --example browser_spike) >"$LOG" 2>&1 &
APP_PID=$!
trap 'kill "$APP_PID" 2>/dev/null || true' EXIT

WID=""
for _ in $(seq 1 120); do
    kill -0 "$APP_PID" 2>/dev/null || {
        echo "browser spike exited before mapping; see $LOG" >&2
        tail -80 "$LOG" >&2
        exit 4
    }
    WID="$(DISPLAY="$DISP" xwininfo -root -children 2>/dev/null | python3 -c '
import re, sys
best = None
for line in sys.stdin:
    match = re.match(r"\s+(0x[0-9a-f]+).*?(\d+)x(\d+)\+", line)
    if not match:
        continue
    width, height = int(match.group(2)), int(match.group(3))
    if width > 1200 and height > 700:
        best = match.group(1)
print(best or "")
')"
    [ -n "$WID" ] && break
    sleep 0.25
done

[ -n "$WID" ] || { echo "browser spike mapped no window; see $LOG" >&2; exit 3; }
sleep 8
timeout 30 env DISPLAY="$DISP" import -window "$WID" "$OUT"
[ -s "$OUT" ]
echo "captured $OUT"
echo "log $LOG"
