# FINISH-leftovers — settings / activity / persistence leftovers (wf-rest3)

Fresh critic pass, lane label `wf-rest3`. Predecessor on this exact shard was killed by an API
error having committed nothing — this file starts clean. Binary pinned once from HEAD at the start
of this pass:

```
git rev-parse HEAD          # 960de6722aafac6b572f00872fc30b155841910a
cargo build --manifest-path rust/Cargo.toml --workspace   # exit 0, warm
cp rust/target/debug/tiller /tmp/wf-rest3-tiller
export TILLER_WL_BIN=/tmp/wf-rest3-tiller
```

Committing after every row per the brief. Rows below are filled in incrementally — a row present
here with a verdict is finished; anything still owed is listed at the end as NOT EXERCISED.

---

## F-SET-15 — select System default vs. a second account, active badge moves

**Confirming the orchestrator's reclassification, not re-deriving it independently — the brief
asked for exactly this.** Re-read `rust/crates/tiller_ui/src/settings.rs` fresh this pass:

```
$ grep -n "account_row\|selected_account\|active_account\|account_selection" \
    rust/crates/tiller/src/main.rs rust/crates/tiller_ui/src/settings.rs
tiller_ui/src/settings.rs:2445:            .child(controls::account_row(
```

Exactly one call site (`settings.rs:2445`), hardcoded `"System default"` / `true` — the whole
"Accounts" card is built from that single literal row, and `account_row`'s own definition
(`controls.rs:367`, `pub fn account_row(label: &'static str, subtitle: &'static str, active: bool,
theme: Theme) -> Div`) takes `active` as a caller-supplied constant, not a read from any selection
state. `grep -c 'selected_account\|active_account\|account_selection'` is 0 in both files. The
comment directly above the call site (`settings.rs:2440-2444`) confirms this is deliberate today:
"The 'Active' badge on the System default row is a *selection* marker... It stays true by
construction, not because anything was checked at render time." There is no second row to select
and no state that could move a badge even if a second row existed.

**Verdict: FAILED — absent.** Agreeing with the orchestrator's reclassification and not softening
it: the clause asks to select between System default and a second account and watch the active
badge move; there is exactly one hardcoded row and zero selection state anywhere in the settings
surface.

---

## F-CORE-ACT-20 — notification suppression follows real window focus, not a hardcoded true

**Reading the sibling's report before touching it, per the brief.** `CRITIC-waveK-builder-claims.md`
§2 (commit `86ae6b33`, ancestor of this pass's HEAD `960de672`) is a **critic** pass (`wf-judge`,
did not build the work), independently re-driving the exact mandatory gap the original builder
(`CENTER-PANE-DESYNC.md`) left open — "repeat independently... with a real notification daemon (not
a stub)". It ran the real, installed `notification-daemon` (GNOME) 3.20.0 on a private D-Bus
session bus (never touching the operator's own `/run/user/1000/bus`), with a positive-control
`GetServerInformation` handshake confirming the real daemon (not a stub) owned the name, then:

- Window genuinely defocused (a second real Wayland client, `foot`, took focus — confirmed via
  `swaymsg get_tree` reporting `foot focused:true` / Tiller `focused:false`), pane still the visible
  tab, a genuine `needs-input`→`done` transition: `dbus-monitor` captured a real `Notify` method
  call with the real daemon's own allocated id (`uint32 2`), full transcript committed at
  `reference/linux-progress/waveK-critic/act20-real-daemon-notify-transcript.txt`.
- Window genuinely refocused (`foot` killed, sway auto-refocused Tiller, confirmed via
  `get_tree`), another genuine transition (`done`→`error`): the monitor log's line count was
  **byte-identical before and after** (784 lines both times) — zero new bus traffic, confirming
  suppression is restored by real focus alone with pane visibility held constant throughout.

This is a hard discriminator (a real daemon's allocated notification id appearing exactly when the
window is genuinely defocused, and disappearing exactly when it is genuinely refocused, both
confirmed independently via `swaymsg get_tree`) and it closes precisely the gap the row's clause
and the prior builder's own report named as unproven. The report and its screenshots/transcript are
committed (`git log` confirms `86ae6b33` and the transcript file are both ancestors of/present at
this pass's HEAD), so this is not an unreplayable claim.

**Verdict: PASSED.** Adopting the critic-pass verdict from `CRITIC-waveK-builder-claims.md` §2 —
a critic (not the builder) exercising live against a real notification daemon, not a code read.
Not re-driven a third time in this pass; the sibling's evidence already satisfies the standard
(named daemon, named transcript file, named commit) and re-running it would not change the verdict.

---
