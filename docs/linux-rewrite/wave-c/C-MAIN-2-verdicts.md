# C-MAIN-2 critic verdicts

Critic pass over slice C-MAIN-2 (15 rows). **The builder returned nothing usable**: no
`C-MAIN-2-report.md` exists, and `git log -- rust/crates/tiller/src/main.rs` shows no commit
between `f3b6169..HEAD` attributable to this slice (the only four wave-C `main.rs` commits —
`32fffaf`, `0e7672d`, `e17f3f7`, `14e9eee` — belong to F-CHAT/F-AGENT-SESSION/F-PRJ/notify-title
work, none of which touches any of this slice's rows). So this pass graded the current HEAD
(post wave-C-integration, commit `f7ec060`) cold, against the slice brief's prior evidence,
using fresh grep/live-drive instruments of my own rather than trusting any builder claim.

Instruments used: `grep -rn` over `rust/` for caller counts; `Scripts/wayland-drive.sh` (label
prefix `cmain2*`) for live control-socket and click-through drives against
`rust/target/debug/tiller` (built `2026-08-15 14:02`, unchanged by this pass); one genuine
kill+relaunch against the same `/tmp/cmain2wrk.sqlite` for the persistence row. 10 PNG frames
read and inspected directly.

## Rows

### `F-CORE-ACT-25` — verdict: **FAILED — absent** (unchanged)

Re-grepped `BootstrapRestoreOrder` workspace-wide on current HEAD: the only hits are its own
definition/impl in `tiller_activity/src/bootstrap.rs`, the `pub use` re-export in
`tiller_activity/src/lib.rs`, and its own test in
`tiller_activity/tests/activity_domain_integration.rs:179`. `restore_tabs_in_workspace`
(`main.rs:7967`) still does not reference it. No wave-C commit touched `bootstrap.rs` or the
restore path. Prior verdict stands on unchanged code.

### `F-CORE-ACT-26` — verdict: **FAILED — absent** (unchanged)

Re-grepped `ids_to_evict` workspace-wide: only `tiller_activity/src/mount.rs:9` (definition) and
its own test at `activity_domain_integration.rs:211`. Zero callers elsewhere, unchanged since the
prior pass; no wave-C commit touched `mount.rs`.
