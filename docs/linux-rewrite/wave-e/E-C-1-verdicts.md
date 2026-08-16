# Wave E slice E-C-1 — critic verdicts

Critic pass over the builder's claims in `E-C-1-report.md`, against the wave-D root causes
recorded in `E-C-1.md`. Verified independently in a worktree the builder never saw.

## `F-CORE-ACT-25` — verdict: `FAILED — absent`

Independently re-confirmed the wave-D root cause at the current HEAD: `select_worktree`
(`rust/crates/tiller/src/main.rs:4063`) still calls `self.panes.set_external(&old_path,
Vec::new())` synchronously on every switch, and `AppModel` has a single `working_directory:
PathBuf` field, not a mounted-set. `grep -rn "BootstrapRestoreOrder"` outside
`tiller_activity/src/bootstrap.rs` and its own tests returns nothing but a `pub use`
re-export — zero production call sites. Builder made no code change and the report's own
diagnosis matches. No lane gesture (socket or synthetic input) can reach a concept the app
does not implement, so the specified behaviour is genuinely absent from the product, not
merely untestable.

## `F-CORE-ACT-26` — verdict: `FAILED — absent`

Same source read as ACT-25 (`select_worktree` tears down the outgoing worktree every time, one
worktree ever mounted). `grep -rn "WorktreeMountPolicy"` outside `tiller_activity/src/mount.rs`
and its own tests returns only the `pub use` re-export — zero callers. `mounted_worktrees` /
`limit_mounted_worktrees` remain persisted-only settings with no eviction consumer in
`main.rs`. Builder made no code change; root cause confirmed unchanged.

## `F-CORE-DOM-07` — verdict: `FAILED — absent`

`grep -rn "AutoNamingThrottle"` outside `tiller_project/src/domain.rs` and its own test module
returns only the `pub use` re-export in `lib.rs` — zero production callers. `main.rs` carries
`auto_naming`/`summarizer_agent` as persisted settings only; no transcript-driven renaming call
site exists anywhere to gate. Builder made no code change; the throttle has nothing to throttle
because the feature it gates was never built.
