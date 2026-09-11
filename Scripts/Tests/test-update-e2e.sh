#!/bin/bash
set -euo pipefail

# The live end-to-end test of the update chain: a really-signed manifest, a
# real HTTP server, a binary compiled the way the release job compiles the app,
# and -- on Windows -- the real silent spawn of the staged artifact.
#
# Why this exists on top of the unit tests. Every stage below is already
# covered with an injected fetch and an injected launcher, which is the right
# shape for `sirio_update` and `sirio_apply` but leaves the compiled wiring
# untested: the manifest URL, the release channel and the accepted key set are
# `option_env!`, baked in at build time, and a build that never saw them looks
# exactly like an updater with nothing to report. A working updater is
# invisible, so the one failure that matters here is the silent one, and it is
# the one no unit test can see.
#
# What it proves, in order, with one HTTP server and one signing key:
#
#   1. `manifest_url()` really returns the compiled-in URL, and the compiled
#      channel really is a channel with updates enabled.
#   2. The manifest parses, its channel matches, and a higher version is
#      reported as Available with its notes and artifact URL.
#   3. The artifact downloads, its SHA-256 and Ed25519 signature verify against
#      the *accepted* key set, and the verified file lands in the staging dir.
#   4. Applying refuses when the binary is not running from an install (the
#      healthy outcome for a dev build, spec 5.1) -- and on Windows, from a
#      real install directory, it launches the staged extensionless PE
#      silently with `/VERYSILENT /NORESTART`.
#   5. A tampered artifact fails verification and leaves nothing staged.
#
# It publishes nothing and touches no installed copy of Sirio: the "installer"
# it signs is `rust/crates/sirio_apply/examples/fake_installer.rs`, which
# writes one marker file and exits.
#
# Note that it builds the probe with `SIRIO_RELEASE_CHANNEL` and friends set,
# so the next ordinary `cargo build` re-compiles `sirio_control`,
# `sirio_update` and `sirio_apply` without them. That is cargo tracking
# `option_env!` correctly, not a problem.
#
# Usage: Scripts/Tests/test-update-e2e.sh

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CARGO_DIR="$REPO_ROOT/rust"

# The manifest URL is compiled in, so the port cannot be picked at run time the
# way a test server's usually is. A fixed high port it is; a busy one fails
# loudly below rather than making the probe look broken.
PORT="${SIRIO_UPDATE_E2E_PORT:-38709}"
MANIFEST_URL="http://127.0.0.1:$PORT/nightly.json"

# Any version above the workspace version would do; this one cannot be reached
# by a bump, so the fixture never expires.
VERSION="99.0.0"
NOTES=$'Probe build.\nSecond line, so the manifest carries a multi-line notes field.'

case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) HOST_OS=windows; EXE=.exe ;;
  Darwin)               HOST_OS=macos;   EXE= ;;
  *)                    HOST_OS=linux;   EXE= ;;
esac

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

# Native binaries take native paths. Git Bash's own conversion is inconsistent
# for `key=value` arguments and does nothing at all for environment variables,
# and a Rust `PathBuf::from("/tmp/x")` on Windows silently means `C:\tmp\x`,
# which would make this test fail in a way that looks like a bug in the
# updater. Shell-side checks keep using the shell's own paths.
winpath() {
  if [ "$HOST_OS" = windows ]; then
    cygpath -w "$1"
  else
    printf '%s' "$1"
  fi
}

if ! command -v cargo >/dev/null 2>&1; then
  fail "cargo is required by this test but is not on PATH"
fi
if ! command -v curl >/dev/null 2>&1; then
  fail "curl is required by this test but is not on PATH"
fi
PYTHON=$(command -v python3 || command -v python || true)
[ -n "$PYTHON" ] || fail "python3 (or python) is required to serve the manifest"

WORK=$(mktemp -d)
SERVER_PID=""
cleanup() {
  if [ -n "$SERVER_PID" ]; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT

SERVE="$WORK/serve"
STAGING="$WORK/staging"
INSTALL="$WORK/local/Programs/Sirio"
mkdir -p "$SERVE" "$STAGING" "$INSTALL"

# `sirio_registry::current_platform_key()` is the string the updater asks the
# manifest for. No CLI prints it, so it is derived here; a mismatch shows up
# immediately as the probe reporting NoArtifact rather than Available.
case "$HOST_OS" in
  windows) PLATFORM=windows-x86_64 ;;
  linux)   PLATFORM=linux-x86_64 ;;
  macos)   PLATFORM=darwin-aarch64 ;;
esac

# --- the signing side ------------------------------------------------------

echo "building the signing CLI and the stand-in installer"
(cd "$CARGO_DIR" && cargo build --quiet -p sirio_release --bin sirio-release)
(cd "$CARGO_DIR" && cargo build --quiet -p sirio_apply --example fake_installer)

SIRIO_RELEASE="$CARGO_DIR/target/debug/sirio-release$EXE"
[ -x "$SIRIO_RELEASE" ] || fail "the signing CLI was not built at $SIRIO_RELEASE"

"$SIRIO_RELEASE" keygen --out "$(winpath "$WORK")" >/dev/null
PUB_KEY=$(tr -d '\r\n' < "$WORK/sirio-release-signing.pub")
[ -n "$PUB_KEY" ] || fail "keygen produced an empty public key"

# The artifact carries the name the real Windows release carries, because the
# manifest's `{file}` expansion is part of what is under test.
ARTIFACT="$SERVE/SirioSetup-$VERSION$EXE"
cp "$CARGO_DIR/target/debug/examples/fake_installer$EXE" "$ARTIFACT"

"$SIRIO_RELEASE" sign \
  --key "$(winpath "$WORK/sirio-release-signing.key")" \
  --channel nightly \
  --version "$VERSION" \
  --notes "$NOTES" \
  --url-template "http://127.0.0.1:$PORT/{file}" \
  --artifact "$PLATFORM=$(winpath "$ARTIFACT")" \
  --out "$(winpath "$SERVE/nightly.json")"

# The same check the release job runs before it publishes: the key that signed
# and the key the binaries accept must be the same one.
"$SIRIO_RELEASE" verify \
  --manifest "$(winpath "$SERVE/nightly.json")" \
  --platform "$PLATFORM" \
  --artifact "$(winpath "$ARTIFACT")" \
  --pub-key "$PUB_KEY" >/dev/null \
  || fail "the manifest does not verify against the key that signed it"

# --- the server ------------------------------------------------------------

if curl -fsS -o /dev/null --max-time 2 "http://127.0.0.1:$PORT/" 2>/dev/null; then
  fail "port $PORT is already in use; set SIRIO_UPDATE_E2E_PORT to a free one"
fi

"$PYTHON" -m http.server "$PORT" --bind 127.0.0.1 --directory "$(winpath "$SERVE")" \
  >/dev/null 2>&1 &
SERVER_PID=$!

SERVED=no
for _ in $(seq 1 50); do
  if curl -fsS -o /dev/null --max-time 1 "$MANIFEST_URL" 2>/dev/null; then
    SERVED=yes
    break
  fi
  sleep 0.2
done
[ "$SERVED" = yes ] || fail "the local manifest server never came up on $MANIFEST_URL"

# --- the probe, compiled the way a release build is ------------------------

echo "building the probe with the release environment compiled in"
(cd "$CARGO_DIR" && \
  SIRIO_RELEASE_CHANNEL=nightly \
  SIRIO_UPDATE_MANIFEST_URL="$MANIFEST_URL" \
  SIRIO_RELEASE_ACCEPTED_KEYS="$PUB_KEY" \
  cargo build --quiet -p sirio_apply --example update_probe)

PROBE="$CARGO_DIR/target/debug/examples/update_probe$EXE"
[ -x "$PROBE" ] || fail "the probe was not built at $PROBE"

PROBE_OUT=""
PROBE_CODE=0
run_probe() {
  # Runs the probe and captures both its output and its exit code without
  # tripping `set -e`; the caller decides which outcome was expected, because
  # "the update was refused" is the right answer in one of the cases below.
  local exe="$1" staging="$2"
  shift 2
  set +e
  PROBE_OUT=$(env "$@" "$exe" "$(winpath "$staging")" 2>&1)
  PROBE_CODE=$?
  set -e
}

dump_probe() {
  echo "----- probe output -----" >&2
  echo "$PROBE_OUT" >&2
  echo "------------------------" >&2
}

# A passing run says nothing about what the chain actually reported, which is
# the wrong default for the one test whose subject is silence. Set
# SIRIO_UPDATE_E2E_VERBOSE=1 to print each stage.
show_probe() {
  if [ "${SIRIO_UPDATE_E2E_VERBOSE:-}" = 1 ]; then
    echo "$PROBE_OUT" | sed 's/^/    /'
  fi
}

expect_line() {
  if ! grep -qxF -- "$1" <<<"$PROBE_OUT"; then
    dump_probe
    fail "expected the probe to print '$1'"
  fi
}

expect_prefix() {
  if ! grep -qF -- "$1" <<<"$PROBE_OUT"; then
    dump_probe
    fail "expected a probe line containing '$1'"
  fi
}

# --- 1. refusing to apply outside an install -------------------------------
#
# Run from the build directory, which is not an install on any platform: the
# check and the download must both succeed and applying must refuse. This is
# the whole dev-build path, and the one that must never guess at an install.

echo "case 1: check + download succeed, applying refuses outside an install"
run_probe "$PROBE" "$STAGING" \
  SIRIO_FAKE_INSTALLER_MARKER="$(winpath "$WORK/marker-should-not-exist")"
show_probe

if [ "$PROBE_CODE" != 14 ]; then
  dump_probe
  fail "expected exit 14 (applying refused), got $PROBE_CODE"
fi
expect_line "CHANNEL nightly"
expect_line "MANIFEST_URL $MANIFEST_URL"
expect_line "CHECK available $VERSION"
expect_prefix "NOTES Probe build. Second line,"
expect_line "ARTIFACT_URL http://127.0.0.1:$PORT/SirioSetup-$VERSION$EXE"
expect_prefix "DOWNLOAD ready "
expect_prefix "APPLY err Sirio is not running from an install directory"

STAGED="$STAGING/sirio-update-$VERSION-$PLATFORM"
[ -f "$STAGED" ] || fail "the verified artifact was not staged at $STAGED"
cmp -s "$STAGED" "$ARTIFACT" || fail "the staged artifact does not match what was served"
if [ -e "$WORK/marker-should-not-exist" ]; then
  fail "a refused apply must not launch anything"
fi
if compgen -G "$STAGING/.*.part" >/dev/null; then
  fail "a completed download left a temporary .part file behind"
fi

# --- 2. Windows: the real silent launch from a real install directory ------

if [ "$HOST_OS" = windows ]; then
  echo "case 2: applying from an install directory launches the staged PE silently"
  cp "$PROBE" "$INSTALL/update_probe.exe"
  STAGING2="$WORK/staging2"
  MARKER="$WORK/installer-ran.txt"
  mkdir -p "$STAGING2"

  # LOCALAPPDATA is what `windows::self_locate` reads; pointing it at the
  # fixture makes `<LOCALAPPDATA>\Programs\Sirio` the expected install dir,
  # which is where the probe now runs from.
  run_probe "$INSTALL/update_probe.exe" "$STAGING2" \
    LOCALAPPDATA="$(winpath "$WORK/local")" \
    SIRIO_FAKE_INSTALLER_MARKER="$(winpath "$MARKER")"
  show_probe

  if [ "$PROBE_CODE" != 0 ]; then
    dump_probe
    fail "expected the whole chain to succeed from an install directory, got exit $PROBE_CODE"
  fi
  expect_line "APPLY ok"

  # The launch is fire-and-forget by design (the real installer closes the
  # app), so wait for the child rather than assuming it has already run.
  RAN=no
  for _ in $(seq 1 50); do
    if [ -f "$MARKER" ]; then
      RAN=yes
      break
    fi
    sleep 0.2
  done
  [ "$RAN" = yes ] || fail "the staged installer never ran: no marker at $MARKER"

  ARGS=$(tr -d '\r' < "$MARKER")
  [ "$ARGS" = "/VERYSILENT /NORESTART" ] \
    || fail "the installer was launched with '$ARGS', expected '/VERYSILENT /NORESTART'"

  # The staged file carries no extension; that it ran at all is the evidence
  # that Windows loaded the PE by content rather than by name.
  STAGED2="$STAGING2/sirio-update-$VERSION-$PLATFORM"
  [ -f "$STAGED2" ] || fail "the verified artifact was not staged at $STAGED2"
  case "$STAGED2" in
    *.exe) fail "the staged artifact should carry no .exe extension" ;;
  esac
else
  echo "case 2: skipped -- the silent installer launch is Windows-only"
fi

# --- 3. a tampered artifact fails verification and stages nothing ----------

echo "case 3: a tampered artifact fails verification and leaves nothing staged"
printf 'tampered' >> "$ARTIFACT"
STAGING3="$WORK/staging3"
mkdir -p "$STAGING3"

run_probe "$PROBE" "$STAGING3" \
  SIRIO_FAKE_INSTALLER_MARKER="$(winpath "$WORK/marker-tampered")"
show_probe

if [ "$PROBE_CODE" != 13 ]; then
  dump_probe
  fail "expected exit 13 (verification failed), got $PROBE_CODE"
fi
expect_prefix "DOWNLOAD err downloaded update failed verification"
if [ -e "$STAGING3/sirio-update-$VERSION-$PLATFORM" ]; then
  fail "a failed verification must not leave a staged artifact behind"
fi
if compgen -G "$STAGING3/.*.part" >/dev/null; then
  fail "a failed verification must not leave a temporary .part file behind"
fi
if [ -e "$WORK/marker-tampered" ]; then
  fail "a failed verification must not launch anything"
fi

echo "UPDATE E2E OK"
