#!/bin/bash
# Drives install.sh with `curl` and `uname` shadowed on PATH, so the whole
# script runs offline and the platform branches can be tested from any host.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALL_SCRIPT="$SCRIPT_DIR/../../install.sh"

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

BIN="$WORK/bin"
mkdir -p "$BIN"

# Records every URL asked for, answers a HEAD with $FAKE_EFFECTIVE_URL and a
# download by writing a marker payload to the -o destination.
cat > "$BIN/curl" <<'EOF'
#!/bin/bash
head_mode=0
out=""
url=""
while [ $# -gt 0 ]; do
  case "$1" in
    -o) out="$2"; shift 2 ;;
    -w|--proto) shift 2 ;;
    -*I*) head_mode=1; shift ;;
    -*) shift ;;
    *) url="$1"; shift ;;
  esac
done
echo "$url" >> "$CURL_LOG"
if [ "$head_mode" = 1 ]; then
  printf '%s' "$FAKE_EFFECTIVE_URL"
  exit 0
fi
printf 'fake-payload' > "$out"
EOF
chmod +x "$BIN/curl"

cat > "$BIN/uname" <<'EOF'
#!/bin/bash
case "${1:-}" in
  -s) printf '%s\n' "$FAKE_OS" ;;
  -m) printf '%s\n' "$FAKE_ARCH" ;;
  *)  printf '%s\n' "$FAKE_OS" ;;
esac
EOF
chmod +x "$BIN/uname"

run_install() {
  env PATH="$BIN:$PATH" \
      HOME="$WORK/home" \
      CURL_LOG="$WORK/curl.log" \
      FAKE_OS="$1" FAKE_ARCH="$2" FAKE_EFFECTIVE_URL="$3" \
      SIRIO_INSTALL_DIR="$WORK/home/.local/bin" \
      SIRIO_VERSION="${4:-}" \
      sh "$INSTALL_SCRIPT"
}

fail() { echo "FAIL: $*" >&2; exit 1; }

# 1. Linux, no pinned version: the tag comes from the /releases/latest redirect
#    and the AppImage lands executable under SIRIO_INSTALL_DIR.
mkdir -p "$WORK/home"
: > "$WORK/curl.log"
run_install Linux x86_64 "https://github.com/ai-sirio/sirio/releases/tag/v9.9.9" >/dev/null
grep -q "download/v9.9.9/Sirio-9.9.9-x86_64.AppImage" "$WORK/curl.log" \
  || fail "expected the download URL to carry the version from the redirect, got: $(cat "$WORK/curl.log")"

# The download is renamed into place as `sirio` and made executable; no
# .download temporary is left behind.
BINARY="$WORK/home/.local/bin/sirio"
[ -x "$BINARY" ] || fail "expected an executable $BINARY"
[ "$(cat "$BINARY")" = "fake-payload" ] || fail "the downloaded bytes must be what lands as sirio"
if ls "$WORK/home/.local/bin/".sirio.download.* >/dev/null 2>&1; then
  fail "the temporary download file must not survive"
fi

# 2. A pinned version skips the redirect entirely.
: > "$WORK/curl.log"
run_install Linux x86_64 "unused" "0.6.0" >/dev/null
grep -q "download/v0.6.0/Sirio-0.6.0-x86_64.AppImage" "$WORK/curl.log" \
  || fail "a pinned SIRIO_VERSION must be used verbatim, got: $(cat "$WORK/curl.log")"
grep -q "releases/latest" "$WORK/curl.log" \
  && fail "a pinned SIRIO_VERSION must not resolve the latest release"

# 3. An architecture with no published artifact fails instead of downloading.
if run_install Linux aarch64 "unused" "0.6.0" >/dev/null 2>&1; then
  fail "aarch64 Linux has no published AppImage and must exit non-zero"
fi
if run_install Darwin x86_64 "unused" "0.6.0" >/dev/null 2>&1; then
  fail "an Intel Mac has no published .dmg and must exit non-zero"
fi

# 4. Before the first tag there is no /tag/ in the redirect: say so, do not
#    download a URL built from an empty version.
if run_install Linux x86_64 "https://github.com/ai-sirio/sirio/releases" >/dev/null 2>&1; then
  fail "an unreleased repository must exit non-zero"
fi

echo "PASS: install.sh"
