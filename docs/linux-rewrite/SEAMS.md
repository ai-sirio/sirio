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
| ~~**Browser permissions settings**~~ | **CLOSED BY REASSIGNMENT, 2026-08-14.** `settings.rs` moved from `sonnet` to `codex11`, which already owns `browser.rs` and the v11 grant table. Both halves now sit with one owner, so there is no handoff left to schedule — only work. Still to build: the Permissions section listing and revoking grants, keeping the existing doorhanger path. | `F-BRW-08` |
| **P71 notices** | ✅ `set_notice` exists, 11 references. | 🟡 **`codex12`** — partially done. **34 `eprintln!` remain** across `crates/tiller/src` + `crates/tiller_ui/src`. Each one is a failure only stderr sees. | the `F-*` rows whose evidence is "fails silently" |
| **Terminal pane composition** | ✅ `tiller_terminal/src/lifecycle.rs` — `TerminalSurfaceHost` and `TerminalPaneCache<T>` preserve terminal content identity, PTY controllers and focus when a leaf moves; `TerminalView::empty_prompt()` is also mountable. `codex11`, P82. | 🔴 **`codex12`** — when composing the recursive pane tree, retain one cache per worktree, call `move_within_worktree` rather than recreating a moved leaf, restore focus by content ID, and apply the existing split divider/minimum-size change in `main.rs`. | `F-TERM-PTY-07`, `F-TERM-PTY-08`, `F-TERM-SPLIT-01` |
| **No-worktree terminal empty state** | ✅ `TerminalView::empty_prompt()` renders the two actions and emits `TerminalPromptEvent`; `tiller_terminal/**`, `codex11`, P82. | 🔴 **`codex12`** — mount the prompt when no worktree is selected and handle `NewTerminal` / `NewTerminalWithCommand` in the app-level tab/pane owner. | `F-TERM-02`, `F-TERM-11` |
| **Project icon persistence** | ✅ `ProjectRecord` has `icon_kind`/`icon_value`, but the app catalog model currently drops the picker value. | 🔴 **`codex11`** — carry `ProjectIcon` through the persistence/domain boundary and add the durable update/reload path; do not make the sidebar reach into `tiller_persistence`. **Confirmed live 2026-08-14** by the orchestrator, and it is worse than "not durable": the value is dropped **in-session too.** Selecting the git-branch glyph and a green tint re-rendered them inside the picker, but after `Close` the sidebar project row still showed the orange folder (`orch18-picked.png` → `orch18-sidebar.png`). So this is **two** gaps, not one — an unwired `on_change` between picker and project row, and no durable store behind it. Closing only the persistence half leaves the picker still visibly inert, and `F-PRJ-13`/`F-PRJ-15` are `FAILED — defective` on the propagation half alone. | `F-PER-07`, and the propagation half of `F-PRJ-13`/`F-PRJ-15` |
| **Avatar network fetch (GitHub / favicon)** | ✅ done. `tiller_ui/src/project_identity.rs`'s Avatar tab validates and commits `AvatarSource::GitHub(identifier)` / `AvatarSource::Favicon(domain)` as descriptors — no HTTP client in the UI crate, no image bytes. `sonnet`, P80. | ⚪ **DEFERRED by decision, 2026-08-14 — not unowned, not forgotten.** It is *half* of one row whose other half already passes, and closing it means adding the project's first HTTP client to a crate that has none. That is a real dependency and a real attack surface bought for the least value of anything outstanding. Revisit when the ledger is otherwise drained; if it is ever dispatched, `tiller_project/**` (`codex11`) is the least-bad home. Recorded here so a later reader sees a decision rather than an oversight. Was: 🔴 owner not yet assigned. This is network I/O, not a `tiller_ui` concern, and does not obviously belong to any existing crate (`tiller_git`/`tiller_project`/`tiller_usage` are the closest neighbours but none fetch arbitrary URLs today) — worth a decision before it is dispatched as a brief, not an assumption. Needs: fetch a GitHub avatar or a site's favicon, decode it, and feed the bytes back through the picker's value. | the avatar-image half of `F-PRJ-14` (the local-PNG half is already closed above) |

## P82 platform ruling

For `F-TERM-UI-02`, Linux uses GPUI's `platform` modifier for the Super key;
macOS uses the same bit for Command. The terminal link router therefore accepts
the platform modifier, and emits a link event to the clicked terminal's identity
instead of opening a global URL. No Control-click fallback is added: Control is
reserved for terminal input semantics and would make the Linux gesture diverge
from GPUI's platform convention.

## Closed, and worth recording as closed

- **Browser mount** — `BrowserSurface` is exported and mounted as a real `TabContent::Browser`: GPUI chrome surrounds the native WebKit child, the tab opens from the palette/menu, routes the ten browser control methods, persists as `browser`, restores at launch, and persists allowed origins through the v11 store. `codex12`, P83, verified 2026-08-14. Rows `F-BRW-01` through `F-BRW-07` and `F-BRW-09`.
- **Project forms and sidebar add menu** — `CloneForm` and `CreateForm` are mounted by `sidebar.rs`; their completion events emit `SidebarEvent::AddProject`, and the owned `+` control offers Open, Clone, and Create. `codex12`, P83, verified 2026-08-14. Rows `F-PRJ-01`, `F-PRJ-05`, `F-PRJ-06`, `F-PRJ-07`, `F-PRJ-08`, `F-PRJ-09`, and `F-PRJ-10`.
- **Terminal divider width** — `SEAM_WIDTH` is 6px and remains the single input to the divider and both width subtractions. `codex12`, P83, verified 2026-08-14. Row `F-TERM-SPLIT-01`'s divider-width half.
- **Project icon picker mount** — `ProjectIconPicker` is mounted in `sidebar.rs::render_project_settings`; its `on_change(ProjectIcon)` callback writes the selected value into the settings card's host state. Durable persistence remains the open `F-PER-07` seam above. `codex12`, P83, verified 2026-08-14. Rows `F-PRJ-13`, `F-PRJ-14` local-PNG half, `F-PRJ-15`, and `F-PRJ-16`.
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
