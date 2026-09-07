#!/bin/bash
# Bundles the already-compiled `sirio` and `sirioctl` binaries into a single
# x86_64 AppImage that runs on a host with no GTK3 or webkit installed.
#
# Compiles nothing, like Scripts/build-app-bundle.sh beside it: it takes the
# release binaries and wraps them. `wry` links libwebkit2gtk-4.1 and GTK3 at
# load time, so a bare archive dies before drawing; the AppImage carries the
# whole closure and swaps by replacing one file (spec of #309, §1.3).
#
# The pipeline is the one Tauri ships wry apps with, byte for byte in intent:
# linuxdeploy + linuxdeploy-plugin-gtk bundle the GTK3 closure and its data
# (GLib schemas, GdkPixbuf loaders, GTK immodules, glib-networking's TLS
# modules via GIO_EXTRA_MODULES); then this script adds what webkit needs and
# linuxdeploy cannot know about:
#
#   - the helper processes. WebKitGTK release builds resolve them from a
#     compile-time baked path (`PKGLIBEXECDIR`, see WebKit's
#     Source/WebKit/Shared/glib/ProcessExecutablePathGLib.cpp) -- the
#     WEBKIT_EXEC_PATH override exists only under ENABLE(DEVELOPER_MODE) -- so
#     the only way to relocate them is rewriting the baked string. Tauri's
#     linuxdeploy-plugin-gtk does `sed -i "s|/usr|././|g"` on libwebkit and
#     ships the helpers where the patched relative path resolves: inside the
#     AppDir's top-level lib/ directory, after AppRun has chdir'd into it.
#     This script mirrors both, against the runner's Debian/Ubuntu layout
#     `/usr/lib/<triplet>/webkit2gtk-4.1/`.
#   - the WPE backend and the injected bundle, which webkit dlopens.
#
# Bundled: GTK3, libwebkit2gtk-4.1 and their closure. NOT bundled: glibc (the
# ubuntu-22.04 build base is the minimum), the Mesa/EGL graphics stack, and
# GStreamer's plugin set (media inside web content needs the host's; the
# window itself draws without it). `ponytail:` gstreamer plugins, add
# linuxdeploy-plugin-gstreamer if a webview must play media on bare hosts.
#
# Tool downloads go to $SIRIO_TOOLS_DIR (default ~/.cache/sirio-appimage-tools)
# so consecutive releases reuse them; both files are environment-overridable
# for offline/CI-cached setups via SIRIO_TOOLS_DIR.
set -euo pipefail

usage() {
  echo "Usage: $0 <sirio-binary> <sirioctl-binary> <version> <output.AppImage>" >&2
  exit 2
}

if [ $# -ne 4 ]; then
  usage
fi

SIRIO_BIN="$1"
SIRIOCTL_BIN="$2"
VERSION="$3"
OUTPUT="$4"

for arg in "$SIRIO_BIN" "$SIRIOCTL_BIN" "$VERSION" "$OUTPUT"; do
  if [ -z "$arg" ]; then
    echo "error: all four arguments are required" >&2
    usage
  fi
done

if [ ! -f "$SIRIO_BIN" ]; then
  echo "error: sirio binary not found at $SIRIO_BIN" >&2
  exit 1
fi
if [ ! -f "$SIRIOCTL_BIN" ]; then
  echo "error: sirioctl binary not found at $SIRIOCTL_BIN" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Identifier, display name and icon come from the one identity source (#304);
# this script only renders their .desktop form. No literal is retyped here.
. "$SCRIPT_DIR/identity.sh"

# Linux needs a PNG; the identity source names the icon directory, and that
# directory's PNG family is the Linux rendering of the same artwork.
ICON_DIR="$SCRIPT_DIR/../$(dirname "$SIRIO_ICON_PATH")"
ICON_PNG="$ICON_DIR/icon_512.png"
if [ ! -f "$ICON_PNG" ]; then
  echo "error: Linux icon not found at $ICON_PNG (from identity path $SIRIO_ICON_PATH)" >&2
  exit 1
fi

APPIMAGE_ARCH="x86_64"
WEBKIT_ROOT="${SIRIO_WEBKIT_ROOT:-/usr}"
# The runner's webkit2gtk-4.1 layout on Debian/Ubuntu (verified against the
# ubuntu 22.04 package file list; no /usr/libexec on Debian).
WEBKIT_4_1_DIR="$WEBKIT_ROOT/lib/x86_64-linux-gnu/webkit2gtk-4.1"
DESKTOP_NAME="${SIRIO_APP_IDENTIFIER}.desktop"

STAGING_ROOT="$(mktemp -d)"
trap 'rm -rf "$STAGING_ROOT"' EXIT
APPDIR="$STAGING_ROOT/Sirio.AppDir"

# -- Tools ------------------------------------------------------------------
# Same arrangement Tauri uses: linuxdeploy and its gtk plugin live in one
# directory so linuxdeploy finds the plugin next to itself.
TOOLS_DIR="${SIRIO_TOOLS_DIR:-$HOME/.cache/sirio-appimage-tools}"
mkdir -p "$TOOLS_DIR"
LINUXDEPLOY="$TOOLS_DIR/linuxdeploy-${APPIMAGE_ARCH}.AppImage"
LINUXDEPLOY_PLUGIN="$TOOLS_DIR/linuxdeploy-plugin-gtk.sh"
if [ ! -x "$LINUXDEPLOY" ]; then
  curl -fL \
    "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-${APPIMAGE_ARCH}.AppImage" \
    -o "$LINUXDEPLOY"
  chmod +x "$LINUXDEPLOY"
fi
if [ ! -f "$LINUXDEPLOY_PLUGIN" ]; then
  curl -fL \
    "https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/master/linuxdeploy-plugin-gtk.sh" \
    -o "$LINUXDEPLOY_PLUGIN"
  chmod +x "$LINUXDEPLOY_PLUGIN"
fi

# -- Stage the AppDir --------------------------------------------------------
# The sibling rule (§1.4): `resolve_sirioctl_path` finds sirioctl next to the
# running executable, so both binaries sit in one directory, usr/bin.
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications"
cp "$SIRIO_BIN" "$APPDIR/usr/bin/sirio"
cp "$SIRIOCTL_BIN" "$APPDIR/usr/bin/sirioctl"
chmod +x "$APPDIR/usr/bin/sirio" "$APPDIR/usr/bin/sirioctl"

# The .desktop entry, generated from the identity source, not hand-written
# (§8.3): Name is the display name on every platform, StartupWMClass carries
# the identifier rather than a typed-out guess.
cat >"$APPDIR/usr/share/applications/$DESKTOP_NAME" <<EOF
[Desktop Entry]
Type=Application
Name=${SIRIO_DISPLAY_NAME}
Comment=Multi-agent coding workspace
Exec=sirio
Icon=${SIRIO_APP_IDENTIFIER}
Terminal=false
Categories=Development;
# StartupWMClass must match what a running window reports to WM_CLASS or GNOME
# shows a second generic icon beside the app. gpui's X11 backend writes
# WM_CLASS from the window's app_id (rust/vendor/gpui_linux/src/linux/x11/
# window.rs, set_app_id); the app wires that app identity from the same
# Scripts/identity.sh source, so this line carries the identifier instead of a
# guessed string. Confirm against the running AppImage with
#     xprop WM_CLASS
# on the first Linux build and keep this line in sync (spec §8.3).
StartupWMClass=${SIRIO_APP_IDENTIFIER}
EOF

# The icon the .desktop names, in the hicolor location desktop environments
# look up, plus the .DirIcon the AppImage packaging embeds. The directory is
# taken from the identity source; only the size is Linux-specific.
mkdir -p "$APPDIR/usr/share/icons/hicolor/512x512/apps"
cp "$ICON_PNG" "$APPDIR/usr/share/icons/hicolor/512x512/apps/${SIRIO_APP_IDENTIFIER}.png"
ln -sf "usr/share/icons/hicolor/512x512/apps/${SIRIO_APP_IDENTIFIER}.png" "$APPDIR/.DirIcon"

# -- Bundle GTK3 --------------------------------------------------------------
# First pass builds the AppDir: linuxdeploy deploys the binary plus its ldd
# closure (which pulls libwebkit2gtk-4.1 and libjavascriptcoregtk-4.1 because
# wry links them) and the gtk plugin adds schemas, loaders and GIO TLS.
APPIMAGE_EXTRACT_AND_RUN=1 ARCH="$APPIMAGE_ARCH" \
  "$LINUXDEPLOY" \
  --appdir "$APPDIR" \
  --executable "$APPDIR/usr/bin/sirio" \
  --desktop-file "$APPDIR/usr/share/applications/$DESKTOP_NAME" \
  --plugin gtk

# -- WebKit closure -----------------------------------------------------------
# Helper processes and injected bundle, mirrored where the /usr -> ././
# rewrite below makes them resolve after AppRun chdirs into the AppDir.
# Required, not best-effort: an image without them dies on the exact error
# this ticket exists to retire.
if [ ! -d "$WEBKIT_4_1_DIR" ]; then
  echo "error: webkit2gtk-4.1 runtime not found at $WEBKIT_4_1_DIR" >&2
  echo "       the release runner must have libwebkit2gtk-4.1 installed" >&2
  exit 1
fi
mkdir -p "$APPDIR/lib/x86_64-linux-gnu/webkit2gtk-4.1/injected-bundle"
for helper in WebKitWebProcess WebKitNetworkProcess; do
  if [ ! -x "$WEBKIT_4_1_DIR/$helper" ]; then
    echo "error: $helper not found under $WEBKIT_4_1_DIR" >&2
    exit 1
  fi
  cp "$WEBKIT_4_1_DIR/$helper" "$APPDIR/lib/x86_64-linux-gnu/webkit2gtk-4.1/"
done
if [ ! -f "$WEBKIT_4_1_DIR/injected-bundle/libwebkit2gtkinjectedbundle.so" ]; then
  echo "error: injected bundle not found under $WEBKIT_4_1_DIR" >&2
  exit 1
fi
cp "$WEBKIT_4_1_DIR/injected-bundle/libwebkit2gtkinjectedbundle.so" \
  "$APPDIR/lib/x86_64-linux-gnu/webkit2gtk-4.1/injected-bundle/"

# WPE backend, which webkit2gtk dlopens for rendering; loader-search resolves
# it through AppRun's LD_LIBRARY_PATH. Best-effort: some builds call it
# directly, and the graphics stack itself stays on the host either way.
for lib in libWPEBackend-fdo-1.0.so.1 libwpe-1.0.so.1; do
  if [ -f "$WEBKIT_ROOT/lib/x86_64-linux-gnu/$lib" ]; then
    cp "$WEBKIT_ROOT/lib/x86_64-linux-gnu/$lib" "$APPDIR/lib/x86_64-linux-gnu/"
  fi
done

# Rewrite the baked-in absolute /usr paths inside the webkit libraries to
# ././ paths, which resolve against the AppDir once AppRun has chdir'd into
# it -- the same byte rewrite Tauri's production plugin applies to the same
# libraries. Same width, no size change, ELF-safe.
#
# `-i.bak` rather than `-i`: GNU sed takes an optional suffix, BSD sed a
# mandatory one, and this script's test runs on the macOS release runner
# (release.yml "Verify release scripts") where bare `-i` is a syntax error.
while IFS= read -r -d '' so; do
  sed -i.bak "s|/usr|././|g" "$so"
  rm -f "$so.bak"
done < <(find "$APPDIR/usr/lib" -type f \( -name 'libwebkit2gtk-4.1.so.0' -o -name 'libjavascriptcoregtk-4.1.so.0' -o -name 'libwebkit2gtk-4.1.so.0.*' -o -name 'libjavascriptcoregtk-4.1.so.0.*' \) -print0)

# -- AppRun -------------------------------------------------------------------
# Replace linuxdeploy's AppRun with one that chdirs into the AppDir first.
# That chdir is load-bearing: the webkit path rewrite above is relative, and
# resolves against this directory. Keeps the gtk plugin's apprun-hooks, which
# carry the GTK_THEME / GSETTINGS_SCHEMA_DIR / GIO_EXTRA_MODULES exports.
cat >"$APPDIR/AppRun" <<'EOF'
#!/bin/sh
# Sirio AppImage launcher (#309). The chdir below is load-bearing: the webkit
# libraries carry ././-relative paths (rewritten at build time), which resolve
# against the AppDir; without it the web process never spawns.
HERE="$(dirname -- "$(readlink -f -- "$0")")"
cd "$HERE" || exit 1

export PATH="$HERE/usr/bin:$HERE/usr/sbin:$HERE/usr/games:$HERE/bin:$HERE/sbin:$PATH"
export LD_LIBRARY_PATH="$HERE/usr/lib:$HERE/usr/lib/i386-linux-gnu:$HERE/usr/lib/x86_64-linux-gnu:$HERE/usr/lib32:$HERE/usr/lib64:$HERE/lib:$HERE/lib/i386-linux-gnu:$HERE/lib/x86_64-linux-gnu:$HERE/lib32:$HERE/lib64:${LD_LIBRARY_PATH:-}"
export XDG_DATA_DIRS="$HERE/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"

# linuxdeploy-plugin-gtk's exports (GTK_THEME, GSETTINGS_SCHEMA_DIR,
# GIO_EXTRA_MODULES, GDK_PIXBUF_MODULE_FILE, ...); $0 is this AppRun, so its
# $(realpath "$0") resolves to the same directory as $HERE.
for hook in "$HERE"/apprun-hooks/*.sh; do
  [ -f "$hook" ] && . "$hook"
done

exec "$HERE/usr/bin/sirio" "$@"
EOF
chmod +x "$APPDIR/AppRun"

# -- Package -----------------------------------------------------------------
# Second pass only packages what the AppDir now holds into a type-2 AppImage.
mkdir -p "$(dirname "$OUTPUT")"
rm -f "$OUTPUT"
APPIMAGE_EXTRACT_AND_RUN=1 ARCH="$APPIMAGE_ARCH" OUTPUT="$OUTPUT" \
  "$LINUXDEPLOY" --appdir "$APPDIR" --output appimage

echo "$OUTPUT"
