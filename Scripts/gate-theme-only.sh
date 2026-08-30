#!/usr/bin/env bash
# Phase 2 gate for the bezel theme adoption.
#
# Phase 2 changes values, never call sites. If a file outside sirio_theme moved,
# a value change is hiding somewhere a reviewer cannot separate it from a
# refactor -- which is the exact failure the phase split exists to prevent.
set -euo pipefail

BASE="${1:?usage: gate-theme-only.sh <base-ref>}"

stray="$(git diff --name-only "$BASE" -- 'rust/' |
    grep -v '^rust/crates/sirio_theme/' || true)"

if [[ -z "$stray" ]]; then
    echo "GATE OK: only sirio_theme changed since $BASE"
    exit 0
fi

echo "GATE FAILED: files outside sirio_theme changed since $BASE" >&2
printf '%s\n' "$stray" >&2
exit 1
