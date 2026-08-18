# Fresh-critic pass — wave B, builder A (2026-08-18)

Box: x86_64 desktop per `ENVIRONMENT.md` top section. Judged at commit `ad1d783e` (HEAD at start —
includes the four commits under review: `604e4f8d`, `6a251b52`, `29d0a672`, `7803b5bd`). Wayland
lane label `wf-jdga`, binary pinned at `/tmp/wf-jdga-tiller` (cp'd from a warm
`cargo build --manifest-path rust/Cargo.toml --workspace`, exit 0, before driving). A second,
disposable label `wfjdgaB` was used once, for a single isolation check on item 4, then torn down.

Load on this box was erratic throughout this pass — `uptime` swung between ~15 and ~58 several
times over the session as other agents' waves overlapped. Every screenshot below was re-taken (or
its sway-connect failure retried) after confirming load had come back down; findings that could be
load-sensitive say so explicitly.

## Item 1 — F-TERM-PTY-07 (Restart Terminal) — verdict: PASSED

VERIFY (brief): right-click a live pane, click Restart Terminal, confirm the screen clears and a
fresh prompt appears, confirm the generation moved.

Drove the real gesture in one continuous `wayland-drive.sh` invocation (a hard lesson from this
session: a *second* script invocation with the same label always kills-and-reboots the app first,
so any multi-step gesture must live inside one `eval "$ACTIONS"` block or it silently starts over —
see "methodology notes" below):

```
ctl project.add path=…/tiller-linux
click 600 500; type echo RESTARTMARK_BEFORE_PID \$\$; key Return   # \$\$ escaped so the REMOTE
                                                                    # bash expands its own PID, not
                                                                    # wayland-drive.sh's eval
rightclick 600 500                        # real 13-item menu, Restart Terminal present
click 659 841                             # "Restart Terminal"
click 600 500; type echo RESTARTMARK_AFTER_PID \$\$; key Return
```

- Before: `RESTARTMARK_BEFORE_PID 1020401`.
- Immediately after the click (before any typing): the pane goes **fully black** — no scrollback,
  no old banner, just a cursor — the real `TerminalState::Pending` clear, not a redraw artifact.
- After: fresh neofetch banner (new shell-init run), `RESTARTMARK_AFTER_PID 1025304` — a
  **different PID**, proving a brand-new process replaced the old one in place. `surface_generation()`
  itself isn't exposed over the control socket (checked `system.capabilities` — no such method), so
  the generation counter can't be read directly from a live drive; the PID change is the externally
  observable effect of exactly what the generation bump gates (a fresh spawn superseding the old
  one), which is the strongest discriminator available outside the gpui test harness.

First attempt at this gesture used unescaped `$$` and got the same number before and after — that
turned out to be `wayland-drive.sh`'s own `eval "$ACTIONS"` expanding `$$` client-side (its own
PID, constant for the whole invocation) before the text ever reached the remote shell. Recorded as
a `WAYLAND-LANE.md` gap: the `$$` trap isn't documented there yet.

## Item 2 — F-TERM-PTY-08 (pane cache / move-preserves-PTY) — verdict: PASSED

VERIFY (brief): move a live agent pane between split groups with real gestures, confirm process and
scrollback continue uninterrupted.

Real gesture: right-click the **Terminal tab** (not the terminal content) at its tab-strip position
→ "Move to New Pane" (creates a second pane-group column and moves the tab into it) → typed a PID
marker before and after, in one continuous invocation with a `shot` between rightclick and click
(without it, the click landed before the popup settled and silently missed — same class of
methodology trap as item 1).

- Before move: `MOVEMARK_BEFORE 1078166`, full neofetch banner + prompt.
- After move (real split now visible — left pane group empty/Chat, right pane group holds the
  moved Terminal): the **same scrollback** (banner, `MOVEMARK_BEFORE 1078166`) is intact, unbroken,
  in the new location.
- Typed again in the moved pane: `MOVEMARK_AFTER 1078166` — **identical PID** to before the move,
  proving the underlying PTY was never torn down.

`ctl panel.list` was tried as a corroborating signal but turned out to be too coarse for this
purpose — its `[{id, tab, title}]` rows are unaffected by which pane *group* a tab lives in (they
stayed byte-identical before/after a confirmed-real move, and also before/after a confirmed-real
empty-prompt transition in item 3), so it is not useful evidence for split-group structure one way
or the other; the screenshot + PID/scrollback pair is what actually proves this row.

## Item 3 — F-TERM-02 (empty-prompt surface) — verdict: PASSED

VERIFY (brief): right-click a tab → Move to New Pane → move it back, leaving an empty pane; confirm
the real empty-prompt surface renders and its actions work.

Chained onto item 2's gesture: with Terminal now in its own new pane group, right-clicked its tab
again → menu now offers **"Move to Pane 0"** (the other, original group) → clicked it. The vacated
group renders exactly the coded surface (`tiller_terminal/src/lib.rs:814` `empty_prompt`), not the
old bare fallback:

- "No terminal in this pane" text, "New Terminal" and "New…" buttons, both real and clickable.
- Clicked "New Terminal" (1019,515): a **live new terminal tab** spawned in that same
  previously-empty group (fresh PTY, fresh banner) — sidebar shows two "Terminal" entries, one per
  group. The action genuinely works, not just renders.

Both conjuncts of the VERIFY clause (render + working action) driven live.

## Item 4 — F-AGENT-OPENCODE-01 (two-launch agent_id persistence) — verdict: UNREACHABLE

VERIFY (brief): run the real binary, spawn an opencode tab, quit, relaunch against the SAME
TILLER_DB, confirm the pane comes back running `opencode --session <ref>` and not plain bash.

**Blocked before the first launch's own action, by a gesture-level defect independent of this
builder's four commits** (confirmed pre-existing: `tab_bar.rs`'s new-tab menu was last touched at
`75175942`, long before this wave). The only real UI path to create *any* agent-CLI tab
(Claude Code / Codex / OpenCode / Pi / Oh-My-Pi) is the "+" tab-strip dropdown
(`tiller_ui/src/tab_bar.rs` `render_menu_item` → `TabBar::on_new_tab` → `WorkspaceAction::NewTab` →
`open_action`) — `add_agent_tab` has no other real caller in `main.rs`. Clicking any item in that
menu — tested **New Terminal, Claude Code, and OpenCode** — through real synthetic pointer clicks
never created a tab, across:

- 7 independent attempts, two separate `TILLER_WL_LABEL`s (`wf-jdga` and a disposable `wfjdgaB`, to
  rule out per-label pointer/DB corruption from reuse),
- both low load (~15-20) and high load (~50+) on `uptime`,
- immediate double-click, click-with-settling-`shot`-between, click at the item's left edge instead
  of centre, and a press-drag-release variant (`down` on the button → `move` to the item without
  releasing → `up` on the item, in case this is a native-menu-style press-hold-drag interaction).

Every attempt: the menu visibly opens (confirmed via an intermediate `shot`, item correctly
positioned and legible at the clicked coordinate) and visibly closes on the second click, but no
tab is added (checked both by full-frame screenshot of the tab strip and by `ctl panel.list`
before/after). Meanwhile **`ctrl-t` (the `WindowCommand::NewTerminalTab` keybinding, a completely
different code path) reliably creates a new tab** in the same session — ruling out a general
tab-creation or pending-actions-queue failure and isolating the defect to this one dropdown's
pointer-click handling.

Since no other real gesture reaches `add_agent_tab`, and typing `opencode` manually into a
plain `ctrl-t` terminal would not set `tab.agent_id` (the exact thing this row exists to prove),
there is no substitute gesture available in this environment today that would actually exercise the
code path under test. This is a **measured, reproducible blocker in the current build**, not a
budget shortfall — reporting it as UNREACHABLE rather than NOT EXERCISED per the evidence standard.
This defect is outside my assigned files (`tiller_terminal`/`main.rs`'s port-gap commits) and is
**not itself one of the five items under review**; flagging it here because it is what stopped item
4, not as a row verdict of its own.

## PORT-2 — verdict: CONFIRMED genuinely skipped off-Linux

Claim: `drawn_save_failure_surfaces_file_notice` (reads `/proc/self/status`) is now gated
`#[cfg(target_os = "linux")]`, matching its sibling `workspace_wires_real_process_signal_without_
title_clobber` at line 12618/12620.

Confirmed by inspection: `rust/crates/tiller/src/main.rs:13912-13914` —

```rust
#[cfg(target_os = "linux")]
#[gpui::test]
async fn drawn_save_failure_surfaces_file_notice(cx: &mut TestAppContext) {
```

`#[cfg(target_os = "linux")]` sits **above** `#[gpui::test]`, so on any other target the function
is stripped before the test-registration macro ever sees it — not merely skipped at runtime, absent
from the compiled test binary entirely. Byte-for-byte the same pattern as the already-accepted
sibling at line 12618-12620.

Attempted a stronger, reachable cross-target check: `rustup target list --installed` showed
`aarch64-apple-darwin` and `x86_64-pc-windows-msvc` present. `cargo check --target
aarch64-apple-darwin -p tiller --tests` and the Windows-MSVC equivalent were both run; both failed,
but at an unrelated transitive dependency's build script (`psm`'s hand-written stack-probe
assembly, `src/arch/aarch_aapcs64.s` / `x86_64_windows_gnu.s`) needing a real cross-target
assembler/SDK this box doesn't have — failing before `tiller` itself is ever type-checked, so this
confirms only that full cross-compilation isn't feasible here today, not anything about the gate
itself. The `#[cfg]`-above-`#[gpui::test]` inspection is the load-bearing evidence; it's
deterministic, unambiguous compiler behaviour, not a guess.

## Methodology notes (for whoever re-drives this)

1. **`wayland-drive.sh`'s `kill_ours` at the top of the script runs unconditionally on every
   invocation**, even under `TILLER_WL_KEEP=1` from a *previous* run — `TILLER_WL_KEEP` only
   suppresses the trap-on-exit cleanup at the end of the invocation that set it, not the next
   invocation's own startup kill. A multi-step gesture (right-click → click a menu item → verify)
   must be one `eval "$ACTIONS"` block, in one script call, or the second call reboots the app from
   the persisted DB and the "continuation" is actually acting on a fresh boot.
2. **`eval "$ACTIONS"` runs in `wayland-drive.sh`'s own process**, so an unescaped `$$` (or any
   other unescaped shell metacharacter) in a `type` payload is expanded client-side against the
   *driver script's* environment before `wtype` ever sees it. Escape as `\$\$` to get the literal
   two characters typed into the remote shell.
3. Sway occasionally failed to come up (`ipc-client.c:67 Unable to connect`) under this box's load
   spikes today (seen at `uptime` ~38-58) — a plain retry after `uptime` settled always recovered;
   never had to intervene beyond that.

Screenshots for items 1-3 are under `/tmp/wf-jdga-shots/` (scratch host paths, not committed, per
the drive-lane convention — reproducible by re-running the `Scripts/wayland-drive.sh` snippets
quoted above against the same commit).
