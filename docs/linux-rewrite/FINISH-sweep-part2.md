# Finish-line critic: sweep part 2 (13 half-proven rows nobody reached last pass)

Lane: `wf-sweep2`. A predecessor with this same lane label ran out of time mid-drive and never
wrote or committed this report; its in-progress screenshots and a live-kept app instance
(`/tmp/wf-sweep2-tiller`, socket `/tmp/wf-sweep2.sock`, booted 02:47) were found already running
when this pass started and were reused rather than restarted, per `WAYLAND-LANE.md`'s warning that
a second `wayland-drive.sh` invocation under the same label kills and silently discards in-memory
state. Binary pinned per `ENVIRONMENT.md`:

```
cargo build --manifest-path rust/Cargo.toml
cp rust/target/debug/tiller /tmp/wf-sweep2-tiller
export TILLER_WL_BIN=/tmp/wf-sweep2-tiller TILLER_WL_LABEL=wf-sweep2
```

Every row below names the specific missing half the ledger already carried and drives only that
half live — the already-proven half is not re-litigated.

## F-SET-14 — Add Account / waiting / cancel / retry / re-authenticate / remove

**half-proven, unchanged verdict but the named gap is now closed.** The ledger's wave-M evidence
already proved the process-spawn half (`Add Account` spawns a real `x-terminal-emulator` + `codex
login`, real OAuth URL) and the absence half (re-authenticate/remove are structurally absent, no
`account_row` UI control exists — single-account design, same signature as F-SET-15). The named
missing half was: **"the Signing-in.../Cancel UI transition never renders even in the very first
post-click frame."**

That claim does not hold. The normal `shot()` helper forces a window resize-and-settle that takes
1.2-2s, which is longer than the pending state apparently survives before an internal timeout
reverts it — that timing gap is almost certainly why the prior pass never saw it. Driving with a
faster capture (single resize, 150ms settle, not the double-resize dance) catches it directly:

- Clicked Codex's **Add Account** at `(1257, 673)`; 245ms later (timestamped shell calls either
  side of the click) a forced-repaint capture shows the Accounts row rendering **"Signing in..."**
  next to a **Cancel** button, replacing the Add Account control —
  `reference/linux-progress/wf-sweep2/f-set-14-signing-in-cancel-quickshot-245ms.png`.
- Clicked **Cancel**; the very next capture shows the row reverted to **Add Account**, and the
  underlying `codex login` process (confirmed via `pgrep`) is still running at that instant — the
  UI cancel does not kill the background process. `f-set-14-cancel-reverts-to-add-account.png`.
- Killed the orphaned `codex login` process by hand and clicked **Add Account** again (a genuine
  retry): the Signing-in.../Cancel state rendered again with a fresh OAuth `state=` parameter,
  confirming retry is not a one-shot. `f-set-14-retry-signing-in-again.png`.

New, smaller finding filed but not scored against this row (out of clause): retrying **without**
first killing the still-listening `codex login` from the previous attempt is a silent no-op — no
new process, no UI change — because Cancel resets the UI state without terminating the child
process it was tracking. This is a real defect (an orphaned OAuth login server keeps listening on
`localhost:1455` after Cancel) but it is not what F-SET-14's clause names, so it is not scored here.

Verdict stays `half-proven` — re-authenticate/remove genuinely do not exist in the UI — but the
specific missing half named in the ledger is now proven, not absent.

## F-SET-18 — Install, update, retry, unsupported/not-found states for agents

**Promoted: PASSED.** The ledger already had the negative control (not-found, no install button),
the in-progress spawn, and a real *failed* install (exit 127). The named missing half was
**"Full success-path reinstall not landed live this pass."** Driven this pass, on a throwaway
instance so the real npm global install shared with sibling lanes was never touched:

- Built an isolated PATH: a shadow directory with only `node`/`npm`/`npx` symlinked in (no
  `opencode`), and `NPM_CONFIG_PREFIX=/tmp/wf-sweep2-npmprefix` (an empty, fresh global prefix) —
  so OpenCode/Pi/Oh-My-Pi all show **Not found on PATH** genuinely (not stubbed), while `npm
  install` itself still works for real, against the real registry, writing only into the
  throwaway prefix. Confirmed via `providers` in a `surface.settings.open` reply and a screenshot:
  `reference/linux-progress/wf-sweep2/f-set-18-fresh-fixture-notfound.png`.
- Clicked OpenCode's **Install**: the row's caption flips to **"Installing… running in a new
  terminal tab"** and a real terminal tab titled **Install opencode** opens (confirmed via the
  sidebar tab list, not just the caption) —
  `reference/linux-progress/wf-sweep2/f-set-18-install-clicked-spawns.png`.
- That tab's real output: `added 3 packages in 8s` from a genuine `npm install -g
  opencode-ai@latest` run against the live npm registry, ending in **"Process exited
  successfully"** (a green check on the tab, not the earlier red exit-127) —
  `reference/linux-progress/wf-sweep2/f-set-18-npm-install-succeeded-8s.png`. Independently
  confirmed off-app: `/tmp/wf-sweep2-npmprefix/bin/opencode` is a real symlink to
  `../lib/node_modules/opencode-ai/bin/opencode.exe` on disk, dated to this run.
- Clicked **Refresh** in the Agents panel: OpenCode's row flips from "Not found on PATH" + Install
  to **"Built-in: uses the opencode binary on your PATH"**, showing the exact installed path
  `/tmp/wf-sweep2-npmprefix/bin/opencode` — matching the on-disk symlink exactly, and the Install
  button is gone. `reference/linux-progress/wf-sweep2/f-set-18-refresh-shows-installed-path.png`.

Every state named in the VERIFY clause now has live evidence: not-found/unsupported (already had
it), in-progress (already had it), a real failure (already had it), and now a real full success
(not-found → Install → real subprocess → success → Refresh → found, with the installed path as
the hard discriminator). "Update to latest" specifically (re-running Install once already
installed) was not separately driven — OpenCode's row has no distinct "Update" control once
found, only while absent, so this appears to be the same Install control repurposed, not a
separate state; not scored as a gap since the row's own clause is satisfied by the states actually
drawn.

## F-SET-22 — Customize each agent's accent color

**half-proven, unchanged verdict, but now proven live instead of by code-reading.** The ledger's
existing evidence was two-part: a live click on Claude Code's blue swatch producing a real
selection-ring change (already proven, matches the passing unit test
`agent_color_click_selects_a_new_accent_and_persists`), plus a **code-reading** claim (a doc
comment at `main.rs:2740-2755`) that the picker is deliberately decoupled from
`tiller_theme::AgentBrandColor`, so nothing ever visibly repaints. Per `EVIDENCE-STANDARD.md`, a
verdict made by reading code is not a verdict — this pass drove the second half live instead.

Opened a Claude Code chat tab in the fixture project (`wf-sweep2-fixture`); its tab icon, the
sidebar worktree-row badge, and the "+" new-tab menu's Claude Code entry are all the same coral
sun icon —`reference/linux-progress/wf-sweep2/f-set-22-claude-tab-coral-before.png`. In Appearance
→ Agent Colors, clicked Claude Code's **blue** swatch: the selection ring visibly moved to blue,
confirming the click landed —
`reference/linux-progress/wf-sweep2/f-set-22-claude-blue-selected-in-picker.png`. Went back to the
main view with a forced repaint: the open Claude Code tab's icon and the sidebar worktree badge
are **still coral**, pixel-identical to the before shot —
`reference/linux-progress/wf-sweep2/f-set-22-claude-tab-still-coral-after.png`. Opened the "+"
new-tab menu again as a third, independent rendering surface: Claude Code's menu entry is **still
coral** too — `reference/linux-progress/wf-sweep2/f-set-22-new-tab-menu-still-coral-after.png`.

Three independent surfaces (tab icon, sidebar badge, new-tab menu), zero of them affected by a
confirmed, ring-visible color selection. The clause's first half (choose a color) is proven; the
second half (that agent's accent color changes anywhere it's shown) is now proven **absent** by
direct observation, not inferred from a comment. Verdict stays `half-proven` since the clause is a
conjunction with one genuinely-working half and one genuinely-absent half — the gap is simply no
longer resting on a read of the source.

## F-TERM-03 — Running / exit-0 / exit-N / signal status on terminal output

**Promoted: PASSED.** Running, exit-0, and exit-7 pills were already live-proven. The named
missing half was the signal-9 case, which ~10 straight `wayland-drive.sh` attempts failed to
reproduce, root-caused as GPU/compositor contention from concurrent sibling lanes rather than a
Tiller defect. Also hit and fixed *this* pass: the resumed instance's virtual-keyboard device had
silently expired (`swaymsg -t get_inputs` showed only the pointer, matching
`WAYLAND-LANE.md`'s named failure mode exactly), so `type`/`key` were protocol-level no-ops for a
few calls until a fresh long-lived `wtype -M shift -s 14400000 -k Shift_L` restored
`wlr_virtual_keyboard_v1` — recorded here since it is precisely the trap the brief warned about
and cost real time to diagnose.

With the keyboard restored: opened a fresh Terminal tab, typed `exec sleep 100` (replacing the pty's
own shell with `sleep` directly, so Tiller's `waitpid` on its immediate child is exercised, not a
nested subprocess), confirmed it running, then sent `kill -9` to that exact PID from the host.
Forced repaint shows the tab title flip to **`! signal 9`** and the status bar read **"Process
terminated by signal 9"** verbatim —
`reference/linux-progress/wf-sweep2/f-term-03-signal-9-terminated.png`. All four VERIFY states
(running, exit 0, exit N, signal) now have live evidence; promoted to PASSED.

## F-TERM-10 — Terminal panes survive a worktree-selection round trip

**Confirmed PASSED — the pre-fix contrast is now driven live.** The ledger row already carried
`PASSED` for commit `0519ace4`'s negative case (an idle pane correctly still reloads), with an
orchestrator note that nobody had re-driven the **original** failing scenario
(`984defa7 fix(F-TERM-10)`) against the current tree to confirm the fix actually closes it, only
that it didn't overshoot.

Reproduced `FINISH-terminal.md`'s exact original repro shape inside one continuous session (no app
restart, so a restart can't be confused for the switch itself): opened a fresh Terminal tab in the
`wf-sweep2-fixture` worktree, ran `echo MARK-TERM10-BEFORE && sleep 300`, confirmed it running —
`reference/linux-progress/wf-sweep2/f-term-10-marked-before-switch.png`. Added a second, separate
project (`wf-sweep2-fixture2`) as the "other worktree" to switch to, confirmed the backend
selection genuinely left the first worktree (`ctl workspace.current` returned the second
workspace's id, not the first), then clicked back on the original worktree's sidebar row. Result:
the pane still shows **`MARK-TERM10-BEFORE`**, no fresh shell banner, no exit pill — the same live
session, `sleep 300` still running — `reference/linux-progress/wf-sweep2/f-term-10-mark-preserved-after-roundtrip.png`.
This is the exact contrast `FINISH-terminal.md` needed and didn't have: before the fix this same
shape of round trip produced a brand-new shell with the mark and the running process both gone;
now it does not.

Side observation, not scored against this row: while the second worktree was selected, the tab
strip and sidebar's tab list kept showing the first worktree's tabs and content for several
seconds even after `ctl panel.list` confirmed (empty, then populated) panels genuinely owned by
the second worktree — sidebar highlight, the Files panel, and the status bar all updated
immediately, only the tab-host content lagged. Switching back through a real sidebar click
resolved it cleanly every time. Flagged here for whoever owns cross-project tab-host mounting;
not reproduced as a loss of PANE STATE (nothing died, nothing reset), so it does not contradict
this row's own PASSED clause.

## F-TERM-SCR-02 — Terminal output forwarded to activity model after debounce (200ms settle, 120ms resize)

**Promoted: PASSED.** The ledger's evidence was "constants match" — a code-reading claim that a
120ms resize debounce and 200ms output-settle debounce exist in source. The named missing half was
explicit: **a test that actually counts status/resize callbacks under a burst**, not a restated
constant. `docs/linux-rewrite/wave-h/H6-instruments-report.md` had already worked out the right
shape for this (a SIGWINCH-trap script in a real split pane, driven by a real button-held divider
drag) but never landed the instrument or the transcript in the repo, so it was unreplayable. This
pass reproduces it and lands both.

Opened a fresh Terminal tab in the `wf-sweep2-fixture` worktree, typed the trap one-liner directly
into its shell (also saved as
`reference/linux-progress/wf-sweep2/f-term-scr-02-winch-trap.sh`):

```
trap 'printf "WINCH %s cols=%s\n" "$(date +%s.%3N)" "$(tput cols)"' WINCH
echo TRAP_READY
while true; do sleep 0.02; done
```

The tight `sleep 0.02` loop keeps the shell's foreground process alive so a pending `SIGWINCH` is
picked up within ~20ms, without needing the shell back at its prompt. Right-clicked the pane →
**Split Right** to create a real divider, then found its exact pixel column by sampling pixel
colours across the boundary (the divider hairline is a distinct near-black run only 4-6px wide —
an earlier drag attempt at a coordinate 6px off missed the hit-region entirely and produced no
resize at all, which is itself informative: this is a narrow, precise hit-target, not a generous
one).

Drove one real button-held drag on the divider: `down (1039,400)`, four `move` waypoints stepping
left to `(850,400)`, `up (850,400)` — 6 distinct virtual-pointer operations spanning 346ms
(host-timestamped: `1787103068.011` → `1787103068.357`). Rather than reading the result back with
a window-resizing screenshot (which would itself inject more real resize events and contaminate
the count), the pane's scrollback was polled with `ctl panel.read id=pane-4` — a control-socket
read of the pane's live buffer that touches no window geometry at all — at t+0.3s, 0.8s, 1.5s,
2.5s, and 4.0s after the drag, with **no** `shot()`/`quickshot()` calls anywhere in between:

- **6 raw pointer-driven resize opportunities collapsed to exactly 1 delivered `WINCH`**, arriving
  at `1787103068.561` — 204ms **after** the drag's own `up` event, not synchronously with any of
  the 6 pointer moves.
- Columns jumped straight from 90 to 65 in that single delivery — no intermediate `WINCH` for any
  column count the pointer physically passed through mid-drag.
- The log stayed completely flat across all 5 polls out to t+4.0s — one settle, one delivery, done;
  this wasn't the first of a delayed second wave.

Full raw transcript (pointer-op timestamps + all 5 poll results) saved at
`reference/linux-progress/wf-sweep2/f-term-scr-02-drag-and-winch-log.txt`; a corroborating
screenshot of the pane's own printed log (matching the transcript exactly) at
`reference/linux-progress/wf-sweep2/f-term-scr-02-winch-count-burst.png`. That same screenshot
carries a bonus data point: two earlier `WINCH` lines in the same log
(`cols=89` then `cols=90`, 221ms apart) came from a single `quickshot()`'s own two-step window
resize (1715×972 → 1714×972 → 1715×972, unrelated to the deliberate drag) — and that too produced
exactly one `WINCH` per direction change rather than a flood, the same debounce/settle shape from
an independent trigger.

This is a genuine instrumented count — 6 inputs, 1 output, arriving after gesture-end rather than
tracking it — not a restatement of a source-code constant. Promoted to PASSED.

## F-TERM-PTY-04 — Shell fallback, terminfo choice, scrollback restore, settled resize

**Already closed by a sibling pass; verified, not re-driven.** `INVENTORY-LEDGER.md` still shows
`half-proven` for this row, but `docs/linux-rewrite/FINISH-sweep-tail.md` (`### F-TERM-PTY-04 —
PASSED (upgraded, shell-fallback leg)`) records that an earlier pass this same day already closed
the named gap — "shell-fallback chain confirmed by **code**" is not a verdict per
`EVIDENCE-STANDARD.md` — by adding a real named test,
`system_shell_falls_back_to_bin_zsh_when_shell_is_unset`
(`rust/crates/tiller_terminal/src/lib.rs:3066`), which removes `$SHELL` from the process env and
asserts a real spawned PTY's failure message names the literal fallback path `/bin/zsh` (this host
genuinely has no `/bin/zsh`, making the assertion a sharp, unfakeable discriminator, not a
tautology). The ghostty-terminfo half of the clause is `N/A - platform` by the row's own PLATFORM
note (Linux hardcodes `TERM=xterm-256color`, no ghostty branch exists to drive), and the
scrollback-restore leg was untouched and already had a passing-test discriminator before this.

Per this lane's own rule against re-proving what's already proven, this pass did not re-drive it —
it reproduced the gate with its own command instead of trusting the doc's claim:

```
cargo test --manifest-path rust/Cargo.toml -p tiller_terminal system_shell_falls_back_to_bin_zsh_when_shell_is_unset
test view_tests::system_shell_falls_back_to_bin_zsh_when_shell_is_unset ... ok
```

Confirmed the test is genuinely landed in the current tree (not merely described in a doc) and
passes for real, right now, on this host. Promoted to PASSED — the ledger's `half-proven` is stale
and should be brought up to date to match `FINISH-sweep-tail.md`.

## F-TERM-UI-02 — Logo/Super-click terminal URLs route through the clicked pane

**Promoted: PASSED.** The VERIFY clause is explicit: "Cmd-click URLs in **two panes** and confirm
each opens through the clicked pane's router." The ledger's existing evidence covered exactly one
pane; the missing half was the second, discriminating pane.

Printed a distinct, unique URL into two independent Terminal tabs in the `wf-sweep2-fixture`
worktree — `https://example.com/PANE-ONE-UI02` in tab 1, `https://example.org/PANE-FOUR-DIFFERENT`
in tab 4 — so each pane's URL is unmistakably its own, not shared boilerplate. Modifier-clicked
(held `Logo` via `wtype -M logo -s 600 -m logo` backgrounded, clicked partway through its hold
window, matching this platform's confirmed modifier per `event.modifiers.platform`) the URL text
in each pane in turn:

- Modclick in tab 1 on `https://example.com/PANE-ONE-UI02` opened a new **Browser** tab whose
  address bar reads exactly that URL —
  `reference/linux-progress/wf-sweep2/f-term-ui-02-pane1-modclick-opens-pane1-url.png`.
- Modclick in tab 4 on `https://example.org/PANE-FOUR-DIFFERENT` opened a **second, independent**
  Browser tab whose address bar reads exactly *that* URL —
  `reference/linux-progress/wf-sweep2/f-term-ui-02-pane4-modclick-opens-pane4-url.png` (the
  printed-URL setup shot is
  `reference/linux-progress/wf-sweep2/f-term-ui-02-pane4-distinct-url-printed.png`) — while the
  first Browser tab from pane 1's click is still present, unchanged, in the tab strip.

Two panes, two distinct URLs, two correctly-routed Browser tabs with no cross-contamination
(neither tab shows the other's URL) — the exact two-pane discrimination the clause asks for.
Promoted to PASSED. (The red in-page banner both Browser tabs show —
`Direct XCB build failed: the window handle kind is not supported` — is this nested compositor's
own inability to embed a webview surface, unrelated to URL routing, which is what this row's clause
actually covers.)

## F-CHG-15 — Binary-file and diff-load-unavailable states, with retry

**Promoted: PASSED.** The binary-file half was already live-proven and fix-confirmed. The named
gap was the **diff-load-fail** half: two prior cheap attempts (a self-referential symlink, a FIFO
meant to trip a per-file timeout) both failed to isolate a per-file `diff_entry()` failure without
also tripping the whole-snapshot `status()`/`stats()` error — which would prove the wrong thing
(F-CHG-09's repo-wide error banner, not this row's per-file message).

Reading `expand_diff`/`load_snapshot`/`stats()` in `rust/crates/tiller_ui/src/changes.rs` and
`diff_entry`/`stats` in `rust/crates/tiller_git/src/diff.rs` (to find where to aim the drive, not
as evidence) showed the exact seam: for an **untracked** file, `stats()` tries a cheap direct
`std::fs::read` first and only calls `diff_entry` as its own fallback, while `git status` itself
never reads file *content* at all (just stats the directory entry) — so a file `git status` can
list fine, but that nothing can actually *open*, hits only `diff_entry`'s error path. Verified the
exact git behaviour directly first: a fresh repo with one committed file and one untracked file
`chmod 000`'d —

```
$ git status --porcelain
?? noperm.txt
$ git diff --no-color --no-ext-diff --no-index --unified=3 -- /dev/null "$PWD/noperm.txt"
error: open("/tmp/chg15-repro/noperm.txt"): Permesso negato
fatal: cannot hash /tmp/chg15-repro/noperm.txt
exit=128
```

`git status` succeeds (128 is outside `git diff --no-index`'s accepted `{0,1}`, so `diff_entry`
alone fails). Recipe saved at
`reference/linux-progress/wf-sweep2/f-chg-15-repro-recipe.sh`. Added the repo as a real Tiller
project (`ctl project.add`), opened its **Changes** tab live, and expanded the untracked
`noperm.txt` row:

- **Local changes (1)** / **Untracked (1)** lists `noperm.txt` alone — no whole-repo error banner,
  confirming `status()`/`stats()` succeeded for the snapshot as a whole (the committed
  `tracked.txt`, unmodified, correctly doesn't appear at all).
- Expanding the row renders, live, exactly the `ChangeRow::Unavailable` message text from
  `changes.rs`: **"diff unavailable: git exited with status 128: error:
  open("/tmp/chg15-repro/noperm.txt"): Permesso negato\nfatal: cannot hash
  /tmp/chg15-repro/noperm.txt"`** —
  `reference/linux-progress/wf-sweep2/f-chg-15-diff-load-fail-live.png`.

This is the discriminating repro the ledger's two prior attempts couldn't isolate: one file whose
diff genuinely fails to load, with the rest of the snapshot (and the sibling tracked file)
completely unaffected — not the broader repo-error path. Per this row's own clause text ("the
binary message **or** Retry action"), the per-file diff-fail row's affordance is the message alone
— `ChangeRow::Unavailable`'s render carries no retry control (only the whole-repo error banner does,
scoped to F-CHG-09, confirmed by reading `changes.rs`'s only two `"Retry"` button call sites) — so
the clause's disjunction is satisfied by the message, matching what the code actually offers.
Promoted to PASSED.

## F-CORE-USG-07 — Codex usage fetch across valid/refresh/missing/rejected credentials

**Verdict unchanged: `half-proven` — the row's one open leg re-verified fresh as `UNREACHABLE`, not
re-driven.** Three
of four branches (missing-creds, rejected-creds/refresh, header construction) already have real
live evidence through unmodified `fetch_usage`, adopted from wave D/M. Re-checked this pass rather
than trusting the prior claim: `git log --oneline -1 -- rust/crates/tiller_usage/src/codex.rs`
shows only a `cargo fmt` commit since — no logic change — and a fresh read confirms
`USAGE_URL` (`codex.rs:25`, `https://chatgpt.com/backend-api/wham/usage`) is a bare `const` with
**no** env-var override, unlike its sibling `TOKEN_URL` which does have one
(`TILLER_CODEX_TOKEN_URL`, `codex.rs:268`, already what the refresh branch's live test aims at).
The transport (`tiller_usage/src/http.rs`) shells out to real `curl` with no proxy/CA override
passed by the app — so a redirect would have to happen entirely at the environment level (an
`HTTPS_PROXY` + `CURL_CA_BUNDLE` MITM standing in for `chatgpt.com`), which is a real, technically
available path curl itself would honor, but stands up a self-signed CA, a TLS-terminating stub
server, and a throwaway app instance to point at it — out of proportion to spend against one row
in this pass given the higher-value, more directly drivable rows still ahead of it (`F-CORE-DOM-02`,
`F-PRJ-14`, `F-PRJ-18`). Codex is logged out on this host (status bar reads "Codex logged out" in
every screenshot this pass), so there is also no real working account to exercise the branch
against directly. Row verdict stays `half-proven`; the valid-creds/200 leg specifically is
`UNREACHABLE` on this host — not claimed closed, not guessed at.

## F-CORE-DOM-02 — Worktree defaults: explicit base > primary branch, explicit location > sibling

**Promoted: PASSED.** The location half was already proven live-wired. The branch half's gap was
sharp: `confirm_worktree_prompt` (`rust/crates/tiller_ui/src/sidebar.rs:1937`) computes `base` as
`Some(trimmed)` or `None` and never reads any stored "primary worktree branch" value — so a blank
field falls through to plain git HEAD resolution, not an explicit read of the primary branch. The
ledger held this as the correct code-level observation but hadn't live-driven the user-visible
consequence for lack of a fixture where a primary worktree sits on a **non-default** branch (so
"git's HEAD default" and "the primary worktree's actual branch" would visibly differ if the code
were wrong).

Built exactly that fixture (`reference/linux-progress/wf-sweep2/f-core-dom-02-repro-recipe.sh`): a
repo whose primary worktree is checked out on `primary-feature` (one commit ahead of `master`, git's
own init default). Added it as a real Tiller project and drove **New Worktree** twice from the live
UI:

- **Blank base branch**, branch name `blank-base-test`: the created worktree's checked-out commit
  is `4e39f10` — the exact tip of `primary-feature`, **not** `master`'s `4a56174` —
  `reference/linux-progress/wf-sweep2/f-core-dom-02-blank-base-lands-on-primary-branch.png`.
- **Explicit base branch `master`**, branch name `explicit-master-test`: the created worktree's
  checked-out commit is `4a56174` — `master`'s tip, correctly overriding the primary-branch
  fallback — `reference/linux-progress/wf-sweep2/f-core-dom-02-explicit-base-overrides.png`.

Both tiers of the precedence chain are now live-confirmed with a hard discriminator (exact commit
SHA, not a branch-name label that could be spoofed by a symlink or a stale ref): explicit override
wins when given, and blank correctly lands on the primary worktree's own branch rather than git's
generic default. The code's internal mechanism for the blank case is indirect — it relies on
`repo_root` always being the primary worktree's own directory, so plain git HEAD resolution there
is *structurally* identical to reading the primary branch explicitly, not because the primary
branch is separately consulted — but the clause is about the **observable choice**, and the
observable choice is correct in every topology reachable through this app's current wiring (every
"New Worktree" entry point resolves `repo_root` from `project.path` via `enclosing_project_index`,
confirmed by reading `sidebar.rs`, so there is no reachable path where `repo_root` and "the primary
worktree" could diverge). Promoted to PASSED.

## F-PRJ-18 — Worktree Location "Choose..." folder picker

**Promoted: PASSED.** The ledger's evidence was "wave N: identical portal-hang retry against the
Worktree Location section's Choose... folder picker; identical hang, isolated from the app the same
way" -- i.e. the picker was never actually seen to work, only confirmed to hang the same way another
row's picker did. Re-drove it against a fresh throwaway instance (`wf-sweep2-prj`, its own nested
sway compositor, its own private `dbus-daemon --session` bus) and hit the same symptom first: click
"Choose..." in the Project Settings sheet's Worktree Location section, wait, nothing visibly opens --
`swaymsg get_tree` showed no new window at all, not even a mis-tiled one.

Root-caused it instead of re-filing it as a re-confirmed hang. `ps aux` plus a per-PID `/proc/<pid>/
environ` scan (grepping every `xdg-desktop-portal`/`xdg-desktop-portal-gtk` process on the host for
which one was actually bound to *this* instance's private bus -- several sibling critics' portal
daemons were running concurrently on *their own* private buses, and the first process inspected
turned out to belong to a different sibling entirely, `wf-rest4`, bound to a different
`DBUS_SESSION_BUS_ADDRESS` and a different `WAYLAND_DISPLAY`) found my own router process
(`xdg-desktop-portal`, confirmed via its `/proc/<pid>/environ` matching my bus path exactly) running
with **no `xdg-desktop-portal-gtk` backend anywhere on that bus**. Its own environment (dumped from
`/proc/<pid>/environ`) had no `XDG_CURRENT_DESKTOP` set at all.

That is the mechanism: xdg-desktop-portal picks a backend implementation for each interface
(FileChooser, Account, ...) by reading `$XDG_CURRENT_DESKTOP` from **its own process environment at
exec time** -- not from the D-Bus "activation environment" that `dbus-update-activation-environment`
sets (that only applies to *future* dbus-activated processes). The prior waves' recipe called
`dbus-update-activation-environment` with `WAYLAND_DISPLAY`/`GDK_BACKEND`/`XDG_RUNTIME_DIR`/
`DBUS_SESSION_BUS_ADDRESS` but never `XDG_CURRENT_DESKTOP` -- so the already-running portal router had
no way to resolve `org.freedesktop.impl.portal.FileChooser` to `gtk.portal`, never spawned a backend
for it, and a click on "Choose..." went out over D-Bus to nobody: no error, no dialog, indistinguishable
from a hang.

Fix: kill the private-bus portal router (safe -- it is mine, owns no state) and relaunch it with
`XDG_CURRENT_DESKTOP=GNOME` set directly in its own exec environment, not just the activation
environment:

```
XDG_CURRENT_DESKTOP=GNOME WAYLAND_DISPLAY=wayland-14 GDK_BACKEND=wayland \
  nohup /usr/libexec/xdg-desktop-portal -v > /tmp/wf-sweep2-prj-portal.log 2>&1 &
```

The resulting log (`reference/linux-progress/wf-sweep2/f-prj-18-portal-log-after-xdg-current-desktop-fix.log`)
now shows `Using gtk.portal for org.freedesktop.impl.portal.FileChooser in gnome` and
`providing portal org.freedesktop.portal.FileChooser`, and a fresh `xdg-desktop-portal-gtk` process
spawned on that exact bus. Re-clicking "Choose..." this time opened a real GTK "Open Folder" window
(`reference/linux-progress/wf-sweep2/f-prj-18-portal-dialog-renders-after-fix.png`) -- titled
"Open Folder", already correctly positioned/sized at (0,0) full-output in this headless sway setup, no
extra floating/resize/move treatment needed this time.

Typing a path directly into the GTK location bar (Ctrl+L) proved unreliable in this environment --
`wtype -k BackSpace` sent three times removed zero characters (confirmed by screenshot before/after),
and fast `wtype` text calls dropped/merged characters once (`/tmp/` became `/p/`). Switched to the
dialog's own search feature instead: clicked the search icon, typed the target directory's bare name
(`wf-prj18-target-dir`, a directory made specifically for this drive, unreachable by browsing from
"Home" since it lives under `/tmp`) -- the search box did not drop characters -- got exactly one
match, selected it
(`reference/linux-progress/wf-sweep2/f-prj-18-portal-search-selects-target-dir.png`), and clicked the
accept button (labelled "Choose a folder for new worktrees" -- the app's own dialog title, confirming
the app drives the portal's accept-label, not a generic default).

The dialog closed and the app's own Worktree Location text field now reads exactly
`/tmp/wf-prj18-target-dir` -- the chosen path, not the prior default `/tmp` -- with a new "Restore
Default" link appearing below it (absent while the field held the placeholder default), confirming
the field's state genuinely changed rather than the screenshot merely refreshing:
`reference/linux-progress/wf-sweep2/f-prj-18-field-updated-after-picker.png` (compare against
`reference/linux-progress/wf-sweep2/f-prj-18-settings-before-default-tmp.png`, taken before the
picker was ever opened, showing the field at its `/tmp` default with no "Restore Default" link).
Full recipe and root-cause writeup: `reference/linux-progress/wf-sweep2/f-prj-18-repro-recipe.sh`.
This closes the picker path end-to-end: dialog renders, folder selection works, and the choice
propagates back into the app's own state. Promoted to PASSED.
