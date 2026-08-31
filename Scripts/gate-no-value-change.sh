#!/usr/bin/env bash
# Phase 1 gate for the bezel theme adoption (see
# docs/superpowers/plans/2026-08-30-bezel-theme-adoption.md).
#
# A rename moves names, never numbers. This compares the multiset of numeric
# literals across the whole Rust tree at <base-ref> against the working tree:
# a pure rename leaves it identical. Comparing multisets rather than diff
# hunks means a moved or reordered line is not a false positive.
set -euo pipefail

BASE="${1:?usage: gate-no-value-change.sh <base-ref>}"
PATTERN='0x[0-9A-Fa-f]+|[0-9]+\.[0-9]+f?'

before="$(mktemp)"
after="$(mktemp)"
trap 'rm -f "$before" "$after"' EXIT

git grep -hoE "$PATTERN" "$BASE" -- 'rust/crates/*.rs' | sort | uniq -c >"$before"
git grep -hoE "$PATTERN" -- 'rust/crates/*.rs' | sort | uniq -c >"$after"

if diff -u "$before" "$after" >/dev/null; then
    echo "GATE OK: no numeric literal changed since $BASE"
    exit 0
fi

echo "GATE FAILED: numeric literals moved since $BASE" >&2
echo "--- $BASE" >&2
diff -u "$before" "$after" >&2 || true
exit 1
