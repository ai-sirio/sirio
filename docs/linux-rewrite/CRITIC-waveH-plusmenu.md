# Critic pass on the "+" tab-menu fix (commit `9976b86d`), wave H

Fresh critic, did not write the fix, did not read `tab_bar.rs`'s diff as evidence — every verdict
below is from a live drive. Host: the x86 desktop in `ENVIRONMENT.md`'s 2026-08-18 section (12
cores, AMD GPU, COSMIC/Wayland). Lane: `Scripts/wayland-drive.sh`, `TILLER_WL_LABEL=wfplus`.
Binary built and pinned once, reused for every drive in this pass:

```
cargo build --manifest-path rust/Cargo.toml --workspace   # exit 0, warm, ~1s (nothing to rebuild)
cp rust/target/debug/tiller /tmp/wfplus-tiller
export TILLER_WL_BIN=/tmp/wfplus-tiller
```

`md5sum` of the pinned binary matched `rust/target/debug/tiller` at drive time, and
`grep -n on_next_frame rust/crates/tiller_ui/src/tab_bar.rs` confirmed the fix hunk (the
`if anchor_bounds.get() != Some(bounds) { … window.on_next_frame(...) }` guard) was present in the
source that produced it — the fix under test, not a stale rebuild.

Screenshots referenced below are committed under
`reference/linux-progress/wf-plus-critic/`.

---

## 1. Replay of the original failures

The two original critics' exact command sequences are not preserved verbatim anywhere in the repo
(the ledger and `PLUS-MENU-INVESTIGATION.md` both paraphrase them: "splitting the click into
move+down+up", "a shot interposed between opening the menu and clicking an item"). I could not
literally replay unrecorded keystrokes, so I reconstructed the two distinct trigger shapes the
investigation names and ran each to a hard discriminator. Both are reproduced faithfully below —
one now succeeds, the other still does not, and that split *is* the finding (see §2).

**Critic-2-shaped sequence — open the menu, let a resize land while it's open, then click an
item** (this is the shape that matches "clicking never creates a tab" over repeated attempts):

```
click 1294 47        # open the "+" menu (atomic)
shot 01-menu-open     # forces the W2xH2-and-back resize WHILE the menu is open
click 1330 86         # click "New Terminal" at its PRE-resize coordinate
shot 02-after-item-click
```

Result: **now succeeds.** `05-testA-menu-open-before-interposed-shot.png` shows the menu still
correctly anchored under the button after the forced resize; `06-testA-after-item-click-tab-created.png`
shows a **third tab strip entry** ("Terminal") plus a **new sidebar row** under `linux/gpui-waku` —
neither existed before the click. Hard discriminator: tab count 2→3, new pane with its own live
shell (`in bash at 17:00:16`), sidebar tree row count increased. This is exactly critic 2's
documented trigger, and it now lands the click correctly.

## 2. Deliberate reproduction — `shot` literally between a `down` and its matching `up`

Task instruction: interpose `shot` between a menu-opening `down` and its `up`, and report plainly
whether the fix closes this or not.

```
shot 00-before
down 1294 47          # press and HOLD on the "+" button, no release
shot 01-shot-between-down-and-up     # forces the resize while the button is still held down
up 1294 47             # release at the SAME coordinate (post-resize the window is back to size)
shot 02-after-up
```

**Result: this still fails.** No menu ever appears — not mid-resize
(`02-testB-shot-mid-press-no-menu.png`), not after the release
(`03-testB-after-up-still-no-menu.png`). Control, same gesture with no `shot` interposed:

```
down 1294 47
up 1294 47
shot
```

`04-testC-control-plain-downup-opens-fine.png` — the menu opens normally. So the difference is
specifically the resize landing *while the button is physically held*, not the split gesture
itself.

**What the evidence supports, stated plainly:** the fix closes exactly the bug it targeted — a
resize while the menu is **already open** — and §1's reproduction is real, independent proof of
that. It does **not** close a second, narrower case: a resize that lands **between a mouse-down
and its matching mouse-up on the same click**, before that click has completed at all. That is a
distinct failure mode from the one `tab_bar.rs`'s `anchor_bounds` fix addresses (there is no open
menu yet for a stale anchor to mis-position — the opening click itself never completes), and it
survives unchanged pre- and post-fix. This reads exactly like a synthetic-button-press being
cancelled by the compositor's output-reconfigure event arriving mid-grab, which would make it a
harness/compositor interaction rather than a `tab_bar.rs` defect — but I did not instrument
`WAYLAND_DEBUG=1` to confirm the wire-level cause, so I am reporting the symptom, not diagnosing
its mechanism. **Real, useful, still-open finding — the fix's scope is narrower than "no `shot`
placement can ever break this menu again."**

## 3. Spending the unblocked rows

I grepped the current `INVENTORY-LEDGER.md` for `UNREACHABLE` before driving anything. **None of
the five current `UNREACHABLE` rows name the "+" menu as blocker** (they are `F-SET-15`,
`F-AGENT-OMP-01/02/03`, `F-PERSIST-DB-12` — oh-my-pi's upstream defect, a settings source-read, and
a persistence-migration non-issue). The three rows this task named were already moved off
`UNREACHABLE` by an *earlier* pass (commit `ace67829`, 13:35) — **three hours before** the "+" menu
fix landed (`9976b86d`, 16:48) — via routes that do not go through this dropdown at all:

- **`F-CHAT-25`** — ledger line 174, `PASSED`, via a real (non-fixture) `AskUserQuestion` card
  surfaced mid-turn. This is a different clause of the row than the one blocked by the dropdown:
  the ledger's own history (`git log -p`, lines 499/808) shows an *older* `F-CHAT-25` entry that
  *was* genuinely blocked on "+" → New Chat → Codex, but the row was later re-closed through the
  AskUserQuestion clause instead, independent of this fix.
- **`F-AGENT-OPENCODE-01`** — ledger line 464, `half-proven`, via a regression test on session
  persistence (commit `29d0a672`, unrelated diff to `9976b86d`). I checked this row's entire
  history in the ledger (`git log -p | grep F-AGENT-OPENCODE-01`): it was never once attributed to
  the tab-bar dropdown — its blockers were "opencode not installed" and then a command-palette
  route, never `tab_bar.rs`. Commit `9976b86d`'s message citing it appears to be over-attribution.
- **`F-CHAT-11`** — ledger line 160, `NOT EXERCISED`, reason: "requires driving a native
  file-picker dialog inside the nested compositor, not attempted." Unrelated to the "+" menu
  entirely; still genuinely unexercised, for a different reason. I did not spend gesture time on
  this — driving a native GTK file-picker inside the nested Wayland compositor is its own
  undertaking, out of scope for this pass, and the ledger's existing reason is accurate.

**So there was nothing left in the current ledger for this fix to unblock.** Rather than report a
null result, I drove the fixed dropdown through the *original* blocked paths named in
`PLUS-MENU-INVESTIGATION.md` and the stale ledger history, to add direct, fresh proof that those
paths are real now (useful corroboration even though the ledger reached PASSED by other means):

**Codex-backed chat via "+" → New Chat → Codex** (the literal path row 499/808's old UNREACHABLE
text named as impossible):

```
click 1294 47      # open "+"
shot
click 1349 349      # "New Chat" — expands in place (chevron flips, agent list appears below)
shot 02-picker-open
click 1330 418      # "Codex" in the expanded picker
shot 03-after-codex-click
```

`07-newchat-picker-open-after-fix.png` shows the expanded picker (Claude Code, Codex) growing
below "New Chat" inside the same popup. `08-codex-tab-created-via-new-chat-submenu.png` shows the
hard discriminator: a **new "Codex" tab** in the strip (distinct icon), a **new sidebar row**
("Codex", 4th entry, selected) under `linux/gpui-waku`, and a composer bound to that surface. Tab
and sidebar row counts both increased by exactly one. **The path row F-CHAT-25 was once blocked on
is reachable now.**

One flakiness note earned by three attempts at this: a `click`, `click` pair with **no** `shot`
between them intermittently failed to register the second click on "New Chat" (menu stayed
collapsed; the click instead landed on the Files panel underneath, scrolling it — see the raw
`/tmp/wfplus-newchat4` frames, not committed). Pacing every click with an intervening `shot`
(as in the sequence above) was reliable across every attempt that used it. This reads as a timing
race in rapid same-target double-clicks through the synthetic pointer, separate from the
resize-anchor bug — noted for the next critic driving this same submenu, not filed as a row
verdict.

**OpenCode terminal tab via "+" → OpenCode** (bonus corroboration, not itself previously blocked
by this bug per the ledger history above, but the same dropdown code path):

```
click 1294 47
shot
click 1330 231      # "OpenCode" top-level item
shot 02-after-opencode-click
```

`09-opencode-tab-created-via-plus-menu.png`: new "OpenCode" tab (orange icon), 5th sidebar row,
Activity bar reads "1 running". Hard discriminator: sidebar row count 4→5, new tab title/icon,
live process count incremented.

---

## Verdict

The `9976b86d` fix is real and correctly scoped to the bug it names: a resize while the "+" popup
is **already open** no longer leaves it glued to the pre-resize position — confirmed by two
independent reproductions (§1, §3) each ending in a hard discriminator (new tab, new sidebar row).
It does **not** cover a resize landing **mid-press**, before the opening click completes (§2) —
that reproduction still fails identically pre- and post-fix, and is a distinct, still-open gap
(most likely a compositor/harness interaction, unconfirmed at the wire level).

No row in the current ledger was actually unblocked by this fix — the three rows named in the task
brief were all closed by unrelated evidence three hours before this commit landed, and one of them
(`F-AGENT-OPENCODE-01`) was never blocked by this dropdown in the first place per its own ledger
history. This is stated as a finding about the task's premise, not a critique of the fix, which is
independently verified correct within its stated scope.

## Rows

- `F-CHAT-25` — **half-proven** (this pass's own drive) — the "+"→New Chat→Codex path this row was
  historically blocked on now works live (new Codex chat tab, hard discriminator: tab+sidebar row
  count). Not a full re-verification of the row's actual current PASSED clause (AskUserQuestion),
  which this pass did not re-drive — the ledger's existing `PASSED` (line 174) stands on its own
  evidence, unrelated to this fix.
- `F-AGENT-OPENCODE-01` — **N/A — not blocked by this fix**. Ledger history shows this row was
  never attributed to the "+" dropdown; its `half-proven` (line 464) rests on an unrelated
  persistence-test commit. Bonus live drive this pass (§3, OpenCode top-level item) confirms the
  dropdown itself launches a real OpenCode tab, but that was not this row's blocker.
- `F-CHAT-11` — **NOT EXERCISED** (unchanged) — blocked on a native file-picker dialog inside the
  nested compositor, unrelated to the "+" menu; not attempted this pass, out of scope.
- Deliberate reproduction (`down`/`shot`/`up` on the same opening click) — **FAILED — defective**
  as a still-open, narrower gap distinct from the fixed bug: a resize landing mid-press cancels the
  click outright. Not filed against an inventory row (no row's VERIFY clause names this exact
  gesture); recorded here as the task explicitly asked for this result either way.
