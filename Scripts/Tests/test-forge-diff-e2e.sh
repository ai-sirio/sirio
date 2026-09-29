#!/bin/bash
set -euo pipefail

# End-to-end test of a change request's diff and files inside Sirio (B1, spec
# 2026-09-28-change-request-diff-design.md §10.1): a real, isolated Sirio
# against the loopback fake forge (Scripts/Tests/fake_forge.py) and a real
# bare git repository standing in for the forge's git side, driven over the
# control socket. GitHub: the success path (a commit click fetches first, then
# Files; snapshot tabs alone keep the refs), a fetch answered 401 and a fetch
# that hangs. GitLab: the success path with Files opened first, so the
# Files-triggered fetch is the one exercised. The forge's repository carries a
# tag and a `main` that moved on after the branch left it, so a tag that came
# along or a base fetched by the wrong ref would show. Each success path also
# quits Sirio gracefully halfway and relaunches it on the same database: the
# snapshot tabs come back with the same text, and the startup sweep keeps
# their refs.
#
# The artifact: --out-dir DIR (default artifacts/forge-diff-e2e-<stamp>-<pid>)
# keeps transcript.log, one app log per launch and one fake-forge request log
# per scenario, the ref dumps (refs-*.log) and -- unless --state-only --
# PID-matched window captures in frames/. Rerunning the script reproduces it.
#
# Usage: Scripts/Tests/test-forge-diff-e2e.sh [--state-only] [--out-dir DIR] [--display :N]

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
[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/forge-diff-e2e-$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$OUT_DIR/frames"
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

RUN_DIR=$(mktemp -d /tmp/sirio-diff-e2e-XXXXXX)
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
escape_text() { python3 -c 'import sys; sys.stdout.write(sys.stdin.read().replace("\\", "\\\\").replace("\n", "\\n"))'; }

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

cat > "$RUN_DIR/tabs.py" <<'PY'
import json, sys
row = json.load(sys.stdin)[0]
mode = sys.argv[1]
tabs = [row[f"tab.{position}"].split("|", 2) for position in range(1, int(row["count"]) + 1)]
if mode == "index":        # the first tab of a kind: tabs.py index <kind>
    for position, (kind, _snapshot, _title) in enumerate(tabs, start=1):
        if kind == sys.argv[2]:
            print(position)
            break
elif mode == "active-is":  # tabs.py active-is <kind> <snapshot yes|no>
    kind, snapshot, _title = tabs[int(row["active"]) - 1]
    print("yes" if (kind, snapshot) == (sys.argv[2], sys.argv[3]) else "no")
elif mode == "titled":     # the first tab of a kind with this title: tabs.py titled <kind> <snapshot> <title>
    for position, (kind, snapshot, title) in enumerate(tabs, start=1):
        if (kind, snapshot, title) == (sys.argv[2], sys.argv[3], sys.argv[4]):
            print(position)
            break
elif mode == "titled-any": # the first tab of a kind and snapshot flag: tabs.py titled-any <kind> <snapshot>
    for position, (kind, snapshot, _title) in enumerate(tabs, start=1):
        if (kind, snapshot) == (sys.argv[2], sys.argv[3]):
            print(position)
            break
elif mode == "count":      # how many tabs of a kind: tabs.py count <kind> <snapshot yes|no>
    print(sum(1 for kind, snapshot, _title in tabs if (kind, snapshot) == (sys.argv[2], sys.argv[3])))
elif mode == "closable":   # the first change request, diff or snapshot tab
    for position, (kind, snapshot, _title) in enumerate(tabs, start=1):
        if kind in ("change_request", "diff") or snapshot == "yes":
            print(position)
            break
PY
select_kind() { # kind
  local index
  index=$(reply surface tabs read | python3 "$RUN_DIR/tabs.py" index "$1")
  [ -n "$index" ] || fail "no $1 tab is open"
  "$CTL" surface tabs select "$index" >/dev/null
}
wait_active_kind() { # kind snapshot(yes|no)
  for _ in $(seq 1 100); do
    [ "$(reply surface tabs read | python3 "$RUN_DIR/tabs.py" active-is "$1" "${2:-no}" 2>/dev/null || echo no)" = yes ] &&
      { echo "OK: active tab is $1 (snapshot=${2:-no})"; return 0; }
    sleep 0.3
  done
  reply surface tabs read || true
  fail "the active tab never became $1 (snapshot=${2:-no})"
}
select_titled() { # kind snapshot(yes|no) title
  local index
  index=$(reply surface tabs read | python3 "$RUN_DIR/tabs.py" titled "$1" "$2" "$3")
  [ -n "$index" ] || { reply surface tabs read || true; fail "no $1 tab (snapshot=$2) is titled '$3'"; }
  "$CTL" surface tabs select "$index" >/dev/null
  wait_active_kind "$1" "$2"
}
wait_tab_count() { # kind snapshot(yes|no) count
  for _ in $(seq 1 100); do
    [ "$(reply surface tabs read 2>/dev/null | python3 "$RUN_DIR/tabs.py" count "$1" "$2" 2>/dev/null || echo 0)" = "$3" ] &&
      { echo "OK: $3 $1 tab(s) (snapshot=$2)"; return 0; }
    sleep 0.3
  done
  reply surface tabs read || true
  fail "never $3 $1 tab(s) (snapshot=$2)"
}
tab_count() { read_field count surface tabs read || fail "tab_count: no reply"; }
sirio_refs() { git -C "$WT" for-each-ref --format='%(refname)' refs/sirio/change-requests/; }
assert_refs() { # number -- exactly the head and base refs of that change request, by name
  local want got
  want=$(printf 'refs/sirio/change-requests/origin/%s/base\nrefs/sirio/change-requests/origin/%s/head' "$1" "$1")
  got=$(sirio_refs | sort)
  [ "$got" = "$want" ] || { echo "refs now:"; sirio_refs; fail "expected exactly the head and base refs of $1"; }
  echo "OK: refs are origin/$1/{base,head}"
}
close_kind() { # kind snapshot(yes|no) -- closes the first such tab
  local index
  index=$(reply surface tabs read | python3 "$RUN_DIR/tabs.py" titled-any "$1" "$2") || fail "close_kind: no reply"
  [ -n "$index" ] || { reply surface tabs read || true; fail "no $1 tab (snapshot=$2) to close"; }
  ctl surface tabs close "$index" >/dev/null
}
dump_refs() { { echo "[$SCENARIO $1]"; sirio_refs; } >> "$RUN_DIR/refs-$SCENARIO.log"; }

# ---- the forge's git side and its fixtures -----------------------------------

build_forge_git() { # flavour -> BASE C1 HEAD_SHA BARE PULL_REF
  local root="$RUN_DIR/git-$1" src
  src="$root/src"
  BARE="$root/forge.git"
  if [ "$1" = github ]; then PULL_REF=refs/pull/101/head; else PULL_REF=refs/merge-requests/201/head; fi
  rm -rf "$root"   # every scenario gets a forge of its own
  mkdir -p "$src"
  export GIT_AUTHOR_NAME=Forge GIT_AUTHOR_EMAIL=forge@example.invalid GIT_COMMITTER_NAME=Forge GIT_COMMITTER_EMAIL=forge@example.invalid
  git -C "$src" init -q -b main
  python3 - "$src" <<'PY'
import os, sys
root = sys.argv[1]
def write(rel, text):
    path = os.path.join(root, rel)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    open(path, "w").write(text)
write("src/login.rs", "".join(f"line {n}\n" for n in range(1, 61)))
write("docs/old notes.md", "alpha\nbeta\ngamma\ndelta\n")
write("gone.txt", "bye\nbye\n")
PY
  git -C "$src" add -A
  git -C "$src" -c commit.gpgSign=false commit -q -m base
  # A tag the fetch must not bring along (`git tag -l` in the worktree is
  # meaningful only because one exists here).
  git -C "$src" tag v1
  git -C "$src" checkout -q -b feat
  python3 - "$src" <<'PY'
import os, sys
root = sys.argv[1]
lines = [f"line {n}\n" for n in range(1, 61)]
lines[41] = "edited 42\n"
open(os.path.join(root, "src/login.rs"), "w").write("".join(lines))
open(os.path.join(root, "src/redirect.rs"), "w").write("fn redirect() {}\n")
PY
  git -C "$src" add -A
  git -C "$src" -c commit.gpgSign=false commit -q -m "fix the redirect"
  C1=$(git -C "$src" rev-parse HEAD)
  git -C "$src" mv "docs/old notes.md" "docs/new notes.md"
  printf 'alpha\nBETA\ngamma\ndelta\n' > "$src/docs/new notes.md"
  git -C "$src" rm -q gone.txt
  git -C "$src" add -A
  git -C "$src" -c commit.gpgSign=false commit -q -m "tidy the docs"
  HEAD_SHA=$(git -C "$src" rev-parse HEAD)
  # `main` moves on after the branch left it, as a real base branch does:
  # BASE is main's new tip, not an ancestor of HEAD, so only the base
  # refspec can bring it in, and only `base...head` gives the change
  # request's diff (`main-only.txt` must never appear in it).
  git -C "$src" checkout -q main
  printf 'later\n' > "$src/main-only.txt"
  git -C "$src" add -A
  git -C "$src" -c commit.gpgSign=false commit -q -m "main moves on"
  BASE=$(git -C "$src" rev-parse HEAD)
  git clone -q --bare "$src" "$BARE"
  git -C "$BARE" update-ref "$PULL_REF" "$HEAD_SHA"
  git -C "$BARE" update-ref -d refs/heads/feat
}

render_fixtures() { # flavour -> FIXTURES_DIR
  FIXTURES_DIR="$RUN_DIR/fixtures-$1"
  rm -rf "$FIXTURES_DIR"
  cp -r "$ROOT/Scripts/Tests/forge-fixtures" "$FIXTURES_DIR"
  python3 - "$1" "$FIXTURES_DIR" "$BASE" "$C1" "$HEAD_SHA" <<'PY'
import json, sys
flavour, root, base, c1, head = sys.argv[1:6]
def load(rel):
    return json.load(open(f"{root}/{rel}"))
def save(rel, data):
    json.dump(data, open(f"{root}/{rel}", "w"))
if flavour == "github":
    data = load("github/ChangeRequestHeader.json")
    node = data["data"]["repository"]["pullRequest"]
    node["baseRefOid"], node["headRefOid"] = base, head
    for item in node["timelineItems"]["nodes"]:
        if item.get("__typename") == "PullRequestReview":
            for comment in item["comments"]["nodes"]:
                comment["path"], comment["line"] = "src/login.rs", 42
    save("github/ChangeRequestHeader.json", data)
    data = load("github/ChangeRequestCommits.json")
    for entry, sha in zip(data["data"]["repository"]["pullRequest"]["commits"]["nodes"], (c1, head)):
        entry["commit"]["oid"] = sha
        entry["commit"]["abbreviatedOid"] = sha[:7]
        entry["commit"]["url"] = f"https://ghe.test/acme/widgets/commit/{sha}"
    save("github/ChangeRequestCommits.json", data)
else:
    data = load("gitlab/MergeRequestHeader.json")
    node = data["data"]["project"]["mergeRequest"]
    node["diffRefs"] = {"baseSha": base, "headSha": head, "startSha": base}
    for note in node["notes"]["nodes"]:
        if note.get("position"):
            note["position"]["filePath"], note["position"]["newLine"] = "src/login.rs", 42
    save("gitlab/MergeRequestHeader.json", data)
    data = load("gitlab/MergeRequestCommits.json")
    for entry, sha in zip(data["data"]["project"]["mergeRequest"]["commits"]["nodes"], (c1, head)):
        entry["sha"], entry["shortId"] = sha, sha[:7]
        entry["webUrl"] = f"https://gitlab.test/team/app/-/commit/{sha}"
    save("gitlab/MergeRequestCommits.json", data)
PY
}

# ---- one scenario ---------------------------------------------------------------

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

make_worktree() { # dir remote-url fetch-url
  mkdir -p "$1"
  git -C "$1" init -q -b main
  git -C "$1" config user.email t@example.com
  git -C "$1" config user.name Tester
  git -C "$1" config credential.helper ""
  git -C "$1" -c commit.gpgSign=false commit -q --allow-empty -m first
  git -C "$1" remote add origin "$2"
  git -C "$1" config "url.$3.insteadOf" "$2"
}

launch_app() { # host [log-name]
  local log="$RUN_DIR/${2:-$SCENARIO-app}.log"
  export SIRIO_SOCKET="$RUN_DIR/$SCENARIO.sock"
  export SIRIO_DB="$RUN_DIR/$SCENARIO.sqlite"
  export SIRIO_CREDENTIALS="$RUN_DIR/$SCENARIO-credentials.json"
  export SIRIO_FORGE_TEST_ENDPOINTS="$1=http://127.0.0.1:$PORT"
  export GH_CONFIG_DIR="$RUN_DIR/gh-$SCENARIO" GLAB_CONFIG_DIR="$RUN_DIR/glab-$SCENARIO"
  mkdir -p "$GH_CONFIG_DIR" "$GLAB_CONFIG_DIR"
  chmod 700 "$GLAB_CONFIG_DIR"
  unset GH_TOKEN GITHUB_TOKEN GITLAB_TOKEN HTTP_PROXY HTTPS_PROXY http_proxy https_proxy || true
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
  local account
  account=$(read_field account surface change-requests token --host "$1" --forge "$2" --token good) || fail "the token was not accepted"
  [ "$account" = "fake-user" ] || fail "the token signed in as '$account'"
  wait_for state ready surface change-requests read
  ctl surface change-request open "$3"
  wait_for state loaded surface change-request read
}

numstat_summary() { # the script's own reading of the diff, from the forge's repository: path +a -d, sorted, joined by |
  python3 - "$BARE" "$BASE" "$HEAD_SHA" <<'PY'
import subprocess, sys
repo, base, head = sys.argv[1:4]
raw = subprocess.check_output(["git", "-C", repo, "-c", "core.quotePath=false", "diff", "--numstat", "-z", "-M", f"{base}...{head}"])
parts = raw.split(b"\0")
rows, i = [], 0
while i < len(parts):
    record = parts[i].decode()
    if not record:
        i += 1
        continue
    adds, dels, path = record.split("\t", 2)
    if path == "":
        path = parts[i + 2].decode()
        i += 3
    else:
        i += 1
    rows.append(f"{path} +{adds} -{dels}")
print("|".join(sorted(rows)))
PY
}

assert_fetched() { # number -- what one fetch of a change request leaves, and leaves alone
  assert_refs "$1"
  [ ! -e "$WT/.git/FETCH_HEAD" ] || fail "FETCH_HEAD was written"
  [ -z "$(git -C "$WT" tag -l)" ] || fail "a tag was fetched (the forge has v1)"
  [ -z "$(git -C "$WT" branch --list --remotes)" ] || fail "a remote-tracking branch was written"
  git -C "$WT" cat-file -e "$HEAD_SHA^{commit}" || fail "the head commit is not local"
  git -C "$WT" cat-file -e "$BASE^{commit}" || fail "the base commit is not local"
  echo "OK: one fetch, and no trace outside refs/sirio/"
}

close_change_request_tabs() {
  for _ in $(seq 1 15); do
    local index
    index=$(reply surface tabs read | python3 "$RUN_DIR/tabs.py" closable)
    [ -n "$index" ] || return 0
    "$CTL" surface tabs close "$index" >/dev/null
    sleep 0.5
  done
  fail "the change request tabs would not close"
}

scenario_success() { # flavour host forge number label remote-url order(commit-first|files-first)
  SCENARIO="$1-success"
  local order=$7
  echo "=== $SCENARIO"
  build_forge_git "$1"
  render_fixtures "$1"
  start_forge "$1"
  WT="$RUN_DIR/worktree-$SCENARIO"
  make_worktree "$WT" "$6" "$BARE"
  # A ref left behind by an earlier run: the startup sweep must remove it.
  git -C "$WT" update-ref refs/sirio/change-requests/origin/999/head "$(git -C "$WT" rev-parse HEAD)"
  launch_app "$2"
  for _ in $(seq 1 50); do
    [ -z "$(sirio_refs)" ] && break
    sleep 0.2
  done
  [ -z "$(sirio_refs)" ] || fail "the orphan ref survived startup"
  echo "OK: the orphan ref was swept at startup"
  dump_refs "after startup"

  connect_and_open "$2" "$3" "$4"
  local expected
  expected=$(numstat_summary) || fail "numstat_summary failed"
  [ -n "$expected" ] || fail "the script's own numstat is empty"
  case "$expected" in *main-only.txt*) fail "the script's own numstat is not the change request's diff: $expected" ;; esac

  if [ "$order" = commit-first ]; then
    echo "step 1: a commit that is not local opens in a Changes tab, after one fetch"
    ctl surface change-request tab commits
    wait_for state loaded surface change-request read
    ctl surface change-request open-commit "$C1"
    wait_active_kind diff no
    assert_fetched "$4"
    dump_refs "after the commit fetch"
    capture commit

    echo "step 2: Files shows the diff the script computes itself, with nothing more to fetch"
    select_kind change_request
    ctl surface change-request tab files
    wait_for files_mode diff surface change-request read
  else
    echo "step 1: Files, before anything else, makes the revisions local and shows the diff"
    [ -z "$(sirio_refs)" ] || { sirio_refs; fail "refs before Files was ever shown"; }
    ctl surface change-request tab files
    wait_for files_mode diff surface change-request read
    assert_fetched "$4"
    dump_refs "after the Files fetch"

    echo "step 2: a commit click opens a Changes tab without a second fetch"
    local refs_before
    refs_before=$(sirio_refs | sort)
    ctl surface change-request tab commits
    wait_for state loaded surface change-request read
    ctl surface change-request open-commit "$C1"
    wait_active_kind diff no
    [ "$(sirio_refs | sort)" = "$refs_before" ] || { sirio_refs; fail "the commit click changed the refs: it fetched again"; }
    [ ! -e "$WT/.git/FETCH_HEAD" ] || fail "FETCH_HEAD was written by the commit click"
    echo "OK: the commit click fetched nothing"
    capture commit
    select_kind change_request
  fi
  wait_for diff_summary "$expected" surface change-request read
  wait_for head "${HEAD_SHA:0:7}" surface change-request read
  assert_refs "$4"
  capture files-diff

  echo "step 3: Open in editor, with the worktree elsewhere, opens read-only snapshots"
  ctl surface change-request open-file src/login.rs 42
  wait_active_kind file yes
  wait_for state loaded surface file read
  wait_for read_only true surface file read
  assert_contains origin "$5 at ${HEAD_SHA:0:7}" surface file read
  local login_text gone_text
  login_text=$(read_field content surface file read) || fail "no snapshot content"
  [ "$login_text" = "$(git -C "$BARE" show "$HEAD_SHA:src/login.rs" | escape_text)" ] ||
    fail "the snapshot's text is not the file at the head"
  echo "OK: the snapshot is the file at the head, byte for byte"
  capture snapshot
  select_kind change_request
  ctl surface change-request open-file gone.txt
  wait_active_kind file yes
  wait_for state loaded surface file read
  assert_contains origin "$5 at ${BASE:0:7}" surface file read
  gone_text=$(read_field content surface file read) || fail "no snapshot content"
  [ "$gone_text" = "$(git -C "$BARE" show "$BASE:gone.txt" | escape_text)" ] ||
    fail "a deleted file's snapshot is not the file at the base"
  echo "OK: a deleted file opens at the base"
  local before
  before=$(tab_count)
  select_kind change_request
  wait_active_kind change_request no
  ctl surface change-request open-file src/login.rs 42
  # The open finishes by selecting the tab it found or made: once the active
  # tab is a snapshot again, the count it left is the count to compare.
  wait_active_kind file yes
  [ "$(tab_count)" = "$before" ] || fail "opening the same snapshot again opened another tab"
  echo "OK: the same snapshot is one tab"

  echo "step 3b: a graceful quit and a relaunch restore the snapshots and keep their refs"
  quit_app
  # A second orphan: once the relaunch's sweep has removed it, the sweep has
  # run, and whatever it left is what it chose to keep.
  git -C "$WT" update-ref refs/sirio/change-requests/origin/998/head "$(git -C "$WT" rev-parse HEAD)"
  dump_refs "before the relaunch"
  launch_app "$2" "$SCENARIO-relaunch-app"
  wait_tab_count change_request no 1
  wait_tab_count file yes 2
  local restored_name restored_origin restored_text
  for restored_name in login gone; do
    if [ "$restored_name" = login ]; then
      select_titled file yes "login.rs @ $5"
      restored_origin="$5 at ${HEAD_SHA:0:7}"
      restored_text=$login_text
    else
      select_titled file yes "gone.txt @ $5"
      restored_origin="$5 at ${BASE:0:7}"
      restored_text=$gone_text
    fi
    wait_for state loaded surface file read
    wait_for read_only true surface file read
    wait_for origin "$restored_origin" surface file read
    [ "$(read_field content surface file read)" = "$restored_text" ] ||
      fail "the restored $restored_name snapshot's text is not what it showed before the restart"
    echo "OK: the restored $restored_name snapshot is the same text, byte for byte"
  done
  for _ in $(seq 1 50); do
    sirio_refs | grep -q /998/ || break
    sleep 0.2
  done
  ! sirio_refs | grep -q /998/ || fail "the relaunch never swept the orphan ref"
  [ "$(sirio_refs | wc -l)" -eq 2 ] || { sirio_refs; fail "the startup sweep removed the refs of restored tabs"; }
  echo "OK: the relaunch swept the orphan and kept the restored tabs' refs"
  dump_refs "after the relaunch"

  echo "step 4: a line comment's link shows its file and line in Files"
  select_kind change_request
  ctl surface change-request tab conversation
  ctl surface change-request reveal src/login.rs 42
  wait_for inner files surface change-request read
  wait_for files_focus src/login.rs surface change-request read

  echo "step 5: with the worktree at the head, Open in editor opens the local file"
  git -C "$WT" fetch -q "$BARE" "$PULL_REF:refs/heads/pr"
  git -C "$WT" checkout -q pr
  select_kind change_request
  ctl surface change-request open-file src/login.rs 42
  wait_active_kind file no
  wait_for read_only false surface file read
  wait_for path "$WT/src/login.rs" surface file read
  capture local-file

  if [ "$order" = commit-first ]; then
    echo "step 6a: the snapshot tabs alone keep the refs (spec §6.4)"
    # An orphan planted now: once the sweep that the close runs has removed
    # it, the sweep has run, and whatever it left is what it chose to keep.
    git -C "$WT" update-ref refs/sirio/change-requests/origin/997/head "$(git -C "$WT" rev-parse HEAD)"
    close_kind change_request no
    close_kind diff no
    for _ in $(seq 1 50); do
      sirio_refs | grep -q /997/ || break
      sleep 0.2
    done
    ! sirio_refs | grep -q /997/ || fail "closing the change request tab never swept"
    wait_tab_count change_request no 0
    assert_refs "$4"
    echo "OK: the snapshots alone kept the refs"
    dump_refs "after closing the change request tab"
  fi

  echo "step 6: closing the change request's tabs removes the refs"
  close_change_request_tabs
  for _ in $(seq 1 50); do
    [ -z "$(sirio_refs)" ] && break
    sleep 0.2
  done
  [ -z "$(sirio_refs)" ] || { sirio_refs; fail "refs survived their tabs"; }
  echo "OK: no revision ref is left"
  dump_refs "after closing"
  git -C "$WT" cat-file -e "$HEAD_SHA^{commit}" || fail "deleting the refs deleted the objects"

  stop_app
  stop_forge
}

scenario_failure() { # name fetch-url expected-text bound-seconds
  SCENARIO="github-$1"
  echo "=== $SCENARIO"
  build_forge_git github
  render_fixtures github
  start_forge github
  WT="$RUN_DIR/worktree-$SCENARIO"
  local fetch_url=${2//@PORT@/$PORT}
  make_worktree "$WT" https://ghe.test/acme/widgets.git "$fetch_url"
  if [ "$1" = hang ]; then
    printf '#!/bin/sh\nsleep 60\n' > "$RUN_DIR/hang.sh"
    chmod 755 "$RUN_DIR/hang.sh"
    git -C "$WT" config core.sshCommand "$RUN_DIR/hang.sh"
    export SIRIO_FORGE_FETCH_TIMEOUT_MS=2000
  fi
  launch_app ghe.test
  connect_and_open ghe.test github 101
  ctl surface change-request tab files
  local started elapsed
  started=$(date +%s)
  wait_for files_mode error surface change-request read
  elapsed=$(( $(date +%s) - started ))
  [ "$elapsed" -lt "$4" ] || fail "the failed fetch took ${elapsed}s, more than the ${4}s bound: it waited instead of failing"
  echo "OK: failed in ${elapsed}s"
  assert_contains files_error "$3" surface change-request read
  wait_for list_rows 4 surface change-request read
  echo "OK: the forge's own list is still there"
  [ -z "$(sirio_refs)" ] || fail "a failed fetch left refs behind"
  dump_refs "after the failure"
  capture files-error
  unset SIRIO_FORGE_FETCH_TIMEOUT_MS
  stop_app
  stop_forge
}

scenario_success github ghe.test github 101 '#101' https://ghe.test/acme/widgets.git commit-first
scenario_failure 401 'http://127.0.0.1:@PORT@/acme/widgets.git' 'terminal prompts disabled' 15
scenario_failure hang 'ssh://hang.invalid/acme/widgets.git' 'did not answer' 25
scenario_success gitlab gitlab.test gitlab 201 '!201' https://gitlab.test/team/app.git files-first

echo "artifact: $OUT_DIR"
echo "FORGE DIFF E2E OK"
