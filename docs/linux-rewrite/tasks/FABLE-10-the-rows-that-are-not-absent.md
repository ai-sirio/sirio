# FABLE-10 — Recipes for the 33 rows that are built but unproven

**This brief is everything you need; your context may have just reset.**

## Why this block and not another

The ledger is 389 rows. Measured just now by parsing column 2, not by grepping the file:

| verdict | rows |
|---|---|
| PASSED | 188 |
| FAILED — absent | 131 |
| N/A — platform | 22 |
| half-proven | 13 |
| NOT EXERCISED | 12 |
| FAILED — defective | 8 |
| builder-claimed, unverified | 8 |
| UNREACHABLE | 7 |

Sums to 389 exactly. (Watch the ID shape: `F-CORE-FILE-03A` ends in a letter, and a regex anchored
on `[0-9]+$` silently drops it — that is how a 389 becomes a 388.)

**`half-proven` + `NOT EXERCISED` + `builder-claimed, unverified` = 33 rows.** These are the cheapest
PASSED conversions on the board, and they are cheap for a reason that is different from the 39 in
`STALE-FAILED-RECIPES.md`: those 39 were mis-filed as absent. **These 33 are not mis-filed. They are
correctly filed as unproven** — something exists, and nobody has driven it. The 39 needed a census to
discover; these need only a route.

You did FABLE-09 for the other block. Same output shape, same rules, different input.

## The 33, already clustered by what a single launch could cover

**Cluster A — activity detection core, 8 rows, all `builder-claimed, unverified`**
`F-CORE-ACT-05 06 07 08 09 10 11 27`

The whole layered-evidence model. Note `F-CORE-ACT-12/13/15` were *overturned from UNREACHABLE to
PASSED in pass 10* on the grounds of "pure classification, every clause case unit-tested" — so for
this cluster, establish first whether the same argument applies. If a row is pure classification with
named unit tests, the recipe is a test name to replay, not a UI route, and you should say so. If it
needs a live pane transition, it needs a live pane.

**Cluster B — settings surface, 7 rows, all `half-proven`**
`F-SET-04 05 06 07 10 19 20`

Half-proven means one half was driven and one was not. **Your recipe must say which half is missing**,
otherwise the critic re-drives the half that already passed and the row does not move. `F-SET-04`'s
across-relaunch half is known to depend on the P58 hand-off to `codex11` — you already flagged that in
FABLE-09; carry the flag forward rather than rediscovering it. `F-SET-05` is the row that was hiding
behind a mis-shaped table cell (see `INVENTORY-LEDGER.md:569`) — check its row renders as four columns
before trusting its text.

**Cluster C — chat surface, 8 rows, all `NOT EXERCISED`**
`F-CHAT-03 05 13 15 20 29 30 33`

These need a live ACP agent. Two things changed in the last hour that make this cluster newly cheap:
the build was down and is now fixed, and `pireview` proved in pass 14 that the picker → Codex →
composer → Enter path works end to end for both Claude and Codex. So the expensive part — getting a
real agent talking — is a solved, repeatable prelude. Write these recipes to **share one agent
session**, and order them so the session is opened once.

One known obstacle to state in the cluster header, not in every recipe: the composer does not
auto-focus on tab creation. `add_chat_tab` has no `&mut Window` parameter, so creation structurally
cannot focus anything — GPUI focus requires it. The critic must click the composer first. That is a
real open defect in `pi`'s file, not a critic error, and a recipe that omits the click will read as a
dead feature.

**Cluster D — terminal, 5 rows**
`F-TERM-04 06` (NOT EXERCISED), `F-TERM-09` (half-proven), `F-TERM-UI-01` (NOT EXERCISED),
`F-CORE-TERM-02` (NOT EXERCISED)

**Cluster E — the five that share nothing**
`F-AGENT-SAFE-01` (half-proven — you already found in FABLE-09 that `agent_skill_install_command` has
no UI consumer because the Copy-install-command control is itself the absent row `F-SET-08`; if that
still holds, say so and mark it not-UI-exercisable rather than inventing a path),
`F-CHG-06`, `F-CORE-ACT-02`, `F-CTRL-NOTIFY-03`, `F-SID-12`.

## The rules that made FABLE-09 usable — unchanged

- **A recipe says what to DO, never what to CONCLUDE.** "Open the + menu, click New Terminal, report
  what the tab strip shows" is a recipe. "Verify the tab renders" is a verdict, and verdicts are
  `pireview`'s alone. You wrote that rule into `STALE-FAILED-RECIPES.md`; keep it.
- **1–2 lines each**: starting state, action, element to look at.
- **Cost the clusters.** Say how many app launches the whole doc needs and what each launch covers.
  That number is the reason the doc exists.
- **Re-verify the risky doors in the snapshot before writing them**, exactly as you did for the
  palette (`ctrl-k`/`ctrl-shift-p`) — do not trust the ledger's own prose for a route.
- **Never assign a verdict, and open the doc by saying so.**

## Two honesty requirements specific to this block

1. **A `builder-claimed, unverified` row is not a PASSED row with missing paperwork.** It is a claim.
   If your reading of the code says the claim looks wrong, the recipe still only routes the critic
   there — but flag it in the cluster header as worth extra attention, and say why.
2. **If a row cannot be exercised from the UI at all, say that instead of inventing a route.** You did
   this for 2 of 39 in FABLE-09 and it was the most useful part of the doc, because an invented route
   costs the critic a whole launch to discover it was fictional.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`. **Your
  pane may start in the main repo on `rust/gpui-rewrite` — `cd` first.**
- **Yours: one new file**, `docs/linux-rewrite/UNPROVEN-ROWS-RECIPES.md`. Touch no source. Do not edit
  `INVENTORY-LEDGER.md` — only `pireview` changes a verdict, and this doc changes none.
- The build is green as of now: `cargo build -p tiller -p tiller_control` = EXIT 0, `cargo clippy`
  with the gate's exact stage-175 invocation = EXIT 0. The only red stage is `fmt`, one file,
  `tiller_ui/src/browser.rs`, `codex11`'s and routed to them. **Nothing in your way.**
- Commit one artefact, conventional-commit subject. **Proceed without asking for approval.**

## Reporting

**10 lines or fewer**: the cluster costing (launches, and what each covers), which rows you found
not-UI-exercisable and why, any row whose `builder-claimed` text you think is wrong and the reason,
whether `F-SET-05`'s row is well-formed, and the honest remainder.
