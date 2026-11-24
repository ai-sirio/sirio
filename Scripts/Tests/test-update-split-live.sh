#!/bin/bash
set -euo pipefail

# The live check of the installer/copy split: the published update manifest names
# a copy of each installer, and an in-app update downloads that copy, so GitHub's
# per-asset counter tells updates from fresh installs.
#
# Why this exists on top of test-release-workflow.sh. That test reads the workflow
# file; it cannot see what the release job really published, whether GitHub keeps
# one counter per asset, or whether the compiled updater, fetching what the live
# manifest names, lands on the copy. Those are facts about a published release,
# so the check runs against one.
#
# What it proves, in order, against the manifest at https://dl.sirioai.app:
#
#   1. Every artifact URL in it ends in -update.dmg, -update.AppImage or
#      -update.exe. Run before the first release with copies and it fails here,
#      naming the installer URL the manifest still points at.
#   2. The release behind those URLs (the tag is in the path) holds, for each
#      copy, an installer of the same name without -update, and each copy carries
#      the label "Sirio <version> for <OS>, used by in-app updates".
#   3. This machine's platform installer and copy counters, read from the
#      release before the download.
#   4. The real compiled updater (update_probe, built the way test-update-e2e.sh
#      builds it, with the production keys) reads the live manifest, served over
#      loopback with its version rewritten to 99.0.0 so the update is always
#      newer. The artifact entries stay untouched, and they are the only part the
#      manifest signs, so the production keys still verify the real download.
#      Nightly passes on "DOWNLOAD ready"; stable downloads during the check and
#      passes on "CHECK Ready". The probe is started without APPIMAGE, the variable
#      that would let it swap an AppImage, so applying refuses and nothing is
#      installed; the script requires that refusal (nightly: exit 14 and "APPLY err
#      Sirio is not running from an install directory"; stable: the probe's exit 10
#      on a ready download, which never reaches applying). Just before the download,
#      which cannot be undone, it reads GitHub's rate limit and stops unless what is
#      left covers every poll of stage 5 and a margin of 5.
#   5. The copy's counter rises by one, polled every 60 s for up to 30 min (GitHub
#      updates counters late: a hand run saw one move after 10 minutes), while the
#      installer's counter stays where stage 3 read it. A failed API read is retried,
#      and the last one's HTTP status and message go into the FAIL line, so rate
#      limiting never reads as a slow counter.
#
# It publishes nothing and touches no installed copy, but the download in stage 4
# is real: it adds one to the counter of this machine's copy, and it runs against
# whichever release the live manifest names. Counters lag by minutes, so a person
# who downloads that release's installer shortly before stage 3, or any time before
# stage 5 ends, moves its counter and fails stage 5; run it again.
#
# Needs SIRIO_RELEASE_ACCEPTED_KEYS, the keys the release binaries accept
# (gh variable get SIRIO_RELEASE_ACCEPTED_KEYS -R ai-sirio/sirio), cargo, curl,
# python3 and network access. GITHUB_TOKEN, when set, is sent to the GitHub API
# for its rate limit; the anonymous one (60 calls an hour) is checked before the
# download and may not cover the window.
#
# Like test-update-e2e.sh, it builds the probe with SIRIO_RELEASE_CHANNEL and
# friends set, so the next ordinary cargo build re-compiles the crates that read
# them without. That is cargo tracking option_env! correctly, not a problem.
#
# Usage: Scripts/Tests/test-update-split-live.sh [--channel nightly|stable] [--out-dir DIR]
#   --channel  the manifest to check (default nightly)
#   --out-dir  where update-split.json is written, on success only
#              (default artifacts/update-split-<stamp>-<pid>)
#   SIRIO_UPDATE_SPLIT_WINDOW  how long stage 5 polls, in seconds (default 1800)
#   SIRIO_UPDATE_SPLIT_PORT    the loopback port of the manifest (default 38710)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CARGO_DIR="$REPO_ROOT/rust"
TARGET_DIR="${CARGO_TARGET_DIR:-$CARGO_DIR/target}"

API=https://api.github.com
MANIFEST_HOST=https://dl.sirioai.app
# The tag is the path segment after /download/, the file name the one after it.
URL_RE='^https://github\.com/ai-sirio/sirio/releases/download/([^/]+)/([^/]+)$'

# The manifest URL is compiled into the probe, so the port cannot be picked at
# run time. A fixed one, apart from test-update-e2e.sh's, so the two can run side
# by side; a busy one fails loudly below.
PORT="${SIRIO_UPDATE_SPLIT_PORT:-38710}"
VERSION="99.0.0"
POLL_SECONDS=60
POLL_WINDOW="${SIRIO_UPDATE_SPLIT_WINDOW:-1800}"

CHANNEL=nightly
OUT_DIR=""

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

usage() {
  echo "usage: $0 [--channel nightly|stable] [--out-dir DIR]" >&2
  exit 2
}

while [ $# -gt 0 ]; do
  case "$1" in
    --channel) [ $# -ge 2 ] || usage; CHANNEL="$2"; shift 2 ;;
    --out-dir) [ $# -ge 2 ] || usage; OUT_DIR="$2"; shift 2 ;;
    *) usage ;;
  esac
done
case "$CHANNEL" in
  nightly|stable) ;;
  *) echo "unknown channel '$CHANNEL': expected nightly or stable" >&2; usage ;;
esac
# A whole number of seconds from 1, with no leading zero: bash reads 010 as octal
# and 08 as an error, and 0 or 00 would poll once and fail after the download.
[[ $POLL_WINDOW =~ ^[1-9][0-9]*$ ]] \
  || fail "SIRIO_UPDATE_SPLIT_WINDOW must be a whole number of seconds from 1, with no leading zero, not '$POLL_WINDOW'"
if [ -z "$OUT_DIR" ]; then
  OUT_DIR="$REPO_ROOT/artifacts/update-split-$(date +%Y%m%d-%H%M%S)-$$"
fi

MANIFEST_URL="http://127.0.0.1:$PORT/$CHANNEL.json"

# The platform key and executable suffix of this machine;
# sirio_registry::current_platform_key() is the string the updater asks for.
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) HOST_OS=windows; EXE=.exe; PLATFORM=windows-x86_64 ;;
  Darwin)               HOST_OS=macos;   EXE=;     PLATFORM=darwin-aarch64 ;;
  *)                    HOST_OS=linux;   EXE=;     PLATFORM=linux-x86_64 ;;
esac

# Native binaries take native paths; see test-update-e2e.sh.
winpath() {
  if [ "$HOST_OS" = windows ]; then
    cygpath -w "$1"
  else
    printf '%s' "$1"
  fi
}

os_for_ext() {
  case "$1" in
    dmg)      echo macOS ;;
    AppImage) echo Linux ;;
    exe)      echo Windows ;;
  esac
}

command -v cargo >/dev/null 2>&1 || fail "cargo is required by this test but is not on PATH"
command -v curl >/dev/null 2>&1 || fail "curl is required by this test but is not on PATH"
PYTHON=$(command -v python3 || command -v python || true)
[ -n "$PYTHON" ] || fail "python3 (or python) is required to read JSON and serve the manifest"
KEYS="${SIRIO_RELEASE_ACCEPTED_KEYS:-}"
[ -n "$KEYS" ] || fail "SIRIO_RELEASE_ACCEPTED_KEYS is required: gh variable get SIRIO_RELEASE_ACCEPTED_KEYS -R ai-sirio/sirio"

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

# GET $1 from the GitHub API, the body on stdout. A failure returns 1 with the reason
# on stderr: "HTTP <status>: <the API's message>" for any answer but 200 (a rate
# limit says so there), curl's own message when nothing answered.
api_get() {
  local code
  code=$(curl -sS --max-time 30 -o "$WORK/api-body" -w '%{http_code}' \
    -H 'Accept: application/vnd.github+json' \
    ${GITHUB_TOKEN:+-H "Authorization: Bearer $GITHUB_TOKEN"} "$API$1") || return 1
  if [ "$code" != 200 ]; then
    echo "HTTP $code: $(api_message <"$WORK/api-body")" >&2
    return 1
  fi
  cat "$WORK/api-body"
}

# The "message" of a GitHub error body, or the start of whatever the body is.
api_message() {
  "$PYTHON" -c '
import json, sys
body = sys.stdin.read()
try:
    print(json.loads(body).get("message") or body[:200])
except Exception:
    print(body[:200] or "no message")
' | tr -d '\r'
}

# Reads release $1 into $WORK/release.json. On failure API_ERROR holds the reason.
API_ERROR=""
read_release() {
  if api_get "/repos/ai-sirio/sirio/releases/tags/$1" >"$WORK/release.json" 2>"$WORK/api-error"; then
    return 0
  fi
  API_ERROR=$(api_error)
  return 1
}

# The reason api_get left in $WORK/api-error, on one line.
api_error() {
  tr '\r\n' '  ' <"$WORK/api-error" | sed 's/ *$//'
}

# "<core calls left><TAB><when the quota resets>", from /rate_limit on stdin.
rate_limit() {
  "$PYTHON" -c '
import json, sys, time
core = json.load(sys.stdin)["resources"]["core"]
print("%d\t%s" % (core["remaining"], time.strftime("%H:%M:%SZ", time.gmtime(core["reset"]))))
' | tr -d '\r'
}

# JSON is read from stdin, so no path crosses into a native python on Windows.
# "<platform><TAB><url>" for each artifact of the manifest.
manifest_urls() {
  "$PYTHON" -c '
import json, sys
for platform, artifact in sorted(json.load(sys.stdin)["artifacts"].items()):
    print(platform + "\t" + artifact["url"])
' | tr -d '\r'
}

manifest_version() {
  "$PYTHON" -c 'import json, sys; print(json.load(sys.stdin)["version"])' | tr -d '\r'
}

# "<label><TAB><download count>" of the release asset called $1; nothing when the
# release has no such asset.
asset_row() {
  "$PYTHON" -c '
import json, sys
for asset in json.load(sys.stdin)["assets"]:
    if asset["name"] == sys.argv[1]:
        print("%s\t%d" % (asset.get("label") or "", asset["download_count"]))
        break
' "$1" | tr -d '\r'
}

# The manifest, rewritten to claim $1 as its version and nothing else.
with_version() {
  "$PYTHON" -c '
import json, sys
manifest = json.load(sys.stdin)
manifest["version"] = sys.argv[1]
json.dump(manifest, sys.stdout, indent=2)
' "$1"
}

# --- 1. the manifest names copies ------------------------------------------

echo "stage 1: every artifact URL in $MANIFEST_HOST/$CHANNEL.json ends in -update"
curl -fsS --max-time 30 -o "$WORK/live-manifest.json" "$MANIFEST_HOST/$CHANNEL.json" \
  || fail "stage 1: could not fetch $MANIFEST_HOST/$CHANNEL.json"
URLS=$(manifest_urls <"$WORK/live-manifest.json") \
  || fail "stage 1: $MANIFEST_HOST/$CHANNEL.json is not a manifest with artifacts"
MANIFEST_VERSION=$(manifest_version <"$WORK/live-manifest.json") \
  || fail "stage 1: $MANIFEST_HOST/$CHANNEL.json has no version"
[ -n "$URLS" ] || fail "stage 1: $MANIFEST_HOST/$CHANNEL.json names no artifact"

while IFS=$'\t' read -r platform url; do
  case "$url" in
    *-update.dmg|*-update.AppImage|*-update.exe) ;;
    *) fail "stage 1: the $CHANNEL manifest names $url for $platform, an installer and not its -update copy" ;;
  esac
done <<<"$URLS"

# --- 2. the release holds each copy beside its installer, labelled ----------

echo "stage 2: the release holds each installer beside its labelled copy"
TAG=""
LABELS=""
while IFS=$'\t' read -r platform url; do
  [[ $url =~ $URL_RE ]] \
    || fail "stage 2: $url for $platform is not an ai-sirio/sirio release download URL"
  if [ -z "$TAG" ]; then
    TAG="${BASH_REMATCH[1]}"
    read_release "$TAG" \
      || fail "stage 2: could not read release $TAG from the GitHub API: $API_ERROR"
  elif [ "${BASH_REMATCH[1]}" != "$TAG" ]; then
    fail "stage 2: the manifest names two releases, $TAG and ${BASH_REMATCH[1]} ($url for $platform)"
  fi

  copy="${BASH_REMATCH[2]}"
  ext="${copy##*.}"
  installer="${copy%-update.*}.$ext"
  expected="Sirio $MANIFEST_VERSION for $(os_for_ext "$ext"), used by in-app updates"

  row=$(asset_row "$copy" <"$WORK/release.json") \
    || fail "stage 2: release $TAG is not a list of assets"
  [ -n "$row" ] || fail "stage 2: release $TAG has no asset named $copy, which the manifest names for $platform"
  label="${row%$'\t'*}"
  [ "$label" = "$expected" ] \
    || fail "stage 2: $copy is labelled '$label', expected '$expected'"

  row=$(asset_row "$installer" <"$WORK/release.json") \
    || fail "stage 2: release $TAG is not a list of assets"
  [ -n "$row" ] || fail "stage 2: release $TAG has no installer named $installer beside its copy $copy"

  LABELS="${LABELS}${LABELS:+$'\n'}$copy"$'\t'"$label"
done <<<"$URLS"

# --- this machine's platform, and the probe built for it --------------------

COPY_URL=$(printf '%s\n' "$URLS" | awk -F'\t' -v p="$PLATFORM" '$1 == p { print $2 }' | tr -d '\r')
[ -n "$COPY_URL" ] || fail "the $CHANNEL manifest has no artifact for $PLATFORM, this machine's platform"
[[ $COPY_URL =~ $URL_RE ]]
COPY="${BASH_REMATCH[2]}"
INSTALLER="${COPY%-update.*}.${COPY##*.}"

SERVE="$WORK/serve"
STAGING="$WORK/staging"
mkdir -p "$SERVE" "$STAGING"
with_version "$VERSION" <"$WORK/live-manifest.json" >"$SERVE/$CHANNEL.json" \
  || fail "could not rewrite the manifest's version"

if curl -fsS -o /dev/null --max-time 2 "http://127.0.0.1:$PORT/" 2>/dev/null; then
  fail "port $PORT is already in use; set SIRIO_UPDATE_SPLIT_PORT to a free one"
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

# Built before stage 3 reads the counters, so the minutes of a cold build do not
# sit between the counters and the download.
echo "building the probe with the release environment compiled in (a cold build takes minutes)"
(cd "$CARGO_DIR" && \
  SIRIO_RELEASE_CHANNEL="$CHANNEL" \
  SIRIO_UPDATE_MANIFEST_URL="$MANIFEST_URL" \
  SIRIO_RELEASE_ACCEPTED_KEYS="$KEYS" \
  cargo build --quiet -p sirio_apply --example update_probe) \
  || fail "cargo could not build the probe"
PROBE="$TARGET_DIR/debug/examples/update_probe$EXE"
[ -x "$PROBE" ] || fail "the probe was not built at $PROBE"

# --- 3. the counters before the download ------------------------------------

echo "stage 3: the $PLATFORM counters of release $TAG before the download"
read_release "$TAG" \
  || fail "stage 3: could not read release $TAG from the GitHub API: $API_ERROR"
row=$(asset_row "$INSTALLER" <"$WORK/release.json") \
  || fail "stage 3: release $TAG is not a list of assets"
INSTALL_BEFORE="${row##*$'\t'}"
row=$(asset_row "$COPY" <"$WORK/release.json") \
  || fail "stage 3: release $TAG is not a list of assets"
UPDATE_BEFORE="${row##*$'\t'}"
[ -n "$INSTALL_BEFORE" ] && [ -n "$UPDATE_BEFORE" ] \
  || fail "stage 3: release $TAG lost $INSTALLER or $COPY"
echo "  $INSTALLER: $INSTALL_BEFORE downloads, $COPY: $UPDATE_BEFORE ($(date -u +%H:%M:%SZ))"

# The download below cannot be undone, so the quota that stage 5 polls with is
# checked first: one read a tick, the first at once, and a margin of 5. Without
# this a rate limited poll would only fail after the download had been spent.
POLLS=$(( (POLL_WINDOW + POLL_SECONDS - 1) / POLL_SECONDS + 1 ))
NEEDED=$((POLLS + 5))
api_get /rate_limit >"$WORK/rate-limit.json" 2>"$WORK/api-error" \
  || fail "stage 3: could not read GitHub's rate limit before the download: $(api_error)"
row=$(rate_limit <"$WORK/rate-limit.json") \
  || fail "stage 3: GitHub's rate limit answer has no resources.core"
REMAINING="${row%$'\t'*}"
echo "  GitHub allows $REMAINING more API calls until ${row##*$'\t'}; polling for ${POLL_WINDOW}s needs $NEEDED ($POLLS reads and a margin of 5)"
[ "$REMAINING" -ge "$NEEDED" ] \
  || fail "stage 3: GitHub allows $REMAINING more API calls until ${row##*$'\t'}, and polling for ${POLL_WINDOW}s needs $NEEDED ($POLLS reads and a margin of 5); set GITHUB_TOKEN, shorten SIRIO_UPDATE_SPLIT_WINDOW or wait"

# --- 4. the compiled updater downloads what the live manifest names ---------

echo "stage 4: the probe checks and downloads against the live manifest served as $VERSION"
set +e
PROBE_OUT=$(env -u APPIMAGE "$PROBE" "$(winpath "$STAGING")" 2>&1)
PROBE_CODE=$?
set -e
PROBE_OUT=$(printf '%s' "$PROBE_OUT" | tr -d '\r')

dump_probe() {
  echo "----- probe output (exit $PROBE_CODE) -----" >&2
  echo "$PROBE_OUT" >&2
  echo "-------------------------------------------" >&2
}
expect_line() {
  if ! grep -qxF -- "$1" <<<"$PROBE_OUT"; then
    dump_probe
    fail "stage 4: expected the probe to print '$1'"
  fi
}
expect_prefix() {
  if ! grep -qF -- "$1" <<<"$PROBE_OUT"; then
    dump_probe
    fail "stage 4: expected a probe line containing '$1'"
  fi
}

# A cached build that never saw the environment says so here, before the network
# makes it look like something else.
expect_line "CHANNEL $CHANNEL"
expect_line "MANIFEST_URL $MANIFEST_URL"
# Nightly checks, then downloads, then refuses to apply outside an install (exit 14),
# which is the one outcome that leaves every installed copy alone: the probe swaps
# whatever file APPIMAGE names, so "APPLY ok" would be a failure, not a pass.
# Stable's check downloads and returns Ready, which the probe reports with exit 10
# before it reaches applying.
if [ "$CHANNEL" = nightly ]; then
  expect_line "ARTIFACT_URL $COPY_URL"
  expect_prefix "DOWNLOAD ready "
  expect_prefix "APPLY err Sirio is not running from an install directory"
  [ "$PROBE_CODE" = 14 ] || { dump_probe; fail "stage 4: expected exit 14 (applying refused outside an install), got $PROBE_CODE"; }
else
  expect_prefix "CHECK Ready"
  [ "$PROBE_CODE" = 10 ] || { dump_probe; fail "stage 4: expected exit 10 (a ready download, reported before applying), got $PROBE_CODE"; }
fi
echo "  the probe downloaded and verified $COPY ($(date -u +%H:%M:%SZ))"

# --- 5. the copy's counter rises, the installer's does not ------------------

echo "stage 5: waiting for the counter of $COPY to pass $UPDATE_BEFORE (window ${POLL_WINDOW}s, one read every ${POLL_SECONDS}s)"
DEADLINE=$((SECONDS + POLL_WINDOW))
UPDATE_AFTER=""
INSTALL_AFTER=""
READS=0
READ_FAILURES=0
LAST_ERROR=""
while :; do
  READS=$((READS + 1))
  TICK_FAILED=yes
  if read_release "$TAG"; then
    row=$(asset_row "$COPY" <"$WORK/release.json") || row=""
    if [ -n "$row" ]; then
      TICK_FAILED=no
      UPDATE_AFTER="${row##*$'\t'}"
      row=$(asset_row "$INSTALLER" <"$WORK/release.json") || row=""
      INSTALL_AFTER="${row##*$'\t'}"
      echo "  $COPY: $UPDATE_AFTER, $INSTALLER: $INSTALL_AFTER ($(date -u +%H:%M:%SZ))"
      if [ "$UPDATE_AFTER" -ge $((UPDATE_BEFORE + 1)) ]; then
        break
      fi
    else
      API_ERROR="release $TAG no longer lists $COPY"
    fi
  fi
  if [ "$TICK_FAILED" = yes ]; then
    READ_FAILURES=$((READ_FAILURES + 1))
    LAST_ERROR="$API_ERROR"
    echo "  the release could not be read ($LAST_ERROR) ($(date -u +%H:%M:%SZ)); trying again"
  fi
  if [ "$SECONDS" -ge "$DEADLINE" ]; then
    # The reason belongs in the line: a rate limit or a 404 must not read as lag.
    FAILED_READS=""
    if [ "$READ_FAILURES" -gt 0 ]; then
      FAILED_READS="; $READ_FAILURES of $READS reads failed, the last with: $LAST_ERROR"
    fi
    if [ "$TICK_FAILED" = yes ]; then
      fail "stage 5: the last read of release $TAG failed, so the counter of $COPY is unknown (last seen '${UPDATE_AFTER:-never read}', before the download $UPDATE_BEFORE)${FAILED_READS}"
    fi
    fail "stage 5: after ${POLL_WINDOW}s the counter of $COPY reads '$UPDATE_AFTER', still not above $UPDATE_BEFORE${FAILED_READS}"
  fi
  sleep "$POLL_SECONDS"
done
[ "$INSTALL_AFTER" = "$INSTALL_BEFORE" ] \
  || fail "stage 5: the counter of $INSTALLER moved from $INSTALL_BEFORE to '$INSTALL_AFTER' while the copy's rose to $UPDATE_AFTER"

# --- the artifact ------------------------------------------------------------

mkdir -p "$OUT_DIR"
{
  printf '%s\n' "$URLS" | awk '{ print "url\t" $0 }'
  printf '%s\n' "$LABELS" | awk '{ print "label\t" $0 }'
  printf '%s\n' "$PROBE_OUT" | awk '{ print "probe\t" $0 }'
} | "$PYTHON" -c '
import json, sys
urls, labels, probe = {}, {}, []
for line in sys.stdin.read().splitlines():
    kind, rest = line.split("\t", 1)
    if kind == "probe":
        probe.append(rest)
    else:
        key, value = rest.split("\t", 1)
        (urls if kind == "url" else labels)[key] = value
channel, tag, platform, ib, ub, ia, ua = sys.argv[1:]
json.dump({
    "channel": channel,
    "tag": tag,
    "platform": platform,
    "urls": urls,
    "labels": labels,
    "before": {"install": int(ib), "update": int(ub)},
    "after": {"install": int(ia), "update": int(ua)},
    "probe": probe,
}, sys.stdout, indent=2)
print()
' "$CHANNEL" "$TAG" "$PLATFORM" "$INSTALL_BEFORE" "$UPDATE_BEFORE" "$INSTALL_AFTER" "$UPDATE_AFTER" \
  >"$OUT_DIR/update-split.json" \
  || fail "could not write $OUT_DIR/update-split.json"

echo "$COPY: $UPDATE_BEFORE -> $UPDATE_AFTER downloads, $INSTALLER: $INSTALL_BEFORE -> $INSTALL_AFTER"
echo "artifact: $OUT_DIR/update-split.json"
echo "UPDATE SPLIT OK"
