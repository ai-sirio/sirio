# P117 — report: one wrapper accounted for both surfaces

**Built by the orchestrator, 18:30. Not judged by the orchestrator** — the pane that verifies this
must be one that did not build it. Captures are in `reference/linux-progress/p117/`.

## The answer to the question the task asked

> *Establish whether it is one root cause or two before you fix either. If it is one, say so
> plainly; that is the most valuable sentence you can write today.*

**It is one.** A single element — the wrapper around `render_group_surfaces` — declared no height
at all. Every centre surface inherited that, and the two symptoms differ only because the two
surfaces have different intrinsic heights. Adding `.h_full()` to that one wrapper fixed the blank
chat transcript **and** the clipped Changes list, with no change to either surface's own code.

## The mechanism, measured rather than argued

`cargo test` cannot see a blank region, but `debug_bounds` can measure one. Instrumenting the whole
chain gave the exact layer where the height died:

| element | before | after |
|---|---|---|
| `centre-surface` | 1178 × **974** | 1178 × 974 |
| `group-surfaces-wrapper` | 1178 × **160** | 1178 × 974 |
| `pane-group-surface` | 1178 × 160 | 1178 × 974 |
| `pane-leaf` | 1178 × 160 | 1178 × 974 |
| `pane-surface` | 1178 × 160 | 1178 × 974 |
| `chat-root` | 1178 × 160 | 1178 × 974 |
| `chat-transcript` | 720 × **22** | 720 × **892** |

Two numbers name the cause outright:

- **160** is not a layout result, it is `MIN_SPLIT_PANE_SIZE` (`main.rs:2140`). The subtree collapsed
  to **zero** and `min_h` on the pane leaf floored it at exactly 160.
- **22** is the transcript's own `.pt(px(22.0))`. Its content height was zero, so the element was
  nothing but its padding.

The wrapper at `main.rs:6363` was:

```rust
div().relative().flex_1().w_full().overflow_hidden()
    .child(self.render_group_surfaces(*theme, entity.clone()))
```

`flex_1` grows an element along its parent's **main axis**. That parent is a default GPUI `div`,
which is `display: flex; flex-direction: row` — so `flex_1` sized the **width** and left the height
to content. The fix is one line:

```rust
.h_full()
```

### Why the two surfaces looked like different bugs

With the parent height-less, each surface fell back to its own intrinsic height:

- **Chat** wraps a virtualized `list()`, whose intrinsic height is **0** — a virtualized list cannot
  report a content height it has not measured. So chat collapsed to nothing and read as *blank*.
- **Changes** is a list of real rows with real intrinsic height, so it drew that much and stopped —
  which read as *clipped to ~150 px*.

Same cause; the symptom is a function of what each surface can report about itself. This is why the
task was right to insist the question be answered before either was "fixed": a fix aimed at chat
alone would have left Changes broken and vice versa.

### The salvaged patch pointed the wrong way

`docs/linux-rewrite/p117-codex11-unverified.patch` proposed deleting

```rust
.child(div().flex_1().w_full().overflow_hidden().child(centre_surface))
```

The measurement exonerates that wrapper: `centre-surface` is **974 px tall and always was**. The
collapse begins at its child. Deleting it would have removed a healthy element and left the defect
in place. The patch's *test*, however, was the right instrument and is now committed (its `E0063`
was stale — `OpenTab` gained/lost fields since it was written and the literal compiles as-is today).

I also tried `.h_full()` on `centre-surface` first. **It changed nothing** — recorded here so nobody
repeats it.

## The proof, which is not the test

`cargo test -p tiller --bin tiller` is **140 passed, 0 failed**, including the new
`drawn_chat_transcript_gets_the_full_center_surface_height`. That is reported as a test and is not
the proof. 134 tests passed over this defect for as long as it existed.

The proof is the frame. Reproduction takes ~8 s, not the 70 s the original report used — the **user**
message alone is enough to expose it, no completed agent turn required:

```bash
TILLER_WL_LABEL=orchp117b Scripts/wayland-drive.sh /tmp/orchp117b '
  ctl project.add path=/home/enzopalmisano/Scrivania/Progetti/tiller-linux
  ctl surface.chat.open >/dev/null
  ctl tab.select index=1
  sleep 2
  ctl surface.chat.send surfaceId=default-chat text=ORCHZZZ_MARKER
  sleep 6
  ctl surface.chat.read surfaceId=default-chat
  shot chat-after-send
'
```

| | frame | what it shows |
|---|---|---|
| before | `01a-BEFORE-chat-crop.png` | composer at the **top**, `ORCHZZZ_MARKER` nowhere, ~670 px empty below |
| after | `02a-AFTER-chat-crop.png` | `ORCHZZZ_MARKER` as a user bubble, the assistant reply beneath it, composer at the **bottom** |
| after | `03a-AFTER-changes-crop.png` | `Local changes (10)`, `Changed (4)` and `Untracked (6)` all drawn, nothing cut |

`ORCHZZZ_MARKER` is a string only this drive could have put on screen, so the after-frame
discriminates against "some region is no longer blank" — see `CRITIC-visual-baseline.md` §18:09 on
why a capture of a value the system could produce on its own proves nothing.

## What this unblocks, and what it does not

Every row whose evidence is "the transcript shows X" was unprovable and now is not — including the
`F-CHAT` rows held back from `P116` Slice A, and the `P114` hover-Copy and code-block-Copy rows that
could not be reached because no message was drawn to hover.

**None of those rows are `PASSED` on this report.** They still have to be exercised. This removes
the obstacle; it does not close a single inventory row, and the ledger stays `pireview`'s.

## Left for the verifier

- Re-run the drive above and confirm the marker draws. Do not take my frames.
- The transcript is `720 × 892` in the test window — check it also scrolls, which I did not exercise.
- Splits: `MIN_SPLIT_PANE_SIZE` was masking a zero-height pane. Now that panes get real height,
  **split behaviour is worth re-driving** — it is the one thing this change could plausibly disturb.
- Instrumentation added: `debug_selector`s for `centre-surface`, `group-surfaces-wrapper`,
  `pane-group-surface`, `pane-leaf`, `pane-surface` and `chat-root`. They are test-addressable
  geometry and are meant to stay.
