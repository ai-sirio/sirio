#!/usr/bin/env bash
# Checks rust/vendor/ely against upstream Ely at the pinned rev. For each crate it extracts
# `git archive <rev> crates/<crate>` into a scratch directory, keeps the taken file set, writes
# `diff -ru` to <out>/<crate>.diff, and collects the paths that differ: changed text files, changed
# binary files and files present on one side only. The set must equal the one listed in the section
# `## Differing paths` of LOCAL-CHANGES.md (one backticked path per bullet, up to the next `## `):
# a differing path that is not listed fails, and so does a listed path that no longer differs.
# Mentioning a path elsewhere in the notes counts for nothing. This checks paths, not hunks: whether
# each hunk is the one the notes describe is a review by hand. It does not look at `ely-palette`,
# `ely-gpui-component` or anything outside `rust/vendor/ely/crates`. Prints VENDOR CHECK OK.
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
: > "$OUT/differing.txt"
for dir in "$VENDOR"/crates/*/; do
    crate=$(basename "$dir")
    git -C "$ELY_UPSTREAM" archive "$REV" "crates/$crate" | tar -x -C "$UP"
    # The taken file set: never AGENTS.md or examples/.
    rm -rf "$UP/crates/$crate/examples"
    rm -f "$UP/crates/$crate/AGENTS.md"
    diff -ru "$UP/crates/$crate" "$VENDOR/crates/$crate" > "$OUT/$crate.diff" || true
    {
        sed -n 's|^diff -ru [^ ]*/crates/\([^ ]*\) .*|crates/\1|p' "$OUT/$crate.diff"
        # `diff -ru` prints no `diff` header for a binary file, only this line.
        sed -n "s|^Binary files $UP/\(crates/.*\) and $VENDOR/crates/.* differ\$|\1|p" "$OUT/$crate.diff"
        sed -n "s|^Only in $UP/crates/\(.*\): \(.*\)|crates/\1/\2|p" "$OUT/$crate.diff"
        sed -n "s|^Only in $VENDOR/crates/\(.*\): \(.*\)|crates/\1/\2|p" "$OUT/$crate.diff"
    } | sort -u > "$OUT/$crate.paths"
    cat "$OUT/$crate.paths" >> "$OUT/differing.txt"
done
sort -u -o "$OUT/differing.txt" "$OUT/differing.txt"
# The listed set: the bullets of the `## Differing paths` section, each one backticked path.
awk '/^## /{in_section = ($0 == "## Differing paths"); next}
     in_section && /^- `[^`]+`$/ {print substr($0, 4, length($0) - 4)}' "$NOTES" | sort -u > "$OUT/listed.txt"
if [ ! -s "$OUT/listed.txt" ]; then
    echo "LOCAL-CHANGES.md has no '## Differing paths' section with bullets" >&2
    fail=1
fi
while IFS= read -r p; do
    echo "UNLISTED DIFFERENCE: $p" >&2
    fail=1
done < <(comm -23 "$OUT/differing.txt" "$OUT/listed.txt")
while IFS= read -r p; do
    echo "STALE ENTRY (listed, no longer differs): $p" >&2
    fail=1
done < <(comm -13 "$OUT/differing.txt" "$OUT/listed.txt")
while IFS= read -r p; do echo "listed: $p"; done < <(comm -12 "$OUT/differing.txt" "$OUT/listed.txt")
if [ "$fail" -ne 0 ]; then echo "VENDOR CHECK FAILED" >&2; exit 1; fi
echo "VENDOR CHECK OK"
