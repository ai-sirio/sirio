#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT="$SCRIPT_DIR/../check-cycle-version.sh"

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# The check reads its tag list from the repository one level above the
# directory holding the script (`cd "$(dirname "$0")/.."`), so the only way to
# choose those tags without touching this repo is to give the script a
# repository of its own: copy it into the same Scripts/ layout under a
# temporary git repo, with a rust/Cargo.toml beside it, and run that copy in
# place. No test-only override env var, so what runs here is the path ci.sh
# runs -- a shortcut nobody takes in production would verify nothing.
REPO="$TMP/repo"
mkdir -p "$REPO/Scripts" "$REPO/rust"
cp "$SCRIPT" "$REPO/Scripts/check-cycle-version.sh"
CHECK="$REPO/Scripts/check-cycle-version.sh"

git -C "$REPO" init -q
git -C "$REPO" -c user.name=test -c user.email=test@example.invalid \
  commit -q --allow-empty -m "fixture"

version() {
  printf '[workspace]\nmembers = []\n\n[workspace.package]\nversion = "%s"\n' "$1" >"$REPO/rust/Cargo.toml"
}

set_tags() {
  if [ -n "$(git -C "$REPO" tag)" ]; then
    # shellcheck disable=SC2046  # one argument per tag is the point; tag names cannot hold whitespace
    git -C "$REPO" tag --delete $(git -C "$REPO" tag) >/dev/null
  fi
  for tag in "$@"; do
    git -C "$REPO" tag "$tag"
  done
}

# `env -u`: this test is in charge of SIRIO_CI_RELEASE_GATE. Left ambient, a
# release-gate environment would turn every refusal below into a skip and the
# suite would pass without checking anything.
run() {
  env -u SIRIO_CI_RELEASE_GATE bash "$CHECK" "$@"
}

# --- no v*.*.* tag at all: nothing to compare against, so skip ----------------
version 0.13.3
OUT=$(run 2>&1)
case "$OUT" in
*skipping*) ;;
*) fail "a checkout with no release tag must skip, got: $OUT" ;;
esac

# --- a version equal to the highest tag: refused, with the fix named ---------
set_tags v0.13.3
version 0.13.3
if ERR=$(run 2>&1); then
  fail "a version equal to the highest tag must be refused, got: $ERR"
fi
case "$ERR" in *"0.13.3"*) ;; *) fail "the refusal must name the version found, got: $ERR" ;; esac
case "$ERR" in *"v0.13.3"*) ;; *) fail "the refusal must name the highest tag, got: $ERR" ;; esac
case "$ERR" in *"set-workspace-version.sh"*) ;; *) fail "the refusal must name the fix, got: $ERR" ;; esac

# --- a version below the highest tag: refused --------------------------------
version 0.12.0
if ERR=$(run 2>&1); then
  fail "a version below the highest tag must be refused, got: $ERR"
fi

# --- a version above the highest tag: passes, and prints the version ---------
version 0.14.0
OUT=$(run)
[ "$OUT" = "0.14.0" ] || fail "expected the version on stdout, got: $OUT"

# --- SIRIO_CI_RELEASE_GATE=1 skips even a version below the tag: that is the -
# release checkout, where version == tag by construction, and refusing there
# would break every release.
version 0.9.0
OUT=$(SIRIO_CI_RELEASE_GATE=1 bash "$CHECK" 2>&1)
case "$OUT" in
*skipping*) ;;
*) fail "SIRIO_CI_RELEASE_GATE=1 must skip, got: $OUT" ;;
esac

# --- tags are ordered by version, not by bytes: 0.9.7 is above 0.9.6 but -----
# below 0.9.10, and a plain `sort` would call it the highest.
set_tags v0.9.6 v0.9.10
version 0.9.7
if ERR=$(run 2>&1); then
  fail "0.9.7 is below the highest tag 0.9.10 and must be refused, got: $ERR"
fi
version 0.9.11
OUT=$(run)
[ "$OUT" = "0.9.11" ] || fail "0.9.11 is above v0.9.10 and must pass, got: $OUT"

# --- only a nightly tag, which is not a release tag: nothing to compare ------
set_tags nightly-202609130804
version 0.13.3
OUT=$(run 2>&1)
case "$OUT" in
*skipping*) ;;
*) fail "a nightly tag alone must skip, got: $OUT" ;;
esac

# --- the optional Cargo.toml argument is honored -----------------------------
set_tags v0.13.3
version 0.13.3
printf '[workspace]\n\n[workspace.package]\nversion = "0.14.0"\n' >"$TMP/Elsewhere.toml"
OUT=$(run "$TMP/Elsewhere.toml")
[ "$OUT" = "0.14.0" ] || fail "the Cargo.toml path argument must be read, got: $OUT"

# --- a file without a workspace version line is refused, not silently passed -
printf '[workspace]\nmembers = []\n' >"$TMP/NoVersion.toml"
if ERR=$(run "$TMP/NoVersion.toml" 2>&1); then
  fail "a Cargo.toml without a workspace version must be refused, got: $ERR"
fi
case "$ERR" in
*"could not find a workspace version"*) ;;
*) fail "a missing version line must be reported by name, got: $ERR" ;;
esac

echo "PASS: cycle version check refuses a version not above the highest release tag"
