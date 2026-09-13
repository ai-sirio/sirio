#!/bin/bash
# Refuse when the workspace version is not above the highest release tag.
#
# The version in rust/Cargo.toml names the NEXT release: it moves once, in the
# commit that opens a cycle right after the previous tag, and then stays put
# until the release. A forgotten bump is silent -- main keeps the number
# already published, the next nightly is stamped
# `<already-released>-nightly.<stamp>`, which semver sorts BELOW the stable the
# user already has, and nobody notices until somebody misses an update. This is
# the check that makes the omission loud: one file read and one `git tag`, so
# Scripts/ci.sh can afford it before building anything.
#
# Exits 0 with a skip message when there is nothing to compare:
#   - no `v*.*.*` tag at all: a fresh clone, or a CI checkout made with
#     `fetch-depth: 1`;
#   - SIRIO_CI_RELEASE_GATE=1, which .github/workflows/build-release.yml and
#     nothing else sets. That workflow runs Scripts/ci.sh against the release
#     tag's own checkout, where the workspace version EQUALS the tag by
#     construction -- comparing there would refuse every release.
#
# Prints the version when it passes, like Scripts/check-release-version.sh.
set -euo pipefail
cd "$(dirname "$0")/.."

usage() {
  echo "Usage: $0 [rust/Cargo.toml path]" >&2
  exit 2
}

if [ $# -gt 1 ]; then
  usage
fi

CARGO_TOML="${1:-rust/Cargo.toml}"

if [ ! -f "$CARGO_TOML" ]; then
  echo "error: Cargo.toml not found at $CARGO_TOML" >&2
  exit 1
fi

# `[workspace.package]`'s version is the only top-level `version = ` assignment
# in this file; every dependency version is inline inside a `{ ... }` table, so
# an anchored match cannot pick the wrong one.
CARGO_VERSION=$(grep -m1 '^version = ' "$CARGO_TOML" | sed -E 's/^version = "([^"]+)".*/\1/' || true)

if [ -z "$CARGO_VERSION" ]; then
  echo "error: could not find a workspace version in $CARGO_TOML" >&2
  exit 1
fi

if [[ "${SIRIO_CI_RELEASE_GATE:-}" == "1" ]]; then
  echo "skipping the cycle version check: SIRIO_CI_RELEASE_GATE=1, a release checkout whose version equals its tag"
  exit 0
fi

# Nightly tags are `nightly-<stamp>`, so the `v` prefix and the three numeric
# components keep them out of this list. `sort -V` orders what is left by
# version rather than by byte (0.9.10 above 0.9.9), which plain `sort` does not.
# `|| true`: outside a git checkout entirely -- an exported tarball -- `git
# tag` fails, and under `set -e` that would kill the gate instead of
# skipping, which is the same "nothing to compare against" case as below.
HIGHEST=$(git tag --list 'v*.*.*' 2>/dev/null | sed 's/^v//' | sort -V | tail -1 || true)
if [ -z "$HIGHEST" ]; then
  echo "skipping the cycle version check: no v*.*.* release tag in this checkout to compare against"
  exit 0
fi

# The higher of the two; if the tag is still on top, the cycle never opened.
if [ "$(printf '%s\n' "$HIGHEST" "$CARGO_VERSION" | sort -V | tail -1)" = "$HIGHEST" ]; then
  echo "error: workspace version '$CARGO_VERSION' in $CARGO_TOML is not above the highest release tag 'v$HIGHEST'" >&2
  echo "error: the bump that opens a new cycle is missing, so a nightly stamped '$CARGO_VERSION-nightly.<stamp>' would sort below the release already published; open the cycle with: Scripts/set-workspace-version.sh <next version>" >&2
  exit 1
fi

echo "$CARGO_VERSION"
