# Critic report — F-CORE-DOM-01 / F-CORE-DOM-02 (worktree-base pin + location override)

Fresh critic, no relationship to the builder. Everything below was compiled and driven by me,
against the builder's own worktree at `/var/tmp/tt-coredom-3622264-30968`
(`fix/core-dom-01-02-3622264-16217`, commits `1f70f38b` F-CORE-DOM-02, `dd31b281` F-CORE-DOM-01),
not against any pre-built binary. I did not touch `DISPLAY=:1` or any `wayland-N` the user might be
using — every drive below ran under `Scripts/wayland-drive.sh`'s own private, headless, nested sway
compositor (`WLR_BACKENDS=headless`), one throwaway label (`coredomcrit-2208985-14206`) that I killed
at the end.

**Verdict: both rows PASSED, live, with no restart confound on -02 and a genuine kill+relaunch on
-01.**

## Setup

```
CARGO_TARGET_DIR=/var/tmp/tt-coredom-critic-target-2208985-14206
cd /var/tmp/tt-coredom-3622264-30968/rust && cargo build -p tiller --bin tiller -p tiller_ui --tests
```

Binary: `/var/tmp/tt-coredom-critic-target-2208985-14206/debug/tiller`. Fixture:
`/var/tmp/tt-coredom-critic-fixture-2208985-14206`, a disposable git repo with `master` (one commit,
`init`) and `feature` (`init` + `feature-commit`, not reachable from `master`) — the same shape the
ledger rows' own reproduction used. Screenshots referenced below are copied into
`docs/linux-rewrite/fullapp/critic-worktree-base-shots/`.

## Which of (a)/(b)/(c) was it — established before writing anything

The dispatch's own hypothesis was one defect wearing two symptoms. It is not. Reading the diff and
the persistence-layer test that already existed (`default_worktree_base_and_location_override_round_trip_through_the_catalog_store`,
`rust/crates/tiller/src/session.rs:2389`) shows the pin was **always written and always read back
correctly at the persistence layer** — `write_catalog`/`restore_catalog` round-trip both fields
faithfully, and that test predates both fixes. So this was never "(a) never written". The two rows
are genuinely independent bugs sharing one map (`project_worktree_defaults`), confirmed by driving
each live:

- **F-CORE-DOM-02 was "(c) read but ignored" by one specific path.** `begin_worktree_prompt` (the
  New Worktree dialog's open handler) never consulted `project_worktree_defaults` at all — it always
  seeded blank drafts and `confirm_worktree_prompt` sent `None`/the sibling directory whenever the
  dialog's own fields were left blank, full stop. The value was sitting correctly in memory the whole
  time (proof: Project Settings, which reads the *same* map, showed "Pinned" immediately); this one
  call site alone never looked.
- **F-CORE-DOM-01 was "(b) written but not read back" — but only by one path, and only right after
  boot.** `main()`'s one-time `cx.open_window` closure built the initial `Sidebar` and called
  `set_project_identity` for every project, but never called `set_project_worktree_defaults` —
  unlike `refresh_sidebar`, which always called both. `project_worktree_defaults` stayed an empty
  `HashMap` until the next unrelated refresh (adding a project, creating a worktree, saving any
  setting) happened to populate it. Reopening Project Settings *before* any such refresh — i.e.
  exactly a genuine restart with nothing else happening in between, exactly what the ledger row
  observed — read the still-empty map and showed the pin as unset, even though the DB had it right
  and every other path read it correctly.

Both fixes are landed:
`1f70f38b` snapshots the pin onto `WorktreePrompt` at open time and has `confirm_worktree_prompt`
fall back to it before reaching for `None`/the sibling directory — mirroring the Swift app's
`WorktreeDefaults.resolveBase`/`resolveParentDirectory` (below). `dd31b281` extracts
`seed_sidebar_identity_and_worktree_defaults`, shared by boot's construction and `refresh_sidebar`,
so the two call sites structurally cannot drift apart again, and also closes F-CORE-DOM-01's own
"...visible through the corresponding domain/control listing" VERIFY clause by adding
`displayName`/`color`/`iconKind`/`iconValue`/`defaultWorktreeBase`/`worktreeLocationOverride` to
`ctl project.list`, which previously exposed none of them.

## Semantics checked against the Swift original

`Packages/TillerCore/Sources/TillerCore/WorktreeDefaults.swift` (in the sibling `tiller` worktree):

```swift
public static func resolveBase(project: Project, worktrees: [Worktree]) -> String? {
    project.defaultWorktreeBase ?? worktrees.first(where: \.isPrimary)?.branch
}
public static func resolveParentDirectory(project: Project) -> String {
    project.worktreeLocationOverride ?? (project.rootPath as NSString).deletingLastPathComponent
}
```

and `App/SidebarView.swift:110-124`: Swift's own "New worktree in ..." UI is a plain SwiftUI
`.alert` with **one** field, a branch-name `TextField` — there is no per-dialog base or location
override in Swift at all. `AppModel.addWorktree(project:branch:)` (`App/AppModel.swift:1291-1303`)
always calls `resolveBase`/`resolveParentDirectory` directly. The Rust port's dialog additionally
offers its own one-off Base/Location fields (F-PRJ-17/F-PRJ-18, pre-existing, not part of this
fix) — the builder's fix correctly makes those the innermost override, falling back to the pin, then
to Swift's own defaults, which matches "explicit per-project override wins" and adds a strictly
additive layer on top rather than changing what Swift specifies. I checked this in the source
myself, not from the builder's commit-message quotation of it.

**Vanished pinned branch**: `GitWorktrees.add` (`Packages/TillerGit/Sources/TillerGit/GitWorktrees.swift:34`)
takes `base: String? = nil` and passes it straight to `git worktree add`, with no existence check
and no catch around a bad ref in `AppModel.addWorktree`. The Rust fix matches this — a vanished pin
is passed straight to `create_worktree` and surfaced as git's own error, not silently swallowed into
a HEAD fallback. I confirmed this by reading both sources directly; I attempted to reproduce it live
(pin set to a non-existent branch, New Worktree left blank) but lost the attempt to a pixel-coordinate
drift of my own making (see Traps below) and did not re-attempt given the row-critical evidence was
already secured elsewhere. This one sub-case rests on code inspection only, not a live repro.

## F-CORE-DOM-02 — live, single uninterrupted session, no restart

One `wayland-drive.sh` invocation, start to finish: `project.add` the fixture → open Project
Settings (gear, revealed on hover) → click "Default Worktree Base" → type `feature` → confirm
"Pinned" on screen → click Close → click "New Worktree…" → type only `cleanbase1`, leaving the
dialog's own Base/Location fields blank → Enter.

- `critic-worktree-base-shots/01-dom02-base-pinned-feature.png` — "feature" / "**Pinned**" shown
  immediately after typing, before Close.
- `critic-worktree-base-shots/02-dom02-new-worktree-placeholder-names-pin.png` — the New Worktree
  dialog's own Base field placeholder now reads *"optional — defaults to the pinned base
  (feature)"* — this is the exact wording the fix added, drawn live from a real pin, not a fixture
  string.
- `critic-worktree-base-shots/03-dom02-branch-typed-cleanbase1.png` — only `cleanbase1` typed; Base
  and Location fields untouched.
- `critic-worktree-base-shots/04-dom02-worktree-created.png` — new `cleanbase1` worktree row
  appears in the sidebar after Enter.

Filesystem, checked independently of the UI:

```
$ git -C /var/tmp/tt-coredom-critic-fixture-2208985-14206 worktree list
/var/tmp/tt-coredom-critic-fixture-2208985-14206             cbab7a9 [master]
/var/tmp/tt-coredom-critic-fixture-2208985-14206-cleanbase1  0524589 [cleanbase1]
$ git -C /var/tmp/tt-coredom-critic-fixture-2208985-14206-cleanbase1 log --oneline
0524589 feature-commit
cbab7a9 init
```

`0524589` is `feature`'s own tip commit (`feature-commit`), not `master`'s (`cbab7a9`). The new
worktree was cut from the pinned base, in the same session, with no restart to confound it — this
is the exact ledger reproduction, now passing.

## F-CORE-DOM-01 — live, genuine kill+relaunch, same DB

Continuing on the same DB (same `TILLER_WL_LABEL`, so the same `/tmp/<label>.sqlite`): reopened
Project Settings, set "Worktree Location" to `/dev/shm/domrestarttest` (the base was still pinned
to `feature` from the -02 drive above), confirmed both on screen, then let a **new**
`wayland-drive.sh` invocation run — its own unconditional `kill_ours` at start kills the previous
`tiller` process by matching `TILLER_SOCKET` in `/proc/*/environ`, then launches a **new PID**
against the same on-disk database. That is a real kill+relaunch, not an in-process reset.

- `critic-worktree-base-shots/05-dom01-both-fields-set.png` /
  `critic-worktree-base-shots/06-dom01-both-set-preclose.png` — both fields set and visibly
  confirmed ("feature"/"Pinned" and `/dev/shm/domrestarttest`) before the kill.
- `critic-worktree-base-shots/07-dom01-fresh-boot-after-restart.png` — first frame of the **new**
  process (plain sidebar, no settings sheet — proof this is a fresh boot, not the same session).
- `critic-worktree-base-shots/08-dom01-reopened-after-restart-both-persisted.png` — Project
  Settings reopened on the new process: **"feature" / "Pinned"** and
  **`/dev/shm/domrestarttest`** both still shown, exactly as set before the restart. This is the
  precise scenario the ledger recorded as reverting to "master / Following primary branch (master)"
  and the bare `/dev/shm` placeholder; it no longer does.

Control-listing half of the VERIFY clause, on the same restarted process:

```
$ ctl project.list
{"...,"color":"coral","defaultWorktreeBase":"feature","displayName":"","iconKind":"icon",
"iconValue":"folder","worktreeLocationOverride":"/dev/shm/domrestarttest",...}
```

`defaultWorktreeBase` and `worktreeLocationOverride` (plus `color`/`iconKind`/`iconValue`) are now
present in `ctl project.list`, closing the half of F-CORE-DOM-01's VERIFY clause the ledger flagged
as failing even for the fields that did persist.

## Regression check

Full suites, run by me from a clean `cargo build`, not reused from the builder's own run:

```
cargo test -p tiller --bin tiller        -> 204 passed; 0 failed; 0 ignored
cargo test -p tiller_ui --lib            -> 366 passed; 0 failed; 0 ignored
```

Both numbers match the builder's own report exactly. The two new regression tests specific to this
fix (`new_worktree_honours_the_pinned_base_and_location_when_the_dialog_is_left_blank`,
`boot_seeding_reaches_project_worktree_defaults_in_one_pass`) are in that count and pass.
`cargo fmt -p tiller -p tiller_ui -- --check` reports pre-existing formatting drift at ~27 sites
across `main.rs`/`session.rs`/`chat.rs`/`sidebar.rs`/`tab_bar.rs`; none of the reported line numbers
fall inside either commit's touched hunks (`sidebar.rs` ~425-460/1955-2075/2490/3830-3900/4356+,
`main.rs` ~735-880/3933-3950/11020-11060/12075-12165) — this is codebase-wide debt the two commits
did not introduce, not something to hold this fix to.

No new test failures anywhere in either suite. I did not exhaustively re-drive every other
F-PRJ-17/18 row (Use Primary, Choose…/Restore Default, the double-submit guard) live myself, but
`typing_worktree_base_and_location_emits_a_durable_update` and the other project-settings tests in
the 366-passed count exercise exactly that code, unmodified by either commit, and passed.

## Traps hit while driving this live (leaving them for the next critic)

- **`wayland-drive.sh` restarts the app on every invocation, even under `TILLER_WL_KEEP=1`.** Its
  entry-time `kill_ours` runs unconditionally, before the `KEEP` check is ever consulted — `KEEP`
  only skips the *exit-time* cleanup. A multi-step interaction (open settings → type → close → open
  a different dialog → type → confirm) has to be a **single** action block in **one** invocation, or
  every intermediate state is silently thrown away and replaced by a fresh boot reading the same DB.
  I lost two attempts to this before recognising it (one looked like "typing does nothing", the
  other looked like "New Worktree opened with the wrong branch cut" — both were actually "the app
  restarted between my clicks and I didn't notice").
- **`type <text>` (multi-char wtype, no `-k`) silently dropped every keystroke into this
  on_key_down-driven field**, while `key <char>` (one `wtype -k` per character) worked immediately
  and synchronously every time. I confirmed the field is driven purely off
  `card.default_worktree_base`/`.borrow()` with no async round-trip for its own display (Pinned
  appeared the instant two `key` presses landed), so `type`'s failure was a genuine input-path gap
  in this specific field's key handling, not a redraw race. Building a positive control (`key f`
  showing `f` in the field) before trusting the `type`-based negative is what caught this.
- **Layout shifts under you.** Once a "Worktree Location" override is set, a "Restore Default" link
  appears and pushes "Remove Project"/"Close" down by one line; a coordinate that hit "Close" on a
  fresh project silently misses it once the sheet has more content. Once a second worktree exists,
  "New Worktree…" moves down a row too. A click that "does nothing" on a later run may just be
  aimed at last run's coordinates.

## Biggest remaining gap

Not in these two rows — both are cleanly fixed and now live-proven. The one loose end from this
pass: the "vanished pinned branch surfaces git's own error rather than silently falling back to
HEAD" claim in the fix's commit message is confirmed correct **by reading both the Rust and Swift
sources side by side**, but I did not manage to reproduce it live (lost to the coordinate-drift trap
above, and did not re-attempt once the two rows' own decisive evidence was already secured). A
future pass — or the same builder, cheaply — should drive it once: pin a project's base to a branch
name that does not exist, leave the New Worktree dialog blank, and confirm the dialog surfaces a
git error (referencing the missing branch) rather than quietly creating a worktree from HEAD. This
is not a doubted claim, just an unclosed live check.
