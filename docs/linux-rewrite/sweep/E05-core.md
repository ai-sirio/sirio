# Drive slice E05-core — 5 rows

Families: F-CORE.

Exercise each row live and record what you saw. **Do not edit the ledger**;
a different agent adjudicates. For a `half-proven` row the evidence column
names the half already proven — drive only the missing half and say which.

| row | ledger line | current verdict | evidence recorded so far |
|---|---|---|---|
| `F-CORE-ACT-02` | 338 | half-proven | sidebar half live (tab_status reads activity.status; FABLE-04 exoneration stands) and socket notify wired; the notification half is NO LONGER caller-less — as of 2026-08-14 main.rs:3892/3917 route transitions through should_notify→build_payload→notify-send (ACT-19/20 updated pass 17) — but that delivery has not been observed live, so the half stays unproven rather than absent |
| `F-CORE-ACT-06` | 342 | NOT EXERCISED | P115 replayed `panes::tests::process_owned_status_survives_title_and_child_exit_events` successfully in a fresh tree at `56977f2` (10/10 exact reruns). This removes the sole stated defective ground, but a passing unit test is not the required live gesture. Owed: identify one pane by title and another by process, replace each title with unrelated text, and observe that only t… |
| `F-CORE-ACT-07` | 343 | NOT EXERCISED | P115 replayed `panes::tests::real_pty_layer_a_debounce_suppresses_first_title_and_accepts_second` successfully in a fresh tree at `56977f2` (10/10 exact reruns). The ledger's unqualified name does not match with `--exact`; plain `cargo test -p tiller` supplied the qualified harness name. This removes the sole stated defective ground, but does not exercise the required live t… |
| `F-CORE-ACT-11` | 347 | NOT EXERCISED | P115 replayed `panes::tests::process_owned_status_survives_title_and_child_exit_events` successfully in a fresh tree at `56977f2` (10/10 exact reruns). This removes the sole stated defective ground, but a passing unit test is not the required separate-pane lifecycle exercise. Owed: exercise title, process, and spawn detection on separate panes; alter or terminate one source … |
| `F-CORE-TERM-02` | 395 | half-proven | upgraded from NOT EXERCISED without a new drive: `reference/linux-progress/p17-rclick-term.png` shows the real menu mounted on a live pane carrying **twelve** items — `Copy`, `Paste`, `Copy C…`, `Set Titl…`, `Copy P…`, `Copy T…`, all four `Split …`, `Clear T…`, `Close T…`. The action *set* is therefore confirmed live, which is more than the drawn test gave. **What remains** … |
