# macOS Release Platform Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make macOS Sirio's reference release platform, producing a signed and notarized DMG on every version tag, with Linux and Windows artifacts released behind it.

**Architecture:** Three bash scripts in `Scripts/` split the release into independently runnable pieces — version check, `.app` bundling, DMG packaging — and `.github/workflows/release.yml` wires them into four jobs where Linux and Windows declare `needs: macos` and a fourth publishes what all three produced. Two of the three scripts already exist in git history and come back nearly verbatim; only the `.app` bundling is new, replacing what Xcode used to do.

**Tech Stack:** bash, GitHub Actions, `codesign`/`notarytool`/`stapler` (Xcode command line tools), `hdiutil`, cargo, Zig 0.15.2.

**Spec:** `docs/superpowers/specs/2026-08-29-macos-release-platform-design.md`

## Global Constraints

- **Zig exactly 0.15.2** must be on `PATH` for anything that builds `sirio_terminal`. A newer Zig fails too.
- **macOS target: `aarch64-apple-darwin` only.** No universal binary, no Intel.
- **Bundle identifier: `dev.sirio.Sirio`.** Bundle name `Sirio.app`, executable `sirio`.
- **`LSMinimumSystemVersion`: `15.0`.**
- **Version source of truth:** `rust/Cargo.toml`, `[workspace.package] version` (line 27, `0.6.0` today). It is the only line in that file starting with `version = `; every dependency version is inline inside `{ ... }`.
- **Repository:** `ai-sirio/sirio`. No tags exist yet, so the first release tag is `v0.6.0`.
- **Binaries:** `sirio` (crate `sirio`) and `sirioctl` (crate `sirio_control`).
- **Script style:** `#!/bin/bash`, `set -euo pipefail`, a `usage()` that exits 2, errors to stderr prefixed `error:`, the result echoed on stdout on success.
- **Test style:** `Scripts/Tests/test-<name>.sh`, a `mktemp -d` fixture with `trap 'rm -rf "$FIXTURE"' EXIT`, assertions that `echo "FAIL: ..." >&2; exit 1`, ending with `echo "PASS: <what>"`. Executables that exist only on macOS are stubbed into `$FIXTURE/bin` and put first on `PATH` — the pattern `Scripts/Tests/test-ci.sh` already uses for `cargo`.
- **Commits:** Conventional Commits, lower-case imperative subject.
- **`Scripts/ci.sh` must print `CI OK`** before a PR is opened.
- **Every new script must be committed executable**, and `chmod +x` alone is not enough: this checkout has `core.fileMode = false`, so the mode never reaches the git object. Use `git update-index --chmod=+x <file>` and confirm with `git ls-files -s` that it reads `100755`, matching every existing script in `Scripts/`. A script committed `100644` fails on Linux and macOS with "Permission denied" while working fine on Windows.

## File Structure

| File | Responsibility |
|---|---|
| `Scripts/check-release-version.sh` | Restored from `c47234de^`, retargeted: fails unless the tag matches the workspace version. Echoes the version. |
| `Scripts/Tests/test-check-release-version.sh` | New. Match, mismatch, missing file. |
| `Scripts/build-dmg.sh` | Restored from `c47234de^` verbatim. Stages an `.app` with an `/Applications` symlink, runs `hdiutil`. |
| `Scripts/Tests/test-build-dmg.sh` | New. Asserts the staged layout with `hdiutil` stubbed. |
| `Scripts/build-app-bundle.sh` | New. Wraps an already-compiled binary into a signed `Sirio.app`. |
| `Scripts/Tests/test-build-app-bundle.sh` | New. Asserts bundle layout, `Info.plist` keys and the hardened-runtime flag, with `codesign` stubbed. |
| `Scripts/Tests/test-release-workflow.sh` | New. Asserts the job graph and the pins that silently rot. |
| `.github/workflows/release.yml` | Rewritten: four jobs (`macos`, `linux`, `windows`, `publish`), `needs: macos`. |
| `Scripts/generate-changelog.sh` | Modified: caps the untagged first release at 100 commits, so the body stays under GitHub's 125,000-character limit. |
| `Scripts/Tests/test-generate-changelog.sh` | Extended: asserts the untagged case is bounded and says how many commits were omitted. |
| `Scripts/ci-linux.sh` | Modified: runs the five new tests beside the ones it already runs. |
| `CLAUDE.md` | Modified: "What this is" and "Commands". |

Each script takes an artifact and produces the next one, so every stage is runnable by hand against the previous stage's output. That is what makes a failed release debuggable without re-running the whole pipeline.

---

### Task 1: Phase 0 — verify the workspace builds on macOS

> **Ordering amended 2026-08-29, by the maintainer's decision.** This task was
> specified as gating every other one. It is now deferred: Tasks 2-6 are built
> first, and this task becomes the gate before the **first release tag** rather
> than before the first line of code. The reason is availability — Task 1 needs
> Mac hardware, and nothing else in the plan does, because every script test
> stubs the macOS-only executable it drives.
>
> What this trades away is real and worth naming: Tasks 2-6 are written against
> a workspace that has never compiled for macOS. If Phase 0 uncovers something
> structural, some of that work is rewritten. The bet is that the three risks
> below are localized (a manifest, a tray, a linker flag) rather than shaped
> like a redesign. **Do not push a `v*.*.*` tag until this task is green.**

**The workspace has never been compiled for macOS.** Nothing below is verified until this is green.

**Files:**
- Modify: whatever the build reveals is broken (unknown until run)

**Interfaces:**
- Consumes: nothing
- Produces: a macOS host where `Scripts/ci.sh` prints `CI OK`; `rust/target/aarch64-apple-darwin/release/sirio` becomes buildable, which every later task assumes

- [ ] **Step 1: Install Zig 0.15.2 on the Mac and verify the exact version**

```bash
# Homebrew's `zig` formula tracks latest and will install the wrong version.
# Download the pinned build directly.
curl -fsSL -o /tmp/zig.tar.xz https://ziglang.org/download/0.15.2/zig-aarch64-macos-0.15.2.tar.xz
mkdir -p ~/toolchains/zig && tar -xf /tmp/zig.tar.xz -C ~/toolchains/zig
export PATH="$HOME/toolchains/zig/zig-aarch64-macos-0.15.2:$PATH"
zig version
```

Expected: `0.15.2` exactly. Anything else, including newer, will fail the build later.

- [ ] **Step 2: Add the Rust target**

```bash
rustup target add aarch64-apple-darwin
```

- [ ] **Step 3: Run the gate and capture the outcome**

```bash
cd /path/to/sirio
Scripts/ci.sh 2>&1 | tee /tmp/phase0.log
```

Expected on success: `CI OK`.

- [ ] **Step 4: If it failed, fix and re-run**

Three places this is expected to break, in descending order of likelihood. Each is an ordinary code fix, not a design change:

1. **`[patch."https://github.com/zed-industries/zed"]`** (`rust/Cargo.toml:89-91`) replaces `gpui_platform` with the local fork in `rust/vendor/gpui_platform`, unconditionally. That fork has only ever been resolved on Linux and Windows. If resolution or compilation fails here, compare `rust/vendor/gpui_platform/Cargo.toml` against the upstream manifest at the pinned rev `c05e34637b4f7f100a688bf6ac71cb70877fc8ad` and add whatever macOS-conditional section the fork dropped.
2. **The objc2 tray** (`rust/crates/sirio/Cargo.toml:56-76`, `rust/crates/sirio/src/tray.rs`) — `NSStatusItem` compiled for the first time. The objc2 versions are deliberately pinned to the ones gpui links, so a mismatch here means gpui's own objc2 version moved.
3. **`#[link(name = "proc")]`** (`rust/crates/sirio_activity/src/process.rs:188`) — the libproc link has never been exercised by a real linker. `proc_listchildpids` and `proc_name` are in `libproc.dylib`, present on every macOS.

Re-run Step 3 after each fix.

- [ ] **Step 5: Commit any fixes**

```bash
git add -A
git commit -m "fix: build the workspace on macOS"
```

Skip this step if `Scripts/ci.sh` was green with no changes.

- [ ] **Step 6: Record the result in the spec**

Append to `docs/superpowers/specs/2026-08-29-macos-release-platform-design.md`, replacing the "Phase 0 is unverified" risk with what actually happened — which of the three risks materialized, and what fixed it. A future reader needs to know whether this was smooth or not.

```bash
git add docs/superpowers/specs/2026-08-29-macos-release-platform-design.md
git commit -m "docs: record the phase 0 macOS build outcome"
```

---

### Task 2: Restore and retarget `check-release-version.sh`

**Files:**
- Create: `Scripts/check-release-version.sh` (restored from `c47234de^`, retargeted)
- Create: `Scripts/Tests/test-check-release-version.sh`

**Interfaces:**
- Consumes: nothing
- Produces: `Scripts/check-release-version.sh <tag> [cargo-toml-path]` — exits 0 and echoes the bare version (`0.6.0`) when the tag matches; exits non-zero otherwise. `release.yml` calls it in Task 5.

- [ ] **Step 1: Write the failing test**

Create `Scripts/Tests/test-check-release-version.sh`:

```bash
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

# Covers the branch the `|| true` above makes reachable. Without that guard this
# path exits 1 with empty stderr, so asserting the exit code alone would pass
# against the broken version too — the `error:` prefix is the real assertion.
printf '[workspace]\nresolver = "2"\n' > "$FIXTURE/no-version.toml"
if STDERR=$("$CHECK_SCRIPT" "v0.6.0" "$FIXTURE/no-version.toml" 2>&1 >/dev/null); then
  echo "FAIL: a Cargo.toml with no workspace version must exit non-zero" >&2
  exit 1
fi
case "$STDERR" in
  *error:*) ;;
  *) echo "FAIL: expected an 'error:' message on stderr, got '$STDERR'" >&2; exit 1 ;;
esac

echo "PASS: release version check"
```

```bash
chmod +x Scripts/Tests/test-check-release-version.sh
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `bash Scripts/Tests/test-check-release-version.sh`
Expected: FAIL — the script does not exist yet, so the first invocation errors with "No such file or directory".

- [ ] **Step 3: Write the script**

Create `Scripts/check-release-version.sh`:

```bash
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
# The `|| true` is load-bearing. Without it a `grep` that matches nothing makes
# the whole substitution fail under `set -e` + `pipefail`, killing the script on
# this line — before the `-z` guard below can report anything. The failure would
# be exit 1 with completely empty stderr.
CARGO_VERSION=$(grep -m1 '^version = ' "$CARGO_TOML" | sed -E 's/^version = "([^"]+)".*/\1/' || true)

if [ -z "$CARGO_VERSION" ]; then
  echo "error: could not find a workspace version in $CARGO_TOML" >&2
  exit 1
fi

if [ "$TAG_VERSION" != "$CARGO_VERSION" ]; then
  echo "error: tag version '$TAG_VERSION' does not match workspace version '$CARGO_VERSION' in $CARGO_TOML" >&2
  exit 1
fi

echo "$CARGO_VERSION"
```

```bash
chmod +x Scripts/check-release-version.sh
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `bash Scripts/Tests/test-check-release-version.sh`
Expected: `PASS: release version check`

- [ ] **Step 5: Verify it agrees with the real manifest**

```bash
Scripts/check-release-version.sh v0.6.0
```

Expected: `0.6.0`

- [ ] **Step 6: Commit**

```bash
git add Scripts/check-release-version.sh Scripts/Tests/test-check-release-version.sh
git commit -m "feat(release): check the tag against the workspace version"
```

---

### Task 3: Restore `build-dmg.sh`

**Files:**
- Create: `Scripts/build-dmg.sh` (restored verbatim from `c47234de^`)
- Create: `Scripts/Tests/test-build-dmg.sh`

**Interfaces:**
- Consumes: an `.app` directory, from `Scripts/build-app-bundle.sh` in Task 4
- Produces: `Scripts/build-dmg.sh <app-path> <volume-name> <output-dmg-path>` — echoes the DMG path

> **Host note:** this test creates a symlink (`ln -s /Applications`). Git Bash on Windows cannot do that reliably, so run this test on the Mac, on Linux, or in WSL. It runs in CI on both the macOS and Linux jobs.

- [ ] **Step 1: Write the failing test**

Create `Scripts/Tests/test-build-dmg.sh`:

```bash
#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DMG_SCRIPT="$SCRIPT_DIR/../build-dmg.sh"

FIXTURE=$(mktemp -d)
trap 'rm -rf "$FIXTURE"' EXIT

# `hdiutil` exists only on macOS. Stubbing it keeps this test runnable on any
# POSIX host and puts the assertion where the script's own logic actually is:
# the staging directory it hands to hdiutil. Whether hdiutil compresses
# correctly is Apple's problem, not this script's.
mkdir -p "$FIXTURE/bin"
cat > "$FIXTURE/bin/hdiutil" <<'EOF'
#!/bin/bash
while [ $# -gt 0 ]; do
  case "$1" in
    -srcfolder) ls -A "$2" > "$DMG_SRCFOLDER_LISTING"; shift 2 ;;
    *) shift ;;
  esac
done
exit 0
EOF
chmod +x "$FIXTURE/bin/hdiutil"

mkdir -p "$FIXTURE/Sirio.app/Contents/MacOS"
touch "$FIXTURE/Sirio.app/Contents/MacOS/sirio"

DMG_SRCFOLDER_LISTING="$FIXTURE/listing.txt" \
PATH="$FIXTURE/bin:$PATH" \
  "$DMG_SCRIPT" "$FIXTURE/Sirio.app" "Sirio" "$FIXTURE/Sirio-0.6.0.dmg" >/dev/null

if ! grep -qx "Sirio.app" "$FIXTURE/listing.txt"; then
  echo "FAIL: the staged folder must contain Sirio.app" >&2
  cat "$FIXTURE/listing.txt" >&2
  exit 1
fi
if ! grep -qx "Applications" "$FIXTURE/listing.txt"; then
  echo "FAIL: the staged folder must contain the /Applications symlink" >&2
  cat "$FIXTURE/listing.txt" >&2
  exit 1
fi

if "$DMG_SCRIPT" "$FIXTURE/does-not-exist.app" "Sirio" "$FIXTURE/x.dmg" >/dev/null 2>&1; then
  echo "FAIL: a missing .app must exit non-zero" >&2
  exit 1
fi

echo "PASS: dmg staging layout"
```

```bash
chmod +x Scripts/Tests/test-build-dmg.sh
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `bash Scripts/Tests/test-build-dmg.sh`
Expected: FAIL — `Scripts/build-dmg.sh` does not exist.

- [ ] **Step 3: Restore the script from history**

```bash
git show c47234de^:Scripts/build-dmg.sh > Scripts/build-dmg.sh
chmod +x Scripts/build-dmg.sh
```

Read what came back before continuing. It should be 30 lines that validate the `.app` exists, stage a copy into a `mktemp -d` alongside an `/Applications` symlink, run `hdiutil create -volname ... -format UDZO`, and echo the output path. Nothing in it is Swift-specific — that is why it is restored rather than rewritten.

- [ ] **Step 4: Run the test to verify it passes**

Run: `bash Scripts/Tests/test-build-dmg.sh`
Expected: `PASS: dmg staging layout`

- [ ] **Step 5: Commit**

```bash
git add Scripts/build-dmg.sh Scripts/Tests/test-build-dmg.sh
git commit -m "feat(release): restore the dmg packaging script"
```

---

### Task 4: `build-app-bundle.sh`

This is the piece with no predecessor: it replaces `xcodegen generate` + `xcodebuild archive` + `xcodebuild -exportArchive`.

**Files:**
- Create: `Scripts/build-app-bundle.sh`
- Create: `Scripts/Tests/test-build-app-bundle.sh`
- Read: `rust/assets/app-icon/icon.icns` (already in the repo)

**Interfaces:**
- Consumes: a compiled `sirio` binary; `CODESIGN_IDENTITY` in the environment
- Produces: `Scripts/build-app-bundle.sh <binary> <version> <output-app-path>` — echoes the signed `.app` path. Task 3's `build-dmg.sh` consumes that path; Task 5 wires both.

- [ ] **Step 1: Write the failing test**

Create `Scripts/Tests/test-build-app-bundle.sh`:

```bash
#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUNDLE_SCRIPT="$SCRIPT_DIR/../build-app-bundle.sh"

FIXTURE=$(mktemp -d)
trap 'rm -rf "$FIXTURE"' EXIT

# `codesign` exists only on macOS, and even there this test must never reach a
# real keychain. The stub records its arguments so the assertions can check the
# hardened runtime is requested — the flag notarization refuses submissions
# without, and the one whose absence would only surface at the Apple round-trip.
mkdir -p "$FIXTURE/bin"
cat > "$FIXTURE/bin/codesign" <<'EOF'
#!/bin/bash
printf '%s\n' "$*" > "$CODESIGN_ARGS"
exit 0
EOF
chmod +x "$FIXTURE/bin/codesign"

printf '#!/bin/sh\nexit 0\n' > "$FIXTURE/sirio"
chmod +x "$FIXTURE/sirio"

CODESIGN_ARGS="$FIXTURE/codesign.args" \
CODESIGN_IDENTITY="Developer ID Application: Test" \
PATH="$FIXTURE/bin:$PATH" \
  "$BUNDLE_SCRIPT" "$FIXTURE/sirio" "0.6.0" "$FIXTURE/Sirio.app" >/dev/null

for path in \
  "$FIXTURE/Sirio.app/Contents/Info.plist" \
  "$FIXTURE/Sirio.app/Contents/MacOS/sirio" \
  "$FIXTURE/Sirio.app/Contents/Resources/icon.icns"
do
  if [ ! -f "$path" ]; then
    echo "FAIL: missing $path" >&2
    exit 1
  fi
done

if [ ! -x "$FIXTURE/Sirio.app/Contents/MacOS/sirio" ]; then
  echo "FAIL: the bundled binary must be executable" >&2
  exit 1
fi

PLIST="$FIXTURE/Sirio.app/Contents/Info.plist"
for value in dev.sirio.Sirio 15.0 public.app-category.developer-tools; do
  if ! grep -q "$value" "$PLIST"; then
    echo "FAIL: Info.plist is missing '$value'" >&2
    cat "$PLIST" >&2
    exit 1
  fi
done

# The version belongs in both CFBundleShortVersionString and CFBundleVersion.
# `|| true` is load-bearing: `grep -c` exits 1 when it counts zero matches, so
# without it this bare assignment aborts under `set -e` — in precisely the
# regression the assertion exists to catch — and the FAIL message below is never
# printed.
COUNT=$(grep -c '<string>0.6.0</string>' "$PLIST" || true)
if [ "$COUNT" != "2" ]; then
  echo "FAIL: expected the version in both version keys, found $COUNT" >&2
  cat "$PLIST" >&2
  exit 1
fi

if ! grep -q -- "--options runtime" "$FIXTURE/codesign.args"; then
  echo "FAIL: codesign must request the hardened runtime" >&2
  cat "$FIXTURE/codesign.args" >&2
  exit 1
fi

if "$BUNDLE_SCRIPT" "$FIXTURE/missing-binary" "0.6.0" "$FIXTURE/X.app" >/dev/null 2>&1; then
  echo "FAIL: a missing binary must exit non-zero" >&2
  exit 1
fi

echo "PASS: app bundle layout and signing flags"
```

```bash
chmod +x Scripts/Tests/test-build-app-bundle.sh
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `bash Scripts/Tests/test-build-app-bundle.sh`
Expected: FAIL — `Scripts/build-app-bundle.sh` does not exist.

- [ ] **Step 3: Write the script**

Create `Scripts/build-app-bundle.sh`:

```bash
#!/bin/bash
# Wraps an already-compiled `sirio` binary in a signed Sirio.app.
#
# Deliberately compiles nothing. Taking a binary rather than building one is
# what makes this runnable by hand against a debug build in two seconds, which
# is the property that makes a broken bundle debuggable. `Scripts/build-dmg.sh`
# beside this one draws the same boundary one step later.
#
# This replaces what `xcodebuild archive` used to do. A macOS bundle is a
# directory convention plus an Info.plist, not a binary format, so anything
# that produces the convention is a legitimate implementation.
set -euo pipefail

usage() {
  echo "Usage: $0 <binary> <version> <output-app-path>" >&2
  exit 2
}

if [ $# -lt 3 ]; then
  usage
fi

BINARY="$1"
VERSION="$2"
APP_PATH="$3"

# The argument-count check above admits an empty third argument, which would
# reach the `rm -rf "$APP_PATH"` below. This is the only destructive operation
# in the release scripts, so it gets its own guard.
if [ -z "$APP_PATH" ]; then
  echo "error: output app path must not be empty" >&2
  exit 1
fi

if [ ! -f "$BINARY" ]; then
  echo "error: binary not found at $BINARY" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ICON="$SCRIPT_DIR/../rust/assets/app-icon/icon.icns"

if [ ! -f "$ICON" ]; then
  echo "error: icon not found at $ICON" >&2
  exit 1
fi

if [ -z "${CODESIGN_IDENTITY:-}" ]; then
  echo "error: CODESIGN_IDENTITY must name the Developer ID identity" >&2
  exit 1
fi

rm -rf "$APP_PATH"
mkdir -p "$APP_PATH/Contents/MacOS" "$APP_PATH/Contents/Resources"

cp "$BINARY" "$APP_PATH/Contents/MacOS/sirio"
chmod +x "$APP_PATH/Contents/MacOS/sirio"
cp "$ICON" "$APP_PATH/Contents/Resources/icon.icns"

# CFBundleVersion repeats CFBundleShortVersionString on purpose. They carry
# different meanings -- user-facing version versus monotonic build number --
# and Sparkle compares the latter, so it has to grow every release. The Swift
# bundle kept them equal for exactly that reason; preserving the rule costs
# nothing and leaves auto-update reachable without a migration.
cat > "$APP_PATH/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleIdentifier</key>
	<string>dev.sirio.Sirio</string>
	<key>CFBundleName</key>
	<string>Sirio</string>
	<key>CFBundleExecutable</key>
	<string>sirio</string>
	<key>CFBundleIconFile</key>
	<string>icon</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>${VERSION}</string>
	<key>CFBundleVersion</key>
	<string>${VERSION}</string>
	<key>LSMinimumSystemVersion</key>
	<string>15.0</string>
	<key>LSApplicationCategoryType</key>
	<string>public.app-category.developer-tools</string>
	<key>NSHighResolutionCapable</key>
	<true/>
</dict>
</plist>
EOF

# --options runtime enables the hardened runtime, without which notarization
# refuses the submission outright. No entitlements on purpose: an app that
# spawns shells and reads PTYs needs none a priori, because the hardened
# runtime restricts dylib injection rather than fork/exec. If notarization or
# first launch fails, add the one exception the error names -- never a
# pre-emptive entitlements file full of permissions nothing uses.
codesign --force --options runtime --timestamp \
  --sign "$CODESIGN_IDENTITY" \
  "$APP_PATH"

echo "$APP_PATH"
```

```bash
chmod +x Scripts/build-app-bundle.sh
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `bash Scripts/Tests/test-build-app-bundle.sh`
Expected: `PASS: app bundle layout and signing flags`

- [ ] **Step 5: Verify against a real build on the Mac**

This is the first time a real `codesign` touches the bundle, and the first time the app is launched from one.

```bash
cd rust && cargo build --release --target aarch64-apple-darwin && cd ..
CODESIGN_IDENTITY=$(security find-identity -v -p codesigning \
  | grep "Developer ID Application" | head -1 | sed -E 's/.*"(.+)"/\1/')
export CODESIGN_IDENTITY
Scripts/build-app-bundle.sh \
  rust/target/aarch64-apple-darwin/release/sirio 0.6.0 build/Sirio.app
codesign --verify --deep --strict --verbose=2 build/Sirio.app
plutil -lint build/Sirio.app/Contents/Info.plist
open build/Sirio.app
```

Expected: `codesign --verify` reports `valid on disk` and `satisfies its Designated Requirement`; `plutil -lint` reports `OK`; the app opens a window.

- [ ] **Step 6: Commit**

```bash
git add Scripts/build-app-bundle.sh Scripts/Tests/test-build-app-bundle.sh
git commit -m "feat(release): build a signed .app bundle around the rust binary"
```

---

### Task 5: Rewrite `release.yml` as four jobs

**Files:**
- Modify: `.github/workflows/release.yml` (full rewrite)
- Create: `Scripts/Tests/test-release-workflow.sh`

**Interfaces:**
- Consumes: `Scripts/check-release-version.sh`, `Scripts/build-app-bundle.sh`, `Scripts/build-dmg.sh`, `Scripts/generate-changelog.sh`
- Produces: a GitHub Release on every `v*.*.*` tag with a notarized DMG, a Linux tarball and a Windows zip

**Secrets that must exist in `ai-sirio/sirio` before the first tag.** They lived in the previous repository and have to be recreated:

```
MAC_LOGIN_KEYCHAIN_PASSWORD          unlocks the login keychain on the runner
DEVELOPER_ID_CERTIFICATE_P12         certificate, base64
DEVELOPER_ID_CERTIFICATE_PASSWORD    .p12 password
KEYCHAIN_PASSWORD                    ephemeral CI keychain
ASC_API_KEY_P8                       App Store Connect key, base64
ASC_API_KEY_ID                       key id
ASC_API_ISSUER_ID                    issuer id
```

The signing identity is **derived** at run time from the certificate just imported into `ci.keychain`, rather than stored as an eighth secret — the certificate already carries its own name, and a stored copy would be a second source of truth that can drift from it.

- [ ] **Step 1: Write the failing test**

Create `Scripts/Tests/test-release-workflow.sh`. It asserts the properties that rot silently — a dropped `needs:`, an unpinned runner image, a drifted Zig version — none of which break anything until a release is already in flight:

```bash
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

grep -q "options runtime\|build-app-bundle.sh" "$WORKFLOW" || fail "no app bundling step"
grep -q "notarytool submit"                    "$WORKFLOW" || fail "no notarization step"
grep -q "stapler staple"                       "$WORKFLOW" || fail "no stapling step"

echo "PASS: release workflow structure"
```

```bash
chmod +x Scripts/Tests/test-release-workflow.sh
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `bash Scripts/Tests/test-release-workflow.sh`
Expected: FAIL — `no macos job`. The current workflow has a single `build-and-test` job.

- [ ] **Step 3: Write the workflow**

Replace `.github/workflows/release.yml` entirely:

```yaml
name: Release

# macOS is the reference release platform: it runs the gate, and it is the only
# job that signs and notarizes. Linux and Windows declare `needs: macos` so a
# broken macOS build stops the release rather than producing artifacts nobody
# will publish. See docs/superpowers/specs/2026-08-29-macos-release-platform-design.md
#
# NOT YET VERIFIED: the workspace has never been compiled for macOS. Phase 0 of
# the spec -- `Scripts/ci.sh` printing `CI OK` on a real Mac -- was deliberately
# deferred by the maintainer and is not in this branch. Do not push a `v*.*.*`
# tag until it has passed there: this workflow is triggered only by such a tag,
# so the first thing an unverified macOS build breaks is a release that is
# already public.
on:
  push:
    tags:
      - 'v*.*.*'

# Read at the top level, so the three build jobs inherit read only. `macos` in
# particular compiles and runs build scripts and test binaries from roughly 400
# third-party crates on the maintainer's personal, non-ephemeral Mac; handing
# that a repo-write token buys nothing, because no build job calls the API. The
# write scope lives on `publish` alone, which is the only job that does.
permissions:
  contents: read

jobs:
  macos:
    runs-on: [self-hosted, macOS, arm64]
    # 120, not 60. The first release runs on a cold self-hosted runner and has
    # to fit a full debug `cargo build --workspace` plus `cargo test
    # --workspace` (the CI gate), then a *second, separate* release compile:
    # `--target aarch64-apple-darwin` uses its own target directory, so nothing
    # from the gate is reused. Then the Apple round-trips on top. The warm
    # `~/.cargo` and `target/` that make 60 minutes plausible only exist from
    # the second release onward.
    timeout-minutes: 120
    outputs:
      version: ${{ steps.version.outputs.version }}
    steps:
      - name: Checkout
        uses: actions/checkout@v4
        with:
          fetch-depth: 0

      # Second on purpose: ordering steps by increasing cost of failure means a
      # version mismatch costs two seconds here instead of an Apple round-trip
      # after notarization.
      - name: Check version matches tag
        id: version
        env:
          TAG_NAME: ${{ github.ref_name }}
        run: |
          VERSION=$(Scripts/check-release-version.sh "$TAG_NAME")
          echo "version=$VERSION" >> "$GITHUB_OUTPUT"

      - name: Verify release scripts
        run: |
          bash Scripts/Tests/test-check-release-version.sh
          bash Scripts/Tests/test-build-dmg.sh
          bash Scripts/Tests/test-build-app-bundle.sh
          bash Scripts/Tests/test-release-workflow.sh

      # There was an `Unlock login keychain` step here, inherited verbatim from
      # the Swift-era pipeline. It is gone on purpose, and should not come back
      # with the next copy-paste from git history.
      #
      # `da9d18da` added it because the Swift app's `KeychainCredentialStore`
      # made the CI gate fail intermittently against a locked keychain. That
      # class went with the Swift app. The only macOS keychain use left in the
      # Rust workspace is `keychain_cookie` in
      # `rust/crates/sirio_usage/src/opencode_go.rs`, which shells out to
      # `security find-generic-password` and returns `None` on any failure -- a
      # locked keychain cannot fail the gate through it.
      #
      # What the step did cost was real: it unlocked the maintainer's personal
      # login keychain, with its password in the environment, before every one
      # of the ~400 crates' build scripts and test binaries ran. Nothing
      # downstream needs it either -- this workflow creates its own
      # `ci.keychain` and puts it first in the search list, `codesign` finds the
      # identity there, `notarytool` authenticates with the API key file, and
      # `stapler` needs no credentials at all. The `MAC_LOGIN_KEYCHAIN_PASSWORD`
      # secret is therefore no longer referenced anywhere in this workflow.
      - name: Run CI gate
        run: Scripts/ci.sh

      - name: Import signing certificate
        env:
          DEVELOPER_ID_CERTIFICATE_P12: ${{ secrets.DEVELOPER_ID_CERTIFICATE_P12 }}
          DEVELOPER_ID_CERTIFICATE_PASSWORD: ${{ secrets.DEVELOPER_ID_CERTIFICATE_PASSWORD }}
          KEYCHAIN_PASSWORD: ${{ secrets.KEYCHAIN_PASSWORD }}
        run: |
          security create-keychain -p "$KEYCHAIN_PASSWORD" ci.keychain
          security set-keychain-settings -lut 21600 ci.keychain
          security unlock-keychain -p "$KEYCHAIN_PASSWORD" ci.keychain
          security list-keychains -d user -s ci.keychain $(security list-keychains -d user | tr -d '"')

          echo "$DEVELOPER_ID_CERTIFICATE_P12" | base64 --decode > cert.p12
          security import cert.p12 -k ci.keychain -P "$DEVELOPER_ID_CERTIFICATE_PASSWORD" -T /usr/bin/codesign -T /usr/bin/security
          security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$KEYCHAIN_PASSWORD" ci.keychain
          rm -f cert.p12

      - name: Build the release binary
        working-directory: rust
        run: cargo build --release --target aarch64-apple-darwin

      # Replaces xcodegen + xcodebuild archive + xcodebuild -exportArchive.
      # The identity is read back from the certificate just imported rather
      # than stored as its own secret, so the two cannot drift apart.
      - name: Build the signed .app
        id: bundle
        env:
          VERSION: ${{ steps.version.outputs.version }}
        run: |
          # This step has no `shell:` key, so it runs as `bash -e {0}` -- pipefail
          # is off, and the pipeline's exit status is sed's (0), not grep's or
          # head's. The `|| true` below is not load-bearing for that reason today,
          # but it keeps the empty-identity guard reachable if this step ever gains
          # an explicit `shell: bash` (as the windows job's Package step already
          # does), which turns pipefail on -- otherwise a failed grep/head would
          # abort the substitution instead of falling through to the guard, at the
          # worst possible moment: signing on a freshly cut release.
          CODESIGN_IDENTITY=$(security find-identity -v -p codesigning ci.keychain \
            | grep "Developer ID Application" | head -1 | sed -E 's/.*"(.+)"/\1/' || true)
          if [ -z "$CODESIGN_IDENTITY" ]; then
            echo "error: no Developer ID Application identity in ci.keychain" >&2
            exit 1
          fi
          export CODESIGN_IDENTITY
          mkdir -p build
          Scripts/build-app-bundle.sh \
            rust/target/aarch64-apple-darwin/release/sirio \
            "$VERSION" \
            build/Sirio.app
          echo "app=build/Sirio.app" >> "$GITHUB_OUTPUT"

      - name: Notarize and staple
        env:
          ASC_API_KEY_P8: ${{ secrets.ASC_API_KEY_P8 }}
          ASC_API_KEY_ID: ${{ secrets.ASC_API_KEY_ID }}
          ASC_API_ISSUER_ID: ${{ secrets.ASC_API_ISSUER_ID }}
          APP_PATH: ${{ steps.bundle.outputs.app }}
        run: |
          echo "$ASC_API_KEY_P8" | base64 --decode > AuthKey.p8
          ditto -c -k --keepParent "$APP_PATH" build/Sirio.zip
          xcrun notarytool submit build/Sirio.zip --key AuthKey.p8 --key-id "$ASC_API_KEY_ID" --issuer "$ASC_API_ISSUER_ID" --wait
          xcrun stapler staple "$APP_PATH"
          rm -f AuthKey.p8

      - name: Build DMG
        id: dmg
        env:
          VERSION: ${{ steps.version.outputs.version }}
          APP_PATH: ${{ steps.bundle.outputs.app }}
        run: |
          DMG_PATH="build/Sirio-${VERSION}.dmg"
          Scripts/build-dmg.sh "$APP_PATH" Sirio "$DMG_PATH"
          echo "path=$DMG_PATH" >> "$GITHUB_OUTPUT"

      # Yes, this is a second Apple round-trip, and it costs another few minutes
      # of wall clock on every release. It is worth it because a notarization
      # ticket is per-artifact: the one stapled onto Sirio.app above says nothing
      # about the DMG that now contains it. The DMG is what the user downloads,
      # so the DMG is what carries the quarantine flag and what Gatekeeper
      # evaluates first -- and an unnotarized container is refused before the
      # stapled .app inside it is ever reachable.
      #
      # Stapling rather than relying on the ticket being fetched is the other
      # half: with the ticket embedded, Gatekeeper resolves the check entirely
      # offline. Without it, first open needs a live call to Apple, which is
      # exactly the moment -- a new user, an unfamiliar app, a corporate proxy or
      # no network -- when a failure reads as "this app is broken".
      #
      # No `--options runtime` here, unlike the .app: the hardened runtime is a
      # property of an executable's code signature, and a disk image has no code
      # to harden. Its signature exists to establish provenance, nothing more.
      #
      # The identity is derived exactly as in `Build the signed .app` above --
      # same command, same `|| true`, same empty-identity guard. See that step's
      # comment for why the `|| true` is there and why the guard has to stay
      # reachable; the two must not drift into variants of each other.
      - name: Sign, notarize and staple the DMG
        env:
          ASC_API_KEY_P8: ${{ secrets.ASC_API_KEY_P8 }}
          ASC_API_KEY_ID: ${{ secrets.ASC_API_KEY_ID }}
          ASC_API_ISSUER_ID: ${{ secrets.ASC_API_ISSUER_ID }}
          DMG_PATH: ${{ steps.dmg.outputs.path }}
        run: |
          CODESIGN_IDENTITY=$(security find-identity -v -p codesigning ci.keychain \
            | grep "Developer ID Application" | head -1 | sed -E 's/.*"(.+)"/\1/' || true)
          if [ -z "$CODESIGN_IDENTITY" ]; then
            echo "error: no Developer ID Application identity in ci.keychain" >&2
            exit 1
          fi
          codesign --force --timestamp --sign "$CODESIGN_IDENTITY" "$DMG_PATH"
          echo "$ASC_API_KEY_P8" | base64 --decode > AuthKey.p8
          xcrun notarytool submit "$DMG_PATH" --key AuthKey.p8 --key-id "$ASC_API_KEY_ID" --issuer "$ASC_API_ISSUER_ID" --wait
          xcrun stapler staple "$DMG_PATH"
          rm -f AuthKey.p8

      - name: Upload the DMG
        uses: actions/upload-artifact@v4
        with:
          name: macos-dmg
          path: ${{ steps.dmg.outputs.path }}
          if-no-files-found: error

      - name: Clean up keychain
        if: always()
        run: |
          security delete-keychain ci.keychain || true
          security lock-keychain login.keychain-db || true
          rm -f cert.p12 AuthKey.p8

  linux:
    needs: macos
    runs-on: ubuntu-22.04
    timeout-minutes: 60
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable

      # libghostty-vt-sys shells out to `zig build`; upstream pins 0.15.2
      # exactly, and a newer Zig fails too.
      - name: Install Zig
        uses: mlugg/setup-zig@v2
        with:
          version: 0.15.2

      - name: Install GUI build dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y \
            pkg-config \
            libgtk-3-dev \
            libwayland-dev \
            libxkbcommon-dev \
            libxcb1-dev

      - name: Cache cargo registry and build artifacts
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: rust

      - name: Build
        working-directory: rust
        run: cargo build --release

      - name: Package
        id: package
        env:
          VERSION: ${{ needs.macos.outputs.version }}
        run: |
          STAGING="sirio-${VERSION}-x86_64-linux"
          mkdir -p "$STAGING"
          cp rust/target/release/sirio rust/target/release/sirioctl "$STAGING/"
          tar -czf "${STAGING}.tar.gz" "$STAGING"
          echo "path=${STAGING}.tar.gz" >> "$GITHUB_OUTPUT"

      - name: Upload the tarball
        uses: actions/upload-artifact@v4
        with:
          name: linux-tarball
          path: ${{ steps.package.outputs.path }}
          if-no-files-found: error

  windows:
    needs: macos
    runs-on: windows-latest
    timeout-minutes: 60
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      # The MSVC toolchain is required: with the GNU ABI, libghostty-vt-sys
      # produces MSVC-shaped artifacts that rustc-gnu cannot link. See
      # docs/prototypes/ghostty-pane-windows.md.
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: stable-x86_64-pc-windows-msvc

      - name: Install Zig
        uses: mlugg/setup-zig@v2
        with:
          version: 0.15.2

      - name: Cache cargo registry and build artifacts
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: rust

      - name: Build
        working-directory: rust
        run: cargo build --release

      - name: Package
        id: package
        shell: bash
        env:
          VERSION: ${{ needs.macos.outputs.version }}
        run: |
          STAGING="sirio-${VERSION}-x86_64-windows"
          mkdir -p "$STAGING"
          cp rust/target/release/sirio.exe rust/target/release/sirioctl.exe "$STAGING/"
          7z a "${STAGING}.zip" "$STAGING"
          echo "path=${STAGING}.zip" >> "$GITHUB_OUTPUT"

      - name: Upload the zip
        uses: actions/upload-artifact@v4
        with:
          name: windows-zip
          path: ${{ steps.package.outputs.path }}
          if-no-files-found: error

  publish:
    needs: [macos, linux, windows]
    runs-on: ubuntu-22.04
    # The only job that talks to the GitHub API, so the only one that needs the
    # write scope the top-level `permissions:` deliberately withholds. It builds
    # nothing and runs no third-party code: it downloads the artifacts the other
    # three produced and calls `gh release create`.
    permissions:
      contents: write
    timeout-minutes: 15
    steps:
      - name: Checkout
        uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Download all artifacts
        uses: actions/download-artifact@v4
        with:
          path: artifacts
          merge-multiple: true

      - name: Generate changelog
        env:
          TAG_NAME: ${{ github.ref_name }}
        run: Scripts/generate-changelog.sh "$TAG_NAME" . > changelog.md

      - name: Publish release
        env:
          GH_TOKEN: ${{ github.token }}
          TAG_NAME: ${{ github.ref_name }}
        run: |
          gh release create "$TAG_NAME" \
            artifacts/* \
            --title "Sirio $TAG_NAME" \
            --notes-file changelog.md
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `bash Scripts/Tests/test-release-workflow.sh`
Expected: `PASS: release workflow structure`

- [ ] **Step 5: Confirm the Linux dependency list empirically**

The apt list above is derived from evidence, not guessed: `gtk = "0.18.2"` in `rust/crates/sirio/Cargo.toml:82` is gtk3-rs, hence `libgtk-3-dev`; gpui's `wayland` and `x11` features need `libwayland-dev`, `libxkbcommon-dev` and `libxcb1-dev`. It has never been run on a clean ubuntu-22.04, so confirm it rather than trust it:

```bash
docker run --rm -it -v "$PWD":/src -w /src ubuntu:22.04 bash -c '
  apt-get update && apt-get install -y curl pkg-config libgtk-3-dev \
    libwayland-dev libxkbcommon-dev libxcb1-dev build-essential &&
  curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y &&
  . "$HOME/.cargo/env" &&
  curl -fsSL -o /tmp/zig.tar.xz https://ziglang.org/download/0.15.2/zig-x86_64-linux-0.15.2.tar.xz &&
  mkdir -p /opt/zig && tar -xf /tmp/zig.tar.xz -C /opt/zig --strip-components=1 &&
  export PATH=/opt/zig:$PATH &&
  cd rust && cargo build --release
'
```

Each `pkg-config` failure names the missing `.pc` file; add the `-dev` package that provides it and re-run. The loop terminates when the build succeeds. Update the workflow's apt list with whatever the loop produced, and add a comment naming which dependency each package serves.

- [ ] **Step 6: Commit**

```bash
git add .github/workflows/release.yml Scripts/Tests/test-release-workflow.sh
git commit -m "feat(release): build signed macos dmg with linux and windows behind it"
```

---

### Task 6: Wire the tests into the gate and update `CLAUDE.md`

The documentation is not an afterthought here. `CLAUDE.md` is the file that instructs every agent working in this repo, it already lags the code, and this decision makes its opening line actively misleading.

**Files:**
- Modify: `Scripts/ci-linux.sh` (add the new tests beside the ones it already runs)
- Modify: `CLAUDE.md` ("What this is" and "Commands")

**Interfaces:**
- Consumes: the four test scripts from Tasks 2-5
- Produces: nothing other tasks depend on

- [ ] **Step 1: Add the new tests to `ci-linux.sh`**

`Scripts/ci-linux.sh` already runs its `Scripts/Tests/*` entries through a `run_root_stage "<label>" <command>` helper, starting at line 252. Add the four new ones in exactly that form, immediately after the `test-visual-sweep.sh` line:

```bash
run_root_stage "test-check-release-version.sh" bash Scripts/Tests/test-check-release-version.sh
run_root_stage "test-build-dmg.sh"             bash Scripts/Tests/test-build-dmg.sh
run_root_stage "test-build-app-bundle.sh"      bash Scripts/Tests/test-build-app-bundle.sh
run_root_stage "test-release-workflow.sh"      bash Scripts/Tests/test-release-workflow.sh
```

`run_root_stage` supplies the stage's PASS/FAIL reporting, so these must not print their own — the four test scripts already end with their own `PASS:` line, which is what the helper's output wraps.

Note that `Scripts/Tests/test-ci.sh` and `Scripts/Tests/test-generate-changelog.sh` are currently wired into no gate at all. Leave that alone; it is a pre-existing gap, not this task's scope.

- [ ] **Step 2: Run the gate's new stage**

Run: `bash Scripts/ci-linux.sh` (on Linux), or run the four tests directly on the Mac.
Expected: four `PASS:` lines.

- [ ] **Step 3: Update "What this is" in `CLAUDE.md`**

The opening currently reads "a native Linux app". Replace it so it states three supported platforms with macOS as the reference release platform, and keep the existing sentences about gpui, the retired Swift app and `alacritty_terminal` intact. Add a pointer to the spec.

Suggested replacement for the first paragraph:

```markdown
Sirio — a native app for macOS, Linux and Windows (Rust, [gpui](https://github.com/zed-industries/zed))
for running multiple AI coding agents (Claude Code, Codex, OpenCode, Pi, Oh-My-Pi) side by side, one
sidebar per project, one terminal per git worktree. **macOS is the reference release platform** — it
gates every release, with Linux and Windows released behind it; see
`docs/superpowers/specs/2026-08-29-macos-release-platform-design.md`. Originally a macOS/Swift app
(itself a fork of Orca with reduced scope); the Swift app was retired once this Rust/gpui port covered
its inventory. Its final commit is `5430d7bfdb4a295be8ce072526ae5108259b80f8`; read any of its source
with `git show 5430d7bfdb4a295be8ce072526ae5108259b80f8:<path>`, or check it out with
`git worktree add <dir> 5430d7bfdb4a295be8ce072526ae5108259b80f8`. Terminal rendering is built on
`alacritty_terminal`.
```

- [ ] **Step 4: Add the release scripts to "Commands" in `CLAUDE.md`**

Append to the existing bash block in the Commands section, after the `Scripts/build-dev.sh` entry:

```bash
# Release (macOS). Each stage takes the previous stage's artifact, so any of
# them can be run by hand against a local build. CODESIGN_IDENTITY must name a
# Developer ID Application identity.
Scripts/check-release-version.sh v0.6.0          # tag vs rust/Cargo.toml
Scripts/build-app-bundle.sh <binary> <version> build/Sirio.app
Scripts/build-dmg.sh build/Sirio.app Sirio build/Sirio-<version>.dmg
```

Also add a line noting that on Windows the MSVC toolchain is required, pointing at `docs/prototypes/ghostty-pane-windows.md` — the session that produced this plan lost forty minutes rediscovering a constraint that document already recorded.

- [ ] **Step 5: Run the full gate**

Run: `Scripts/ci.sh`
Expected: `CI OK`

- [ ] **Step 6: Commit**

```bash
git add Scripts/ci-linux.sh CLAUDE.md
git commit -m "docs: make macos the reference platform in the project guide"
```

---

## After the plan

The first real release is the remaining unknown, and it cannot be rehearsed:

1. Add the seven secrets to `ai-sirio/sirio`.
2. Confirm the self-hosted macOS runner is registered, online, and has Zig 0.15.2 on the `PATH` its shell actually sees.
3. Tag `v0.6.0` and push it.

Expect the first notarization to need an iteration or two — not because the bundle is complex (it is a single self-contained executable with no nested frameworks, the easy case) but because it is the first non-Xcode binary Apple sees under this Developer ID. `xcrun notarytool log <submission-id>` returns the specific reason; act on what it names rather than adding entitlements speculatively.
