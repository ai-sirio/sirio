# P65 — Finish the tab surface, and close two debts you have now carried twice

**You are codex12, pane `w1:p3`.** Your context was just reset, so this brief is everything you need.

## P63 landed the piece the whole project's finish line runs through

The goal's acceptance test is that the critic **connects a real workspace agent over ACP, sends
messages, and verifies streaming and replies**. Until P63 there was nothing to choose — New Chat made
a generic tab. Now the picker offers the catalog, `discover_availability()` gates it so no menu item
promises something that cannot work, and the chosen adapter reaches the tab as title, icon and
`agent_id`. That last part is what makes the acceptance test expressible at all.

You also gated the fallback honestly — "Other agents…" plus "No supported agent found on PATH"
rather than an empty menu that looks broken — and landed three one-line truths: `TILLER_SOCKET_ENABLE`
now observed on the boot path, the `add_file_tab` dedupe, and the `main.rs:3026` mapping that
completes `F-CHG-22` with `codex11`'s panel-side half. That mapping was where the fifth status died.

## Two debts, both now carried across two passes

Neither is a criticism — you reported both plainly, which is why they are still visible. But a debt
reported twice and fixed neither time starts to look like a permanent feature of the ledger.

### 1. ⌘W fails on a missing PTY workdir

P60: "not verifiable after concurrent drift." P63: "fallisce ancora per workdir PTY mancante; non
modificato."

**Settle it this pass.** Either it is a test-harness gap (the test does not give the PTY a workdir)
or it is a real defect (the app does not set one). Those are very different verdicts and the ledger
currently records neither. Find out which, fix the one that is broken, and say which it was.

### 2. Twenty-one failures you attributed to "concurrent/zbus/PTY, not mine"

That attribution is probably right, and you were right to name them separately rather than swallow
them. But "not mine" is a claim nobody has checked, and this project has been bitten twice this week
by a plausible attribution that was wrong — the `save_projects` scare and the `tiller_git`
three-parallel-APIs scare were both confident readings that did not survive verification.

**Triage them into three buckets and name the counts**: genuinely environmental (no D-Bus session, no
PTY, no display), genuinely another agent's live edit, and *anything left over*. The leftovers are
what matter. If the answer is "all 21 are environmental", that is a fine result — but it should be a
measured one, and it means the suite cannot be green on this machine, which is worth knowing before
anyone treats a green suite as the gate.

## The piece: finish the tab surface

`F-TAB-02` (All-Tabs overflow), `F-TAB-09` (Open File), `F-TAB-25`, `F-TAB-27`. These were held back
from P60 and P63 as their own piece; this is that piece. `tab_bar.rs` and `main.rs` are both yours,
so there is no seam to negotiate.

`F-TAB-18`/`F-TAB-24` (drag) — take them only if the machinery from the others carries them, and
**say plainly if it does not**. A drag that draws but drops nothing is this project's most-produced
defect class.

## Your own fmt drift

`Scripts/ci-linux.sh` is red on `cargo fmt --check`, and `crates/tiller/src/main.rs` is yours — 7
sites, plus `session.rs` which you also named. `settings.rs` belongs to `pi` and is queued to them.

**Run `cargo fmt` on your own files only.** Do not format the workspace: it rewrites files other
agents have open right now and their next write lands on top of it. Closing your half is what makes
the gate reachable at all.

## COSMIC

The visual bar is **Pop!_OS COSMIC**, not waku. `sonnet` is wiring the token layer into
`tiller_theme::Theme` and converting `controls.rs`, the shared widget vocabulary behind ~80 call
sites — so the vocabulary you build tabs from is changing underneath you this pass.

Take colours, spacing and radii from `tiller_theme::Theme`, **never a literal**. Your menu-width and
geometric-hairline gaps are recorded and still open; `sonnet` has them. Name any new gap the same
way — that list is the only channel by which `sonnet` learns what to build.

## Evidence

Follow `docs/linux-rewrite/EVIDENCE-STANDARD.md`. **Named drawn tests** (`TestAppContext` /
`VisualTestContext`, `.debug_selector(id)`, full `run_until_parked()` pump).

For overflow, the test that counts opens **more tabs than fit** and asserts the overflow surface
lists the hidden ones — a test that renders three tabs proves nothing about overflow. For Open File,
assert a file actually opens in a tab.

**Establish the build state yourself.** The workspace compiled clean at 20:12 and was red at 20:47
(`CosmicTheme`, in `sonnet`'s own files, mid-wiring). It moves in and out of red on a timescale of
minutes with five builders live. Another agent's transient red is not a finding — it has been
mistaken for one three times today. Re-run before reporting, and if still red, name the crate and
its owner rather than the symptom.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.
- **Yours: `tiller/src/main.rs`, `tiller_control/**`, `tiller_ui/src/tab_bar.rs`.**
- **Do not edit** `settings.rs`, `sidebar.rs`, `chat.rs`, `status_bar.rs` (`pi`), `changes.rs`,
  `right_panel.rs`, `editor.rs`, `file_view.rs`, `tiller_git/**`, `tiller_terminal/**` (`codex11`),
  `tiller_theme/**`, `controls.rs`, `titlebar.rs`, `composer.rs` (`sonnet`).
- **Standing rule:** whoever widens an enum owns every match arm it breaks, in any file — but only
  those arms.
- **Never copy code from the reference checkouts.** Mark rows `builder-claimed, unverified`, never
  `PASSED`. **Proceed without asking for approval.**

## Reporting

**12 lines or fewer**: overflow / Open File / `F-TAB-25` / `F-TAB-27` and whether drag fitted,
**⌘W settled as harness-gap or real defect and which**, the 21 failures triaged into three buckets
with counts and any leftover named, whether your fmt half is clean, tokens `tiller_theme` still
lacks, tests by name, the gate with not-yours failures named separately, and the honest remainder.
