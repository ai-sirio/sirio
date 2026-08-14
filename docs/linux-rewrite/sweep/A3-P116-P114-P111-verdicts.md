# Verdicts — A3-P116-P114-P111

Adjudicated against `docs/linux-rewrite/P116-report.md`, `docs/linux-rewrite/P114-report.md`,
`docs/linux-rewrite/P111-report.md` and their captures under `reference/linux-progress/`. I did
not drive or build any of these 12 rows before this pass. One row (`F-SET-10`) got a short,
genuinely ambiguity-resolving drive of my own, recorded below with its captures.

**Headline finding — read this before the row sections.** Ten of these twelve rows have "fresh"
evidence recorded in `P116-report.md`'s headless slice. Four of those ten cite evidence that does
**not** exercise the row's actual clause: the report drove a differently-implemented, similarly
named feature and the resulting text reads as relevant without being so. This is the conjunction
trap from `EVIDENCE-STANDARD.md` in a new shape — not nouns co-existing in one file, but a *whole
drive* attached to the wrong row ID. Each case is backed by a workspace-wide grep showing the
clause's actual target function has **zero production callers anywhere**, so the mismatch isn't a
matter of interpretation. Details under each row: `F-CORE-DOM-07`, `F-CORE-WSP-04`,
`F-AGENT-SESSION-01`, `F-GIT-BRANCH-01`.

---

## F-CORE-DOM-07 — auto-naming throttle

**Verdict: NOT EXERCISED** (unchanged).

VERIFY: "Feed transcript growth below each threshold and above both thresholds while observing
generated-name requests." The clause's target is `AutoNamingThrottle`
(`rust/crates/tiller_project/src/domain.rs:92`, `MIN_INTERVAL = 30s`, `MIN_GROWTH = 200` chars,
`should_request` at `:104`) — throttled auto-naming driven by chat-transcript growth.

P116's evidence for this row: `new-workspace --project … ` without `--branch` produced worktree
branch `wt-1786722152`. That branch name comes from `format!("wt-{seconds}")` at
`rust/crates/tiller/src/main.rs:7829` — a timestamp-based fallback **git branch name generator**,
unrelated to `AutoNamingThrottle`. Confirmed by grep: `AutoNamingThrottle`/`should_request`/
`record_request` appear only in `tiller_project/src/domain.rs`, its re-export in `lib.rs`, and
`tests/p99_naming_throttle.rs` — zero callers anywhere else in the workspace. The clause remains
completely untouched by this drive.

No frame opened (transcript-only evidence on both sides).

---

## F-CORE-WSP-04 — workspace LayoutCommand (insert/split/move/close/activate/divider/view-state/rename)

**Verdict: NOT EXERCISED** (unchanged).

P116's evidence: `panel split right --from <pane>` created a `sh (right)` pane, visible in
`panel list`. That routes through `PaneRegistry::split` (`main.rs:1131`, `self.panes.split(...)`) —
a raw terminal-pane split, a different subsystem from `tiller_project::layout::LayoutCommand`
(`layout.rs:282`, variants `Insert/Split/Move/Close/Activate/SetDividerFraction/UpdateViewState/
Rename`) and `classify_layout_command` (`:333`). Grep confirms `LayoutCommand`/
`classify_layout_command` appear only in `layout.rs` and its `lib.rs` re-export — zero callers
anywhere in `main.rs`, `tiller_ui`, or any other crate. The row's own prior evidence (RECENSUS
Slice B) already established "no production executor applies commands" for this exact type; P116's
drive doesn't change that, it drove an unrelated pane-split feature that happens to share the word
"split". None of the eight sub-operations this row asks for were exercised.

No frame opened (transcript-only evidence).

---

## F-CORE-WSP-08 — WorkspaceTabViewState (caret/selection/scroll/folds, chat draft, follows-tail, terminal viewport)

**Verdict: NOT EXERCISED** (unchanged).

P116's evidence is honest and does not overclaim: it explicitly states the drive "did not expose
`WorkspaceTabViewState`." What it showed instead — before `quit`+relaunch the worktree had Chat,
Terminal, Changes, and three control panes; after relaunch against the same `/tmp/pi.sqlite`,
`panel list` returned **no panes at all** — is a different finding (whole-pane session restore
after an app restart), not this row's clause (per-tab view-state survives a close/reopen inside a
live session). Worth flagging for whoever owns session-restore rows, but it doesn't move this row.

No frame opened (transcript-only evidence).

---

## F-AGENT-OMP-01 — Oh-My-Pi hook generation and launch

**Verdict: NOT EXERCISED — instrument reason** (unchanged).

P116 explicitly did not re-drive this: `ENVIRONMENT.md` already documents `oh-my-pi@0.2.0` dying
on un-transpiled TypeScript before argv is ever read (`SyntaxError: Unexpected token ':'` in
`bin/oh-my-pi.js:176`), upstream of Tiller. P116's own text: "Not re-driven... No Tiller adapter
change was made." Correct call; nothing to promote or demote.

No frame opened.

---

## F-AGENT-OMP-02 — omp hook status events (session_start/turn_start/turn_end/session_shutdown)

**Verdict: NOT EXERCISED — instrument reason** (unchanged), same upstream blocker as -01.

No frame opened.

---

## F-AGENT-SESSION-01 — AgentSessionValidator (Claude/Codex on-disk session file checks)

**Verdict: NOT EXERCISED** (unchanged).

VERIFY: "Create and remove each expected session file and inspect the validator result for Claude,
Codex, and an uncheckable agent." Target: `AgentSessionValidator::is_likely_valid`
(`rust/crates/tiller_agents/src/session_validator.rs:11`), which checks for
`<claude_config>/projects/<slug>/<ref>.jsonl` (Claude) or a matching filename under
`<codex_home>/sessions` (Codex).

P116's evidence: set a `session.ref` string on a live control pane (`main.rs:1417`, an in-memory/
persisted `session_refs: BTreeMap<String,String>` keyed by pane id — just a label store), then
`restore-session`, quit, relaunch; the relaunched worktree had no panes to query. This never
touches `AgentSessionValidator` — no session file was created or removed anywhere on disk, and
`is_likely_valid` has zero callers outside `tests/session_sources.rs`-style test files (confirmed
by grep across `tiller_agents`, `tiller`, `tiller_ui`). `session.ref`/`restore-session` and
`AgentSessionValidator` are different mechanisms that happen to share the word "session."

No frame opened (transcript-only evidence).

---

## F-AGENT-SESSION-02 — Claude/Codex transcript extraction (recent text from JSONL)

**Verdict: NOT EXERCISED** (unchanged).

P116 is honest here, unlike -01: it states plainly that "no transcript/recent-text API appears in
`capabilities`," so it correctly reports non-exercise rather than attaching an unrelated drive.
Nothing to correct.

No frame opened.

---

## F-CHAT-34 — Chat History browse/open/delete surface

**Verdict: NOT EXERCISED** (unchanged).

None of the three source reports carries fresh evidence for this row specifically. `P114-report.md`
mentions it only as `F-CHAT-35`'s unmet dependency ("F-CHAT-34's browser/open/delete UI has no
production surface"), which is consistent with, not a change to, the existing finding: the
persistence half (`Db::chat_sessions`, `Db::delete_chat_session`) has no production caller, and the
UI half is resume-most-recent only (`handle_resume_chat` → `retained_chats.first()`), with no
browsable list, no open-arbitrary, no delete-with-confirmation.

No frame opened.

---

## F-CTRL-CLI-02 — tillerctl shim install (XDG symlink) and invocation from a real agent hook

**Verdict: NOT EXERCISED** (unchanged), evidence text upgraded.

VERIFY: "Install the shim, inspect the symlink target and executable resolution, then invoke it
from a real agent hook." P116 drove the **compiled `tillerctl` binary directly** against a live
socket: `ping`→`pong`, and structured, correct responses across project/workspace, panel, settings,
changes, session, notification, and capability requests, including two malformed `notify`
invocations correctly rejected with explicit CLI errors rather than silently accepted. That is real,
useful evidence that the CLI's protocol implementation and argument handling are correct.

It is not evidence for this row's clause, though: it never inspects the installed symlink at the
resolved XDG path (`resolve_tillerctl_path`/`install_tillerctl`, already established as BUILT by
static reading in RECENSUS Slice B, still never exercised live), and it was never invoked *from* an
agent hook — it was run by hand from the compiled binary's own path. None of the three VERIFY steps
happened.

No frame opened (transcript-only evidence).

---

## F-GIT-BRANCH-01 — GitBranches::list (`git branch --list`, names preserved verbatim incl. spaces)

**Verdict: NOT EXERCISED** (unchanged).

VERIFY: "Create local branches with spaces and ordinary names, list them through the Git layer, and
compare exact names." Target: `GitBranches::list`/`list_branches`
(`rust/crates/tiller_git/src/branches.rs:14,36`, deliberately line-based parsing so branch names
*may* contain internal whitespace).

P116's evidence: `new-workspace` on a scratch repo created branch `wt-1786722152`, confirmed via
`git branch --all` (raw shell) and the socket's `list-workspaces`. Neither of those calls
`GitBranches::list`/`list_branches` — grep confirms zero callers of either outside
`tiller_git/tests/p99_git_rows.rs` and the `lib.rs` re-export; `main.rs`'s `workspace.create`
handler never references `GitBranches`. No branch name containing a space was created or listed
through "the Git layer" as the clause asks. Same shape as `F-CORE-DOM-07`: a worktree-creation
side-effect (auto-generated branch name) stood in for a differently-implemented, unwired listing
function.

No frame opened (transcript-only evidence).

---

## F-TERM-PTY-06 — terminal file-drop (exact path text, no implicit Enter, PTY-write not gesture)

**Verdict: NOT EXERCISED — instrument reason** (unchanged).

VERIFY: "Drop files onto a live agent shell and confirm the agent receives the exact path text
without an implicit Enter." P116's evidence is `panel write --input '...' --enter` / `panel key
enter` — generic PTY text injection over the control socket, not `terminal_file_drop`/
`TerminalView::receive_file_drop`. The same report's `F-CORE-FILE-03` entry states outright "the
socket has no file-drop/external-path method," confirming there is no headless route to this
clause either. `ENVIRONMENT.md` already records XDND as unexercisable by this harness (`xdotool`
has no drag source). Nothing here changes the instrument-blocked status.

No frame opened.

---

## F-SET-10 — usage-bar visibility toggle, refresh interval, Refresh now

**Verdict: half-proven** (unchanged word; evidence materially corrected).

None of P111/P114/P116 touch this row. It was genuinely ambiguous enough to be worth the drive the
task permits: the current ledger evidence claims "DB half absent — persisted schema lacks the keys
(P58 handoff open)," but `P111-report.md`'s cross-cutting note #3 states in passing that
`usage.ollamaVisible` "follows F-SET-10/11's established pattern" — implying the keys already
existed. I drove it myself to settle that (wayland lane, no drive-lock contention,
`TILLER_WL_LABEL=adjset10`, fresh `/tmp/adjset10.sqlite`):

1. Opened Settings → AI Providers. `surface.settings.read` over the socket already reports
   `claudeShowInBar: true`, `codexShowInBar: true` — both defaults, so this alone is non-discriminating.
2. **Real synthetic click** (`wayland-drive.sh`'s persistent virtual pointer, not a socket call) on
   Claude Code's "Show in usage bar" toggle. Frame
   `reference/linux-progress/adj-A3-P116-P114-P111/03-providers-after-click.png` shows the toggle
   flipped OFF (grey) while Codex's identical control stays ON (orange) — proves the click landed on
   the right control and the state genuinely changed, not a stuck/default read.
3. Read the live DB directly: `usage.claudeVisible = 'false'` — the write reached SQLite immediately.
4. **Fresh app process**, same `/tmp/adjset10.sqlite`, no click, no socket write — just
   `surface.settings.open`/`.select`. Frame
   `reference/linux-progress/adj-A3-P116-P114-P111/02-providers-after-relaunch.png` shows Claude's
   toggle still OFF and Codex's still ON, read purely from disk by a new process. This is the
   discriminating half: the default is `true`, so a relaunch reading `false` can only come from the
   persisted row, not a construction default.

**The "DB half absent" claim in the current ledger is stale.** `claude_show_in_bar`/
`codex_show_in_bar` are fully wired: `settings.rs` UI toggle → `AppSettings` → `SettingsSnapshot` →
`main.rs:8032-8066` mapping → `session.rs:1663-1664` (`usage.claudeVisible`/`usage.codexVisible`
rows) — and this drive proves the whole chain live, not just by reading the source.

What remains genuinely unexercised live: **changing the refresh interval**, and **clicking Refresh
now and confirming the status/read-time changes** — both still covered only by the drawn GPUI tests
(`refresh_now_re_runs_provider_discovery` etc.), not by a live gesture. That is the half still owed,
and it's a different half than the ledger currently records.

Frames opened: `02-providers-initial.png`, `03-providers-after-click.png`,
`02-providers-after-relaunch.png` (all under
`reference/linux-progress/adj-A3-P116-P114-P111/`).
