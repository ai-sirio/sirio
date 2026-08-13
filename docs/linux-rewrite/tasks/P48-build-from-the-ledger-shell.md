# P48 — Build from the ledger: the absent rows in the shell and control crates

**You are codex12, pane `w1:p3`.** Your context was just reset, so this brief is everything you
need. **This brief is re-dispatchable**: each time you receive it, read the ledger, take the next
block, and stop. It is not a one-off.

## P46 closed the sidebar block and took the routed warnings with it

Typed context actions for `F-SID-07/08/09/12/14` **with disabled-reason types for Git
initialisation** — reusing the `F-TAB-11` pattern rather than inventing a second scheme, which is
what keeps a greyed menu item from being indistinguishable from a bug. Git init, project refresh,
primary-worktree persistence, and `xdg-open` reveal. Parked visual and event tests. The contract file
for pi updated with selectors and eligibility, so the affordance half can be built without a
conversation.

And `tiller/src/main.rs` **now emits no clippy diagnostics** — you cleared the routed warnings
without being asked twice. The gate is strict now; that is what keeps it green for four agents.

You also said *"no disagreement with the critic on the named missing entries"*, having checked
rather than inherited. **`F-WIN 02/03/04/05` is still open** — you scoped it out deliberately and
said so, which is the right way to leave something.

## Where the project stands

`docs/linux-rewrite/INVENTORY-LEDGER.md` holds one row per entry for all 388, with verdict, evidence
and the pass that judged it. Roughly: **89 PASSED · 89 FAILED — absent · 142 NOT EXERCISED**, and
the display gates only **7**.

The critic is converting `NOT EXERCISED` into verdicts. **The builders take the absent rows.**

## The piece

**Read the ledger. Take the `FAILED — absent` rows that fall in crates you own. Build them.**

Yours: `tiller_control/**`, `tiller/src/main.rs`, `tiller_git/**`, project discovery.
**Not** `tiller_ui/**` (pi — chat surface, and the menu affordances from your contract), and **not**
`tiller_terminal`, `panes.rs`, `tiller_activity`, `tiller_persistence`, `tiller_project`,
`tiller_usage`, `tiller_markdown`, `tiller_acp`, `tiller_agents` (codex11).

**`F-WIN 02/03/04/05` is the obvious first block** — it is what P46 scoped out, and it belongs to the
command layer you built. Note the platform question and answer it explicitly: macOS drives these from
an application menu bar; Linux conventions differ, so a transliteration would be wrong. **The
capability is the contract; the chrome that exposes it is a judgement — make it and say why.**

After that, choose **the largest coherent block you own**, not one entry from each corner. A group
that shares machinery gets built once and verified together; codex11 demonstrated this with three
entries sharing filesystem and session machinery that came out as one piece rather than three.

For each entry, all three questions must end up answered:

1. **It exists now.**
2. **It is exercisable here** — and where the only route to a user is a surface you do not own, add
   it to the contract file rather than declaring the entry done. Nineteen entries sat "built and
   unreachable" before anyone noticed they shared one missing menu layer.
3. **It does what the `VERIFY` clause says**, not what it plausibly does.

**Update the ledger row when you finish one, marked `builder-claimed, unverified` — never
`PASSED`.** Only the critic's verdict counts toward done. That distinction is what caught the
21-entry mega-row, the `F-PER-06` proof that was true and one process too narrow, and
`F-CHAT-24/25`, where the mechanism passed and the feature did not exist.

## Rules

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  The Swift original named in each `SRC:` field is the reference for **behaviour** only.
- **A capability nobody exercised does not exist**, and *exercised narrowly* is its own failure.
- **Keep the gate green** — it is strict now, and a warning you introduce is a red gate for everyone.
- Behaviour is provable with no display: `TestAppContext`, `VisualTestContext`,
  `.debug_selector(id)`, `simulate_keystrokes`. **Harden every drawn test** with the full
  `run_until_parked()`; an unhardened one does not count as evidence.
- Proceed without asking for design approval.

## Reporting

Reply in **12 lines or fewer**: which ledger rows you selected and why, what you built, the evidence
per entry, how you resolved the menu-bar platform question, what went into the contract file, the
gate result, and the honest remainder — including how many absent rows are left in your crates.
