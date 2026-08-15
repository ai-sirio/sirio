# C-MAIN-1 critic verdicts

Verified against HEAD (`f7ec060`, `linux/gpui-waku`) with no access to the builder's own
reasoning — the orchestrator reported the builder returned nothing usable, so every row below is
graded from the slice brief (`C-MAIN-1.md`), the builder's own write-up read only for routes
(`C-MAIN-1-report.md`), and fresh, independent instruments: `cargo test -p <crate>` per crate, a
`grep`-verified `cargo build -p tiller`, and live drives on the wayland lane (real UI clicks,
real D-Bus `Notify` capture via `dbus-monitor`, and two real graceful-quit + relaunch restarts
against the same sqlite DB with `/proc`-tree inspection of the actual spawned `claude` argv).
Fixture artifacts written under `/home/enzopalmisano/.claude/projects/-tmp-cmain1-critic-*` during
transcript/resume testing were deleted after use; nothing under the real worktree's own
`~/.claude/projects/-home-...-tiller-linux` slug was touched.

`cargo build -p tiller` is green (single pre-existing, unrelated `pump_task` dead-code warning).
`cargo test -p tiller` (142 passed) and `cargo test -p tiller_agents` (8+ passed) both green,
run per-crate as required.

## `F-CORE-ACT-24` — PASSED

Real restart, real process tree, both directions. Built a fixture repo, opened a live Claude Code
tab (UI clicks: tab-bar `+` at `(978,49)`, `Claude Code` menu item), attached a session ref via
`tillerctl notify --session pane-3 --agent-session <ref> --status running` with a matching
`~/.claude/projects/-tmp-cmain1-critic-repo/<ref>.jsonl` fixture file present, then `tillerctl
quit` (graceful, flushes session state) and relaunched against the **same** sqlite DB.
`pstree -p -a` on the real relaunched `tiller` pid showed its restored terminal's direct child as
`claude --resume cmain1critic-fake-ref` — the literal resume argument, in a real process, after a
real restart. Negative control: a second tab given a session ref with **no** matching transcript
file restored as plain `claude`, no `--resume` — the prunable branch. This directly refutes the
ledger's prior "2 real restarts each spawn a fresh random --session-id" finding; the fix
(`32fffaf`, `resumable_session_refs`/`restored_agent_shell` in `main.rs`) is real and live-proven.

**Evidence discriminates:** yes — `claude --resume <ref>` vs bare `claude` is not a state either
restore path could reach by accident.

## `F-AGENT-SESSION-01` — PASSED

Same drive as above additionally exercises `AgentSessionValidator::is_likely_valid` itself, not
just `main.rs`'s restore-plan bookkeeping: the resumable case only produced `--resume` because the
fixture `.jsonl` existed at the exact slug path `is_likely_valid` checks; the no-file negative
control fell back to fresh precisely because that same check returned false. `session_ref.rs`'s
`is_file()` gate is now reachable from a real restart, not just its own unit tests.

**Evidence discriminates:** yes — same instrument as F-CORE-ACT-24, isolating the validator's
specific effect via the positive/negative fixture-file pair.

## `F-AGENT-SAFE-02` — PASSED

Isolated from any `prepare()` confound: added a second fixture repo as a project, hand-wrote a
`.claude/settings.local.json` with a stale `'/old/DerivedData/Tiller/tillerctl'` leading path,
`marker` field, and `pane-99` session id — then quit and relaunched **without ever opening a pane
in that worktree** (confirmed via `panel.list` after relaunch: only the other worktree's panes
appear). Post-relaunch file diff: the leading path is now the real resolved
`/home/enzopalmisano/.local/share/TillerRust/bin/tillerctl`, while `pane-99` and `"other":
"preserved-marker-xyz-2"` are byte-identical. `ClaudeHookMigrator::migrate_file` is genuinely
invoked at startup across every known worktree, independent of any pane ever opening — exactly the
gap the row named. (An earlier attempt against a worktree whose Claude Code tab **did** restore
this run showed the whole file rewritten by `prepare()` instead — a confound, discarded, not used
as evidence; the clean run above avoids it entirely.)

**Evidence discriminates:** yes — the ledger's own claim was "0 refs outside its module"; a
worktree with zero panes opened this run getting its file rewritten anyway is not reachable by any
path other than the new startup sweep.

## `F-AGENT-SESSION-02` — PASSED

Live socket round-trip: wrote a fixture `.jsonl` with two distinguishable marker strings, called
`session.transcript session=pane-9 agent=claude worktree=<fixture>` over the raw control socket —
returned `{"text":"CMAIN1_TRANSCRIPT_MARKER_HELLO\nCMAIN1_TRANSCRIPT_MARKER_REPLY"}`, sourced
straight from `ClaudeTranscriptSource`. Two negative controls returned typed failures, not silent
empty success: unknown pane → `"no session reference recorded for this pane"`; a pane whose ref
exists but the worktree param doesn't match → `"no transcript available for this session"`.
`session.transcript` is also confirmed present in `system.capabilities`'s advertised list
(code-read, `main.rs:867`).

**Evidence discriminates:** yes — the marker text could only come from the fixture file this
specific method read.

## `F-CORE-ACT-19` — PASSED

Live D-Bus capture, not a re-paraphrase of P120: spawned a real Claude Code pane via UI clicks
(`panel.list` confirmed `agent:"claude"`), backgrounded it with `tab.select`, ran `dbus-monitor
--session "interface='org.freedesktop.Notifications',member='Notify'"` throughout, then `tillerctl
notify --session pane-2 --status done`. Captured method call:
`title="Claude Code — tiller/linux/gpui-waku"`, `body="linux/gpui-waku · tiller"`. Title is now
agent + worktree label (`context.activity_label`, `"{project}/{branch}"`), not agent + status —
the old defective form was literally `"Claude Code — finished"`. `e17f3f7`'s one-line override in
`post_activity_notification` is real and live-confirmed with a fresh capture, not inherited P120
evidence.

**Evidence discriminates:** yes — the title text names the worktree/project, a value the old
status-suffixed form could never produce.

## `F-CORE-ACT-20` — PASSED

All four branches of `NotificationPolicy::should_notify` driven live with the same rig, each
checked against a fresh `dbus-monitor` window: (1) hidden pane, fresh status (`error` after
`done`) → **fired** (reused as the base case, see ACT-19); (2) hidden pane, **identical** status
resent (`done` twice) → zero `Notify` calls in the monitor; (3) a pane with **no** agent identity
(`pane-1`, plain Terminal) sent a status → zero calls; (4) the same Claude Code pane brought to
**foreground** via `tab.select` (confirmed `"active":"true"` in `panel.list`), sent a fresh status
→ zero calls. This closes both branches the ledger recorded as untested (no-agent-running,
identical-status) plus independently reconfirms both halves of the visibility gate.

**Evidence discriminates:** yes — same positive-control rig proves the monitor is alive (branch 1
fires); the three suppression branches are then genuine negatives against a working detector, not
silence from a broken one.

## `F-AGENT-OMP-03` — half-proven (unchanged)

Independently reran the exact upstream command myself (not trusting the report's prose):
`oh-my-pi --print --no-tools 'echo hi'` still throws `SyntaxError: Unexpected token ':'` out of
`oh-my-pi.js:176` at module load, before Tiller's generated command string is even relevant. The
command-string half (unit-tested, matches spec) stays proven; the live-run half stays blocked by a
genuine upstream defect no file in this repo can fix. `main.rs` correctly has no adapter-invocation
caller at all (confirmed unchanged, `grep`); that gap is F-SET-05's, not this row's, as the ledger
already notes.

**Evidence discriminates:** yes for the negative (upstream crash reproduced fresh, not inherited).

## `F-CORE-ACT-10` — NOT EXERCISED (unchanged)

Not re-driven this pass: `panes.rs` (the file most likely to hold the real Layer-D wiring) is
outside this slice's owned files and was not touched by any C-MAIN-1 commit. The ledger's standing
finding — prior evidence was a tab-active-highlight confound plus an accidentally misrouted chat
conversation, leaving no valid discriminating evidence either way — stands; I did not spend this
pass's budget re-deriving a clean drive for a row this slice made no code change to.

## `F-BRW-04` — FAILED — defective (unchanged)

Re-read `handle_browser_action` in `main.rs` (owned by this slice, untouched by any C-MAIN-1
commit) line by line: `browser.open` validates only non-emptiness, no scheme/reachability check;
`browser.navigate`'s `submit_address(...).map_err(...)` only surfaces an error if the underlying
webview call itself returns `Err`, and per the ledger's live P96 evidence it does not for either an
unreachable host or a syntactically-bad URL — both return `ok:true`. Static re-read confirms the
same code paths P96 drove live are still present verbatim; not re-driven live this pass (foreign
file, no code change to verify against).

## `F-BRW-05` — half-proven (unchanged)

`grep`-reconfirmed: `browser.act` (`main.rs:4613`) is still the only call site of
`BrowserSurface::set_agent_driving` in the tree. The render-wiring half stays proven (P96/prior
passes); the trigger half (a real ACP browser action actually flipping this flag) still has no
implementation anywhere.

## `F-BRW-09` — FAILED — defective (unchanged)

Re-read `chat.rs` (foreign file, untouched by this slice): `Entry::User` still renders through
`render_plain_text` (`chat.rs:3954-3974`, no link parsing) while only `Entry::Assistant` uses
`render_markdown`. Both `cx.open_url` call sites (`chat.rs:813`, inside `render_inline`'s
`on_click` around `chat.rs:3676`) still have no modifier check and no internal-tab routing. Same
defect the ledger recorded, confirmed present in the current tree by direct read rather than
inherited.

## `F-CHAT-34` — FAILED — absent (upgraded from NOT EXERCISED)

`grep -rn "chat_sessions" rust/crates/` across the **whole** workspace: the only non-test
occurrence is the method's own definition in `tiller_persistence/src/db.rs:523`; every other hit
is that crate's own integration test. Zero callers in `main.rs` or `tiller_ui` — the browse/
open/delete past-chats capability this row names has no path in the app a user could ever reach,
which is a structural absence, not merely an untried one. The prior NOT EXERCISED verdict predates
this exhaustive check ("no fresh evidence... consistent with the standing finding" was itself an
inference from a different row's aside, not a direct search).

**Evidence discriminates:** yes — an exhaustive negative-space grep across the whole workspace is
decisive for "0 production callers," which a partial/targeted grep is not.

## `F-CHAT-35` — FAILED — absent (unchanged)

`grep`-reconfirmed no "no past chats"/empty-state string anywhere in `chat.rs`. Consistent with
F-CHAT-34 above: the browse UI this empty state would belong to doesn't exist either.

## `F-CHG-02` — FAILED — absent (unchanged verdict; genuine partial fix, doesn't reach the clause)

Live-reproduced the row's actual clause first: added a fixture project, selected its worktree,
`workspace.close`d it, then `surface.changes.open worktree=<that same still-mounted worktree>` —
now returns `{"ok":true, ...report...}` where it previously errored `"no current workspace"`
(`14e9eee`'s `control_state.select_worktree` re-mark fix, live-confirmed, positive+negative: same
call sequence against an un-patched mental model would 404). This is a real fix to a real bug. But
the row's actual required behaviour — a visible "no worktree selected" empty state rendered in the
Changes surface/right panel itself when no worktree is genuinely current — is still absent
(`grep`-reconfirmed no such state in `changes.rs`/`right_panel.rs`, both foreign files, both
untouched). The socket-layer fix and the UI-empty-state clause are different bugs; only the first
is closed.

## `F-CHG-13` — FAILED — defective (unchanged verdict; scope claim only half correct)

Re-read `main.rs`'s two `OpenDiff` handlers (`2819`, `4018`, both in this slice's **owned** file):
both discard the emitted path (`ChangesTabActionEvent::OpenDiff(_path) =>
workspace.add_changes_tab(cx)`) and always open the generic multi-file Changes tab. No
`add_diff_tab`/single-file-view method exists anywhere in the tree (`grep`-confirmed). The
builder's report files this row entirely under "the defect lives in a foreign file
(`changes.rs`)" — that's only half true: `main.rs` itself throws the path away before `changes.rs`
ever gets a chance to render it differently, and `main.rs` is this slice's own file. The underlying
capability (an isolated per-file diff tab) genuinely doesn't exist in either file, so the row stays
FAILED — defective either way, but the scope claim in the report is inaccurate and worth flagging
for the next link in the chain.

**Evidence discriminates:** yes — the `_path` discard is unambiguous, unrelated to which file
"owns" the fix.
