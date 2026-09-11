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
# exactly -- a newer Zig fails too. All three runners download it.
if [ "$(grep -c "version: 0.15.2" "$BUILD")" -lt 3 ]; then
  fail "all three runners must pin Zig 0.15.2"
fi
if [ "$(grep -c "uses: mlugg/setup-zig" "$BUILD")" -ne 3 ]; then
  fail "all three runners must install Zig through setup-zig"
fi

# The macOS image is pinned, and this is the assertion that keeps it pinned.
# The downloaded Zig 0.15.2 links against its own bundled libSystem stub, which
# is older than macOS 26: on a macOS 26 host every libc symbol comes back
# undefined and `zig build` dies inside libghostty-vt-sys, naming none of this.
# That is why the job ran on a self-hosted Mac with Homebrew's Zig until
# 2026-09-11, and it is why `macos-latest` must never appear here -- the day
# GitHub points it at 26, an unpinned label walks into that failure on its own
# schedule.
grep -q "runs-on: macos-15" "$BUILD" \
  || fail "the macos job must pin macos-15: the bundled Zig libSystem stub predates macOS 26"
# Targets `runs-on:` rather than the file, because the comments explaining the
# pin necessarily name the label they are warning against -- the same reason
# test-build-inno.sh's WebView2 assertion matches install entries and not prose.
if grep -qE '^\s*runs-on:.*macos-latest' "$BUILD"; then
  fail "macos-latest must not be used: it can move to macOS 26, where the downloaded Zig cannot link"
fi

# rust/.cargo/config.toml sets `rustc-wrapper = "sccache"` unconditionally, so
# every job that runs cargo -- macos, linux, windows, and publish, which builds
# the signing CLI -- must install it or fail before compiling (spec §1.5).
if [ "$(grep -c "mozilla-actions/sccache-action" "$BUILD")" -lt 4 ]; then
  fail "macos, linux, windows and publish must all install sccache"
fi

# The macOS job is the only one that runs Scripts/ci.sh, and the gate runs the
# workspace's tests through nextest. Without it the failure is `no such
# subcommand: nextest`, arriving after the gate has already paid for its own
# compile.
grep -q "get.nexte.st" "$BUILD" || fail "the macos job must install cargo-nextest"

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

# The CPU floor belongs in the same place and for the same reason. Every job
# builds natively, so without an explicit floor the Zig-built VT parser is
# compiled for whichever CPU the runner happens to have -- which is how v0.9.6
# shipped a Windows binary that died with STATUS_ILLEGAL_INSTRUCTION on a Zen 3
# desktop. `baseline` is also what the Rust half compiles at, so the two halves
# of the binary agree on the floor.
grep -qE '^  LIBGHOSTTY_VT_SYS_CPU: baseline$' "$BUILD" \
  || fail "LIBGHOSTTY_VT_SYS_CPU must be pinned to baseline at workflow level"
CPU_LINE=$(line_of 'LIBGHOSTTY_VT_SYS_CPU: baseline' "$BUILD")
[ "$CPU_LINE" -lt "$JOBS_LINE" ] || fail "the CPU floor env must be workflow-level, above jobs:"

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

# ...but on macOS, after the gate. The gate tests the code; the version is
# packaging, and a `-nightly.<stamp>` version fails three tests that encode the
# shape of a release version rather than of any version. Stamping first turns
# every nightly red for a reason that says nothing about the build (#317).
GATE_LINE=$(line_of 'run: Scripts/ci.sh' "$BUILD")
STAMP_LINE=$(line_of 'run: Scripts/set-workspace-version.sh "$VERSION"' "$BUILD")
[ -n "$GATE_LINE" ]  || fail "the macos job must run the CI gate"
[ -n "$STAMP_LINE" ] || fail "the macos job must stamp the workspace version"
[ "$GATE_LINE" -lt "$STAMP_LINE" ] \
  || fail "the CI gate must run before the version is stamped, not after"

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

# Nothing is public until the signatures verify: the release is created as a
# draft and un-drafted after verification, before the manifest names its
# assets. A failure in between leaves a draft to retry, not a public release
# with no manifest that §9.3 forbids deleting -- and a re-run re-uploads into
# the existing release instead of failing on a second `create`.
grep -qF -- '--draft \' "$BUILD" || fail "the release must be created as a draft"
grep -qF 'gh release upload "$TAG" artifacts/* --clobber' "$BUILD" || fail "a re-run must re-upload into the existing release"
UNDRAFT_LINE=$(line_of 'gh release edit "$TAG" --draft=false' "$BUILD")
[ -n "$UNDRAFT_LINE" ] || fail "publish must un-draft the release after verification"
[ "$VERIFY_LINE" -lt "$UNDRAFT_LINE" ] || fail "the release must go public only after the manifest verifies"
[ "$UNDRAFT_LINE" -lt "$PAGES_LINE" ]  || fail "the release must be public before the manifest is pushed"

# Re-signing the same bytes yields the same manifest; that is "already
# published", not a failed commit. A rejected push is rebased and retried.
grep -q "git diff --cached --quiet" "$BUILD" || fail "an unchanged manifest must not fail the publish"
grep -q "git rebase FETCH_HEAD" "$BUILD" || fail "a rejected gh-pages push must be rebased and retried"

# A green push is not a served manifest: the branch is read back through the
# API and the URL the binaries poll is polled for the version, after the push.
READBACK_LINE=$(line_of '.json?ref=gh-pages' "$BUILD")
[ -n "$READBACK_LINE" ] || fail "publish must read the manifest back from gh-pages"
[ "$PAGES_LINE" -lt "$READBACK_LINE" ] || fail "the read-back must come after the push"

# The host polled must be the one compiled into every install, or the check
# goes green while installs get a 404. The constant lives in sirio_update.
UPDATE_LIB="$SCRIPT_DIR/../../rust/crates/sirio_update/src/lib.rs"
COMPILED_HOST=$(grep -E '^const MANIFEST_HOST: &str = "' "$UPDATE_LIB" | sed -E 's/.*"([^"]+)".*/\1/')
[ -n "$COMPILED_HOST" ] || fail "could not read MANIFEST_HOST from $UPDATE_LIB"
grep -qF "MANIFEST_URL=\"${COMPILED_HOST}/\${CHANNEL}.json\"" "$BUILD" \
  || fail "publish must poll ${COMPILED_HOST}/<channel>.json, the URL compiled into the binaries"
POLL_LINE=$(line_of 'curl -fsSL "$MANIFEST_URL"' "$BUILD")
[ -n "$POLL_LINE" ] || fail "publish must poll the served manifest"
[ "$READBACK_LINE" -lt "$POLL_LINE" ] || fail "the served-manifest poll must come after the branch read-back"

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
READ_SCOPES=$(grep -c "contents: read" "$BUILD" || true)
[ "$READ_SCOPES" -ge 3 ] || fail "each of the three build jobs must be scoped to contents: read (found $READ_SCOPES)"

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
