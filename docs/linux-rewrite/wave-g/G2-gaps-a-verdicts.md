# Wave G slice G2-gaps-a — critic verdicts

Independently verified at HEAD (`c1ecc7c`), builder commit `d4d9294`. ACT-25/26 driven live
under `Scripts/wayland-drive.sh` (labels `g2critA`, `g2critB`) against a real 3-worktree git
fixture (`/tmp/g2crit-repo/{main,second,third}`), not just re-read from the diff. DOM-07 checked
by fresh grep against the builder's "no code changed" claim.

## F-CORE-ACT-25 — PASSED

Live drive, label `g2critA`: `project.add path=/tmp/g2crit-repo/main` (3 worktrees, none
selected) followed immediately by `workspace.list` returned all three rows `mounted:false` —
this is `ControlState::from_catalog`'s real production call site (`main.rs` `fn main`, same
function this row's diff touches), not a synthetic harness. The old default was "mount
everything unconditionally," so an all-false result here is only possible post-fix. Second drive,
label `g2critB`: after `workspace.select workspace=.../main`, `workspace.list` showed
`main: mounted=true, selected=true`, the other two still `false` — the flip-on-select half. Both
halves of the row's VERIFY clause exercised through the same code path a sidebar click uses
(`SidebarEvent::SelectWorktree` and the control door `control_select_worktree` both call the
identical `TillerWorkspace::select_worktree`, confirmed by reading both call sites). `cargo test
-p tiller` also carries the new `bootstrap_restore_order_mounts_only_the_selected_worktree_at_first_paint`.

## F-CORE-ACT-26 — PASSED

Same `g2critB` drive, continued: enabled "Limit mounted worktrees" and set "Keep mounted" to 2
via real synthetic clicks on the actual toggle/stepper widgets in the rendered Settings > General
page (`click 1287 686` then `click 1213 741` x4; screenshot `/tmp/g2critB-shots/02-settings-cap2.png`
shows the toggle lit and the stepper reading "2"; `surface.settings.read` independently confirmed
`limitMountedWorktrees:true, mountedWorktrees:2`) — the real user gesture, not a settings
backdoor. Then selected main, second, third via `workspace.select` in sequence (the sidebar's own
code path, see ACT-25 note): after main+second selected, both read `mounted=true` (cap 2, not yet
exceeded); after third selected, `workspace.list` returned `main:false, second:true, third:true`
— the oldest-opened idle worktree evicted, the two most recently touched kept mounted, exactly
the row's VERIFY clause. This is a real `TillerWorkspace::select_worktree` -> 
`evict_over_capacity_worktrees` -> `WorktreeMountPolicy::ids_to_evict` -> `ControlState::close_worktree`
round trip, not the pure-policy unit test alone. `cargo test -p tiller` also carries
`selecting_past_the_mount_cap_evicts_the_oldest_idle_worktree`.

## F-CORE-DOM-07 — FAILED — absent

Builder claims blocked with no code change; verified independently. Fresh grep,
`grep -rn AutoNamingThrottle rust/crates --include=*.rs`: every hit is inside
`tiller_project/src/domain.rs` (definition), its `lib.rs` re-export, or its own
`tests/p99_naming_throttle.rs` — zero callers in `main.rs` or anywhere else. Fresh grep of
`main.rs` for `auto_naming`/`AutoNaming`/`summarizer_command`/`SummarizerChoice`: every hit is
settings plumbing (`SettingsSnapshot` read/write, the settings-report encoder, tests of the
snapshot round-trip) — no transcript-growth trigger, no async spawn of a summarizer command, no
tab-rename call site. `git log --oneline -- rust/crates/tiller_project/src/domain.rs` shows no
commit in this slice touched the file; the builder's "no code change, matches wave-D diagnosis"
claim holds. The subsystem genuinely does not exist (trigger, spawn, sink, per-tab throttle
state), and building even a minimal call site would be the exact "token call site with no
observable behaviour" trap this wave's brief warns against — correctly left unbuilt rather than
faked. Ledger stays `FAILED — absent`.
