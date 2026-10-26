#!/bin/bash
set -euo pipefail

# The contract of Scripts/capture-diff.py, the pixel diff every bezel-off PR is judged by
# (docs/testing/bezel-off.md). It builds synthetic 64x64 PNGs with ImageMagick and asserts, case
# by case, what the diff prints, which exit code it returns, what changed.tsv records and what
# index.html lists. A diff that calls a changed frame identical would let a regression through
# unseen, so most cases are the ways it could lie: a mask one pixel too small, an allow
# rectangle that covers only half the change, a frame missing on one side, a size change, a
# malformed table, an empty tree, a file that is not an image.
#
# It needs no display and no build. Usage: Scripts/Tests/test-capture-diff.sh
# Prints "CAPTURE DIFF CONTRACT OK" when every case holds.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DIFF="$ROOT/Scripts/capture-diff.py"

fail() { echo "FAIL: $*" >&2; exit 1; }
command -v magick >/dev/null || fail "ImageMagick (magick) is required"
command -v python3 >/dev/null || fail "python3 is required"
[ -f "$DIFF" ] || fail "Scripts/capture-diff.py is missing"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/capture-diff-test-XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

TAB=$'\t'
CASE=""; B=""; A=""; O=""; OUTPUT=""; CODE=0

new_case() { # name: fresh BEFORE, AFTER and OUT directories
  CASE="$1"
  B="$WORK/$1/before"; A="$WORK/$1/after"; O="$WORK/$1/out"
  mkdir -p "$B" "$A"
}
base() { magick -size 64x64 xc:'#336699' "PNG24:$1"; }
# The base picture with single pixels painted red: pixel X,Y [X,Y ...] FILE
pixels() {
  local file="${*: -1}" args=() point
  for point in "${@:1:$#-1}"; do args+=(-draw "point $point"); done
  magick -size 64x64 xc:'#336699' -fill '#ff0000' +antialias "${args[@]}" "PNG24:$file"
}
run_diff() { # args...: sets OUTPUT and CODE
  set +e
  OUTPUT="$(python3 "$DIFF" "$@" 2>&1)"
  CODE=$?
  set -e
}
expect_code() {
  [ "$CODE" -eq "$1" ] || fail "$CASE: exit $CODE, want $1; output: $OUTPUT"
  echo "ok: $CASE: exit $1"
}
expect_out() {
  case "$OUTPUT" in *"$1"*) echo "ok: $CASE: prints '$1'" ;; *) fail "$CASE: output lacks '$1': $OUTPUT" ;; esac
}
expect_no_out() {
  case "$OUTPUT" in *"$1"*) fail "$CASE: output has '$1': $OUTPUT" ;; *) ;; esac
}
cell() { # frame column: the changed.tsv cell, exactly one row for the frame
  python3 - "$O/changed.tsv" "$1" "$2" <<'PY'
import csv, sys
path, frame, column = sys.argv[1:4]
with open(path, newline="") as handle:
    rows = [row for row in csv.DictReader(handle, delimiter="\t") if row["frame"] == frame]
if len(rows) != 1:
    sys.exit(f"{path}: {len(rows)} rows for frame {frame!r}")
print(rows[0][column])
PY
}
expect_cell() { # frame column want
  local got
  got="$(cell "$1" "$2")" || fail "$CASE: $got"
  [ "$got" = "$3" ] || fail "$CASE: changed.tsv $1/$2 is '$got', want '$3'"
  echo "ok: $CASE: $1 $2 = $3"
}
rows() { # the number of data rows in changed.tsv, after checking its header
  python3 - "$O/changed.tsv" <<'PY'
import sys
lines = open(sys.argv[1]).read().splitlines()
want = "frame\tstatus\tpixels\tbbox\tunattributed_pixels\trows"
if not lines or lines[0] != want:
    sys.exit(f"header is {lines[:1]!r}, want {want!r}")
print(len(lines) - 1)
PY
}
expect_rows() {
  local got
  got="$(rows)" || fail "$CASE: changed.tsv: $got"
  [ "$got" = "$1" ] || fail "$CASE: changed.tsv has $got rows, want $1"
  echo "ok: $CASE: changed.tsv has $1 rows"
}
mask_line() { printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$@"; } # glob x y w h why

# ---- 1. two identical trees (a nested frame included)
new_case identical
base "$B/a.png"; base "$A/a.png"
mkdir -p "$B/sub" "$A/sub"; pixels 3,3 "$B/sub/b.png"; pixels 3,3 "$A/sub/b.png"
run_diff "$B" "$A" --out "$O"
expect_code 0
expect_out "CAPTURE DIFF IDENTICAL"
expect_rows 0

# ---- 2. one pixel changed at (10,10)
new_case one-pixel
base "$B/a.png"; pixels 10,10 "$A/a.png"
base "$B/same.png"; base "$A/same.png"
run_diff "$B" "$A" --out "$O"
expect_code 1
expect_out "UNATTRIBUTED"
expect_rows 1
expect_cell a status changed
expect_cell a pixels 1
expect_cell a bbox 10,10,1,1
expect_cell a unattributed_pixels 1

# a difference of 1/255 in one channel is invisible to the eye and to ImageMagick's AE figure
new_case one-step
base "$B/a.png"
magick -size 64x64 xc:'#336699' -fill '#336698' +antialias -draw "point 10,10" "PNG24:$A/a.png"
run_diff "$B" "$A" --out "$O"
expect_code 1
expect_cell a pixels 1
expect_cell a bbox 10,10,1,1

# the bounding box is the union of the changed regions, corners included
new_case corners
base "$B/a.png"; pixels 0,0 "$A/a.png"
run_diff "$B" "$A" --out "$O"
expect_code 1
expect_cell a bbox 0,0,1,1
pixels 0,0 63,63 "$A/a.png"
run_diff "$B" "$A" --out "$O"
CASE="corners: two corners"; expect_cell a pixels 2
expect_cell a bbox 0,0,64,64

# ---- 3. the same change inside a --masks rectangle, and the rectangle's edges
new_case mask
base "$B/a.png"; pixels 10,10 "$A/a.png"
mask_line a 10 10 1 1 "caret blink" > "$WORK/mask.tsv"
run_diff "$B" "$A" --out "$O" --masks "$WORK/mask.tsv"
expect_code 0
expect_out "CAPTURE DIFF IDENTICAL"
expect_rows 0

mask_line a 0 0 11 11 "last pixel of the rectangle" > "$WORK/mask.tsv"
run_diff "$B" "$A" --out "$O" --masks "$WORK/mask.tsv"
CASE="mask ends on the change"; expect_code 0
mask_line a 11 10 1 1 "one pixel to the right" > "$WORK/mask.tsv"
run_diff "$B" "$A" --out "$O" --masks "$WORK/mask.tsv"
CASE="mask starts after the change"; expect_code 1
mask_line a 0 0 10 10 "one pixel short" > "$WORK/mask.tsv"
run_diff "$B" "$A" --out "$O" --masks "$WORK/mask.tsv"
CASE="mask ends before the change"; expect_code 1
mask_line other 0 0 64 64 "a glob that matches no frame" > "$WORK/mask.tsv"
run_diff "$B" "$A" --out "$O" --masks "$WORK/mask.tsv"
CASE="mask of another frame"; expect_code 1
mask_line '*' 0 0 64 64 "a glob that matches every frame" > "$WORK/mask.tsv"
run_diff "$B" "$A" --out "$O" --masks "$WORK/mask.tsv"
CASE="mask of every frame"; expect_code 0
{ echo "# a comment"; echo; mask_line a 10 10 1 1 "after a comment and a blank line"; } > "$WORK/mask.tsv"
run_diff "$B" "$A" --out "$O" --masks "$WORK/mask.tsv"
CASE="mask after a comment"; expect_code 0

# ---- 4. the same change inside an --allow rectangle
new_case allow
base "$B/a.png"; pixels 10,10 "$A/a.png"
mask_line a 8 8 5 5 "ui::tooltip::Tooltip" > "$WORK/allow.tsv"
run_diff "$B" "$A" --out "$O" --allow "$WORK/allow.tsv"
expect_code 0
expect_out "CAPTURE DIFF OK (1 changed, all attributed)"
expect_rows 1
expect_cell a status changed
expect_cell a pixels 1
expect_cell a unattributed_pixels 0
expect_cell a rows "ui::tooltip::Tooltip"
python3 - "$O/index.html" <<'PY' || fail "allow: index.html does not name the frame and its row"
import re, sys
html = open(sys.argv[1]).read()
section = re.search(r'<section[^>]*data-frame="a"[^>]*>(.*?)</section>', html, re.S)
if not section or "ui::tooltip::Tooltip" not in section.group(1):
    sys.exit(1)
PY
echo "ok: allow: index.html names frame a and its row"

# half of the change is outside the rectangle: attributed in part is not attributed
new_case allow-half
base "$B/a.png"; pixels 10,10 50,50 "$A/a.png"
{ mask_line a 8 8 5 5 "ui::tooltip::Tooltip"; mask_line a 40 40 5 5 "ui::unrelated::Row"; } > "$WORK/allow.tsv"
run_diff "$B" "$A" --out "$O" --allow "$WORK/allow.tsv"
expect_code 1
expect_out "UNATTRIBUTED"
expect_cell a pixels 2
expect_cell a unattributed_pixels 1
expect_cell a rows "ui::tooltip::Tooltip"

# an allow rectangle of another frame attributes nothing
new_case allow-other
base "$B/a.png"; pixels 10,10 "$A/a.png"
mask_line other 0 0 64 64 "ui::tooltip::Tooltip" > "$WORK/allow.tsv"
run_diff "$B" "$A" --out "$O" --allow "$WORK/allow.tsv"
expect_code 1
expect_cell a unattributed_pixels 1

# ---- 5. a frame only in AFTER
new_case added
base "$B/a.png"; base "$A/a.png"; base "$A/new.png"
run_diff "$B" "$A" --out "$O"
expect_code 1
expect_rows 1
expect_cell new status added

# ---- 6. a frame only in BEFORE
new_case removed
base "$B/a.png"; base "$B/gone.png"; base "$A/a.png"
run_diff "$B" "$A" --out "$O"
expect_code 1
expect_rows 1
expect_cell gone status removed

# ---- 7. different sizes
new_case size
base "$B/a.png"; magick -size 32x32 xc:'#336699' "PNG24:$A/a.png"
run_diff "$B" "$A" --out "$O"
expect_code 1
expect_rows 1
expect_cell a status size

# ---- 8. index.html lists exactly the changed, added, removed and size frames
new_case index
base "$B/same.png"; base "$A/same.png"
base "$B/chg.png"; pixels 10,10 "$A/chg.png"
base "$A/add.png"
base "$B/rem.png"
base "$B/sz.png"; magick -size 32x32 xc:'#336699' "PNG24:$A/sz.png"
mkdir -p "$B/sub" "$A/sub"; base "$B/sub/deep.png"; pixels 20,20 "$A/sub/deep.png"
mask_line chg 8 8 5 5 "ui::tooltip::Tooltip" > "$WORK/allow.tsv"
run_diff "$B" "$A" --out "$O" --allow "$WORK/allow.tsv"
expect_code 1
expect_rows 5
python3 - "$O" <<'PY' || fail "index: index.html does not list exactly the changed, added, removed and size frames"
import os, re, sys
out = sys.argv[1]
html = open(os.path.join(out, "index.html")).read()
frames = sorted(re.findall(r'data-frame="([^"]*)"', html))
want = sorted(["chg", "add", "rem", "sz", "sub/deep"])
if frames != want:
    sys.exit(f"index.html lists {frames}, want {want}")
sources = re.findall(r'src="([^"]*)"', html)
if not sources:
    sys.exit("index.html shows no image")
for source in sources:
    if source.startswith("/") or "://" in source:
        sys.exit(f"{source} is not a relative path with no leading slash")
    if not os.path.isfile(os.path.join(out, source)):
        sys.exit(f"{source} is not under OUT")
section = re.search(r'<section[^>]*data-frame="chg"[^>]*>(.*?)</section>', html, re.S).group(1)
shown = re.findall(r'src="([^"]*)"', section)
if [part.split("/")[0] for part in shown] != ["before", "after", "diff"]:
    sys.exit(f"the changed frame shows {shown}, want before, after, diff in that order")
if "same" in frames:
    sys.exit("an identical frame is listed")
PY
echo "ok: index: index.html lists chg, add, rem, sz and sub/deep, before | after | diff, all under OUT"

# ---- the ways the tool itself could be wrong or fooled
new_case bad-table
base "$B/a.png"; pixels 10,10 "$A/a.png"
printf 'a\t10\t10\n' > "$WORK/mask.tsv"
run_diff "$B" "$A" --out "$O" --masks "$WORK/mask.tsv"
expect_code 2
expect_no_out "CAPTURE DIFF IDENTICAL"
printf 'a\tten\t10\t1\t1\tcaret\n' > "$WORK/mask.tsv"
run_diff "$B" "$A" --out "$O" --masks "$WORK/mask.tsv"
CASE="bad-table: a rectangle that is not a number"; expect_code 2
expect_no_out "CAPTURE DIFF IDENTICAL"

new_case missing-before
base "$A/a.png"
run_diff "$WORK/no-such-tree" "$A" --out "$O"
expect_code 2
expect_no_out "CAPTURE DIFF IDENTICAL"

# two trees with no frame at all would be "identical" only because nothing was captured
new_case no-frames
run_diff "$B" "$A" --out "$O"
expect_code 2
expect_no_out "CAPTURE DIFF IDENTICAL"

new_case not-an-image
echo "one" > "$B/a.png"; echo "two" > "$A/a.png"
run_diff "$B" "$A" --out "$O"
expect_code 2
expect_no_out "CAPTURE DIFF IDENTICAL"

echo "CAPTURE DIFF CONTRACT OK"
