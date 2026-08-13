# D4 — Empty states: the three front doors

**You are pi, pane `w1:p4`.** Your context was just reset, so this brief is everything you need.
Fourth in the design series by `fable` (`05-the-design-of-the-program.md`, decision D4). Code and
ledger outrank this file; read the current `tiller_ui` state first — D1 and D3 landed after this
was written.

## The refusal that sets this piece's bar

Under P44 you left `F-SET-03` PARTIAL because *the handler was still empty* — the version
rendered, the click dispatched, and you refused to call that a feature. That distinction **is**
this piece: an empty state whose CTA does not dispatch the real action is a poster, and this
project has produced that shape five times. Every CTA below must move real state.

## Where

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
cargo test -p tiller_ui -p tiller_theme
./Scripts/ci-linux.sh   # strict
```

**`tiller_ui/**` and `tiller_theme/**` are yours.** The typed actions the CTAs dispatch already
exist in the shell's command layer (P46/P48 — see `P43-tab-command-layer-contract.md`); if one you
need is missing, add what you need to that contract file and say so in your report rather than
inventing a parallel path.

## The piece: `D-EMPTY-01/02/03` — one anatomy, three moments

Zero designed empty states exist today. Build the anatomy **once** — icon, 20px headline, 12.5px
description, primary CTA, optional secondary CTA, centred in the surface it replaces — and
instantiate it three times:

- **`D-EMPTY-01` — app with no project.** Headline names the app's purpose in one line, not an
  apology. Primary CTA **Add Project** dispatches the real add-project flow (the same typed path
  the sidebar's `+` uses).
- **`D-EMPTY-02` — worktree open, no tab.** Primary CTA **Start Claude Code** dispatches the real
  new-agent action (the typed `claude-code` path from the contract); secondary **Open Terminal**
  dispatches the real new-terminal action. Both are the genuine articles that create the tab.
- **`D-EMPTY-03` — chat with no turns.** Same anatomy; primary CTA focuses the composer. This
  replaces whatever bare state the chat draws today, and it is the frame `SHOT-LIST.md` captures
  as `chat-empty`.

While here, judge **F-CHAT-35** (no-past-chats state, in the resume/history surface) and
**F-CHAT-36** (no-models fallback) against this same anatomy: build them with it where they fit;
where one does not fit, say why in the report instead of forcing it.

## Evidence

Drawn tests, per state (`TestAppContext` + `VisualTestContext`, `.debug_selector(id)`, real
clicks at debug bounds, full `run_until_parked()` hardening):

- the headline element is present in the drawn frame;
- **a real click on the primary CTA changes real state** — a project appears in the catalog / a
  tab exists after the click / the composer has focus. Assert the state, not the highlight;
- the state disappears once its condition ends (add a project → `D-EMPTY-01` is no longer in the
  frame).

Appearance (type sizes as rendered, spacing, the waku comparison) is display-debt; the shot list
schedules `launch-clean` and `worktree-no-tab` frames for it. Claim behaviour; never pixels.

## Rules

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
- Update `D-EMPTY-01/02/03` in `DESIGN-LEDGER.md` (and F-CHAT-35/36 in `INVENTORY-LEDGER.md` if
  you take them) as **`builder-claimed, unverified` — never `PASSED`**.
- Keep the gate green. Proceed without asking for design approval; this brief is the design.

## Reporting

Reply in **12 lines or fewer**: the three states and what each CTA really dispatched, proof by
test name per state, the F-CHAT-35/36 judgement, the gate result, and the honest remainder.
