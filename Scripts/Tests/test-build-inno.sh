#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GEN="$SCRIPT_DIR/../build-inno.sh"

FIXTURE=$(mktemp -d)
trap 'rm -rf "$FIXTURE"' EXIT

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

# The generator places the .iss where its relative Source: paths resolve -- the
# directory ISCC later compiles from. The fixture stands in for that directory;
# the test never compiles, because ISCC is Windows-only and not installed here.
printf 'sirio.exe\n'   > "$FIXTURE/sirio.exe"
printf 'sirioctl.exe\n' > "$FIXTURE/sirioctl.exe"

"$GEN" "0.6.0" "$FIXTURE/Sirio.iss" >/dev/null

ISS="$FIXTURE/Sirio.iss"
[ -f "$ISS" ] || fail "no .iss generated at $ISS"

# The generated script must carry the identity source's facts verbatim, not a
# retyping of them: source the source itself and assert what it declares is
# what the .iss carries (#304).
. "$SCRIPT_DIR/../identity.sh"

grep -Fq "AppId={{$SIRIO_INNO_APP_ID}" "$ISS" \
  || fail ".iss must quote the identity AppId in Inno's {{GUID} escaping form"
grep -Fq "AppName=$SIRIO_DISPLAY_NAME" "$ISS" \
  || fail ".iss must use the identity display name"
grep -Fq "DefaultDirName={localappdata}\\Programs\\$SIRIO_DISPLAY_NAME" "$ISS" \
  || fail ".iss must install per-user under %LOCALAPPDATA%\\Programs\\Sirio"
grep -q "^PrivilegesRequired=lowest$" "$ISS" \
  || fail ".iss must never require elevation"

# The sibling rule (spec §1.4): both binaries land in one directory.
for bin in sirio.exe sirioctl.exe; do
  grep -Fq "Source: \"$bin\"; DestDir: \"{app}\"" "$ISS" \
    || fail ".iss must install $bin into {app}"
done

# Start Menu entry plus the uninstaller Inno always generates.
grep -Fq 'Name: "{group}\' "$ISS" || fail ".iss must add a Start Menu entry"
grep -Fq "UninstallDisplayIcon={app}\\sirio.exe" "$ISS" \
  || fail ".iss must give the uninstaller the app icon"

# The artifact that replaces the zip.
grep -Fq "OutputBaseFilename=SirioSetup-0.6.0" "$ISS" \
  || fail ".iss must name SirioSetup-<version>.exe"

# ...and where it lands, which is the half that was missing. OutputBaseFilename
# sets only the name; without OutputDir, Inno writes into an `Output`
# subdirectory of the script's own directory, ISCC still reports a successful
# compile, and the release workflow's upload step is the first thing to notice
# -- "No files were found with the provided path", after the whole Windows
# build and packaging succeeded. Pinned here so the line cannot be dropped
# again without a test saying so.
grep -Fq "OutputDir=." "$ISS" \
  || fail ".iss must set OutputDir=. so the installer lands beside the binaries, not in Output/"

# Acceptance: no WebView2Loader.dll ships -- the MSVC build links the loader
# statically (webview2-com-sys/WebView2LoaderStatic.lib), so adding the DLL
# would only ship dead weight. The .iss may mention it in the comment above
# [Files], so the assertion targets install entries, not prose.
if grep -Eq '^Source: "WebView2Loader|Filename: ".*WebView2Loader' "$ISS"; then
  fail ".iss must not install WebView2Loader.dll"
fi

# Spec §7.5: chain the runtime bootstrapper only when the runtime is absent,
# silently, and treat a failed fetch as non-fatal.
grep -Fq 'Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' "$ISS" \
  || fail ".iss must probe the WebView2 runtime's clients key"
grep -Fq "RegValueExists(HKCU" "$ISS" || fail "runtime check must read HKCU"
grep -Fq "RegValueExists(HKLM" "$ISS" || fail "runtime check must read HKLM"
grep -Fq "go.microsoft.com/fwlink" "$ISS" || fail "no WebView2 bootstrapper URL"
grep -Fq "DownloadTemporaryFile" "$ISS" || fail "bootstrapper must be downloaded, not bundled"
grep -Fq '"/silent /install"' "$ISS" || fail "bootstrapper must run silently"
grep -Fq "Flags: nowait skipifdoesntexist" "$ISS" \
  || fail "bootstrapper must not block or fail the install"
grep -Fq "InstallWebView2RuntimeIfAbsent" "$ISS" \
  || fail "no conditional WebView2 bootstrap entry"
grep -Fq "except" "$ISS" || fail "a failed download must be swallowed, not fatal"

# The generator reads the AppId from the identity source; it must not carry a
# literal of it (#304's rule, applied to the new script).
if grep -Fq "581478B4" "$GEN"; then
  fail "$GEN retypes the AppId; take it from Scripts/identity.sh"
fi

# An empty version must be rejected by name: it would otherwise fill AppVersion
# and SirioSetup- with nothing, and a versionless installer is a release the
# updater cannot order.
ERR=$("$GEN" "" "$FIXTURE/Y.iss" 2>&1 >/dev/null || true)
case "$ERR" in
  *"version must not be empty"*) ;;
  *) fail "an empty version must be rejected by name, got: $ERR" ;;
esac

echo "PASS: Inno .iss layout, identity source, WebView2 bootstrap"