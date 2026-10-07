#!/bin/bash
set -euo pipefail

# End-to-end test of checking a change request out into a worktree (change
# requests C1, spec 2026-10-07-change-request-handoff-design.md §10-§11): a
# real, isolated Sirio against the loopback fake forge
# (Scripts/Tests/fake_forge.py) and real local git repositories, driven over
# the control socket. The forge's git side is a bare repository per host
# (origin) and one per fork (sirio-<owner>-<number>), reached through insteadOf rewrites
# of the project's remote URLs, so nothing leaves the machine.
#
# Scenarios, each for GitHub and GitLab where the forge allows it:
#   same            the branch is in the project's repository: checkout creates
#                   <project>-feat tracking origin/feat and selects it (the
#                   selected workspace's path is that folder, and the change
#                   request tab reads in it); a push from it lands on origin; a
#                   second checkout fast-forwards a clean worktree to a commit
#                   pushed meanwhile (the forge's head moves with each push),
#                   refuses a dirty one, leaves a worktree switched to another
#                   branch alone (its link no longer shows the change request),
#                   leaves a detached one alone, and recreates a worktree whose
#                   folder was deleted with rm -rf (git still lists it).
#   fork-push       a fork that accepts pushes (GitHub: the head is read-only to
#                   the viewer, the base is writable and maintainerCanModify is
#                   set): a worktree on <owner>/feat with its own remote; a push
#                   from it lands on the fork's branch, and origin has no such
#                   branch.
#   two-forks       two fork change requests (#101 feat, #102 fix) from one owner:
#                   each has its own remote with exactly one push mapping, and a
#                   plain push from one worktree moves only its own fork branch.
#   own-fork        the viewer's own fork is origin and the base is upstream: the
#                   branch is feat, tracking origin/feat, with no sirio- remote.
#   fork-readonly   a fork that refuses pushes: a read-only worktree at the
#                   head commit; when the forge's head moves on, a second
#                   checkout fast-forwards it; when the fork starts accepting
#                   pushes, the same worktree gets its remote, upstream and one
#                   push mapping. GitLab's variant has allowCollaboration
#                   true and the target's pushCode false, so only the stricter rule
#                   makes it read-only.
#   gone            the source repository is gone and the change request is CLOSED
#                   (spec §10): the checkout still completes, read-only at the head,
#                   on the local branch pr-101, because refs/pull/N/head survives a
#                   closed request.
#   refusals        a branch that points at other commits, a project with no remote
#                   for the forge, and a second checkout while one runs.
#
# The artifact: --out-dir DIR (default artifacts/handoff-e2e-<stamp>-<pid>)
# keeps transcript.log, one app log per launch, one fake-forge request log per
# scenario, the state of every repository per scenario (<scenario>-git.txt: the
# project checkout's worktrees, remotes and branches, and the refs of the forge's
# origin and fork bares) and -- unless --state-only -- PID-matched window
# captures in frames/. A rerun clears the previous artifacts first.
#
# Usage: Scripts/Tests/test-change-request-handoff-e2e.sh [--state-only] [--out-dir DIR] [--display :N]


ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/sirio"
CTL="${CARGO_TARGET_DIR:-$ROOT/rust/target}/debug/sirioctl"
STATE_ONLY=0
OUT_DIR=""
DISPLAY_TARGET="${DISPLAY:-}"
while [ $# -gt 0 ]; do
  case "$1" in
    --state-only) STATE_ONLY=1; shift ;;
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    --display) DISPLAY_TARGET="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/handoff-e2e-$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$OUT_DIR/frames"
rm -f "$OUT_DIR"/*.log "$OUT_DIR"/*-git.txt "$OUT_DIR"/frames/* 2>/dev/null || true
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

fail() { echo "FAIL: $*" >&2; exit 1; }
if [ "$STATE_ONLY" -eq 0 ]; then
  [ -n "$DISPLAY_TARGET" ] || fail "no DISPLAY; pass --display :N or --state-only"
  for tool in import identify xwininfo xprop; do
    command -v "$tool" >/dev/null || fail "$tool is required for captures; pass --state-only to skip them"
  done
fi
command -v git >/dev/null || fail "git is required"
command -v python3 >/dev/null || fail "python3 is required"
command -v curl >/dev/null || fail "curl is required (the fake forge's readiness probe)"

RUN_DIR=$(mktemp -d "${TMPDIR:?TMPDIR must be set; the run never falls back to /tmp}/sirio-handoff-e2e-XXXXXX")
APP_PID=""
FORGE_PID=""
stop_app() { [ -z "$APP_PID" ] || { kill "$APP_PID" 2>/dev/null || true; wait "$APP_PID" 2>/dev/null || true; APP_PID=""; }; }
stop_forge() { [ -z "$FORGE_PID" ] || { kill "$FORGE_PID" 2>/dev/null || true; wait "$FORGE_PID" 2>/dev/null || true; FORGE_PID=""; }; }
cleanup() {
  stop_app
  stop_forge
  cp "$RUN_DIR"/*.log "$OUT_DIR/" 2>/dev/null || true
  rm -rf "$RUN_DIR"
}
trap cleanup EXIT

echo "building sirio and sirioctl"
(cd "$ROOT/rust" && cargo build --quiet -p sirio --bin sirio && cargo build --quiet -p sirio_control --bin sirioctl)

# ---- helpers (the shape test-forge-ui-e2e.sh uses) ---------------------------

ctl() { echo "+ sirioctl $*"; "$CTL" "$@"; }
reply() { "$CTL" "$@" --json; }
field() { python3 -c 'import json, sys; print(json.load(sys.stdin)[0].get(sys.argv[1], ""))' "$1"; }
# One key of one reply, for `x=$(read_field key args...)`: a sirioctl error
# prints a FAIL line instead of aborting the script silently inside the
# substitution (set -e does not label it).
read_field() { # key sirioctl-args...
  local key=$1
  shift
  reply "$@" | field "$key" || fail "sirioctl $* did not answer (reading $key)"
}
wait_for() { # key value sirioctl-args...
  local key=$1 want=$2
  shift 2
  local got=""
  for _ in $(seq 1 100); do
    got=$(reply "$@" | field "$key" || true)
    [ "$got" = "$want" ] && { echo "OK: $key=$want"; return 0; }
    sleep 0.3
  done
  reply "$@" || true
  fail "$key never became '$want' (last: '$got') for: $*"
}
assert_contains() { # key needle sirioctl-args...
  local key=$1 needle=$2
  shift 2
  local got
  got=$(read_field "$key" "$@") || fail "assert_contains: no reply for: $*"
  case "$got" in
    *"$needle"*) echo "OK: $key contains '$needle'" ;;
    *) fail "$key was '$got', expected it to contain '$needle'" ;;
  esac
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
  import -display "$DISPLAY_TARGET" -window "$window" "$OUT_DIR/frames/$SCENARIO-$1.png"
  local colours
  colours=$(identify -format '%k' "$OUT_DIR/frames/$SCENARIO-$1.png")
  [ "$colours" -ge 200 ] || fail "$1 is blank ($colours colours)"
  echo "FRAME: $SCENARIO-$1 ($colours colours)"
}


# ---- the forge's git side and its fixtures -----------------------------------

# build_repos flavour head-kind -> SRC BASE HEAD_SHA ORIGIN FORK
#   head-kind: same | fork-push | fork-readonly | gone
#   ORIGIN is the project's bare; it carries refs/heads/feat only for `same`,
#   and always the forge's head ref (refs/pull/101/head or
#   refs/merge-requests/201/head). FORK is the fork's bare with refs/heads/feat.
build_repos() {
  local flavour=$1 kind=$2 dir="$RUN_DIR/git-$SCENARIO"
  rm -rf "$dir"; mkdir -p "$dir"
  SRC="$dir/src"; ORIGIN="$dir/origin.git"; FORK="$dir/fork.git"
  git init -q -b main "$SRC"
  git -C "$SRC" config user.email e2e@sirio.dev; git -C "$SRC" config user.name E2E
  echo base >"$SRC/a.txt"; git -C "$SRC" add .; git -C "$SRC" commit -q -m base
  BASE=$(git -C "$SRC" rev-parse HEAD)
  git -C "$SRC" checkout -q -b feat
  echo feat >"$SRC/a.txt"; git -C "$SRC" commit -q -am feat
  HEAD_SHA=$(git -C "$SRC" rev-parse HEAD)
  git -C "$SRC" checkout -q main
  echo main >"$SRC/m.txt"; git -C "$SRC" add .; git -C "$SRC" commit -q -m main-moves
  git clone -q --bare "$SRC" "$ORIGIN"
  git clone -q --bare "$SRC" "$FORK"
  local pull_ref
  pull_ref=$(pull_ref_of "$flavour")
  git -C "$ORIGIN" update-ref "$pull_ref" "$HEAD_SHA"
  [ "$kind" = same ] || git -C "$ORIGIN" update-ref -d refs/heads/feat
  [ "$kind" = gone ] && git -C "$FORK" update-ref -d refs/heads/feat
  return 0
}

pull_ref_of() { # flavour -> the forge's ref for the change request's head
  if [ "$1" = gitlab ]; then echo refs/merge-requests/201/head; else echo refs/pull/101/head; fi
}

# owner_of flavour head-kind -> the owner the forge names for the head repository
owner_of() {
  case "$1:$2" in
    github:same|github:gone) echo acme ;;
    gitlab:same|gitlab:gone) echo team ;;
    github:*) echo alice ;;
    gitlab:*) echo forks/alice ;;
  esac
}

# make_project dir origin-url fork-url -> WT: the project's main checkout,
# its remote URLs rewritten to the local bares.
make_project() {
  WT=$1
  git init -q -b main "$WT"
  git -C "$WT" config user.email e2e@sirio.dev; git -C "$WT" config user.name E2E
  git -C "$WT" commit -q --allow-empty -m init
  git -C "$WT" remote add origin "$2"
  git -C "$WT" config "url.$ORIGIN.insteadOf" "$2"
  git -C "$WT" config "url.$FORK.insteadOf" "$3"
}

# make_next_head flavour -> NEXT_HEAD: a commit on top of HEAD_SHA, which the
# forge's head ref moves to in the fork-readonly scenario (never before then).
make_next_head() {
  local scratch="$RUN_DIR/next-$SCENARIO"
  rm -rf "$scratch"
  git clone -q "$ORIGIN" "$scratch"
  git -C "$scratch" fetch -q "$ORIGIN" "$(pull_ref_of "$1"):refs/heads/pr"
  git -C "$scratch" checkout -q pr
  git -C "$scratch" config user.email e2e@sirio.dev; git -C "$scratch" config user.name E2E
  echo "the head moves on" >"$scratch/moved.txt"
  git -C "$scratch" add moved.txt; git -C "$scratch" commit -q -m "the head moves on"
  NEXT_HEAD=$(git -C "$scratch" rev-parse HEAD)
}

# render_fixtures flavour head-kind owner base head next-head -> FIXTURES_DIR
render_fixtures() {
  FIXTURES_DIR="$RUN_DIR/fixtures-$SCENARIO"
  rm -rf "$FIXTURES_DIR"
  cp -r "$ROOT/Scripts/Tests/forge-fixtures" "$FIXTURES_DIR"
  python3 - "$1" "$2" "$3" "$4" "$5" "$6" "$FIXTURES_DIR" <<'PY'
import copy, json, sys
flavour, kind, owner, base, head, next_head, root = sys.argv[1:8]
def load(rel):
    return json.load(open(f"{root}/{rel}"))
def save(rel, data):
    json.dump(data, open(f"{root}/{rel}", "w"))
cross = kind in ("fork-push", "fork-readonly")
if flavour == "github":
    def patch(node):
        node["baseRefOid"], node["headRefOid"] = base, head
        node["headRefName"] = "feat"
        node["isCrossRepository"] = cross
        node["maintainerCanModify"] = kind == "fork-push"
        node["headRef"] = None if kind == "gone" else {"id": "REF_1"}
        # A closed request whose head repository was deleted (spec §10): the
        # checkout still works, because refs/pull/N/head survives the close.
        # The forge then drops the head owner with the repository.
        gone = kind == "gone"
        if gone:
            node["state"] = "CLOSED"
        node["headRepositoryOwner"] = None if gone else {"login": owner}
        # The viewer reads the head fork; a maintainer's write comes from
        # maintainerCanModify and the base's WRITE, which fork-push relies on.
        node["headRepository"] = None if gone else {
            "nameWithOwner": f"{owner}/widgets",
            "url": f"https://ghe.test/{owner}/widgets",
            "sshUrl": f"git@ghe.test:{owner}/widgets.git",
            "viewerPermission": "WRITE" if kind == "same" else "READ",
        }
    for name in ("ChangeRequestHeader", "ChangeRequestByNumber"):
        data = load(f"github/{name}.json")
        patch(data["data"]["repository"]["pullRequest"])
        save(f"github/{name}.json", data)
    data = load("github/ChangeRequestHeader.json")
    moved = copy.deepcopy(data)
    moved["data"]["repository"]["pullRequest"]["headRefOid"] = next_head
    save("github/ChangeRequestHeader.after.push.json", moved)
    data = load("github/ChangeRequestActionContext.json")
    data["data"]["repository"]["pullRequest"]["headRefOid"] = head
    save("github/ChangeRequestActionContext.json", data)
else:
    def patch(node):
        node["diffRefs"] = {"baseSha": base, "headSha": head, "startSha": base}
        node["diffHeadSha"] = head
        node["sourceBranch"] = "feat"
        node["sourceProjectId"] = 7 if kind == "same" else 8
        node["targetProjectId"] = 7
        node["sourceBranchExists"] = kind != "gone"
        # A fork's collaboration is granted only with push access to the target,
        # and the read-only fork case grants collaboration but withholds the
        # target's push: the stricter rule is the one that must decide it.
        node["allowCollaboration"] = kind in ("fork-push", "fork-readonly")
        node["sourceProject"] = {
            "fullPath": f"{owner}/app",
            "httpUrlToRepo": f"https://gitlab.test/{owner}/app.git",
            "sshUrlToRepo": f"git@gitlab.test:{owner}/app.git",
            "userPermissions": {"pushCode": kind == "same"},
        }
        node["targetProject"] = {"userPermissions": {"pushCode": kind != "fork-readonly"}}
    for name in ("MergeRequestHeader", "MergeRequestByNumber"):
        data = load(f"gitlab/{name}.json")
        patch(data["data"]["project"]["mergeRequest"])
        save(f"gitlab/{name}.json", data)
    data = load("gitlab/MergeRequestHeader.json")
    moved = copy.deepcopy(data)
    moved_node = moved["data"]["project"]["mergeRequest"]
    moved_node["diffRefs"]["headSha"] = next_head
    moved_node["diffHeadSha"] = next_head
    save("gitlab/MergeRequestHeader.after.push.json", moved)
    data = load("gitlab/MergeRequestActionContext.json")
    data["data"]["project"]["mergeRequest"]["diffHeadSha"] = head
    save("gitlab/MergeRequestActionContext.json", data)
PY
}

# ---- one scenario ---------------------------------------------------------------

prepare_scenario() { # flavour head-kind origin-url fork-url -- repos, fixtures, forge and project; no app yet
  mkdir -p "$RUN_DIR/work-$SCENARIO"
  build_repos "$1" "$2"
  make_next_head "$1"
  render_fixtures "$1" "$2" "$(owner_of "$1" "$2")" "$BASE" "$HEAD_SHA" "$NEXT_HEAD"
  if [ "${SECOND_CHANGE_REQUEST:-0}" = 1 ]; then add_second_change_request; fi
  start_forge "$1"
  local project=widgets
  [ "$1" = gitlab ] && project=app
  make_project "$RUN_DIR/work-$SCENARIO/$project" "$3" "$4"
}

open_scenario() { # host forge number -- the app, connected and on the change request
  launch_app "$1"
  connect_and_open "$1" "$2" "$3"
}

# add_second_change_request -- a second fork change request (#102, head branch fix)
# from the same owner: the fork's fix branch and the forge's pull ref for #102.
add_second_change_request() {
  FIX_SHA=$(git -C "$SRC" commit-tree "$(git -C "$SRC" rev-parse "$BASE^{tree}")" -p "$BASE" -m "fix the sidebar")
  git -C "$SRC" push -q "$FORK" "$FIX_SHA:refs/heads/fix"
  git -C "$SRC" push -q "$ORIGIN" "$FIX_SHA:refs/pull/102/head"
  python3 - "$FIXTURES_DIR" "$FIX_SHA" <<'PY'
import json, sys
root, sha = sys.argv[1:3]
for name in ("ChangeRequestHeader", "ChangeRequestByNumber"):
    data = json.load(open(f"{root}/github/{name}.json"))
    node = data["data"]["repository"]["pullRequest"]
    node.update(number=102, url="https://ghe.test/acme/widgets/pull/102", title="Fix the sidebar",
                headRefName="fix", headRefOid=sha)
    json.dump(data, open(f"{root}/github/{name}.n102.json", "w"))
PY
}

# flip_fork_to_pushable flavour -- the fork starts accepting pushes: every header
# fixture, the after-push overlays included (a push served before the flip),
# now grants the maintainer write (GitHub maintainerCanModify, GitLab the target's
# pushCode). Read per request, so the next checkout sees it.
flip_fork_to_pushable() {
  python3 - "$1" "$FIXTURES_DIR" <<'PY'
import json, os, sys
flavour, root = sys.argv[1:3]
dir_name, prefixes = ("github", ("ChangeRequestHeader", "ChangeRequestByNumber")) if flavour == "github" else ("gitlab", ("MergeRequestHeader", "MergeRequestByNumber"))
for entry in sorted(os.listdir(f"{root}/{dir_name}")):
    if not entry.startswith(prefixes) or not entry.endswith(".json"):
        continue
    path = f"{root}/{dir_name}/{entry}"
    data = json.load(open(path))
    if flavour == "github":
        data["data"]["repository"]["pullRequest"]["maintainerCanModify"] = True
    else:
        data["data"]["project"]["mergeRequest"]["targetProject"]["userPermissions"]["pushCode"] = True
    json.dump(data, open(path, "w"))
PY
}

label_of() { if [ "$1" = gitlab ]; then echo '!201'; else echo '#101'; fi; }

# The fork's branch, remote and folder name for flavour (the owner path differs).
fork_branch_of() { if [ "$1" = gitlab ]; then echo 'forks/alice/feat'; else echo 'alice/feat'; fi; }
fork_remote_of() { if [ "$1" = gitlab ]; then echo 'sirio-forks-alice-201'; else echo 'sirio-alice-101'; fi; }
fork_dir_of() { if [ "$1" = gitlab ]; then echo 'app-forks-alice-feat'; else echo 'widgets-alice-feat'; fi; }

checkout_and_wait() { # expected-state (done|failed)
  ctl surface change-request checkout
  wait_for checkout "$1" surface change-request read
  DETAIL=$(read_field checkout_detail surface change-request read)
  echo "detail: $DETAIL"
}

# set_forge_head flavour sha -- the forge's head ref and the head its header
# serves from now on: the header's `after.push` overlay is rewritten (the fixture
# is read per request) and /__push makes the forge serve it.
set_forge_head() {
  python3 - "$1" "$2" "$FIXTURES_DIR" <<'PY'
import json, sys
flavour, sha, root = sys.argv[1:4]
if flavour == "github":
    path = f"{root}/github/ChangeRequestHeader.after.push.json"
    data = json.load(open(path))
    data["data"]["repository"]["pullRequest"]["headRefOid"] = sha
else:
    path = f"{root}/gitlab/MergeRequestHeader.after.push.json"
    data = json.load(open(path))
    node = data["data"]["project"]["mergeRequest"]
    node["diffRefs"]["headSha"] = sha
    node["diffHeadSha"] = sha
json.dump(data, open(path, "w"))
PY
  git -C "$ORIGIN" update-ref "$(pull_ref_of "$1")" "$2"
  curl -s -o /dev/null -X POST "http://127.0.0.1:$PORT/__push"
}

# push_to_origin_feat flavour message -> the new tip of origin's feat, from a scratch
# clone; the forge's head moves to it too, as a push to the change request does.
push_to_origin_feat() {
  local scratch="$RUN_DIR/scratch-$SCENARIO"
  rm -rf "$scratch"
  git clone -q "$ORIGIN" "$scratch"
  git -C "$scratch" config user.email e2e@sirio.dev; git -C "$scratch" config user.name E2E
  git -C "$scratch" checkout -q -B feat origin/feat
  echo "$1" >>"$scratch/a.txt"
  git -C "$scratch" commit -q -am "$1"
  git -C "$scratch" push -q origin feat
  local tip
  tip=$(git -C "$scratch" rev-parse HEAD)
  set_forge_head "$1" "$tip"
  echo "$tip"
}

dump_git() { # -- the repository's worktrees, remotes and branches, for the artifact
  {
    echo "[$SCENARIO]"
    echo "-- worktrees"; git -C "$WT" worktree list --porcelain
    echo "-- remotes"; git -C "$WT" remote -v
    echo "-- branches"; git -C "$WT" for-each-ref --format='%(refname) %(objectname) %(upstream)' refs/heads
    for bare in "$ORIGIN" "$FORK"; do
      [ -d "$bare" ] || continue
      echo "-- refs of $(basename "$bare")"; git -C "$bare" for-each-ref --format='%(refname) %(objectname)'
    done
  } >"$OUT_DIR/$SCENARIO-git.txt"
}

scenario_same() { # flavour host forge number origin-url fork-url
  SCENARIO="$1-same"
  echo "=== $SCENARIO"
  local label NEW X2 X3 OTHER
  label=$(label_of "$1")
  prepare_scenario "$1" same "$5" "$6"
  open_scenario "$2" "$3" "$4"
  NEW="$(dirname "$WT")/$(basename "$WT")-feat"

  checkout_and_wait done
  assert_contains checkout_detail "tracking origin/feat" surface change-request read
  [ -d "$NEW" ] || fail "the worktree $NEW was not created"
  [ "$(git -C "$NEW" rev-parse --abbrev-ref '@{u}')" = origin/feat ] || fail "the new worktree does not track origin/feat"
  echo "OK: $(basename "$NEW") tracks origin/feat"
  [ "$(read_field path current-workspace)" = "$NEW" ] || fail "the selected worktree is not $NEW"
  echo "OK: the selected worktree is $NEW"
  assert_contains label "$label" surface change-request read
  assert_contains checkout done surface change-request read

  echo "a commit from the worktree pushes to origin"
  printf 'pushed from the worktree\n' >>"$NEW/a.txt"
  git -C "$NEW" commit -q -am "from the worktree"
  git -C "$NEW" push -q
  [ "$(git -C "$ORIGIN" rev-parse refs/heads/feat)" = "$(git -C "$NEW" rev-parse HEAD)" ] || fail "the push did not reach origin"
  echo "OK: origin's feat is the worktree's HEAD"
  ctl surface change-requests show >/dev/null
  wait_for linked "$label" surface change-requests read
  wait_for card "$label" surface change-requests read
  capture "checkout-$1-same-done"

  echo "reuse: a clean worktree fast-forwards to a commit pushed meanwhile"
  X2=$(push_to_origin_feat "$1" "pushed by someone else")
  checkout_and_wait done
  assert_contains checkout_detail "fast-forwarded" surface change-request read
  [ "$(git -C "$NEW" rev-parse HEAD)" = "$X2" ] || fail "the reused worktree is not at the pushed commit"
  echo "OK: the reused worktree is at $X2"

  echo "dirty: a worktree with uncommitted changes is left as it is"
  X3=$(push_to_origin_feat "$1" "pushed while dirty")
  echo "uncommitted" >>"$NEW/a.txt"
  checkout_and_wait done
  assert_contains checkout_detail "uncommitted changes" surface change-request read
  [ "$(git -C "$NEW" rev-parse HEAD)" = "$X2" ] || fail "a dirty worktree moved"
  git -C "$NEW" checkout -q -- a.txt
  echo "OK: the dirty worktree did not move"

  echo "another branch: a worktree switched elsewhere is left alone"
  git -C "$NEW" checkout -q -b other
  printf 'other\n' >"$NEW/other.txt"
  git -C "$NEW" add other.txt; git -C "$NEW" commit -q -m "work on other"
  OTHER=$(git -C "$NEW" rev-parse HEAD)
  echo "link: a linked worktree switched to another branch no longer shows the change request"
  ctl surface change-requests show >/dev/null
  wait_for linked "-" surface change-requests read
  checkout_and_wait done
  assert_contains checkout_detail "it is on another branch, so it was left as it is" surface change-request read
  [ "$(git -C "$NEW" rev-parse other)" = "$OTHER" ] || fail "the other branch moved"
  echo "OK: the other branch did not move"
  git -C "$NEW" checkout -q feat

  echo "detached: a worktree with a detached HEAD is left alone, and says so"
  git -C "$NEW" checkout -q --detach
  checkout_and_wait done
  assert_contains checkout_detail "it is on a detached HEAD, so it was left as it is" surface change-request read
  git -C "$NEW" checkout -q feat
  echo "OK: the detached worktree was left alone"

  echo "stale folder: a worktree whose folder was deleted outside git is forgotten, and created again"
  rm -rf "$NEW"
  checkout_and_wait done
  assert_contains checkout_detail "Created" surface change-request read
  [ -d "$NEW" ] || fail "the worktree was not created again"
  [ "$(git -C "$NEW" rev-parse HEAD)" = "$X3" ] || fail "the recreated worktree is not at origin's feat"
  echo "OK: the worktree was created again at origin's feat"

  dump_git
  quit_app; stop_forge
}

scenario_fork_push() { # flavour host forge number origin-url fork-url
  SCENARIO="$1-fork-push"
  echo "=== $SCENARIO"
  local label branch remote NEW
  label=$(label_of "$1"); branch=$(fork_branch_of "$1"); remote=$(fork_remote_of "$1")
  prepare_scenario "$1" fork-push "$5" "$6"
  open_scenario "$2" "$3" "$4"
  NEW="$(dirname "$WT")/$(fork_dir_of "$1")"

  checkout_and_wait done
  [ -d "$NEW" ] || fail "the worktree $NEW was not created"
  [ "$(git -C "$NEW" rev-parse --abbrev-ref HEAD)" = "$branch" ] || fail "the worktree is not on $branch"
  [ "$(git -C "$WT" config --get "remote.$remote.url")" = "$6" ] || fail "the remote $remote is not the fork"
  [ "$(git -C "$NEW" rev-parse --abbrev-ref '@{u}')" = "$remote/feat" ] || fail "the upstream is not $remote/feat"
  [ "$(git -C "$WT" config --get-all "remote.$remote.push" | wc -l)" -eq 1 ] || fail "$remote does not carry exactly one push mapping"
  echo "OK: $(basename "$NEW") is on $branch, pushing to $remote, with one push mapping"

  printf 'from the fork worktree\n' >>"$NEW/a.txt"
  git -C "$NEW" commit -q -am "from the fork worktree"
  git -C "$NEW" push -q
  [ "$(git -C "$FORK" rev-parse refs/heads/feat)" = "$(git -C "$NEW" rev-parse HEAD)" ] || fail "the push did not reach the fork's feat"
  git -C "$ORIGIN" show-ref --verify --quiet refs/heads/feat && fail "origin gained a feat branch"
  echo "OK: the push landed on the fork's feat, and origin has no feat"
  ctl surface change-requests show >/dev/null
  wait_for card "$label" surface change-requests read
  dump_git
  quit_app; stop_forge
}

scenario_fork_readonly() { # flavour host forge number origin-url fork-url
  SCENARIO="$1-fork-readonly"
  echo "=== $SCENARIO"
  local branch remote NEW
  branch=$(fork_branch_of "$1"); remote=$(fork_remote_of "$1")
  prepare_scenario "$1" fork-readonly "$5" "$6"
  open_scenario "$2" "$3" "$4"
  NEW="$(dirname "$WT")/$(fork_dir_of "$1")"

  checkout_and_wait done
  assert_contains checkout_detail "read-only: the fork does not accept pushes" surface change-request read
  [ -d "$NEW" ] || fail "the worktree $NEW was not created"
  if git -C "$NEW" rev-parse --abbrev-ref '@{u}' >/dev/null 2>&1; then fail "a read-only worktree has an upstream"; fi
  [ "$(git -C "$NEW" rev-parse HEAD)" = "$HEAD_SHA" ] || fail "the read-only worktree is not at the head"
  [ -z "$(git -C "$WT" config --get "remote.$remote.url" || true)" ] || fail "a read-only checkout added the remote $remote"
  echo "OK: $(basename "$NEW") is read-only at the head, with no upstream and no $remote"

  echo "the forge's head moves on: the read-only worktree follows it"
  git -C "$RUN_DIR/next-$SCENARIO" push -q "$ORIGIN" "$NEXT_HEAD:$(pull_ref_of "$1")"
  curl -s -o /dev/null -X POST "http://127.0.0.1:$PORT/__push"
  checkout_and_wait done
  assert_contains checkout_detail "read-only: the fork does not accept pushes" surface change-request read
  assert_contains checkout_detail "fast-forwarded to ${NEXT_HEAD:0:8}" surface change-request read
  [ "$(git -C "$NEW" rev-parse HEAD)" = "$NEXT_HEAD" ] || fail "the read-only worktree did not follow the head"
  echo "OK: the read-only worktree is at $NEXT_HEAD"

  echo "the fork accepts pushes now: the same worktree gets its remote, upstream and one push mapping"
  flip_fork_to_pushable "$1"
  checkout_and_wait done
  [ "$(git -C "$NEW" rev-parse --abbrev-ref '@{u}')" = "$remote/feat" ] || fail "the reused worktree does not track $remote/feat"
  [ "$(git -C "$WT" config --get "remote.$remote.url")" = "$6" ] || fail "the remote $remote is not the fork after the flip"
  [ "$(git -C "$WT" config --get-all "remote.$remote.push" | wc -l)" -eq 1 ] || fail "$remote does not carry exactly one push mapping after the flip"
  echo "OK: the reused worktree tracks $remote/feat with one push mapping"

  dump_git
  quit_app; stop_forge
}

scenario_gone() { # flavour host forge number origin-url fork-url -- a closed change request, its head branch gone
  SCENARIO="$1-gone"
  echo "=== $SCENARIO"
  prepare_scenario "$1" gone "$5" "$6"
  open_scenario "$2" "$3" "$4"
  local NEW="$(dirname "$WT")/$(basename "$WT")-pr-101"

  checkout_and_wait done
  assert_contains cr_state closed surface change-request read
  assert_contains checkout_detail "read-only: the source repository is gone" surface change-request read
  git -C "$WT" rev-parse --verify --quiet refs/heads/pr-101 >/dev/null || fail "the local branch pr-101 was not made"
  echo "OK: the local branch is pr-101"
  [ -d "$NEW" ] || fail "the worktree $NEW was not created"
  [ "$(git -C "$NEW" rev-parse HEAD)" = "$HEAD_SHA" ] || fail "the read-only worktree is not at the head"
  echo "OK: $(basename "$NEW") is read-only at the head"

  dump_git
  quit_app; stop_forge
}

scenario_two_forks() { # -- two fork change requests (#101 feat, #102 fix) from one owner, GitHub
  SCENARIO="github-two-forks"
  echo "=== $SCENARIO"
  local NEW1 NEW2 FIX_BEFORE
  SECOND_CHANGE_REQUEST=1
  prepare_scenario github fork-push https://ghe.test/acme/widgets.git https://ghe.test/alice/widgets.git
  SECOND_CHANGE_REQUEST=0
  open_scenario ghe.test github 101
  NEW1="$(dirname "$WT")/widgets-alice-feat"
  NEW2="$(dirname "$WT")/widgets-alice-fix"

  checkout_and_wait done
  [ -d "$NEW1" ] || fail "the worktree $NEW1 was not created"
  # The checkout selected the new worktree, and the view connects to that one.
  ctl surface change-requests show >/dev/null
  wait_for state ready surface change-requests read
  ctl surface change-request open 102
  wait_for state loaded surface change-request read
  assert_contains label "#102" surface change-request read
  checkout_and_wait done
  [ -d "$NEW2" ] || fail "the worktree $NEW2 was not created"
  [ "$(git -C "$NEW2" rev-parse --abbrev-ref HEAD)" = alice/fix ] || fail "the second worktree is not on alice/fix"
  [ "$(git -C "$WT" config --get-all remote.sirio-alice-101.push)" = "refs/heads/alice/feat:refs/heads/feat" ] || fail "remote sirio-alice-101 has the wrong push mapping"
  [ "$(git -C "$WT" config --get-all remote.sirio-alice-102.push)" = "refs/heads/alice/fix:refs/heads/fix" ] || fail "remote sirio-alice-102 has the wrong push mapping"
  [ "$(git -C "$WT" config --get-all remote.sirio-alice-101.push | wc -l)" -eq 1 ] || fail "sirio-alice-101 carries more than one push mapping"
  [ "$(git -C "$WT" config --get-all remote.sirio-alice-102.push | wc -l)" -eq 1 ] || fail "sirio-alice-102 carries more than one push mapping"
  echo "OK: each fork remote carries exactly one push mapping, for its own branch"

  echo "a plain push from the feat worktree moves only the fork's feat"
  FIX_BEFORE=$(git -C "$FORK" rev-parse refs/heads/fix)
  printf 'from the feat worktree\n' >>"$NEW1/a.txt"
  git -C "$NEW1" commit -q -am "from the feat worktree"
  git -C "$NEW1" push -q
  [ "$(git -C "$FORK" rev-parse refs/heads/feat)" = "$(git -C "$NEW1" rev-parse HEAD)" ] || fail "the push did not reach the fork's feat"
  [ "$(git -C "$FORK" rev-parse refs/heads/fix)" = "$FIX_BEFORE" ] || fail "a push from the feat worktree moved the fork's fix"
  echo "OK: the fork's fix is unchanged at $FIX_BEFORE"

  echo "a plain push from the fix worktree moves only the fork's fix"
  printf 'from the fix worktree\n' >>"$NEW2/a.txt"
  git -C "$NEW2" commit -q -am "from the fix worktree"
  git -C "$NEW2" push -q
  [ "$(git -C "$FORK" rev-parse refs/heads/fix)" = "$(git -C "$NEW2" rev-parse HEAD)" ] || fail "the push did not reach the fork's fix"
  [ "$(git -C "$FORK" rev-parse refs/heads/feat)" = "$(git -C "$NEW1" rev-parse HEAD)" ] || fail "a push from the fix worktree moved the fork's feat"
  echo "OK: the fork's feat is unchanged by the fix push"

  dump_git
  quit_app; stop_forge
}

scenario_own_fork() { # -- the viewer's own fork is origin, the base repository is upstream
  SCENARIO="github-own-fork"
  echo "=== $SCENARIO"
  prepare_scenario github fork-push https://ghe.test/acme/widgets.git https://ghe.test/alice/widgets.git
  git -C "$WT" remote rename origin upstream
  git -C "$WT" remote add origin https://ghe.test/alice/widgets.git
  open_scenario ghe.test github 101
  local NEW="$(dirname "$WT")/widgets-feat"

  checkout_and_wait done
  [ -d "$NEW" ] || fail "the worktree $NEW was not created"
  [ "$(git -C "$NEW" rev-parse --abbrev-ref HEAD)" = feat ] || fail "the worktree is not on feat"
  [ "$(git -C "$NEW" rev-parse --abbrev-ref '@{u}')" = origin/feat ] || fail "the branch does not track origin/feat"
  [ -z "$(git -C "$WT" remote | grep '^sirio-' || true)" ] || fail "a sirio- remote was made for the viewer's own fork"
  echo "OK: feat tracks origin/feat, and no sirio- remote was made"

  printf 'from the own fork\n' >>"$NEW/a.txt"
  git -C "$NEW" commit -q -am "from the own fork"
  git -C "$NEW" push -q
  [ "$(git -C "$FORK" rev-parse refs/heads/feat)" = "$(git -C "$NEW" rev-parse HEAD)" ] || fail "the push did not reach the fork's feat"
  echo "OK: a push from the worktree lands on the own fork's feat"

  dump_git
  quit_app; stop_forge
}

scenario_refusals() { # flavour host forge number origin-url fork-url
  SCENARIO="$1-refusals"
  echo "=== $SCENARIO"
  prepare_scenario "$1" same "$5" "$6"
  # A feat that is not an ancestor of the head: the checkout must refuse it
  # before it creates anything.
  git -C "$WT" fetch -q origin main
  git -C "$WT" branch feat FETCH_HEAD
  open_scenario "$2" "$3" "$4"
  local NEW="$(dirname "$WT")/$(basename "$WT")-feat"

  echo "collision: a local feat at other commits is refused, and nothing is created"
  checkout_and_wait failed
  assert_contains checkout_detail "has commits the change request does not" surface change-request read
  [ ! -e "$NEW" ] || fail "the refused checkout created $NEW"
  capture "checkout-$1-refused"
  git -C "$WT" branch -D feat >/dev/null
  echo "OK: the collision was refused and the branch deleted"

  echo "no remote: a project with no remote for the forge is refused"
  git -C "$WT" remote set-url origin https://ghe.test/other/repo.git
  checkout_and_wait failed
  assert_contains checkout_detail "no remote of this project points at acme/widgets" surface change-request read
  [ ! -e "$(dirname "$WT")/$(basename "$WT")-feat" ] || fail "the refused checkout created a worktree"
  git -C "$WT" remote set-url origin "$5"
  echo "OK: the project with no matching remote was refused, and its URL restored"

  echo "twice: a second checkout while one runs is refused"
  curl -s -o /dev/null -X POST "http://127.0.0.1:$PORT/__slowgraphql?seconds=3"
  ctl surface change-request checkout
  local second
  second=$(reply surface change-request checkout 2>&1 || true)
  case "$second" in
    *"a checkout is already running"*) echo "OK: the second checkout was refused while one runs" ;;
    *) fail "the second checkout was not refused as running: $second" ;;
  esac
  curl -s -o /dev/null -X POST "http://127.0.0.1:$PORT/__reset"
  wait_for checkout done surface change-request read
  [ -d "$NEW" ] || fail "the checkout that ran did not create $NEW"
  echo "OK: the checkout that ran finished"

  dump_git
  quit_app; stop_forge
}

start_forge() { # flavour
  PORT=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')
  python3 "$ROOT/Scripts/Tests/fake_forge.py" --flavor "$1" --port "$PORT" --fixtures "$FIXTURES_DIR" --log "$RUN_DIR/$SCENARIO-forge-requests.log" &
  FORGE_PID=$!
  local ready=0
  for _ in $(seq 1 50); do
    curl -s -o /dev/null --max-time 1 "http://127.0.0.1:$PORT/api/v3/meta" && { ready=1; break; }
    sleep 0.2
  done
  [ "$ready" -eq 1 ] || fail "the fake forge never answered on port $PORT"
}

launch_app() { # host [log-name]
  local log="$RUN_DIR/${2:-$SCENARIO-app}.log"
  export SIRIO_SOCKET="$RUN_DIR/$SCENARIO.sock"
  export SIRIO_DB="$RUN_DIR/$SCENARIO.sqlite"
  export SIRIO_CREDENTIALS="$RUN_DIR/$SCENARIO-credentials.json"
  # The data root of the isolated host: without it a debug build adopts or starts a host
  # in the real data root (CLAUDE.md, SP1 limitation).
  export SIRIO_HOST_HOME="$RUN_DIR/$SCENARIO-host"
  export SIRIO_FORGE_FETCH_TIMEOUT_MS=20000
  export SIRIO_FORGE_TEST_ENDPOINTS="$1=http://127.0.0.1:$PORT"
  export GH_CONFIG_DIR="$RUN_DIR/gh-$SCENARIO" GLAB_CONFIG_DIR="$RUN_DIR/glab-$SCENARIO"
  mkdir -p "$GH_CONFIG_DIR" "$GLAB_CONFIG_DIR"
  chmod 700 "$GLAB_CONFIG_DIR"
  unset GH_TOKEN GITHUB_TOKEN GH_ENTERPRISE_TOKEN GITHUB_ENTERPRISE_TOKEN GITLAB_TOKEN \
    GL_TOKEN SIRIO_FORGE_LIVE_GITLAB_TOKEN GH_HOST GITLAB_HOST HTTP_PROXY HTTPS_PROXY \
    http_proxy https_proxy ALL_PROXY all_proxy || true
  rm -f "$SIRIO_SOCKET"
  if [ "$STATE_ONLY" -eq 1 ]; then
    (cd "$WT" && exec env -u DISPLAY -u WAYLAND_DISPLAY "$BIN" >"$log" 2>&1) &
  else
    (cd "$WT" && exec env -u WAYLAND_DISPLAY DISPLAY="$DISPLAY_TARGET" GPUI_X11_SCALE_FACTOR=1 "$BIN" >"$log" 2>&1) &
  fi
  APP_PID=$!
  for _ in $(seq 1 75); do
    [ -S "$SIRIO_SOCKET" ] && break
    sleep 0.2
  done
  [ -S "$SIRIO_SOCKET" ] || fail "sirio never opened its control socket"
}

quit_app() { # the graceful quit a user makes, which flushes the session
  ctl quit
  for _ in $(seq 1 100); do
    kill -0 "$APP_PID" 2>/dev/null || break
    sleep 0.2
  done
  ! kill -0 "$APP_PID" 2>/dev/null || fail "sirio did not exit after quit"
  wait "$APP_PID" 2>/dev/null || true
  APP_PID=""
  echo "OK: sirio quit"
}

connect_and_open() { # host forge number
  ctl project add "$WT"
  ctl select-workspace --workspace "$WT"
  ctl surface change-requests show
  # Let the initial host probe settle before saving a token, as the sign-in UI does.
  if [ "$2" = github ]; then
    wait_for state not-connected surface change-requests read
  else
    wait_for state unknown-forge surface change-requests read
  fi
  wait_for host "$1" surface change-requests read
  local account
  account=$(read_field account surface change-requests token --host "$1" --forge "$2" --token good) || fail "the token was not accepted"
  [ "$account" = "fake-user" ] || fail "the token signed in as '$account'"
  wait_for state ready surface change-requests read
  ctl surface change-request open "$3"
  wait_for state loaded surface change-request read
}


scenario_same github ghe.test github 101 https://ghe.test/acme/widgets.git https://ghe.test/alice/widgets.git
scenario_same gitlab gitlab.test gitlab 201 https://gitlab.test/team/app.git https://gitlab.test/forks/alice/app.git
scenario_fork_push github ghe.test github 101 https://ghe.test/acme/widgets.git https://ghe.test/alice/widgets.git
scenario_fork_push gitlab gitlab.test gitlab 201 https://gitlab.test/team/app.git https://gitlab.test/forks/alice/app.git
scenario_fork_readonly github ghe.test github 101 https://ghe.test/acme/widgets.git https://ghe.test/alice/widgets.git
scenario_fork_readonly gitlab gitlab.test gitlab 201 https://gitlab.test/team/app.git https://gitlab.test/forks/alice/app.git
scenario_gone github ghe.test github 101 https://ghe.test/acme/widgets.git https://ghe.test/alice/widgets.git
scenario_refusals github ghe.test github 101 https://ghe.test/acme/widgets.git https://ghe.test/alice/widgets.git
scenario_two_forks
scenario_own_fork

echo "artifact: $OUT_DIR"
echo "HANDOFF E2E OK"
