#!/usr/bin/env bash
# Checks rust/vendor/ely against upstream Ely at the pinned rev. For each crate it extracts
# `git archive <rev> crates/<crate>` into a scratch directory, keeps the taken file set, writes
# `diff -ru` to <out>/<crate>.diff, and fails when a differing path is not named in
# LOCAL-CHANGES.md. Prints VENDOR CHECK OK.
#
#   ELY_UPSTREAM=/path/to/a/clone/of/Ely-GPUI-Components rust/vendor/ely/check-vendor.sh [out-dir]
#
# `out-dir` must be absent or empty; without it a fresh `mktemp -d` is used and its path printed.
# Moving the pinned rev means changing REV here and in LOCAL-CHANGES.md.
set -euo pipefail
REV=8d7723ec
VENDOR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
NOTES=$VENDOR/LOCAL-CHANGES.md

if [ -z "${ELY_UPSTREAM:-}" ]; then
    echo "ELY_UPSTREAM is not set: point it at a clone of https://github.com/ELY-Open-Source/Ely-GPUI-Components that has $REV" >&2
    exit 2
fi
if ! git -C "$ELY_UPSTREAM" cat-file -e "$REV^{commit}" 2>/dev/null; then
    echo "ELY_UPSTREAM=$ELY_UPSTREAM has no commit $REV" >&2
    exit 2
fi
if [ $# -ge 1 ]; then
    OUT=$1
    mkdir -p "$OUT"
    if [ -n "$(ls -A "$OUT")" ]; then
        echo "out-dir $OUT is not empty" >&2
        exit 2
    fi
else
    OUT=$(mktemp -d)
fi
OUT=$(cd "$OUT" && pwd)
echo "diffs in $OUT"
UP=$OUT/upstream
mkdir -p "$UP"
fail=0
for dir in "$VENDOR"/crates/*/; do
    crate=$(basename "$dir")
    git -C "$ELY_UPSTREAM" archive "$REV" "crates/$crate" | tar -x -C "$UP"
    # The taken file set: never AGENTS.md or examples/.
    rm -rf "$UP/crates/$crate/examples"
    rm -f "$UP/crates/$crate/AGENTS.md"
    diff -ru "$UP/crates/$crate" "$VENDOR/crates/$crate" > "$OUT/$crate.diff" || true
    {
        sed -n 's|^diff -ru [^ ]*/crates/\([^ ]*\) .*|crates/\1|p' "$OUT/$crate.diff"
        sed -n "s|^Only in $UP/crates/\(.*\): \(.*\)|crates/\1/\2|p" "$OUT/$crate.diff"
        sed -n "s|^Only in $VENDOR/crates/\(.*\): \(.*\)|crates/\1/\2|p" "$OUT/$crate.diff"
    } | sort -u > "$OUT/$crate.paths"
    while IFS= read -r p; do
        [ -n "$p" ] || continue
        if ! grep -qF "\`$p\`" "$NOTES"; then
            echo "UNLISTED DIFFERENCE: $p" >&2
            fail=1
        else
            echo "listed: $p"
        fi
    done < "$OUT/$crate.paths"
done
if [ "$fail" -ne 0 ]; then echo "VENDOR CHECK FAILED" >&2; exit 1; fi
echo "VENDOR CHECK OK"
