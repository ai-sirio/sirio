# Wave C slice C-MAIN-3 — verdicts

Critic pass. **The builder returned nothing usable: no `C-MAIN-3-report.md` exists, and
`git log f3b6169..HEAD -- rust/crates/tiller/src/main.rs` shows exactly four wave-C commits
touching `main.rs` (`32fffaf`, `0e7672d`, `e17f3f7`, `14e9eee`) — all four match C-MAIN-1's
own report byte-for-byte (F-CORE-ACT-24/F-AGENT-SESSION-01, F-AGENT-SAFE-02,
F-AGENT-SESSION-02, F-CORE-ACT-19, F-CHG-02).** C-MAIN-3 (link 3 of 4) landed zero commits.
Graded from the slice brief alone, against the current tree (which does carry other slices'
concurrent work — checked per row below) and, where the row's discriminating half was
reachable, a live Wayland-lane drive (`TILLER_WL_LABEL=cmain3crit1`).

Two rows' evidence turned out to be stale for reasons unrelated to this slice's own
(non-)work — flagged explicitly below.

## F-EDIT-08 — NOT EXERCISED (unchanged, new context)

Prior evidence (portal dbus-monitor, contaminated capture) stands; no builder work landed.
Source read found a nuance worth recording: `DocumentRegistry` (editor.rs) — the component
the row's own code comment references — has **zero production callers**, exactly like
several other rows' dead infrastructure in this wave. But `TillerWorkspace::add_file_tab`
(main.rs:4423) implements its own inline dedup independently: it scans open `TabContent::File`
paths, and if the path is already open it sets `active_tab`/`select_tab` to focus the existing
tab instead of pushing a new one — a second, real, wired path to the same guarantee, reachable
via the Files-panel double-click route rather than the ctrl+o portal that stumped prior passes.
Attempted to drive it live (double-click `f.txt` in the Files panel via two `click` calls) but
the coordinate pair landed on stray targets across a resolution toggle instead (produced a
duplicate "Changes" tab, not a file open) — inconclusive, not a negative. Retains NOT EXERCISED;
the Files-panel route is a real, untried instrument for whoever drives this next.

## F-GIT-BRANCH-01 — NOT EXERCISED (unchanged)

Re-confirmed live via grep: `GitBranches::list`/`list_branches` (tiller_git/src/branches.rs)
still has zero callers outside `tests/p99_git_rows.rs`. No commit in the wave-C range touched
`tiller_git/src/branches.rs` (`git log f3b6169..HEAD` on that path is empty). Record stands.

## F-PER-01 — FAILED — defective (unchanged)

No commit in range touched `tiller_ui/src/chat.rs`'s persistence write path or
`tiller_acp/src/chat.rs` in a way relevant to this row (the three wave-C `chat.rs` commits are
the C-CHAT chain's status-pill/error-banner/MCP-warning work — confirmed via
`git log f3b6169..HEAD -- rust/crates/tiller_ui/src/chat.rs`, cross-checked against
INTEGRATION.md §3). Pass-17's live restart evidence (0 rows in `chat_turn`/`session_ref` after
two real ACP exchanges) is the most recent and most direct instrument available; nothing
supersedes it. Record stands.

## F-PER-08 — half-proven (unchanged)

Re-confirmed via source: `newly_allowed` (main.rs:4645-4668) is still the only site that could
synthesize a browser-origin permission grant, and it is socket/`browser.*`-driven only — no
UI-settings path reaches it. General-settings half stays proven from prior evidence;
browser-origin half remains unreached on this lane. Record stands.

## F-PERSIST-DB-05 — FAILED — defective (unchanged)

`load_chat_transcript` (tiller_persistence/src/db.rs:558) still has exactly its two production
callers in `tiller_acp/src/chat.rs` (re-confirmed by grep) plus test callers; no commit in range
touched that call graph. The row's own VERIFY (create chat tabs, restart, inspect transcript
restoration in the app) remains unexerciseable on the path the app actually uses, per the
standing finding. Record stands.

## F-PRJ-03 — FAILED — absent (confirmed live; evidence text corrected)

**The row's cited evidence text is wrong on one claim but the verdict is still correct** —
checked the actual VERIFY clause in `01-inventory-app.md:42`, which is not about an
Open-Project menu item at all: *"Browse to a non-Git folder and confirm the prompt offers
Initialize Git, Add without Git, and Cancel."* Live-drove it (`TILLER_WL_LABEL=cmain3crit1`):
`ctl project.add path=/tmp/cmain3-nongit` (a real folder with no `.git`) returned `ok:true`
and the project was added **silently, with zero prompt** — frame `09-nongit-add.png` shows the
new "cmain3-nongit" row appear in the sidebar with no dialog anywhere on screen. Source grep
(`initialize git|add without git|without git`, all non-test hits) confirms only a **post-add**
"Initialize Git" context-menu action exists (sidebar.rs:739,2142; command_palette.rs:414) —
never a browse-time three-choice prompt with a Cancel arm. Separately, and worth flagging for
whoever revisits this row: the ledger's own evidence text ("no Open Project/browse entry
exists") is now **false** — live frame `07-menuopen.png` shows the `+` menu offering
"Open Project…", "Clone Repository…", "Create Project…", all three, and `sidebar.rs`'s
`add-project-open` route is wired to a real `cx.prompt_for_paths` directory picker with two
green drawn tests. That code (commit `5c1248f`) predates the wave-C range entirely — it was
already on `main` before this slice was dispatched — so this is stale prior evidence, not
something this pass or slice changed. Verdict unchanged because the row's actual clause (the
non-git three-way prompt) is still genuinely absent; the evidence cell is corrected to test the
right thing.

## F-SET-04 — FAILED — defective (upgraded from half-proven)

**New, discriminating finding: a sibling slice's concurrent work in the same file
(`main.rs`, owned by all four C-MAIN links) changed the ground this row's stale reasoning stood
on.** C-MAIN-1's commit `32fffaf` (F-CORE-ACT-24/F-AGENT-SESSION-01) added a real production
caller of `resume_command` (main.rs:7807, inside `restored_agent_shell`, called from
`resumable_session_refs`/`restore_tabs`/`restore_tabs_in_workspace`) — so the old evidence's
"resume_command() having zero production call sites" is now **false**, and this row's functional
half is no longer merely "unproven," it is now directly testable. Traced the full call chain
(main.rs:7738-7840, 8450-8506): `saved_session_refs = session_store.load_session_refs()`
(main.rs:8450) is loaded and passed into `restore_tabs` **unconditionally** — grepped every
`.resume_agent_sessions` reference in `tiller/src/main.rs` and `tiller_persistence/src/db.rs`;
the setting is read only for UI round-trip (settings snapshot conversion, DB save/load,
`report.resume_agent_sessions` for `system.capabilities`) and is **never checked** anywhere in
the restore/resume code path. Toggling "Resume agent sessions" off therefore has **zero effect**
on whether a validated session actually gets `--resume`d on restart — the functional half the
row's VERIFY requires ("confirm restored agent-session behavior changes accordingly") is
concretely disproven by static trace, not merely unreached. DB-persistence half (settings value
itself round-trips) still holds. Upgrading half-proven → FAILED — defective: this is no longer
an unproven claim, it is a proven decorative setting.

## F-SET-09 — half-proven (unchanged)

Re-confirmed via grep: no `InstallStatus`/install-progress field anywhere in `settings.rs` (only
hit is a test name, "an installed CLI renders its status pill"). Click-reaches-consumer half
stays proven from prior evidence; feedback/control-state-change half stays structurally absent.
Record stands.

## F-SET-10 — half-proven (unchanged)

No commit in range touched the wiring cited in the standing evidence
(main.rs:8032-8066-area settings⇄AppSettings conversion, session.rs usage-bar plumbing); the one
`status_bar.rs` commit in range (`8b7c265`) is Codex refresh-classification copy, unrelated to
this row's toggle/refresh-interval/Refresh-now clause. DB half stays proven from prior live
kill/relaunch evidence; refresh-interval-change and Refresh-now-confirmed-update stay test-only.
Record stands.

## F-SET-18 — half-proven (unchanged)

Re-confirmed via grep: no Install/Update/Retry control site anywhere in `settings.rs`'s agent
rows. No commit in range added one. Record stands.

## F-SET-22 — FAILED — defective (confirmed, unchanged)

Re-confirmed directly in source: `settings_snapshot_from_app_settings` (main.rs:8284) still
carries the row's own tracking comment verbatim — `// F-SET-22 has no AppSettings field yet;
do not pretend this UI-only picker is persisted until its schema follow-up lands` — and
hard-codes `agent_colors: SettingsSnapshot::default().agent_colors` rather than reading anything
from `AppSettings`. `AppSettings` has no `agent_colors` field (grepped the whole struct); the
picker's click→`SettingsSnapshot.agent_colors` route is real (per prior evidence) but the value
dead-ends at exactly the site this row's own defect names. Record stands.

## F-SID-06 — half-proven (unchanged)

No commit in range touched `sidebar.rs`'s dot-badge/descendant-agent-status rendering (the three
in-range `sidebar.rs` commits are New-Worktree-prompt field focus/base-branch and a GitHub
avatar-prefill fix — confirmed via `git log f3b6169..HEAD -- .../sidebar.rs`, cross-checked
against INTEGRATION.md §3, neither touches the collapsed-project badge path). Standing P106
half-proven (dots shown live on worktree rows; collapsed-project descendant badge not exercised,
code gates dots to worktree rows) is the most recent applicable evidence. Record stands.

## F-SID-11 — half-proven (unchanged)

Same file-touch check as F-SID-06 above — no in-range `sidebar.rs` commit is relevant to the
branch/folder/primary/comment/status display clause. Standing P106 half-proven stands.

## F-SID-12 — half-proven (unchanged)

No commit in range touched `wayland-virtual-pointer.c` or the command-palette chord path.
BTN_LEFT hardcoding and the ctrl-shift-p-gated "Set Primary Worktree" palette entry
(re-confirmed present at sidebar.rs:762) remain the only routes; both still require an
out-of-scope chord/right-click this lane cannot deliver. Record stands.

## F-SID-15 — FAILED — defective (confirmed live, unchanged)

Re-confirmed directly in source, both halves. Grepped the whole file: the literal string
"Remove Worktree" does not occur anywhere in `sidebar.rs` — no context-menu entry exists. The
only door is the hover-`×` control (sidebar.rs:2454-2472): its `on_click` calls
`sidebar.remove_worktree_row(row_id, cx)` directly, with `cx.stop_propagation()` and no
intervening confirmation state, dialog, or even a synchronous guard — `remove_worktree_row`
(sidebar.rs:1525) spawns the actual `remove_worktree` (which deletes the on-disk directory) on
the background executor immediately. No commit in range touched this code (the three in-range
`sidebar.rs` commits are unrelated, see F-SID-06 above). Destructive, no safety confirmation,
unreachable via any named menu route — matches the standing verdict exactly. Record stands.

---

## Summary of changes from the standing ledger

| Row | Was | Now | Why |
|---|---|---|---|
| F-SET-04 | half-proven | **FAILED — defective** | A sibling slice's concurrent `main.rs` commit (`32fffaf`, C-MAIN-1) gave `resume_command` a real production caller, which made the functional half directly traceable for the first time; the trace shows `resume_agent_sessions` is read for UI/DB round-trip only and never gates the restore path — the setting is proven decorative, not merely unproven. |
| F-PRJ-03 | FAILED — absent | FAILED — absent (evidence corrected) | Live-confirmed the row's actual clause (non-git three-way prompt) is still absent; separately discovered the ledger's own evidence sentence ("no Open Project/browse entry exists") is false and pre-dates wave-C — flagged so it isn't propagated forward. |

All other 13 rows: no wave-C commit (from any slice) touched code relevant to their clause;
standing verdicts re-confirmed via source read and, where reachable, live drive.
