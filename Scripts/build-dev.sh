#!/bin/bash
# Dev build+launch loop: cargo build (debug) -> kill any running instance -> relaunch.
set -euo pipefail
cd "$(dirname "$0")/.."

cd rust
cargo build -p tiller -p tiller_control
cd ..

BIN="rust/target/debug/tiller"
LOG="/tmp/tiller-dev.log"

pkill -x tiller || true

# `nohup` keeps the direct app process alive after this script exits and sends
# its output to the log on macOS; `setsid` provides the same behavior on Linux.
if [[ "$(uname -s)" == "Darwin" ]]; then
  nohup "$BIN" >"$LOG" 2>&1 </dev/null &
else
  setsid "$BIN" >"$LOG" 2>&1 &
fi
disown
echo "launched $BIN (pid $!); log: $LOG"
