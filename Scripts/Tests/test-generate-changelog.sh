#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHANGELOG_SCRIPT="$SCRIPT_DIR/../generate-changelog.sh"

FIXTURE=$(mktemp -d)
trap 'rm -rf "$FIXTURE"' EXIT

git -C "$FIXTURE" init -q
git -C "$FIXTURE" config user.email "test@test.com"
git -C "$FIXTURE" config user.name "Test"

echo "a" > "$FIXTURE/a.txt"
git -C "$FIXTURE" add a.txt
git -C "$FIXTURE" commit -q -m "chore: init"
git -C "$FIXTURE" tag v0.1.0

echo "b" > "$FIXTURE/b.txt"
git -C "$FIXTURE" add b.txt
git -C "$FIXTURE" commit -q -m "feat: add b"

echo "c" > "$FIXTURE/c.txt"
git -C "$FIXTURE" add c.txt
git -C "$FIXTURE" commit -q -m "fix: fix c"

git -C "$FIXTURE" tag v0.2.0

OUTPUT=$("$CHANGELOG_SCRIPT" "v0.2.0" "$FIXTURE")

if ! echo "$OUTPUT" | grep -q "## Features"; then
  echo "FAIL: missing Features section" >&2
  echo "$OUTPUT" >&2
  exit 1
fi
if ! echo "$OUTPUT" | grep -q -- "- add b"; then
  echo "FAIL: missing feat commit in output" >&2
  echo "$OUTPUT" >&2
  exit 1
fi
if ! echo "$OUTPUT" | grep -q "## Fixes"; then
  echo "FAIL: missing Fixes section" >&2
  echo "$OUTPUT" >&2
  exit 1
fi
if ! echo "$OUTPUT" | grep -q -- "- fix c"; then
  echo "FAIL: missing fix commit in output" >&2
  echo "$OUTPUT" >&2
  exit 1
fi

echo "PASS: changelog generation"
