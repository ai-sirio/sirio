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

# setsid detaches the new instance from this script's session so it survives the
# script exiting, the same as `open` detaching the app from the shell on macOS.
setsid "$BIN" >"$LOG" 2>&1 &
disown
echo "launched $BIN (pid $!); log: $LOG"
