# F-CORE-WSP-05 — live drive of the Insert and Move wiring

**Driven by** an independent verifier (one of three adversarial passes), 2026-08-20, against
`feat/f-core-wsp-05-layout-command-8804` at `aeb8a70c`, merged as `8ef5f9c0`.
**Frames** in `critic-wsp05-live-drive-shots/`.

These screenshots are preserved here because the drive that produced them ran in a scratch
worktree under `/var/tmp`. A proof that lives only in scratch is deleted by the next pass, and
the verdict resting on it quietly becomes unreplayable.

## What was driven, and why it counts

Through `Scripts/wayland-drive.sh` with real synthetic pointer and keyboard input — the actual
"+" affordance and the actual tab context menu, **not** the control socket. The verifier built
their own binary in their own worktree and pinned it with `TILLER_WL_BIN`.

The row's claim is that the app's real layout mutations are routed through
`classify_layout_command`. Focus is the observable half of that claim, so it is proven the only
way that means anything: **by typing with no intervening click** and seeing where the characters
land. A screenshot of a moved tab shows the structural half; only the typed marker shows focus
actually followed.

| Frame | What it establishes |
|---|---|
| `06-01-after-insert.png` | **Insert, structural.** "+" → "New Terminal" creates a tab that becomes the pane group's own active tab, with live PTY output rendering in the content area. Reproduced independently across two runs. |
| `07-02-after-type-insert.png` | **Insert, focus — the disclosed gap, reproduced rather than assumed.** `INSERTMARK1` typed with no click lands nowhere: not in the prompt, not in scrollback, in this or any later frame. This confirms the implementer's own report that Insert's focus leg is unwired, and confirms it *live* instead of by reading the code. |
| `08-03-context-menu.png` | The real tab context menu, item set matching source. |
| `09-04-after-move-new-pane.png` | **Move to New Pane, structural.** A second pane appears holding the terminal's live content; the original pane falls back to "No Terminals". |
| `10-05-after-type-move.png` | **Move to New Pane, focus.** `MOVEMARK1` typed with no click lands directly on the moved tab's prompt line. |
| `09-04-context-menu-second.png` | With two pane groups now present, the menu shows the numbered variant **"Move to Pane 0"** rather than "Move to New Pane" — this is what proves the `other_groups`-driven branch was exercised, and not the same code path twice. |
| `10-05-after-move-to-pane-n.png` | **Move to Pane N, structural.** The tab moves back; the vacated pane falls to its own empty state. |
| `11-06-after-type-move2.png` | **Move to Pane N, focus.** `MOVEMARK2` lands on the new prompt, again with no click. |

Both source branches behind the menu (`TabContextAction::MoveToPane(usize::MAX)` for a new pane,
`MoveToPane(group_id)` for an existing one) were therefore driven, which was the task's explicit
ask rather than an incidental extra.

## A caveat the verifier raised, kept with the evidence

`classify_layout_command`'s `Insert` and `Move` arms are **unconditional** — `structural: true,
focus: Tab`, with no per-field logic — so for real Insert and Move traffic the gate can never
evaluate false. Mutation testing (a separate pass) proves the wiring is load-bearing: break the
classifier and named tests fail. But that is not the same as a runtime branch that goes both
ways, and the distinction belongs next to the screenshots rather than in a footnote. The
pre-existing `Rename` caller from F-CORE-WSP-04 has exactly the same property.

## Conditions

The box was heavily oversubscribed during this run (load average 30–124 on 12 cores, several
agents building and driving concurrently). Two harness symptoms followed from that and were
mitigated with retries and settle shots inside a single action string, not worked around
silently:

- the control socket's client-side 5 s dispatch timeout fired on `project.add` several times,
  the action usually completing asynchronously a moment later;
- one GPUI deferred-popup click race on the first attempt — two atomic clicks back-to-back with
  no repaint between them can arrive before the popup finishes linking, a trap already recorded
  in `WAYLAND-LANE.md`.

The final evidence runs are clean. No processes were left behind, and no other agent's worktree
or session was touched.
