# SYNTHESIS — consolidated verdict across the full-app critic pass

Final synthesiser pass. Snapshot cutoff: **2026-08-19 21:00 CEST**, `linux/gpui-waku` HEAD
`e8941ece` (local branch tip; `origin/linux/gpui-waku` is 3 docs-only commits behind at
`8393d324`). Inputs: `docs/linux-rewrite/INVENTORY-LEDGER.md` (389 rows) plus every file
currently under `docs/linux-rewrite/fullapp/` — 28 section critic reports (one per ledger
section, together covering all 389 row ids exactly once, verified by id-set diff), 9
point-fix `CRITIC-*.md` reports judging specific commits, `VISUAL-BAR.md`, and
`ACP-HARNESS.md`. Other agents in this session (`critic-brw-close`, `critic-brw-unmap`,
`critic-omp-ui`, `critic-theme-2`, `critic-x11-forced`, `fullapp-critic`, `render-critic`,
`theme-transplant-critic`, `transplant-critic`) were still active while this synthesis was
written; `CRITIC-modal.md` appeared mid-pass (committed 20:51) and is included. Anything
committed after the cutoff above is not reflected here.

## 1. Finish-line verdict: **not met**

**364 of 389 rows (94%) were exercised live by this pass; 25 were not, and one section's
core question — does the Chat surface even render — is left as an open, unresolved
contradiction between two same-day critics.** The finish line requires the full-app critic
to tick *every* row by exercising it live. It did not: 25 rows carry an explicit `NOT
EXERCISED` verdict from their own assigned section critic (not silently carried over — each
one says so, with a reason), and a further 5 are `UNREACHABLE` this pass (attempted, blocked
with a stated reason — counted as exercised-with-reason per the ledger's own vocabulary, not
as a pass).

## 2. Headline numbers, checked against the rows

The brief handed me: **exercised 362, passed 261, failed 27.** Recomputed directly from the
389 per-row verdicts in the 28 section reports (one `id -> verdict` pull per report, cross-
checked against `01/02-inventory-*.md` row counts per section — every section's row count in
its own report matches the ledger's section header exactly, so there is no coverage gap in
the *set* of rows judged):

| metric | headline claimed | recomputed from rows | note |
|---|---|---|---|
| exercised (389 − NOT EXERCISED) | 362 | **364** | off by 2; UNREACHABLE (5) counted as exercised-with-reason either way |
| PASSED | 261 | **260** | includes one manual correction below (F-BRW-08) |
| FAILED (defective + absent) | 27 | **27** | matches exactly: 26 defective + 1 absent |
| half-proven | — | 65 | |
| N/A — platform | — | 7 | |
| NOT EXERCISED | — | 25 | |
| UNREACHABLE | — | 5 | |

389 = 260 + 65 + 27 + 7 + 25 + 5. The FAILED count is exact; PASSED and "exercised" are each
off by ~2 in the headline, small enough to be rounding/late-arriving-file drift, not a
material distortion — but per this task's own instruction, the row-level count is what's
recorded below, not the headline.

**The one manual correction**, applied because it is independently verifiable in the current
tree: `F-BRW.md`'s own row table still reads `half-proven` for `F-BRW-08`, but its own
Defects section carries a later inline annotation — *"RESOLVED 2026-08-19... case 3... by
`474f266a` (`CRITIC-brw-close.md`, CLEARED)"* — and `474f266a` **is** an ancestor of
`linux/gpui-waku` HEAD (`git merge-base --is-ancestor 474f266a HEAD` succeeds), and the fix
is genuinely in `browser.rs` (`flush_native_window_ops`, present, referencing "the `brw-close`
critic" by name). `F-BRW.md`'s table simply was never re-touched after its own later
annotation. Counted as **PASSED** above, not half-proven — the ledger has not caught up to
this either (still `half-proven`, citing the pre-fix defect).

## 3. Rows never exercised live this pass (25) — cluster: F-CHAT

All 25 are self-reported `NOT EXERCISED` by their own section critic (no inference on my
part). 23 of the 25 are one section:

| section | not-exercised rows |
|---|---|
| **F-CHAT** (23 of 37 rows, 62% of the section) | 02, 03, 05, 07, 11, 12, 13, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 32, 33, 36 |
| F-WIN (2 of 12) | 03, 10 |

`F-CHAT.md`'s critic spent its budget on a deep, high-value investigation of `F-CHAT-34`
(overturned PASSED→FAILED, see §6) and a real character-loss bug in the slash/mention popup
under fast typing (a genuine new defect, not in the row list) — and explicitly declined to
silently carry the ledger's prior evidence forward as if re-earned, writing `NOT EXERCISED`
with a one-line reason on each of the 23 instead. That is the right call procedurally, but
it means the *inventory's single largest section* has less than 40% direct re-confirmation
this pass. `F-WIN-03`/`F-WIN-10` are ordinary budget carryover, no stated concern attached.

## 4. Rows attempted but UNREACHABLE this pass (5)

| row | reason | note |
|---|---|---|
| `F-WIN-08` | no tray/StatusNotifierWatcher host in the headless sway lane — "reopen from Dock" half has nothing to click | matches the ledger's own existing half-proven framing; not new |
| `F-TAB-09` | this pass's session never got `xdg-desktop-portal-gtk` running (`Gtk-WARNING: cannot open display`) | **contradicts** the ledger's own PASSED (wave P, same day, using the documented "private D-Bus + portal-restart" recipe) — see §7, this reads as portal-setup variance between sessions, not a settled regression |
| `F-AGENT-OMP-01/02/03` | `F-AGENT.md`'s critic got the real crash against the binary it was handed (`/dev/shm/tt/debug/tiller`, built before the `oh-my-pi`→`omp` binary-name fix) | **already reconciled in the ledger itself** (commit `8393d324`, same day) to `half-proven`: the adapter fix is real and independently proven at the adapter level, but nobody has launched an omp pane from a rebuilt binary through the actual UI yet — a good model for how a disagreement should be resolved, cited here rather than re-litigated |

## 5. The unresolved, most consequential contradiction: does the Chat surface render at all?

Two same-day critic passes flatly disagree, and nothing in the source explains why:

- **`ACP-HARNESS.md` (09:00) and `F-CHAT.md` (13:22), both against `/dev/shm/tt/debug/tiller`:**
  real `claude` CLI launched via the ACP bridge, composer visible and editable, dozens of real
  turns sent and answered, streaming observed mid-word, `F-CHAT-01/04/08/37` (composer present
  at rest) all PASSED with screenshots.
- **`VISUAL-BAR.md` (20:29), same binary, same menu path (`+` → `New Chat` → `Claude Code`):**
  the resulting tab's content area is **totally blank** — no composer, no placeholder, no
  error, no caret under a direct click-and-type probe — reproduced across an 8s wait, a
  cumulative 45s+ wait, a full app restart (tab survived via `session.restore`, still blank),
  and both light and dark theme. The critic ruled out cold-npx-fetch (package already cached),
  a `PATH` gap for the spawned child, an auth problem (the same process's AI Providers page
  shows "Signed in" with live usage data, and a *raw-CLI* Claude Code terminal pane in the
  same session worked fine), and a known-fixed layout bug (P117 — confirmed still fixed, and
  a different code path). The app's own log contains **zero** lines mentioning `acp`/`chat`/the
  adapter for that tab, even though the tab-creation code unconditionally schedules a composer
  focus grab on the next frame.

`git log` between 13:22 and 20:29 touching `chat.rs`/`main.rs` shows exactly four commits —
X11-forcing, two browser-webview-unmap commits, and the omp binary-name fix — nothing that
plausibly touches `AgentChat` rendering. So this is not explained by a source regression
between the two passes. The one real difference: `VISUAL-BAR.md` itself documents extreme,
sustained host contention throughout its run (`uptime` load average 44–48, ~25 concurrent
`tiller` processes from sibling critics, 18 GiB swap in use) — a plausible, if unconfirmed,
explanation (a starved render pass silently dropping the composer's paint, or the ACP child
process failing to spawn under memory pressure with the failure never reaching the log).

**I am not resolving this in either direction** — averaging it away (e.g., quietly filing it
as "probably host load") is exactly the failure mode this task's own brief warns against.
Both reports are internally rigorous and both would be wrong to discard. It is named here as
open and unsettled, and it is the reason `F-CHAT`'s 23 NOT-EXERCISED rows in §3 matter more
than a simple budget-carryover count would suggest: until someone re-drives `+ → New Chat →
Claude Code` on an idle host and gets a clean, unambiguous answer, roughly a seventh of the
entire 389+14-row inventory (`F-CHAT`'s 37 rows plus the 14 ACP appendix rows, all of which
assume a rendering composer) rests on evidence that a same-day, same-binary critic could not
reproduce at all.

## 6. Fixes verified live — on branches that were never merged into `linux/gpui-waku`

This is the second major finding, independent of the chat question, and it is directly
verifiable from `git`, not from reading prose. Several point-fix critic reports describe a
commit as built, live-driven, and CLEARED/PASSED — but the commit is not an ancestor of this
worktree's `HEAD`. The docs-only report itself *is* on `linux/gpui-waku` (each critic commits
its own `.md`); the code fix it is praising is not:

| row(s) | report | fix commit(s) | lives on | `git merge-base --is-ancestor <fix> HEAD` |
|---|---|---|---|---|
| `F-TERM-05`, `F-TERM-08`, `F-TAB-26` | `CRITIC-modal.md` | `272ff995`, `23312e0c`, `e25c0d82` | `worktree-wf_29906a5b-fa0-1` / `builder/f-term-05-08-tab-26` | **NOT an ancestor** |
| `F-WIN-06`, `F-CORE-DOM-06` | `CRITIC-commands.md` | `fd0417f5`, `7f41e684`, `5e6c1715` | `wf3-rust-work` | **NOT an ancestor** |
| `F-CORE-FILE-01`, `F-CORE-FILE-08` | `CRITIC-files.md` | `c3b9e7ac`, `ec7e84eb` | `file-01-08-natsort-icons-b18`/`-v2` | **NOT an ancestor** |
| `F-CHG-06` | `CRITIC-staged-marker.md` | `0db25a26`, `98b91fb2` | `fchg06-staged-marker-3687962-4311` | **NOT an ancestor** |

I confirmed this two ways, not one: `git merge-base --is-ancestor`, and by reading the actual
current source. In this worktree, right now: `rust/crates/tiller_ui/src/modal.rs` **does not
exist**; `panes.rs`'s `jump()` (the clamp-to-last-tab bug `F-CORE-DOM-06` names) is **still
present**, unchanged; `DirectoryGitStatus` (`tiller_git/src/directory_status.rs`) **still has
only three variants** (`Conflicted, Changed, Untracked`, no `Staged`); no `FocusAddressBar`
action exists anywhere in `main.rs`; `file.rs`'s directory sort is **still** a plain
`.to_lowercase()` comparison, not natural/localized. Every one of the section critics' FAILED
verdicts for these 8 rows is therefore the accurate description of `linux/gpui-waku` today —
the point-fix reports are correct about the code they tested, but that code has not shipped
to the branch this whole inventory certifies.

**The converse also happened, once, correctly:** `F-BRW-08`'s fix (`382bf383`, `474f266a`)
*is* an ancestor of HEAD and *is* present in `browser.rs` — see the correction in §2. So this
is not a blanket "distrust every point-fix report" finding; it is a **merge-tracking gap**:
nothing in this process currently distinguishes "verified on a branch" from "verified on the
branch that ships," and four of five recent point-fix passes landed on the wrong side of that
line. One of the four unmerged branches has its own additional problem worth flagging to
whoever merges it: `CRITIC-staged-marker.md` found `cargo test -p tiller_ui` **fails to
compile** on `fchg06-staged-marker-3687962-4311` (`let cx` needs to be `let mut cx`,
`right_panel.rs:2827`) — a one-line fix, but as committed it silently breaks CI for the whole
crate, not just the new test.

## 7. Two critics disagree, not reconciled

- **§5 above** (Chat surface: renders vs. blank) — the largest, unresolved.
- **`F-TAB-09`**: this pass's critic hit `xdg-desktop-portal-gtk` failing to start
  (`Gtk-WARNING: cannot open display`) and returned UNREACHABLE; the ledger's own PASSED
  (same day, wave P) used a documented "private D-Bus + portal-restart" recipe successfully,
  and several *other* rows this same pass (`F-WIN-03`, `F-PRJ-02/03/14`) independently got the
  portal working via that recipe. Reads as portal-bring-up being session-fragile rather than
  the picker regressing — but it was not re-tried with the working recipe within this pass, so
  it stands as an open disagreement, not a resolved one.
- **`F-AGENT-OMP-01/02/03`** — cited in §4 as the model for how this *should* be done: the
  ledger itself already names both evidences, explains why they don't meet (adapter-level
  proof vs. a UI drive against a stale binary), and lands on `half-proven` rather than picking
  a side. No action needed here beyond noting it as the positive example.

## 8. Systematic ledger-vs-critic disagreements (67 rows, not counting §5/§6/NOT EXERCISED)

389 rows: 260 PASSED + 65 half-proven + 27 FAILED + 7 N/A + 25 NOT EXERCISED + 5 UNREACHABLE.
Of the rows with a verdict (359), the ledger's *currently recorded* verdict and this pass's
fresh critic verdict **disagree on 67** — meaning the ledger was never updated after the
section critic ran. Grouped by root cause, not flattened into 67 independent findings:

| cluster | rows | root cause |
|---|---|---|
| **F-CORE-ACT** | 25 of 27 | One harness event, not 25 app defects: 14 consecutive `wayland-drive.sh` boots failed after ~25 min with a GPU/EGL `ZINK: failed to choose pdev` error under host load average 30–39; the critic fell back to fresh unit-test replay + source-read (not "PASSED before") for everything past that point, correctly downgrading to `half-proven` rather than re-stamping. Two rows (09, 27) were fully live-driven and stayed PASSED. This is a genuine "not exercised live this pass" outcome wearing the label `half-proven` rather than `NOT EXERCISED` — either way it counts against the finish line. |
| **F-CTRL-PANEL-04/05/07/08/09** | 5 | One root cause, confirmed by live reproduction and a source citation: `panel.write`/`key`/`wait`/`focus`/`close` all route through a private `get()` helper that only checks the control-socket's own `panes` map, never the `external` map that holds UI-created panes — so none of the five work on a pane a human actually opened, only on one the socket itself created (which never appears as a visible tab). `close()` additionally no-ops silently instead of erroring. |
| **F-CTRL-BROWSER-01/03/04/05/06**, **F-CTRL-CLI-01**, **F-CTRL-NOTIFY-01/02/03** | 9 | Independent smaller findings in the same section: `system.capabilities`'s advertised `browser.*` set has quietly drifted from the row's documented set (swapped `screenshot` for `permission`); `tillerctl` has zero `browser` subcommands (every `browser.*` call must be hand-built raw socket JSON); `browser.wait`'s own `timeoutMs` above ~5s is unreachable because the dispatcher's hardcoded 5.0s action timeout fires first. See `F-CTRL.md` for the NOTIFY-group specifics. |
| **F-TERM-PTY-04/06/08**, **F-TERM-REG-01**, **F-TERM-SPLIT-01**, **F-TERM-UI-01** | 6 | Package-tier terminal findings, no single root cause; see `F-TERMpackage.md`. |
| **F-SET-13/18/22/24** | 4 | Re-judged half-proven vs. ledger's PASSED; see `F-SET.md`. |
| Scattered singles | 18 | `F-CORE-AUTH-01`, `F-CORE-SET-01`, `F-CORE-TERM-02/03`, `F-CORE-WSP-05/07`, `F-GIT-DIFF-02`, `F-PERSIST-DB-03/08`, `F-PRJ-02/03/14`, `F-TAB-23/24`, `F-WIN-06/09`, `F-CHAT-06/31` — each independent, see the named section report. |

`F-WIN-06` deserves one clarifying note: two *different* defects were found under the same
row id at different times of day. `F-WIN.md` (11:30) found the embedded browser page could
never render at all (`Direct XCB build failed`) — but that specific blocker was fixed and
merged at 16:14 (`7d4d2182`, confirmed ancestor of HEAD). The ledger's own row (written after
that fix, referencing `critic-x11-forced.md`) correctly moved on to the *next* problem: the
New-Browser/Focus-Address-Bar commands don't exist at all — `FAILED — absent`, not
`defective`. A fix for *that* exists (`CRITIC-commands.md`) but is one of the four unmerged
branches in §6. Net: `F-WIN-06` is still FAILED today, just not for the reason any single
report states in isolation — the full picture only exists by reading all three in commit
order.

## 9. All 27 FAILED rows (this pass's verdict), for reference

`F-CHAT-34`, `F-CHG-06`, `F-CORE-AUTH-01`, `F-CORE-DOM-01`, `F-CORE-DOM-02`, `F-CORE-DOM-06`,
`F-CORE-FILE-01`, `F-CORE-FILE-08`, `F-CORE-SET-01`, `F-CORE-TERM-02`, `F-CTRL-PANEL-04/05/07/08/09`,
`F-PER-01`, `F-PER-03`, `F-PER-04`, `F-PER-06`, `F-PERSIST-DB-08`, `F-TAB-12`, `F-TAB-26`,
`F-TERM-05`, `F-TERM-08`, `F-TERM-REG-01`, `F-TERM-UI-01`, `F-WIN-06`. (`F-CORE-DOM-01/02` were
already FAILED in both the ledger and this pass — no disagreement, listed here only for
completeness of the FAILED set.) Four of these (`F-CHG-06`, `F-CORE-FILE-01/08`,
`F-TERM-05/08`, `F-TAB-26`, `F-WIN-06`'s successor problem) have a real, live-verified fix
sitting unmerged — see §6.

## 10. The single largest gap

**Whether the ACP-backed Chat surface — the inventory's single largest cluster at 37 rows
plus 14 appendix rows, about an eighth of everything tracked — renders at all is an open,
unresolved contradiction between two same-day critic passes (`F-CHAT.md`/`ACP-HARNESS.md`:
works, dozens of real turns; `VISUAL-BAR.md`, hours later under heavy host load: composer
never appears, reproduced across a restart and both themes), and 23 of that section's 37 rows
were never independently re-driven this pass to help settle it — so a builder's very first
action should be a clean `+ → New Chat → Claude Code` drive on an otherwise-idle host, before
touching anything else, because every other Chat finding in this inventory is downstream of
that one question.**

Close behind, and independently actionable regardless of how §10's question resolves: **four
real, live-verified fixes for `F-TERM-05`, `F-TERM-08`, `F-TAB-26`, `F-WIN-06`,
`F-CORE-DOM-06`, `F-CORE-FILE-01`, `F-CORE-FILE-08`, and `F-CHG-06` exist today but were never
merged into `linux/gpui-waku`** — merging `builder/f-term-05-08-tab-26`, `wf3-rust-work`,
`file-01-08-natsort-icons-v2`, and `fchg06-staged-marker-3687962-4311` (fixing the one-line
`tiller_ui` test-compile break on the last one first) would clear 8 FAILED rows for free,
with the verification work already done and written up.

## 11. What a builder should do next, in order

1. Re-drive the Chat surface on an idle host (§5/§10) and get an unambiguous answer — this
   gates confidence in ~51 rows.
2. Merge the four branches in §6, fixing the `tiller_ui` compile break first — clears 8
   FAILED rows with zero new investigation.
3. Fix the one real, un-forked root cause in `F-CTRL-PANEL-04/05/07/08/09`: make `get()`
   check the `external` pane map (§8).
4. Re-drive the 23 `F-CHAT` NOT-EXERCISED rows and the F-CORE-ACT rows blocked by host
   contention (§3, §8) on an idle host — both are budget/environment carryovers, not known
   defects, but neither is proven either.
5. Sync the ledger to this pass's 67 disagreements (§8) so the next critic starts from the
   real state instead of re-discovering it.
