#!/bin/bash
set -euo pipefail

# End-to-end test of the Changes surface on Ely (spec
# 2026-10-03-change-requests-on-ely-design.md §8): a real, isolated Sirio on a
# real git repository holding a staged, a changed and an untracked file,
# driven over the control socket. It opens the surface, draws a replaced line
# unified and split, makes git fail and recover, asks for Discard inside the
# window and confirms it (a debug build's `surface changes confirm`), and asks
# for Discard all and closes the dialog without discarding.
#
# The artifact: --out-dir DIR (default artifacts/changes-e2e-<stamp>-<pid>)
# keeps transcript.log, app.log and -- unless --state-only -- PID-matched
# window captures in frames/. Rerunning the script reproduces it.
#
# Usage: Scripts/Tests/test-changes-e2e.sh [--state-only] [--out-dir DIR] [--display :N]
#          [--appearance light|dark]

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/sirio"
CTL="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/sirioctl"
STATE_ONLY=0
OUT_DIR=""
APPEARANCE=""
DISPLAY_TARGET="${DISPLAY:-}"
while [ $# -gt 0 ]; do
  case "$1" in
    --state-only) STATE_ONLY=1; shift ;;
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    --display) DISPLAY_TARGET="$2"; shift 2 ;;
    --appearance) APPEARANCE="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/changes-e2e-$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$OUT_DIR/frames"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() { echo "FAIL: $*" >&2; exit 1; }
if [ "$STATE_ONLY" -eq 0 ]; then
  [ -n "$DISPLAY_TARGET" ] || fail "no DISPLAY; pass --display :N or --state-only"
  for tool in import identify xwininfo xprop; do
    command -v "$tool" >/dev/null || fail "$tool is required for captures; pass --state-only to skip them"
  done
fi
command -v git >/dev/null || fail "git is required"
command -v python3 >/dev/null || fail "python3 is required"

RUN_DIR=$(mktemp -d "${TMPDIR:-/tmp}/sirio-changes-e2e-XXXXXX")
APP_PID=""
cleanup() {
  [ -z "$APP_PID" ] || { kill "$APP_PID" 2>/dev/null || true; wait "$APP_PID" 2>/dev/null || true; }
  [ ! -d "$RUN_DIR/repo/.git-away" ] || mv "$RUN_DIR/repo/.git-away" "$RUN_DIR/repo/.git"
  cp "$RUN_DIR"/*.log "$OUT_DIR/" 2>/dev/null || true
  rm -rf "$RUN_DIR"
}
trap cleanup EXIT

echo "building sirio and sirioctl"
(cd "$ROOT/rust" && cargo build --quiet -p sirio --bin sirio && cargo build --quiet -p sirio_control --bin sirioctl)

ctl() { echo "+ sirioctl $*"; "$CTL" "$@"; }
reply() { "$CTL" "$@" --json; }
field() { python3 -c 'import json, sys; print(json.load(sys.stdin)[0].get(sys.argv[1], ""))' "$1"; }
read_field() { # key sirioctl-args...
  local key=$1
  shift
  reply "$@" | field "$key" || fail "sirioctl $* did not answer (reading $key)"
}
wait_for() { # key value sirioctl-args...
  local key=$1 want=$2
  shift 2
  local got=""
  for _ in $(seq 1 100); do
    got=$(reply "$@" | field "$key" || true)
    [ "$got" = "$want" ] && { echo "OK: $key=$want"; return 0; }
    sleep 0.3
  done
  reply "$@" || true
  fail "$key never became '$want' (last: '$got') for: $*"
}
wait_nonempty() { # key sirioctl-args...
  local key=$1
  shift
  local got=""
  for _ in $(seq 1 100); do
    got=$(reply "$@" | field "$key" || true)
    [ -n "$got" ] && { echo "OK: $key='$got'"; return 0; }
    sleep 0.3
  done
  reply "$@" || true
  fail "$key stayed empty for: $*"
}

find_window() {
  DISPLAY_TARGET="$DISPLAY_TARGET" APP_PID="$APP_PID" python3 - <<'PY'
import os, re, subprocess
display = os.environ["DISPLAY_TARGET"]
want = int(os.environ["APP_PID"])
listing = subprocess.run(["xwininfo", "-display", display, "-root", "-children"], capture_output=True, text=True, timeout=5).stdout
best = None
for line in listing.splitlines():
    match = re.match(r"\s+(0x[0-9a-fA-F]+).*?\s(\d+)x(\d+)\+", line)
    if not match:
        continue
    window, width, height = match.group(1), int(match.group(2)), int(match.group(3))
    prop = subprocess.run(["xprop", "-display", display, "-id", window, "_NET_WM_PID"], capture_output=True, text=True, timeout=5).stdout
    pid = re.search(r"=\s*(\d+)\s*$", prop)
    if pid and int(pid.group(1)) == want and (best is None or width * height > best[0]):
        best = (width * height, window)
if best:
    print(best[1])
PY
}
capture() { # name
  [ "$STATE_ONLY" -eq 0 ] || return 0
  sleep 2
  local window
  window=$(find_window)
  [ -n "$window" ] || fail "no window with _NET_WM_PID=$APP_PID for $1"
  import -display "$DISPLAY_TARGET" -window "$window" "$OUT_DIR/frames/$1.png"
  local colours
  colours=$(identify -format '%k' "$OUT_DIR/frames/$1.png")
  [ "$colours" -ge 200 ] || fail "$1 is blank ($colours colours)"
  echo "FRAME: $1 ($colours colours)"
}

# ---- the repository: one staged, one changed (a replaced word), one untracked
WT="$RUN_DIR/repo"
mkdir -p "$WT/src"
git -C "$WT" init -q -b main
git -C "$WT" config user.email tester@example.invalid
git -C "$WT" config user.name Tester
python3 - "$WT" <<'PY'
import sys
root = sys.argv[1]
lines = [f"fn line_{n}() -> u32 {{ {n} }}\n" for n in range(1, 41)]
open(f"{root}/src/app.rs", "w").write("".join(lines))
open(f"{root}/README.md", "w").write("# app\n")
PY
git -C "$WT" add -A
git -C "$WT" commit -q -m first
python3 - "$WT" <<'PY'
import sys
root = sys.argv[1]
path = f"{root}/src/app.rs"
lines = open(path).read().splitlines(keepends=True)
lines[19] = "fn line_20() -> u32 { 2000 }\n"
lines.insert(30, "fn città() -> &'static str { \"è\" }\n")
open(path, "w").write("".join(lines))
open(f"{root}/notes.md", "w").write("staged notes\n")
open(f"{root}/scratch.txt", "w").write("untracked\n")
PY
git -C "$WT" add notes.md

export SIRIO_SOCKET="$RUN_DIR/control.sock"
export SIRIO_DB="$RUN_DIR/session.sqlite"
export SIRIO_CREDENTIALS="$RUN_DIR/credentials.json"
if [ -n "$APPEARANCE" ]; then
  (cd "$ROOT/rust" && cargo run --quiet -p sirio_persistence --example appearance_seed -- --database "$SIRIO_DB" --appearance "$APPEARANCE") || fail "could not seed the appearance"
fi
if [ "$STATE_ONLY" -eq 1 ]; then
  (cd "$WT" && exec env -u DISPLAY -u WAYLAND_DISPLAY "$BIN" >"$RUN_DIR/app.log" 2>&1) &
else
  (cd "$WT" && exec env -u WAYLAND_DISPLAY DISPLAY="$DISPLAY_TARGET" GPUI_X11_SCALE_FACTOR=1 "$BIN" >"$RUN_DIR/app.log" 2>&1) &
fi
APP_PID=$!
for _ in $(seq 1 75); do
  [ -S "$SIRIO_SOCKET" ] && break
  sleep 0.2
done
[ -S "$SIRIO_SOCKET" ] || fail "sirio never opened its control socket"

echo "step 1: the surface reads the repository's three sections"
ctl project add "$WT"
ctl select-workspace --workspace "$WT"
ctl surface changes open
wait_for ready true surface changes read
wait_for stagedCount 1 surface changes read
wait_for changedCount 1 surface changes read
wait_for untrackedCount 1 surface changes read

echo "step 2: unified, the changed file open"
ctl surface changes view --mode unified --expand src/app.rs
wait_for mode unified surface changes read
capture local-unified

echo "step 3: split"
ctl surface changes view --mode split
wait_for mode split surface changes read
capture local-split
ctl surface changes view --mode unified
wait_for mode unified surface changes read

echo "step 4: a repository git cannot read says so, and recovers"
mv "$WT/.git" "$WT/.git-away"
ctl surface changes view --refresh
wait_nonempty error surface changes read
capture error
mv "$WT/.git-away" "$WT/.git"
ctl surface changes view --refresh
wait_for error "" surface changes read

echo "step 5: Discard asks inside the window, and its confirm discards"
ctl surface changes dialog --discard src/app.rs
wait_for dialog "discard:src/app.rs" surface changes read
capture discard-dialog
ctl surface changes confirm
wait_for dialog "" surface changes read
wait_for changedCount 0 surface changes read
[ -z "$(git -C "$WT" diff -- src/app.rs)" ] || fail "the confirmed Discard left src/app.rs changed"
echo "OK: the confirmed Discard restored src/app.rs"

echo "step 6: Discard all asks too, and closing it discards nothing"
echo "more" >> "$WT/README.md"
ctl surface changes view --refresh
wait_for changedCount 1 surface changes read
ctl surface changes dialog --discard-all
wait_for dialog discard-all surface changes read
ctl surface changes dialog --close
wait_for dialog "" surface changes read
wait_for changedCount 1 surface changes read
[ -n "$(git -C "$WT" diff -- README.md)" ] || fail "closing the dialog discarded README.md"
echo "OK: closing the dialog changed nothing"

echo "CHANGES E2E OK"
