# Drive slice E10-persist+sid+agent+per — 6 rows

Families: F-PERSIST+F-SID+F-AGENT+F-PER.

Exercise each row live and record what you saw. **Do not edit the ledger**;
a different agent adjudicates. For a `half-proven` row the evidence column
names the half already proven — drive only the missing half and say which.

| row | ledger line | current verdict | evidence recorded so far |
|---|---|---|---|
| `F-PERSIST-DB-06` | 509 | half-proven | the clause's subject is a conjunction — "**Agent account** *and* session records persist… VERIFY: …inspect restore planning *and* **account lookup**" — and only the session half is proven. Session refs: v5 upsert/load/delete + reopen replayed green (persistence_integration, pass 14); that half stands. The account half was never built: pass 14's own note, "agent-account rows … |
| `F-PERSIST-DB-11` | 514 | half-proven | **the clause's harder conjunct has no test at all.** VERIFY asks to "*Open databases representing **earlier schema versions** and inspect that **each migration preserves data** and creates the expected current records*". The cited test does the second half only: it opens a **fresh** `TempDir` database, asserts `schema_version == CURRENT_SCHEMA_VERSION`, then checks the expec… |
| `F-SID-12` | 81 | half-proven | catalog half: `set_primary_flips_the_application_level_marker` green (session.rs, pass 13) — set clears project siblings, unset leaves none, unknown path errors, other projects untouched. Sidebar half: the typed `ContextAction{Worktree, SetPrimary/UnsetPrimary}` dispatch is the same proven mechanism as the green NewTab context test; the menu itself photographed (H2-rc-wt). F… |
| `F-SID-19` | 88 | NOT EXERCISED | P104 §Group 1 pressed ctrl-t only with existing tabs, not the required no-terminal empty state; that row’s precondition and replacement behavior remain owed. |
| `F-AGENT-SAFE-01` | 472 | half-proven | worktree-local half measured pass 11 (fake HOME unchanged); skill-provisioning half absent — only the npx command builder exists (in `tiller_project`, not `tiller_agents`: scope correction to the pass-11 evidence), no management-marker or overwrite-refusal logic anywhere (pass 14 re-check) |
| `F-PER-08` | 244 | NOT EXERCISED | **P96 headless lane:** isolated v11 DB has `browser_origin_grant` and `setting` tables, both with **0 rows** before quit/relaunch and still **0 rows** after relaunch. The settings socket can read General/Permissions but exposes no mutation path for the required setting + browser grant. Restart was exercised; the persistence roundtrip was not |
