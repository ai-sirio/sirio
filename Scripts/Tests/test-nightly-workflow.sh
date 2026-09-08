#!/bin/bash
# Structure test for nightly.yml, the scheduled caller of build-release.yml
# (#317, spec §3.3). The shared build and publish path is tested by
# test-release-workflow.sh; this file covers only what makes a nightly a
# nightly.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NIGHTLY="$SCRIPT_DIR/../../.github/workflows/nightly.yml"

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

[ -f "$NIGHTLY" ] || fail "$NIGHTLY not found"

# Built from main on a schedule, plus a manual trigger for exercising it.
grep -q "schedule:" "$NIGHTLY" || fail "nightly.yml must run on a schedule"
grep -q "cron:" "$NIGHTLY" || fail "nightly.yml must declare a cron expression"
grep -q "workflow_dispatch:" "$NIGHTLY" || fail "nightly.yml must be triggerable by hand"

# Same three artifacts, same signing: the whole build path is the shared one.
grep -qF "uses: ./.github/workflows/build-release.yml" "$NIGHTLY" \
  || fail "nightly.yml must call the reusable build workflow"
grep -qF "channel: nightly" "$NIGHTLY" || fail "nightly.yml must build the nightly channel"
grep -q "secrets: inherit" "$NIGHTLY" || fail "nightly.yml must pass secrets to the reusable workflow"
grep -qF "prerelease: true" "$NIGHTLY" || fail "a nightly release must be marked prerelease"

# The version the binary carries must sort above yesterday's: the updater
# compares strict semver. `<workspace>-nightly.<YYYYMMDDHHMM>` is one numeric
# prerelease identifier, monotonic per minute, with no leading zero.
grep -qF -- '-nightly.' "$NIGHTLY" || fail "the nightly version must carry a -nightly.<stamp> prerelease"
grep -qF '%Y%m%d%H%M' "$NIGHTLY" || fail "the nightly stamp must be a UTC YYYYMMDDHHMM timestamp"
grep -q "date -u" "$NIGHTLY" || fail "the nightly stamp must be taken in UTC"

# Its tag must never match the v*.*.* pattern that triggers a stable release.
grep -qF 'tag=nightly-' "$NIGHTLY" || fail "the nightly tag must be nightly-<stamp>"
if grep -qE 'tag=v\$|tag=v\{' "$NIGHTLY"; then
  fail "a nightly tag must not start with v"
fi

# A nightly run touches nightly.json only: the other channel's manifest and
# channel value must not appear anywhere in this caller.
if grep -qE "stable\.json|channel: stable|SIRIO_RELEASE_CHANNEL=stable" "$NIGHTLY"; then
  fail "nightly.yml must not name the stable channel or its manifest"
fi

# The macOS gate runs on the maintainer's own machine. Do not rebuild a commit
# that already has a nightly; the manual trigger can force one. A release
# alone is not proof: the skip must also see that commit's manifest on
# gh-pages, or a run that died between release and manifest would never be
# retried.
grep -q "targetCommitish" "$NIGHTLY" || fail "nightly.yml must compare main against the last nightly's commit"
grep -qF 'nightly.json?ref=gh-pages' "$NIGHTLY" || fail "the skip must check that the last nightly's manifest was published"
grep -qF 'FORCE: ${{ inputs.force }}' "$NIGHTLY" || fail "the manual trigger's force input must reach the skip step"

# Overlapping schedules would race on the release list and on gh-pages.
grep -q "concurrency:" "$NIGHTLY" || fail "nightly.yml must declare a concurrency group"

# Notes on nightly say once that it tracks main (spec §7.4), and name the commit.
grep -q "tracks" "$NIGHTLY" || fail "nightly notes must say the channel tracks main"
grep -qF 'github.sha' "$NIGHTLY" || fail "nightly notes must name the built commit"

# No in-app control switches channel; nothing here may reach into settings.
if grep -qi "settings" "$NIGHTLY"; then
  fail "nightly.yml must not touch app settings"
fi

echo "PASS: nightly workflow structure"
