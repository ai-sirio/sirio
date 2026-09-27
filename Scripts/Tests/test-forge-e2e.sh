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
PROBE="${CARGO_TARGET_DIR:-$CARGO_DIR/target}/debug/examples/forge_probe"

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

write_gh_hosts() { # dir credential
  mkdir -p "$1"
  printf 'github.localhost:\n    users:\n        fake-user:\n            oauth_token: %s\n    git_protocol: https\n    user: fake-user\n    oauth_token: %s\n' "$2" "$2" >"$1/hosts.yml"
}
write_glab_config() { # dir credential
  mkdir -p "$1"
  chmod 700 "$1"
  printf 'hosts:\n    gitlab.localhost:\n        token: %s\n        api_protocol: http\n        api_host: 127.0.0.1:%s\n        git_protocol: https\n' "$2" "$GL_PORT" >"$1/config.yml"
  chmod 600 "$1/config.yml"
}
write_gh_hosts "$WORK/gh-good" good
write_gh_hosts "$WORK/gh-expired" expired
write_glab_config "$WORK/glab-good" good
write_glab_config "$WORK/glab-expired" expired
# gh serves `github.localhost` over plain HTTP at api.github.localhost and
# honours HTTP_PROXY, which routes it to the github fake. glab refuses a
# port in --hostname, so its host key is portless and `api_host` carries the
# fake's address.
GH_CLI=(HTTP_PROXY="http://127.0.0.1:$GH_PORT")
GH_CLI_ARGS=(--forge github --host github.localhost --project acme/widgets --cli)
GL_CLI_ARGS=(--forge gitlab --host gitlab.localhost --project team/app --cli)

echo "case 3: the real gh and glab carry the same requests, and keep their HTTP errors' meaning"
if command -v gh >/dev/null; then
  probe GH_CONFIG_DIR="$WORK/gh-good" "${GH_CLI[@]}" "$PROBE" "${GH_CLI_ARGS[@]}" viewer
  expect_code 0 "gh viewer"
  expect_line "VIEWER fake-user"
  expect_log github "POST /graphql Viewer"
  probe GH_CONFIG_DIR="$WORK/gh-expired" "${GH_CLI[@]}" "$PROBE" "${GH_CLI_ARGS[@]}" viewer
  expect_code 20 "gh answering 401 with --include and exit 1"
  expect_line "ERR NotAuthenticated"
  probe GH_CONFIG_DIR="$WORK/gh-empty" "${GH_CLI[@]}" "$PROBE" "${GH_CLI_ARGS[@]}" viewer
  expect_code 20 "gh holding no credential for the host (exit 4)"
  expect_line "ERR NotAuthenticated"
else
  echo "SKIP: gh is not on PATH -- the gh transport was not exercised"
fi
if command -v glab >/dev/null; then
  probe GLAB_CONFIG_DIR="$WORK/glab-good" "$PROBE" "${GL_CLI_ARGS[@]}" viewer
  expect_code 0 "glab viewer"
  expect_line "VIEWER fake-user"
  expect_log gitlab "POST /api/graphql CurrentUser"
  probe GLAB_CONFIG_DIR="$WORK/glab-expired" "$PROBE" "${GL_CLI_ARGS[@]}" viewer
  expect_code 20 "glab answering 401 with --include and exit 1"
  expect_line "ERR NotAuthenticated"
else
  echo "SKIP: glab is not on PATH -- the glab transport was not exercised"
fi
probe PATH="$WORK/no-programs" "$PROBE" "${GH_CLI_ARGS[@]}" viewer
expect_code 20 "a CLI that is not installed"
expect_line "ERR NotInstalled"

echo "case 4: a host resolves to its forge and means, asking the system only as far as it must"
if command -v gh >/dev/null; then
  probe GH_CONFIG_DIR="$WORK/gh-good" "${GH_CLI[@]}" "$PROBE" resolve --host github.localhost
  expect_code 0 "gh signed in to github.localhost"
  expect_line "RESOLVE ready github cli"
else
  echo "SKIP: gh is not on PATH -- CLI detection for GitHub was not exercised"
fi
if command -v glab >/dev/null; then
  probe GLAB_CONFIG_DIR="$WORK/glab-good" "$PROBE" resolve --host gitlab.localhost
  expect_code 0 "glab signed in to gitlab.localhost"
  expect_line "RESOLVE ready gitlab cli"
else
  echo "SKIP: glab is not on PATH -- CLI detection for GitLab was not exercised"
fi
meta_before=$(log_count github "GET /api/v3/meta")
probe "$PROBE" resolve --host ghe.test
expect_line "RESOLVE not-connected github"
[ "$(log_count github "GET /api/v3/meta")" -gt "$meta_before" ] || fail "ghe.test was never asked /api/v3/meta"
probe "$PROBE" resolve --host nowhere.test
expect_line "RESOLVE unknown"
meta_before=$(log_count gitlab "GET /api/v3/meta")
probe "$PROBE" resolve --host gitlab.test --token-forge gitlab
expect_line "RESOLVE ready gitlab token"
[ "$(log_count gitlab "GET /api/v3/meta")" = "$meta_before" ] || fail "a stored token must settle the forge without the meta probe"
probe "$PROBE" resolve --host nowhere.test --setting gitlab:token
expect_line "RESOLVE not-connected gitlab"

# Later cases are added above this line.

echo "artifact: $OUT_DIR"
echo "FORGE E2E OK"
