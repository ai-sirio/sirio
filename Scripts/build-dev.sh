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

# `open` detaches the app from the shell on macOS; `setsid` provides the same
# behavior on Linux.
if [[ "$(uname -s)" == "Darwin" ]]; then
  open "$BIN" >"$LOG" 2>&1 &
else
  setsid "$BIN" >"$LOG" 2>&1 &
fi
disown
echo "launched $BIN (pid $!); log: $LOG"
