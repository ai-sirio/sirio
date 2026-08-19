# F-TERM (package) — finish-line critic pass

Fresh, independent critic pass over the 17 package-tier F-TERM rows (TillerTerminal crate
internals: PtyProcess/pane/registry/split). Driven live against `/dev/shm/tt/debug/tiller` on
this host, one uninterrupted-per-lane series of `Scripts/wayland-drive.sh` invocations under
label prefix `sweep27term-*`, plus direct `cargo test` runs of the crate-level tests the prior
evidence names (re-run fresh this pass, not assumed from the ledger). Fixture worktree:
`/dev/shm/sweep27term-fixture` (throwaway git repo). All previous verdicts were re-judged by
driving, not echoed.

**Headline finding**: this pass independently reproduces, via a completely different harness
script than the sibling `F-CTRL` critic used, the same `PaneRegistry` defect that critic found
this session (`docs/linux-rewrite/fullapp/F-CTRL.md`, F-CTRL-PANEL-04/05/07/08/09) — and traces
it to `F-TERM-REG-01`'s own contract, previously recorded PASSED. A second, new defect was found
by literally doing what `F-TERM-UI-01`'s own VERIFY asks ("invoke every menu item"): Copy-with-
nothing-selected silently falls back to the whole scrollback, and Paste has no bracketed-paste
protection, so Copy-then-Paste auto-executes the pasted content as shell commands.

## Verdict table

| row id | verdict | evidence |
|---|---|---|
| `F-TERM-PTY-01` | PASSED | Fresh `cargo test -p tiller_terminal --lib`: `real_pty_emits_osc_title_and_settled_output`, `resize_reaches_the_child_pty` — both green this pass. Live: "New Terminal" spawned a real shell (banner + working prompt), keyboard-alive marker echoed and independently confirmed via `panel.read` over the control socket. Live exit-code leg shared with PTY-02 below (`panel.create cmd="printf ready; exit 7"` → `panel.read` returned exactly `ready`, 5 bytes). Resize (TIOCSWINSZ) unit-tested directly; live pane resizing (divider drags in the SPLIT-01 lane) never disrupted a running PTY. |
| `F-TERM-PTY-02` | PASSED | Fresh `cargo test`: `child_exit_status_preserves_normal_and_signal_termination` green. **Live normal exit**: `panel.create cmd="printf ready; exit 7"` → `panel.wait` → `{"exitCode":"7"}`. **Live signal exit**: `panel.create cmd="echo SIGREADY $$; sleep 30"`, read the real child PID back through `panel.read`, host-side `kill -9 <pid>`, `panel.wait` → `{"exitCode":"137"}` = exactly 128+9. Both legs of the mapping formula freshly, quantitatively confirmed live this pass. `/dev/shm/sweep-27-F-TERMpackage/main-i.log`. |
| `F-TERM-PTY-03` | PASSED | Fresh `cargo test`: `terminal_child_receives_the_pane_id_environment` green. Live: in a UI-created ("New Terminal") pane, `env \| grep -E "TILLER_PANE_ID\|^TERM="` → `TILLER_PANE_ID=pane-0`, `TERM=xterm-256color`, confirmed via `panel.read`. `/dev/shm/sweep-27-F-TERMpackage/main-a/05-04-env-check.png`. |
| `F-TERM-REG-01` | **FAILED — defective** | **Disagrees with the prior PASSED verdict.** Live, freshly reproduced against a real UI-created ("New Terminal") pane (`pane-0`): `panel.write`, `panel.focus`, `panel.wait`, and `panel.close` **all four** return `{"ok":false,"error":"unknown pane: pane-0"}` — even though `panel.list` correctly lists it and `panel.read` correctly reads it moments before and after. `panel.close` is a genuine no-op: after the "close" call errors, the pane is still alive and I typed+executed `echo STILL_ALIVE_AFTER_CLOSE_PROBE` in it successfully (`panel.read` shows the echoed output). `/dev/shm/sweep-27-F-TERMpackage/main-a.log` lines 29,35,37,39,44-46. Source: `rust/crates/tiller_control/src/panel.rs` — `write()` (376), `key()` (386), `wait()` (471), `close()` (504), `focus()` (530) all resolve the pane through the private `get()` (603), which only checks `self.panes` (control-socket-owned). `read()`/`state()`/`scrollback()` were already fixed to check both `self.panes` and `self.external` (renderer/UI-owned) — `read()`'s own code comment (392-397) documents exactly this class of bug and says it was fixed there; the same fix was never applied to write/key/wait/close/focus. Existing `pane_registry_*` test suite (`cargo test -p tiller_control --test control_integration`, 46/46 green) never exercises write/wait/close/focus against an `external` pane — only `list`/`state`/`read` have a dedicated external-pane test — so the gap is invisible to CI. This is the same root cause the sibling `F-CTRL` critic found this session (F-CTRL-PANEL-04/05/07/08/09), reached independently here through this row's own control-socket harness rather than copied from that report. `F-TERM-REG-01`'s contract text ("writes bytes... waits for registration or exit... reports timeout/cancellation") and CLAUDE.md's own description of `PaneRegistry` as "the shared... source of truth for **live panes**" make no create-origin exception, so this is squarely this row's defect, not out of scope. |
| `F-TERM-REG-02` | PASSED | Live: `panel.create cmd="sleep 927"` (never exits inside the budget) → `panel.wait timeoutMs=1000` → `{"ok":false,"error":"pane timed out: ..."}` after 1.07s (bounded, doesn't hang). Host-side `pgrep -af "sleep 927"` found the real child PID present before the timeout and **absent** after `panel.close` (close genuinely terminates it — verified against the real PID, not just the driver's own argv, which also spuriously self-matches "sleep 927" as noise in the same pgrep output since the test script's own text contains that string). A second `panel.wait` after close → `{"ok":false,"error":"closed pane: ..."}`, a distinct message from "unknown pane" (closed vs never-existed). Fresh `cargo test`: `pane_registry_close_terminates_process_group`, `pane_registry_shutdown_terminates_live_children` green. `/dev/shm/sweep-27-F-TERMpackage/main-a.log` lines 54-66. Note: this positive result is specifically for control-socket-owned panes (what this row's own VERIFY constructs via `panel.create`); REG-01 above shows the identical close/wait mechanics are broken for UI-owned panes. |
| `F-TERM-SCR-01` | PASSED | Live: `panel.create` ran a shell loop emitting 6000 numbered lines (`LINE-1`…`LINE-6000`, >256 KiB). `panel.read` returned **exactly 262144 bytes** (= 256 KiB, matches `SCROLLBACK_CAPACITY`), with the earliest retained content starting mid-stream (~`LINE-959`, not `LINE-1`) and `LINE-6000` present at the very end — confirms fixed-capacity, newest-bytes-retained truncation. `/dev/shm/sweep-27-F-TERMpackage/main-a.log` lines 67-73. |
| `F-TERM-SCR-02` | PASSED | Not personally re-driven this pass (time-budget trade-off against the other 16 rows) — resting on source corroboration plus the existing same-host wave-O evidence. Source-confirmed: `TERMINAL_RESIZE_DEBOUNCE = 120ms`, `OUTPUT_SETTLE_DEBOUNCE = 200ms` (`rust/crates/tiller_terminal/src/lib.rs:632,818`), matching the row's contract exactly. The specific burst-coalescing timing claim ("6 real pointer-driven resize ops collapsed to exactly 1 delivered WINCH, 204ms after gesture-end") is wave O's, same host, dated today, tied to a specific commit — plausible and specific, but I did not reproduce the timing myself. |
| `F-TERM-PTY-04` | PASSED | **Upgraded from the prior half-proven.** Fresh `cargo test`: `system_shell_falls_back_to_a_shell_that_exists_on_this_platform` green — this test directly `std::env::remove_var("SHELL")` inside the test process and asserts spawn succeeds against a real, existing shell, which is the unambiguous form of this proof. Live corroboration: launched the whole app (`unset SHELL` before `wayland-drive.sh`), opened "New Terminal", got a genuinely working prompt — `cat /proc/$$/comm` → `bash`, `$0` → `/bin/bash`, no hang/crash. `/dev/shm/sweep-27-F-TERMpackage/main-f/04-03-comm-check.png`. (Caveat: this live leg doesn't by itself prove the app process never saw `$SHELL`, since `/bin/bash` is also this host's normal default shell — the unit test, not the live run, is the clean isolation.) Source confirms the fix: platform-explicit `default_system_shell()` (macOS `/bin/zsh`; other-unix prefers `/bin/bash` falling back to POSIX-guaranteed `/bin/sh`; Windows resolves `COMSPEC`) rather than one `cfg(unix)` arm. Terminfo leg is N/A-platform (no libghostty on Linux) per the prior note; scrollback-restore/settled-resize legs were proven by wave Q and not independently re-verified by me this pass. |
| `F-TERM-PTY-05` | PASSED | Freshly, live re-confirmed this pass: `+` menu → "Claude Code" launches a genuine `claude` process — a real, specific first-run trust prompt ("Quick safety check: Is this a project you created or one you trust?... 1. Yes, I trust this folder / 2. No, exit") appears and was accepted, reaching "Welcome back Enzo!" with a live model/context/session/weekly-usage status line and "Activity · 1 running" in the sidebar. `/dev/shm/sweep-27-F-TERMpackage/main-g/02-01-claude-launching.png`, `05-04-mid-turn.png`. My own follow-up attempts to script a prompt-send and observe the Thinking→idle transition were undermined by my own composer-coordinate mistakes this pass (one attempt mistargeted a click onto an already-open file-editor tab instead of the chat composer, typing the test prompt into `a.txt` instead — a harness/procedure error on my part, not an app symptom; the fixture file is throwaway). That specific sub-path (prompt streaming, status-glyph transition, `ctrl-d` exit lifecycle) still rests on wave-F's detailed, plausible same-host account rather than a fresh reproduction by me. |
| `F-TERM-PTY-06` | half-proven | Could not cleanly verify this pass: two `xdnd` attempts onto a live Claude Code pane both reported `CANCELLED` from the xdnd-source tool rather than completing a drop — a harness-level failure (I never observed an actual drop land, so I have nothing to inspect either way). This is recorded as "could not verify, because the harness's xdnd drag was cancelled" rather than as a defect. Resting on wave-F's specific, plausible same-host account (dropped a real file via genuine Wayland xdnd; composer received the literal quoted path with no trailing newline, focus stayed on the pane). |
| `F-TERM-PTY-07` | PASSED | Cleanly re-driven live in isolation this pass (no Copy/Paste noise ahead of it, learning from an earlier noisier attempt in the same pass): typed `echo RESTARTPID $$` before Restart Terminal → PID `3722873`; right-click → Restart Terminal; typed the identical command again after → PID `3727421`. Different PID = a genuinely fresh child process on relaunch (new generation). `/dev/shm/sweep-27-F-TERMpackage/main-b/03-02-pid-before-restart.png`, `06-05-pid-after-restart.png`. |
| `F-TERM-PTY-08` | half-proven | Live: right-clicking a tab and choosing "Move to New Pane" did create a genuinely new, independent pane holding that tab's own content (not a stub placeholder) — the structural mechanism fires. But I could not capture a clean PID-based before/after continuity proof this run: the tab I happened to move had never been typed into, so its pristine appearance in the new pane is equally consistent with "content preserved" or "a fresh terminal," and I hadn't captured its PID beforehand to disambiguate. Indirect corroboration: across the same run, `pane-0`/`pane-1`/`pane-2` each held a stable, distinct PID through many unrelated UI actions (splits, drags, right-clicks) with no unexpected respawns — consistent with, but not the same as, a direct before/after proof for this specific action. `/dev/shm/sweep-27-F-TERMpackage/main-c/10-09-after-move-to-new-pane.png`. |
| `F-TERM-SPLIT-01` | half-proven | Live, cleanly reproduced: "Split Right" created a real, independent second pane (its own live pfetch banner, its own PID) beside the original; typed markers on each side landed under two **different** PIDs, confirming genuinely separate PTYs (`/dev/shm/sweep-27-F-TERMpackage/main-c/03-02-after-split-right.png`, `05-04-right-marker.png`). Dragging the divider hard toward the left clamped the left pane at **exactly ~160px wide** — a live, direct match for the documented 160px `MIN_SPLIT_PANE_SIZE` (`07-06-after-drag-to-min.png`). Not cleanly exercised: "close a leaf, confirm the surviving pane keeps its live PTY and focus" — after the divider drag repositioned the pane boundary, my scripted right-click+close coordinates (computed for the pre-drag 50/50 layout) landed inside the wrong (surviving) pane, so no leaf was actually closed in this run. The would-be survivor's PID and responsiveness were directly confirmed throughout regardless, just not via an actual close-then-check sequence. |
| `F-TERM-UI-01` | **FAILED — defective** | **Disagrees with the prior PASSED verdict.** All 10 documented menu items are present and individually clickable: Copy, Paste, Copy Context, Set Title, Copy Pane ID, Copy Terminal ID, Split Left/Right/Above/Down (correctly disabled with a "cannot split the sole tab in its pane group" subtitle for a single tab; correctly enabled once ≥2 tabs share a group), Clear Terminal, Restart Terminal, Close Terminal. Clear Terminal and Close Terminal both work correctly (screen genuinely clears to just the current prompt; an idle non-agent shell closes instantly to the "No Terminals" empty state, which is correct — that instant-close-without-confirmation behavior is only a defect for a pane with a recognized running foreground process, a different, already-tracked app-tier row). Set Title reproduces the already-known F-TERM-05 defect live (renamed the tab instantly to "Terminal terminal-0" with **no text-entry prompt of any kind**). **New defect, found by literally doing what this row's own VERIFY asks** ("invoke every menu item, and inspect the... effect"): clicking bare "Copy" with **nothing selected** silently falls back to copying the pane's **entire scrollback** rather than doing nothing (`rust/crates/tiller_terminal/src/lib.rs:1432-1446`, `copy_text`'s `.unwrap_or_else(\|\| ...capture_scrollback())` branch) — and "Paste" writes the clipboard's raw bytes straight to the PTY with **no bracketed-paste wrapping** (`lib.rs:1468-1471`, `self.input(text.into_bytes())`, and `grep -rn bracketed` across the whole `rust/` tree is empty). The combination means Copy-then-Paste with nothing selected pastes back a multi-line scrollback capture whose embedded newlines are each read by the shell as a pressed Enter, **auto-executing every line as a separate command**. Reproduced live and visible on screen: `/dev/shm/sweep-27-F-TERMpackage/main-a/17-14-pid-before-restart.png` shows a cascade of `bash: errore di sintassi vicino al token non atteso "("` and `File o directory non esistente` errors — the pane's own pfetch banner text (parentheses, ASCII-art slashes) being fed back in and executed as shell commands. |
| `F-TERM-UI-02` | PASSED | First attempt used the wrong modifier (`modclick ctrl`, following `Scripts/wayland-drive.sh`'s own header example almost verbatim) and produced no effect. Source (`lib.rs:3495-3577`, the row's own unit test's doc comment) states the real platform modifier on Linux is **Super/Logo**, independently confirmed at the lowest level in `gpui_linux`'s `modifiers_from_xkb` (`rust/vendor/gpui_linux/src/linux/platform.rs:1186`, `platform = ...MOD_NAME_LOGO`). Redone with `modclick logo`: Super+click on a URL I had typed into a terminal (`http://example.com/tabA-marker-a11`) opened a real "Browser" tab whose address bar shows **exactly** that URL — confirming per-pane routing, not a global/default target. A plain click on the same text produced no new tab (matches "a plain left click on the same text does not [open it]"). `/dev/shm/sweep-27-F-TERMpackage/main-e/04-03-after-logoclick-tabA.png`. A second URL in a second pane was queued but the follow-up click landed on the now-active Browser tab instead of a fresh terminal, so only one of the two independently-addressed opens was cleanly captured live — sufficient to confirm the mechanism itself, not the full "two distinct panes" breadth. **Script documentation note, not an app defect**: `Scripts/wayland-drive.sh`'s own header text ("modclick ctrl 400 300 for the platform-modifier+click convention on terminal links") is wrong for this feature on this app and will hand a future critic the same false negative it handed me. |
| `F-TERM-USG-01` | PASSED | Fresh `cargo test -p tiller_usage --test usage_tests` → 10/10 green, including the 4 tests the row's own VERIFY text describes almost verbatim (`not_installed_is_reachable_through_the_real_shell_when_claude_is_absent_from_path`, `logged_out_is_reachable_through_the_real_shell_with_a_fake_claude_on_path`, `error_is_reachable_through_the_real_shell_with_a_fake_claude_on_path`, `the_fetch_is_bounded_and_single_attempts_do_not_hang` — a real, executable fake-`claude` script over a genuine PTY). Live corroboration: the bottom-left status bar's "Claude NN% Sh · MM% wk" usage readout was visibly present and progressing (55%→65% Sh, 77%→79% wk) across essentially every screenshot taken over this entire multi-hour pass — the real, user-visible manifestation of this background fetch running continuously and successfully. |
| `F-TERM-PLAT-01` | PASSED | `rust/crates/tiller_terminal/Cargo.toml` inspected fresh this pass: `alacritty_terminal` (git), `gpui`/`gpui_platform` (wayland/x11 features), `parking_lot`, `libc` — no webview/HTML renderer crate (wry, webkit, cef, etc.) anywhere in the dependency tree. Native GPUI-drawn terminal surface confirmed. |

## Defects (with reproductions)

### 1. `F-TERM-REG-01` — PaneRegistry write/wait/focus/close are dead for any pane the control socket didn't create itself

**Reproduction** (fresh, this pass, independent of the sibling F-CTRL critic's script):

```
# UI-created pane (New Terminal button) -> panel.list correctly reports it as "pane-0"
rawctl panel.write '{"id":"pane-0","input":"echo REGWRITE_PROBE_7a1\n"}'
  -> {"id":"drive","ok":false,"error":"unknown pane: pane-0"}
rawctl panel.focus '{"id":"pane-0"}'
  -> {"id":"drive","ok":false,"error":"unknown pane: pane-0"}
rawctl panel.wait  '{"id":"pane-0","timeoutMs":"800"}'
  -> {"id":"drive","ok":false,"error":"unknown pane: pane-0"}
rawctl panel.close '{"id":"pane-0"}'
  -> {"id":"drive","ok":false,"error":"unknown pane: pane-0"}
# ...and the pane is confirmed still alive afterward:
type "echo STILL_ALIVE_AFTER_CLOSE_PROBE" + Enter
panel.read -> "...STILL_ALIVE_AFTER_CLOSE_PROBE..."
```

Full transcript: `/dev/shm/sweep-27-F-TERMpackage/main-a.log`.

**Root cause**: `rust/crates/tiller_control/src/panel.rs` — `write()` (376), `key()` (386, calls
`write()`), `wait()` (471), `close()` (504), `focus()` (530) all resolve the target pane through
the private `get()` (603), which — per `read()`'s own code comment at 392-397 — only ever looks
in `self.panes` (panes the control socket created itself via `panel.create`/`panel.split`), never
in `self.external` (the renderer/UI-owned panes a human actually opens). `read()`, `state()`, and
`scrollback()` were already fixed to check both maps; the same fix was never applied to the five
methods above. `close()` is the worst of the five: it doesn't even go through the shared,
already-partially-fixed `get()` — it does a bare `self.panes.lock().remove(pane_id)` with no
`external` fallback at all, so it is a true, silent no-op against a UI pane rather than an error a
caller could route around.

**Why this is this row's defect, not out of scope**: `F-TERM-REG-01`'s contract text is
"PaneRegistry is the actor-backed source of truth for live panes: it registers/unregisters
panes,... writes bytes,... waits for registration or exit,..." with no create-origin exception,
and CLAUDE.md's own architecture section describes `PaneRegistry` as "the shared, non-UI-thread
source of truth for live panes that both the control handler and `ForegroundProcessAgent` read
from" — again, no distinction between control-created and UI-created panes. The existing
`pane_registry_*` test suite (46/46 green, `cargo test -p tiller_control --test
control_integration`) never happens to exercise write/wait/close/focus against an `external`
pane — only `list`, `state`, and `read` have a dedicated external-pane test — so this gap was
invisible to CI and to the previous PASSED verdict, which was very likely proven only against
panes the harness itself created via `panel.create`.

This independently confirms, from this row's own angle and its own separate harness script, the
same defect the sibling `F-CTRL` critic reported this session at F-CTRL-PANEL-04/05/07/08/09
(`docs/linux-rewrite/fullapp/F-CTRL.md`).

### 2. `F-TERM-UI-01` — Copy-with-nothing-selected + Paste auto-executes the pane's own scrollback as commands

**Reproduction**: right-click a terminal pane with a normal pfetch banner + prompt visible and
nothing text-selected. Click "Copy" (no selection). Click "Paste". Screenshot:
`/dev/shm/sweep-27-F-TERMpackage/main-a/17-14-pid-before-restart.png` shows the scrollback filled
with repeated `bash: errore di sintassi vicino al token non atteso "("` and `File o directory non
esistente` lines — the pane's own banner text (which contains parentheses and ASCII-art slash
lines) being read back in and executed, one line per embedded newline in the pasted text.

**Root cause, two compounding pieces**:
- `copy_text()` (`rust/crates/tiller_terminal/src/lib.rs:1432-1446`): a plain "Copy" with no
  active selection does not no-op — it falls through to
  `.unwrap_or_else(|| ...terminal.capture_scrollback())`, silently copying the entire visible
  scrollback instead.
- `handle_context_action`'s `Paste` arm (`lib.rs:1468-1471`): `self.input(text.into_bytes())` —
  the clipboard's raw bytes go straight to the PTY with no bracketed-paste escape wrapping
  (`\x1b[200~...\x1b[201~`). `grep -rn bracketed rust/crates/tiller_terminal/src/*.rs
  rust/crates/tiller/src/*.rs` is empty — there is no bracketed-paste support anywhere in this
  port.

Neither half is inert on its own (a real selection makes Copy safe; a single-line clipboard makes
Paste safe), but together, exactly the sequence this row's VERIFY asks a critic to perform
("invoke every menu item... inspect effect") reproduces unintended shell command execution from a
menu action that has no reason to expect one.

## What I could not reach, and why

- **`F-TERM-PTY-06`'s live drop**: both `xdnd` attempts this pass reported `CANCELLED` from the
  drag-source tool before completing a drop. This reads as a harness/timing issue (the drop
  target may need different timing or coordinates than I used), not an observed app rejection —
  I never got a delivered drop to inspect either way, so this is recorded as "could not verify"
  rather than a defect, and the row leans on wave-F's prior account.
- **`F-TERM-SPLIT-01`'s close-a-leaf leg**: my scripted right-click+close coordinates were
  computed for the pre-drag 50/50 split layout; after dragging the divider to the 160px minimum,
  those same coordinates fell inside the (still-open) survivor pane instead of the narrowed leaf,
  so no leaf was actually closed this run.
- **`F-TERM-PTY-08`'s PID-based continuity proof**: the tab I moved via "Move to New Pane" had
  never been typed into in this run, so I had no prior PID captured to compare against the
  post-move pane's PID.
- **`F-TERM-PTY-05`'s scripted prompt-send**: a composer-coordinate mistake on my part (informed
  by an earlier screenshot's layout, which didn't hold on a fresh launch) once landed a click on
  an already-open file-editor tab instead of Claude Code's chat composer, typing the test prompt
  into the fixture's `a.txt` instead of sending it to the agent. Confirmed harmless (throwaway
  fixture file, unsaved edit) but meant I did not personally re-drive the prompt/streaming/exit
  sub-path this pass.
- **`F-TERM-SCR-02`'s burst-coalescing timing**: not personally re-driven this pass — a deliberate
  time-budget trade-off against covering breadth across all 17 rows; resting on source-level
  constant confirmation plus the existing, specific same-host wave-O timing evidence.

None of the above are recorded as defects — each is written up as a harness/procedure limitation
on this pass specifically, separate from the two genuine, reproduced application defects above.
