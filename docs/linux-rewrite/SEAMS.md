# Open seams — the integration debt

**A seam is not done when Half A ships. It is done when a row moves.**

Last verified: 2026-08-14, 02:00.

## Why this file exists

The ownership map is what has let several builders work in one worktree all day without a collision.
It does that by forbidding an agent from editing another's file — so when a piece needs two owners,
it is cut into Half A (build it in your own file) and Half B (call it from mine).

**That discipline prevents collisions and manufactures orphans.** Half A is exciting, gets a brief,
and lands. Half B is one line in somebody else's file, gets named in prose at the bottom of a brief,
and is never dispatched. The result is code that is complete, tested, and unreachable — which the
ledger records as `FAILED — absent`, because from the user's side that is exactly what it is.

Tonight's count found **thirteen ledger rows** with that shape. At least one of them was created by
this project's own process rather than by any builder's mistake: `codex11` finished a 1283-line
browser and correctly did **not** mount it, because the mount point is `main.rs` and `main.rs`
belongs to `codex12`. It did what it was told. Nothing was tracking the other half.

**So: a brief that cuts a seam must register Half B here, in the same commit.** An unregistered
seam is how a fortnight of work becomes dead code.

## How to check whether a seam is really open

Do not trust prose, including this file's. Count references from the consuming side:

```bash
cd rust
grep -rn "\bSymbolName\b" --include=*.rs crates/ | grep -v "crates/<defining>/src/<file>.rs" | wc -l
grep -rn "\bSymbolName\b" --include=*.rs crates/tiller/src/ | wc -l   # the app
```

**References only from `examples/` or from tests mean the seam is open.** That is precisely the
signature the browser shows today.

## Open

| seam | Half A | Half B — owner and change | rows it blocks |
|---|---|---|---|
| **Browser mount** | ✅ done. `tiller_ui/src/browser.rs`, 1283 lines: `BrowserState` (address, back/forward/reload/stop, navigation lifecycle, errors, permission allow/deny/revoke, agent-driving, link routing) and `BrowserSurface` with `Render` at `browser.rs:1011`. `codex11`, 2026-08-13. **6 refs in tree, 0 in app.** | 🔴 **`codex12`, brief `P83`.** Bigger than this row first said — see below. Mount `BrowserSurface`, remove **three `unreachable!()` panics** (`main.rs:2156` icon, `:2803` persistence, `:3904` width), invert **two tests** that pin the stubs (`main.rs:9781`, `:9817`), and route all ten `BROWSER_METHODS` at `main.rs:1668`. | all 9 `F-BRW` rows |
| **P71 notices** | ✅ `set_notice` exists, 11 references. | 🟡 **`codex12`** — partially done. **34 `eprintln!` remain** across `crates/tiller/src` + `crates/tiller_ui/src`. Each one is a failure only stderr sees. | the `F-*` rows whose evidence is "fails silently" |
| **Project forms mount** | ✅ done. `tiller_ui/src/project_forms.rs` — `CloneForm` and `CreateForm` with explicit states, double-submit guard, progress, visible errors and retry. `codex11`, P77, committed `da67e8c`. | 🔴 **`codex12`**, handoff verbatim: *"Clone: mount `CloneForm`, listen for `CloneFormEvent::Cloned(path)`, register/load the resulting project. Create: mount `CreateForm`, listen for `CreateFormEvent::Created(path)`, register/load the resulting project."* Plus turn the sidebar `+` (`sidebar.rs:795`, `start_add_project`) into three choices. | `F-PRJ-01`, and makes `F-PRJ-05/06/07/08/09/10` reachable |

## Closed, and worth recording as closed

- **`add_chat_tab` takes `&mut Window`** — `main.rs:4201`. Verified 2026-08-14. `OWNERSHIP.md` still
  listed this as open; it is not.
- **`tiller_markdown` → `file_view.rs`** — `file_view.rs:28` imports `Document`/`parse` and uses
  them. Verified 2026-08-14.

## Two more ways a feature stays dead — found 2026-08-14

Seams were the first mechanism, and the builder's own loop was the second. Auditing the browser mount
turned up two more, and both are worse because **the codebase actively defends its own
incompleteness.** Neither is visible from the ledger, and neither shows up as a failing test.

**A test that pins the stub in place.** `main.rs:9781` asserts that every one of the ten
`BROWSER_METHODS` *fails* with `"unsupported on Linux"`; `main.rs:9817` asserts capabilities
advertise none of them. **The suite is green precisely because the feature is absent.** A builder who
wires the browser and sees two tests go red has every reason to read that as a regression and revert
— the correct move is to invert the assertions, because the test encoded the stub as the contract.

**The shell asserting the feature is impossible.** Three `unreachable!()` calls stand between a
browser tab and the screen, one of them carrying the design claim *"Browser surfaces are external to
the shell"*. That claim is half true — the page pixels really are a native WebKitGTK child window —
and the half that is false, that the *tab* is external, panics on render, on layout and on save.

The lesson for estimating: **a reference count of zero tells you a seam is open, not what it costs to
close.** `SEAMS.md` guessed the browser mount at "often a dozen lines" on the strength of that count.
Before sizing a mount, grep the consuming file for the variant's own name — the panics and the tests
that forbid it are the actual work.

## The structural problem this exposes

**`codex12` owns `main.rs`, and `main.rs` is where nearly every Half B lands.** The ownership map
therefore makes one pane the integration bottleneck by construction, while the other builders
generate Half As faster than it can consume them.

Two consequences worth acting on rather than rediscovering:

1. **Batch the mounts.** Dispatching three seams as three briefs pays three context resets and three
   build cycles for what is often a dozen lines. Collect open Half Bs into one integration pass and
   list the rows each unblocks, so the piece stays individually judgeable per seam.
2. **A seam is cheap to cut and expensive to leave.** Before cutting one, ask whether the piece can
   be shaped so a single owner holds both halves — that is why `chat.rs`/`composer.rs` and
   `tiller_markdown`/`file_view.rs` were reassigned rather than seamed, and both stopped generating
   debt the moment they had one owner.
