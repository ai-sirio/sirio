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
# `--stage NAME` runs one: wire-github, wire-gitlab, failures, merge, metadata, cli,
# scopes, ui. Nothing
# is published and the user's own gh/glab configuration is never read.
#
# The `ui` stage launches a real, isolated Sirio (debug build) against the
# same fake forges and drives the change request tab over the control socket.
# The verb that writes -- `surface change-request act` -- exists only in a debug
# build, and calls the very handlers the tab's buttons call.
#
# The artifact: --out-dir DIR (default artifacts/forge-actions-e2e-<stamp>-<pid>)
# keeps transcript.log and every fake forge's request log. Rerunning the
# script reproduces it.
#
# Usage: Scripts/Tests/test-forge-actions-e2e.sh [--stage NAME] [--state-only] [--out-dir DIR] [--display :N]
#        SIRIO_FORGE_E2E_VERBOSE=1 prints every probe answer. `--state-only`
#        skips the ui stage's window captures (frames/), which need a display.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CARGO_DIR="$REPO_ROOT/rust"

OUT_DIR=""
ONLY=""
STATE_ONLY=0
DISPLAY_TARGET="${DISPLAY:-}"
while [ $# -gt 0 ]; do
  case "$1" in
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    --stage) ONLY="$2"; shift 2 ;;
    --state-only) STATE_ONLY=1; shift ;;
    --display) DISPLAY_TARGET="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$OUT_DIR" ] || OUT_DIR="$REPO_ROOT/artifacts/forge-actions-e2e-$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$OUT_DIR/frames"
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

# The committed fixtures, plus what a read shows after each write: generated
# from them here, so each overlay is the base with one change rather than a
# second copy to keep in step.
FIXTURES="$WORK/fixtures"
cp -r "$SCRIPT_DIR/forge-fixtures" "$FIXTURES"
"$PYTHON" - "$FIXTURES" <<'PY'
import copy, json, sys
root = sys.argv[1]

def load(flavor, name):
    with open(f"{root}/{flavor}/{name}.json", encoding="utf-8") as handle:
        return json.load(handle)

def save(flavor, name, value):
    with open(f"{root}/{flavor}/{name}.json", "w", encoding="utf-8") as handle:
        json.dump(value, handle)

def github(name, change):
    doc = copy.deepcopy(load("github", "ChangeRequestHeader"))
    change(doc["data"]["repository"]["pullRequest"])
    save("github", name, doc)

def comment_of(pr):
    return next(node for node in pr["timelineItems"]["nodes"] if node.get("__typename") == "IssueComment")

def closed(pr):
    pr.update(state="CLOSED", viewerCanClose=False, viewerCanReopen=True)

def drafted(pr):
    pr["isDraft"] = True

def edited(pr):
    pr.update(title="A better title", body="New description", baseRefName="develop")

def comment_edited(pr):
    comment_of(pr)["body"] = "Edited comment"

def readonly(pr):
    pr.update(locked=True, viewerDidAuthor=True, viewerCanUpdate=False, viewerCanClose=False, viewerCanReopen=False)
    comment_of(pr)["viewerCanUpdate"] = False

github("ChangeRequestHeader.after.ClosePullRequest", closed)
github("ChangeRequestHeader.after.ReopenPullRequest", lambda pr: None)
github("ChangeRequestHeader.after.ConvertPullRequestToDraft", drafted)
github("ChangeRequestHeader.after.MarkPullRequestReadyForReview", lambda pr: None)
github("ChangeRequestHeader.after.UpdatePullRequest", edited)
github("ChangeRequestHeader.after.UpdateIssueComment", comment_edited)
github("ChangeRequestHeader.readonly", readonly)

def gitlab(name, change):
    doc = copy.deepcopy(load("gitlab", "MergeRequestHeader"))
    change(doc["data"]["project"]["mergeRequest"])
    save("gitlab", name, doc)

def note_of(mr):
    return next(note for note in mr["notes"]["nodes"] if note["userPermissions"]["adminNote"])

gitlab("MergeRequestHeader.after.MergeRequestUpdate.CLOSED", lambda mr: mr.update(state="closed"))
gitlab("MergeRequestHeader.after.MergeRequestUpdate.OPEN", lambda mr: None)
gitlab("MergeRequestHeader.after.MergeRequestSetDraft.true", lambda mr: mr.update(draft=True))
gitlab("MergeRequestHeader.after.MergeRequestSetDraft.false", lambda mr: None)
gitlab("MergeRequestHeader.after.MergeRequestUpdate", lambda mr: mr.update(title="Better", description="Desc", targetBranch="develop"))
gitlab("MergeRequestHeader.after.UpdateNote", lambda mr: note_of(mr).update(body="Edited comment"))
gitlab("MergeRequestHeader.readonly", lambda mr: (
    mr.update(discussionLocked=True, userPermissions={"canApprove": False, "createNote": False, "updateMergeRequest": False}),
    note_of(mr)["userPermissions"].update(adminNote=False),
))
PY

start_forge() { # flavour port
  "$PYTHON" "$SCRIPT_DIR/fake_forge.py" --flavor "$1" --port "$2" --fixtures "$FIXTURES" \
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

# Every input the fake forge saw of an operation, one canonical JSON per line.
sent_inputs() { # flavour Operation
  "$PYTHON" - "$WORK/$1-requests.log" "$2" <<'PY'
import json, re, sys
for line in open(sys.argv[1], encoding="utf-8"):
    found = re.match(r"^POST \S+ (\S+) interaction=\w+ vars=(.*)$", line)
    if found and found.group(1) == sys.argv[2]:
        print(json.dumps(json.loads(found.group(2)).get("input"), sort_keys=True, separators=(",", ":"), ensure_ascii=False))
PY
}
expect_nth_input() { # flavour Operation n(1-based) json
  local got want
  got=$(sent_inputs "$1" "$2" | sed -n "${3}p")
  want=$("$PYTHON" -c 'import json,sys; print(json.dumps(json.loads(sys.argv[1]), sort_keys=True, separators=(",", ":"), ensure_ascii=False))' "$4")
  [ "$got" = "$want" ] || { cat "$WORK/$1-requests.log" >&2; fail "$1 $2 #$3 was sent as $got, expected $want"; }
}
expect_prefix() { echo "$PROBE_OUT" | grep -q -- "^$1" || { dump; fail "no line starting: $1"; }; }
no_rest() { # flavour "METHOD /path-prefix"
  ! grep -q -- "^$2" "$WORK/$1-requests.log" || { cat "$WORK/$1-requests.log" >&2; fail "the $1 forge saw: $2"; }
}
expect_var() { # flavour Operation key value -- a read's variable, as the forge saw it
  "$PYTHON" - "$WORK/$1-requests.log" "$2" "$3" "$4" <<'PY' || { cat "$WORK/$1-requests.log" >&2; fail "$1 $2 never carried $3=$4"; }
import json, re, sys
for line in open(sys.argv[1], encoding="utf-8"):
    found = re.match(r"^POST \S+ (\S+) interaction=\w+ vars=(.*)$", line)
    if found and found.group(1) == sys.argv[2] and json.loads(found.group(2)).get(sys.argv[3]) == sys.argv[4]:
        sys.exit(0)
sys.exit(1)
PY
}

GH_HEAD=b2c3d4e5f60718293a4b5c6d7e8f901234567890
GL_HEAD=d4e5f60718293a4b5c6d7e8f901234567890a1b2

if wanted merge; then
echo "stage merge: a merge reaches each forge with the head the user saw, and only when the forge would take it"
reset_forge github "$GH_PORT"
probe "${GH[@]}" header 101
expect_code 0 "github header"
expect_line "MERGE ready methods=merge,squash,rebase default=merge auto=yes enabled=- delete-default=no"
probe "${GH[@]}" act 101 merge --method squash --head "$GH_HEAD" --title "Ship it" --message "because"
expect_code 0 "a squash merge"
expect_input github MergePullRequest "{\"commitBody\":\"because\",\"commitHeadline\":\"Ship it\",\"expectedHeadOid\":\"$GH_HEAD\",\"mergeMethod\":\"SQUASH\",$GH_ID}"
no_rest github "DELETE "
echo "  a rebase carries no message; delete-branch is a REST call after the merge"
reset_forge github "$GH_PORT"
probe "${GH[@]}" act 101 merge --method rebase --head "$GH_HEAD" --title "ignored" --delete-branch yes
expect_code 0 "a rebase merge"
expect_input github MergePullRequest "{\"expectedHeadOid\":\"$GH_HEAD\",\"mergeMethod\":\"REBASE\",$GH_ID}"
expect_rest github "DELETE /api/v3/repos/acme/widgets/git/refs/heads/feat/login"
echo "  a moved head is refused before the wire"
reset_forge github "$GH_PORT"
probe "${GH[@]}" act 101 merge --method merge --head 0000000000000000000000000000000000000000
expect_code 20 "a merge of a head that moved"
expect_line "ERR HeadMoved"
expect_sent github MergePullRequest 0
echo "  a failed branch deletion leaves the merge done, with a warning"
reset_forge github "$GH_PORT"
probe "$PROBE" --forge github --host ghe.test --project acme/widgets --token deletefails act 101 merge --method merge --head "$GH_HEAD" --delete-branch yes
expect_code 0 "a merge whose branch deletion fails"
expect_line "ACT ok"
expect_prefix "WARNING merged; deleting the branch failed:"
echo "  a head in a fork is never deleted"
reset_forge github "$GH_PORT"
probe "$PROBE" --forge github --host ghe.test --project acme/widgets --token fork act 101 merge --method merge --head "$GH_HEAD" --delete-branch yes
expect_code 0 "a merge of a fork's head"
expect_sent github MergePullRequest 1
no_rest github "DELETE "
echo "  a blocked pull request sends nothing and says why"
reset_forge github "$GH_PORT"
probe "$PROBE" --forge github --host ghe.test --project acme/widgets --token blocked header 101
expect_line "MERGE blocked:a-review-is-required methods=merge,squash,rebase default=merge auto=yes enabled=- delete-default=no"
probe "$PROBE" --forge github --host ghe.test --project acme/widgets --token blocked act 101 merge --method merge --head "$GH_HEAD"
expect_code 20 "a blocked merge"
expect_line "MESSAGE It cannot be merged yet: a review is required."
expect_sent github MergePullRequest 0
echo "  waiting on checks: only merge-when-checks-pass, then cancel it"
reset_forge github "$GH_PORT"
GHW=("$PROBE" --forge github --host ghe.test --project acme/widgets --token waiting)
probe "${GHW[@]}" act 101 merge --method merge --head "$GH_HEAD"
expect_code 20 "a merge while checks run"
expect_line "MESSAGE Checks are still running."
probe "${GHW[@]}" act 101 merge --method squash --head "$GH_HEAD" --title "Ship it" --when-checks-pass yes --delete-branch yes
expect_code 0 "auto-merge"
expect_input github EnablePullRequestAutoMerge "{\"commitHeadline\":\"Ship it\",\"expectedHeadOid\":\"$GH_HEAD\",\"mergeMethod\":\"SQUASH\",$GH_ID}"
no_rest github "DELETE "
probe "${GHW[@]}" header 101
expect_line "MERGE waiting methods=merge,squash,rebase default=merge auto=yes enabled=squash delete-default=no"
probe "${GHW[@]}" act 101 cancel-auto-merge
expect_code 0 "cancel auto-merge"
expect_input github DisablePullRequestAutoMerge "{$GH_ID}"
echo "  a token without a write scope is Forbidden on a merge too"
reset_forge github "$GH_PORT"
probe "$PROBE" --forge github --host ghe.test --project acme/widgets --token scopeless act 101 merge --method merge --head "$GH_HEAD"
expect_line "ERR Forbidden"

reset_forge gitlab "$GL_PORT"
MR_ID='"iid":"201","projectPath":"team/app"'
probe "${GL[@]}" header 201
expect_code 0 "gitlab header"
expect_line "MERGE ready methods=merge,squash default=merge auto=yes enabled=- delete-default=yes"
probe "${GL[@]}" act 201 merge --method squash --head "$GL_HEAD" --title "Ship it" --message "because" --delete-branch yes
expect_code 0 "a gitlab squash merge"
expect_input gitlab MergeRequestAccept "{$MR_ID,\"sha\":\"$GL_HEAD\",\"shouldRemoveSourceBranch\":true,\"squash\":true,\"squashCommitMessage\":\"Ship it\\n\\nbecause\"}"
reset_forge gitlab "$GL_PORT"
probe "${GL[@]}" act 201 merge --method merge --head "$GL_HEAD" --title "Merge it"
expect_input gitlab MergeRequestAccept "{\"commitMessage\":\"Merge it\",$MR_ID,\"sha\":\"$GL_HEAD\",\"shouldRemoveSourceBranch\":false,\"squash\":false}"
probe "${GL[@]}" act 201 merge --method rebase --head "$GL_HEAD"
expect_code 20 "a rebase on gitlab"
expect_line "ERR Rejected"
probe "${GL[@]}" act 201 merge --method merge --head 0000000000000000000000000000000000000000
expect_line "ERR HeadMoved"
expect_sent gitlab MergeRequestAccept 1
reset_forge gitlab "$GL_PORT"
probe "$PROBE" --forge gitlab --host gitlab.test --project team/app --token blocked act 201 merge --method merge --head "$GL_HEAD"
expect_line "MESSAGE It cannot be merged yet: a review is required."
expect_sent gitlab MergeRequestAccept 0
GLW=("$PROBE" --forge gitlab --host gitlab.test --project team/app --token waiting)
probe "${GLW[@]}" act 201 merge --method merge --head "$GL_HEAD" --when-checks-pass yes
expect_code 0 "gitlab auto-merge"
expect_input gitlab MergeRequestAccept "{$MR_ID,\"sha\":\"$GL_HEAD\",\"shouldRemoveSourceBranch\":false,\"squash\":false,\"strategy\":\"MERGE_WHEN_CHECKS_PASS\"}"
probe "${GLW[@]}" act 201 cancel-auto-merge
expect_code 0 "gitlab cancel auto-merge"
expect_rest gitlab "POST /api/v4/projects/team%2Fapp/merge_requests/201/cancel_merge_when_pipeline_succeeds"
echo "  an older GitLab reports no merge capability, and nothing is offered"
probe "$PROBE" --forge gitlab --host gitlab.test --project team/app --token old header 201
expect_code 0 "an old gitlab header"
expect_line "MERGE unreported methods=- default=- auto=no enabled=- delete-default=no"
fi

if wanted metadata; then
echo "stage metadata: reviewers and labels change by one write each, with ids the forge gave"
reset_forge github "$GH_PORT"
probe "${GH[@]}" header 101
expect_line "REVIEWER_ID fake-user U_kwDOfake"
expect_line "LABEL LA_kwDObug bug"
probe "${GH[@]}" act 101 set-reviewers --add U_kwDOann --remove U_kwDOfake
expect_code 0 "github reviewers"
expect_input github RequestReviews "{$GH_ID,\"teamIds\":[\"T_kwDOcore\"],\"union\":false,\"userIds\":[\"U_kwDOann\"]}"
probe "${GH[@]}" act 101 set-reviewers
expect_code 20 "reviewers with nothing to change"
expect_line "MESSAGE There is nothing to change."
expect_sent github RequestReviews 1
probe "${GH[@]}" act 101 set-labels --add LA_kwDOfeat --remove LA_kwDObug
expect_code 0 "github labels"
expect_input github AddLabelsToLabelable '{"labelIds":["LA_kwDOfeat"],"labelableId":"PR_kwDOfake101"}'
expect_input github RemoveLabelsFromLabelable '{"labelIds":["LA_kwDObug"],"labelableId":"PR_kwDOfake101"}'
probe "${GH[@]}" candidates reviewers 101 --text ann
expect_code 0 "github reviewer candidates"
expect_var github ReviewerCandidates q ann
expect_line "CANDIDATE U_kwDOann ann Ann Lee"
no_line "CANDIDATE U_kwDOalice alice Alice"
probe "${GH[@]}" candidates labels 101 --text fe
expect_line "CANDIDATE LA_kwDOfeat feature -"

reset_forge gitlab "$GL_PORT"
probe "${GL[@]}" header 201
expect_line "REVIEWER_ID carol carol"
expect_line "LABEL gid://gitlab/ProjectLabel/1 bug"
probe "${GL[@]}" act 201 set-reviewers --add ann --remove carol
expect_code 0 "gitlab reviewers"
expect_sent gitlab MergeRequestSetReviewers 2
expect_nth_input gitlab MergeRequestSetReviewers 1 '{"iid":"201","operationMode":"APPEND","projectPath":"team/app","reviewerUsernames":["ann"]}'
expect_nth_input gitlab MergeRequestSetReviewers 2 '{"iid":"201","operationMode":"REMOVE","projectPath":"team/app","reviewerUsernames":["carol"]}'
probe "${GL[@]}" act 201 set-labels --add gid://gitlab/ProjectLabel/2
expect_code 0 "gitlab labels"
expect_sent gitlab MergeRequestSetLabels 1
expect_input gitlab MergeRequestSetLabels '{"iid":"201","labelIds":["gid://gitlab/ProjectLabel/2"],"operationMode":"APPEND","projectPath":"team/app"}'
probe "${GL[@]}" candidates reviewers 201 --text ann
expect_var gitlab ReviewerCandidates q ann
expect_line "CANDIDATE ann ann Ann Lee"
probe "${GL[@]}" candidates labels 201 --text fe
expect_line "CANDIDATE gid://gitlab/ProjectLabel/2 feature -"
echo "  a GitLab without the reviewers mutation says so, rather than failing to read"
probe "$PROBE" --forge gitlab --host gitlab.test --project team/app --token old act 201 set-reviewers --add ann
expect_code 20 "reviewers on an old gitlab"
expect_line "ERR Unsupported"
fi

if wanted scopes; then
echo "stage scopes: what a token says it may do, for Settings"
probe "${GH[@]}" scopes
expect_code 0 "the scopes of a classic github token"
expect_line "SCOPES repo,read:org"
expect_line "CAN_WRITE yes"
probe "$PROBE" --forge github --host ghe.test --project acme/widgets --token readonly scopes
expect_line "SCOPES read:org"
expect_line "CAN_WRITE no"
probe "$PROBE" --forge github --host ghe.test --project acme/widgets --token finegrained scopes
expect_line "SCOPES not-reported"
probe "${GL[@]}" scopes
expect_line "SCOPES api,read_api"
expect_line "CAN_WRITE yes"
probe "$PROBE" --forge gitlab --host gitlab.test --project team/app --token readonly scopes
expect_line "SCOPES read_api"
expect_line "CAN_WRITE no"
probe "$PROBE" --forge gitlab --host gitlab.test --project team/app --token finegrained scopes
expect_line "SCOPES not-reported"
probe "$PROBE" --forge github --host ghe.test --project acme/widgets --token expired scopes
expect_line "SCOPES not-reported"
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
  reset_forge github "$GH_PORT"
  probe GH_CONFIG_DIR="$WORK/gh-good" "${GH_CLI[@]}" "$PROBE" "${GH_CLI_ARGS[@]}" act 101 merge --method squash --head "$GH_HEAD" --title "Via gh" --delete-branch yes
  expect_code 0 "a merge through gh, then the branch deletion (REST DELETE)"
  expect_line "ACT ok"
  ! echo "$PROBE_OUT" | grep -q "^WARNING" || { dump; fail "the branch deletion through gh failed"; }
  expect_input github MergePullRequest "{\"commitHeadline\":\"Via gh\",\"expectedHeadOid\":\"$GH_HEAD\",\"mergeMethod\":\"SQUASH\",$GH_ID}"
  expect_rest github "DELETE /repos/acme/widgets/git/refs/heads/feat/login"  # gh drops /api/v3 for *.localhost
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
  reset_forge gitlab "$GL_PORT"
  write_glab_config "$WORK/glab-waiting" waiting
  probe GLAB_CONFIG_DIR="$WORK/glab-waiting" "$PROBE" "${GL_CLI_ARGS[@]}" act 201 merge --method merge --head "$GL_HEAD" --when-checks-pass yes
  expect_code 0 "an auto-merge through glab"
  probe GLAB_CONFIG_DIR="$WORK/glab-waiting" "$PROBE" "${GL_CLI_ARGS[@]}" act 201 cancel-auto-merge
  expect_code 0 "a cancel of the auto-merge (REST) through glab"
  expect_rest gitlab "POST /api/v4/projects/team%2Fapp/merge_requests/201/cancel_merge_when_pipeline_succeeds"
else
  echo "SKIP: glab is not on PATH -- writes through glab (the REST approval and the REST cancel of an auto-merge) were not exercised"
fi
fi


if wanted ui; then
echo "stage ui: a real Sirio, the change request tab, every action of B2a and B2b"
BIN="${CARGO_TARGET_DIR:-$CARGO_DIR/target}/debug/sirio"
CTL="${CARGO_TARGET_DIR:-$CARGO_DIR/target}/debug/sirioctl"
if [ "$STATE_ONLY" -eq 0 ]; then
  [ -n "$DISPLAY_TARGET" ] || fail "no DISPLAY; pass --display :N or --state-only"
  for tool in import identify xwininfo xprop; do
    command -v "$tool" >/dev/null || fail "$tool is required for captures; pass --state-only to skip them"
  done
fi
echo "building sirio and sirioctl"
(cd "$CARGO_DIR" && cargo build --quiet -p sirio --bin sirio && cargo build --quiet -p sirio_control --bin sirioctl)

APP_PID=""
stop_app() {
  [ -z "$APP_PID" ] || { kill "$APP_PID" 2>/dev/null || true; wait "$APP_PID" 2>/dev/null || true; APP_PID=""; }
}
trap 'stop_app; cleanup' EXIT

ctl() { echo "+ sirioctl $*"; "$CTL" "$@"; }
reply() { "$CTL" "$@" --json; }
field() { python3 -c 'import json, sys; print(json.load(sys.stdin)[0].get(sys.argv[1], ""))' "$1"; }
key() { reply "${@:2}" | field "$1"; } # key sirioctl-args...
wait_for() { # key value sirioctl-args...
  local want_key=$1 want=$2
  shift 2
  local got=""
  for _ in $(seq 1 100); do
    got=$(reply "$@" | field "$want_key" || true)
    [ "$got" = "$want" ] && { echo "OK: $want_key=$want"; return 0; }
    sleep 0.3
  done
  reply "$@" || true
  fail "$want_key never became '$want' (last: '$got') for: $*"
}
find_window() {
  DISPLAY_TARGET="$DISPLAY_TARGET" APP_PID="$APP_PID" python3 - <<'PY'
import os, re, subprocess
display = os.environ["DISPLAY_TARGET"]
want = int(os.environ["APP_PID"])
listing = subprocess.run(["xwininfo", "-display", display, "-root", "-children"], capture_output=True, text=True, timeout=5).stdout
best = None
for line in listing.splitlines():
    match = re.match(r"\s+(0x[0-9a-fA-F]+).*?\s(\d+)x(\d+)\+", line)
    if not match:
        continue
    window, width, height = match.group(1), int(match.group(2)), int(match.group(3))
    prop = subprocess.run(["xprop", "-display", display, "-id", window, "_NET_WM_PID"], capture_output=True, text=True, timeout=5).stdout
    pid = re.search(r"=\s*(\d+)\s*$", prop)
    if pid and int(pid.group(1)) == want and (best is None or width * height > best[0]):
        best = (width * height, window)
if best:
    print(best[1])
PY
}
capture() { # name
  [ "$STATE_ONLY" -eq 0 ] || return 0
  sleep 2
  local window
  window=$(find_window)
  [ -n "$window" ] || fail "no window with _NET_WM_PID=$APP_PID for $1"
  import -display "$DISPLAY_TARGET" -window "$window" "$OUT_DIR/frames/$1.png"
  local colours
  colours=$(identify -format '%k' "$OUT_DIR/frames/$1.png")
  [ "$colours" -ge 200 ] || fail "$1 is blank ($colours colours)"
  echo "FRAME: $1 ($colours colours)"
}

# Where the change request tab is in the tab list, so it can be closed and
# opened again -- which is how it picks up another token.
reopen_tab() { # number
  local index
  index=$(reply surface tabs read | python3 -c '
import json, sys
row = json.load(sys.stdin)[0]
print(next((name.split(".")[1] for name, value in row.items() if name.startswith("tab.") and value.startswith("change_request|")), ""))')
  [ -z "$index" ] || ctl surface tabs close "$index" >/dev/null
  ctl surface change-request open "$1" >/dev/null
  wait_for state loaded surface change-request read
}
saved_token() { # host forge token
  local account
  account=$(reply surface change-requests token --host "$1" --forge "$2" --token "$3" | field account)
  [ "$account" = "fake-user" ] || fail "token $3 signed in as '$account'"
  # A new token makes the right panel connect again; the tab's list is what
  # `surface change-request open` reads, so it waits for it.
  wait_for state ready surface change-requests read >/dev/null
}

run_ui() { # flavour host project number origin-url commentIndex noteOperation editedTitle
  local flavour=$1 host=$2 number=$4 origin=$5 comment_index=$6 comment_op=$7
  local port=$GH_PORT
  [ "$flavour" = gitlab ] && port=$GL_PORT
  local run_dir
  run_dir=$(mktemp -d "${TMPDIR:-/tmp}/sirio-forge-actions-XXXXXX")
  reset_forge "$flavour" "$port"

  local repo="$run_dir/repo"
  mkdir -p "$repo"
  git -C "$repo" init -q -b main
  git -C "$repo" config user.email t@example.com
  git -C "$repo" config user.name Tester
  git -C "$repo" commit -q --allow-empty -m first
  git -C "$repo" checkout -q -b feat/work
  git -C "$repo" remote add origin "$origin"

  export SIRIO_SOCKET="$run_dir/control.sock" SIRIO_DB="$run_dir/session.sqlite" SIRIO_CREDENTIALS="$run_dir/credentials.json"
  export GH_CONFIG_DIR="$run_dir/gh" GLAB_CONFIG_DIR="$run_dir/glab"
  mkdir -p "$GH_CONFIG_DIR" "$GLAB_CONFIG_DIR"
  chmod 700 "$GLAB_CONFIG_DIR"
  if [ "$STATE_ONLY" -eq 1 ]; then
    (cd "$repo" && exec env -u DISPLAY -u WAYLAND_DISPLAY "$BIN" >"$run_dir/app.log" 2>&1) &
  else
    (cd "$repo" && exec env -u WAYLAND_DISPLAY DISPLAY="$DISPLAY_TARGET" GPUI_X11_SCALE_FACTOR=1 "$BIN" >"$run_dir/app.log" 2>&1) &
  fi
  APP_PID=$!
  for _ in $(seq 1 75); do [ -S "$SIRIO_SOCKET" ] && break; sleep 0.2; done
  [ -S "$SIRIO_SOCKET" ] || fail "sirio never opened its control socket"

  ctl project add "$repo" >/dev/null
  ctl select-workspace --workspace "$repo" >/dev/null
  ctl surface change-requests show >/dev/null
  saved_token "$host" "$flavour" good
  ctl surface change-request open "$number" >/dev/null
  wait_for state loaded surface change-request read
  wait_for caps "comment,approve,request-changes,edit,state,draft" surface change-request read
  wait_for cr_state open surface change-request read
  wait_for action idle surface change-request read
  capture "$flavour-tab"

  sent() { sent_count "$flavour" "$1"; }
  local comment_mutation="AddComment" review_mutation="AddPullRequestReview"
  if [ "$flavour" = gitlab ]; then comment_mutation="CreateNote"; fi

  echo "  [$flavour] the composer: words go out exactly, and only success clears them"
  ctl surface change-request act compose --text $'He said "ok" \\ naïve café ☕\n\ttabbed' >/dev/null
  [ "$(key composer_len surface change-request read)" != 0 ] || fail "the composer took no text"
  ctl surface change-request act send --how comment >/dev/null
  wait_for action idle surface change-request read
  wait_for composer_len 0 surface change-request read
  expect_sent "$flavour" "$comment_mutation" 1
  "$PYTHON" - "$WORK/$flavour-requests.log" "$comment_mutation" <<'PY' || fail "the comment reached the forge changed"
import json, re, sys
last = None
for line in open(sys.argv[1], encoding="utf-8"):
    found = re.match(r"^POST \S+ (\S+) interaction=\w+ vars=(.*)$", line)
    if found and found.group(1) == sys.argv[2]:
        last = json.loads(found.group(2))["input"]["body"]
sys.exit(0 if last == 'He said "ok" \\ naïve café ☕\n\ttabbed' else 1)
PY
  echo "  [$flavour] an approval needs no words; its own button, its own request"
  ctl surface change-request act send --how approve >/dev/null
  wait_for action idle surface change-request read
  if [ "$flavour" = gitlab ]; then expect_rest gitlab "POST /api/v4/projects/team%2Fapp/merge_requests/201/approve"; else expect_sent github "$review_mutation" 1; fi
  echo "  [$flavour] a comment with no words is refused, and nothing is sent"
  before=$(sent "$comment_mutation")
  ctl surface change-request act send --how comment >/dev/null
  wait_for action failed surface change-request read
  wait_for action_message "A comment needs some text." surface change-request read
  [ "$(sent "$comment_mutation")" = "$before" ] || fail "an empty comment reached the forge"

  echo "  [$flavour] the state comes back from the forge: close, reopen, draft, ready"
  reset_forge "$flavour" "$port"
  ctl surface change-request act close >/dev/null
  wait_for cr_state closed surface change-request read
  wait_for caps "comment,edit,state" surface change-request read
  capture "$flavour-closed"
  # No reset from here to the end of the sequence: the fake remembers each write,
  # so a reopen finds a closed change request and a ready finds a draft.
  ctl surface change-request act reopen >/dev/null
  wait_for cr_state open surface change-request read
  ctl surface change-request act draft >/dev/null
  wait_for cr_state draft surface change-request read
  ctl surface change-request act ready >/dev/null
  wait_for cr_state open surface change-request read

  echo "  [$flavour] Edit: only what changed is sent, and the tab's title follows the forge"
  reset_forge "$flavour" "$port"
  ctl surface change-request act edit --title "$(key title surface change-request read)" >/dev/null
  wait_for editing no surface change-request read
  [ "$(sent UpdatePullRequest)$(sent MergeRequestUpdate)" = "00" ] || fail "saving an untouched edit sent something"
  ctl surface change-request act edit --title "Better" --body "New description" --target develop >/dev/null
  wait_for action idle surface change-request read
  wait_for editing no surface change-request read
  wait_for title "$([ "$flavour" = gitlab ] && echo Better || echo 'A better title')" surface change-request read

  echo "  [$flavour] one's own comment can be edited; another's cannot"
  reset_forge "$flavour" "$port"
  ctl surface change-request act edit-comment --index "$comment_index" --text "Edited comment" >/dev/null
  wait_for action idle surface change-request read
  wait_for comment_editing "" surface change-request read
  expect_sent "$flavour" "$comment_op" 1
  if ctl surface change-request act edit-comment --index 0 --text "no" >/dev/null 2>&1; then fail "an event was edited"; fi
  expect_sent "$flavour" "$comment_op" 1
  wait_for comment_editing "" surface change-request read
  # A refused save leaves its editor open; editing another entry must not
  # reuse that stale editor.
  ctl surface change-request act edit-comment --index "$comment_index" --text "" >/dev/null
  wait_for action failed surface change-request read
  [ -n "$(key comment_editing surface change-request read)" ] || fail "the refused save left no editor open"
  if ctl surface change-request act edit-comment --index 0 --text "X" >/dev/null 2>"$WORK/stale-err.txt"; then fail "a stale editor edited the wrong comment"; fi
  case "$(cat "$WORK/stale-err.txt")" in *"that timeline entry cannot be edited"*) ;; *) fail "the stale edit was refused for the wrong reason" ;; esac
  expect_sent "$flavour" "$comment_op" 1
  wait_for comment_editing "" surface change-request read
  # Saving the words the entry already holds sends nothing.
  ctl surface change-request act edit-comment --index "$comment_index" --text "Edited comment" >/dev/null
  wait_for comment_editing "" surface change-request read
  expect_sent "$flavour" "$comment_op" 1

  echo "  [$flavour] merge: the strip, the dialog, and the head the user saw"
  local merge_op=MergePullRequest head=$GH_HEAD head_key=expectedHeadOid
  local auto_op=EnablePullRequestAutoMerge
  local reviewers_op=RequestReviews labels_op=AddLabelsToLabelable ann=U_kwDOann feature=LA_kwDOfeat
  if [ "$flavour" = gitlab ]; then
    merge_op=MergeRequestAccept head=$GL_HEAD head_key=sha auto_op=MergeRequestAccept
    reviewers_op=MergeRequestSetReviewers labels_op=MergeRequestSetLabels ann=ann feature=gid://gitlab/ProjectLabel/2
  fi
  reset_forge "$flavour" "$port"
  reopen_tab "$number"
  wait_for merge_strip merge surface change-request read
  wait_for merge_verdict ready surface change-request read
  capture "$flavour-merge-strip"
  ctl surface change-request act merge-open --method squash >/dev/null
  wait_for merge_dialog open surface change-request read
  capture "$flavour-merge-dialog"
  ctl surface change-request act merge-confirm --title "Ship it" --message "because" >/dev/null
  wait_for merge_dialog closed surface change-request read
  wait_for cr_state merged surface change-request read
  wait_for merge_strip none surface change-request read
  expect_sent "$flavour" "$merge_op" 1
  [ "$(sent_input "$flavour" "$merge_op" | "$PYTHON" -c 'import json,sys; print(json.load(sys.stdin)[sys.argv[1]])' "$head_key")" = "$head" ] || fail "the merge did not carry the head the user saw"

  echo "  [$flavour] someone pushes between the dialog and the click: nothing is merged"
  reset_forge "$flavour" "$port"
  reopen_tab "$number"
  ctl surface change-request act merge-open --method merge >/dev/null
  curl -s -o /dev/null -X POST "http://127.0.0.1:$port/__push"
  ctl surface change-request act merge-confirm >/dev/null
  wait_for merge_dialog closed surface change-request read
  wait_for action failed surface change-request read
  case "$(key action_message surface change-request read)" in *"The branch changed since you opened this"*) ;; *) fail "a moved head was not named" ;; esac
  expect_sent "$flavour" "$merge_op" 0

  echo "  [$flavour] a blocked change request offers no merge"
  saved_token "$host" "$flavour" blocked
  reopen_tab "$number"
  reset_forge "$flavour" "$port"
  wait_for merge_strip blocked surface change-request read
  case "$(key merge_message surface change-request read)" in *"a review is required"*) ;; *) fail "the block's reason was not shown" ;; esac
  capture "$flavour-merge-blocked"
  if ctl surface change-request act merge-open >/dev/null 2>&1; then fail "a blocked change request opened the merge dialog"; fi
  expect_sent "$flavour" "$merge_op" 0

  echo "  [$flavour] checks still running: merge when they pass, then cancel it"
  saved_token "$host" "$flavour" waiting
  reopen_tab "$number"
  reset_forge "$flavour" "$port"
  wait_for merge_strip auto-merge surface change-request read
  ctl surface change-request act merge-open --when-checks-pass yes >/dev/null
  ctl surface change-request act merge-confirm >/dev/null
  wait_for action idle surface change-request read
  wait_for merge_strip cancel surface change-request read
  expect_sent "$flavour" "$auto_op" 1
  capture "$flavour-merge-cancel"
  ctl surface change-request act cancel-auto-merge >/dev/null
  wait_for action idle surface change-request read
  if [ "$flavour" = gitlab ]; then expect_rest gitlab "POST /api/v4/projects/team%2Fapp/merge_requests/201/cancel_merge_when_pipeline_succeeds"; else expect_sent github DisablePullRequestAutoMerge 1; fi

  echo "  [$flavour] reviewers and labels: one write when the picker closes, none when nothing changed"
  saved_token "$host" "$flavour" good
  reset_forge "$flavour" "$port"
  reopen_tab "$number"
  ctl surface change-request act picker-open --kind reviewers >/dev/null
  wait_for picker reviewers surface change-request read
  ctl surface change-request act picker-type --text ann --now yes >/dev/null
  wait_for picker_candidates 1 surface change-request read
  capture "$flavour-picker"
  ctl surface change-request act picker-pick --id "$ann" >/dev/null
  ctl surface change-request act picker-close >/dev/null
  wait_for picker closed surface change-request read
  wait_for action idle surface change-request read
  expect_sent "$flavour" "$reviewers_op" 1
  ctl surface change-request act picker-open --kind labels >/dev/null
  wait_for picker labels surface change-request read
  ctl surface change-request act picker-close >/dev/null
  wait_for picker closed surface change-request read
  expect_sent "$flavour" "$labels_op" 0
  ctl surface change-request act picker-open --kind labels >/dev/null
  ctl surface change-request act picker-type --text fe --now yes >/dev/null
  wait_for picker_candidates 1 surface change-request read
  ctl surface change-request act picker-pick --id "$feature" >/dev/null
  ctl surface change-request act picker-pick --id "$feature" >/dev/null
  ctl surface change-request act picker-pick --id "$feature" >/dev/null
  ctl surface change-request act picker-close >/dev/null
  wait_for action idle surface change-request read
  expect_sent "$flavour" "$labels_op" 1

  echo "  [$flavour] a double send while the first is in flight is one request; words typed meanwhile stay"
  saved_token "$host" "$flavour" slow
  reopen_tab "$number"
  reset_forge "$flavour" "$port"
  ctl surface change-request act compose --text "Once" >/dev/null
  ctl surface change-request act send --how comment >/dev/null
  if ctl surface change-request act send --how comment >/dev/null 2>&1; then fail "a second send was accepted while the first was in flight"; fi
  # The first send is still in flight (this forge answers after 1.5 s): what is
  # typed now is not what was sent, so success must not wipe it.
  ctl surface change-request act compose --text "typed meanwhile" >/dev/null
  wait_for action idle surface change-request read
  wait_for composer_len 15 surface change-request read
  expect_sent "$flavour" "$comment_mutation" 1

  echo "  [$flavour] a token that cannot write: the reason is shown and the words stay"
  saved_token "$host" "$flavour" scopeless
  reopen_tab "$number"
  reset_forge "$flavour" "$port"
  ctl surface change-request act compose --text "keep me" >/dev/null
  ctl surface change-request act send --how comment >/dev/null
  wait_for action failed surface change-request read
  case "$(key action_message surface change-request read)" in *"cannot write here"*) ;; *) fail "the write scope was not named" ;; esac
  wait_for composer_len 7 surface change-request read
  capture "$flavour-failed"

  echo "  [$flavour] a connection dropped after the send: it is looked at, never resent"
  saved_token "$host" "$flavour" dropped
  reopen_tab "$number"
  reset_forge "$flavour" "$port"
  ctl surface change-request act compose --text "maybe sent" >/dev/null
  ctl surface change-request act send --how comment >/dev/null
  wait_for action unconfirmed surface change-request read
  wait_for composer_len 10 surface change-request read
  expect_sent "$flavour" "$comment_mutation" 1
  # The header was read again before the tab said anything.
  [ "$(sent_count "$flavour" "$([ "$flavour" = gitlab ] && echo MergeRequestHeader || echo ChangeRequestHeader)")" -ge 1 ] || fail "the tab did not look again after an unconfirmed write"

  echo "  [$flavour] a viewer who may not act sees no action, and one forced is refused unsent"
  saved_token "$host" "$flavour" readonly
  reopen_tab "$number"
  reset_forge "$flavour" "$port"
  wait_for caps "-" surface change-request read
  ctl surface change-request act compose --text "not allowed" >/dev/null
  ctl surface change-request act send --how comment >/dev/null
  wait_for action failed surface change-request read
  wait_for action_message "You cannot comment on this change request." surface change-request read
  expect_sent "$flavour" "$comment_mutation" 0
  capture "$flavour-readonly"
  if [ "$flavour" = gitlab ]; then
  echo "  [$flavour] an approval whose comment is refused keeps the words"
  saved_token "$host" "$flavour" notefails
  reopen_tab "$number"
  reset_forge "$flavour" "$port"
  ctl surface change-request act compose --text "words that stay" >/dev/null
  ctl surface change-request act send --how approve >/dev/null
  wait_for action warning surface change-request read
  wait_for composer_len 15 surface change-request read
  [ "$(grep -cF "POST /api/v4/projects/team%2Fapp/merge_requests/201/approve " "$WORK/gitlab-requests.log")" = 1 ] || fail "the approval did not reach GitLab exactly once"
  expect_sent gitlab CreateNote 1
  fi
  stop_app
  cp "$run_dir/app.log" "$OUT_DIR/app-$flavour.log" 2>/dev/null || true
  rm -rf "$run_dir"
}

run_ui github ghe.test acme/widgets 101 https://ghe.test/acme/widgets.git 1 UpdateIssueComment
run_ui gitlab gitlab.test team/app 201 https://gitlab.test/team/app.git 1 UpdateNote
fi

# Later stages are added above this line.

echo "artifact: $OUT_DIR"
echo "FORGE ACTIONS E2E OK"
