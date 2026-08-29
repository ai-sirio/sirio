#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$ROOT/Scripts/visual-sweep.sh"

[[ -x "$SCRIPT" ]] || {
    echo "visual sweep script is missing or not executable" >&2
    exit 1
}

help="$($SCRIPT --help)"
grep -Fq -- "--state-only" <<<"$help"
grep -Fq -- "--out-dir" <<<"$help"
grep -Fq -- "--display" <<<"$help"

grep -Fq -- 'xprop' "$SCRIPT"
grep -Fq -- 'WID="$(find_window)"' "$SCRIPT"
grep -Fq -- 'import -window' "$SCRIPT"
grep -Fq -- 'MIN_COLORS=200' "$SCRIPT"
grep -Fq -- 'SIRIO_SOCKET="$SOCKET"' "$SCRIPT"
grep -Fq -- 'git -C "$FIXTURE" init -q' "$SCRIPT"
grep -Fq -- 'no usable X display' "$SCRIPT"
grep -Fq -- 'distinct colours' "$SCRIPT"
grep -Fq -- 'Scripts/sbs.py' "$SCRIPT"
grep -Fq -- 'montage' "$SCRIPT"

echo "visual sweep contract OK"
