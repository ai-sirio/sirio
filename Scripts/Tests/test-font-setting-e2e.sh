#!/bin/bash
set -eEuo pipefail

# End-to-end test of Settings -> Appearance -> Interface -> Font (spec
# docs/superpowers/specs/2026-11-21-font-setting-design.md §1, §2, §5): a real,
# isolated debug Sirio under a private Xvfb, driven with real X clicks and keys at the
# pixels the app reports (`surface layout read`'s shellRects: settingsFont and
# settingsFontSizeUp) and read back over the control socket (`surface attention read`'s
# `fonts`, `surface settings read`'s fontPair, `surface changes read`'s numberCell).
# Every step asserts, writes its rows to measurements.tsv and captures frames/. It proves:
#    1  a fresh database: the pair is atkinson, and Sirio's typography, Ely and bezel name
#       Atkinson's families and every measured width matches its bundled files (a SKIP in
#       --state-only when the read carries no widths); the startup line "[fonts] registered N interface faces in T"
#       is parsed into measurements.tsv (the debug build's time)
#    2  the sheet: a project with a source file open in the editor, a chat tab on the
#       font-sheet stub (the fixture's font_sheet mode answers one fixed markdown text),
#       and a terminal tab printing a fixed line. The terminal's crop is kept once.
#       sheet-atkinson-<appearance>.png is taken
#    3  inter, plex, redhat, recursive, figtree, geist, atkinson, then plex again, each through the
#       real Settings UI: Settings opens on Appearance, the page is waited for (the
#       trigger's rect non-null and equal over two reads), a click on the Font select, then
#       Down x n and Enter, or, for figtree, a click on its row. Right after the choice, before
#       anything else saves the settings, the database row appearance.fontPair is polled for
#       the id. Afterwards: `fontPair` is
#       the pair; `fonts` names the pair's families in typography, Ely and bezel; every
#       `fonts.widths` sample (ten `m`, `a`, `0`) is within 0.5 px of ten advances that
#       font_face_advance.py reads from the bundled file the CSS rule picks for that slot
#       (no italic file: skipped, said in the row); the sidebar's New Chat row (interface
#       text) and the editor's code lines (code) crop differently from the previous pair's;
#       Settings closes (Escape); sheet-<id>-<appearance>.png is taken. After the second
#       plex (and atkinson) the crops equal the first visit's; the chat reply's crop is
#       compared like the other two, and the terminal's crop (its pane is always drawn) equals
#       step 2's in every pair's frame. bezel's sans fallback for Red Hat equals
#       the one Geist reads (both bundle no italic)
#    4  Escape with the list open: a crop of where the list was drawn equals the crop
#       before it opened and Settings still reads; a second Escape closes Settings
#    5  with plex: one click on the interface size's increase button (the size is one
#       more, `fonts` still names Plex), a click on the decrease button (back), then the
#       terminal tab's crop equals the one of step 2: the terminal does not change
#    6  quit: the database row appearance.fontPair is plex; relaunch: the first `fonts`
#       read names Plex
#    7  quit, seed appearance.fontPair = 'nope', relaunch: atkinson
#    8  quit, seed appearance.uiFontSize = 18 (pair atkinson), relaunch; a tracked file of
#       12,400 lines with a line inserted as line 12,345 is open in Changes (unified): the strip of
#       pixels [x-6, x) left of `numberCell` (x its left edge), over the cell's height, is
#       one colour and equals the row's background (and the cell itself holds digit ink, its
#       rightmost ink column within 3 px of its right edge, so the strip is not vacuous), so no digit ink crossed the cell's left
#       edge. Six, not eight: the old number's last digit ends at exactly x-8 (the row gap)
#       and its antialiasing must not count. The cells are whitespace_nowrap (spec §2.9):
#       without it gpui wraps an overflowing number at a character and the strip stays
#       clean while a digit is lost. The probed row is the added line (the first row drawn:
#       the unchanged lines fold into bands), new number 12,345, in the hunk that starts at
#       12,342 (asserted from `git diff`)
#       --diff-only runs only this step on a fresh database seeded with uiFontSize 18, and
#       ends FONT SETTING E2E OK (diff only); that run failing at the strip assertion with
#       `diff_number_width` forced to 40 is the proof the check can fail
#    9  contact-sheet-<appearance>.png: the seven sheet frames, labelled by title, scaled
#       down; contact-sheet-detail-<appearance>.png: per pair, at 1:1, the chat's reply and
#       the editor's first 25 lines, labelled
# The run ends with FONT SETTING E2E OK (<appearance>). --state-only runs steps 1 and 7
# without a display and ends FONT SETTING E2E OK (state only).
#
# The sheet: no control verb puts the chat and the editor in one window (a tab is one
# surface), so each sheet frame is two window captures per pair, the chat tab above the
# editor tab, under a strip naming the pair: sheet-<id>-<appearance>.png (the two captures
# are sheet-<id>-chat-.. and sheet-<id>-editor-..). The terminal keeps JetBrainsMono Nerd
# Font Mono whatever the pair: its crop is the one that must never change.
#
# Limits of what the widths prove (the rows are in measurements.tsv, never silent):
#  - Where the 400 and the 700 advances of a role are equal (every monospace face, and
#    Recursive Sans), the width cannot tell the weight: `weight-limit` rows.
#  - Where a family is missing the text system falls back through gpui's stack (.ZedMono ->
#    Lilex, .ZedSans -> IBM Plex Sans) or the system's, and a fallback whose `m` has the same
#    advance passes the width within 0.5 px. JetBrains Mono, IBM Plex Mono, Red Hat Mono,
#    Recursive Mono and Geist Mono draw `m` at 0.600 em, so for them the width proves neither
#    the weight nor the family; Atkinson Mono (0.632 em) and Fira Code (0.615 em) are told apart
#    by width. `width-limit` rows (status limit). The family of the code faces is carried by
#    the code crop differing across pairs.
#  - `italic-limit` is the same for the italic, `installed-limit` says a bundled family is
#    also installed on the machine (its system copy may be drawn instead of the bundled file).
#  - Rows have the status ok, limit (a limit of what the row can prove, never silent), skip (a
#    legitimate skip: no italic file for Red Hat and Geist) or FAIL; the last check prints the
#    count of each and accepts nothing else.
# The tolerance is 0.5 px and no rounding is modelled: the text system's width is the sum
# of unrounded advances at the requested size, plus the kerning the shaper applies between
# two `m` (Geist kerns m-m by -20 to -14 units, about 2 % of its width: the helper reads it
# from the GPOS table; the other families have no such pair).
#
# Keys on the open list (Ely's Select, rust/vendor/ely/crates/ely-core/src/forms/select.rs
# and options.rs): the list opens with the current pair highlighted (`start` is the
# selected row), Down and Up step with `rem_euclid` (they wrap at both ends), Enter picks
# the highlighted row. So Down is pressed (target - current) mod 7 times from the current
# pair, in FontPair::ALL order: atkinson inter plex redhat recursive figtree geist.
# Figtree (row 5) is picked by a click on its row. The click's place, from the trigger
# rect (x, y, w, h): the list floats at the trigger's bottom left (anchored, the open
# animation's 4 px NUDGE has settled), min_w the trigger's width; its surface has a 1 px
# border and p_1 (4 px); each row is py_1 (4 + 4 px) plus one line of the Sm text. So row k
# centres at y + h + LIST_BORDER_PAD + (k + 0.5) * ROW_PITCH, with LIST_BORDER_PAD = 1 + 4
# and ROW_PITCH = 8 + the Sm line height (ROW_LINE below, 20 at the default interface
# size: a measurement of a frame of this run, checked by the pair that results).
#
# The size stepper (rust/crates/sirio_ui/src/controls.rs `stepper_probed`): a 28 px
# decrease button, a rail of at least 48 px, a 28 px increase button. The decrease button's
# centre is therefore the increase button's centre minus (28 + 48) px.
#
# Isolation: the app starts under env -i with an allowlist (launch_app), from a copy of its
# binary with no sirio-host beside it and SIRIO_HOST_BIN naming a missing file. PATH is the
# font-sheet claude stub, the notify-send stub, the run's bin, then /usr/bin:/bin; HOME,
# XDG directories, the database, the socket and the credentials are the run's own. Every
# process the app starts carries SIRIO_FONT_E2E_RUN=<run dir>; the last check quits the
# app and fails if any of them is still alive.
#
# A framed run needs --display :N, a private Xvfb of at least 1600x1000 that this user
# started (never $DISPLAY), Mesa's lavapipe (lvp_icd*.json), import, identify, xwininfo,
# xprop, xdotool and ImageMagick's magick or convert (with montage).
#
# The artifact: --out-dir DIR (must be empty; default
# artifacts/font-setting-e2e-<stamp>-<pid>) keeps transcript.log, app.log,
# measurements.tsv (case, key, expected, drawn, ok), reads/ (the fonts read of every pair),
# crops/ and PID-matched window captures in frames/, named <what>-<appearance>.png. Run
# once with --appearance light and once with --appearance dark (dark when omitted).
#
# Usage: Scripts/Tests/test-font-setting-e2e.sh --display :N [--appearance light|dark]
#          [--diff-only] [--out-dir DIR]
#        Scripts/Tests/test-font-setting-e2e.sh --state-only [--out-dir DIR]
#   Xvfb :97 -screen 0 1720x1000x24 &   # a private display for a framed run

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/sirio"
CTL="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/sirioctl"
ASSETS="$ROOT/rust/assets/fonts"
HELPER="$ROOT/Scripts/Tests/font_face_advance.py"
WINDOW_W=1600
WINDOW_H=1000
STATE_ONLY=0
DIFF_ONLY=0
OUT_DIR=""
APPEARANCE=""
DISPLAY_TARGET=""
while [ $# -gt 0 ]; do
  case "$1" in
    --state-only) STATE_ONLY=1; shift ;;
    --diff-only) DIFF_ONLY=1; shift ;;
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    --display) DISPLAY_TARGET="$2"; shift 2 ;;
    --appearance) APPEARANCE="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

refuse() { echo "refused: $*" >&2; exit 2; }
case "$APPEARANCE" in ""|light|dark) ;; *) refuse "--appearance is light or dark" ;; esac
[ "$STATE_ONLY" -eq 0 ] || [ -z "$DISPLAY_TARGET" ] || refuse "--state-only runs with no display; drop --display"
[ "$STATE_ONLY" -eq 0 ] || [ "$DIFF_ONLY" -eq 0 ] || refuse "--diff-only paints a diff: it needs a display, not --state-only"
display_number() { # DISPLAY-string
  [[ "$1" =~ ^([A-Za-z0-9._-]*):([0-9]+)(\.[0-9]+)?$ ]] || return 0
  [ -z "${BASH_REMATCH[1]}" ] || [ "${BASH_REMATCH[1]}" = localhost ] || return 0
  echo $((10#${BASH_REMATCH[2]}))
}
IM=""
MONTAGE=()
if [ "$STATE_ONLY" -eq 0 ]; then
  for tool in import identify xwininfo xprop xdotool timeout; do
    command -v "$tool" >/dev/null || refuse "$tool is required for a framed run; pass --state-only to skip the clicks"
  done
  if command -v magick >/dev/null; then
    IM=$(command -v magick)
    MONTAGE=("$IM" montage)
  elif command -v convert >/dev/null && command -v montage >/dev/null; then
    IM=$(command -v convert)
    MONTAGE=("$(command -v montage)")
  fi
  [ -n "$IM" ] || refuse "ImageMagick's magick (or convert and montage) is required to compare the captures"
  [ -n "$DISPLAY_TARGET" ] || refuse "no --display: the clicks need a display of their own; pass --display :N (a private Xvfb), or --state-only"
  target_number=$(display_number "$DISPLAY_TARGET")
  [ -n "$target_number" ] || refuse "--display takes a local X display such as :97"
  [ "$target_number" != "$(display_number "${DISPLAY:-}")" ] \
    || refuse "--display is this session's own \$DISPLAY: start a private display and pass it"
  ps -u "$(id -u)" -o args= | python3 -c 'import re, sys; n = sys.argv[1]; sys.exit(0 if any(re.search(r"(^|/)Xvfb :" + n + r"( |$)", line) for line in sys.stdin) else 1)' "$target_number" \
    || refuse "no Xvfb :$target_number is running as this user: start a private one (Xvfb :$target_number -screen 0 1720x1000x24 &)"
  DISPLAY_TARGET=":$target_number"
  geometry=$(DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool getdisplaygeometry 2>/dev/null || true)
  [ -n "$geometry" ] || refuse "no X server answers on the display"
  { [ "${geometry% *}" -ge "$WINDOW_W" ] && [ "${geometry#* }" -ge "$WINDOW_H" ]; } \
    || refuse "the display is ${geometry% *}x${geometry#* }, smaller than the ${WINDOW_W}x${WINDOW_H} window: start a larger Xvfb"
  [ -n "$APPEARANCE" ] || APPEARANCE=dark
fi

[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/font-setting-e2e-$(date +%Y%m%d-%H%M%S)-$$"
if [ -d "$OUT_DIR" ] && [ -n "$(ls -A "$OUT_DIR")" ]; then
  refuse "the --out-dir already has content (pass a new one)"
fi
mkdir -p "$OUT_DIR/frames" "$OUT_DIR/reads" "$OUT_DIR/crops"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

STEP=setup
fail() { echo "FAIL: $STEP: $*" >&2; exit 1; }
trap 'echo "FAIL: $STEP: a command failed (exit $?, line $LINENO)" >&2' ERR
step() { # n text
  STEP="step $1"
  echo "step $1: $2"
}
[ "$(uname -s)" = Linux ] || fail "this E2E runs on Linux"
command -v git >/dev/null || fail "git is required"
command -v python3 >/dev/null || fail "python3 is required"
echo "display: ${DISPLAY_TARGET:-none (--state-only)}; appearance: ${APPEARANCE:-default}"
TAG="${APPEARANCE:-default}"

# The seed repository reads no git configuration of the user's.
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1

# ---- the run's own directory and what must be stopped ---------------------
RUN_DIR=$(cd "$(mktemp -d "${TMPDIR:-/tmp}/sirio-font-XXXXXX")" && pwd)
APP_PID=""
run_processes() {
  python3 -I - "$RUN_DIR" <<'PY'
import os, sys
marker = b"SIRIO_FONT_E2E_RUN=" + sys.argv[1].encode() + b"\0"
for entry in os.listdir("/proc"):
    if not entry.isdigit() or int(entry) == os.getpid():
        continue
    try:
        environ = open(f"/proc/{entry}/environ", "rb").read()
    except OSError:
        continue
    if marker in environ:
        print(entry)
PY
}
stop_run_processes() {
  local pids _
  for _ in $(seq 1 15); do
    pids=$(run_processes)
    [ -n "$pids" ] || return 0
    # shellcheck disable=SC2086 # one pid per word
    kill $pids 2>/dev/null || true
    sleep 0.2
  done
  # shellcheck disable=SC2086 # one pid per word
  kill -9 $(run_processes) 2>/dev/null || true
}
cleanup() {
  [ -z "$APP_PID" ] || { kill "$APP_PID" 2>/dev/null || true; wait "$APP_PID" 2>/dev/null || true; }
  stop_run_processes
  cp "$RUN_DIR"/*.log "$OUT_DIR/" 2>/dev/null || true
  rm -rf --one-file-system "$RUN_DIR"
}
trap cleanup EXIT

echo "building sirio and sirioctl"
(cd "$ROOT/rust" && cargo build --quiet -p sirio --bin sirio && cargo build --quiet -p sirio_control --bin sirioctl) \
  2>>"$RUN_DIR/build.log" || { tail -n 40 "$RUN_DIR/build.log" >&2; fail "the build failed"; }
dirty=''
git -C "$ROOT" diff --quiet HEAD -- || dirty=' (with uncommitted changes)'
echo "commit: $(git -C "$ROOT" rev-parse HEAD)$dirty"
echo "binary sha256 $(sha256sum "$BIN" | cut -d' ' -f1)"

# A copy of the app with no sibling sirio-host.
mkdir -p "$RUN_DIR/alone"
cp "$BIN" "$RUN_DIR/alone/sirio"
APP_BIN="$RUN_DIR/alone/sirio"

# ---- the project: a source file, and a ledger of 12,400 lines with one line inserted ----
WT="$RUN_DIR/repo"
mkdir -p "$WT/src" "$WT/data"
git -C "$WT" init -q -b main
git -C "$WT" config user.email tester@example.invalid
git -C "$WT" config user.name Tester
python3 -I - "$WT" <<'PY'
import sys
root = sys.argv[1]
lines = ["use std::collections::HashMap;\n", "\n"]
for n in range(1, 9):
    lines += [
        f"/// Totals of group {n}, read from `lines` and summed per label.\n",
        f"fn totals_{n}(lines: &[&str]) -> HashMap<String, u64> {{\n",
        "    let mut map = HashMap::new();\n",
        "    for line in lines {\n",
        "        let (label, value) = line.split_once('=').unwrap_or((line, \"0\"));\n",
        f"        *map.entry(label.trim().to_string()).or_insert({n}) += value.trim().parse::<u64>().unwrap_or(0);\n",
        "    }\n",
        "    map // 0O 1lI {}[]() -> != == >= <=\n",
        "}\n",
        "\n",
    ]
open(f"{root}/src/main.rs", "w").write("".join(lines))
open(f"{root}/data/ledger.txt", "w").write("".join(f"ledger row {n:05d}: amount = {n % 997}.{n % 89:02d}\n" for n in range(1, 12401)))
open(f"{root}/README.md", "w").write("# ledger\n")
PY
git -C "$WT" add -A
git -C "$WT" commit -q -m first
python3 -I - "$WT" <<'PY'
import sys
path = sys.argv[1] + "/data/ledger.txt"
lines = open(path).read().splitlines(keepends=True)
assert len(lines) == 12400
# One line inserted: it is the new line 12,345, the first changed row drawn (the unchanged
# lines around it fold into bands), and its new-number cell holds five digits.
lines.insert(12344, "ledger row 12345: amount = 99999.99 inserted\n")
open(path, "w").write("".join(lines))
PY
# The probed row has a five-digit new number by construction: the hunk starts at line 12,342.
HUNK_HEADER=$(git -C "$WT" diff -U3 -- data/ledger.txt | grep -m1 '^@@' | sed 's/ @@.*/ @@/')
[ "$HUNK_HEADER" = "@@ -12342,6 +12342,7 @@" ] || fail "the ledger's changed hunk does not start at line 12342"
echo "OK: the ledger's hunk header is $HUNK_HEADER (every probed number has five digits)"

# ---- stubs ---------------------------------------------------------------
mkdir -p "$RUN_DIR/bin"
for name in xdg-open browser-stub; do
  printf '#!/bin/sh\nprintf "%%s\\n" "%s $*" >>"%s/browser.log"\n' "$name" "$RUN_DIR" >"$RUN_DIR/bin/$name"
  chmod +x "$RUN_DIR/bin/$name"
done
STUBS="$ROOT/Scripts/Tests/stubs/font-sheet"
APP_PATH="$STUBS:$ROOT/Scripts/Tests/stubs/notify-send:$RUN_DIR/bin:/usr/bin:/bin"
[ "$(PATH="$APP_PATH" command -v claude || true)" = "$STUBS/claude" ] || fail "claude does not resolve to the font-sheet stub under the app's PATH"
echo "OK: claude resolves to the font-sheet stub"

# ---- calling the app -----------------------------------------------------
export SIRIO_SOCKET="$RUN_DIR/c.sock"
export SIRIO_DB="$RUN_DIR/session.sqlite"
[ "${#SIRIO_SOCKET}" -lt 108 ] || fail "the control socket path is too long for a unix socket (point TMPDIR at a shorter directory)"
CALL_TIMEOUT=30
ctl() {
  local status=0
  echo "+ sirioctl $*"
  timeout -k 5 "$CALL_TIMEOUT" "$CTL" "$@" || status=$?
  echo "  -> sirioctl exit $status"
  return "$status"
}
tsv_row() { # case key expected drawn ok
  printf '%s\t%s\t%s\t%s\t%s\n' "$@" >>"$OUT_DIR/measurements.tsv"
  printf '  %-4s  %s: expected %s, drawn %s\n' "$5" "$2" "$3" "$4"
}
printf 'case\tkey\texpected\tdrawn\tok\n' >"$OUT_DIR/measurements.tsv"

# The reads: the control socket's line-delimited JSON, spoken directly.
cat >"$RUN_DIR/fs.py" <<'PY'
import glob, json, os, re, socket, sqlite3, subprocess, sys, time

TSV = os.environ["FS_TSV"]
READS = os.environ["FS_READS"]
ASSETS = os.environ["FS_ASSETS"]
HELPER = os.environ["FS_HELPER"]
IM = os.environ.get("FS_IM", "")

# id: (ui family, code family, directory of its files under assets/fonts, bundles a ui italic)
PAIRS = {
    "atkinson": ("Atkinson Hyperlegible Next", "Atkinson Hyperlegible Mono", "atkinson", True),
    "inter": ("Inter", "JetBrains Mono", "inter", True),
    "plex": ("IBM Plex Sans", "IBM Plex Mono", "plex", True),
    "redhat": ("Red Hat Text", "Red Hat Mono", "redhat", False),
    "recursive": ("Recursive Sans Linear Static", "Recursive Mono Linear Static", "recursive", True),
    "figtree": ("Figtree", "Fira Code", "figtree", True),
    "geist": ("Geist", "Geist Mono", ".", False),
}


def rpc(method, params=None):
    s = socket.socket(socket.AF_UNIX)
    s.settimeout(30)
    s.connect(os.environ["SIRIO_SOCKET"])
    s.sendall((json.dumps({"id": "fs", "method": method, "params": params or {}}) + "\n").encode())
    buf = b""
    while not buf.endswith(b"\n"):
        chunk = s.recv(1 << 20)
        if not chunk:
            break
        buf += chunk
    s.close()
    response = json.loads(buf)
    if not response.get("ok"):
        raise RuntimeError(response.get("error") or method)
    return response.get("result") or {}


def fmt(v):
    return f"{v:g}" if isinstance(v, (int, float)) and not isinstance(v, bool) else str(v)


def row(case, key, expected, drawn, ok):
    with open(TSV, "a") as f:
        f.write("\t".join([case, key, str(expected), str(drawn), ok]) + "\n")
    print(f"  {ok:4}  {key}: expected {expected}, drawn {drawn}")


def fonts_read():
    return json.loads(rpc("surface.attention.read")["fonts"])


def rects():
    return json.loads(rpc("surface.layout.read")["shellRects"])


def helper(directory, family, weight, italic):
    args = [sys.executable, "-I", HELPER, "--dir", os.path.join(ASSETS, directory), "--family", family, "--weight", str(weight)]
    if italic:
        args.append("--italic")
    out = subprocess.run(args, capture_output=True, text=True, timeout=30)
    if out.returncode != 0:
        print(f"  the face-advance helper failed: {out.stderr.strip()}", file=sys.stderr)
        sys.exit(1)
    return json.loads(out.stdout)


def poll_rect(key, stable, seconds=25):
    deadline = time.time() + seconds
    previous = None
    last_error = None
    while time.time() < deadline:
        try:
            r = rects().get(key)
        except Exception as e:  # not yet
            last_error = repr(e)
            r = None
        if r is not None and (not stable or r == previous):
            return r
        previous = r
        time.sleep(0.4 if stable else 0.3)
    sys.exit(f"{key} was not drawn within {seconds} s ({last_error})")


command = sys.argv[1]
if command == "rect":
    # rect KEY [stable]: "x y w h" of a drawn rect (stable: equal over two reads 0.4 s apart).
    print(" ".join(fmt(v) for v in poll_rect(sys.argv[2], len(sys.argv) > 3 and sys.argv[3] == "stable")))
elif command == "rect-gone":
    # rect-gone KEY SECONDS: waits for a rect to be null (not drawn).
    deadline = time.time() + float(sys.argv[3])
    while time.time() < deadline:
        try:
            if rects().get(sys.argv[2]) is None:
                sys.exit(0)
        except Exception:
            pass
        time.sleep(0.3)
    sys.exit(f"{sys.argv[2]} was still drawn")
elif command == "pane":
    # pane [primary|secondary|X]: "x y w h" of the content (below the tab strip) of the primary
    # pane, of the named one, or of the one that holds window x.
    deadline = time.time() + 25
    while time.time() < deadline:
        try:
            r = rects()
            which = "primary"
            if len(sys.argv) > 2 and sys.argv[2] in ("primary", "secondary"):
                which = sys.argv[2]
            elif len(sys.argv) > 2 and r.get("secondary") and r["secondary"][0] <= float(sys.argv[2]) <= r["secondary"][0] + r["secondary"][2]:
                which = "secondary"
            pane, strip = r.get(which), r.get(which + "Strip")
            if pane and strip:
                top = max(pane[1], strip[1] + strip[3])
                print(" ".join(fmt(v) for v in (pane[0], top, pane[2], pane[1] + pane[3] - top)))
                sys.exit(0)
        except Exception:
            pass
        time.sleep(0.3)
    sys.exit("the primary pane was not drawn")
elif command == "settings":
    # settings open|closed SECONDS [CASE LABEL]: waits for `surface settings read` to answer (open)
    # or fail (closed); with CASE and LABEL, the state the read really gave is a row.
    want_open, deadline = sys.argv[2] == "open", time.time() + float(sys.argv[3])
    while True:
        try:
            rpc("surface.settings.read")
            is_open = True
        except Exception:
            is_open = False
        if is_open == want_open or time.time() >= deadline:
            break
        time.sleep(0.3)
    if len(sys.argv) > 5:
        row(sys.argv[4], sys.argv[5], sys.argv[2], "open" if is_open else "closed", "ok" if is_open == want_open else "FAIL")
    sys.exit(0 if is_open == want_open else f"settings did not become {sys.argv[2]}")
elif command == "settings-get":
    print(rpc("surface.settings.read")[sys.argv[2]])
elif command == "settings-wait":
    # settings-wait KEY VALUE SECONDS [CASE]: with CASE, the value the read gave is a row.
    deadline, got = time.time() + float(sys.argv[4]), None
    while True:
        try:
            got = rpc("surface.settings.read").get(sys.argv[2])
        except Exception as e:
            got = repr(e)
        if got == sys.argv[3] or time.time() >= deadline:
            break
        time.sleep(0.3)
    if len(sys.argv) > 5:
        row(sys.argv[5], f"settings read {sys.argv[2]}", sys.argv[3], got, "ok" if got == sys.argv[3] else "FAIL")
    sys.exit(0 if got == sys.argv[3] else f"settings {sys.argv[2]} is '{got}', not '{sys.argv[3]}'")
elif command == "tabs":
    t = rpc("surface.tabs.read")
    print(t["count"], t["active"])
    for key, value in sorted(t.items()):
        if key.startswith("tab."):
            kind, _, title = value.split("|", 2)
            print(key.split(".")[1], kind, title)
elif command == "tab-of":
    # tab-of KIND: the position of the first tab of this kind.
    t = rpc("surface.tabs.read")
    found = sorted(int(k.split(".")[1]) for k, v in t.items() if k.startswith("tab.") and v.split("|")[0] == sys.argv[2])
    if not found:
        sys.exit(f"no {sys.argv[2]} tab")
    print(found[0])
elif command == "tab-positions":
    t = rpc("surface.tabs.read")
    kinds = sys.argv[2:]
    print(*sorted((int(k.split(".")[1]) for k, v in t.items() if k.startswith("tab.") and v.split("|")[0] in kinds), reverse=True))
elif command == "chat-open":
    print(rpc("surface.chat.open")["surfaceId"])
elif command == "chat-send":
    rpc("surface.chat.send", {"surfaceId": sys.argv[2], "text": sys.argv[3]})
elif command == "chat-wait":
    # chat-wait SURFACE STATUS... SECONDS(last): until the chat's status is one of STATUS.
    surface, wanted, deadline = sys.argv[2], sys.argv[3:-1], time.time() + float(sys.argv[-1])
    status = None
    while time.time() < deadline:
        try:
            result = rpc("surface.chat.read", {"surfaceId": surface})
            status = result.get("status")
            if "answered" in wanted and status == "completed" and "fixed answer" in (result.get("transcript") or ""):
                sys.exit(0)
            if status in wanted:
                sys.exit(0)
        except Exception as e:
            status = repr(e)
        time.sleep(0.4)
    sys.exit(f"the chat's status is '{status}', not one of {wanted}")
elif command == "check-fonts":
    # check-fonts CASE PAIR [widths|first]: the fonts read names the pair's families; with
    # `widths`, every width matches the bundled file's; `first` takes the first answer.
    case, pair = sys.argv[2], sys.argv[3]
    mode = sys.argv[4] if len(sys.argv) > 4 else ""
    ui, code, directory, italic = PAIRS[pair]
    deadline = time.time() + 30
    f, error = None, None
    while True:
        try:
            f = fonts_read()
            if mode == "first" or f["pair"] == pair:
                break
        except Exception as e:
            error = repr(e)
        if time.time() > deadline:
            row(case, "fonts read names the pair", pair, f"{f and f.get('pair')} within 30 s; error {error}", "FAIL")
            sys.exit(1)
        time.sleep(0.3)
    if mode == "widths" and not f.get("widths"):
        print("  the fonts read carries no widths")
        sys.exit(3)
    with open(os.path.join(READS, f"fonts-{case}.json"), "w") as out:
        json.dump(f, out, indent=1, sort_keys=True)
    bad = False

    def check(key, expected, drawn):
        global bad
        ok = expected == drawn
        bad = bad or not ok
        row(case, key, expected, drawn, "ok" if ok else "FAIL")

    check("pair", pair, f["pair"])
    check("typography ui family", ui, f["ui"])
    check("typography code family", code, f["code"])
    check("ely ui family", ui, f["elyUi"])
    check("ely code family", code, f["elyCode"])
    check("bezel sans family", ui, f["bezelSans"])
    check("bezel mono family", code, f["bezelMono"])
    if italic:
        check("bezel sans fallback (the pair bundles an italic)", ui, f["bezelSansFallback"])
    else:
        # Red Hat and Geist: bezel's default (the system face); compared across the two later.
        row(case, "bezel sans fallback (no bundled italic: the system face)", "recorded", f["bezelSansFallback"], "ok")
    if mode == "widths":
        slots = [
            ("ui", "400", 400, False, ui), ("ui", "500", 500, False, ui), ("ui", "600", 600, False, ui),
            ("ui", "700", 700, False, ui), ("ui", "400i", 400, True, ui),
            ("code", "400", 400, False, code), ("code", "700", 700, False, code),
        ]
        faces = {}
        for role, slot, weight, is_italic, family in slots:
            drawn = f["widths"][role][slot]
            h = helper(directory, family, weight, is_italic)
            faces[(role, slot)] = h
            if h["status"] == "skip":
                if is_italic and italic:
                    bad = True
                    row(case, f"width {role} {slot}: the bundled italic file is found", "found", "none found", "FAIL")
                else:
                    row(case, f"width {role} {slot} (the pair bundles no italic)", "no file to compare with", "skipped", "skip")
                continue
            for char in "ma0":
                want, got = h["samples"][char]["width"], drawn[char]
                ok = abs(got - want) <= 0.5
                bad = bad or not ok
                row(case, f"width {role} {slot} of '{char}' x10 within 0.5 px of {h['file']}", fmt(want), fmt(got), "ok" if ok else "FAIL")

        def shown(h):
            return ", ".join(f"{c} {h['samples'][c]['width']:g}" for c in "ma0")

        def differs(a, b):
            return any(abs(a["samples"][c]["width"] - b["samples"][c]["width"]) > 1e-6 for c in "ma0")

        # Falsifiability, per pair: can the width tell the weight (400 vs 700), and the italic?
        for role in ("ui", "code"):
            a, b = faces[(role, "400")], faces[(role, "700")]
            if differs(a, b):
                row(case, f"weight {role}: at least one sample's 400 and 700 advances differ", "differ", f"400: {shown(a)}; 700: {shown(b)}", "ok")
            else:
                row(case, f"weight-limit {role}: no sample's 400 and 700 advances differ, the width cannot tell the weight",
                    "equal", f"400: {shown(a)}; 700: {shown(b)}", "limit")
        if italic:
            a, b = faces[("ui", "400")], faces[("ui", "400i")]
            if differs(a, b):
                row(case, "italic ui: at least one sample's 400 and 400i advances differ", "differ", f"400: {shown(a)}; 400i: {shown(b)}", "ok")
            else:
                row(case, "italic-limit ui: no sample's 400 and 400i advances differ, the width cannot tell the italic",
                    "equal", f"400: {shown(a)}; 400i: {shown(b)}", "limit")
        for slot in ("400", "700"):
            h = faces[("code", slot)]
            detail = f"0.600 em passes {h['passes_0600']}, 0.602 em passes {h['passes_0602']}"
            if h["passes_0600"] or h["passes_0602"]:
                row(case, f"width-limit code {slot} ({code}, em {h['em']})",
                    "a fallback with the same advance passes within 0.5 px: the width proves neither the weight nor the family", detail, "limit")
            else:
                row(case, f"width code {slot} ({code}, em {h['em']}): no 0.600 or 0.602 em fallback passes",
                    "the width tells this face apart", detail, "ok")
    if bad:
        print(f"  last read: {json.dumps(f)}", file=sys.stderr)
        sys.exit(1)
elif command == "fallback-check":
    # Red Hat and Geist bundle no italic: bezel's sans fallback is the same system face.
    a = json.load(open(os.path.join(READS, "fonts-pair-redhat.json")))["bezelSansFallback"]
    b = json.load(open(os.path.join(READS, "fonts-pair-geist.json")))["bezelSansFallback"]
    row("pairs", "bezel sans fallback of Red Hat equals Geist's", b, a, "ok" if a == b else "FAIL")
    sys.exit(0 if a == b else 1)
elif command == "changes-cell":
    # changes-cell: "x y w h size" of the new-number cell and the px its numbers are drawn at, once
    # the unified diff is drawn and stable.
    deadline = time.time() + 40
    previous, last = None, None
    while time.time() < deadline:
        try:
            result = rpc("surface.changes.read")
            cell = json.loads(result.get("numberCell") or "null")
            last = (result.get("ready"), result.get("mode"), cell)
            if result.get("ready") == "true" and result.get("mode") == "unified" and cell is not None and cell == previous:
                print(" ".join(fmt(v) for v in cell), result.get("numberFontSize"))
                sys.exit(0)
            previous = cell
        except Exception as e:
            last = repr(e)
            previous = None
        time.sleep(0.4)
    sys.exit(f"the diff's number cell was not drawn: {last}")
elif command == "db-get":
    c = sqlite3.connect(os.environ["SIRIO_DB"])
    r = c.execute("SELECT value FROM setting WHERE key = ?", (sys.argv[2],)).fetchone()
    print(r[0] if r else "")
elif command == "db-set":
    c = sqlite3.connect(os.environ["SIRIO_DB"])
    c.execute("INSERT INTO setting (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value", (sys.argv[2], sys.argv[3]))
    c.commit()
    c.close()
elif command == "db-del":
    c = sqlite3.connect(os.environ["SIRIO_DB"])
    c.execute("DELETE FROM setting WHERE key = ?", (sys.argv[2],))
    c.commit()
    c.close()
elif command == "db-wait":
    # db-wait KEY VALUE SECONDS CASE: the row is saved with this value (read straight from the
    # database, the app running); the value read is a row.
    deadline, got = time.time() + float(sys.argv[4]), None
    while True:
        try:
            c = sqlite3.connect(os.environ["SIRIO_DB"], timeout=5)
            r = c.execute("SELECT value FROM setting WHERE key = ?", (sys.argv[2],)).fetchone()
            c.close()
            got = r[0] if r else "(no row)"
        except Exception as e:
            got = repr(e)
        if got == sys.argv[3] or time.time() >= deadline:
            break
        time.sleep(0.3)
    row(sys.argv[5], f"saved database row {sys.argv[2]} right after the choice", sys.argv[3], got, "ok" if got == sys.argv[3] else "FAIL")
    sys.exit(0 if got == sys.argv[3] else 1)
elif command == "installed":
    # installed: each bundled family the system also lists is an installed-limit row.
    try:
        listing = subprocess.run(["fc-list", ":", "family"], capture_output=True, text=True, timeout=30).stdout
    except Exception:
        row("installed", "fc-list", "available", "unavailable", "skip")
        sys.exit(0)
    names = {n.strip().lower() for line in listing.splitlines() for n in line.split(",")}
    for family in sorted({fam for entry in PAIRS.values() for fam in entry[:2]}):
        if family.lower() in names:
            row("installed", f"installed-limit {family}", "not installed", "installed on this machine: its system copy may be drawn instead of the bundled file", "limit")
        else:
            row("installed", f"{family} is not installed", "not installed", "not installed: the bundled face is the one drawn", "ok")
elif command == "reg-time":
    # reg-time LOG CASE: the startup registration line, as a row.
    m = None
    for line in open(sys.argv[2], errors="replace"):
        m = re.search(r"\[fonts\] registered (\d+) interface faces in (\S+)", line) or m
        if re.search(r"\[fonts\] failed to register", line):
            row(sys.argv[3], "no pair failed to register", "none", line.strip()[:120], "FAIL")
            sys.exit(1)
    if not m:
        row(sys.argv[3], "startup registration line", "[fonts] registered N interface faces in T", "absent", "FAIL")
        sys.exit(1)
    six = ["atkinson", "inter", "plex", "redhat", "recursive", "figtree"]
    bundled = len(glob.glob(os.path.join(ASSETS, "Geist*.ttf"))) + sum(len(glob.glob(os.path.join(ASSETS, d, "*.ttf"))) for d in six)
    row(sys.argv[3], "registered interface faces equal the bundled interface TTFs (Geist's files and the six pairs')", bundled, m.group(1), "ok" if int(m.group(1)) == bundled else "FAIL")
    row(sys.argv[3], "startup registration time (debug build)", f"{m.group(1)} faces registered", m.group(2), "ok")
    sys.exit(0 if int(m.group(1)) == bundled else 1)
elif command == "ink-check":
    # ink-check CROP.png SIZE CASE: the probed number cell holds digit ink, drawn five digits wide.
    px = {}
    out = subprocess.run([IM, sys.argv[2], "txt:-"], capture_output=True, text=True, timeout=30).stdout
    for line in out.splitlines():
        m = re.match(r"^(\d+),(\d+): \((\d+),(\d+),(\d+)", line)
        if m:
            px[(int(m.group(1)), int(m.group(2)))] = m.groups()[2:5]
    width, height = max(x for x, _ in px) + 1, max(y for _, y in px) + 1
    ground = px[(0, 0)]
    columns = [x for x in range(width) if any(px[(x, y)] != ground for y in range(height))]
    colours = len(set(px.values()))
    case, size = sys.argv[4], float(sys.argv[3])
    mono = helper("atkinson", "Atkinson Hyperlegible Mono", 400, False)
    advance = mono["samples"]["0"]["advance"] / mono["upem"] * size
    need = 4.1 * advance
    bad = False
    ok = colours > 1
    bad = bad or not ok
    row(case, "the probed cell holds digit ink (its crop has more than one colour)", "> 1", colours, "ok" if ok else "FAIL")
    edge = width - 1 - max(columns) if columns else None
    ok = edge is not None and edge <= 3
    bad = bad or not ok
    row(case, "the cell's rightmost ink column is within 3 px of its right edge", "<= 3", edge, "ok" if ok else "FAIL")
    extent = max(columns) - min(columns) + 1 if columns else 0
    ok = extent >= need
    bad = bad or not ok
    row(case, f"the cell's ink extent covers five digits (>= 4.1 x the '0' advance {advance:.2f} px at {size:g} px)", f">= {need:.2f}", extent, "ok" if ok else "FAIL")
    sys.exit(1 if bad else 0)
elif command == "gutter":
    # gutter FRAME PX PY: where the editor's code starts, in px right of the pane's left edge: the
    # start of the second cluster of ink along line 1 (the line number is the first).
    out = subprocess.run([IM, sys.argv[2], "-crop", f"240x12+{round(float(sys.argv[3]))}+{round(float(sys.argv[4])) + 34}", "+repage", "txt:-"],
                         capture_output=True, text=True, timeout=30).stdout
    px = {}
    for line in out.splitlines():
        m = re.match(r"^(\d+),(\d+): \((\d+),(\d+),(\d+)", line)
        if m:
            px[(int(m.group(1)), int(m.group(2)))] = m.groups()[2:5]
    counts = {}
    for colour in px.values():
        counts[colour] = counts.get(colour, 0) + 1
    ground = max(counts, key=counts.get)
    ink = sorted({x for (x, _), colour in px.items() if colour != ground})
    clusters, last = [], None
    for x in ink:
        if last is None or x - last >= 12:
            clusters.append(x)
        last = x
    if len(clusters) < 2:
        sys.exit("the editor's gutter and code were not told apart on line 1")
    print(clusters[1])
elif command == "attn-save":
    A = rpc("surface.attention.read")
    json.dump({"fonts": json.loads(A["fonts"])}, open(sys.argv[2], "w"), indent=2)
else:
    sys.exit(f"unknown command {command}")
PY
export FS_TSV="$OUT_DIR/measurements.tsv" FS_READS="$OUT_DIR/reads" FS_ASSETS="$ASSETS" FS_HELPER="$HELPER" FS_IM="$IM"
FS() { python3 -I "$RUN_DIR/fs.py" "$@"; }

# ---- launching an isolated Sirio -----------------------------------------
seed_appearance() {
  [ -z "$APPEARANCE" ] || (cd "$ROOT/rust" && cargo run --quiet -p sirio_persistence --example appearance_seed -- --database "$SIRIO_DB" --appearance "$APPEARANCE") \
    || fail "could not seed the appearance"
  # The seed saves the whole settings, appearance.fontPair (atkinson) among them: a fresh
  # database has no such row, so it goes.
  [ -z "$APPEARANCE" ] || FS db-del appearance.fontPair || fail "could not remove the seeded fontPair row"
}
VULKAN_ENV=()
if [ "$STATE_ONLY" -eq 0 ]; then
  LVP=$(find /usr/share/vulkan/icd.d -name 'lvp_icd*.json' 2>/dev/null | head -n 1 || true)
  [ -n "$LVP" ] || fail "software Vulkan (lvp_icd*.json in /usr/share/vulkan/icd.d) is required for a framed run on Xvfb; install vulkan-swrast or pass --state-only"
  VULKAN_ENV=("VK_DRIVER_FILES=$LVP" "VK_ICD_FILENAMES=$LVP")
fi
mkdir -p "$RUN_DIR/home" "$RUN_DIR/xdg/config" "$RUN_DIR/xdg/data" "$RUN_DIR/xdg/state" "$RUN_DIR/xdg/cache"
mkdir -m 700 "$RUN_DIR/xdg/runtime"
# The language server for .rs is the fake one (Scripts/Tests/fake_lsp.py, answering nothing: no
# findings, no hover), never a real rust-analyzer: a machine's own may fail to start, and the
# popup "language server transport failed" would then cover the code in the editor frames. No
# Sirio setting turns language servers off, so the rust entry of languages.toml names a stub
# on the app's PATH, as test-editor-e2e.sh does.
mkdir -p "$RUN_DIR/xdg/config/sirio" "$RUN_DIR/fake-lsp"
echo '{}' >"$RUN_DIR/fake-lsp/answers.json"
printf '#!/bin/sh\nexec python3 -I "%s/Scripts/Tests/fake_lsp.py" "$@"\n' "$ROOT" >"$RUN_DIR/bin/fake-lsp"
chmod +x "$RUN_DIR/bin/fake-lsp"
cat >"$RUN_DIR/xdg/config/sirio/languages.toml" <<TOML
[[language]]
name = "rust"
extensions = ["rs"]
command = "fake-lsp"
args = ["$RUN_DIR/fake-lsp/answers.json"]
TOML
# env -i and this allowlist, so nothing of the user's reaches the app.
APP_ENV=(
  "PATH=$APP_PATH" "HOME=$RUN_DIR/home" "SHELL=/bin/bash" "TERM=xterm-256color" "LANG=C.UTF-8"
  "XDG_CONFIG_HOME=$RUN_DIR/xdg/config" "XDG_DATA_HOME=$RUN_DIR/xdg/data"
  "XDG_STATE_HOME=$RUN_DIR/xdg/state" "XDG_CACHE_HOME=$RUN_DIR/xdg/cache" "XDG_RUNTIME_DIR=$RUN_DIR/xdg/runtime"
  "BROWSER=$RUN_DIR/bin/browser-stub"
  "SIRIO_SOCKET=$SIRIO_SOCKET" "SIRIO_DB=$SIRIO_DB" "SIRIO_CREDENTIALS=$RUN_DIR/credentials.json"
  "SIRIO_HOST_BIN=$RUN_DIR/no-sirio-host" "SIRIO_STUB_LOG=$RUN_DIR/stub-calls.log"
  "SIRIO_FONT_E2E_RUN=$RUN_DIR"
  "DBUS_SESSION_BUS_ADDRESS=unix:path=$RUN_DIR/no-bus"
)
if [ "$STATE_ONLY" -eq 0 ]; then
  APP_ENV+=("DISPLAY=$DISPLAY_TARGET" "GPUI_X11_SCALE_FACTOR=1" "${VULKAN_ENV[@]}")
fi

find_window() {
  DISPLAY_TARGET="$DISPLAY_TARGET" APP_PID="$APP_PID" python3 -I - <<'PY'
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
app_window() {
  local window="" _
  for _ in $(seq 1 50); do
    window=$(find_window)
    [ -n "$window" ] && break
    sleep 0.2
  done
  [ -n "$window" ] || fail "no window with _NET_WM_PID=$APP_PID"
  echo "$window"
}
launch_app() {
  rm -f "$SIRIO_SOCKET"
  (cd "$WT" && exec env -i "${APP_ENV[@]}" "$APP_BIN" >>"$RUN_DIR/app.log" 2>&1) &
  APP_PID=$!
  for _ in $(seq 1 75); do
    [ -S "$SIRIO_SOCKET" ] && break
    sleep 0.2
  done
  [ -S "$SIRIO_SOCKET" ] || fail "sirio never opened its control socket"
  if [ "$STATE_ONLY" -eq 0 ]; then
    local window
    window=$(app_window)
    DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool windowmove "$window" 0 0 windowsize "$window" "$WINDOW_W" "$WINDOW_H" \
      || fail "could not size the window to ${WINDOW_W}x${WINDOW_H}"
    local _
    for _ in $(seq 1 50); do
      DISPLAY="$DISPLAY_TARGET" timeout 5 xwininfo -id "$window" | grep -q 'Map State: IsViewable' && break
      sleep 0.2
    done
    DISPLAY="$DISPLAY_TARGET" timeout 10 xdotool windowfocus --sync "$window" || fail "could not focus the window"
    sleep 1
  fi
}
app_running() {
  local state
  state=$(ps -o stat= -p "$APP_PID" 2>/dev/null | tr -d ' ') || true
  [ -n "$state" ] && [ "${state#Z}" = "$state" ]
}
quit_app() {
  ctl quit >/dev/null || true
  for _ in $(seq 1 75); do
    app_running || break
    sleep 0.2
  done
  if app_running; then
    kill -9 "$APP_PID" 2>/dev/null || true
    fail "sirio did not quit"
  fi
  wait "$APP_PID" 2>/dev/null || true
  APP_PID=""
}

# ---- real X input at the pixels the app reports ---------------------------
window_origin() { # prints "X Y"
  DISPLAY="$DISPLAY_TARGET" timeout 5 xwininfo -id "$(app_window)" | awk '/Absolute upper-left X/ {x=$NF} /Absolute upper-left Y/ {y=$NF} END {print x, y}'
}
screen_xy() { # "x y" (window pixels) -> "X Y" (screen)
  local origin
  origin=$(window_origin)
  echo "$(( ${1% *} + ${origin% *} )) $(( ${1#* } + ${origin#* } ))"
}
# A press and a release 0.3 s apart, as a hand's are: this window paints in software.
click_window() { # "x y" (window pixels)
  local s
  s=$(screen_xy "$1")
  echo "  click: window pixel $1"
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool mousemove "${s% *}" "${s#* }" mousedown 1 sleep 0.3 mouseup 1 \
    || fail "xdotool could not click"
  sleep 1
}
# The pointer to a window pixel, in two moves (a hover needs a motion after the first).
move_window() { # "x y"
  local s x y
  s=$(screen_xy "$1")
  x=${s% *}
  y=${s#* }
  echo "  pointer: window pixel $1"
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool mousemove "$((x - 2))" "$((y - 2))" sleep 0.15 mousemove "$x" "$y" \
    || fail "xdotool could not move the pointer"
}
rect_center() { # "x y w h" -> "cx cy" plus optional dx dy
  python3 -I -c 'import sys
x, y, w, h = (float(v) for v in sys.argv[1:5])
dx, dy = (float(v) for v in sys.argv[5:7])
print(round(x + w / 2 + dx), round(y + h / 2 + dy))' "$@"
}
click_rect() { # key [dx dy]: the centre of the rect, moved by dx dy
  click_window "$(rect_center $(FS rect "$1") "${2:-0}" "${3:-0}")"
}
# Parks the pointer on the status bar: off the sidebar, the tabs, the page and the diff.
park_pointer() {
  move_window "$(rect_center $(FS rect status) 0 0)"
  sleep 0.5
}
press_key() { # key
  echo "  key: $1"
  DISPLAY="$DISPLAY_TARGET" timeout 5 xdotool key --clearmodifiers "$1" || fail "xdotool could not press $1"
  sleep 1
}

# ---- captures --------------------------------------------------------------
capture_now() { # name: no wait
  local window colours name="$1-$TAG"
  window=$(app_window)
  timeout -k 5 20 import -display "$DISPLAY_TARGET" -window "$window" "$OUT_DIR/frames/$name.png" || fail "the capture of $name failed"
  colours=$(identify -format '%k' "$OUT_DIR/frames/$name.png")
  [ "$colours" -ge 200 ] || fail "$name is blank ($colours colours)"
  echo "FRAME: $name ($colours colours; pid $APP_PID)"
}
capture() { # name
  sleep 2
  capture_now "$1"
}
# A crop of a frame of this run as its own PNG; its pixel signature is what two crops are
# compared by (equal iff every pixel is).
crop_frame() { # frame crop-name x y w h
  local spec
  spec=$(python3 -I -c 'import sys
x, y, w, h = (float(v) for v in sys.argv[1:5])
print("%dx%d+%d+%d" % (max(1, round(w)), max(1, round(h)), max(0, round(x)), max(0, round(y))))' "$3" "$4" "$5" "$6")
  "$IM" "$OUT_DIR/frames/$1-$TAG.png" -crop "$spec" +repage "$OUT_DIR/crops/$2-$TAG.png" || fail "could not crop $1"
}
crop_sig() { # crop-name
  "$IM" "$OUT_DIR/crops/$1-$TAG.png" -format '%#' info: || fail "could not read the signature of $1"
}
expect_crops_differ() { # case key a b
  local sa sb
  sa=$(crop_sig "$3")
  sb=$(crop_sig "$4")
  if [ "$sa" != "$sb" ]; then
    tsv_row "$1" "$2" "$3 differs from $4" "differ" ok
  else
    tsv_row "$1" "$2" "$3 differs from $4" "identical" FAIL
    fail "$2: the crops $3 and $4 are identical"
  fi
}
expect_crops_equal() { # case key a b
  local sa sb
  sa=$(crop_sig "$3")
  sb=$(crop_sig "$4")
  if [ "$sa" = "$sb" ]; then
    tsv_row "$1" "$2" "$3 equals $4" "equal" ok
  else
    tsv_row "$1" "$2" "$3 equals $4" "differ" FAIL
    fail "$2: the crops $3 and $4 differ"
  fi
}

EDITOR_KIND=file # the kind `surface tabs read` gives an editor tab
# ---- the sheet's tabs and frames ---------------------------------------------
select_tab() { # kind
  local position
  position=$(FS tab-of "$1") || fail "there is no $1 tab to select"
  ctl surface tabs select "$position" >/dev/null || fail "could not select the $1 tab"
  sleep 1
}
# The crops compared across pairs, each rect taken fresh from the debug reads: the sidebar's
# New Chat row is interface text (no caret, spinner or relative time); the editor's lines
# 13 to 21 are code, below the caret's line.
interface_crop() { # frame crop-name
  crop_frame "$1" "$2" $(FS rect sidebarNewChat)
}
# The editor and the chat share the secondary pane (the terminal is the primary one's only tab,
# always drawn). The code crop is the code body only: lines 13 to 21 of the source, right of the
# line-number gutter (no rect reports the gutter's width, so it is measured in the frame: where
# the second cluster of ink along line 1 starts, after the line number) and left of the minimap.
code_crop() { # frame crop-name
  local pane x y w h gutter
  pane=$(FS pane secondary)
  read -r x y w h <<<"$pane"
  gutter=$(FS gutter "$OUT_DIR/frames/$1-$TAG.png" "$x" "$y") || fail "could not measure the editor's gutter"
  crop_frame "$1" "$2" $(python3 -I -c 'import sys
x, y, w, g = (float(v) for v in sys.argv[1:5])
print(x + g, y + 280, min(400, w - g - 100), 190)' "$x" "$y" "$w" "$gutter")
}
# The chat's reply, prose through the text block: 110 px below the secondary pane's content top
# (the "TODAY" line and the prompt bubble are above) and 450 tall, which leaves the clock line out
# in every pair (a wrapping pair pushes both down together).
chat_crop() { # frame crop-name
  crop_frame "$1" "$2" $(python3 -I -c 'import sys
x, y, w, h = (float(v) for v in sys.argv[1:5])
print(x + 4, y + 110, w - 8, 450)' $(FS pane secondary))
}
# The fixed line the terminal printed, its first row. Above it the terminal draws a notice
# (no sirio-host) in the interface face, whose height follows the pair: the row's top is where
# the notice's background ends, found in the column of pixels at the pane's left edge.
terminal_crop() { # frame crop-name
  local pane x y w h top
  pane=$(FS pane primary)
  read -r x y w h <<<"$pane"
  top=$("$IM" "$OUT_DIR/frames/$1-$TAG.png" -crop "1x$(python3 -I -c 'import sys; print(round(float(sys.argv[1])))' "$h")+$(python3 -I -c 'import sys; print(round(float(sys.argv[1])) + 2)' "$x")+$(python3 -I -c 'import sys; print(round(float(sys.argv[1])))' "$y")" +repage txt:- \
    | python3 -I -c 'import re, sys
rows = [tuple(int(v) for v in m.groups()[1:4]) for m in (re.match(r"^0,(\d+): \((\d+),(\d+),(\d+)", l) for l in sys.stdin) if m]
ground = rows[-6]
top = next((i for i, c in enumerate(rows) if c == ground), 0)
print(top)') || fail "could not find the terminal's first row"
  crop_frame "$1" "$2" "$x" "$(python3 -I -c 'import sys; print(float(sys.argv[1]) + float(sys.argv[2]))' "$y" "$top")" 520 24
}
LABEL_FONT=$(fc-match -f '%{file}' 'DejaVu Sans' 2>/dev/null || true)
# The sheet frame of a pair: the chat tab and the editor tab, stacked under a strip naming it.
sheet_frame() { # id title
  select_tab chat
  park_pointer
  capture "sheet-$1-chat"
  select_tab "$EDITOR_KIND"
  park_pointer
  capture "sheet-$1-editor"
  local font=()
  [ -z "$LABEL_FONT" ] || font=(-font "$LABEL_FONT")
  "$IM" -size "${WINDOW_W}x56" xc:'#202020' "${font[@]}" -fill white -pointsize 30 -gravity West \
    -annotate +16+0 "$2   (chat above, editor below; $TAG)" "$OUT_DIR/crops/label-$1-$TAG.png" \
    || fail "could not draw the label of $2"
  "$IM" "$OUT_DIR/crops/label-$1-$TAG.png" "$OUT_DIR/frames/sheet-$1-chat-$TAG.png" "$OUT_DIR/frames/sheet-$1-editor-$TAG.png" \
    -append "$OUT_DIR/frames/sheet-$1-$TAG.png" || fail "could not stack the sheet frame of $2"
  echo "FRAME: sheet-$1-$TAG (the chat and editor captures, stacked)"
}

# ---- the Settings page --------------------------------------------------------
# Opens Settings on Appearance and waits for the page: the Font trigger's rect non-null and
# equal over two reads.
open_appearance() {
  ctl surface settings open --section appearance >/dev/null || fail "could not open Settings on Appearance"
  FS rect settingsFont stable >/dev/null || fail "the Appearance page never drew the Font select"
  sleep 0.5
}
IDS=(atkinson inter plex redhat recursive figtree geist)
TITLES=("Atkinson Hyperlegible" "Inter + JetBrains Mono" "IBM Plex" "Red Hat" "Recursive" "Figtree + Fira Code" "Geist")
index_of() { # id
  local i
  for i in "${!IDS[@]}"; do
    [ "${IDS[$i]}" = "$1" ] && { echo "$i"; return 0; }
  done
  return 1
}
LIST_BORDER_PAD=5
ROW_LINE=20
ROW_PITCH=$((8 + ROW_LINE))
CURRENT=atkinson
# Chooses a pair in the open Settings page through the Font select, as a hand does.
choose_pair() { # id
  local want=$1 trigger from to n
  from=$(index_of "$CURRENT")
  to=$(index_of "$want")
  trigger=$(FS rect settingsFont stable)
  click_window "$(rect_center $trigger 0 0)"
  sleep 1
  if [ "$want" = figtree ]; then
    # Row k's centre: below the trigger, past the list's border and padding, k + 0.5 pitches down.
    click_window "$(python3 -I -c 'import sys
x, y, w, h = (float(v) for v in sys.argv[1:5])
k, pad, pitch = int(sys.argv[5]), float(sys.argv[6]), float(sys.argv[7])
print(round(x + w / 2), round(y + h + pad + (k + 0.5) * pitch))' $trigger "$to" "$LIST_BORDER_PAD" "$ROW_PITCH")"
  else
    n=$(( (to - from + 7) % 7 ))
    echo "  keys: Down x $n from ${CURRENT} to ${want}, then Enter"
    for _ in $(seq 1 "$n"); do press_key Down; done
    press_key Return
  fi
  CURRENT=$want
}

# ---- the diff step (8), also run alone by --diff-only --------------------------
# Changes on the ledger at interface size 18 in Atkinson: no digit ink left of the new-number
# cell. `diff_number_width` forced to 40 makes this fail (the proof run of the plan).
diff_numbers_step() {
  step 8 "at interface size 18 in Atkinson, no digit of a five-digit line number crosses its cell's left edge"
  # The preconditions, read before the capture: Settings reads size 18, and the pair is atkinson.
  # The window with Settings open is kept: at size 18 the Font title is drawn whole (evidence).
  ctl surface settings open --section appearance >/dev/null || fail "could not open Settings on Appearance"
  FS rect settingsFont stable >/dev/null || fail "the Appearance page never drew the Font select"
  sleep 0.5
  FS settings-wait interfaceFontSize 18 15 8 || fail "the interface font size is not 18"
  FS check-fonts size18 atkinson || fail "the pair is not atkinson at size 18"
  capture settings-font-18
  crop_frame settings-font-18 settings-font-18-trigger $(FS rect settingsFont)
  press_key Escape
  FS settings closed 15 8 "Settings closed again after the size read" || fail "Escape did not close Settings"
  ctl select-workspace --workspace "$WT" >/dev/null
  ctl surface changes open >/dev/null || fail "could not open Changes"
  ctl surface changes view --mode unified --expand data/ledger.txt >/dev/null || fail "could not expand the ledger"
  local cell x y w h nfs px
  cell=$(FS changes-cell) || fail "the diff's number cell was not drawn"
  read -r x y w h nfs <<<"$cell"
  echo "  numberCell: $x $y $w $h at $nfs px"
  tsv_row 8 "numberFontSize (px the diff numbers are drawn at)" "recorded" "$nfs" ok
  park_pointer
  # The read once more after the pointer left: the cell must not have moved.
  [ "$(FS changes-cell)" = "$cell" ] || fail "the number cell moved after the pointer left"
  capture "diff-numbers-atkinson-18"
  local frame="$OUT_DIR/frames/diff-numbers-atkinson-18-$TAG.png"
  local strip strip_colours strip_px row_px
  strip=$(python3 -I -c 'import sys
x, y, w, h = (float(v) for v in sys.argv[1:5])
print("6x%d+%d+%d" % (max(1, round(h)), round(x) - 6, round(y)))' "$x" "$y" "$w" "$h")
  strip_colours=$("$IM" "$frame" -crop "$strip" +repage -format '%k' info:) || fail "could not read the strip"
  strip_px=$("$IM" "$frame" -crop "$strip" +repage -crop 1x1+0+0 +repage -format '%[pixel:p{0,0}]' info:) || fail "could not read the strip's colour"
  # The row's background at the row's right end: 6 px in from the pane's right edge, one pixel
  # below the cell's top edge (above the glyphs' ink, which can run to the pane's edge).
  local pane rx ry
  pane=$(FS pane "$x")
  rx=$(python3 -I -c 'import sys; print(round(float(sys.argv[1]) + float(sys.argv[3]) - 6))' $pane)
  ry=$(python3 -I -c 'import sys; print(round(float(sys.argv[1])) + 1)' "$y")
  row_px=$("$IM" "$frame" -crop "1x1+$rx+$ry" +repage -format '%[pixel:p{0,0}]' info:) || fail "could not read the row's background"
  "$IM" "$frame" -crop "$strip" +repage "$OUT_DIR/crops/diff-strip-$TAG.png"
  if [ "$strip_colours" -eq 1 ]; then
    tsv_row 8 "the strip [x-6, x) left of the number cell is one colour" "1" "$strip_colours" ok
  else
    tsv_row 8 "the strip [x-6, x) left of the number cell is one colour" "1" "$strip_colours" FAIL
    fail "ink crossed the number cell's left edge: the strip holds $strip_colours colours"
  fi
  if [ "$strip_px" = "$row_px" ]; then
    tsv_row 8 "the strip's colour equals the row's background at its right end" "$row_px" "$strip_px" ok
  else
    tsv_row 8 "the strip's colour equals the row's background at its right end" "$row_px" "$strip_px" FAIL
    fail "the strip is not the row's background"
  fi
  # The probed cell must hold five digits of ink at its right edge: on an empty cell, or one that
  # lost a digit, the strip check above would prove nothing.
  crop_frame "diff-numbers-atkinson-18" "diff-cell" "$x" "$y" "$w" "$h"
  FS ink-check "$OUT_DIR/crops/diff-cell-$TAG.png" "$nfs" 8 || fail "the probed number cell does not hold five digits of ink"
  tsv_row 8 "the ledger's git hunk header (asserted at setup)" "@@ -12342,6 +12342,7 @@" "$HUNK_HEADER" ok
}

finish() { # message
  STEP="the last check"
  echo "the last check: quitting leaves no process of this run behind"
  quit_app
  local leftover="" _
  for _ in $(seq 1 25); do
    leftover=$(run_processes)
    [ -z "$leftover" ] && break
    sleep 0.2
  done
  [ -z "$leftover" ] || fail "processes of this run survived the quit: $(echo "$leftover" | tr '\n' ' ')"
  echo "OK: no process of this run is left"
  echo "measurements: $(wc -l <"$OUT_DIR/measurements.tsv") lines in measurements.tsv"
  if ! awk -F'\t' 'NR > 1 && $5 != "ok" && $5 != "limit" && $5 != "skip" {bad = 1} END {exit bad}' "$OUT_DIR/measurements.tsv"; then
    fail "measurements.tsv holds a row that is neither ok, limit nor skip"
  fi
  echo "rows: $(awk -F'\t' 'NR > 1 && $5 == "ok"' "$OUT_DIR/measurements.tsv" | wc -l) ok, $(awk -F'\t' 'NR > 1 && $5 == "limit"' "$OUT_DIR/measurements.tsv" | wc -l) limit, $(awk -F'\t' 'NR > 1 && $5 == "skip"' "$OUT_DIR/measurements.tsv" | wc -l) skip"
  echo "$1"
}

# =============================================================================
seed_appearance
if [ "$DIFF_ONLY" -eq 1 ]; then
  FS db-set appearance.fontPair atkinson
  FS db-set appearance.uiFontSize 18
  launch_app
  ctl project add "$WT" >/dev/null
  diff_numbers_step
  finish "FONT SETTING E2E OK (diff only)"
  exit 0
fi

launch_app
ctl project add "$WT" >/dev/null
ctl select-workspace --workspace "$WT" >/dev/null

# ---- step 1 ----------------------------------------------------------------
step 1 "a fresh database: the pair is atkinson, and every layer names its families"
FS installed || fail "could not list the installed families"
FS check-fonts fresh atkinson first || fail "the fresh run does not read atkinson"
# The default pair's bundled weights too (it is never chosen through the list). Without a painted
# window the read may carry no widths: that is a SKIP under --state-only and a failure otherwise.
widths_status=0
FS check-fonts fresh-widths atkinson widths || widths_status=$?
if [ "$widths_status" -eq 3 ] && [ "$STATE_ONLY" -eq 1 ]; then
  echo "SKIP: the fonts read carries no widths without a painted window"
elif [ "$widths_status" -ne 0 ]; then
  fail "the fresh run's widths do not match atkinson's bundled files"
fi
FS reg-time "$RUN_DIR/app.log" fresh || fail "the startup log's registration line is wrong or absent"

if [ "$STATE_ONLY" -eq 1 ]; then
  echo "SKIP: steps 2-6 and 8-9 need a window to paint and click"
  step 7 "a saved pair is applied from the first read; an empty or unknown id reads as atkinson"
  quit_app
  FS db-set appearance.fontPair plex
  launch_app
  FS check-fonts saved-plex plex first || fail "a saved plex was not applied by the first read"
  quit_app
  FS db-set appearance.fontPair ''
  launch_app
  FS check-fonts empty-id atkinson first || fail "an empty fontPair did not read atkinson"
  quit_app
  FS db-set appearance.fontPair nope
  launch_app
  FS check-fonts unknown-id atkinson first || fail "an unknown fontPair did not read atkinson"
  finish "FONT SETTING E2E OK (state only)"
  exit 0
fi

# ---- step 2 ----------------------------------------------------------------
step 2 "the sheet: a chat with the fixed answer, a source file in the editor, a terminal line"
for position in $(FS tab-positions chat terminal); do
  ctl surface tabs close "$position" --confirm >/dev/null || true
done
for _ in $(seq 1 50); do
  ctl surface open-list open --entry agent:claude >/dev/null 2>&1 && break
  sleep 0.3
done
SURFACE=""
for _ in $(seq 1 50); do
  SURFACE=$(FS chat-open 2>/dev/null || true)
  [ -n "$SURFACE" ] && break
  sleep 0.3
done
[ -n "$SURFACE" ] || fail "no chat tab opened"
FS chat-wait "$SURFACE" idle completed 40 || fail "the chat never became ready"
FS chat-send "$SURFACE" "show me the sheet" || fail "could not send the prompt"
FS chat-wait "$SURFACE" answered 40 || fail "the turn never completed with the fixed answer"
echo "OK: the chat answered with the fixed text"
ctl surface editor open --path "$WT/src/main.rs" >/dev/null || fail "could not open the source file"
ctl surface terminal open --cmd "printf 'TERMINAL LINE 0123456789 abcdefg ABCDEFG\\n'; exec sleep 100000" >/dev/null \
  || fail "could not open the terminal tab"
sleep 2
echo "tabs: $(FS tabs | tr '\n' ';')"
select_tab terminal
park_pointer
capture terminal-start
terminal_crop terminal-start terminal-start
sheet_frame atkinson "${TITLES[0]}"
select_tab "$EDITOR_KIND"
interface_crop sheet-atkinson-editor interface-atkinson
code_crop sheet-atkinson-editor code-atkinson
chat_crop sheet-atkinson-chat chat-atkinson

# ---- step 3 ----------------------------------------------------------------
step 3 "each pair through the real Settings UI"
run_pair() { # id label
  local id=$1 label=$2 title
  title=${TITLES[$(index_of "$id")]}
  echo "-- $title ($label)"
  open_appearance
  choose_pair "$id"
  # The saved row first, before anything else that saves the settings (a size click, a panel resize).
  FS db-wait appearance.fontPair "$id" 10 "pair-$label" || fail "choosing $id did not save appearance.fontPair"
  FS settings-wait fontPair "$id" 15 "pair-$label" || fail "settings read does not name $id after choosing it"
  FS check-fonts "pair-$label" "$id" widths || fail "the fonts read does not match $id"
  press_key Escape
  FS settings closed 15 || fail "Escape did not close Settings"
  FS rect-gone settingsFont 15 || fail "the Font select is still drawn after Settings closed"
  sheet_frame "$label" "$title"
  # The terminal's pane is always drawn: its crop is the one of step 2 in every pair's frame.
  terminal_crop "sheet-$label-chat" "terminal-$label"
  expect_crops_equal "pair-$label" "the terminal's crop is the one of step 2 (it does not change)" "terminal-$label" terminal-start
  chat_crop "sheet-$label-chat" "chat-$label"
  select_tab "$EDITOR_KIND"
  interface_crop "sheet-$label-editor" "interface-$label"
  code_crop "sheet-$label-editor" "code-$label"
}
PREV_LABEL=atkinson
SEEN=" atkinson "
for id in inter plex redhat recursive figtree geist atkinson plex; do
  label=$id
  case "$SEEN" in *" $id "*) label=$id-again ;; esac
  SEEN="$SEEN$id "
  run_pair "$id" "$label"
  expect_crops_differ "pair-$label" "the interface text differs from the previous pair's ($PREV_LABEL)" "interface-$label" "interface-$PREV_LABEL"
  expect_crops_differ "pair-$label" "the code differs from the previous pair's ($PREV_LABEL)" "code-$label" "code-$PREV_LABEL"
  expect_crops_differ "pair-$label" "the chat reply differs from the previous pair's ($PREV_LABEL)" "chat-$label" "chat-$PREV_LABEL"
  PREV_LABEL=$label
done
for again in atkinson plex; do
  expect_crops_equal "pair-$again-again" "choosing $again twice draws the same interface text" "interface-$again-again" "interface-$again"
  expect_crops_equal "pair-$again-again" "choosing $again twice draws the same code" "code-$again-again" "code-$again"
  expect_crops_equal "pair-$again-again" "choosing $again twice draws the same chat reply" "chat-$again-again" "chat-$again"
done
FS fallback-check || fail "bezel's sans fallback differs between Red Hat and Geist"
# The sheet frames of the seven pairs in the list's order, each labelled by its title.
SHEETS=()
for id in "${IDS[@]}"; do SHEETS+=("$OUT_DIR/frames/sheet-$id-$TAG.png"); done
"${MONTAGE[@]}" "${SHEETS[@]}" -tile 4x -geometry 640x+10+10 -background '#303030' "$OUT_DIR/frames/contact-sheet-$TAG.png" \
  || fail "could not build the contact sheet"
echo "FRAME: contact-sheet-$TAG (the seven sheet frames)"
# The detail sheet, at 1:1: per pair a labelled crop of the chat's reply and of the editor's
# first lines, side by side. Both come from the secondary pane's rect (read fresh now, the
# layout is the same in every frame): the reply from 110 px below the pane's content top (the
# "TODAY" line and the prompt bubble above it are skipped, the clock line below the reply is
# mostly left out), 450 px tall; the editor's first 25 lines (21 px each) from 8 px below it.
DETAIL_PANE=$(FS pane secondary)
DETAIL_TILES=()
for i in "${!IDS[@]}"; do
  id=${IDS[$i]}
  crop_frame "sheet-$id-chat" "detail-chat-$id" $(python3 -I -c 'import sys
x, y, w, h = (float(v) for v in sys.argv[1:5])
print(x + 4, y + 110, w - 8, 450)' $DETAIL_PANE)
  crop_frame "sheet-$id-editor" "detail-editor-$id" $(python3 -I -c 'import sys
x, y, w, h = (float(v) for v in sys.argv[1:5])
print(x + 4, y + 8, min(420, w - 100), 25 * 21 + 4)' $DETAIL_PANE)
  font=()
  [ -z "$LABEL_FONT" ] || font=(-font "$LABEL_FONT")
  "$IM" "$OUT_DIR/crops/detail-chat-$id-$TAG.png" "$OUT_DIR/crops/detail-editor-$id-$TAG.png" -background '#202020' -gravity North \
    +smush 12 "$OUT_DIR/crops/detail-row-$id-$TAG.png" || fail "could not join the detail crops of $id"
  width=$("$IM" "$OUT_DIR/crops/detail-row-$id-$TAG.png" -format '%w' info:)
  "$IM" -size "${width}x34" xc:'#303030' "${font[@]}" -fill white -pointsize 22 -gravity West -annotate +10+0 "${TITLES[$i]}  ($TAG)" \
    "$OUT_DIR/crops/detail-row-$id-$TAG.png" -append "$OUT_DIR/crops/detail-tile-$id-$TAG.png" || fail "could not label the detail tile of $id"
  DETAIL_TILES+=("$OUT_DIR/crops/detail-tile-$id-$TAG.png")
done
"${MONTAGE[@]}" "${DETAIL_TILES[@]}" -tile 3x -geometry +12+12 -background '#101010' "$OUT_DIR/frames/contact-sheet-detail-$TAG.png" \
  || fail "could not build the detail sheet"
echo "FRAME: contact-sheet-detail-$TAG (the seven pairs' chat reply and editor lines at 1:1)"

# ---- step 4 ----------------------------------------------------------------
step 4 "Escape closes the open list only; a second Escape closes Settings"
open_appearance
TRIGGER=$(FS rect settingsFont stable)
LIST_AREA=$(python3 -I -c 'import sys
x, y, w, h = (float(v) for v in sys.argv[1:5])
print(x, y + h + 2, w, 170)' $TRIGGER)
move_window "$(rect_center $TRIGGER 0 0)"
sleep 0.6
capture escape-closed-before
crop_frame escape-closed-before escape-list-before $LIST_AREA
click_window "$(rect_center $TRIGGER 0 0)"
sleep 1
capture escape-list-open
crop_frame escape-list-open escape-list-open $LIST_AREA
expect_crops_differ 4 "the list is drawn where it opens" escape-list-open escape-list-before
press_key Escape
FS settings open 5 || fail "the first Escape closed Settings: the open list must take it"
capture escape-list-closed
crop_frame escape-list-closed escape-list-after $LIST_AREA
expect_crops_equal 4 "the list is gone after the first Escape" escape-list-after escape-list-before
FS settings open 2 4 "Settings still reads after the first Escape" || fail "Settings is not open after the first Escape"
press_key Escape
FS settings closed 15 4 "Settings fails to read after the second Escape" || fail "the second Escape did not close Settings"

# ---- step 5 ----------------------------------------------------------------
step 5 "one interface size step keeps the pair's families; the terminal does not change"
open_appearance
SIZE_BEFORE=$(FS settings-get interfaceFontSize)
UP=$(FS rect settingsFontSizeUp stable)
click_window "$(rect_center $UP 0 0)"
FS settings-wait interfaceFontSize "$((SIZE_BEFORE + 1))" 15 5 || fail "the size did not step up"
FS check-fonts size-step plex || fail "a size step changed the pair's families"
capture "size-step-plex-$((SIZE_BEFORE + 1))"
# The decrease button: the increase button's centre minus (28 px button + 48 px rail).
UP=$(FS rect settingsFontSizeUp stable)
click_window "$(rect_center $UP -76 0)"
FS settings-wait interfaceFontSize "$SIZE_BEFORE" 15 5 || fail "the size did not step back"
FS check-fonts size-back plex || fail "stepping back changed the pair's families"
press_key Escape
FS settings closed 15 || fail "Escape did not close Settings"
select_tab terminal
park_pointer
capture terminal-end
terminal_crop terminal-end terminal-end
expect_crops_equal 5 "the terminal's crop is the one of step 2 (it does not change)" terminal-end terminal-start

# ---- step 6 ----------------------------------------------------------------
step 6 "quit and relaunch: the chosen pair is back from the first read"
quit_app
chosen=$(FS db-get appearance.fontPair)
if [ "$chosen" = plex ]; then
  tsv_row 6 "the database row appearance.fontPair" "plex" "$chosen" ok
else
  tsv_row 6 "the database row appearance.fontPair" "plex" "$chosen" FAIL
  fail "the saved pair is '$chosen', not plex"
fi
launch_app
FS check-fonts relaunch plex first || fail "the relaunched app does not name plex from its first read"

# ---- step 7 ----------------------------------------------------------------
step 7 "an unknown appearance.fontPair reads as atkinson"
quit_app
FS db-set appearance.fontPair nope
launch_app
FS check-fonts unknown-id atkinson first || fail "an unknown fontPair did not read atkinson"

# ---- step 8 ----------------------------------------------------------------
quit_app
FS db-set appearance.fontPair atkinson
FS db-set appearance.uiFontSize 18
launch_app
ctl project add "$WT" >/dev/null
diff_numbers_step

finish "FONT SETTING E2E OK (${APPEARANCE:-default})"
