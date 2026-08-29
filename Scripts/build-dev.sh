#!/bin/bash
# Dev build+launch loop: cargo build (debug) -> kill any running instance -> relaunch.
set -euo pipefail
cd "$(dirname "$0")/.."

cd rust
cargo build -p sirio -p sirio_control
cd ..

BIN="rust/target/debug/sirio"
LOG="/tmp/sirio-dev.log"

pkill -x sirio || true

# `setsid` keeps the direct process alive on Linux after this script exits.
if [[ "$(uname -s)" == "Darwin" ]]; then
  # Control Center silently refuses to display an NSStatusItem (#99's tray
  # icon) for a process it doesn't recognize as an app -- no error, the item
  # is simply never drawn. Neither signing the bare binary nor `exec`-ing it
  # directly is enough; it has to be a signed .app bundle launched through
  # LaunchServices (`open`, not run directly), and -- the one that cost real
  # time to pin down -- the bundle can't live under this checkout: it sits
  # under `~/Desktop`, which is one of TCC's protected folders, and Control
  # Center (an out-of-process renderer that has to read the bundle's own
  # Info.plist/icon to draw the item) silently declines rather than prompt
  # for access it doesn't already have. Staging the wrapper in `$TMPDIR`
  # instead of `rust/target/debug/` sidesteps that; `open`'s own
  # `-o`/`--stderr` redirect the log the way `nohup`'s redirection did for
  # the bare binary.
  APP="${TMPDIR:-/tmp/}sirio-dev/Sirio.app"
  rm -rf "$APP"
  mkdir -p "$APP/Contents/MacOS"
  cat >"$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>dev.sirio.sirio-dev</string>
  <key>CFBundleName</key><string>Sirio</string>
  <key>CFBundleExecutable</key><string>sirio</string>
  <key>CFBundlePackageType</key><string>APPL</string>
</dict>
</plist>
PLIST
  cp "$BIN" "$APP/Contents/MacOS/sirio"
  codesign --force --sign - "$APP"
  open -n --stdout "$LOG" --stderr "$LOG" "$APP"
else
  setsid "$BIN" >"$LOG" 2>&1 &
  disown
fi
echo "launched $BIN; log: $LOG"
