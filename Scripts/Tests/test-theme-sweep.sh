#!/usr/bin/env bash
# Captures an isolated, real Sirio in every base colour x appearance through
# Scripts/visual-sweep.sh, seeding the persisted theme settings before each
# launch. Visual evidence for a person to compare side by side; the
# pixel-exact proof of the theme migration is the token dump in
# docs/testing/theme-ely-palette.
#
#   Scripts/Tests/test-theme-sweep.sh --out-dir DIR [--bin PATH] [--display D] [--settle S]
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR=""
PASS=()
while (($# > 0)); do
    case "$1" in
        --out-dir) OUT_DIR="$2"; shift 2 ;;
        --bin|--display|--settle) PASS+=("$1" "$2"); shift 2 ;;
        *) echo "unknown option '$1'" >&2; exit 2 ;;
    esac
done
[[ -n "$OUT_DIR" ]] || { echo "--out-dir is required" >&2; exit 2; }
for base in neutral stone zinc gray slate notte onice; do
    for appearance in dark light; do
        "$ROOT/Scripts/visual-sweep.sh" "${PASS[@]}" --out-dir "$OUT_DIR/$base-$appearance" \
            --setting "appearance.theme=\"$appearance\"" \
            --setting "appearance.baseColor=\"$base\""
    done
done
echo "THEME SWEEP OK: $OUT_DIR"
