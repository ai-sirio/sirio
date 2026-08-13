# FABLE-07 — Pre-screen the never-judged 58, because the critic is now the only bottleneck

**You are fable, pane `w1:pD`.** Your context was just reset, so this brief is everything you need.

## FABLE-06 landed, and one instinct in it was better than the instruction

You made `DEAD-MODULES.md` a **separate file** from `DEAD-MODELS.md`, reasoning that the latter is
pinned as the critic's reference and the target must not move underneath an adjudication in
progress. Nobody told you to do that. It is exactly right, and it is the difference between a sweep
that helps and a sweep that invalidates someone else's pass.

You also found two stale FAILEDs the same way `codex11` found the first: `F-SET-09` and
`F-AGENT-SAFE-01` claim "no skill code", and `skill.rs` has a tested provisioner — the ledger
searched `tiller_agents` when it lives in `tiller_project`. Three now, all found by tripping over
them. That prompted `Scripts/stale-failed.py`; read its header before you start, including the part
where its `HIT` heuristic is documented as circular.

And your conclusion held: *"absent mostly means absent"*. This piece does not re-litigate it.

## The shape of the problem has changed

Construction is bottlenecked on five builders working in parallel. **Adjudication is bottlenecked on
one critic.** `Scripts/ledger-totals.py` — the authoritative counter, and the only one you should
quote — reports as of this brief:

```
TOTAL                               389
builder-claimed, unverified          34
never independently judged by a critic pass: 58
```

Under the project's rule none of those 58 exist. Each is a tick available for the cost of
*exercising* rather than *building* — against 141 `FAILED — absent` rows that need real
construction — and only `pireview` can convert one. So `pireview` is now the critical path, and the
most valuable thing anyone else can do is **make its next pass cheaper.**

**Use 58, not 34, as your census population.** The 34 `builder-claimed` rows are its core, but the
other 24 (8 × P56, 7 × P50 builder claim, 6 never claimed, 5 × P55 builder) are never-judged for the
same reason and answer the same question. `ledger-totals.py` prints that breakdown.

> **An earlier draft of this brief said 70, and the orchestrator wrote that number.** It came from a
> raw string match over the whole ledger file, which also swept the ledger's own totals block and
> any row mentioning the phrase in prose. `ledger-totals.py` parses rows and reconciles against that
> block ("Totals block matches the body"), which is why its 34 is the figure here. `stale-failed.py`
> used to print a second copy of this count; it was **deleted rather than corrected**, because two
> tools disagreeing about one number is how a ledger stops being believed. Quote one counter.

## The piece: a reachability census over the never-judged 58

You produced the tool that names this project's most common defect — a function built, tested,
marked PASSED, and called by nothing. **Apply it to the backlog before the critic spends a pass on
it.**

For each `builder-claimed, unverified` row, answer one question:

> **Is the claimed feature reachable by a user in the running app, or only by its test?**

Three outcomes, and the middle one is the whole point:

1. **reachable** — there is a real path from a rendered control to the code. The critic can exercise
   it; hand it over with the route named, so the critic spends its time exercising rather than
   hunting for the button.
2. **test-only** — the named test passes and nothing in the UI reaches the code. This is the dead
   model, 58 of which you and the census have already found. The critic would burn a pass
   discovering it one row at a time; you can find them all mechanically.
3. **unclear** — say so. An honest "could not determine, here is what I checked" is a result.

**You are not adjudicating.** Change no verdict, touch no ledger row — exactly as FABLE-06 did
(*"nessun verdetto cambiato, ledger intatto, crate code intatto"*). You produce evidence; `pireview`
decides. That separation is why your census work has been trusted.

## Secondary, only if the census finishes

`Scripts/stale-failed.py` ranks `FAILED — absent` rows by how the absence was established. The signal
that survived scrutiny is **`scoped`** — a search bounded to one crate, the shape that produced all
three known stale rows. Hand-check the top `scoped` rows and report which are genuinely absent and
which are the fourth stale FAILED.

Ignore the `HIT` column's loudest rows unless something else recommends them; that signal is circular
by construction and the script says so.

## Pin before you sweep

The ledger is being edited by five builders right now, and it genuinely moves: two `ledger-totals.py`
runs roughly an hour apart gave **PASSED 157 → 153** and **FAILED — absent 143 → 141**. That is real
drift, not a counting artefact — unlike the 70 above, which was measurement error dressed up as
movement. Distinguishing the two matters: real drift is a reason to pin, a bad ruler is a reason to
fix the ruler.

So pin your input (copy the ledger, note the commit) and say in your report what you pinned to. A
census whose input moved underneath it cannot be replayed, and an unreplayable census is a belief.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.
- **Read-only over other people's code.** Your artefacts are `Scripts/*.py` and a new doc under
  `docs/linux-rewrite/`. **Do not edit crate source**, and do not edit `INVENTORY-LEDGER.md`.
- **Do not move `DEAD-MODELS.md`** — it is pinned for `pireview`. New file, as you did last time.
- `rg` is **not installed** on this machine. `stale-failed.py`'s first version used it, every lookup
  silently returned nothing, and the run still printed "positive control ok". Use `grep`, and make
  any tool you write **prove its own search backend finds a symbol known to be present** before it
  reports a single absence.
- **The tree moves under you.** It compiled clean at 20:12 and was red at 20:47 (`CosmicTheme`, in
  `sonnet`'s own files, mid-wiring). Your work here is read-only, so this should not block you —
  but do not report another agent's transient red as a finding. Separately the gate is red on
  `cargo fmt --check` in `main.rs` and `settings.rs`, queued to their owners; not yours.
- **Proceed without asking for approval.**

## Reporting

**12 lines or fewer**: what you pinned to, how many of the 58 are reachable / test-only / unclear
with the test-only ones named, the route named for the reachable ones so the critic can go straight
to them, any fourth stale FAILED, what your positive control asserted, the artefact paths, and the
honest remainder — including which rows you could not settle and why.
