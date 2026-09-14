#!/bin/bash
# Structure test for pr.yml, the gate that runs on every pull request.
#
# This one guards against a specific kind of regression: a gate that still
# exists, still reports green, and no longer checks anything. Every assertion
# below is a way that has already happened here or is one edit away -- the
# repo spent its whole life until #477 with four green CodeQL ticks on PRs
# that compiled nothing, which is exactly what a hollowed-out gate looks like
# from the outside.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PR="$SCRIPT_DIR/../../.github/workflows/pr.yml"

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

[ -f "$PR" ] || fail "$PR not found"

# pr.yml carries more prose than YAML, and two of the assertions below are
# about a string being *absent*. Read through a comment-stripped copy so that
# explaining why something is not set cannot be mistaken for setting it --
# which is exactly what the first run of this test caught.
PR_CODE="$(mktemp)"
trap 'rm -f "$PR_CODE"' EXIT
sed 's/[[:space:]]*#.*$//' "$PR" >"$PR_CODE"

# The whole point: a pull request must trigger it. Lose this line and the file
# stays, the job stays, and no PR is ever checked again.
grep -q "pull_request:" "$PR_CODE" || fail "pr.yml must run on pull requests"

# `main` too. A push can reach it without a PR (the ruleset has bypass
# holders), and a merge commit compiles neither of its parents.
grep -q "push:" "$PR_CODE" || fail "pr.yml must also check main itself"

# It must run the repo's declared gate, not a hand-rolled subset of it.
# CLAUDE.md says Scripts/ci.sh printing `CI OK` is what a change must clear;
# a workflow running `cargo build` directly would drift from that silently.
# Anchored to the `run:` line on purpose: the job is *named* after the script
# too, and matching the bare string passed happily against a workflow whose
# only real command had been swapped for `cargo build --workspace`.
grep -qE "^[[:space:]]*run:[[:space:]]*Scripts/ci\.sh[[:space:]]*$" "$PR_CODE" \
  || fail "pr.yml must run Scripts/ci.sh, the declared gate"

# Scripts/ci.sh runs check-cycle-version.sh, which compares the workspace
# version against the last released tag. A shallow clone carries no tags, so
# the check would pass by knowing nothing -- the exact failure it exists to
# catch, and the one that let #474 propose walking 0.14.1 back to 0.14.0.
grep -qF "fetch-depth: 0" "$PR_CODE" || fail "pr.yml must fetch tags for check-cycle-version.sh"

# libghostty-vt-sys builds only with Zig EXACTLY 0.15.2, and a newer one fails
# too (#63), so an unpinned or bumped version here is not a maintenance chore.
grep -qF "version: 0.15.2" "$PR_CODE" || fail "pr.yml must pin Zig to exactly 0.15.2"

# rust/.cargo/config.toml sets `rustc-wrapper = "sccache"` unconditionally, so
# a runner without it fails before compiling a single crate.
grep -q "sccache" "$PR_CODE" || fail "pr.yml must install sccache"

# Scripts/ci.sh preflights cargo-nextest and refuses without it.
grep -q "nextest" "$PR_CODE" || fail "pr.yml must install cargo-nextest"

# GPUI links the system GTK/WebKit stack on Linux; without these the run dies
# in a transitive -sys crate's build script naming neither GPUI nor this repo.
grep -q "libgtk-3-dev" "$PR_CODE" || fail "pr.yml must install GPUI's GTK dependencies"
grep -q "libwebkit2gtk-4.1-dev" "$PR_CODE" || fail "pr.yml must install GPUI's WebKit dependencies"

# SIRIO_CI_RELEASE_GATE selects the `ci` nextest profile: two retries and the
# skip list for the agent-CLI conformance tests. Those exist so a *release*
# does not die of one timing flake. A pull request is where a developer has to
# see that flake, and where nothing should be skipped -- setting it here would
# quietly turn a flaky test green on the one run that could still catch it.
if grep -q "SIRIO_CI_RELEASE_GATE" "$PR_CODE"; then
  fail "pr.yml must not select the release profile's retries and skips"
fi

echo "PASS: pr workflow structure"
