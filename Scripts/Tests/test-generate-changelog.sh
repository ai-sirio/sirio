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

# --- The first release: a repository with no previous tag ------------------
#
# `generate-changelog.sh` falls back to the bare tag as its range when
# `git describe` finds no earlier tag, and `git log <tag>` then walks the whole
# history. On the real first release of this repository that produced a body of
# 140,547 characters against GitHub's 125,000-character release-body limit, and
# `gh release create --notes-file` answered HTTP 422 -- in the `publish` job,
# which runs *after* signing, notarization and every artifact upload have
# succeeded. So the untagged case gets its own fixture: a history long enough to
# cross the cap, asserted to come back bounded and to say out loud that it was
# truncated.
#
# The fixture is built with `git fast-import` rather than a loop of
# `git commit --allow-empty`. 105 separate commit invocations take about two
# minutes on a Windows host; one fast-import process takes half a second. The
# gate runs this test, so that difference is the difference between a test that
# gets run and one that gets skipped.
UNTAGGED=$(mktemp -d)
trap 'rm -rf "$FIXTURE" "$UNTAGGED"' EXIT

git -C "$UNTAGGED" init -q
{
  for i in $(seq 1 105); do
    MSG="feat: change $i"
    printf 'commit refs/heads/main\ncommitter Test <test@test.com> %d +0000\ndata %d\n%s\n' \
      $((1700000000 + i)) "${#MSG}" "$MSG"
  done
  echo "done"
} | git -C "$UNTAGGED" fast-import --quiet
git -C "$UNTAGGED" symbolic-ref HEAD refs/heads/main

# GitHub's documented limit. The assertion is deliberately against the real
# number rather than a round "is it small" figure, because the whole point of
# the cap is the limit it exists to stay under.
GITHUB_BODY_LIMIT=125000

assert_bounded_and_noted() {
  local label="$1"
  local output="$2"

  if [ "${#output}" -ge "$GITHUB_BODY_LIMIT" ]; then
    echo "FAIL: $label: body is ${#output} characters, at or over GitHub's $GITHUB_BODY_LIMIT limit" >&2
    exit 1
  fi

  # 105 commits, a cap of 100: exactly 100 bullets and 5 omitted. Asserting the
  # exact count rather than "fewer than 105" is what makes this fail loudly if
  # the cap is ever removed or silently raised.
  local bullets
  bullets=$(echo "$output" | grep -c '^- ' || true)
  if [ "$bullets" -ne 100 ]; then
    echo "FAIL: $label: expected 100 commit bullets, got $bullets" >&2
    echo "$output" >&2
    exit 1
  fi

  if ! echo "$output" | grep -q "omitted"; then
    echo "FAIL: $label: truncated output must say so" >&2
    echo "$output" >&2
    exit 1
  fi
  if ! echo "$output" | grep -q "5 earlier commits are omitted"; then
    echo "FAIL: $label: truncation note must state how many commits were omitted" >&2
    echo "$output" >&2
    exit 1
  fi

  # Newest kept, oldest dropped: the cap must take the *recent* commits, not an
  # arbitrary hundred. `-x` because "- change 1" is a prefix of "- change 105".
  if ! echo "$output" | grep -qx -- "- change 105"; then
    echo "FAIL: $label: the most recent commit must be listed" >&2
    exit 1
  fi
  if echo "$output" | grep -qx -- "- change 1"; then
    echo "FAIL: $label: the oldest commit must have been truncated away" >&2
    exit 1
  fi
}

# Literally no tags at all: the range argument is just a rev.
assert_bounded_and_noted "no tags" "$("$CHANGELOG_SCRIPT" HEAD "$UNTAGGED")"

# The shape the first release actually takes: the tag being released exists, and
# it is the only one, so there is no *previous* tag to bound the range.
git -C "$UNTAGGED" tag v0.1.0 main
assert_bounded_and_noted "first tag, no previous tag" "$("$CHANGELOG_SCRIPT" v0.1.0 "$UNTAGGED")"

echo "PASS: changelog truncates an untagged first release"
