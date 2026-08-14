# E06-brw verdicts — F-BRW (adjudicated)

Adjudicator note: driven by a separate agent (report at
`docs/linux-rewrite/sweep/E06-brw-evidence.md`, captures under
`reference/linux-progress/drive-E06-brw/`). This agent did not drive or build any part of
this slice; per fleet rule the critic is never the driver. All 10 captures were opened and
inspected directly (not taken on the driver's prose alone); source claims were independently
re-grepped/re-read against `rust/` at HEAD `4073297` (read-only, no compile, no edits); the
`/tmp/drive-E06-brw.sqlite` file left behind by the drive was independently re-queried with
Python's stdlib `sqlite3` and matches the evidence file's stated end state (one row,
`https://f-brw-07-relaunch-proof.example`).

| row | ledger line | verdict | evidence |
|---|---|---|---|
| `F-BRW-05` | 254 | half-proven | Live-driven, positive+negative control: `ctl browser.act driving=true` shows an orange "Agent driving" pill next to Browser/Stop in the toolbar; `driving=false` removes it (frames 02/03/04-f-brw-05-*, all independently viewed, matches). `Browser::agent_driving()`/`set_agent_driving` render-wiring is proven live. But `browser.act` (`main.rs:4512`) is the *only* caller of `set_agent_driving` anywhere in the tree, and its own error text is `"browser.act is unsupported on Linux: only the driving flag is implemented"` — there is no ACP browser-tool-call anywhere that flips this flag as a side effect of a real agent action. The clause's "start an agent browser action" trigger does not exist; only a manual test lever does. |
| `F-BRW-06` | 255 | UNREACHABLE | `grep -rn request_permission rust/` (re-run independently) confirms zero production callers: `browser.rs:588`/`:880` define `request_permission`, the doorhanger renders at `:1242` with real Allow/Deny buttons, but the only call site anywhere in the tree is the unit test `permission_doorhanger_resolves_and_persists_by_origin` (`:1666`). No webview permission callback, control-socket method, or other event ever invokes it. Same "code complete, zero app callers, wiring owed" pattern the ledger already grades UNREACHABLE elsewhere (F-TERM-02, F-CORE-ACT-17/18, F-GIT-STATUS-02, F-GIT-DIFF-03) — complete-and-unreachable is not PASSED, and it is stronger than NOT EXERCISED since no lane, now or in the future without a code change, could reach it. |
| `F-BRW-07` | 256 | half-proven | Live-driven: confirmed `browser_origin_grant` empty, seeded one distinct origin (`https://f-brw-07-relaunch-proof.example`) directly into the running instance's SQLite DB (stdlib `sqlite3`, no compile/source edit), then killed the process and started a **completely fresh** Wayland instance (new socket). On first render, Settings → Permissions already lists the seeded origin (frame 02-f-brw-07-relaunch-persisted.png, viewed and confirmed) — proves `main.rs:8161` `load_browser_origin_grants()` → `with_browser_origins` (`:8234`) genuinely reloads and renders persisted grants after a real relaunch, not carried over in memory. Independently re-queried `/tmp/drive-E06-brw.sqlite` after the whole drive: exactly 1 row, matching. **Owed half:** the clause's literal "trigger access to the same origin, confirm no new prompt appears" step was never attempted. Note this checkpoint is currently evidence-null regardless of grant state — F-BRW-06 shows `request_permission` (the only thing that would ever show a prompt) has zero live callers anywhere, so no drive can make this specific checkpoint discriminating until that is fixed. Graded half-proven rather than PASSED because the clause names it explicitly and it was not attempted, even though attempting it today could not have added signal. |
| `F-BRW-08` | 257 | half-proven | Live-driven: seeded one origin into the running instance's DB, opened Settings → Permissions, and **clicked the real rendered per-origin Revoke button** (not a socket shortcut). Card flipped from listing `https://e06-brw-proof.example` to "No browser origins have been granted" (frames 02/03-f-brw-08-*, viewed and confirmed, click position lands on the rendered button in the before-shot). Independently re-queried the DB after the whole drive sequence — state is consistent with the row's own claimed 0-then-reseeded-by-F-BRW-07 progression, corroborating the delete genuinely hit `Db::revoke_browser_origin`, not just an in-memory list. **Owed half:** the clause also names the "Revoke all" bulk action; independently confirmed it is real, wired production code (`settings.rs:3051` render, `on_revoke_all_browser_origins` → `session_store.revoke_all_browser_origins()` at `main.rs:8253`) — fully drivable in this build, simply never clicked in this drive. Only the single-origin path is proven. |
| `F-BRW-09` | 258 | FAILED — defective | Live-driven: sent a real user chat turn (`surface.chat.compose`/`send`) containing markdown link syntax `[open-example-link](https://example.com)`. It rendered in the transcript as literal, unstyled raw text — not a clickable link (frame 02-f-brw-09-chat-link-rendered.png, viewed and confirmed: plain monospace-ish bracket/paren text, no accent color). Source re-read confirms why and extends the defect: `Entry::User` renders via `render_plain_text` (`chat.rs:3606`), which never parses markdown or populates the `links` list; only `Entry::Assistant` goes through `render_markdown` (`:3672`). The one click handler that exists (`chat.rs:777`, `MouseUpEvent`) unconditionally calls `cx.open_url(target)` — independently confirmed no modifier check exists there and `grep -rn "BrowserLinkTarget\|open_link" chat.rs` returns nothing. So even an agent-authored link (the only kind that is ever clickable) always opens the system browser, never Tiller's own internal Browser tab, with no modifier distinction. Neither half of the clause — internal-tab-by-default, modifier-bypass-to-system — exists in this build. |

## Disagreements with the driver's report

- **F-BRW-07** and **F-BRW-08** were both submitted by the driver as flatly
  `exercised-working`. Both are downgraded here to `half-proven`. F-BRW-08's clause names two
  distinct controls (per-origin Revoke, Revoke-all) and only the first was driven — the second
  is real, wired, production code (confirmed independently) that was simply never clicked; a
  bulk-revoke defect would not have been caught by this drive. F-BRW-07's remaining half
  ("confirm no new prompt on revisit") is currently evidence-null given F-BRW-06, but the
  clause names it and it was not attempted, so it is recorded as owed rather than credited.
- **F-BRW-06**'s driver-side label `could-not-reach` is mapped to the ledger's `UNREACHABLE`
  vocabulary term, matching the existing "code complete, zero app callers" convention used
  elsewhere in the ledger (F-TERM-02, F-CORE-ACT-17/18, F-GIT-STATUS-02) rather than treating
  it as a softer `NOT EXERCISED`.
- **F-BRW-05** and **F-BRW-09**: driver's own characterizations (`partially-exercised`,
  `exercised-broken`) map cleanly onto `half-proven` and `FAILED — defective` respectively —
  no substantive disagreement, source claims independently re-verified by grep/read and match.
- No overclaim found in the underlying facts stated by the driver — all screenshots and source
  citations checked out on independent inspection. The disagreements above are about how
  generously to round partial coverage of a multi-part VERIFY clause up to a ledger verdict,
  not about any fabricated or misread evidence.
