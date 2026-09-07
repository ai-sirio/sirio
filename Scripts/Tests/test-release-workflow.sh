#!/bin/bash
# Structure test for the stable release path: release.yml (the tag-triggered
# caller) and build-release.yml (the reusable workflow both channels share).
# The nightly caller has its own test, test-nightly-workflow.sh.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RELEASE="$SCRIPT_DIR/../../.github/workflows/release.yml"
BUILD="$SCRIPT_DIR/../../.github/workflows/build-release.yml"

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

[ -f "$RELEASE" ] || fail "$RELEASE not found"
[ -f "$BUILD" ]   || fail "$BUILD not found"

# Line number of the first match, for ordering assertions. Prints nothing on
# no match rather than failing: under `set -e` a failing `$(...)` in an
# assignment would exit the script before the `-n` check names what is missing.
line_of() {
  { grep -n -F -- "$1" "$2" || true; } | head -1 | cut -d: -f1
}

# ---------------------------------------------------------------------------
# build-release.yml: the shared build and publish path
# ---------------------------------------------------------------------------

grep -q "workflow_call:" "$BUILD" || fail "build-release.yml must be a reusable workflow (workflow_call)"
for input in channel version tag notes prerelease publish; do
  grep -qE "^      $input:" "$BUILD" || fail "build-release.yml must declare the '$input' input"
done

grep -q "^  macos:"   "$BUILD" || fail "no macos job"
grep -q "^  linux:"   "$BUILD" || fail "no linux job"
grep -q "^  windows:" "$BUILD" || fail "no windows job"
grep -q "^  publish:" "$BUILD" || fail "no publish job"

# macOS gates the release: both other platforms must depend on it. A parallel
# matrix would treat the three as peers, which is precisely the decision this
# workflow exists to encode.
if [ "$(grep -c "needs: macos" "$BUILD")" -lt 2 ]; then
  fail "linux and windows must both declare 'needs: macos'"
fi

# The binary links against the runner's glibc, which becomes users' minimum.
# `ubuntu-latest` moves that floor without warning when GitHub rotates images.
grep -q "ubuntu-22.04" "$BUILD" || fail "the linux runner must be pinned to ubuntu-22.04"
# Written as an `if` rather than `grep ... && fail`: under `set -e` a compound
# whose first command fails takes down the whole script, so the `&&` form would
# exit non-zero exactly when ubuntu-latest is correctly absent.
if grep -q "ubuntu-latest" "$BUILD"; then
  fail "ubuntu-latest must not be used"
fi

# libghostty-vt-sys shells out to `zig build`, and upstream pins 0.15.2
# exactly -- a newer Zig fails too.
if [ "$(grep -c "version: 0.15.2" "$BUILD")" -lt 2 ]; then
  fail "both hosted runners must pin Zig 0.15.2"
fi

# rust/.cargo/config.toml sets `rustc-wrapper = "sccache"` unconditionally, so
# every hosted job that runs cargo -- linux, windows, and publish, which builds
# the signing CLI -- must install it or fail before compiling (spec §1.5).
if [ "$(grep -c "mozilla-actions/sccache-action" "$BUILD")" -lt 3 ]; then
  fail "linux, windows and publish must all install sccache"
fi

# --- spec §3.3 / §3.5: the channel and the accepted keys reach the compiler ---
# Workflow-level env applies to every step of every job, so the CI gate, the
# release compile and the packaging scripts all see the same values. A step-
# level env on one cargo call would let the next refactor drop it silently --
# the Zed post-mortem bug.
grep -qF 'SIRIO_RELEASE_CHANNEL: ${{ inputs.channel }}' "$BUILD" \
  || fail "SIRIO_RELEASE_CHANNEL must be set from inputs.channel at workflow level"
grep -qF 'SIRIO_RELEASE_ACCEPTED_KEYS: ${{ vars.SIRIO_RELEASE_ACCEPTED_KEYS }}' "$BUILD" \
  || fail "SIRIO_RELEASE_ACCEPTED_KEYS must be set from the repository variable at workflow level"
CHANNEL_LINE=$(line_of 'SIRIO_RELEASE_CHANNEL: ${{ inputs.channel }}' "$BUILD")
JOBS_LINE=$(line_of 'jobs:' "$BUILD")
[ "$CHANNEL_LINE" -lt "$JOBS_LINE" ] || fail "the channel env must be workflow-level, above jobs:"

# An empty key set is fail-closed in the app (spec §4.2): every download fails
# verification, which from the outside is indistinguishable from a dead
# updater. Refuse to build at all.
grep -q "SIRIO_RELEASE_ACCEPTED_KEYS is empty" "$BUILD" \
  || fail "the build must refuse to run when SIRIO_RELEASE_ACCEPTED_KEYS is empty"

# A malformed key degrades to an empty set at runtime, which is fail-closed
# and therefore invisible; the guard decodes every entry, not just non-emptiness.
grep -q "openssl base64 -d" "$BUILD" || fail "the key guard must decode each accepted key"

# The version compiled into every binary is the one the manifest will carry:
# each build job rewrites the workspace version before compiling.
if [ "$(grep -c "Scripts/set-workspace-version.sh" "$BUILD")" -lt 3 ]; then
  fail "macos, linux and windows must each run Scripts/set-workspace-version.sh"
fi

# Spec §3.5 on the artifact, not only on the gate's test build: every job
# asks the sirioctl it just built what channel and version it carries.
if [ "$(grep -c "Scripts/assert-built-channel.sh" "$BUILD")" -lt 3 ]; then
  fail "macos, linux and windows must each run Scripts/assert-built-channel.sh on the built sirioctl"
fi
ASSERT_LINE=$(line_of 'Scripts/assert-built-channel.sh rust/target/aarch64-apple-darwin' "$BUILD")
BUNDLE_LINE=$(line_of 'Scripts/build-app-bundle.sh' "$BUILD")
[ -n "$ASSERT_LINE" ] || fail "the macos job must assert the channel on the aarch64-apple-darwin sirioctl"
[ "$ASSERT_LINE" -lt "$BUNDLE_LINE" ] || fail "the channel must be asserted before the binary is bundled and signed"

# The linux job builds the AppImage and nothing else: webkit2gtk-4.1 must be
# installed on the runner so build-appimage.sh can bundle its closure, and no
# step may still produce the old tarball.
grep -q "libwebkit2gtk-4.1-dev" "$BUILD" || fail "the linux job must install libwebkit2gtk-4.1-dev"
grep -qF "Sirio-\${VERSION}-x86_64.AppImage" "$BUILD" || fail "the linux artifact must be Sirio-<version>-x86_64.AppImage"
if grep -q "tar.gz" "$BUILD"; then
  fail "the linux job must no longer produce a tarball"
fi

grep -q "options runtime\|build-app-bundle.sh" "$BUILD" || fail "no app bundling step"
grep -q "notarytool submit"                    "$BUILD" || fail "no notarization step"
grep -q "stapler staple"                       "$BUILD" || fail "no stapling step"

# The Windows release is an Inno Setup installer, not a zip: the .iss is
# generated from the identity source, compiled with the ISCC.exe that ships
# with the windows-latest runner image, and uploaded as SirioSetup-<version>.exe.
grep -q "Scripts/build-inno.sh" "$BUILD" || fail "the windows job must generate the .iss from the identity source"
grep -q "ISCC.exe"              "$BUILD" || fail "the windows job must compile the .iss with ISCC"
grep -q "SirioSetup-"           "$BUILD" || fail "no SirioSetup-<version>.exe artifact name"
grep -q "name: windows-setup"   "$BUILD" || fail "no windows-setup artifact upload"
if grep -q "windows-zip" "$BUILD"; then
  fail "the windows zip upload is gone"
fi

# --- spec §9.1: upload every asset first, write the manifest last -------------
# Verified by ordering in the file: the release (with its assets) is created,
# the asset list is read back and compared, the artifacts are signed, and only
# then does the manifest get pushed to the branch the manifest host serves.
# The patterns are the command lines themselves, not their names, so a
# comment that mentions a step cannot satisfy its ordering check.
CREATE_LINE=$(line_of 'gh release create "$TAG"' "$BUILD")
ASSETS_LINE=$(line_of '--json assets --jq' "$BUILD")
SIGN_LINE=$(line_of 'sirio-release sign \' "$BUILD")
VERIFY_LINE=$(line_of 'sirio-release verify \' "$BUILD")
PAGES_LINE=$(line_of 'git push origin HEAD:gh-pages' "$BUILD")
[ -n "$CREATE_LINE" ] || fail "publish must create the GitHub release with gh release create"
[ -n "$ASSETS_LINE" ] || fail "publish must read the release's asset list back before signing"
[ -n "$SIGN_LINE" ]   || fail "publish must sign the artifacts with sirio-release sign"
[ -n "$VERIFY_LINE" ] || fail "publish must verify the manifest with sirio-release verify"
[ -n "$PAGES_LINE" ]  || fail "publish must push the manifest to the gh-pages branch"
[ "$CREATE_LINE" -lt "$ASSETS_LINE" ] || fail "assets must be uploaded before they are checked"
[ "$ASSETS_LINE" -lt "$SIGN_LINE" ]   || fail "assets must be checked before the manifest is signed"
[ "$SIGN_LINE" -lt "$VERIFY_LINE" ]   || fail "the manifest must be verified after it is signed"
[ "$VERIFY_LINE" -lt "$PAGES_LINE" ]  || fail "the manifest must be published last, after verification"

# A green push is not a served manifest: the branch is read back through the
# API and the Pages site is polled for the version, after the push.
READBACK_LINE=$(line_of '.json?ref=gh-pages' "$BUILD")
[ -n "$READBACK_LINE" ] || fail "publish must read the manifest back from gh-pages"
[ "$PAGES_LINE" -lt "$READBACK_LINE" ] || fail "the read-back must come after the push"
grep -qF 'repos/${GITHUB_REPOSITORY}/pages' "$BUILD" || fail "publish must look up the Pages site to poll it"
grep -q "pages: read" "$BUILD" || fail "publish needs pages: read to look up the Pages site"
grep -q "pages: read" "$RELEASE" || fail "release.yml must grant pages: read to the called workflow"

# The artifacts live in GitHub Releases under their packaging-script names, so
# the URL template names the file rather than reconstructing it per platform.
grep -qF 'releases/download/' "$BUILD" || fail "artifact URLs must point at GitHub Releases"
grep -qF '{file}' "$BUILD" || fail "the url-template must use the {file} placeholder"

# The manifest is signed with the private key from the secret and then checked
# against the *public* keys the binaries were compiled with: a key the app
# does not accept would publish a manifest every install rejects.
grep -qF 'secrets.SIRIO_RELEASE_SIGNING_KEY' "$BUILD" || fail "signing must use the SIRIO_RELEASE_SIGNING_KEY secret"
grep -qF -- '--pub-key' "$BUILD" || fail "verification must pass the accepted public keys"

# The manifest file is named after the channel, so a nightly publish can never
# overwrite stable.json; both channels share one branch, so publishes queue.
grep -qF 'CHANNEL: ${{ inputs.channel }}' "$BUILD" || fail "publish must bind CHANNEL to inputs.channel"
grep -qF '"$CHANNEL.json"' "$BUILD" || fail "the manifest must be named <channel>.json"
grep -q "concurrency:" "$BUILD" || fail "publish must serialise manifest pushes with a concurrency group"

# Only the one manifest is added on the branch: the other channel's file and
# the CNAME GitHub Pages writes for the custom domain must survive.
if grep -qE "git (rm|clean)" "$BUILD"; then
  fail "publish must not remove files from the gh-pages branch"
fi
if grep -q "push --force\|push -f" "$BUILD"; then
  fail "publish must not force-push the gh-pages branch"
fi

# Write scope belongs to publish alone; build jobs run ~400 crates' build
# scripts and get read only.
grep -q "contents: write" "$BUILD" || fail "publish needs contents: write"
grep -q "contents: read"  "$BUILD" || fail "build jobs must be scoped to contents: read"

# ---------------------------------------------------------------------------
# release.yml: the stable caller
# ---------------------------------------------------------------------------

grep -q "v\*\.\*\.\*" "$RELEASE" || fail "release.yml must trigger on v*.*.* tags"
grep -qF "uses: ./.github/workflows/build-release.yml" "$RELEASE" \
  || fail "release.yml must call the reusable build workflow"
grep -qF "channel: stable" "$RELEASE" || fail "release.yml must build the stable channel"
grep -q "secrets: inherit" "$RELEASE" || fail "release.yml must pass secrets to the reusable workflow"

# §9.2: a tag that disagrees with rust/Cargo.toml is refused, before any build.
grep -q "Scripts/check-release-version.sh" "$RELEASE" || fail "release.yml must run check-release-version.sh"

# §7.4: notes are hand-written, one file per version, required. Seeded from
# generate-changelog.sh, never published raw.
grep -qF 'docs/release-notes/' "$RELEASE" || fail "release.yml must read docs/release-notes/<version>.md"
grep -q "no hand-written release notes" "$RELEASE" || fail "release.yml must refuse a release without its notes file"

# The manual trigger validates a branch and publishes nothing.
grep -qF "publish: \${{ github.event_name == 'push' }}" "$RELEASE" \
  || fail "release.yml must publish only on a tag push"

# The build jobs' tests are the workflow's own gate; every script test runs.
for test in test-set-workspace-version.sh test-assert-built-channel.sh test-release-workflow.sh test-nightly-workflow.sh; do
  grep -q "Scripts/Tests/$test" "$BUILD" || fail "build-release.yml must run Scripts/Tests/$test"
done

echo "PASS: release workflow structure"
