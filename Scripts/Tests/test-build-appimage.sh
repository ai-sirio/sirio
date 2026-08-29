#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUILD_SCRIPT="$SCRIPT_DIR/../build-appimage.sh"

FIXTURE=$(mktemp -d)
trap 'rm -rf "$FIXTURE"' EXIT

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

# The build script reads identifier, name and icon from Scripts/identity.sh;
# source the source itself so the assertions below check what it declares.
. "$SCRIPT_DIR/../identity.sh"

# -- Fixtures ---------------------------------------------------------------

# A fake linuxdeploy, like the fake hdiutil in test-build-dmg.sh: the release
# pipeline's own logic is what this test exercises, not linuxdeploy's. It
# simulates the deployment pass (copy the executable, land a webkit library
# with the baked-in /usr path for the sed rewrite to hit, write the gtk
# plugin's apprun-hook) and the packaging pass (create $OUTPUT).
FAKE_TOOLS="$FIXTURE/tools"
mkdir -p "$FAKE_TOOLS"
cat >"$FAKE_TOOLS/linuxdeploy-x86_64.AppImage" <<'EOF'
#!/bin/bash
set -euo pipefail
APPDIR=""
while [ $# -gt 0 ]; do
  case "$1" in
    --appdir) APPDIR="$2"; shift 2 ;;
    --executable)
      mkdir -p "$APPDIR/usr/bin"
      [ "$2" != "$APPDIR/usr/bin/sirio" ] && cp "$2" "$APPDIR/usr/bin/sirio"
      shift 2 ;;
    --desktop-file|--icon-file|--plugin) shift 2 ;;
    --output)
      # packaging pass: the AppDir has been tailored; emit the image. Rather
      # than a squashed type-2 runtime, fake it as a tarball of the AppDir so
      # the test can extract and assert the exact layout the real AppImage
      # carries.
      mkdir -p "$(dirname "${OUTPUT:?}")"
      tar -cf "$OUTPUT" -C "$APPDIR" .
      shift 2
      ;;
    *) shift ;;
  esac
done
# What linuxdeploy's ldd scan would deliver: the bundled webkit library with
# the compile-time baked path still inside, at the usr/lib location it copies
# to. The build script's sed rewrite must turn /usr into ././
mkdir -p "$APPDIR/usr/lib/x86_64-linux-gnu"
printf '%s' "fake lib /usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/WebKitWebProcess" \
  > "$APPDIR/usr/lib/x86_64-linux-gnu/libwebkit2gtk-4.1.so.0"
# The gtk plugin's hook, which AppRun sources at launch.
mkdir -p "$APPDIR/apprun-hooks"
cat > "$APPDIR/apprun-hooks/linuxdeploy-plugin-gtk.sh" <<'HEOF'
export APPDIR="${APPDIR:-$(dirname "$(realpath "$0")")}"
export GSETTINGS_SCHEMA_DIR="$APPDIR/usr/share/glib-2.0/schemas"
export GIO_EXTRA_MODULES="$APPDIR/usr/lib/x86_64-linux-gnu/gio/modules"
HEOF
EOF
chmod +x "$FAKE_TOOLS/linuxdeploy-x86_64.AppImage"
touch "$FAKE_TOOLS/linuxdeploy-plugin-gtk.sh"

# The runner's webkit2gtk-4.1 layout, under a fake root: Debian/Ubuntu keep
# the helper processes and the injected bundle in /usr/lib/<triplet>/
# webkit2gtk-4.1/ (verified against the ubuntu 22.04 package file list).
WEBKIT_ROOT="$FIXTURE/usr"
mkdir -p "$WEBKIT_ROOT/lib/x86_64-linux-gnu/webkit2gtk-4.1/injected-bundle"
# Content starts with a shebang so the executable bit survives on hosts whose
# chmod is simulated (MSYS/NTFS); on a real runner these are ELF helpers.
printf '#!/bin/sh\nweb-process exec\n' >"$WEBKIT_ROOT/lib/x86_64-linux-gnu/webkit2gtk-4.1/WebKitWebProcess"
printf '#!/bin/sh\nnetwork-process exec\n' >"$WEBKIT_ROOT/lib/x86_64-linux-gnu/webkit2gtk-4.1/WebKitNetworkProcess"
printf 'injected' >"$WEBKIT_ROOT/lib/x86_64-linux-gnu/webkit2gtk-4.1/injected-bundle/libwebkit2gtkinjectedbundle.so"
chmod +x "$WEBKIT_ROOT/lib/x86_64-linux-gnu/webkit2gtk-4.1/WebKitWebProcess" \
  "$WEBKIT_ROOT/lib/x86_64-linux-gnu/webkit2gtk-4.1/WebKitNetworkProcess"

mkdir -p "$FIXTURE/bin"
# Shebang content so the executable bit survives hosts with simulated chmod
# (MSYS/NTFS); on the release runner these are real ELF binaries.
printf '#!/bin/sh\nsirio\n' >"$FIXTURE/bin/sirio"
printf '#!/bin/sh\nsirioctl\n' >"$FIXTURE/bin/sirioctl"

OUTPUT="$FIXTURE/build/Sirio-0.6.0-x86_64.AppImage"

# -- Run ---------------------------------------------------------------------
SIRIO_TOOLS_DIR="$FAKE_TOOLS" SIRIO_WEBKIT_ROOT="$WEBKIT_ROOT" \
  "$BUILD_SCRIPT" "$FIXTURE/bin/sirio" "$FIXTURE/bin/sirioctl" "0.6.0" "$OUTPUT" >/dev/null

# -- Artifact layout ---------------------------------------------------------
# The produced file is a tarball-shaped fake of the type-2 AppImage; extract it
# so the assertions below check the layout the real image carries.
if [ ! -f "$OUTPUT" ]; then
  fail "the AppImage was not produced at $OUTPUT"
fi
EXTRACTED="$FIXTURE/extracted"
mkdir -p "$EXTRACTED"
tar -xf "$OUTPUT" -C "$EXTRACTED"
APPDIR="$EXTRACTED"

# The sibling rule: both binaries in one directory.
if [ ! -x "$APPDIR/usr/bin/sirio" ] || [ ! -x "$APPDIR/usr/bin/sirioctl" ]; then
  fail "usr/bin must hold both sirio and sirioctl"
fi

# -- The .desktop entry reads the identity source -----------------------------
DESKTOP="$APPDIR/usr/share/applications/${SIRIO_APP_IDENTIFIER}.desktop"
if [ ! -f "$DESKTOP" ]; then
  fail "the .desktop entry was not generated at $DESKTOP"
fi
grep -qx "Name=$SIRIO_DISPLAY_NAME" "$DESKTOP" || fail "Name= must be the display name ($SIRIO_DISPLAY_NAME)"
grep -qx "Icon=$SIRIO_APP_IDENTIFIER" "$DESKTOP" || fail "Icon= must be the identifier ($SIRIO_APP_IDENTIFIER)"
grep -qx "StartupWMClass=$SIRIO_APP_IDENTIFIER" "$DESKTOP" || fail "StartupWMClass= must carry the identifier ($SIRIO_APP_IDENTIFIER)"
grep -qx "Exec=sirio" "$DESKTOP" || fail "Exec= must launch the bundled sirio"

# And no packaging script may retype an identifier literal (#304): the
# identity source is the only place an identifier is written.
if grep -Eq '(dev|ai|com)\.sirio' "$BUILD_SCRIPT"; then
  fail "$BUILD_SCRIPT retypes an identifier literal; take it from Scripts/identity.sh"
fi

# -- Icon from the identity source -------------------------------------------
if [ ! -f "$APPDIR/usr/share/icons/hicolor/512x512/apps/${SIRIO_APP_IDENTIFIER}.png" ]; then
  fail "the hicolor icon for ${SIRIO_APP_IDENTIFIER} is missing"
fi

# -- WebKit closure -----------------------------------------------------------
# Helpers land where the sed rewrite resolves after AppRun's chdir: the
# AppDir's top-level lib/<triplet>/webkit2gtk-4.1/.
for helper in WebKitWebProcess WebKitNetworkProcess injected-bundle/libwebkit2gtkinjectedbundle.so; do
  if [ ! -f "$APPDIR/lib/x86_64-linux-gnu/webkit2gtk-4.1/$helper" ]; then
    fail "webkit helper $helper was not staged"
  fi
done

# The baked-in absolute path is rewritten to ./. relative paths. The rewrite
# consumes `/usr` and leaves `/lib` behind, so the result carries a double
# slash (././/lib/...) -- an empty path component the OS ignores, exactly what
# Tauri's production plugin produces for the same libraries.
BUNDLED_SO="$APPDIR/usr/lib/x86_64-linux-gnu/libwebkit2gtk-4.1.so.0"
if ! grep -qF "././/lib/x86_64-linux-gnu/webkit2gtk-4.1/WebKitWebProcess" "$BUNDLED_SO"; then
  fail "the bundled webkit library was not rewritten to ./. paths"
fi
if grep -qF "/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1" "$BUNDLED_SO"; then
  fail "the bundled webkit library still carries a /usr path"
fi

# -- AppRun -------------------------------------------------------------------
if [ ! -x "$APPDIR/AppRun" ]; then
  fail "AppRun is missing or not executable"
fi
grep -q 'cd "\$HERE"' "$APPDIR/AppRun" || fail "AppRun must chdir into the AppDir (webkit ././ paths resolve from there)"
grep -q 'exec "\$HERE/usr/bin/sirio"' "$APPDIR/AppRun" || fail "AppRun must exec the bundled sirio"
grep -q 'apprun-hooks' "$APPDIR/AppRun" || fail "AppRun must source the gtk plugin's hook"

# -- Failure modes ------------------------------------------------------------
if SIRIO_TOOLS_DIR="$FAKE_TOOLS" SIRIO_WEBKIT_ROOT="$WEBKIT_ROOT" \
  "$BUILD_SCRIPT" "$FIXTURE/bin/missing" "$FIXTURE/bin/sirioctl" "0.6.0" "$FIXTURE/x.AppImage" >/dev/null 2>&1; then
  fail "a missing sirio binary must exit non-zero"
fi

if SIRIO_TOOLS_DIR="$FAKE_TOOLS" SIRIO_WEBKIT_ROOT="$FIXTURE/empty-root" \
  "$BUILD_SCRIPT" "$FIXTURE/bin/sirio" "$FIXTURE/bin/sirioctl" "0.6.0" "$FIXTURE/x.AppImage" >/dev/null 2>&1; then
  fail "a runner without the webkit2gtk-4.1 runtime must exit non-zero"
fi

echo "PASS: appimage staging layout"
