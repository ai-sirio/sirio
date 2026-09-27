#!/bin/bash
set -euo pipefail

# End-to-end test of the forge layer (rust/crates/sirio_forge): the real
# transports -- HTTP with a token, `gh api`, `glab api` -- against loopback
# fake forges (Scripts/Tests/fake_forge.py), driven by
# rust/crates/sirio_forge/examples/forge_probe.rs compiled the way the app is.
#
# Why beside the unit tests: what breaks here is the seam with the outside --
# a CLI's `--include` output, a chunked request body, an HTTP status turned
# into the error it means, a GitLab that rejects a newer field. No unit test
# sees any of it.
#
# Each case says what it proves as it runs. Nothing is published and the
# user's own gh/glab configuration is never read: both CLIs run against
# throwaway config directories, and the token variables are unset.
#
# The artifact: --out-dir DIR (default artifacts/forge-e2e-<stamp>-<pid>)
# keeps transcript.log and every fake forge's request log. Rerunning the
# script reproduces it.
#
# Usage: Scripts/Tests/test-forge-e2e.sh [--out-dir DIR]
#        SIRIO_FORGE_E2E_VERBOSE=1 prints every probe answer.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CARGO_DIR="$REPO_ROOT/rust"

OUT_DIR=""
while [ $# -gt 0 ]; do
  case "$1" in
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$OUT_DIR" ] || OUT_DIR="$REPO_ROOT/artifacts/forge-e2e-$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$OUT_DIR"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() { echo "FAIL: $*" >&2; exit 1; }

command -v cargo >/dev/null || fail "cargo is required"
command -v curl >/dev/null || fail "curl is required"
PYTHON=$(command -v python3 || true)
[ -n "$PYTHON" ] || fail "python3 is required to run the fake forges"

WORK=$(mktemp -d)
PIDS=()
cleanup() {
  for pid in ${PIDS[@]+"${PIDS[@]}"}; do # empty-array safe under set -u on bash 3.2
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  done
  cp "$WORK"/*.log "$OUT_DIR/" 2>/dev/null || true
  rm -rf "$WORK"
}
trap cleanup EXIT

free_port() {
  "$PYTHON" -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()'
}

start_forge() { # flavour port
  "$PYTHON" "$SCRIPT_DIR/fake_forge.py" --flavor "$1" --port "$2" \
    --log "$WORK/$1-requests.log" 2>"$WORK/$1-server.log" &
  PIDS+=($!)
  for _ in $(seq 1 50); do
    curl -s -o /dev/null --max-time 1 "http://127.0.0.1:$2/api/v3/meta" && return 0
    sleep 0.2
  done
  fail "the $1 fake forge never came up on port $2"
}

GH_PORT=$(free_port)
GL_PORT=$(free_port)
NONE_PORT=$(free_port)
DEAD_PORT=$(free_port) # nothing listens here
start_forge github "$GH_PORT"
start_forge gitlab "$GL_PORT"
start_forge none "$NONE_PORT"
: >"$WORK/github-requests.log"
: >"$WORK/gitlab-requests.log"

# Debug builds only: the hosts below are served by the fake forges.
export SIRIO_FORGE_TEST_ENDPOINTS="ghe.test=http://127.0.0.1:$GH_PORT,gitlab.test=http://127.0.0.1:$GL_PORT,nowhere.test=http://127.0.0.1:$NONE_PORT,down.test=http://127.0.0.1:$DEAD_PORT"
# Neither CLI may see the user's own configuration, tokens or proxy.
export GH_CONFIG_DIR="$WORK/gh-empty" GLAB_CONFIG_DIR="$WORK/glab-empty"
mkdir -p "$GH_CONFIG_DIR" "$GLAB_CONFIG_DIR" "$WORK/no-programs"
chmod 700 "$GLAB_CONFIG_DIR"
unset GH_TOKEN GITHUB_TOKEN GH_ENTERPRISE_TOKEN GITHUB_ENTERPRISE_TOKEN GITLAB_TOKEN \
  GL_TOKEN GH_HOST GITLAB_HOST HTTP_PROXY HTTPS_PROXY http_proxy https_proxy ALL_PROXY all_proxy || true

echo "building the probe"
(cd "$CARGO_DIR" && cargo build --quiet -p sirio_forge --example forge_probe)
PROBE="$CARGO_DIR/target/debug/examples/forge_probe"

PROBE_OUT=""
PROBE_CODE=0
probe() { # [VAR=value ...] "$PROBE" args...
  set +e
  PROBE_OUT=$(env "$@" 2>&1)
  PROBE_CODE=$?
  set -e
  if [ "${SIRIO_FORGE_E2E_VERBOSE:-}" = 1 ]; then echo "$PROBE_OUT" | sed 's/^/    /'; fi
}
dump() { printf -- '----- probe output (exit %s) -----\n%s\n-----\n' "$PROBE_CODE" "$PROBE_OUT" >&2; }
expect_code() { [ "$PROBE_CODE" = "$1" ] || { dump; fail "$2: expected exit $1, got $PROBE_CODE"; }; }
expect_line() { echo "$PROBE_OUT" | grep -qxF -- "$1" || { dump; fail "missing line: $1"; }; }
expect_rows() { # the ROW labels, in order, comma-separated
  local rows
  rows=$(echo "$PROBE_OUT" | grep '^ROW ' | cut -d' ' -f2 | paste -sd, -)
  [ "$rows" = "$1" ] || { dump; fail "rows were '$rows', expected '$1'"; }
}
expect_log() { grep -qF -- "$2" "$WORK/$1-requests.log" || { cat "$WORK/$1-requests.log" >&2; fail "the $1 forge never saw: $2"; }; }
log_count() { grep -cF -- "$2" "$WORK/$1-requests.log" || true; }

GH_TOKEN_ARGS=(--forge github --host ghe.test --project acme/widgets)
GL_TOKEN_ARGS=(--forge gitlab --host gitlab.test --project team/app)

echo "case 1: a token reaches both forges and names the signed-in account"
probe "$PROBE" "${GH_TOKEN_ARGS[@]}" --token good viewer
expect_code 0 "github viewer"
expect_line "VIEWER fake-user"
expect_log github "POST /api/graphql Viewer"
probe "$PROBE" "${GL_TOKEN_ARGS[@]}" --token good viewer
expect_code 0 "gitlab viewer"
expect_line "VIEWER fake-user"
expect_log gitlab "POST /api/graphql CurrentUser"

echo "case 2: each HTTP failure reaches the caller as the error it means"
for scenario in "expired NotAuthenticated" "sso Forbidden" "limited RateLimited" "throttled RateLimited"; do
  read -r credential variant <<<"$scenario"
  probe "$PROBE" "${GH_TOKEN_ARGS[@]}" --token "$credential" viewer
  expect_code 20 "credential $credential"
  expect_line "ERR $variant"
done
probe "$PROBE" "${GH_TOKEN_ARGS[@]}" --token sso viewer
expect_line "SSO https://github.com/orgs/acme/sso?authorization_request=abc"
probe "$PROBE" "${GH_TOKEN_ARGS[@]}" --token limited viewer
expect_line "RESET 4102444800"
probe "$PROBE" --forge github --host down.test --project acme/widgets --token good viewer
expect_code 20 "a host nothing listens on"
expect_line "ERR Network"
probe "$PROBE" --forge github --host nowhere.test --project acme/widgets --token good viewer
expect_code 20 "a host that is not a forge"
expect_line "ERR NotFound"

# Later cases are added above this line.

echo "artifact: $OUT_DIR"
echo "FORGE E2E OK"
