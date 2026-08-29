#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHECK_SCRIPT="$SCRIPT_DIR/../check-release-version.sh"

FIXTURE=$(mktemp -d)
trap 'rm -rf "$FIXTURE"' EXIT

# Shaped like the real rust/Cargo.toml: the workspace version is a top-level
# assignment and every dependency version is inline inside braces. A parser
# that matched "version" anywhere would return 0.61 instead of 0.6.0, which is
# the whole reason this fixture carries a dependency at all.
cat > "$FIXTURE/Cargo.toml" <<'EOF'
[workspace]
resolver = "2"
members = ["crates/sirio"]

[workspace.package]
version = "0.6.0"
edition = "2024"

[workspace.dependencies]
windows-sys = { version = "0.61", features = ["Win32_Foundation"] }
EOF

OUTPUT=$("$CHECK_SCRIPT" "v0.6.0" "$FIXTURE/Cargo.toml")
if [ "$OUTPUT" != "0.6.0" ]; then
  echo "FAIL: expected '0.6.0' for a matching tag, got '$OUTPUT'" >&2
  exit 1
fi

if "$CHECK_SCRIPT" "v0.7.0" "$FIXTURE/Cargo.toml" >/dev/null 2>&1; then
  echo "FAIL: a mismatched tag must exit non-zero" >&2
  exit 1
fi

if "$CHECK_SCRIPT" "v0.6.0" "$FIXTURE/missing.toml" >/dev/null 2>&1; then
  echo "FAIL: a missing Cargo.toml must exit non-zero" >&2
  exit 1
fi

echo "PASS: release version check"
