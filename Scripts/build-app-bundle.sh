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

if [ -z "$APP_PATH" ]; then
  echo "error: output app path must not be empty" >&2
  exit 1
fi

# Guarded for the same reason as the two beside it, and nothing more: the
# release pipeline cannot reach this today, because `check-release-version.sh`
# either prints a non-empty version or exits non-zero. The case this closes is a
# hand-run invocation -- which the whole script exists to make easy -- where the
# caller passes a variable they forgot to set. It is worth closing because it is
# silent: an empty second argument substitutes into `<string></string>` for both
# CFBundleShortVersionString and CFBundleVersion, which is a well-formed plist,
# so `codesign`, `notarytool` and `hdiutil` all accept it. The result installs
# and launches; it just has no version anywhere a user or Sparkle can read one.
if [ -z "$VERSION" ]; then
  echo "error: version must not be empty" >&2
  exit 1
fi

if [ ! -f "$BINARY" ]; then
  echo "error: binary not found at $BINARY" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Identifier, display name and icon path come from the one identity source
# (#304); this script only generates their plist rendering.
. "$SCRIPT_DIR/identity.sh"
ICON="$SCRIPT_DIR/../$SIRIO_ICON_PATH"

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
#
# Equal up to the prerelease part: Apple allows only dot-separated integers
# in CFBundleVersion, so a nightly (`0.6.0-nightly.202609072133`) keeps its
# full version in CFBundleShortVersionString and only `0.6.0` here. Sirio's
# own updater never reads either key; it compares the version compiled into
# the binary, so the nightly stamp is not lost to it.
cat > "$APP_PATH/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleIdentifier</key>
	<string>${SIRIO_APP_IDENTIFIER}</string>
	<key>CFBundleName</key>
	<string>${SIRIO_DISPLAY_NAME}</string>
	<key>CFBundleExecutable</key>
	<string>sirio</string>
	<key>CFBundleIconFile</key>
	<string>icon</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>${VERSION}</string>
	<key>CFBundleVersion</key>
	<string>${VERSION%%-*}</string>
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
