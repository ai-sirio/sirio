# Drive slice E06-brw — 5 rows

Families: F-BRW.

Exercise each row live and record what you saw. **Do not edit the ledger**;
a different agent adjudicates. For a `half-proven` row the evidence column
names the half already proven — drive only the missing half and say which.

| row | ledger line | current verdict | evidence recorded so far |
|---|---|---|---|
| `F-BRW-05` | 254 | NOT EXERCISED | needs an agent driving the browser; no ACP browser action was run against the surface |
| `F-BRW-06` | 255 | NOT EXERCISED | **not absent** — `browser.rs` carries `request_permission`, `permission_prompt`, `allow_permission`, `deny_permission` and the doorhanger render at :1083, with a green unit test `permission_doorhanger_resolves_and_persists_by_origin`. Never triggered live, and code plus a green test is `NOT EXERCISED`, never `PASSED` |
| `F-BRW-07` | 256 | NOT EXERCISED | **P96 headless lane:** `browser_origin_grant` held **0 rows** before and after relaunch. Raw browser open/navigate does not trigger an Allow-origin prompt, and the control capabilities expose no permission-grant action; therefore no allowed origin could be retriggered to prove persistence |
| `F-BRW-08` | 257 | NOT EXERCISED | RECENSUS Slice B BUILT: Settings::render_browser_grants at rust/crates/tiller_ui/src/settings.rs:2361 — production Revoke and Revoke all controls call the per-origin/all grant callbacks. No source report exercised the clause’s user-visible/live result; census evidence cannot produce PASSED. |
| `F-BRW-09` | 258 | NOT EXERCISED | the row is **chat**-link routing plus the modifier bypass to the system browser, not in-page navigation. An in-page link click was exercised (example.com -> iana.org) but does not satisfy it; no HTTP link was clicked in a chat transcript and no modifier bypass was attempted |
