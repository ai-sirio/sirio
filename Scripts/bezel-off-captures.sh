#!/bin/bash
set -euo pipefail

# The capture set every bezel-off PR is judged by (docs/testing/bezel-off.md): the frames of the
# visual sweep, the surfaces probe, the Changes E2E and the Ely chat E2E, taken from the tree
# this script runs in. A base run uses a worktree of the base commit. Scripts/capture-diff.py
# compares two such sets.
#
# What makes two runs of one build comparable: SIRIO_REDUCED_MOTION=1 (a debug build snaps every
# animation to its rest state), Mesa's software Vulkan (lavapipe) as the only renderer, one fixed
# Xvfb geometry with --xvfb, and one build that every source drives (CARGO_TARGET_DIR is unset,
# the build is made here, up front). What stays clock-driven after that is masked in
# Scripts/capture-masks.tsv, or dropped by name in DROPPED below.
#
# Artifact: --out-dir DIR keeps captures.log, runs/<source>/ (each source's own --out-dir:
# transcript, app log, every frame it took) and frames/<source>-<appearance>/ (only the PNG
# frames, the set capture-diff compares). Rerunning the script reproduces it; DIR/frames and
# DIR/runs are cleared first, so a frame a source stopped producing cannot linger.
#
# Usage: Scripts/bezel-off-captures.sh --out-dir DIR (--xvfb | --display :N)
#   --xvfb        start a private Xvfb (1600x1000x24) on a free display and stop it on exit
#   --display :N  use this display; its geometry is part of every frame, so use the same one
#                 for the base and the PR runs
# Prints "CAPTURES OK: DIR (<n> frames)". Any source that fails fails the script.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR=""
START_XVFB=0
DISPLAY_TARGET=""
XVFB_GEOMETRY="1600x1000x24"
LVP_ICD="${SIRIO_CAPTURE_VK_ICD:-/usr/share/vulkan/icd.d/lvp_icd.json}"

# Frames that cannot be made deterministic with a mask are dropped from the set, one entry each,
# "frame name (its path under frames/, no .png)|why". Each prints a NOTE line; none is silent.
# The chat E2E's two behaviour scenarios (composer_keys, reduced_motion) are
# here for their frames only: the scenarios themselves run in every capture, but they are checks
# with their own pass or fail, and their frames postdate the determinism runs. Scenario
# directories are named by the Scenario(...) call (composer-keys), not by the function.
DROPPED=(
  "chat-ui-default/scenarios/composer-keys/frames/before-slash|composer_keys postdates the determinism runs (real keys, a live caret)"
  "chat-ui-default/scenarios/composer-keys/frames/slash-popup|composer_keys postdates the determinism runs (real keys, a live caret)"
  "chat-ui-default/scenarios/composer-keys/frames/slash-dismissed|composer_keys postdates the determinism runs (real keys, a live caret)"
  "chat-ui-default/scenarios/reduced-motion/frames/still-1|reduced_motion postdates the determinism runs (a burst taken 250 ms apart)"
  "chat-ui-default/scenarios/reduced-motion/frames/still-2|reduced_motion postdates the determinism runs (a burst taken 250 ms apart)"
  "chat-ui-default/scenarios/reduced-motion/frames/still-3|reduced_motion postdates the determinism runs (a burst taken 250 ms apart)"
)

die() { echo "FAIL: $*" >&2; exit 1; }
usage() { sed -n '/^# Usage:/,/^# Prints/p' "$0" | sed 's/^# \{0,1\}//'; }

while [ $# -gt 0 ]; do
  case "$1" in
    --out-dir) [ $# -ge 2 ] || die "--out-dir needs a directory"; OUT_DIR="$2"; shift 2 ;;
    --xvfb) START_XVFB=1; shift ;;
    --display) [ $# -ge 2 ] || die "--display needs a display like :99"; DISPLAY_TARGET="$2"; shift 2 ;;
    --help|-h) usage; exit 0 ;;
    *) die "unknown argument: $1 (use --help)" ;;
  esac
done
[ -n "$OUT_DIR" ] || die "--out-dir DIR is required"
if [ "$START_XVFB" -eq 1 ] && [ -n "$DISPLAY_TARGET" ]; then die "pass --xvfb or --display :N, not both"; fi
if [ "$START_XVFB" -eq 0 ] && [ -z "$DISPLAY_TARGET" ]; then die "no display: pass --xvfb or --display :N"; fi

# Scratch never goes to /tmp (a small tmpfs); control sockets must stay under 108 bytes. The path
# is fixed, not an option; the length check is for a $HOME that is unusually long.
export TMPDIR="$HOME/.cache/sirio-e2e-tmp"
mkdir -p "$TMPDIR"
[ "${#TMPDIR}" -le 60 ] || die "TMPDIR '$TMPDIR' is too long: a control socket path must stay under 108 bytes"
export SIRIO_REDUCED_MOTION=1
[ -f "$LVP_ICD" ] || die "Mesa's software Vulkan ($LVP_ICD) is required: it is the one renderer every capture uses (SIRIO_CAPTURE_VK_ICD names another file)"
export VK_ICD_FILENAMES="$LVP_ICD" VK_DRIVER_FILES="$LVP_ICD"
# visual-sweep.sh drives rust/target/debug/sirio whatever this says; every source must drive one build.
unset CARGO_TARGET_DIR

mkdir -p "$OUT_DIR"
OUT_ARG="$OUT_DIR"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"
FRAMES="$OUT_DIR/frames"
RUNS="$OUT_DIR/runs"
rm -rf "$FRAMES" "$RUNS"
mkdir -p "$FRAMES" "$RUNS"
exec > >(tee "$OUT_DIR/captures.log") 2>&1

XVFB_PID=""
cleanup() { [ -z "$XVFB_PID" ] || kill "$XVFB_PID" 2>/dev/null || true; }
trap cleanup EXIT

# start_xvfb: a private Xvfb on the first free display from :90 to :99. Two runs started together
# (the base and the head) can both see :90 free. Xvfb refuses a display whose lock file exists, so
# the loser exits, but the xdpyinfo that waits for readiness may already have answered from the
# winner's server. The lock file holds the pid of the server that owns the display, so a display is
# ours only if that pid is our Xvfb; otherwise this one is given up and the next number is tried.
start_xvfb() {
  local n lock_pid ready
  command -v Xvfb >/dev/null || die "Xvfb is required for --xvfb"
  for n in $(seq 90 99); do
    if [ -e "/tmp/.X11-unix/X$n" ] || [ -e "/tmp/.X$n-lock" ]; then continue; fi
    Xvfb ":$n" -screen 0 "$XVFB_GEOMETRY" -nolisten tcp >"$OUT_DIR/xvfb.log" 2>&1 &
    XVFB_PID=$!
    ready=0
    for _ in $(seq 1 50); do
      kill -0 "$XVFB_PID" 2>/dev/null || break
      if timeout 2 xdpyinfo -display ":$n" >/dev/null 2>&1; then ready=1; break; fi
      sleep 0.1
    done
    lock_pid="$(tr -d ' \n' <"/tmp/.X$n-lock" 2>/dev/null || true)"
    if [ "$ready" -eq 1 ] && kill -0 "$XVFB_PID" 2>/dev/null && [ -n "$lock_pid" ] \
      && [ "$((10#$lock_pid))" -eq "$XVFB_PID" ]; then
      DISPLAY_TARGET=":$n"
      return 0
    fi
    kill "$XVFB_PID" 2>/dev/null || true
    wait "$XVFB_PID" 2>/dev/null || true
    XVFB_PID=""
    echo "NOTE display :$n was taken by another run; trying the next one"
  done
  die "no free display between :90 and :99"
}
if [ "$START_XVFB" -eq 1 ]; then start_xvfb; fi
command -v xdpyinfo >/dev/null || die "xdpyinfo is required"
timeout 3 xdpyinfo -display "$DISPLAY_TARGET" >/dev/null 2>&1 || die "no usable X display '$DISPLAY_TARGET'"
DIMENSIONS="$(xdpyinfo -display "$DISPLAY_TARGET" | awk '/dimensions:/ { print $2 }')"

echo "bezel-off captures of $ROOT"
echo "commit: $(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo unknown)"
echo "display: $DISPLAY_TARGET ($DIMENSIONS), renderer: $LVP_ICD, SIRIO_REDUCED_MOTION=$SIRIO_REDUCED_MOTION, TMPDIR=$TMPDIR"

echo "building sirio, sirioctl, the chat E2E's seed and probe and the surfaces probe"
(cd "$ROOT/rust" && cargo build --quiet -p sirio --bin sirio -p sirio_control --bin sirioctl \
  -p sirio_persistence --example ely_chat_seed --example appearance_seed \
  -p sirio_ui --example ely_chat_probe --example surfaces_probe)

# run_source NAME COMMAND...: one source, its console in runs/NAME.console.log.
run_source() {
  local name="$1"
  shift
  echo "== source $name"
  mkdir -p "$RUNS"
  if ! "$@" >"$RUNS/$name.console.log" 2>&1; then
    tail -40 "$RUNS/$name.console.log" >&2 || true
    die "source $name failed (full console: $RUNS/$name.console.log)"
  fi
  echo "ok: $name: $(tail -n 1 "$RUNS/$name.console.log")"
}

# keep_flat RUN DEST: RUN's frames/*.png, as FRAMES/DEST/<name>.png. A source that gives no frame
# at all has failed, whatever else it printed.
keep_flat() {
  local count=0 frame
  mkdir -p "$FRAMES/$2"
  for frame in "$RUNS/$1"/frames/*.png; do
    [ -f "$frame" ] || continue
    cp "$frame" "$FRAMES/$2/"
    count=$((count + 1))
  done
  [ "$count" -gt 0 ] || die "source $1 produced no frame under $RUNS/$1/frames"
  echo "kept $count frames: frames/$2"
}

# keep_tree RUN DEST PATTERN...: files under RUN matching each glob, keeping their path under RUN
# (the chat E2E's scenarios reuse frame names, so flattening them would collide). Each pattern is
# counted on its own: a lane that gave nothing (the chat E2E's identity lane) while another did
# would otherwise leave the set short without a word. A pattern with no frame fails the script,
# unless the source printed a GAP, NOTE or SKIP line about that lane (its first path segment, the
# first 7 letters of it, so `identity` finds "SKIP identities: ..."), which is then echoed.
keep_tree() {
  local run="$1" dest="$2" pattern frame rel lane count total=0
  shift 2
  for pattern in "$@"; do
    count=0
    for frame in "$RUNS/$run"/$pattern; do
      [ -f "$frame" ] || continue
      rel="${frame#"$RUNS/$run"/}"
      mkdir -p "$FRAMES/$dest/$(dirname "$rel")"
      cp "$frame" "$FRAMES/$dest/$rel"
      count=$((count + 1))
    done
    if [ "$count" -eq 0 ]; then
      lane="${pattern%%/*}"
      if grep -Eiq "^(GAP|NOTE|SKIP)[: ].*${lane:0:7}" "$RUNS/$run.console.log"; then
        echo "NOTE source $run gave no frame for $pattern; its own line says why: $(grep -Ei "^(GAP|NOTE|SKIP)[: ].*${lane:0:7}" "$RUNS/$run.console.log" | head -n 1)"
      else
        die "source $run produced no frame matching $pattern, and printed no GAP, NOTE or SKIP line about it"
      fi
    fi
    echo "kept $count frames: frames/$dest ($pattern)"
    total=$((total + count))
  done
  [ "$total" -gt 0 ] || die "source $run produced no frame matching: $*"
}

for appearance in dark light; do
  run_source "sweep-$appearance" "$ROOT/Scripts/visual-sweep.sh" \
    --out-dir "$RUNS/sweep-$appearance" --display "$DISPLAY_TARGET" \
    --setting "appearance.theme=\"$appearance\""
  keep_flat "sweep-$appearance" "sweep-$appearance"
done

# The surfaces probe (the Settings pages, a tab menu, a tooltip): the build above is the one it
# drives, hence --no-build. A missing script or a failed probe fails like any other source.
for appearance in dark light; do
  run_source "surfaces-$appearance" "$ROOT/Scripts/Tests/test-surfaces-probe.sh" \
    --out-dir "$RUNS/surfaces-$appearance" --display "$DISPLAY_TARGET" --appearance "$appearance" --no-build
  keep_flat "surfaces-$appearance" "surfaces-$appearance"
done

for appearance in dark light; do
  run_source "changes-$appearance" "$ROOT/Scripts/Tests/test-changes-e2e.sh" \
    --out-dir "$RUNS/changes-$appearance" --display "$DISPLAY_TARGET" --appearance "$appearance"
  keep_flat "changes-$appearance" "changes-$appearance"
done

# No --only: it would skip the identity lane. No --appearance exists: one run, its own default.
echo "NOTE test-ely-chat-ui-e2e.py has no --appearance flag: it runs once, in its default appearance (frames/chat-ui-default)"
run_source chat-ui-default python3 "$ROOT/Scripts/Tests/test-ely-chat-ui-e2e.py" \
  --out-dir "$RUNS/chat-ui-default" --display "$DISPLAY_TARGET"
keep_tree chat-ui-default chat-ui-default "scenarios/*/frames/*.png" "identity/*.png"

for entry in ${DROPPED[@]+"${DROPPED[@]}"}; do
  frame="${entry%%|*}"
  why="${entry#*|}"
  if [ -f "$FRAMES/$frame.png" ]; then
    rm -f "$FRAMES/$frame.png"
    echo "NOTE dropped $frame: $why"
  else
    echo "NOTE the drop list names $frame, which no source produced (dropped: $why)"
  fi
done

COUNT="$(find "$FRAMES" -name '*.png' | wc -l | tr -d ' ')"
[ "$COUNT" -gt 0 ] || die "no frame was captured"
echo "CAPTURES OK: $OUT_ARG ($COUNT frames)"
