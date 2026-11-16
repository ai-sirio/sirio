#!/bin/bash
set -eEuo pipefail

# End-to-end test of the "Project added" dialog (docs/designs/
# project-identity-on-add-design.md, file 18): a real, isolated debug Sirio per
# scenario, driven through the debug-only `sirioctl surface project-add` and
# `surface project-identity` verbs, `sirioctl project add|list`, real X keys
# and clicks for the display steps, and the stored `project` rows.
#
# Every scenario starts a fresh app on a fresh SIRIO_DB, a fresh private HOME
# and a fresh SIRIO_PROJECTS_DIR, and builds its own fixtures under
# $WORK/<scenario>/. No scenario depends on another, so --only and --state-only
# select any of them. Every add uses the absolute path of its fixture.
#
# Scenarios (name, then what it proves; [display] ones are skipped with
# --state-only, because they need real keys, a window or a capture):
#    1 open_git            [display]
#    2 skip_matches_socket Skip leaves the same row as `sirioctl project add`; [display] step 4
#    3 save_unchanged      [display]
#    4 create_enter_release [display]
#    5 clone_async         [display]
#    6 clone_truncated     [display]
#    7 non_git             [display]
#    8 sidebar_hidden      [display]
#    9 over_settings       [display]
#   10 shortcuts_blocked   [display]
#   11 emoji_esc           [display]
#   12 path_selection_esc  [display]
#   13 queue               a second add waits behind the open dialog and opens on Skip
#   14 duplicate           an add of a tracked path opens no dialog, says why, keeps the label
#   15 refresh_then_relaunch [display]
#   16 picker_modes        [display]
#   17 over_open_form      [display]
#   18 settings_tab        [display]
#   19 copy_field          [display]
#   20 save_error          [display]
#   21 trace_privacy       [display]
#   22 icon_save           [display]
#   23 clone_dismissed     [display]
#   24 clone_replaced_fails  a replaced Clone form's clone that fails says so, its URL's password redacted
#   25 clone_dismissed_by_click [display]
#   26 clone_replaced_by_create  a Clone form replaced by the Create form mid-clone still adds the clone's project and opens its dialog
#   27 create_replaced_by_clone  a Create form replaced by the Clone form while its mkdir stalls still adds the folder's project and opens its dialog
#   28 create_replaced_fails     a replaced Create form whose mkdir then fails says so in the notice and a toast, and adds nothing
#
# The git wrapper: the debug build leads the PATH of its git calls with
# $XDG_DATA_HOME/Sirio/bin-debug (its own copy directory), so a `git` placed
# there is the one Sirio runs. It logs every call to <scenario>/git-calls.log
# and, on a flag file in <scenario>/, floods stderr (flood-clone), delays a
# clone (slow-clone), holds a clone until the scenario creates clone-go
# (gate-clone), makes a clone fail after leaving a half-made checkout
# (fail-clone) or delays discovery (slow-discovery).
#
# The mkdir hold (scenarios 27 and 28, Linux only): an LD_PRELOAD shim, built with
# cc into $WORK, makes the app's mkdir of exactly $SIRIO_E2E_MKDIR_HOLD stall like
# a slow filesystem: it creates <scenario>/mkdir-held and waits until the scenario
# creates <scenario>/mkdir-go (or its work dir is gone, or 300 s pass), then runs
# the real mkdir. It acts only in the process whose executable is named sirio:
# children (git, shells, sirio-host) inherit LD_PRELOAD and load it, but it stays
# inactive there. macOS has no LD_PRELOAD, so there both scenarios are skipped.
#
# The artifact: --out-dir DIR (default artifacts/project-identity-e2e-<stamp>-<pid>)
# keeps transcript.log (every sirioctl call and its reply, prefixed by the
# scenario), <scenario>/app.log, <scenario>/projects.tsv (the final `project`
# table), <scenario>/git-calls.log and frames/<scenario>-*.png (PID-matched
# captures). Rerunning the script reproduces it.
#
# Usage: Scripts/Tests/test-project-identity-e2e.sh [--xvfb | --display :N] [--state-only]
#          [--out-dir DIR] [--appearance light|dark] [--only NAME[,NAME...]]
#
#   --xvfb        start a private Xvfb on a free display between :90 and :99
#   --display :N  use an Xvfb this user started (never your own $DISPLAY)
#   --state-only  take no captures and send no real X input; the window still
#                 draws, because the dialog opens only in a window that renders
#                 (on Linux --xvfb or --display is needed all the same). macOS
#                 runs state-only.
#   --appearance  seed light or dark (default light)
#   --only        run the named scenarios alone and print
#                 `PROJECT IDENTITY E2E PARTIAL (<n> scenarios)` instead of OK

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/rust/target}"
case "$TARGET_DIR" in /*) ;; *) TARGET_DIR="$ROOT/$TARGET_DIR" ;; esac
BIN="$TARGET_DIR/debug/sirio"
CTL="$TARGET_DIR/debug/sirioctl"
SEED="$TARGET_DIR/debug/examples/appearance_seed"
PROBE="$TARGET_DIR/debug/examples/host_probe"
SURFACES_PROBE="$TARGET_DIR/debug/examples/surfaces_probe"

case "$(uname -s)" in
  Darwin) OS=macos ;;
  Linux) OS=linux ;;
  *) echo "PROJECT IDENTITY E2E runs on Linux and macOS only" >&2; exit 2 ;;
esac

# Every scenario, in the design's order, with its kind: `d` is a display
# scenario (skipped with --state-only), `s` runs everywhere.
SCENARIO_KINDS=(
  open_git:d skip_matches_socket:s save_unchanged:d create_enter_release:d
  clone_async:d clone_truncated:d non_git:d sidebar_hidden:d over_settings:d
  shortcuts_blocked:d emoji_esc:d path_selection_esc:d queue:s duplicate:s
  refresh_then_relaunch:d picker_modes:d over_open_form:d settings_tab:d
  copy_field:d save_error:d trace_privacy:d icon_save:d
  clone_dismissed:d clone_replaced_fails:s clone_dismissed_by_click:d
  clone_replaced_by_create:s create_replaced_by_clone:s create_replaced_fails:s
)
# Scenarios registered but not yet written. A run that reaches one fails: the
# OK line cannot be printed while this list is not empty.
UNWRITTEN=()
# Scenarios that need LD_PRELOAD (the mkdir hold): skipped wherever it is not Linux.
LINUX_ONLY=(create_replaced_by_clone create_replaced_fails)

STATE_ONLY=0
START_XVFB=0
DISPLAY_TARGET=""
OUT_DIR=""
APPEARANCE="light"
ONLY=""
while [ $# -gt 0 ]; do
  case "$1" in
    --xvfb) START_XVFB=1; shift ;;
    --display) DISPLAY_TARGET="${2:?--display needs :N}"; shift 2 ;;
    --state-only) STATE_ONLY=1; shift ;;
    --out-dir) OUT_DIR="${2:?--out-dir needs a directory}"; shift 2 ;;
    --appearance) APPEARANCE="${2:?--appearance needs light or dark}"; shift 2 ;;
    --only) ONLY=",${2:?--only needs scenario names},"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
case "$APPEARANCE" in light|dark) ;; *) echo "--appearance is light or dark" >&2; exit 2 ;; esac
if [ "$OS" = macos ]; then
  # No capture path and no X input there: macOS is state-only.
  STATE_ONLY=1
fi
if [ -n "$ONLY" ]; then
  for name in $(printf '%s' "$ONLY" | tr ',' ' '); do
    known=0
    for entry in "${SCENARIO_KINDS[@]}"; do
      [ "${entry%%:*}" = "$name" ] && known=1
    done
    [ "$known" -eq 1 ] || { echo "--only: unknown scenario $name" >&2; exit 2; }
  done
fi
[ -n "$OUT_DIR" ] || OUT_DIR="$ROOT/artifacts/project-identity-e2e-$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$OUT_DIR/frames"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"
exec > >(tee "$OUT_DIR/transcript.log") 2>&1

SCENARIO="setup"
fail() {
  echo "FAIL: $*" >&2
  exit 1
}
trap 'echo "FAIL: $SCENARIO: unexpected error at line $LINENO" >&2' ERR
note() { echo "[$SCENARIO] $*"; }

# Tools the checks use. Every X tool is pointed at this run's display, never at
# $DISPLAY. A state-only run on Linux still needs the display (the window must
# draw), but not the tools that type, click or capture.
for tool in python3 git; do
  command -v "$tool" >/dev/null || fail "$tool is required"
done
TIMEOUT_BIN="$(command -v timeout || command -v gtimeout || true)"
if [ "$OS" = linux ]; then
  for tool in xdpyinfo; do
    command -v "$tool" >/dev/null || fail "$tool is required"
  done
  if [ "$STATE_ONLY" -eq 0 ]; then
    for tool in xdotool xwininfo xprop import identify; do
      command -v "$tool" >/dev/null || fail "$tool is required; pass --state-only to skip real input and captures"
    done
  fi
  [ -n "$DISPLAY_TARGET" ] || [ "$START_XVFB" -eq 1 ] || fail "no display; pass --xvfb or --display :N (the dialog opens only in a window that renders, even with --state-only)"
  [ -z "$DISPLAY_TARGET" ] || [ "$DISPLAY_TARGET" != "${DISPLAY:-}" ] || fail "--display $DISPLAY_TARGET is your own \$DISPLAY; pass --xvfb, or an Xvfb you started on another number"
fi
REAL_GIT="$(command -v git)"
case "$REAL_GIT" in /*) ;; *) fail "git resolves to $REAL_GIT, not an absolute path" ;; esac

# ---------------------------------------------------------------- build

echo "building sirio, sirioctl, sirio-host, the host probe, the surfaces probe and the appearance seed"
(cd "$ROOT/rust" && cargo build --quiet -p sirio --bin sirio -p sirio_control --bin sirioctl \
  -p sirio_host --bin sirio-host -p sirio_host_client --example host_probe \
  -p sirio_persistence --example appearance_seed -p sirio_ui --example surfaces_probe)
for binary in "$BIN" "$CTL" "$SEED" "$PROBE" "$SURFACES_PROBE"; do
  [ -x "$binary" ] || fail "missing binary: $binary"
done

# The `project list` field of the project with id ID; `row-missing` for none.
listed_by_id() { # key id
  ctlq project list --json | python3 -I -c 'import json, sys
key, ident = sys.argv[1], sys.argv[2]
for project in json.load(sys.stdin):
    if project["id"] == ident:
        print(project.get(key, "no-field"))
        raise SystemExit
print("row-missing")' "$1" "$2"
}
# Polls until the `project list` KEY of the project rooted at ROOT reads WANT.
wait_list_value() { # key root want
  local tries=0 got=""
  while [ "$tries" -lt "$POLL_TRIES" ]; do
    got="$(listed "$1" "$2" 2>/dev/null || true)"
    if [ "$got" = "$3" ]; then
      note "OK: listed $1=$3"
      return 0
    fi
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$SCENARIO: listed $1 of $2 is '$got', not '$3'"
}
# A "stays" check on the list: KEY of ROOT reads WANT on every read for SECONDS.
hold_list_value() { # seconds key root want
  local end got
  end=$(( $(date +%s) + $1 ))
  while :; do
    got="$(listed "$2" "$3")"
    [ "$got" = "$4" ] || fail "$SCENARIO: listed $2 of $3 was '$got', not '$4'"
    [ "$(date +%s)" -lt "$end" ] || break
    sleep 0.2
  done
  note "OK: held for $1 s: listed $2=$4"
}
none_listed() { [ "$(listed_count)" = 0 ]; }
settings_open() { ctlq surface settings read >/dev/null 2>&1; }
TABS_BEFORE=""
tabs_unchanged() { [ "$(tab_count)" = "$TABS_BEFORE" ]; }

# 1. A first add is skipped with a real Esc; a second, with the same root name
# as the first, warns, is renamed by typing and saved with a real Return.
scenario_open_git() {
  local client="$SD/client/app" server="$SD/server/app" id_client id_server
  mkrepo "$client"; mkrepo "$server"

  note "step 1: add client/app with the path verb"
  add_path "$client"
  wait_listed "$client"
  id_client="$(listed id "$client")"
  wait_for open=true "projectId=$id_client"

  note "step 2: skip with Esc, a real key"
  xkey Escape
  wait_for open=false lastOutcome=skipped

  note "step 3: add server/app: focus on the field, the same-name warning, the root name"
  add_path "$server"
  wait_listed "$server"
  id_server="$(listed id "$server")"
  wait_for open=true "projectId=$id_server" focus=field warning=on nameDraft=app

  note "step 4: capture"
  capture dialog

  note "steps 5-7: type server (the name is preselected, so it replaces it), press Return"
  xtype server
  wait_for open=true nameDraft=server
  xkey Return
  wait_for open=false lastOutcome=saved

  note "step 8: the list shows the label at once (one read, no polling), and the colour"
  [ "$(listed displayName "$server")" = server ] || fail "$SCENARIO: displayName is '$(listed displayName "$server")' right after saved, not server"
  [ "$(listed color "$server")" = coral ] || fail "$SCENARIO: color is '$(listed color "$server")', not coral"
  note "OK: displayName=server color=coral"

  note "step 9: the stored row has no worktree base and no location override"
  wait_stored "$server" $'server\t<NULL>\t<NULL>' display_name default_worktree_base worktree_location_override
}

# 3. Return with no edit is Unchanged and writes the same row a socket add does.
scenario_save_unchanged() {
  local a="$SD/unchanged/a" b="$SD/unchanged/b"
  mkrepo "$a"; mkrepo "$b"

  note "step 1: add unchanged/a through sirioctl project add, without a label"
  ctl project add "$a" >/dev/null
  wait_listed "$a"

  note "step 2: add unchanged/b with the path verb and press Return without edits"
  add_path "$b"
  wait_listed "$b"
  wait_for open=true "projectId=$(listed id "$b")"
  xkey Return

  note "step 3: Unchanged, and the two rows are equal"
  wait_for open=false lastOutcome=unchanged
  wait_same_rows "$a" "$b" color_hex display_name icon_kind icon_value
}

# 4. A Return held down when the Create form opens the dialog must not save it
# when the key is released.
scenario_create_enter_release() {
  local made="$SD/projects/made"

  note "step 1: key repeat off for this display (the trap turns it back on)"
  keyrepeat_off

  note "step 2: the Create form with the name made, not submitted"
  add_form create made
  wait_for projectForm=create

  note "step 3-4: keydown Return opens the dialog"
  focus_app
  DISPLAY="$DISPLAY_TARGET" xdotool keydown Return
  wait_for open=true

  note "step 5-6: keyup Return leaves it open, on the field, with the root name and no label"
  DISPLAY="$DISPLAY_TARGET" xdotool keyup Return
  hold_for "$KEY_HOLD_SECS" open=true focus=field nameDraft=made
  wait_stored "$made" '<NULL>' display_name

  note "step 7: type made here, Return"
  xtype "made here"
  wait_for open=true "nameDraft=made here"
  xkey Return
  wait_for open=false lastOutcome=saved
  [ "$(listed displayName "$made")" = "made here" ] || fail "$SCENARIO: displayName is '$(listed displayName "$made")' right after saved, not 'made here'"
  note "OK: displayName=made here"
}

# 5. A clone opens the dialog on the dialog's own focus; typed keys do not edit
# the draft; clicking the field does; saving closes the form and moves focus to root.
scenario_clone_async() {
  local origin="$SD/origin/lib" draft id
  mkbare "$origin"

  note "step 1-2: the Clone form with the local path, submitted"
  add_form clone "$origin" --submit
  wait_for open=true origin=cloned focus=dialog
  read_state
  draft="$(state_value nameDraft)"
  id="$(state_value projectId)"
  [ -n "$draft" ] || fail "$SCENARIO: the draft is empty -- read: $(last_read)"

  note "step 3: ls and Return do not edit the draft or close the dialog"
  xtype ls
  xkey Return
  hold_for "$KEY_HOLD_SECS" open=true "nameDraft=$draft" "projectId=$id"

  note "step 4: click the field, Ctrl+A, type lib copy, Return"
  click_bounds fieldBounds
  xkey ctrl+a
  xtype "lib copy"
  wait_for open=true "nameDraft=lib copy"
  xkey Return
  wait_for open=false lastOutcome=saved
  [ "$(listed_by_id displayName "$id")" = "lib copy" ] || fail "$SCENARIO: displayName is '$(listed_by_id displayName "$id")' right after saved, not 'lib copy'"
  note "OK: displayName=lib copy"

  note "step 5: the form's field is gone, so focus moved to root; capture the sidebar"
  wait_for focus=root
  capture sidebar
}

# 6. A clone whose stderr runs past the output cap opens the dialog with a
# notice, and the form closes.
scenario_clone_truncated() {
  local origin="$SD/origin/lib"
  mkbare "$origin"
  : >"$SD/flood-clone"

  note "step 2: the Clone form submitted; the dialog says its output was cut"
  add_form clone "$origin" --submit
  wait_for open=true notice=on

  note "step 3-4: the form is gone; capture"
  wait_for projectForm=none
  capture notice
  act skip
  wait_for open=false lastOutcome=skipped
}

# 7. The not-a-git prompt's three answers.
scenario_non_git() {
  local cancel="$SD/plain/cancel" notes="$SD/plain/notes" init="$SD/plain/init"
  mkdir -p "$cancel" "$notes" "$init"

  note "step 1: answer 2 (Cancel): no dialog and no project, for the whole window"
  add_path "$cancel" --answer 2
  HOLD_CHECK=none_listed hold_for "$KEY_HOLD_SECS" open=false queue=0

  note "step 2: answer 1 (Add without Git)"
  add_path "$notes" --answer 1
  wait_for open=true origin=added-without-git focus=field
  note "step 3"
  act skip
  wait_for open=false lastOutcome=skipped

  note "step 4: answer 0 (Initialize Git)"
  add_path "$init" --answer 0
  wait_for open=true origin=initialized-git focus=dialog
  [ -d "$init/.git" ] || fail "$SCENARIO: $init/.git does not exist"
  note "OK: $init/.git exists"
  note "step 5"
  act skip
  wait_for open=false lastOutcome=skipped
}

# 8. With the sidebar hidden the dialog still opens, and focus ends on root.
scenario_sidebar_hidden() {
  note "step 1: hide the sidebar with its shortcut"
  xkey ctrl+shift+s
  wait_for sidebarVisible=false

  note "step 2: the Create form submitted: the dialog opens; capture; type a name, Return"
  add_form create hidden --submit
  wait_for open=true
  capture dialog
  xtype "hidden name"
  wait_for open=true "nameDraft=hidden name"
  xkey Return

  note "step 3: saved, the sidebar still hidden, focus on root"
  wait_for open=false lastOutcome=saved sidebarVisible=false focus=root
}

# 9. Esc on the dialog over the Settings screen closes the dialog only.
scenario_over_settings() {
  local app="$SD/settings/app"
  mkrepo "$app"
  ! settings_open || fail "$SCENARIO: Settings reads as open before it was opened"

  note "step 1: open Settings, then add settings/app with the path verb"
  ctl surface settings open >/dev/null
  settings_open || fail "$SCENARIO: Settings did not open"
  add_path "$app"
  wait_listed "$app"
  wait_for open=true "projectId=$(listed id "$app")"

  note "step 2: capture the dialog over the Settings screen"
  capture dialog

  note "step 3: Esc closes the dialog; Settings stays open"
  xkey Escape
  wait_for open=false lastOutcome=skipped
  HOLD_CHECK=settings_open hold_for "$HOLD_SECS" open=false
}

# 10. Tab, palette and close-tab shortcuts do nothing while the dialog is open.
scenario_shortcuts_blocked() {
  local app="$SD/keys/app" tries=0
  mkrepo "$app"

  note "precondition: a tab is open"
  ctl surface terminal open --cmd 'sleep 600' >/dev/null

  note "step 1: add keys/app with the path verb"
  add_path "$app"
  wait_listed "$app"
  wait_for open=true "projectId=$(listed id "$app")"
  while [ "$(tab_count)" = 0 ] && [ "$tries" -lt 50 ]; do tries=$((tries + 1)); sleep 0.2; done
  TABS_BEFORE="$(tab_count)"
  [ "$TABS_BEFORE" -ge 1 ] || fail "$SCENARIO: no tab is open, so Ctrl+Shift+W would prove nothing"
  note "tabs before: $TABS_BEFORE"

  note "step 2: Ctrl+T, Ctrl+Shift+P, Ctrl+Shift+W"
  xkey ctrl+t
  xkey ctrl+shift+p
  xkey ctrl+shift+w

  note "step 3: the same tab count, no palette, the dialog still open"
  HOLD_CHECK=tabs_unchanged hold_for "$KEY_HOLD_SECS" open=true paletteOpen=false pendingPaneClose=false
}

# 11. Esc closes the emoji grid first and the dialog second.
scenario_emoji_esc() {
  local app="$SD/emoji/app"
  mkrepo "$app"
  add_path "$app"
  wait_listed "$app"
  wait_for open=true "projectId=$(listed id "$app")"

  note "step 1: open the emoji grid (it is drawn only in Emoji mode, so the mode is set first)"
  act mode emoji
  act open-emoji-grid
  capture grid

  note "step 2: the first Esc leaves the dialog open"
  xkey Escape
  hold_for "$KEY_HOLD_SECS" open=true

  note "step 3: a second Esc closes it"
  xkey Escape
  wait_for open=false lastOutcome=skipped
}

# 12. A double click that selects the path moves focus to root; Esc there skips.
scenario_path_selection_esc() {
  local app="$SD/path/app"
  mkrepo "$app"
  add_path "$app"
  wait_listed "$app"
  wait_for open=true "projectId=$(listed id "$app")"

  note "step 1: double-click inside the path"
  double_click_bounds pathBounds
  wait_for open=true focus=root

  note "step 2: Esc skips the dialog"
  xkey Escape
  wait_for open=false lastOutcome=skipped
}

# 15. Saving while a focus refresh's discovery is running keeps the label (#684).
# Waits until no `worktree list` call has been logged for SECS seconds, within
# the positive waits' timeout.
wait_discovery_quiet() { # seconds
  local count last now quiet_since
  last="$(grep -c 'worktree list' "$SD/git-calls.log" || true)"
  quiet_since="$(date +%s)"
  local deadline=$(( quiet_since + 30 ))
  while :; do
    now="$(date +%s)"
    count="$(grep -c 'worktree list' "$SD/git-calls.log" || true)"
    if [ "$count" != "$last" ]; then
      last="$count"
      quiet_since="$now"
    elif [ $(( now - quiet_since )) -ge "$1" ]; then
      note "OK: no discovery for $1 s ($count worktree list calls)"
      return 0
    fi
    [ "$now" -lt "$deadline" ] || fail "$SCENARIO: discovery never went quiet for $1 s"
    sleep 0.2
  done
}

scenario_refresh_then_relaunch() {
  local app="$SD/refresh/app" id before pw tries=0 start save end
  mkrepo "$app"

  note "step 1: add refresh/app with the path verb"
  add_path "$app"
  wait_listed "$app"
  id="$(listed id "$app")"
  wait_for open=true "projectId=$id"

  note "step 2: select its workspace and wait for its own rescan"
  before="$(grep -cF -- "worktree list --porcelain  cwd=$app" "$SD/git-calls.log" || true)"
  ctl select-workspace --workspace "$app" >/dev/null
  while [ "$(grep -cF -- "worktree list --porcelain  cwd=$app" "$SD/git-calls.log" || true)" -le "$before" ] && [ "$tries" -lt "$POLL_TRIES" ]; do
    tries=$((tries + 1)); sleep 0.2
  done
  [ "$tries" -lt "$POLL_TRIES" ] || fail "$SCENARIO: selecting the workspace never ran 'worktree list' in $app -- git-calls.log: $(tail -n 5 "$SD/git-calls.log" | tr '\n' '|')"
  note "OK: the rescan ran ($before calls before, $(grep -cF -- "worktree list --porcelain  cwd=$app" "$SD/git-calls.log") now)"

  note "step 3: while Sirio has focus, type refreshed in the field"
  wait_for open=true focus=field
  xtype refreshed
  wait_for open=true nameDraft=refreshed

  # The probe takes focus before the flag file exists, and the flag waits for
  # the selection's later rescans to stop: one landing after the flag would
  # pass for the refocus's.
  note "step 5: start the surfaces probe so it takes focus from Sirio"
  (exec env -i HOME="$SD/home" DISPLAY="$DISPLAY_TARGET" GPUI_X11_SCALE_FACTOR=1 TMPDIR="$SD/tmp" \
    ${VULKAN_ENV[@]+"${VULKAN_ENV[@]}"} "$SURFACES_PROBE" --frame settings-git-hosting --appearance "$APPEARANCE" \
    >"$SD/probe.log" 2>&1) &
  SURFACES_PID=$!
  pw=""
  tries=0
  while [ -z "$pw" ] && [ "$tries" -lt 100 ]; do
    pw="$(DISPLAY="$DISPLAY_TARGET" xdotool search --onlyvisible --pid "$SURFACES_PID" 2>/dev/null | head -n 1 || true)"
    tries=$((tries + 1)); sleep 0.2
  done
  [ -n "$pw" ] || fail "$SCENARIO: the surfaces probe never showed a window (see $SD/probe.log)"
  DISPLAY="$DISPLAY_TARGET" with_timeout 10 xdotool windowfocus --sync "$pw"
  note "OK: the probe's window $pw has the focus"
  wait_discovery_quiet 3

  note "step 4: the wrapper will hold the next discovery for 3 s"
  : >"$SD/slow-discovery"
  sleep 1

  # Discovery must not have run yet, or the START below would not be the refocus's.
  ! grep -Fq SLOW-DISCOVERY-START "$SD/git-calls.log" || fail "$SCENARIO: a slow discovery started before Sirio was refocused"

  note "step 6: refocus Sirio's window; that starts the focus refresh"
  focus_app
  wait_log SLOW-DISCOVERY-START

  note "step 7: while discovery sleeps, act save; the reply is stamped in the log"
  act save
  echo SAVE-REPLIED >>"$SD/git-calls.log"
  wait_log SLOW-DISCOVERY-END

  note "step 8: the log is START, SAVE-REPLIED, END, in that order; the label survives the landing"
  start="$(grep -nF SLOW-DISCOVERY-START "$SD/git-calls.log" | head -n 1 | cut -d: -f1)"
  save="$(grep -nF SAVE-REPLIED "$SD/git-calls.log" | head -n 1 | cut -d: -f1)"
  end="$(grep -nF SLOW-DISCOVERY-END "$SD/git-calls.log" | head -n 1 | cut -d: -f1)"
  { [ "$start" -lt "$save" ] && [ "$save" -lt "$end" ]; } || fail "$SCENARIO: the order is START line $start, SAVE-REPLIED line $save, END line $end"
  note "OK: START (line $start) < SAVE-REPLIED (line $save) < END (line $end)"
  hold_list_value 6 displayName "$app" refreshed

  note "step 9: no slow discovery; quit and relaunch on the same database"
  rm -f "$SD/slow-discovery"
  terminate "$SURFACES_PID"
  SURFACES_PID=""
  app_quit
  app_start
  wait_list_value displayName "$app" refreshed
  capture relaunch
}

# 16. The picker's three modes, one capture each; the Avatar caption and its
# source rows are judged by eye from the captures.
scenario_picker_modes() {
  local app="$SD/picker/app" mode
  mkrepo "$app"
  add_path "$app"
  wait_listed "$app"
  wait_for open=true "projectId=$(listed id "$app")"
  for mode in icon emoji avatar; do
    note "mode $mode"
    act mode "$mode"
    read_state
    capture "mode-$mode"
  done
  must_differ "Icon and Emoji" mode-icon mode-emoji
  must_differ "Emoji and Avatar" mode-emoji mode-avatar
  act skip
  wait_for open=false lastOutcome=skipped
}

# 17. Esc skips the dialog and leaves the Clone form behind the scrim open.
scenario_over_open_form() {
  local origin="$SD/origin/lib" two="$SD/fresh/two"
  mkbare "$origin"; mkrepo "$two"
  : >"$SD/slow-clone"

  note "step 1: a clone is running"
  add_form clone "$origin" --submit
  wait_log "git clone"
  wait_for projectForm=clone

  note "step 2: add fresh/two with the path verb"
  add_path "$two"
  wait_listed "$two"
  wait_for open=true "projectId=$(listed id "$two")"

  note "steps 3-4: one Esc skips the dialog; the form stays open"
  xkey Escape
  wait_for open=false lastOutcome=skipped projectForm=clone
  hold_for "$HOLD_SECS" open=false projectForm=clone
}

# 23. Esc on the Clone form while git runs (#681): the clone goes on, and when it
# lands its project is added and its "Project added" dialog opens, as for any
# clone, with focus on the dialog itself; Skip gives focus back to root.
scenario_clone_dismissed() {
  local origin="$SD/origin/lib" checkout="$SD/projects/lib"
  mkbare "$origin"
  : >"$SD/gate-clone"

  note "step 1: a clone is running"
  add_form clone "$origin" --submit
  wait_log "git clone"
  wait_for projectForm=clone focus=other

  note "step 2: Esc dismisses the form while git still runs, and focus goes back to root"
  xkey Escape
  wait_for projectForm=none focus=root
  note "release the clone"
  : >"$SD/clone-go"

  note "step 3: the project is listed and the checkout holds the origin's file"
  wait_listed "$checkout"
  [ -f "$checkout/README.md" ] || fail "$SCENARIO: $checkout has no README.md: the clone did not arrive"
  note "OK: $checkout holds README.md"

  note "step 4: its dialog opens as for any clone, focus on the dialog, with no notice"
  wait_for open=true origin=cloned focus=dialog "projectId=$(listed id "$checkout")" sidebarNotice=

  note "step 5: Skip closes it and focus goes back to root"
  act skip
  wait_for open=false lastOutcome=skipped focus=root
  [ "$(listed_count)" = 1 ] || fail "$SCENARIO: $(listed_count) projects are listed, not 1"
}

# 24. A clone that fails after its form was replaced by the Create form (#681)
# says so, with the URL's credentials redacted, in the sidebar's notice and in a
# toast, and leaves no folder, no project and no dialog behind. It sends no X
# input, so the failure path and the redaction also run with --state-only.
scenario_clone_replaced_fails() {
  local url="https://user:secret@example.invalid/lib.git" checkout="$SD/projects/lib" notice want
  want="could not clone https://example.invalid/lib.git: fatal: the E2E wrapper failed this clone Please make sure you have the correct access rights"
  : >"$SD/gate-clone"
  : >"$SD/fail-clone"

  note "step 1: a clone is running"
  add_form clone "$url" --submit
  wait_log "git clone"
  wait_for projectForm=clone

  note "step 2: the Create form replaces the Clone form while git still runs"
  add_form create other
  wait_for projectForm=create
  note "release the clone"
  : >"$SD/clone-go"

  note "step 3: the failure is in the sidebar's notice and in a toast, without the password"
  wait_nonempty sidebarNotice
  notice="$(state_value sidebarNotice)"
  if [ "$notice" = "$want" ]; then
    note "OK: sidebarNotice=$notice"
  else
    fail "$SCENARIO: sidebarNotice is '$notice', not the clone's failure"
  fi
  [ "$(state_value toast)" = "$want" ] || fail "$SCENARIO: toast is '$(state_value toast)', not the clone's failure"
  ! grep -Fq secret <<<"$STATE" || fail "$SCENARIO: the read shows the URL's password: $(last_read)"
  capture notice

  note "step 4: no folder, no project, no dialog"
  [ ! -e "$checkout" ] || fail "$SCENARIO: the failed clone left $checkout behind"
  [ "$(listed_count)" = 0 ] || fail "$SCENARIO: $(listed_count) project(s) are listed, not 0"
  hold_for "$HOLD_SECS" open=false projectForm=create
}

# 25. CUJ 2 with the Clone form dismissed (#681): a Clone form under the dialog,
# dismissed by a click inside the dialog while git runs. After Skip, focus is
# back on root and keys reach the workspace; the user moves to a terminal, and
# when git finishes the clone's dialog opens with focus on the dialog itself:
# keys typed before a click in its field do not act on it, nothing is saved,
# and Skip gives focus back to the terminal.
scenario_clone_dismissed_by_click() {
  local two="$SD/fresh/two" three="$SD/fresh/three" checkout="$SD/projects/lib" tabs tries=0 id draft
  mkrepo "$two"; mkrepo "$three"; mkbare "$SD/origin/lib"
  : >"$SD/gate-clone"
  ctl project add "$two" >/dev/null
  wait_listed "$two"
  ctl select-workspace --workspace "$two" >/dev/null

  note "step 1: a clone is running, its form open with the URL field focused"
  add_form clone "$SD/origin/lib" --submit
  wait_log "git clone"
  wait_for projectForm=clone focus=other

  note "step 2: an add opens the dialog over the form"
  add_path "$three"
  wait_listed "$three"
  wait_for open=true "projectId=$(listed id "$three")"

  note "step 3: a click inside the dialog, outside the card, dismisses the form while git still runs"
  click_bounds pathBounds
  wait_for projectForm=none open=true

  note "step 4: Skip hands focus back to root, not to the dismissed form's field"
  act skip
  wait_for open=false lastOutcome=skipped focus=root

  note "step 5: Ctrl+T reaches the workspace and opens a terminal"
  tabs="$(tab_count)"
  xkey ctrl+t
  while [ "$(tab_count)" -le "$tabs" ] && [ "$tries" -lt "$POLL_TRIES" ]; do
    tries=$((tries + 1))
    sleep 0.2
  done
  [ "$(tab_count)" -gt "$tabs" ] || fail "$SCENARIO: Ctrl+T opened no tab (still $tabs): keys do not reach the workspace"
  note "OK: Ctrl+T opened a tab"
  wait_for focus=terminal
  note "the user runs ls in the terminal before the clone finishes"
  xtype ls
  xkey Return
  note "release the clone"
  : >"$SD/clone-go"

  note "step 6: the clone's dialog opens with focus on the dialog itself, its field holding the root name, not ls"
  wait_listed "$checkout"
  [ -f "$checkout/README.md" ] || fail "$SCENARIO: $checkout has no README.md: the clone did not arrive"
  id="$(listed id "$checkout")"
  wait_for open=true origin=cloned focus=dialog "projectId=$id" nameDraft=lib
  draft=lib

  note "step 7: ls and Return typed before a click in the field do not edit the draft or close the dialog"
  xtype ls
  xkey Return
  hold_for "$KEY_HOLD_SECS" open=true "nameDraft=$draft" "projectId=$id" focus=dialog

  note "step 8: Skip gives focus back to the terminal, and nothing was saved"
  act skip
  wait_for open=false lastOutcome=skipped focus=terminal
  hold_for "$HOLD_SECS" open=false focus=terminal
  [ -z "$(listed_by_id displayName "$id")" ] || fail "$SCENARIO: displayName is '$(listed_by_id displayName "$id")' after Skip, not empty"
  note "OK: no displayName saved"
  [ "$(listed_count)" = 3 ] || fail "$SCENARIO: $(listed_count) projects are listed, not 3"
}

# 26. The Create form opened over a Clone form whose clone is running (#681):
# the replaced form's clone goes on, adds its project and opens its dialog, as
# for any clone, over the Create form, which stays open behind it.
scenario_clone_replaced_by_create() {
  local origin="$SD/origin/lib" checkout="$SD/projects/lib"
  mkbare "$origin"
  : >"$SD/gate-clone"

  note "step 1: a clone is running"
  add_form clone "$origin" --submit
  wait_log "git clone"
  wait_for projectForm=clone

  note "step 2: the Create form replaces the Clone form while git still runs"
  add_form create other
  wait_for projectForm=create
  if ! skip_display "step 2: the Create form's field holds focus (focus=other)"; then
    wait_for focus=other
  fi
  note "release the clone"
  : >"$SD/clone-go"

  note "step 3: the clone adds its project and its dialog opens over the Create form"
  wait_listed "$checkout"
  [ -f "$checkout/README.md" ] || fail "$SCENARIO: $checkout has no README.md: the clone did not arrive"
  wait_for open=true origin=cloned "projectId=$(listed id "$checkout")" projectForm=create
  if ! skip_display "step 3: focus=dialog, not the Create form's field"; then
    wait_for focus=dialog
  fi

  note "step 4: Skip closes the dialog and leaves the Create form open, with no notice"
  act skip
  wait_for open=false lastOutcome=skipped projectForm=create
  hold_for "$HOLD_SECS" open=false projectForm=create sidebarNotice=
  if ! skip_display "step 4: Skip gives focus back to the Create form's field (focus=other)"; then
    wait_for focus=other projectForm=create
    hold_for "$HOLD_SECS" focus=other projectForm=create
  fi
  [ "$(listed_count)" = 1 ] || fail "$SCENARIO: $(listed_count) projects are listed, not 1"
}

# 27. The Clone form opened over a Create form whose folder is still being made
# (#681; a stalled mkdir, held by the mkdir hold): the replaced form's Create goes
# on, adds its project and opens its dialog, as after any Create, over the Clone
# form, which stays open behind it.
scenario_create_replaced_by_clone() {
  local made="$SD/projects/made"
  note "step 1: the Create form submitted; its mkdir is held"
  add_form create made --submit
  wait_file "$SD/mkdir-held"
  wait_for projectForm=create
  [ ! -e "$made" ] || fail "$SCENARIO: $made exists while its mkdir is held"

  note "step 2: the Clone form replaces the Create form while the folder is still being made"
  add_form clone "$SD/origin/none"
  wait_for projectForm=clone
  note "release the mkdir"
  : >"$SD/mkdir-go"

  note "step 3: the folder is made, its project listed, its dialog open over the Clone form"
  wait_file "$made"
  wait_listed "$made"
  wait_for open=true origin=created "projectId=$(listed id "$made")" nameDraft=made projectForm=clone
  if ! skip_display "step 3: focus=field, as after any Create"; then
    wait_for focus=field
  fi

  note "step 4: Skip closes the dialog and leaves the Clone form open, with no notice"
  act skip
  wait_for open=false lastOutcome=skipped projectForm=clone
  hold_for "$HOLD_SECS" open=false projectForm=clone sidebarNotice=
  if ! skip_display "step 4: Skip gives focus back to the Clone form's field (focus=other)"; then
    wait_for focus=other projectForm=clone
  fi
  [ "$(listed_count)" = 1 ] || fail "$SCENARIO: $(listed_count) projects are listed, not 1"
}

# 28. A Create whose form was replaced while its mkdir stalled, and whose mkdir
# then fails because the folder appeared meanwhile (#681), says so in the
# sidebar's notice and in a toast, adds no project and opens no dialog.
scenario_create_replaced_fails() {
  local made="$SD/projects/made" want notice
  want="could not create $made: File exists (os error 17)"
  note "step 1: the Create form submitted; its mkdir is held"
  add_form create made --submit
  wait_file "$SD/mkdir-held"
  wait_for projectForm=create

  note "step 2: the Clone form replaces the Create form while the folder is still being made"
  add_form clone "$SD/origin/none"
  wait_for projectForm=clone

  note "step 3: the folder appears before the held mkdir runs, so that mkdir fails"
  mkdir "$made"
  : >"$SD/mkdir-go"

  note "step 4: the failure is in the sidebar's notice and in a toast"
  wait_nonempty sidebarNotice
  notice="$(state_value sidebarNotice)"
  [ "$notice" = "$want" ] || fail "$SCENARIO: sidebarNotice is '$notice', not '$want'"
  note "OK: sidebarNotice=$notice"
  [ "$(state_value toast)" = "$want" ] || fail "$SCENARIO: toast is '$(state_value toast)', not '$want'"
  capture notice

  note "step 5: no project and no dialog; the Clone form stays"
  [ "$(listed_count)" = 0 ] || fail "$SCENARIO: $(listed_count) project(s) are listed, not 0"
  hold_for "$HOLD_SECS" open=false projectForm=clone
}

# 18. The Project Settings tab's label field writes at once, and typing the root name clears it.
scenario_settings_tab() {
  local app="$SD/tab/app" label="tab label" backspaces=()
  mkrepo "$app"

  note "step 1: add it through sirioctl project add and select its workspace"
  ctl project add "$app" >/dev/null
  wait_listed "$app"
  ctl select-workspace --workspace "$app" >/dev/null

  note "step 2: open its Project Settings tab, click the label field, type a label"
  ctl surface open-list open --entry project-settings >/dev/null
  wait_nonempty projectSettingsLabelBounds
  click_bounds projectSettingsLabelBounds
  xtype "$label"
  wait_list_value displayName "$app" "$label"

  note "step 3: BackSpace as many times as the label has characters, then type the root name"
  local i
  for i in $(seq 1 ${#label}); do backspaces+=(BackSpace); done
  xkey "${backspaces[@]}"
  wait_list_value displayName "$app" ""
  xtype app
  wait_stored "$app" '<NULL>' display_name
  hold_list_value "$HOLD_SECS" displayName "$app" ""
}

# 19. Ctrl+A, Ctrl+C in the name field copies its text and keeps the dialog.
scenario_copy_field() {
  local app="$SD/copy/app"
  mkrepo "$app"
  add_path "$app"
  wait_listed "$app"
  wait_for open=true "projectId=$(listed id "$app")" focus=field nameDraft=app

  note "step 1: Ctrl+A, Ctrl+C in the field"
  xkey ctrl+a
  xkey ctrl+c

  note "step 2: the clipboard holds the field's text and the dialog is still open"
  expect_clipboard "Ctrl+C copied the field" app
  hold_for "$HOLD_SECS" open=true nameDraft=app
}

# 20. A save that fails closes its dialog with a toast and opens the queued one.
prelaunch_save_error() { APP_EXTRA_ENV+=("SIRIO_DEBUG_FAIL_CATALOG_WRITES=1"); }
scenario_save_error() {
  local one="$SD/err/one" two="$SD/err/two" id_one id_two
  mkrepo "$one"; mkrepo "$two"

  note "step 1: add both with the path verb, back to back"
  add_path "$one"
  add_path "$two"
  wait_listed "$one"
  wait_listed "$two"
  id_one="$(listed id "$one")"
  id_two="$(listed id "$two")"
  wait_for open=true queue=1 "projectId=$id_one"

  note "step 2: type a label in the first dialog and act save"
  xtype "label that cannot be written"
  wait_for open=true "nameDraft=label that cannot be written"
  act save

  note "step 3: a toast, the dialog closed, the other still queued, the outcome error"
  wait_nonempty toast
  state_has open=false || fail "$SCENARIO: open is not false with the toast up -- read: $(last_read)"
  state_has queue=1 || fail "$SCENARIO: queue is not 1 with the toast up -- read: $(last_read)"
  state_has lastOutcome=error || fail "$SCENARIO: lastOutcome is not error -- read: $(last_read)"
  note "OK: open=false queue=1 lastOutcome=error"

  note "step 4: after the toast clears, the second project's dialog opens"
  wait_for open=true queue=0 "projectId=$id_two" toast=
}

# 21. The perf trace and the app log carry no label, path or project id.
prelaunch_trace_privacy() { APP_EXTRA_ENV+=("SIRIO_PERF_TRACE=$SD/trace.tsv"); }
scenario_trace_privacy() {
  local one="$SD/trace/one" two="$SD/trace/two" label="PRIVLABEL-9c41e7a2b86f-NEVER-IN-A-TRACE" id_one id_two needle lines
  mkrepo "$one"; mkrepo "$two"

  note "step 1: add trace/one, type a label, act save; add trace/two, act skip"
  add_path "$one"
  wait_listed "$one"
  id_one="$(listed id "$one")"
  wait_for open=true "projectId=$id_one"
  xtype "$label"
  wait_for open=true "nameDraft=$label"
  act save
  wait_for open=false lastOutcome=saved
  add_path "$two"
  wait_listed "$two"
  id_two="$(listed id "$two")"
  wait_for open=true "projectId=$id_two"
  act skip
  wait_for open=false lastOutcome=skipped
  [ "$(listed displayName "$one")" = "$label" ] || fail "$SCENARIO: the label was not saved, so the grep proves nothing"

  note "step 2: quit (the trace is flushed), then grep the trace and app.log"
  app_quit
  lines="$(wc -l <"$SD/trace.tsv" | tr -d ' ')"
  [ "$lines" -gt 2 ] || fail "$SCENARIO: the trace has $lines line(s); it recorded nothing to search"
  [ -s "$SD/app.log" ] || fail "$SCENARIO: app.log is empty"
  note "trace: $lines lines; app.log: $(wc -l <"$SD/app.log" | tr -d ' ') lines"
  # The basenames one and two are not distinctive, so the absolute paths and
  # their parent directory stand for them. (The scenario's own directory is
  # not a project path: the control socket's line in app.log names it.)
  for needle in "$label" "$one" "$two" "$SD/trace" "$id_one" "$id_two"; do
    if grep -Fq -- "$needle" "$SD/trace.tsv" "$SD/app.log"; then
      fail "$SCENARIO: '$needle' appears in the trace or app.log: $(grep -F -- "$needle" "$SD/trace.tsv" "$SD/app.log" | head -n 3 | cut -c1-200)"
    fi
  done
  note "OK: none of the label, both paths, their parent directory and both ids appears in either file"
}

# 22. Picking a glyph and saving stores the icon; picking and picking back is Unchanged.
scenario_icon_save() {
  local app="$SD/icon/app" two="$SD/icon/two"
  mkrepo "$app"; mkrepo "$two"

  note "step 1: pick terminal and save"
  add_path "$app"
  wait_listed "$app"
  wait_for open=true "projectId=$(listed id "$app")"
  act pick-glyph terminal
  wait_for iconKind=icon iconValue=terminal
  act save
  wait_for open=false lastOutcome=saved
  wait_stored "$app" $'icon\tterminal\t<NULL>\t<NULL>' icon_kind icon_value default_worktree_base worktree_location_override

  note "step 2: pick terminal, then pick the original folder back, save: Unchanged"
  add_path "$two"
  wait_listed "$two"
  wait_for open=true "projectId=$(listed id "$two")"
  act pick-glyph terminal
  wait_for iconValue=terminal
  act pick-glyph folder
  wait_for iconValue=folder
  act save
  wait_for open=false lastOutcome=unchanged
}

# ---------------------------------------------------------------- the run

BASE="${SIRIO_E2E_TMP:-$HOME/.cache/sirio-e2e}"
mkdir -p "$BASE"
# The real path: the app canonicalizes every path it stores and lists.
WORK="$(python3 -I -c 'import os, sys; print(os.path.realpath(sys.argv[1]))' "$(mktemp -d "$BASE/pid-XXXXXX")")"
SD=""
APP_PID=""
XVFB_PID=""
SURFACES_PID=""
KEYREPEAT_CHANGED=0
VULKAN_ENV=()
SCENARIOS_PASSED=0
SKIPPED=()

is_a_host() {
  local comm
  if [ -r "/proc/$1/comm" ]; then comm="$(cat "/proc/$1/comm" 2>/dev/null || true)"
  else comm="$(ps -p "$1" -o comm= 2>/dev/null || true)"
  fi
  case "$comm" in *sirio-host*) return 0 ;; *) return 1 ;; esac
}
alive() { kill -0 "$1" 2>/dev/null; }
# Ends a child of this script: TERM, then KILL if it has not exited within 5 s.
terminate() { # pid
  kill -TERM "$1" 2>/dev/null || return 0
  local n=0
  while alive "$1" && [ "$n" -lt 50 ]; do n=$((n + 1)); sleep 0.1; done
  kill -KILL "$1" 2>/dev/null || true
  wait "$1" 2>/dev/null || true
}
# Every host this run started: each scenario has its own SIRIO_HOST_HOME, and
# a host is detached, so it is not this shell's child.
stop_hosts() {
  local dir pid
  for dir in "$WORK"/*/host; do
    [ -d "$dir" ] || continue
    SIRIO_HOST_HOME="$dir" "$PROBE" shutdown --force >/dev/null 2>&1 || true
  done
  for pid in $(pgrep -f "$WORK" 2>/dev/null || true); do
    is_a_host "$pid" && kill "$pid" 2>/dev/null || true
  done
}
# The current scenario's evidence into the out dir; run on success and on failure.
collect_scenario() {
  [ -n "$SD" ] && [ -d "$SD" ] || return 0
  mkdir -p "$OUT_DIR/$SCENARIO"
  cp "$SD/app.log" "$SD/git-calls.log" "$OUT_DIR/$SCENARIO/" 2>/dev/null || true
  dump_projects "$OUT_DIR/$SCENARIO/projects.tsv" 2>/dev/null || true
}
cleanup() {
  local rc=$?
  set +e
  trap - ERR
  [ -z "$APP_PID" ] || terminate "$APP_PID"
  [ -z "$SURFACES_PID" ] || terminate "$SURFACES_PID"
  [ "$rc" -eq 0 ] || collect_scenario
  stop_hosts
  if [ "$KEYREPEAT_CHANGED" -eq 1 ]; then DISPLAY="$DISPLAY_TARGET" xset r on >/dev/null 2>&1; fi
  [ -z "$XVFB_PID" ] || terminate "$XVFB_PID"
  rm -rf "$WORK"
  exit "$rc"
}
trap cleanup EXIT

# A display: a private Xvfb on a free number, or the one given.
if [ "$OS" = linux ]; then
  if [ "$START_XVFB" -eq 1 ]; then
    command -v Xvfb >/dev/null || fail "Xvfb is required for --xvfb"
    for number in $(seq 90 99); do
      [ -e "/tmp/.X11-unix/X$number" ] || [ -e "/tmp/.X$number-lock" ] || { DISPLAY_TARGET=":$number"; break; }
    done
    [ -n "$DISPLAY_TARGET" ] || fail "no free display between :90 and :99"
    # A 1920x1080 screen: the window is then whole in every capture.
    Xvfb "$DISPLAY_TARGET" -screen 0 1920x1080x24 >"$OUT_DIR/xvfb.log" 2>&1 &
    XVFB_PID=$!
    for _ in $(seq 1 50); do DISPLAY="$DISPLAY_TARGET" timeout 2 xdpyinfo >/dev/null 2>&1 && break; sleep 0.1; done
    DISPLAY="$DISPLAY_TARGET" timeout 2 xdpyinfo >/dev/null 2>&1 || fail "Xvfb did not start on $DISPLAY_TARGET"
  fi
  lvp="$(ls /usr/share/vulkan/icd.d/lvp_icd*.json 2>/dev/null | head -n 1 || true)"
  [ -n "$lvp" ] || fail "software Vulkan (lvp_icd*.json in /usr/share/vulkan/icd.d) is required on Xvfb"
  VULKAN_ENV=("VK_DRIVER_FILES=$lvp" "VK_ICD_FILENAMES=$lvp")
  echo "display: $DISPLAY_TARGET (appearance $APPEARANCE, state-only $STATE_ONLY)"
else
  echo "macOS: state-only (appearance $APPEARANCE)"
fi

# Shared by every scenario: stub agent CLIs (without them the app runs
# `claude --version` through the user's real mise wrapper) and a shell that
# reads no profile, so the login PATH the app reads is the launch PATH.
mkdir -p "$WORK/stubs" "$WORK/shell"
for cli in claude codex opencode pi omp; do
  printf '#!/bin/sh\necho "2.1.257 (Claude Code)"\n' >"$WORK/stubs/$cli"
  chmod +x "$WORK/stubs/$cli"
done
printf '#!/bin/sh\nexec /bin/bash --noprofile --norc "$@"\n' >"$WORK/shell/shell"
chmod +x "$WORK/shell/shell"

# ---------------------------------------------------------------- calling the app

CALL_TIMEOUT=30
with_timeout() { # seconds command...
  local seconds=$1
  shift
  if [ -n "$TIMEOUT_BIN" ]; then "$TIMEOUT_BIN" -k 5 "$seconds" "$@"; else "$@"; fi
}
# sirioctl with nothing of the caller's environment but this scenario's socket:
# a script started inside a Sirio pane must not hand its pane variables to it.
ctlq() { # args...: quiet, for polled reads
  with_timeout "$CALL_TIMEOUT" env -i PATH=/usr/bin:/bin HOME="$SD/home" SIRIO_SOCKET="$SIRIO_SOCKET" "$CTL" "$@"
}
# One logged call: the call and its reply go to the transcript, prefixed by the
# scenario; the reply is also on stdout. A failing call fails the scenario.
ctl() {
  local out rc=0
  echo "[$SCENARIO] + sirioctl $*" >&2
  out="$(ctlq "$@" 2>&1)" || rc=$?
  if [ -n "$out" ]; then printf '%s\n' "$out" | sed "s/^/[$SCENARIO]   /" >&2; fi
  [ "$rc" -eq 0 ] || fail "$SCENARIO: sirioctl $* exited $rc"
  printf '%s\n' "$out"
}
# The same call, for one that is expected to fail: the exit code is the answer.
ctl_try() {
  local out rc=0
  echo "[$SCENARIO] + sirioctl $* (may fail)" >&2
  out="$(ctlq "$@" 2>&1)" || rc=$?
  if [ -n "$out" ]; then printf '%s\n' "$out" | sed "s/^/[$SCENARIO]   /" >&2; fi
  return "$rc"
}

# The dialog's state: `key=value` lines, matched whole (a substring never counts).
STATE=""
read_state() {
  STATE="$(ctlq surface project-identity read 2>&1)" || fail "$SCENARIO: surface project-identity read did not answer: $STATE"
}
# Here-strings, not pipes: under pipefail `grep -q` closing the pipe early makes
# the writer's SIGPIPE a failure of the match.
state_value() { sed -n "s/^$1=//p" <<<"$STATE" | head -n 1; }
state_has() { grep -Fxq -- "$1" <<<"$STATE"; }
last_read() { printf '%s' "$STATE" | tr '\n' ' ' | cut -c1-700; }

POLL_TRIES=150 # x 0.2 s: the timeout every positive wait and every unguarded negative one uses
HOLD_SECS=3    # a "stays" window after a completion signal has already been seen
KEY_HOLD_SECS=$((POLL_TRIES / 5)) # a "stays" window after real keys: nothing signals when the app has handled them
HOLD_CHECK=""

# Polls until one read holds every KEY=VALUE given; the timeout is the failure.
# A read the app does not answer (its main thread busy) is a miss, not a failure:
# the timeout still ends the wait, and the last miss is in its message.
wait_for() { # key=value...
  local deadline pair ok
  deadline=$(( $(date +%s) + POLL_TRIES / 5 ))
  while [ "$(date +%s)" -lt "$deadline" ]; do
    STATE="$(ctlq surface project-identity read 2>&1)" || { STATE="(no answer) $STATE"; sleep 0.2; continue; }
    ok=1
    for pair in "$@"; do
      state_has "$pair" || { ok=0; break; }
    done
    if [ "$ok" -eq 1 ]; then
      note "OK: $*"
      return 0
    fi
    sleep 0.2
  done
  fail "$SCENARIO: never read: $* -- last read: $(last_read)"
}
# A "stays" check: every read for SECONDS must hold every KEY=VALUE. One bad
# read fails it. A check of this kind is only meaningful after a completion
# signal (a project listed, a toast, an outcome, a git-calls.log line), or for
# the full POLL_TRIES x 0.2 s window.
hold_for() { # seconds key=value...
  local end pair
  end=$(( $(date +%s) + $1 ))
  shift
  while :; do
    read_state
    for pair in "$@"; do
      state_has "$pair" || fail "$SCENARIO: $pair stopped holding -- read: $(last_read)"
    done
    # HOLD_CHECK names a command that must also keep succeeding (a tab count, say).
    [ -z "${HOLD_CHECK:-}" ] || "$HOLD_CHECK" || fail "$SCENARIO: $HOLD_CHECK stopped holding -- read: $(last_read)"
    [ "$(date +%s)" -lt "$end" ] || break
    sleep 0.2
  done
  note "OK: held: $*"
}

# ---------------------------------------------------------------- the stored rows and the list

# The database's `project` row of the project rooted at ROOT, the COLUMNs
# tab-separated, <NULL> for a NULL; `row-missing` for no such row. Read-only.
stored() { # root column...
  python3 -I -c 'import os, sqlite3, sys
db, root, columns = sys.argv[1], os.path.realpath(sys.argv[2]), sys.argv[3:]
connection = sqlite3.connect("file:" + db + "?mode=ro", uri=True, timeout=5)
for row in connection.execute("SELECT root_path, " + ", ".join(columns) + " FROM project"):
    if os.path.realpath(row[0]) == root:
        print("\t".join("<NULL>" if value is None else str(value) for value in row[1:]))
        raise SystemExit
print("row-missing")' "$SIRIO_DB" "$1" "${@:2}"
}
# The final `project` table of the scenario, for the artifact.
dump_projects() { # file
  python3 -I -c 'import sqlite3, sys
connection = sqlite3.connect("file:" + sys.argv[1] + "?mode=ro", uri=True, timeout=5)
columns = ["id", "name", "root_path", "color_hex", "display_name", "icon_kind", "icon_value",
           "default_worktree_base", "worktree_location_override", "order_idx"]
lines = ["\t".join(columns)]
for row in connection.execute("SELECT " + ", ".join(columns) + " FROM project ORDER BY order_idx, id"):
    lines.append("\t".join("<NULL>" if value is None else str(value) for value in row))
open(sys.argv[2], "w").write("\n".join(lines) + "\n")' "$SIRIO_DB" "$1"
}
# The field of `project list --json` for the project rooted at ROOT; `row-missing` for none.
listed() { # key root
  ctlq project list --json | python3 -I -c 'import json, os, sys
key, target = sys.argv[1], os.path.realpath(sys.argv[2])
for project in json.load(sys.stdin):
    if os.path.realpath(project["path"]) == target:
        print(project.get(key, "no-field"))
        raise SystemExit
print("row-missing")' "$1" "$2"
}
listed_count() { ctlq project list --json | python3 -I -c 'import json, sys; print(len(json.load(sys.stdin)))'; }
# Polls until the stored COLUMNs of the project rooted at ROOT read WANT
# (tab-separated, <NULL> for a NULL); the timeout is the failure.
wait_stored() { # root want column...
  local root=$1 want=$2 tries=0 got=""
  shift 2
  while [ "$tries" -lt "$POLL_TRIES" ]; do
    got="$(stored "$root" "$@" 2>/dev/null || echo error)"
    if [ "$got" = "$want" ]; then
      note "OK: stored ($*) = $(printf '%s' "$got" | tr '\t' '|')"
      return 0
    fi
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$SCENARIO: the stored ($*) of $root is '$(printf '%s' "$got" | tr '\t' '|')', not '$(printf '%s' "$want" | tr '\t' '|')'"
}
# Polls until the read verb's KEY is not empty; STATE keeps that read.
wait_nonempty() { # key
  local deadline
  deadline=$(( $(date +%s) + POLL_TRIES / 5 ))
  while [ "$(date +%s)" -lt "$deadline" ]; do
    STATE="$(ctlq surface project-identity read 2>&1)" || { STATE="(no answer) $STATE"; sleep 0.2; continue; }
    if [ -n "$(state_value "$1")" ]; then
      note "OK: $1='$(state_value "$1")'"
      return 0
    fi
    sleep 0.1
  done
  fail "$SCENARIO: $1 stayed empty -- last read: $(last_read)"
}
# Polls until a file appears (the mkdir hold's marker, the folder it was holding).
wait_file() { # path
  local tries=0
  while [ "$tries" -lt "$POLL_TRIES" ]; do
    ! [ -e "$1" ] || { note "OK: $1 exists"; return 0; }
    tries=$((tries + 1))
    sleep 0.1
  done
  fail "$SCENARIO: $1 never appeared"
}
# Polls until a log line (fixed text) appears in this scenario's git-calls.log.
wait_log() { # text
  local tries=0
  while [ "$tries" -lt "$POLL_TRIES" ]; do
    ! grep -Fq -- "$1" "$SD/git-calls.log" || { note "OK: git-calls.log has '$1'"; return 0; }
    tries=$((tries + 1))
    sleep 0.1
  done
  fail "$SCENARIO: git-calls.log never had '$1' -- tail: $(tail -n 5 "$SD/git-calls.log" | tr '\n' '|')"
}
tab_count() { ctlq surface tabs read --json | python3 -I -c 'import json, sys; print(json.load(sys.stdin)[0]["count"])'; }
# Polls until `project list` has the project; the completion signal of an add.
wait_listed() { # root
  local tries=0 got=""
  while [ "$tries" -lt "$POLL_TRIES" ]; do
    got="$(listed id "$1" 2>/dev/null || true)"
    if [ -n "$got" ] && [ "$got" != row-missing ]; then
      note "OK: listed $1 (id $got)"
      return 0
    fi
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$SCENARIO: $1 never appeared in project list"
}
# Polls until the two projects' stored COLUMNs are equal (and both rows exist).
wait_same_rows() { # rootA rootB column...
  local a=$1 b=$2 tries=0 row_a="" row_b=""
  shift 2
  while [ "$tries" -lt "$POLL_TRIES" ]; do
    row_a="$(stored "$a" "$@" 2>/dev/null || echo error)"
    row_b="$(stored "$b" "$@" 2>/dev/null || echo error)"
    if [ "$row_a" != row-missing ] && [ "$row_a" != error ] && [ "$row_a" = "$row_b" ]; then
      note "OK: same row ($*): $(printf '%s' "$row_a" | tr '\t' '|')"
      return 0
    fi
    tries=$((tries + 1))
    sleep 0.2
  done
  fail "$SCENARIO: the stored rows of $a and $b differ in ($*): '$(printf '%s' "$row_a" | tr '\t' '|')' vs '$(printf '%s' "$row_b" | tr '\t' '|')'"
}

# ---------------------------------------------------------------- the display

# The window of this app, matched by PID, for captures and clicks.
find_window() {
  DISPLAY_TARGET="$DISPLAY_TARGET" APP_PID="$APP_PID" python3 -I - <<'PY'
import os, re, subprocess
display = os.environ["DISPLAY_TARGET"]
want = int(os.environ["APP_PID"])
listing = subprocess.run(["xwininfo", "-display", display, "-root", "-children"],
                         capture_output=True, text=True, timeout=5).stdout
best = None
for line in listing.splitlines():
    match = re.match(r"\s+(0x[0-9a-fA-F]+).*?\s(\d+)x(\d+)\+", line)
    if not match:
        continue
    window, width, height = match.group(1), int(match.group(2)), int(match.group(3))
    prop = subprocess.run(["xprop", "-display", display, "-id", window, "_NET_WM_PID"],
                          capture_output=True, text=True, timeout=5).stdout
    pid = re.search(r"=\s*(\d+)\s*$", prop)
    if pid and int(pid.group(1)) == want and (best is None or width * height > best[0]):
        best = (width * height, window)
if best:
    print(best[1])
PY
}
# frames/<scenario>-<name>.png from this app's window, once it has drawn the
# state just read. Skipped with --state-only.
capture() { # name
  [ "$STATE_ONLY" -eq 0 ] || return 0
  sleep 2
  local window="" colours size _ file="$OUT_DIR/frames/$SCENARIO-$1.png"
  for _ in $(seq 1 50); do
    window="$(find_window)"
    [ -n "$window" ] && break
    sleep 0.2
  done
  [ -n "$window" ] || fail "$SCENARIO: no window with _NET_WM_PID=$APP_PID for $1"
  with_timeout 20 import -display "$DISPLAY_TARGET" -window "$window" "$file" || fail "$SCENARIO: capture of $1 failed"
  [ -s "$file" ] || fail "$SCENARIO: $1.png is empty"
  colours="$(identify -format '%k' "$file")"
  [ "$colours" -ge 200 ] || fail "$SCENARIO: $1 is blank ($colours colours)"
  size="$(xwininfo -display "$DISPLAY_TARGET" -id "$window" | awk '/Width:/ {w = $2} /Height:/ {h = $2} END {print w "x" h}')"
  [ "$(identify -format '%wx%h' "$file")" = "$size" ] \
    || fail "$SCENARIO: $1.png is $(identify -format '%wx%h' "$file"), the window is $size"
  note "FRAME: $SCENARIO-$1.png ($colours colours, $size; pid $APP_PID)"
}
# Two captures of one switch must differ: byte-identical frames mean the switch
# drew nothing the capture can see. Skipped with --state-only.
must_differ() { # label capture capture
  [ "$STATE_ONLY" -eq 0 ] || return 0
  if cmp -s "$OUT_DIR/frames/$SCENARIO-$2.png" "$OUT_DIR/frames/$SCENARIO-$3.png"; then
    fail "$SCENARIO: $1 captures are identical ($2 and $3): the switch drew nothing the capture can see"
  fi
  note "DIFFERS: $1 ($2 and $3)"
}
# A click in the app's window at (X, Y), counted from the window's corner: the
# read verb's `fieldBounds` and `pathBounds` are in these window pixels
# (GPUI_X11_SCALE_FACTOR=1 makes a pixel an X pixel).
click_at() { # x y
  local window
  window="$(find_window)"
  [ -n "$window" ] || fail "$SCENARIO: no window to click in"
  DISPLAY="$DISPLAY_TARGET" xdotool mousemove --window "$window" "$1" "$2" click 1
}
# Real keys and clicks go to the focused window, and Xvfb has no window
# manager to focus one, so the app's window is focused first.
focus_app() {
  local window
  window="$(find_window)"
  [ -n "$window" ] || fail "$SCENARIO: no window to focus"
  DISPLAY="$DISPLAY_TARGET" with_timeout 10 xdotool windowfocus --sync "$window"
}
xkey() { # xdotool key names...
  focus_app
  DISPLAY="$DISPLAY_TARGET" xdotool key "$@"
}
xtype() { # text
  focus_app
  DISPLAY="$DISPLAY_TARGET" xdotool type --delay 60 -- "$1"
}
# The centre of a `x,y,w,h` bounds value (window pixels).
bounds_center() { python3 -I -c 'import sys
x, y, w, h = [float(v) for v in sys.argv[1].split(",")]
print(int(x + w / 2), int(y + h / 2))' "$1"; }
# Clicks the centre of the bounds the read verb reports under KEY.
click_bounds() { # key
  local bounds point
  read_state
  bounds="$(state_value "$1")"
  [ -n "$bounds" ] || fail "$SCENARIO: the read verb has no $1 -- read: $(last_read)"
  point="$(bounds_center "$bounds")"
  focus_app
  click_at ${point% *} ${point#* }
}
double_click_bounds() { # key
  local bounds point window
  read_state
  bounds="$(state_value "$1")"
  [ -n "$bounds" ] || fail "$SCENARIO: the read verb has no $1 -- read: $(last_read)"
  point="$(bounds_center "$bounds")"
  focus_app
  window="$(find_window)"
  DISPLAY="$DISPLAY_TARGET" xdotool mousemove --window "$window" ${point% *} ${point#* } click --repeat 2 --delay 80 1
}
# The CLIPBOARD selection of this display, read through GTK (python3-gi). The
# clean environment pinned to X11 keeps a caller's WAYLAND_DISPLAY or
# GDK_BACKEND from steering GTK to the desktop's clipboard.
x_clipboard() { # get
  with_timeout 10 env -i PATH=/usr/bin:/bin HOME="$SD/home" GDK_BACKEND=x11 DISPLAY="$DISPLAY_TARGET" \
    python3 -I - "$@" <<'PY'
import sys
import gi
gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gdk, Gtk
text = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD).wait_for_text()
sys.stdout.write("" if text is None else text)
PY
}
expect_clipboard() { # desc want
  local got="" _
  for _ in $(seq 1 15); do
    got="$(x_clipboard get)"
    if [ "$got" = "$2" ]; then
      note "OK: $1 (clipboard '$got')"
      return 0
    fi
    sleep 0.3
  done
  fail "$SCENARIO: $1: the clipboard holds '$got', not '$2'"
}
# `xset r off` for this run's display only, undone by the cleanup trap.
keyrepeat_off() {
  command -v xset >/dev/null || fail "$SCENARIO: xset is required to turn key repeat off"
  DISPLAY="$DISPLAY_TARGET" xset r off
  KEYREPEAT_CHANGED=1
}
# Records and announces a step that needs the display, under --state-only.
# Use: `if ! skip_display "step 4 ..."; then ...; fi`
skip_display() { # label
  if [ "$STATE_ONLY" -eq 1 ]; then
    note "SKIPPED (state-only): $1"
    SKIPPED+=("$SCENARIO: $1")
    return 0
  fi
  return 1
}

# ---------------------------------------------------------------- fixtures

fixture_git() {
  env -u GIT_DIR -u GIT_WORK_TREE -u GIT_INDEX_FILE GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \
    "$REAL_GIT" "$@"
}
# A git repository with one commit: what a project needs.
mkrepo() { # dir
  mkdir -p "$1"
  fixture_git -C "$1" init -q -b main
  fixture_git -C "$1" config user.email tester@example.invalid
  fixture_git -C "$1" config user.name Tester
  printf '# %s\n' "$(basename "$1")" >"$1/README.md"
  fixture_git -C "$1" add -A
  fixture_git -C "$1" commit -q -m first
}
# A bare repository with one commit, for a clone.
mkbare() { # dir
  mkrepo "$1.src"
  fixture_git clone -q --bare "$1.src" "$1"
}
# The path verb: starts an add the way the sidebar does; it replies before the add finishes.
add_path() { # path [extra surface project-add args...]
  ctl surface project-add --kind path --value "$1" "${@:2}" >/dev/null
}
# The Clone or Create form of the sidebar, started the way its menu starts it.
add_form() { # clone|create value [--submit]
  ctl surface project-add --kind "$1" --value "$2" "${@:3}" >/dev/null
}
act() { ctl surface project-identity act "$@" >/dev/null; }

# ---------------------------------------------------------------- a scenario's app

# The git wrapper (spec file 18): into the debug build's own copy directory, so
# Sirio's git calls reach it first. Written once per scenario, before the launch.
install_git_wrapper() {
  local bin="$XDG_DATA_HOME/Sirio/bin-debug"
  case "$SD$REAL_GIT" in *\'*) fail "$SCENARIO: a single quote in $SD or $REAL_GIT" ;; esac
  mkdir -p "$bin"
  chmod 755 "$bin"
  python3 -I -c 'import os, stat, sys
info = os.lstat(sys.argv[1])
ok = (stat.S_ISDIR(info.st_mode) and info.st_uid == os.getuid()
      and stat.S_IMODE(info.st_mode) == 0o755)
sys.exit(0 if ok else 1)' "$bin" \
    || fail "$SCENARIO: $bin must be a real directory, owned by this user, mode 755 (the app refuses it as a PATH prefix otherwise)"
  cat >"$bin/git" <<WRAPPER
#!/bin/sh
SD='$SD'
REAL='$REAL_GIT'
PATH=/usr/bin:/bin:\$PATH
printf 'git %s  cwd=%s\n' "\$*" "\$PWD" >>"\$SD/git-calls.log"
if [ "\$1" = clone ] && [ -f "\$SD/flood-clone" ]; then
  # 11 MiB of progress lines: more than the 10 MiB output cap. Not
  # "Receiving objects" lines: Sirio turns each of those into a progress update.
  yes 'Counting objects:  42% (42/100), 1.00 MiB | 1.00 MiB/s' | head -c 11534336 >&2
fi
if [ "\$1" = clone ] && [ -f "\$SD/slow-clone" ]; then
  sleep 20
fi
if [ "\$1" = clone ] && [ -f "\$SD/gate-clone" ]; then
  # Holds the clone until the scenario releases it (or its work dir is gone). A
  # progress line every 5 s keeps Sirio's 60 s clone stall budget
  # (CLONE_STALL_TIMEOUT in rust/crates/sirio_git/src/clone.rs, restarted by every
  # stderr line) from killing a held clone however long a scenario's steps take on
  # a slow runner; the cap is only a backstop for a scenario that died without
  # releasing it.
  n=0
  while [ -d "\$SD" ] && [ ! -f "\$SD/clone-go" ] && [ "\$n" -lt 3000 ]; do
    sleep 0.1
    n=\$((n + 1))
    [ \$((n % 50)) -ne 0 ] || echo 'Receiving objects:   0% (0/1)' >&2
  done
fi
if [ "\$1" = clone ] && [ -f "\$SD/fail-clone" ]; then
  # A half-made checkout, the way a git that dies mid-clone leaves one.
  for last; do :; done
  mkdir -p "\$last/.git"
  echo "Cloning into 'lib'..." >&2
  echo 'fatal: the E2E wrapper failed this clone' >&2
  echo 'Please make sure you have the correct access rights' >&2
  exit 128
fi
if [ "\$1" = worktree ] && [ "\$2" = list ] && [ -f "\$SD/slow-discovery" ]; then
  echo SLOW-DISCOVERY-START >>"\$SD/git-calls.log"
  sleep 3
  echo SLOW-DISCOVERY-END >>"\$SD/git-calls.log"
fi
exec "\$REAL" "\$@"
WRAPPER
  chmod 755 "$bin/git"
}

# The mkdir hold's LD_PRELOAD shim (see the header), compiled once per run.
build_mkdir_hold() {
  [ ! -f "$WORK/mkdir-hold.so" ] || return 0
  command -v cc >/dev/null || fail "$SCENARIO: cc is required for the mkdir hold"
  cat >"$WORK/mkdir-hold.c" <<'SHIM'
/* The E2E mkdir hold: see the header of test-project-identity-e2e.sh. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>

static char hold_path[PATH_MAX];
static char work_dir[PATH_MAX];
static char held_marker[PATH_MAX];
static char go_marker[PATH_MAX];
static int active;

/* Read once at load: getenv from the worker thread could race a setenv. */
__attribute__((constructor)) static void mkdir_hold_init(void) {
    const char *hold = getenv("SIRIO_E2E_MKDIR_HOLD");
    const char *dir = getenv("SIRIO_E2E_MKDIR_DIR");
    char exe[PATH_MAX];
    ssize_t n = readlink("/proc/self/exe", exe, sizeof exe - 1);
    if (!hold || !dir || n <= 0) return;
    exe[n] = '\0';
    const char *base = strrchr(exe, '/');
    if (strcmp(base ? base + 1 : exe, "sirio") != 0) return;
    snprintf(hold_path, sizeof hold_path, "%s", hold);
    snprintf(work_dir, sizeof work_dir, "%s", dir);
    snprintf(held_marker, sizeof held_marker, "%s/mkdir-held", dir);
    snprintf(go_marker, sizeof go_marker, "%s/mkdir-go", dir);
    active = 1;
}

int mkdir(const char *path, mode_t mode) {
    static int (*real_mkdir)(const char *, mode_t);
    if (!real_mkdir) real_mkdir = (int (*)(const char *, mode_t))dlsym(RTLD_NEXT, "mkdir");
    if (active && strcmp(path, hold_path) == 0) {
        FILE *marker = fopen(held_marker, "w");
        if (marker) fclose(marker);
        for (int i = 0; i < 3000 && access(go_marker, F_OK) != 0 && access(work_dir, F_OK) == 0; i++)
            usleep(100000);
    }
    return real_mkdir(path, mode);
}
SHIM
  cc -shared -fPIC -O2 -o "$WORK/mkdir-hold.so" "$WORK/mkdir-hold.c" -ldl \
    || fail "$SCENARIO: the mkdir hold did not compile"
}
# Launches the scenario's app with its mkdir of PATH held.
hold_mkdir_of() { # path
  # LD_PRELOAD splits its list on spaces and colons.
  case "$WORK" in *[[:space:]:]*) fail "$SCENARIO: the mkdir hold needs a work dir without spaces or colons: $WORK (set SIRIO_E2E_TMP)" ;; esac
  build_mkdir_hold
  APP_EXTRA_ENV=("LD_PRELOAD=$WORK/mkdir-hold.so" "SIRIO_E2E_MKDIR_HOLD=$1" "SIRIO_E2E_MKDIR_DIR=$SD")
}
# The Create form's default parent is SIRIO_PROJECTS_DIR, which app_start sets to $SD/projects.
prelaunch_create_replaced_by_clone() { hold_mkdir_of "$SD/projects/made"; }
prelaunch_create_replaced_fails() { hold_mkdir_of "$SD/projects/made"; }

app_ready() { ctlq surface tabs read >/dev/null 2>&1; }
app_start() {
  local n=0
  rm -f "$SIRIO_SOCKET"
  WENV=(
    "HOME=$SD/home" "PATH=$WORK/stubs:/usr/bin:/bin" "SHELL=$WORK/shell/shell" "TMPDIR=$SD/tmp" "LANG=C.UTF-8"
    "XDG_CONFIG_HOME=$SD/xdg-config" "XDG_DATA_HOME=$XDG_DATA_HOME" "XDG_STATE_HOME=$SD/xdg-state"
    "XDG_CACHE_HOME=$SD/xdg-cache" "XDG_RUNTIME_DIR=$SD/xdg-runtime"
    "SIRIO_DB=$SIRIO_DB" "SIRIO_SOCKET=$SIRIO_SOCKET" "SIRIO_CREDENTIALS=$SD/credentials.json"
    "SIRIO_HOST_HOME=$SIRIO_HOST_HOME" "SIRIO_PROJECTS_DIR=$SD/projects"
  )
  if [ "$OS" = linux ]; then
    WENV+=("DISPLAY=$DISPLAY_TARGET" "GPUI_X11_SCALE_FACTOR=1")
    WENV+=(${VULKAN_ENV[@]+"${VULKAN_ENV[@]}"})
  fi
  # `exec` keeps the subshell's PID: $! is the app, which the captures match.
  (cd "$SD/cwd" && exec env -i "${WENV[@]}" ${APP_EXTRA_ENV[@]+"${APP_EXTRA_ENV[@]}"} "$BIN" >>"$SD/app.log" 2>&1) &
  APP_PID=$!
  while [ ! -S "$SIRIO_SOCKET" ] && [ "$n" -lt 300 ]; do n=$((n + 1)); sleep 0.1; done
  [ -S "$SIRIO_SOCKET" ] || fail "$SCENARIO: sirio never opened its control socket (see $SD/app.log)"
  n=0
  while ! app_ready && [ "$n" -lt 600 ]; do n=$((n + 1)); sleep 0.1; done
  app_ready || fail "$SCENARIO: sirio's control actions never became ready"
  note "app up (pid $APP_PID)"
}
app_quit() {
  [ -n "$APP_PID" ] || return 0
  ctlq quit >/dev/null 2>&1 || true
  local n=0
  while alive "$APP_PID" && [ "$n" -lt 200 ]; do n=$((n + 1)); sleep 0.1; done
  alive "$APP_PID" && fail "$SCENARIO: sirio $APP_PID did not quit after sirioctl quit"
  wait "$APP_PID" 2>/dev/null || true
  APP_PID=""
}
host_shutdown() { SIRIO_HOST_HOME="$SIRIO_HOST_HOME" "$PROBE" shutdown --force >/dev/null 2>&1 || true; }

# A fresh world and a fresh app: private HOME and XDG dirs, a database seeded
# with the appearance, a projects dir, a host root, the git wrapper, stub CLIs.
begin_scenario() { # name
  SCENARIO="$1"
  SD="$WORK/$1"
  rm -rf "$SD"
  mkdir -p "$SD/home" "$SD/tmp" "$SD/cwd" "$SD/projects" "$SD/host" "$SD/xdg" \
    "$SD/xdg-config" "$SD/xdg-state" "$SD/xdg-cache" "$SD/xdg-runtime"
  chmod 700 "$SD/xdg-runtime"
  : >"$SD/git-calls.log"
  XDG_DATA_HOME="$SD/xdg"
  SIRIO_SOCKET="$SD/c.sock"
  SIRIO_DB="$SD/session.sqlite"
  SIRIO_HOST_HOME="$SD/host"
  APP_EXTRA_ENV=()
  [ "${#SIRIO_SOCKET}" -lt 100 ] || fail "$SCENARIO: the socket path is too long for a unix socket: $SIRIO_SOCKET (set SIRIO_E2E_TMP to a shorter directory)"
  [ "${#SIRIO_HOST_HOME}" -lt 80 ] || fail "$SCENARIO: the host root is too long for the host's socket: $SIRIO_HOST_HOME (set SIRIO_E2E_TMP to a shorter directory)"
  "$SEED" --database "$SIRIO_DB" --appearance "$APPEARANCE"
  install_git_wrapper
  # A scenario that needs the app launched with something extra says so here.
  if declare -F "prelaunch_$1" >/dev/null; then "prelaunch_$1"; fi
  app_start
  [ "$(listed_count)" = 0 ] || fail "$SCENARIO: the app started with $(listed_count) project(s), not 0"
}
end_scenario() {
  app_quit
  host_shutdown
  # A slow clone's wrapper outlives the app for as long as its sleep.
  local n=0
  while [ -n "$(pgrep -f "$SD/" 2>/dev/null || true)" ] && [ "$n" -lt 300 ]; do n=$((n + 1)); sleep 0.1; done
  collect_scenario
  SCENARIOS_PASSED=$((SCENARIOS_PASSED + 1))
  note "scenario passed"
}

# ---------------------------------------------------------------- scenarios

# 2. Skip leaves the same row as a socket add with no label.
scenario_skip_matches_socket() {
  local a="$SD/skip/a" b="$SD/skip/b" c="$SD/skip/c" id_a out
  mkrepo "$a"; mkrepo "$b"; mkrepo "$c"

  note "step 1: add skip/a with the path verb, then skip"
  add_path "$a"
  wait_listed "$a"
  id_a="$(listed id "$a")"
  wait_for open=true "projectId=$id_a"
  act skip
  wait_for open=false lastOutcome=skipped

  note "step 2: add skip/b through sirioctl project add, without a label"
  out="$(ctl project add "$b" --json)"
  printf '%s' "$out" | python3 -I -c 'import json, sys
reply = json.load(sys.stdin)[0]
sys.exit(0 if reply.get("added") == "true" else 1)' || fail "$SCENARIO: sirioctl project add did not add skip/b: $out"
  wait_listed "$b"

  note "step 3: the two rows hold the same color_hex, display_name, icon_kind and icon_value"
  wait_same_rows "$a" "$b" color_hex display_name icon_kind icon_value

  note "step 4: a skip over a focused terminal gives focus back to it"
  if ! skip_display "step 4: open a terminal, add skip/c with the path verb, skip, focus=terminal"; then
    ctl select-workspace --workspace "$a" >/dev/null
    # The debug verb `surface terminal open` opens a tab without focusing it,
    # so this step uses the user's own shortcut, Ctrl+T.
    xkey ctrl+t
    wait_for focus=terminal
    add_path "$c"
    wait_listed "$c"
    wait_for open=true "projectId=$(listed id "$c")"
    # Focus left the terminal for the dialog, so the focus=terminal after the
    # skip is a hand-back, not a terminal that never lost it.
    ! state_has focus=terminal || fail "$SCENARIO: focus stayed on the terminal while the dialog was open -- read: $(last_read)"
    act skip
    wait_for open=false lastOutcome=skipped focus=terminal
    hold_for "$HOLD_SECS" open=false focus=terminal
  fi
}

# 13. A second add waits behind the open dialog and opens when the first is skipped.
scenario_queue() {
  local one="$SD/queue/one" two="$SD/queue/two" id_one id_two
  mkrepo "$one"; mkrepo "$two"

  note "step 1: the path verb on both, back to back"
  add_path "$one"
  add_path "$two"
  wait_listed "$one"
  wait_listed "$two"
  id_one="$(listed id "$one")"
  id_two="$(listed id "$two")"
  [ "$id_one" != "$id_two" ] || fail "$SCENARIO: both projects have the id $id_one"
  wait_for open=true queue=1 "projectId=$id_one"

  note "step 2: skip shows the second project's dialog"
  act skip
  wait_for open=true queue=0 "projectId=$id_two"
  act skip
  wait_for open=false queue=0 lastOutcome=skipped
}

# 14. A tracked path opens no dialog, says why in a toast, and keeps its label.
scenario_duplicate() {
  local app="$SD/dup/app" toast
  mkrepo "$app"
  toast="already tracked or nested: $app"

  note "step 1: sirioctl project add --display-name server opens no dialog"
  ctl project add "$app" --display-name server >/dev/null
  wait_listed "$app"
  [ "$(listed displayName "$app")" = server ] || fail "$SCENARIO: displayName is '$(listed displayName "$app")', not server"
  hold_for "$HOLD_SECS" open=false queue=0 lastOutcome=

  note "step 2: the path verb on the same path: no dialog, and the toast says why"
  add_path "$app"
  wait_for open=false "toast=$toast"
  hold_for "$HOLD_SECS" open=false queue=0 lastOutcome=

  note "step 3: the label is untouched"
  [ "$(listed displayName "$app")" = server ] || fail "$SCENARIO: displayName is '$(listed displayName "$app")', not server"
  [ "$(listed_count)" = 1 ] || fail "$SCENARIO: $(listed_count) projects are listed, not 1"
  note "OK: displayName=server, one project"
}

# ---------------------------------------------------------------- the run

is_unwritten() { # name
  local entry
  for entry in ${UNWRITTEN[@]+"${UNWRITTEN[@]}"}; do
    [ "$entry" = "$1" ] && return 0
  done
  return 1
}

is_linux_only() { # name
  local entry
  for entry in "${LINUX_ONLY[@]}"; do
    [ "$entry" = "$1" ] && return 0
  done
  return 1
}

for entry in "${SCENARIO_KINDS[@]}"; do
  name="${entry%%:*}"
  kind="${entry##*:}"
  if [ -n "$ONLY" ]; then
    case "$ONLY" in *",$name,"*) ;; *) continue ;; esac
  fi
  SCENARIO="$name"
  # An unwritten scenario fails before the --state-only skip can hide it.
  is_unwritten "$name" && fail "scenario $name is not written yet"
  if is_linux_only "$name" && [ "$OS" != linux ]; then
    echo "[$name] SKIPPED (not Linux): the mkdir hold needs LD_PRELOAD"
    SKIPPED+=("$name: the whole scenario (not Linux)")
    continue
  fi
  if [ "$kind" = d ] && [ "$STATE_ONLY" -eq 1 ]; then
    echo "[$name] SKIPPED (state-only): a display scenario"
    SKIPPED+=("$name: the whole scenario")
    continue
  fi
  echo "=== scenario $name"
  begin_scenario "$name"
  "scenario_$name"
  end_scenario
done

# Teardown: nothing of this run may outlive it.
SCENARIO="teardown"
app_quit
stop_hosts
if [ "$KEYREPEAT_CHANGED" -eq 1 ]; then DISPLAY="$DISPLAY_TARGET" xset r on >/dev/null 2>&1 || true; KEYREPEAT_CHANGED=0; fi
if [ -n "$XVFB_PID" ]; then terminate "$XVFB_PID"; XVFB_PID=""; fi
leftover=""
for _ in $(seq 1 25); do
  leftover="$(pgrep -f "$WORK" 2>/dev/null || true)"
  [ -z "$leftover" ] && break
  sleep 0.2
done
# A host is detached and its command line names no path of this run: its own state file does.
for file in "$WORK"/*/host/host-v*.json; do
  [ -f "$file" ] || continue
  pid="$(python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1])).get("pid", ""))' "$file" 2>/dev/null || true)"
  [ -z "$pid" ] || ! alive "$pid" || leftover="$leftover $pid"
done
[ -z "$leftover" ] || fail "processes of this run survived teardown: $(echo "$leftover" | tr '\n' ' ')"
echo "OK: no process of this run is left"

if [ "${#SKIPPED[@]}" -gt 0 ]; then
  echo "skipped under --state-only (${#SKIPPED[@]}):"
  for entry in "${SKIPPED[@]}"; do echo "  $entry"; done
fi
if [ -n "$ONLY" ]; then
  echo "PROJECT IDENTITY E2E PARTIAL ($SCENARIOS_PASSED scenarios)"
else
  [ "${#UNWRITTEN[@]}" -eq 0 ] || fail "scenarios not written yet: ${UNWRITTEN[*]}"
  echo "PROJECT IDENTITY E2E OK ($SCENARIOS_PASSED scenarios, appearance $APPEARANCE, state-only $STATE_ONLY)"
fi
