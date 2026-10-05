#!/bin/bash
# Wraps already-compiled `sirio` and `sirio-host` binaries in a signed Sirio.app.
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
  echo "Usage: $0 <binary> <version> <output-app-path> <sirio-host-binary>" >&2
  exit 2
}

if [ $# -lt 4 ]; then
  usage
fi

BINARY="$1"
VERSION="$2"
APP_PATH="$3"
HOST_BINARY="$4"

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

if [ ! -f "$HOST_BINARY" ]; then
  echo "error: sirio-host binary not found at $HOST_BINARY" >&2
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
# The session host sits beside the app's executable, where `ensure_host`
# looks for it. The app never runs it from here: it copies it to
# <data root>/bin/<version>/ first (spec §4.3), so an update replacing the
# bundle never pulls the binary out from under a running host.
cp "$HOST_BINARY" "$APP_PATH/Contents/MacOS/sirio-host"
chmod +x "$APP_PATH/Contents/MacOS/sirio-host"
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
	<key>NSAppleEventsUsageDescription</key>
	<string>Tools and agents running in Sirio's terminals use Apple Events to control other apps.</string>
	<key>NSLocalNetworkUsageDescription</key>
	<string>Tools and agents running in Sirio's terminals reach development servers on your local network.</string>
</dict>
</plist>
EOF

# --options runtime enables the hardened runtime, without which notarization
# refuses the submission outright. An app that spawns shells and reads PTYs
# needs no exception for that, because the hardened runtime restricts dylib
# injection rather than fork/exec. It does need one for Apple Events: the
# hardened runtime refuses them outright -- errAEEventNotPermitted, no prompt
# -- unless the signature carries `automation.apple-events`, and the tools in
# a pane run inside Sirio's TCC envelope, so an agent's `osascript` driving
# another app was dead in the signed build while working in the ad-hoc dev
# one. That is the only entry in sirio.entitlements; add another only for a
# failure that names it, never pre-emptively.
#
# Signing the bundle signs only its main executable (CFBundleExecutable): a
# second Mach-O in Contents/MacOS is nested code, which must be signed first,
# inside-out, or the bundle's seal names an unsigned subcomponent and
# notarization refuses it. So sirio-host is signed on its own, with the same
# identity, runtime and entitlements, before the bundle that seals it. The
# signature is embedded in the binary, so the staged copy the app runs
# carries it too.
codesign --force --options runtime --timestamp \
  --entitlements "$SCRIPT_DIR/sirio.entitlements" \
  --sign "$CODESIGN_IDENTITY" \
  "$APP_PATH/Contents/MacOS/sirio-host"

codesign --force --options runtime --timestamp \
  --entitlements "$SCRIPT_DIR/sirio.entitlements" \
  --sign "$CODESIGN_IDENTITY" \
  "$APP_PATH"

echo "$APP_PATH"
