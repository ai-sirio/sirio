# Drive slice E11-git+tab+win — 3 rows

Families: F-GIT+F-TAB+F-WIN.

Exercise each row live and record what you saw. **Do not edit the ledger**;
a different agent adjudicates. For a `half-proven` row the evidence column
names the half already proven — drive only the missing half and say which.

| row | ledger line | current verdict | evidence recorded so far |
|---|---|---|---|
| `F-GIT-REMOTE-01` | 489 | half-proven | genuinely half, unlike `F-CORE-ACT-22`/`-USG-05` whose hedges failed. Re-swept 2026-08-14: `GitRemote::project_name` is live with **8 app references** (called at `tiller_ui/src/project_forms.rs:704` to derive a project name from a clone URL, and that form was driven live at pass 17). `github_owner` (`lib.rs:62` — note the pass-12 note misnamed it `github_owner_from_url`) has… |
| `F-TAB-08` | 124 | half-proven | live, pass 15: bare PATH launch → the New Chat submenu renders the no-agent fallback (two text lines at the picker position — "Other agents…" + "No supported agent found on PATH" per render_chat_empty, frame pass15/l1-01-picker-bare-path.png); the clause's second half is absent — the fallback carries no click handler, selecting it does NOT open Agents settings |
| `F-WIN-07` | 59 | half-proven | P106 fable §F-WIN-07: boot relaunch restored mounted worktrees, tabs and split panes, but explicit session.restore returned restoredCount:0 and the required History menu/⇧⌘O is absent. |
