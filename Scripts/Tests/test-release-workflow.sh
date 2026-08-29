#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKFLOW="$SCRIPT_DIR/../../.github/workflows/release.yml"

if [ ! -f "$WORKFLOW" ]; then
  echo "FAIL: $WORKFLOW not found" >&2
  exit 1
fi

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

grep -q "^  macos:"   "$WORKFLOW" || fail "no macos job"
grep -q "^  linux:"   "$WORKFLOW" || fail "no linux job"
grep -q "^  windows:" "$WORKFLOW" || fail "no windows job"

# macOS gates the release: both other platforms must depend on it. A parallel
# matrix would treat the three as peers, which is precisely the decision this
# workflow exists to encode.
if [ "$(grep -c "needs: macos" "$WORKFLOW")" -lt 2 ]; then
  fail "linux and windows must both declare 'needs: macos'"
fi

# The binary links against the runner's glibc, which becomes users' minimum.
# `ubuntu-latest` moves that floor without warning when GitHub rotates images.
grep -q "ubuntu-22.04" "$WORKFLOW" || fail "the linux runner must be pinned to ubuntu-22.04"
# Written as an `if` rather than `grep ... && fail`: under `set -e` a compound
# whose first command fails takes down the whole script, so the `&&` form would
# exit non-zero exactly when ubuntu-latest is correctly absent.
if grep -q "ubuntu-latest" "$WORKFLOW"; then
  fail "ubuntu-latest must not be used"
fi

# libghostty-vt-sys shells out to `zig build`, and upstream pins 0.15.2
# exactly -- a newer Zig fails too.
if [ "$(grep -c "version: 0.15.2" "$WORKFLOW")" -lt 2 ]; then
  fail "both hosted runners must pin Zig 0.15.2"
fi

# rust/.cargo/config.toml sets `rustc-wrapper = "sccache"` unconditionally, so
# the linux runner must install it or the job fails before compiling (spec §1.5).
grep -q "sccache" "$WORKFLOW" || fail "the release workflow must install sccache"

# The linux job builds the AppImage and nothing else: webkit2gtk-4.1 must be
# installed on the runner so build-appimage.sh can bundle its closure, and no
# step may still produce the old tarball.
grep -q "libwebkit2gtk-4.1-dev" "$WORKFLOW" || fail "the linux job must install libwebkit2gtk-4.1-dev"
grep -qF "Sirio-\${VERSION}-x86_64.AppImage" "$WORKFLOW" || fail "the linux artifact must be Sirio-<version>-x86_64.AppImage"
if grep -q "tar.gz" "$WORKFLOW"; then
  fail "the linux job must no longer produce a tarball"
fi

grep -q "options runtime\|build-app-bundle.sh" "$WORKFLOW" || fail "no app bundling step"
grep -q "notarytool submit"                    "$WORKFLOW" || fail "no notarization step"
grep -q "stapler staple"                       "$WORKFLOW" || fail "no stapling step"

echo "PASS: release workflow structure"
