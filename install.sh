#!/bin/sh
# One-command install for Sirio on macOS and Linux:
#
#   curl -fsSL https://dl.sirioai.app/install.sh | sh
#
# POSIX sh on purpose -- it is executed by whatever /bin/sh the machine has,
# which on Debian and Ubuntu is dash, not bash. No arrays, no [[ ]], no local.
#
# It fetches the artifact the release workflow published for this platform
# (.github/workflows/release.yml) and puts it where the OS expects it: the
# .app into /Applications on macOS, the AppImage into ~/.local/bin on Linux.
# Windows has its own script beside this one, install.ps1.
#
# Environment:
#   SIRIO_VERSION      pin a version ("0.6.0" or "v0.6.0"); default: latest
#   SIRIO_INSTALL_DIR  Linux only: where the AppImage lands (default ~/.local/bin)
#
# What this deliberately does NOT do is verify the Ed25519 release signature.
# A `curl | sh` one-liner already rests on TLS to the download host -- that is
# what fetched this script -- and re-checking the payload with a key carried by
# the same fetch adds no trust. The artifacts keep their own OS-level checks
# (the .dmg is notarized and stapled), and the in-app updater is the path that
# does verify signatures against a compiled-in key set (docs/release-signing.md).
set -eu

REPO="ai-sirio/sirio"
RELEASES="https://github.com/$REPO/releases"

die() {
  echo "error: $*" >&2
  exit 1
}

command -v curl >/dev/null 2>&1 || die "curl is required to download Sirio"

version="${SIRIO_VERSION:-}"
if [ -z "$version" ]; then
  # /releases/latest redirects to /releases/tag/v<version>. Following it costs
  # one request and no token, unlike api.github.com, which rate-limits per IP
  # and would fail on a shared network exactly when a lot of people install.
  effective=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "$RELEASES/latest") \
    || die "could not reach $RELEASES/latest"
  case "$effective" in
    */tag/*) version=${effective##*/tag/} ;;
    *) die "no published release yet -- build from source, see $RELEASES" ;;
  esac
fi
version=${version#v}
tag="v$version"

os=$(uname -s)
arch=$(uname -m)

download() {
  # $1 asset name, $2 destination path
  url="$RELEASES/download/$tag/$1"
  echo "Downloading $1 ..."
  curl -fsSL --proto '=https' --tlsv1.2 -o "$2" "$url" \
    || die "download failed: $url"
}

case "$os" in
  Darwin)
    # Only aarch64-apple-darwin is built and notarized (release.yml); an Intel
    # Mac gets told so rather than a .dmg it cannot run.
    [ "$arch" = "arm64" ] || die "the published .dmg is Apple Silicon only (got $arch) -- build from source, see $RELEASES"

    work=$(mktemp -d)
    mount=""
    cleanup() {
      if [ -n "$mount" ]; then
        hdiutil detach "$mount" -quiet >/dev/null 2>&1 || true
      fi
      rm -rf "$work"
    }
    trap cleanup EXIT INT TERM

    download "Sirio-$version.dmg" "$work/Sirio.dmg"

    mount=$(hdiutil attach -nobrowse -readonly "$work/Sirio.dmg" \
      | sed -n 's|.*\(/Volumes/.*\)$|\1|p' | tail -1)
    [ -n "$mount" ] && [ -d "$mount/Sirio.app" ] || die "the disk image did not contain Sirio.app"

    dest="/Applications"
    [ -w "$dest" ] || dest="$HOME/Applications"
    mkdir -p "$dest"

    # cp -R over a bundle merges instead of replacing, leaving files from the
    # previous version inside the new one. An upgrade replaces the whole .app.
    if [ -d "$dest/Sirio.app" ]; then
      echo "Replacing the existing $dest/Sirio.app ..."
      rm -rf "$dest/Sirio.app"
    fi
    cp -R "$mount/Sirio.app" "$dest/"

    echo "Sirio $version installed to $dest/Sirio.app"
    ;;

  Linux)
    [ "$arch" = "x86_64" ] || die "the published AppImage is x86_64 only (got $arch) -- build from source, see $RELEASES"

    dir="${SIRIO_INSTALL_DIR:-$HOME/.local/bin}"
    mkdir -p "$dir"

    # Download beside the target and move into place, so an interrupted
    # download never leaves a half-written executable named `sirio`.
    tmp="$dir/.sirio.download.$$"
    trap 'rm -f "$tmp"' EXIT INT TERM
    download "Sirio-$version-x86_64.AppImage" "$tmp"
    chmod +x "$tmp"

    # Desktop integration: the AppImage carries its own .desktop entry and
    # icon (rendered from Scripts/identity.sh by Scripts/build-appimage.sh).
    # An AppImage never registers those on its own, so a bare binary install
    # stays invisible to launchers that enumerate ~/.local/share/applications
    # (Omarchy's walker/wofi, GNOME, KDE) -- unlike packaged apps. Extract
    # both into the per-user XDG locations; best-effort, so a runtime that
    # cannot extract still leaves a working binary behind.
    if work_extract=$(mktemp -d 2>/dev/null); then
      if (cd "$work_extract" && "$tmp" --appimage-extract usr/share/applications usr/share/icons >/dev/null 2>&1); then
        data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
        desktop_src=$(find "$work_extract/squashfs-root/usr/share/applications" -maxdepth 1 -name '*.desktop' 2>/dev/null | head -n 1)
        if [ -n "$desktop_src" ]; then
          mkdir -p "$data_home/applications"
          # Point Exec at the installed file instead of relying on PATH.
          sed "s|^Exec=.*|Exec=$dir/sirio|" "$desktop_src" > "$data_home/applications/$(basename "$desktop_src")"
          # A previous install predating desktop integration left this stale
          # name behind; the identifier-based entry above supersedes it.
          rm -f "$data_home/applications/sirio.desktop"
        fi
        icon_src=$(find "$work_extract/squashfs-root/usr/share/icons" -name '*.png' 2>/dev/null | head -n 1)
        if [ -n "$icon_src" ]; then
          mkdir -p "$data_home/icons/hicolor/512x512/apps"
          cp -f "$icon_src" "$data_home/icons/hicolor/512x512/apps/"
        fi
        if command -v update-desktop-database >/dev/null 2>&1; then
          update-desktop-database "$data_home/applications" >/dev/null 2>&1 || true
        fi
      fi
      rm -rf "$work_extract"
    fi

    mv -f "$tmp" "$dir/sirio"

    echo "Sirio $version installed to $dir/sirio"
    case ":$PATH:" in
      *":$dir:"*) ;;
      *) echo "note: $dir is not on your PATH -- add it, or run $dir/sirio directly" ;;
    esac
    ;;

  MINGW*|MSYS*|CYGWIN*|Windows_NT)
    die "on Windows, run: powershell -ExecutionPolicy ByPass -c \"irm https://dl.sirioai.app/install.ps1 | iex\""
    ;;

  *)
    die "unsupported platform: $os -- build from source, see $RELEASES"
    ;;
esac
