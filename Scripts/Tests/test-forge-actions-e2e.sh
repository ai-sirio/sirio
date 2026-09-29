#!/bin/bash
set -euo pipefail

# End-to-end test of acting on a change request (docs/superpowers/specs/
# 2026-09-29-change-request-actions-design.md): the writes of
# rust/crates/sirio_forge, through the real transports -- HTTP with a token,
# `gh api`, `glab api` -- against loopback fake forges
# (Scripts/Tests/fake_forge.py), driven by
# rust/crates/sirio_forge/examples/forge_probe.rs compiled the way the app is.
#
# What it proves is on the wire. Every fake forge logs each request with its
# operation and variables, and that log is the artifact that says what Sirio
# SENT: exactly which mutation, with which input, and -- as important -- what
# it did not send: a write the forge would refuse, a retry of one that may
# have gone through.
#
# One stage per slice of the spec (B2a, B2b, B2c), each added by its slice.
# `--stage NAME` runs one: wire-github, wire-gitlab, failures, cli. Nothing is
# published and the user's own gh/glab configuration is never read.
#
# The artifact: --out-dir DIR (default artifacts/forge-actions-e2e-<stamp>-<pid>)
# keeps transcript.log and every fake forge's request log. Rerunning the
# script reproduces it.
#
# Usage: Scripts/Tests/test-forge-actions-e2e.sh [--stage NAME] [--out-dir DIR]
#        SIRIO_FORGE_E2E_VERBOSE=1 prints every probe answer.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CARGO_DIR="$REPO_ROOT/rust"

OUT_DIR=""
ONLY=""
while [ $# -gt 0 ]; do
  case "$1" in
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    --stage) ONLY="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$OUT_DIR" ] || OUT_DIR="$REPO_ROOT/artifacts/forge-actions-e2e-$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$OUT_DIR"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() { echo "FAIL: $*" >&2; exit 1; }
wanted() { [ -z "$ONLY" ] || [ "$ONLY" = "$1" ]; }

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
start_forge github "$GH_PORT"
start_forge gitlab "$GL_PORT"
: >"$WORK/github-requests.log"
: >"$WORK/gitlab-requests.log"

# Debug builds only: the hosts below are served by the fake forges.
export SIRIO_FORGE_TEST_ENDPOINTS="ghe.test=http://127.0.0.1:$GH_PORT,gitlab.test=http://127.0.0.1:$GL_PORT"
# Neither CLI may see the user's own configuration, tokens or proxy.
export GH_CONFIG_DIR="$WORK/gh-empty" GLAB_CONFIG_DIR="$WORK/glab-empty"
mkdir -p "$GH_CONFIG_DIR" "$GLAB_CONFIG_DIR"
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
no_line() { ! echo "$PROBE_OUT" | grep -qxF -- "$1" || { dump; fail "unexpected line: $1"; }; }

# What the fake forge saw of an operation: how many times, and the JSON `input`
# variable of the last one -- canonical, so a comparison is a string compare.
sent_count() { # flavour Operation
  "$PYTHON" - "$WORK/$1-requests.log" "$2" <<'PY'
import re, sys
count = 0
for line in open(sys.argv[1], encoding="utf-8"):
    found = re.match(r"^POST \S+ (\S+) interaction=\w+ vars=", line)
    if found and found.group(1) == sys.argv[2]:
        count += 1
print(count)
PY
}
sent_input() { # flavour Operation  -> canonical JSON of the last input, or "none"
  "$PYTHON" - "$WORK/$1-requests.log" "$2" <<'PY'
import json, re, sys
last = None
for line in open(sys.argv[1], encoding="utf-8"):
    found = re.match(r"^POST \S+ (\S+) interaction=\w+ vars=(.*)$", line)
    if found and found.group(1) == sys.argv[2]:
        last = json.loads(found.group(2)).get("input")
print("none" if last is None else json.dumps(last, sort_keys=True, separators=(",", ":"), ensure_ascii=False))
PY
}
expect_input() { # flavour Operation json -- key order does not matter, values do
  local got want
  got=$(sent_input "$1" "$2")
  want=$("$PYTHON" -c 'import json,sys; print(json.dumps(json.loads(sys.argv[1]), sort_keys=True, separators=(",", ":"), ensure_ascii=False))' "$3")
  [ "$got" = "$want" ] || { cat "$WORK/$1-requests.log" >&2; fail "$1 $2 was sent as $got, expected $want"; }
}
# The last input's `body`, byte for byte, against a file.
expect_body_is_file() { # flavour Operation file
  "$PYTHON" - "$WORK/$1-requests.log" "$2" "$3" <<'PY' || { cat "$WORK/$1-requests.log" >&2; fail "$1 $2 did not carry the file's bytes"; }
import json, re, sys
last = None
for line in open(sys.argv[1], encoding="utf-8"):
    found = re.match(r"^POST \S+ (\S+) interaction=\w+ vars=(.*)$", line)
    if found and found.group(1) == sys.argv[2]:
        last = json.loads(found.group(2))["input"]["body"]
sys.exit(0 if last == open(sys.argv[3], encoding="utf-8").read() else 1)
PY
}
expect_sent() { # flavour Operation how-many
  local got
  got=$(sent_count "$1" "$2")
  [ "$got" = "$3" ] || { cat "$WORK/$1-requests.log" >&2; fail "$1 saw $2 $got times, expected $3"; }
}
expect_rest() { # flavour "POST /path"
  grep -qF -- "$2 " "$WORK/$1-requests.log" || { cat "$WORK/$1-requests.log" >&2; fail "the $1 forge never saw: $2"; }
}
reset_forge() { # flavour port -- forget the writes it applied, and what it logged
  local port=$GH_PORT
  [ "$1" = gitlab ] && port=$GL_PORT
  curl -s -o /dev/null -X POST "http://127.0.0.1:$port/__reset"
  : >"$WORK/$1-requests.log"
}

# A body with everything that could be mangled on the way: quotes, a
# backslash, non-ASCII, an emoji, tabs, blank lines, a trailing newline.
BODY_FILE="$WORK/body.md"
printf 'He said "ok" \\ naïve café ☕ 日本語\n\n\ttabbed line\nlast line\n' >"$BODY_FILE"

GH=("$PROBE" --forge github --host ghe.test --project acme/widgets --token good)
GL=("$PROBE" --forge gitlab --host gitlab.test --project team/app --token good)
GH_ID='"pullRequestId":"PR_kwDOfake101"'

if wanted wire-github; then
echo "stage wire-github: every action of B2a reaches GitHub as the mutation it means"
reset_forge github "$GH_PORT"
probe "${GH[@]}" header 101
expect_code 0 "github header"
expect_line "CAPS comment,approve,request-changes,edit,state,draft"
expect_line "EDITABLE comment IC_kwDOwidgets1"
no_line "EDITABLE review PRR_kwDObob1"

probe "${GH[@]}" act 101 comment --body-file "$BODY_FILE"
expect_code 0 "a comment"
expect_line "ACT ok"
expect_body_is_file github AddComment "$BODY_FILE"

probe "${GH[@]}" act 101 approve
expect_code 0 "an approval with no words"
expect_input github AddPullRequestReview "{\"event\":\"APPROVE\",$GH_ID}"
probe "${GH[@]}" act 101 approve --body "Nice."
expect_input github AddPullRequestReview "{\"body\":\"Nice.\",\"event\":\"APPROVE\",$GH_ID}"
probe "${GH[@]}" act 101 request-changes --body "Please handle None."
expect_input github AddPullRequestReview "{\"body\":\"Please handle None.\",\"event\":\"REQUEST_CHANGES\",$GH_ID}"
probe "${GH[@]}" act 101 review-comment --body "A thought."
expect_input github AddPullRequestReview "{\"body\":\"A thought.\",\"event\":\"COMMENT\",$GH_ID}"

probe "${GH[@]}" act 101 edit --title "A better title"
expect_input github UpdatePullRequest "{$GH_ID,\"title\":\"A better title\"}"
probe "${GH[@]}" act 101 edit --body "New description" --target develop
expect_input github UpdatePullRequest "{\"baseRefName\":\"develop\",\"body\":\"New description\",$GH_ID}"
probe "${GH[@]}" act 101 edit-comment --id IC_kwDOwidgets1 --body "Edited"
expect_input github UpdateIssueComment '{"body":"Edited","id":"IC_kwDOwidgets1"}'
probe "${GH[@]}" act 101 edit-comment --id PRR_kwDObob1 --kind review --body "Edited review"
expect_input github UpdatePullRequestReview '{"body":"Edited review","pullRequestReviewId":"PRR_kwDObob1"}'

echo "  the state follows what the forge answered: draft, ready, close, reopen"
probe "${GH[@]}" act 101 ready
expect_code 20 "ready on a pull request that is not a draft"
expect_line "ERR Rejected"
expect_sent github MarkPullRequestReadyForReview 0
probe "${GH[@]}" act 101 draft
expect_code 0 "draft"
expect_input github ConvertPullRequestToDraft "{$GH_ID}"
probe "${GH[@]}" act 101 draft
expect_code 20 "draft on a draft"
expect_sent github ConvertPullRequestToDraft 1
probe "${GH[@]}" act 101 ready
expect_code 0 "ready"
expect_input github MarkPullRequestReadyForReview "{$GH_ID}"
probe "${GH[@]}" act 101 close
expect_code 0 "close"
expect_input github ClosePullRequest "{$GH_ID}"
probe "${GH[@]}" act 101 close
expect_code 20 "close on a closed pull request"
expect_sent github ClosePullRequest 1
probe "${GH[@]}" act 101 approve
expect_code 20 "approval of a closed pull request"
expect_line "ERR Rejected"
probe "${GH[@]}" act 101 reopen
expect_code 0 "reopen"
expect_input github ReopenPullRequest "{$GH_ID}"
fi

if wanted wire-gitlab; then
echo "stage wire-gitlab: every action of B2a reaches GitLab as the mutation or REST call it means"
reset_forge gitlab "$GL_PORT"
MR_ID='"iid":"201","projectPath":"team/app"'
probe "${GL[@]}" header 201
expect_code 0 "gitlab header"
expect_line "CAPS comment,approve,request-changes,edit,state,draft"
expect_line "EDITABLE comment gid://gitlab/Note/2"

probe "${GL[@]}" act 201 comment --body-file "$BODY_FILE"
expect_code 0 "a comment"
expect_body_is_file gitlab CreateNote "$BODY_FILE"
probe "${GL[@]}" act 201 comment --body "Plain"
expect_input gitlab CreateNote '{"body":"Plain","noteableId":"gid://gitlab/MergeRequest/201"}'

# GitLab has no approve mutation: approving is REST, and a comment beside it is
# a second call.
probe "${GL[@]}" act 201 approve
expect_code 0 "an approval"
expect_rest gitlab "POST /api/v4/projects/team%2Fapp/merge_requests/201/approve"
expect_sent gitlab CreateNote 2
probe "${GL[@]}" act 201 approve --body "Ship it"
expect_code 0 "an approval with a comment"
expect_sent gitlab CreateNote 3
expect_input gitlab CreateNote '{"body":"Ship it","noteableId":"gid://gitlab/MergeRequest/201"}'
probe "${GL[@]}" act 201 request-changes --body "Needs work"
expect_code 0 "a request for changes"
expect_input gitlab MergeRequestRequestChanges "{$MR_ID}"
expect_input gitlab CreateNote '{"body":"Needs work","noteableId":"gid://gitlab/MergeRequest/201"}'
probe "${GL[@]}" act 201 review-comment --body "Just a note"
expect_input gitlab CreateNote '{"body":"Just a note","noteableId":"gid://gitlab/MergeRequest/201"}'

probe "${GL[@]}" act 201 edit --title "Better" --body "Desc" --target develop
expect_input gitlab MergeRequestUpdate "{$MR_ID,\"description\":\"Desc\",\"targetBranch\":\"develop\",\"title\":\"Better\"}"
probe "${GL[@]}" act 201 edit-comment --id gid://gitlab/Note/2 --body "Edited"
expect_input gitlab UpdateNote '{"body":"Edited","id":"gid://gitlab/Note/2"}'

echo "  the state follows what the forge answered: draft, ready, close, reopen"
probe "${GL[@]}" act 201 ready
expect_code 20 "ready on a merge request that is not a draft"
probe "${GL[@]}" act 201 draft
expect_code 0 "draft"
expect_input gitlab MergeRequestSetDraft "{\"draft\":true,$MR_ID}"
probe "${GL[@]}" act 201 ready
expect_code 0 "ready"
expect_input gitlab MergeRequestSetDraft "{\"draft\":false,$MR_ID}"
probe "${GL[@]}" act 201 close
expect_code 0 "close"
expect_input gitlab MergeRequestUpdate "{$MR_ID,\"state\":\"CLOSED\"}"
probe "${GL[@]}" act 201 close
expect_code 20 "close on a closed merge request"
probe "${GL[@]}" act 201 reopen
expect_code 0 "reopen"
expect_input gitlab MergeRequestUpdate "{$MR_ID,\"state\":\"OPEN\"}"

echo "  an older GitLab lacks canApprove: it can be edited and commented on, and not approved"
reset_forge gitlab "$GL_PORT"
probe "$PROBE" --forge gitlab --host gitlab.test --project team/app --token old header 201
expect_code 0 "an older gitlab's header"
expect_line "CAPS comment,edit,state,draft"
probe "$PROBE" --forge gitlab --host gitlab.test --project team/app --token old act 201 approve
expect_code 20 "an approval on an older gitlab"
expect_line "ERR Rejected"
expect_line "MESSAGE You cannot approve this change request."
if grep -qF "/approve " "$WORK/gitlab-requests.log"; then fail "an approval an older gitlab cannot take reached it"; fi
probe "$PROBE" --forge gitlab --host gitlab.test --project team/app --token old act 201 request-changes --body "No"
expect_line "MESSAGE You cannot request changes on this change request."
probe "$PROBE" --forge gitlab --host gitlab.test --project team/app --token old act 201 comment --body "Still works"
expect_code 0 "a comment on an older gitlab"
fi

if wanted failures; then
echo "stage failures: what Sirio does not send, and what it does with a refusal"
reset_forge github "$GH_PORT"
reset_forge gitlab "$GL_PORT"
echo "  a write the viewer may not make is refused before the wire"
probe "$PROBE" --forge github --host ghe.test --project acme/widgets --token readonly act 101 close
expect_code 20 "close by a viewer who may not"
expect_line "ERR Rejected"
expect_line "MESSAGE This change request cannot be closed."
probe "$PROBE" --forge github --host ghe.test --project acme/widgets --token readonly act 101 comment --body "Hello"
expect_line "MESSAGE You cannot comment on this change request."
probe "$PROBE" --forge github --host ghe.test --project acme/widgets --token readonly act 101 approve
expect_line "MESSAGE You cannot approve this change request."
expect_sent github ClosePullRequest 0
expect_sent github AddComment 0
expect_sent github AddPullRequestReview 0
probe "$PROBE" --forge gitlab --host gitlab.test --project team/app --token readonly act 201 approve
expect_code 20 "approval by a viewer who may not"
if grep -qF "/approve " "$WORK/gitlab-requests.log"; then fail "an approval nobody may make reached GitLab"; fi
echo "  words are required, on the client, whichever forge"
probe "${GH[@]}" act 101 comment --body "   "
expect_line "MESSAGE A comment needs some text."
probe "${GH[@]}" act 101 request-changes
expect_line "MESSAGE Requesting changes needs a reason."
probe "${GH[@]}" act 101 edit --title ""
expect_line "MESSAGE The title cannot be empty."
expect_sent github AddComment 0
echo "  a token without a write scope is Forbidden, and says what the forge said"
probe "$PROBE" --forge github --host ghe.test --project acme/widgets --token scopeless act 101 comment --body "Hi"
expect_code 20 "a github token without scopes"
expect_line "ERR Forbidden"
echo "$PROBE_OUT" | grep -q "^DETAIL .*required scopes" || { dump; fail "the forge's own words were lost"; }
probe "$PROBE" --forge gitlab --host gitlab.test --project team/app --token scopeless act 201 comment --body "Hi"
expect_line "ERR Forbidden"
echo "$PROBE_OUT" | grep -q "^DETAIL .*higher privileges" || { dump; fail "the forge's own words were lost"; }
probe "$PROBE" --forge gitlab --host gitlab.test --project team/app --token scopeless act 201 approve
expect_line "ERR Forbidden"
echo "  a refusal with a reason keeps the reason, HTTP 200 or not"
probe "$PROBE" --forge github --host ghe.test --project acme/widgets --token rejected act 101 comment --body "Hi"
expect_code 20 "a github refusal answered with HTTP 200"
expect_line "ERR Rejected"
expect_line "MESSAGE Pull request is not mergeable"
probe "$PROBE" --forge gitlab --host gitlab.test --project team/app --token rejected act 201 comment --body "Hi"
expect_line "ERR Rejected"
expect_line "MESSAGE Validation failed: title is invalid"
probe "$PROBE" --forge gitlab --host gitlab.test --project team/app --token rejected act 201 approve
expect_line "ERR Rejected"
expect_line "MESSAGE SHA does not match HEAD of source branch"
echo "  a request that may have gone through is never sent twice"
before=$(sent_count github AddComment)
probe "$PROBE" --forge github --host ghe.test --project acme/widgets --token dropped act 101 comment --body "Once"
expect_code 20 "a connection dropped after the request"
expect_line "ERR Network"
[ "$(sent_count github AddComment)" = "$((before + 1))" ] || fail "a dropped write was sent again (or never sent)"
echo "  a rate limit stops a write before it is made"
before=$(sent_count github AddComment)
probe "$PROBE" --forge github --host ghe.test --project acme/widgets --token limited act 101 comment --body "Hi"
expect_code 20 "a rate limited host"
expect_line "ERR RateLimited"
[ "$(sent_count github AddComment)" = "$before" ] || fail "a comment was sent to a rate limited host"
fi

if wanted cli; then
echo "stage cli: the real gh and glab carry the same writes"
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
write_glab_config "$WORK/glab-good" good
GH_CLI=(HTTP_PROXY="http://127.0.0.1:$GH_PORT")
GH_CLI_ARGS=(--forge github --host github.localhost --project acme/widgets --cli)
GL_CLI_ARGS=(--forge gitlab --host gitlab.localhost --project team/app --cli)
if command -v gh >/dev/null; then
  reset_forge github "$GH_PORT"
  probe GH_CONFIG_DIR="$WORK/gh-good" "${GH_CLI[@]}" "$PROBE" "${GH_CLI_ARGS[@]}" act 101 comment --body-file "$BODY_FILE"
  expect_code 0 "a comment through gh"
  expect_line "ACT ok"
  expect_body_is_file github AddComment "$BODY_FILE"
  probe GH_CONFIG_DIR="$WORK/gh-good" "${GH_CLI[@]}" "$PROBE" "${GH_CLI_ARGS[@]}" act 101 approve --body "Via gh"
  expect_input github AddPullRequestReview "{\"body\":\"Via gh\",\"event\":\"APPROVE\",$GH_ID}"
else
  echo "SKIP: gh is not on PATH -- writes through gh were not exercised"
fi
if command -v glab >/dev/null; then
  reset_forge gitlab "$GL_PORT"
  probe GLAB_CONFIG_DIR="$WORK/glab-good" "$PROBE" "${GL_CLI_ARGS[@]}" act 201 comment --body-file "$BODY_FILE"
  expect_code 0 "a comment through glab"
  expect_body_is_file gitlab CreateNote "$BODY_FILE"
  probe GLAB_CONFIG_DIR="$WORK/glab-good" "$PROBE" "${GL_CLI_ARGS[@]}" act 201 approve
  expect_code 0 "an approval (REST) through glab"
  expect_rest gitlab "POST /api/v4/projects/team%2Fapp/merge_requests/201/approve"
else
  echo "SKIP: glab is not on PATH -- writes through glab (and the REST approval) were not exercised"
fi
fi
# Later stages are added above this line.

echo "artifact: $OUT_DIR"
echo "FORGE ACTIONS E2E OK"
