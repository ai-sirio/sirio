#!/bin/bash
set -euo pipefail

usage() {
  echo "Usage: $0 <current-tag> [repo-path]" >&2
  exit 2
}

if [ $# -lt 1 ]; then
  usage
fi

CURRENT_TAG="$1"
REPO_PATH="${2:-.}"

cd "$REPO_PATH"

# GitHub rejects a release body longer than 125,000 characters with HTTP 422,
# and the `gh release create --notes-file` call that would hit that limit runs
# *last* in .github/workflows/release.yml -- after Developer ID signing, the
# Apple notarization round-trip and all three artifact uploads have already
# succeeded. A body that is too long therefore fails at the most expensive
# possible moment, so the cap belongs here, in the script that owns range
# selection, rather than in the workflow that consumes its output.
#
# With a previous tag the range is bounded by construction: one release's worth
# of commits. Without one -- the first release of a repository that has never
# been tagged -- the fallback range is the bare tag, and `git log <tag>` walks
# the entire history. For this repository that was measured at 2552 commits and
# a body of 140,547 characters: over the limit, and not useful release notes
# anyway, since nobody reads "everything since the project began" as a summary
# of what shipped. Cap the untagged case and state the truncation in the output,
# so the omission is a documented fact rather than a silent one.
MAX_FIRST_RELEASE_COMMITS=100

PREV_TAG=$(git describe --tags --abbrev=0 "${CURRENT_TAG}^" 2>/dev/null || true)

TRUNCATION_NOTE=""

if [ -n "$PREV_TAG" ]; then
  RANGE="$PREV_TAG..$CURRENT_TAG"
  COMMITS=$(git log "$RANGE" --pretty=format:'%s')
else
  RANGE="$CURRENT_TAG"
  TOTAL_COMMITS=$(git rev-list --count "$RANGE")
  COMMITS=$(git log "$RANGE" --max-count="$MAX_FIRST_RELEASE_COMMITS" --pretty=format:'%s')
  if [ "$TOTAL_COMMITS" -gt "$MAX_FIRST_RELEASE_COMMITS" ]; then
    OMITTED_COMMITS=$((TOTAL_COMMITS - MAX_FIRST_RELEASE_COMMITS))
    TRUNCATION_NOTE="_First release: no previous tag to compare against, so this list is truncated to the most recent ${MAX_FIRST_RELEASE_COMMITS} of ${TOTAL_COMMITS} commits; ${OMITTED_COMMITS} earlier commits are omitted._"
  fi
fi

FEATS=$(echo "$COMMITS" | grep -E '^feat(\(.+\))?:' || true)
FIXES=$(echo "$COMMITS" | grep -E '^fix(\(.+\))?:' || true)
OTHERS=$(echo "$COMMITS" | grep -vE '^(feat|fix)(\(.+\))?:' || true)

print_section() {
  local title="$1"
  local items="$2"
  if [ -n "$items" ]; then
    echo "## $title"
    echo ""
    echo "$items" | sed -E 's/^[a-z]+(\([^)]+\))?: */- /'
    echo ""
  fi
}

if [ -n "$TRUNCATION_NOTE" ]; then
  echo "$TRUNCATION_NOTE"
  echo ""
fi

print_section "Features" "$FEATS"
print_section "Fixes" "$FIXES"
print_section "Other" "$OTHERS"
