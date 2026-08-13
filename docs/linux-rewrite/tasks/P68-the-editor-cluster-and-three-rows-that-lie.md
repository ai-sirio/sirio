# P68 — The editor cluster, starting with the three rows that are already built

**You are codex11, pane `w1:p2`.** Your context was just reset, so this brief is everything you need.

## P64 landed, and you disclosed the seam instead of hiding it

You built the diff surface — 1528 insertions, UI and terminal suites green, named drawn tests
(`a_conflict_terminal_is_drawn_with_the_exact_path`,
`a_drawn_terminal_receives_a_diff_payload_drop`). You also wrote: *"Seam codex12: sottoscrivere
RightPanelActionEvent e ChangesTabActionEvent in main.rs."*

That sentence was the most valuable line in the report. It was **verified true** at 21:15 — `main.rs`
subscribes only the old `ChangesTabEvent`/`RightPanelEvent`, so Open-diff and Resolve-in-terminal
currently fire into nothing and the feature is unreachable from the app. It is dispatched to
`codex12` as P67. Had you not named it, it would have been found by the critic as a dead model, or
worse, marked PASSED on the strength of your (entirely honest) green tests.

You were also **right about the gate**, and were contradicted in error: the orchestrator told other
agents `tiller_theme` clippy was clean, having run `cargo clippy -p tiller_theme` without
`-D warnings` — a weaker check than `ci-linux.sh:175`. You, `pi` and `codex12` all reported that red
and all three of you were correct. `sonnet` is fixing it.

## Start here: three rows in your own cluster are already built

`F-EDIT` has 13 rows, 11 marked `FAILED — absent`, **every verdict from pass 3 or 7.** Checked at
21:40 against a live positive control:

| row | ledger says | actually |
|---|---|---|
| `F-EDIT-04` | "no ⌘S binding" | `main.rs:116` — `(WindowCommand::SaveFile, "ctrl-s")` |
| `F-EDIT-06` | "no save path" | `editor.rs:641 pub fn save()`, `main.rs:5584 handle_save_file` |
| `F-EDIT-08` | "`add_file_tab` pushes unconditionally — no dedupe" | `main.rs:4001` collects `open_paths` and dedupes |

You found `F-EDIT-04` yourself in P61. `F-EDIT-06` is P61's own work. `F-EDIT-08` is `codex12`'s P63.

**Do not rebuild these.** Confirm each by exercising it, and report them as stale-FAILED candidates
for `pireview` — **only the critic changes a verdict**, so you produce the evidence, not the verdict.

Two things worth carrying forward from this:

- `F-EDIT-04`'s row says **⌘S**, and the Linux binding is correctly **`ctrl-s`**. A row written in
  the old platform's vocabulary defeats every search made in that vocabulary. Expect more of these.
- The reason these went stale is not bad wording, it is **the clock**: verdicts from pass 3/7, and
  builders shipped pieces in exactly this area afterwards. When you meet an old verdict in an area
  someone has since worked, check before building.

## Then build the rest of the cluster

Remaining, genuinely absent as far as anyone knows — **verify before building each one**, same as
above:

- `F-EDIT-01` Code/Preview switch (a P39 builder claim of a drawn switch was never verified)
- `F-EDIT-03` manual-preview state (currently a hardcoded 1 MiB notice)
- `F-EDIT-05` Reload/Keep banner for on-disk changes
- `F-EDIT-07` language detection — code renders as plain numbered lines
- `F-EDIT-02` formatting toolbar
- `F-EDIT-10` context menu on file rows
- `F-EDIT-11` copy-path

**Take them in that order** — the first four are the editor actually working; the last three are
chrome. If you run out of piece before you run out of list, that is fine and expected: stop, and say
where you stopped.

`F-EDIT-12` (drag) — the row notes the existing payload-drag fixture is **harness-only**. Take it
only if the machinery carries it, and **say plainly if it does not**. A drag that draws and drops
nothing is this project's most-produced defect class, and `codex12` correctly refused the same thing
in P65 rather than fake it.

## Evidence

Follow `docs/linux-rewrite/EVIDENCE-STANDARD.md`. **Named drawn tests** (`TestAppContext` /
`VisualTestContext`, `.debug_selector(id)`, full `run_until_parked()` pump).

For the Reload/Keep banner, the test that counts **changes the file on disk** and asserts the banner
appears and each button does its thing. For language detection, assert two different languages
produce different spans — a test that renders one file proves nothing. For the three stale rows,
your evidence is an *exercise*, not a grep.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.
- **Yours: `tiller_ui/src/editor.rs`, `file_view.rs`, `changes.rs`, `right_panel.rs`,
  `tiller_git/**`, `tiller_terminal/**`.**
- **Do not edit `main.rs`** — it is `codex12`'s and they are in it right now, wiring your P64 seam.
  If a row needs a `main.rs` change, name it in your report as a seam, exactly as you did for P64.
  That worked.
- **Do not edit** `chat.rs`, `settings.rs`, `sidebar.rs`, `status_bar.rs`, `tiller_agents/**` (`pi`);
  `tab_bar.rs`, `tiller_control/**` (`codex12`); `tiller_theme/**`, `controls.rs`, `titlebar.rs`,
  `composer.rs` (`sonnet`).
- **Standing rule:** whoever widens an enum owns every match arm it breaks, in any file — but only
  those arms.
- Colours, spacing and radii from `tiller_theme::Theme`, never a literal. The visual bar is **Pop!_OS
  COSMIC**, not waku. Name any token you need and lack; that list is how `sonnet` learns.
- **Establish the build state with the gate's own commands**, not a paraphrase of them —
  `grep -n clippy Scripts/ci-linux.sh` and run what it says. `sonnet` is mid-fix in `tiller_theme`;
  another agent's transient red is not a finding.
- **Never copy code from the reference checkouts.** Mark rows `builder-claimed, unverified`, never
  `PASSED`. **Proceed without asking for approval.**

## Reporting

**12 lines or fewer**: the three stale rows confirmed-by-exercise (and any fourth you find), which
`F-EDIT` rows you built and which you did not reach, whether `F-EDIT-12`'s drag was carried or
refused and why, tests by name, any `main.rs` seam named for `codex12`, the gate run with its own
invocation with not-yours failures named separately, tokens `tiller_theme` still lacks, and the
honest remainder.
