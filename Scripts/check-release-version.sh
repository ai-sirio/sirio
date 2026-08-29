#!/bin/bash
set -euo pipefail

usage() {
  echo "Usage: $0 <tag> [rust/Cargo.toml path]" >&2
  exit 2
}

if [ $# -lt 1 ]; then
  usage
fi

TAG="$1"
CARGO_TOML="${2:-$(dirname "$0")/../rust/Cargo.toml}"

if [ ! -f "$CARGO_TOML" ]; then
  echo "error: Cargo.toml not found at $CARGO_TOML" >&2
  exit 1
fi

TAG_VERSION="${TAG#v}"
# `[workspace.package]`'s version is the only top-level `version = ` assignment
# in this file; every dependency version is inline inside a `{ ... }` table, so
# an anchored match cannot pick the wrong one.
CARGO_VERSION=$(grep -m1 '^version = ' "$CARGO_TOML" | sed -E 's/^version = "([^"]+)".*/\1/')

if [ -z "$CARGO_VERSION" ]; then
  echo "error: could not find a workspace version in $CARGO_TOML" >&2
  exit 1
fi

if [ "$TAG_VERSION" != "$CARGO_VERSION" ]; then
  echo "error: tag version '$TAG_VERSION' does not match workspace version '$CARGO_VERSION' in $CARGO_TOML" >&2
  exit 1
fi

echo "$CARGO_VERSION"
