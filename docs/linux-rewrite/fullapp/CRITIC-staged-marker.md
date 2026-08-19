# CRITIC — F-CHG-06 (Files-tree staged marker), fresh pass

Judged in isolation, against the builder's branch `fchg06-staged-marker-3687962-4311`
(commits `0db25a26`, `98b91fb2`, both `fix(F-CHG-06): …`), on top of `linux/gpui-waku`.
I did not write this code. Own worktree: `/var/tmp/tt-fchg06-3687962-15564` (already the
builder's — reused read-only plus one isolated build). Own build:
`CARGO_TARGET_DIR=/var/tmp/tt-fchg06-critic-target-3990743-8974`. Own fixture repo:
`/var/tmp/tt-fchg06-critic-fixture-3990743`. Own app data dir (to avoid touching the
shared `~/.local/share/tiller` every other agent's `wayland-drive.sh` run defaults to):
`XDG_DATA_HOME=/var/tmp/tt-fchg06-critic-xdgdata-3990743`. Driven live with
`Scripts/wayland-drive.sh` (`TILLER_WL_BIN=.../debug/tiller`, `TILLER_WL_LABEL=fchg06critic2`)
against my own compiled binary, not the builder's.

## Verdict: half-proven

The defect the row names — the Files-tree marker dot cannot represent "staged" — **is
genuinely fixed and I verified it live**, pixel-by-pixel, including the one case a
two-state marker cannot express (staged *and* further modified). But the change as
committed does not compile its own test suite (a hard, one-line bug, not a style nit),
and it exposes — without fixing — a sibling defect in the Changes list that gets the
*exact same* hard case wrong in the opposite direction. Real progress, not a clean pass.

## What the fix does, and why it's the right shape

`DirectoryGitStatus` gained a `Staged` variant plus `for_file()`, a **second** precedence
resolver alongside the pre-existing `for_entry()` (`tiller_git/src/directory_status.rs`):

- `for_entry()` (directory roll-up, unchanged): conflicted > changed > untracked — still
  never returns `Staged`, on purpose, matching Swift's `DirectoryStatusAggregator`.
- `for_file()` (new, one file's own marker): conflicted > untracked > staged > modified.

I checked this order against the Swift original myself rather than trusting the builder's
citation: `GitStatusStyle.color` (`App/RightPanel/GitPanelTypes.swift:36-41`) is

```swift
if entry.isConflicted { return AppTheme.gitConflict }
if entry.isUntracked { return AppTheme.gitUntracked }
if entry.isStaged { return AppTheme.gitStaged }
return AppTheme.gitModified
```

— an exact match, including the load-bearing detail: `isStaged` (`indexState != nil &&
indexState != .untracked`, `TillerGit/GitStatus.swift:56`) doesn't care whether the
worktree also has changes, so staged wins over a simultaneous unstaged edit. `for_file`
reproduces this precisely. `right_panel.rs` wires it in: the one file-row call site
switched from `for_entry` to `for_file` (`right_panel.rs:317`), and `git_status_color`
gained a fourth arm mapping `Staged -> theme.git_staged` (`right_panel.rs:1315-1327`).
`theme.git_staged` is not a new colour — it's the pre-existing token `changes.rs`'s own
`status_color` already used (`= success`, the same green already proven correct for the
Changes list), so the two views are drawing from one paint can, just two different
precedence functions.

## Live verification (my own binary, my own fixture, real pixels)

Fixture (`/var/tmp/tt-fchg06-critic-fixture-3990743`, plain `git init`/`add`/`commit`):
`modified-only.txt` (unstaged only), `staged-add.txt` (pure staged add), `staged-rename.txt`
(pure staged rename), `staged-and-modified.txt` (staged, **then** edited again — `git
status --short` shows `MM`), `untracked-only.txt`. Booted the app under
`wayland-drive.sh`, `project.add` + `workspace.select` the fixture, opened Files, and
sampled the marker-dot pixel for each row (`convert … txt:-`):

| file | expected (Swift precedence) | sampled hex | verdict |
|---|---|---|---|
| `modified-only.txt` | amber (modified) | `#F2B847` | correct |
| `staged-add.txt` | green (staged) | `#7AC791` | correct |
| `staged-rename.txt` | green (staged) | `#7AC791` | correct |
| `staged-and-modified.txt` | **green** (staged wins over the unstaged edit) | `#7AC791` | **correct — the hard case** |
| `untracked-only.txt` | blue (untracked) | `#8CA3FF` | correct |

All four colours are now genuinely distinct on screen, and the one case a naive two-state
(or even a naively-ordered four-state) marker gets wrong invisibly — `staged-and-modified.txt`
— renders green, not amber. This is the row's defect, fixed, on a running instance I built
and drove myself, not inferred from the diff.

`conflicted > staged` (a file that's both conflicted and staged) is covered by the unit
test (`for_file_resolves_conflicted_over_untracked_over_staged_over_modified`, green,
re-run by me) but I did not additionally drive it live — constructing a real merge
conflict *and* a staged file on the same path in one fixture was not worth the added
build-cycle cost given the two defects already found below; noting it as the one cell of
the truth table proven only by unit test, not by pixels.

## Regression check

- The only call site touched is the one file-row marker (`right_panel.rs:317`);
  `for_entry` (directory roll-up) is untouched and its own dedicated test
  (`the_files_tree_marks_every_ancestor_with_conflict_over_change_over_untracked`,
  `tiller_ui`) exercises directory names only (`src`, `docs`, `conflict`, `move`,
  `moved_dest`) — no root-level file in that fixture is ever staged, so the file-marker
  change cannot perturb it. Directory-level ancestor colouring (F-GIT-STATUS-02) is
  unaffected by design.
- `tiller_git`'s own test suite for the new code
  (`for_file_resolves_conflicted_over_untracked_over_staged_over_modified`,
  `for_entry_still_collapses_staged_into_changed_for_directory_aggregation`) — both green,
  re-run fresh by me against my own build.
- No other `match` on `DirectoryGitStatus` exists anywhere in the tree outside
  `precedence()`, `slug()`, and `git_status_color()` (all three updated for the new
  variant) — grepped the whole workspace to confirm, so adding the variant cannot have
  left a stale non-exhaustive arm elsewhere.

## Defect 1 (the biggest remaining gap): the new gpui test does not compile

`cargo test -p tiller_ui` **fails to build**, full stop — not a failing test, a compile
error:

```
error[E0596]: cannot borrow `cx` as mutable, as it is not declared as mutable
   --> crates/tiller_ui/src/right_panel.rs:2827:13
2827 |         let cx = VisualTestContext::from_window(window.into(), cx);
     |             ^^ not mutable
...(nine more `cx.debug_bounds(...)` call sites downstream, all needing `&mut self`)
error: could not compile `tiller_ui` (lib test) due to 1 previous error
```

The new test, `the_files_tree_marks_a_staged_file_distinctly_from_a_merely_changed_one`
(`right_panel.rs:2810-2892`), shadows `cx` with `let cx = VisualTestContext::from_window(…)`
instead of `let mut cx = …` — the pattern the *pre-existing* sibling test two hundred lines
above (`the_files_tree_marks_every_ancestor_with_conflict_over_change_over_untracked`,
line 2604) gets right. One missing keyword. One-line fix:

```rust
let mut cx = VisualTestContext::from_window(window.into(), cx);
```

This is not cosmetic. `Scripts/ci-linux.sh` runs `cargo test -p tiller_ui` per-crate
(line 221) and, before that, `cargo clippy --workspace --all-targets` (line 194) — both
compile test code, so **both CI stages fail on this commit as committed**, not with a
lint or a red test but with a hard build error. `Scripts/ci.sh` cannot print `CI OK`.
Practically: nobody can currently run *any* `tiller_ui` test — not just the new one, the
whole crate's `#[cfg(test)] mod tests` is one compilation unit — until this line is fixed.
The very live-pixel gpui test the builder's own account cites as proof was, as committed,
never actually run to green; only the separate `tiller_git` unit tests (a different crate)
were. I found this by literally running `cargo test -p tiller_ui --lib right_panel::` on
my own build, not by reading the diff.

Two much smaller `cargo fmt -- --check` nits in the same new test block (lines 2786,
2838 pre-fix — both are the new code, not pre-existing drift elsewhere in the file):
trivial line-wrap reflows, not a build blocker, but `cargo fmt` was evidently not run
before committing either.

## Defect 2: the Changes list gets the exact same hard case backwards

The row's ledger evidence says "the Changes-list half of this row holds" — verified for
*pure* states only (a pure staged add, a pure modification). I drove the untested
combination — a file that is staged *and further modified* — through the live Changes
list on the same fixture, and it does not hold there either:

`changes.rs`'s own colour resolver, `status_color` (`changes.rs:1866-1877`, untouched by
this commit), orders precedence **backwards** from Swift and from the just-fixed Files
tree:

```rust
fn status_color(entry: &StatusEntry, theme: Theme) -> Rgba {
    if entry.is_conflicted() { theme.git_conflict }
    else if entry.is_untracked() { theme.git_untracked }
    else if entry.has_worktree_changes() { theme.git_modified }   // <- checked before is_staged()
    else if entry.is_staged() { theme.git_staged }
    else { theme.title }
}
```

Swift's single shared `GitStatusStyle.color` (quoted above) has no "worktree changes"
branch at all — staged always beats modified, is a single function purposely factored
out because (its own doc comment) "the file explorer and the changes list both need it
and had drifted into two copies." Rust now has that exact drift again, just moved: one
function per view, disagreeing on the one case that matters.

Live proof, same fixture, same file: `staged-and-modified.txt` sits in *both* the
`Staged` and `Changed` sections (confirmed via `ctl surface.changes.read` — appears in
both `staged` and `changed` result arrays, both with the real diff stat `+2 -0`) and I
pixel-cropped its row icon in both — **amber in both**, identical to the plain
`modified-only.txt` row right below it, even though it sits in the `Staged (3)` section
next to two genuinely green pure-staged siblings (`staged-add.txt`, `staged-rename.txt`).
In Swift the same file renders green in both sections. There is an existing test,
`a_staged_and_modified_file_appears_in_both_sections` (`changes.rs:2209`), that proves
the file lands in both sections — but it asserts presence only, never colour, so this
never tripped it.

Net effect after this commit: open the same worktree's Files tree and Changes list side
by side on a file that's staged-then-edited-again, and they now visibly disagree —
green in one, amber in the other — for the precise state the row calls out as the one
"invisible until someone stages a half-finished file." The Files-tree half is now right;
the Changes-list half, only ever spot-checked on easy cases, is not.

## For the next builder

1. **Fix `right_panel.rs:2827` first** — `let cx` → `let mut cx`. One word, unblocks
   `cargo test -p tiller_ui` and `cargo clippy --workspace --all-targets` for everyone,
   not just this row. Re-run `cargo fmt -p tiller_ui -- --check` while in there (two
   nits, same test block).
2. **Reorder `changes.rs::status_color`** (`changes.rs:1866`) to check `is_staged()`
   before `has_worktree_changes()`, matching `for_file`'s precedence and Swift's actual
   `GitStatusStyle.color`. Add the colour assertion `a_staged_and_modified_file_appears_in_both_sections`
   is missing, or a sibling test, so a future refactor can't silently re-flip it — and
   re-verify live: the Files tree and the Changes list must agree on `staged-and-modified.txt`.
