#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT="$SCRIPT_DIR/../set-workspace-version.sh"

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# A miniature of rust/Cargo.toml: the one top-level `version = ` line under
# [workspace.package], plus dependency versions that live inline in `{ ... }`
# tables and must not be touched -- including one that happens to carry the
# same string as the workspace version.
cat > "$TMP/Cargo.toml" <<'EOF'
[workspace]
members = ["crates/*"]

[workspace.package]
version = "0.6.0"
edition = "2024"

[workspace.dependencies]
serde = { version = "1.0", features = ["derive"] }
sirio_release = { path = "crates/sirio_release", version = "0.6.0" }
EOF

# --- happy path: the nightly stamp lands on the workspace line only ----------
OUT=$(bash "$SCRIPT" 0.6.0-nightly.202609072133 "$TMP/Cargo.toml")
[ "$OUT" = "0.6.0-nightly.202609072133" ] || fail "expected the new version on stdout, got: $OUT"

grep -q '^version = "0.6.0-nightly.202609072133"$' "$TMP/Cargo.toml" \
  || fail "the workspace version line was not rewritten"
[ "$(grep -c '^version = ' "$TMP/Cargo.toml")" -eq 1 ] \
  || fail "there must still be exactly one top-level version line"
grep -qF 'serde = { version = "1.0", features = ["derive"] }' "$TMP/Cargo.toml" \
  || fail "an inline dependency version was altered"
grep -qF 'sirio_release = { path = "crates/sirio_release", version = "0.6.0" }' "$TMP/Cargo.toml" \
  || fail "an inline dependency version equal to the old workspace version was altered"
if ls "$TMP" | grep -q '\.bak$'; then
  fail "a sed backup file was left behind"
fi

# --- a second run is idempotent ----------------------------------------------
bash "$SCRIPT" 0.6.0-nightly.202609072133 "$TMP/Cargo.toml" >/dev/null
[ "$(grep -c '^version = "0.6.0-nightly.202609072133"$' "$TMP/Cargo.toml")" -eq 1 ] \
  || fail "re-running with the same version must be a no-op"

# --- an empty version is refused by name --------------------------------------
if ERR=$(bash "$SCRIPT" "" "$TMP/Cargo.toml" 2>&1); then
  fail "an empty version must be rejected"
fi
case "$ERR" in
  *"version must not be empty"*) ;;
  *) fail "an empty version must be rejected by name, got: $ERR" ;;
esac

# --- a version that is not semver is refused: it would compile into the ------
# binary and make the updater's own version parse fail on every check.
if ERR=$(bash "$SCRIPT" "nightly-0930" "$TMP/Cargo.toml" 2>&1); then
  fail "a non-semver version must be rejected"
fi
case "$ERR" in
  *"not a semantic version"*) ;;
  *) fail "a non-semver version must be rejected by name, got: $ERR" ;;
esac
if ERR=$(bash "$SCRIPT" "0.6.0-nightly.0930" "$TMP/Cargo.toml" 2>&1); then
  fail "a numeric prerelease identifier with a leading zero must be rejected"
fi

# --- a file without a workspace version line is refused, not silently left ---
printf '[workspace]\nmembers = []\n' > "$TMP/NoVersion.toml"
if ERR=$(bash "$SCRIPT" 1.0.0 "$TMP/NoVersion.toml" 2>&1); then
  fail "a Cargo.toml without a workspace version must be rejected"
fi
case "$ERR" in
  *"could not find a workspace version"*) ;;
  *) fail "a missing version line must be reported by name, got: $ERR" ;;
esac

echo "PASS: set-workspace-version rewrites the workspace line only"
