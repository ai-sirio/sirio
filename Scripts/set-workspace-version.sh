#!/bin/bash
# Rewrite `[workspace.package] version` in rust/Cargo.toml.
#
# The updater compares strict semver against CARGO_PKG_VERSION, and that
# constant comes from this one line: a nightly compiled at the bare workspace
# version would never see the next nightly. The nightly job (#317) stamps
# `<workspace>-nightly.<YYYYMMDDHHMM>` here before compiling; the stable job
# runs it too, as a no-op, so both channels take the same path. Prints the
# version it wrote.
set -euo pipefail

usage() {
  echo "Usage: $0 <version> [rust/Cargo.toml path]" >&2
  exit 2
}

if [ $# -lt 1 ]; then
  usage
fi

VERSION="$1"
CARGO_TOML="${2:-$(dirname "$0")/../rust/Cargo.toml}"

if [ -z "$VERSION" ]; then
  echo "error: version must not be empty" >&2
  exit 1
fi

# semver.org's grammar, as an ERE. It also rejects a numeric prerelease
# identifier with a leading zero (`nightly.0930`), which the `semver` crate
# inside the updater refuses to parse -- and a version the updater cannot
# parse fails every check for the lifetime of that binary.
SEMVER='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-((0|[1-9][0-9]*|[0-9]*[a-zA-Z-][0-9a-zA-Z-]*)(\.(0|[1-9][0-9]*|[0-9]*[a-zA-Z-][0-9a-zA-Z-]*))*))?(\+([0-9a-zA-Z-]+(\.[0-9a-zA-Z-]+)*))?$'
if ! [[ "$VERSION" =~ $SEMVER ]]; then
  echo "error: '$VERSION' is not a semantic version" >&2
  exit 1
fi

if [ ! -f "$CARGO_TOML" ]; then
  echo "error: Cargo.toml not found at $CARGO_TOML" >&2
  exit 1
fi

# `[workspace.package]`'s version is the only top-level `version = ` line in
# this file; every dependency version sits inline in a `{ ... }` table
# (Scripts/check-release-version.sh relies on the same fact). Refuse rather
# than guess if that ever stops being true.
COUNT=$(grep -c '^version = ' "$CARGO_TOML" || true)
if [ "$COUNT" -eq 0 ]; then
  echo "error: could not find a workspace version in $CARGO_TOML" >&2
  exit 1
fi
if [ "$COUNT" -ne 1 ]; then
  echo "error: expected exactly one top-level version line in $CARGO_TOML, found $COUNT" >&2
  exit 1
fi

# `-i.bak`: GNU sed takes an optional suffix, BSD sed a mandatory one, and this
# runs on the macOS release runner as well as the hosted linux and windows ones.
sed -i.bak -E "s/^version = \"[^\"]*\"/version = \"${VERSION}\"/" "$CARGO_TOML"
rm -f "${CARGO_TOML}.bak"

echo "$VERSION"
