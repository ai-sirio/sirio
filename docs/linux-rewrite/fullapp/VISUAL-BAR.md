# Visual bar — critic report

Fresh critic, one clause only: does this port look and feel like the reference Swift app in
`reference/shots/`? Everything below was driven live against the warm binary
(`/dev/shm/tt/debug/tiller`) over `Scripts/wayland-drive.sh`, on branch `linux/gpui-waku`. No
source was modified. Screenshots referenced by number (e.g. `03-04-terminal-pane.png`) live in
`/dev/shm/visual-compare/`, which is ephemeral scratch space (per the task's disk-safety
constraint I did not commit a shots directory) — every claim below is instead backed by a quoted
control-socket exchange, a quoted/sampled pixel value, or a specific code citation, so it is
checkable without the PNGs. A future critic can reproduce any row with the same commands, listed
inline.

**Verdict up front:** the static chrome — sidebar, tab strip, terminal pane, Changes list, and
every Settings page except the one macOS-only "Permissions" page — is a genuinely close visual and
structural match to the reference. The accent-colour question the task asked me to settle
concretely (below) confirms the earlier critic's finding: the reference's functional accent is
**blue**, ours is **coral**, everywhere a toggle, a badge, or a selected segment paints it. And one
new, load-bearing defect surfaced that no reference frame prepared me for: the actual chat surface
(the ACP-backed "Chat" tab — reference frames 01/15/16/17/21/22/23, six of the twenty-one frames
asked for) renders **completely empty** — no composer, no placeholder, no error — every time I
opened it, across a full app restart and both themes. That blocks six of the twenty-one requested
frames from being reproduced at all; see the dedicated section below.

## Method

```
export TILLER_WL_BIN=/dev/shm/tt/debug/tiller
export TILLER_WL_LABEL=vbar5          # (also vbar2/3/4, visualbar1 for earlier throwaway attempts)
mkdir -p /dev/shm/vbar5-fixture && cd /dev/shm/vbar5-fixture
git init -q && git config user.email a@b.c && git config user.name x
echo hi > a.txt && git add a.txt && git commit -q -m init
Scripts/wayland-drive.sh /dev/shm/visual-compare '<actions>' 20
```

Host load was extreme throughout this pass (`uptime` showed load average 44–48 with ~25 concurrent
`tiller` processes from sibling critics and 18 GiB of swap in use). Three early attempts
(`visualbar1`, a same-label relaunch too soon after the previous run's cleanup trap) failed with
`Unable to connect to /tmp/<label>-sway.sock` or a blank first frame — both exactly the two races
`wayland-drive.sh`'s own header and the scout's notes warn about, not app defects. Every result
below came from a run that passed the script's own `verify_nested_sway` / non-blank-baseline gates,
so none of it is contaminated by those races. `wayland-drive.sh` kills the app on every exit (`trap
cleanup EXIT`), so I re-launched (same label, same `/tmp/<label>.sqlite`) between logical phases;
the project registration, the open tabs, and even the appearance-mode choice were all confirmed to
survive that restart via `session.restore`, so continuity across phases is real, not assumed.

## Frame-by-frame

Each row is PASSED only where I drove the port into the matching state and looked at the result.

| # | Reference frame | Verdict | What I drove and saw |
|---|---|---|---|
| 1 | `01-chat-empty` | **FAILED — absent** | Opened via the real menu path (`+` → `New Chat` → `Claude Code`, the ACP-backed agent, matching what produces the reference's polished chat UI — see the dedicated section). The resulting "Claude Code" tab's content area is **totally blank**: no `Message…` composer, no idle-status pill, no send button, nothing. See "The chat surface renders empty" below for the full trail. |
| 2 | `02-project-expanded` | half-proven | Sidebar structure matches well: project row with folder glyph, worktree row with a `Primary` badge, `New Worktree…` affordance — all present and laid out the same way as the reference's `tiller`/`main`/`Primary` rows. What differs: the reference's default worktree-selected state already has a `Chat` tab open with its composer visible; ours lands on a `No Terminals` empty state with **zero tabs open** and an `Open a new terminal to get started` prompt. Confirmed via `ctl workspace.select` + `shot`: `/dev/shm/visual-compare/02-02-worktree-selected.png`. Reference's default-tab behaviour is not reproduced (may be intentional — the port doesn't auto-provision a Chat tab — but it is a visible difference at first paint). |
| 3 | `03-new-tab-menu` | **PASSED** | `click 1289 48` opens a dropdown that lines up closely with the reference's: `New Terminal`, `Claude Code`, `Codex`, `OpenCode`, `Pi`, `Oh-My-Pi`, `New Browser`, `New Chat ›`. Two items the reference menu does not show also appear here: `Changes` and `Split Claude Code` — the first is explained by a real architecture difference (see the Changes-panel row below), the second is an authentic extra capability (also present in the port's own test suite, `tab_bar.rs`'s `drawn_new_tab_menu_dispatches_every_item_action`). Icon-to-label pairing, ordering of the agent block, and the muted-text/hover-highlight styling on each row all match. |
| 4 | `04-terminal-pane` | **PASSED** | `New Terminal` from the menu opens a live PTY with a system-info banner (`pfetch`-style; OS/Kernel/Shell/CPU/GPU/Memory/Disk rows, a colour swatch strip) and a segmented prompt (`bash` pill → path pill → git-branch pill → timing badge → clock), structurally the same genre as the reference's banner + `user@host › tiller › main ⎇ ~3 ✓ · 21:04:28` prompt row. Content differs by design (different fetch tool, different demo repo) but the *composition* — banner above, segmented live prompt below — is a faithful match. |
| 5, 19 | `05-changes-panel`, `19-changes-list` | half-proven | Content match is strong once open: `Local changes (N)`, `Stage all`/`Discard all`, per-file `−X +Y` stat, expand chevrons — all present, matching the reference almost field-for-field (confirmed against a real 2-file diff I introduced in the fixture: `Staged (1)` / `Changed (1)` sections, exact stat counts). **Placement differs architecturally**: the reference keeps `Changes` as a segmented toggle living permanently beside `Files` in the always-visible right panel (`06-settings.png`'s sibling frames show `Files`/`Changes` as two pill buttons at the same coordinates throughout). This port instead treats `Changes` as its own tab in the *main* tab strip, opened from the same `+` menu as `Terminal`/`Chat` (`tab_bar.rs:751`, `NewTabAction::NewChanges`) — confirmed live: `rust/crates/tiller_ui/src/right_panel.rs`'s `Files` header (`render_header`, line 761) has no `Changes` sibling control anywhere in the source, and the right panel in every screenshot I took only ever shows `Files` / `Refresh` / close. This is a real, user-visible relocation of a core feature, not a cosmetic gap. |
| 20 | `20-changes-expanded-hunks` | **PASSED** | Clicked the `a.txt` row chevron; got a real unified-diff hunk (`@@ -1 +1,2 @@`, gutter line numbers, green-highlighted added line, per-file `Discard`/`Stage`/`Open diff` actions) that matches the reference's expanded-hunk layout closely, down to the gutter-column/content-column split. |
| 6, 7, 9 | `06/07/09-settings*` | **PASSED** (content); **coral vs blue** (accent) | `ctl surface.settings.select section=ai-providers` reproduces the reference almost exactly: per-provider card (`Claude Code`, `Codex`, `OpenCode Go`…), `Status` row with a coloured dot and `Signed in <account>` / `Not signed in`, `Show in usage bar` toggle, `Refresh interval` stepper, `Refresh now` button, `Accounts` sub-row with `This device` + `Active` badges and an `Add Account` button — every one of those elements is present and in the same order. Genuinely nice: our copy shows the real, currently-signed-in Claude account (this environment has a working `claude` CLI login), where the reference screenshot is necessarily a fixed demo state. The *only* difference is colour: see the dedicated accent section below — the reference's `09-settings-general.png` is, on inspection, pixel-identical to its own `06`/`07` (the reference set itself never actually switched to General for that capture), so there is no true reference image for the General page; I compared it on its own merits below instead. |
| — | (General, not in reference set as a distinct frame) | **PASSED** (own merits) | `About/Version`, `Agents/Resume agent sessions on launch`, `Automation/Auto-rename tabs`, `Chat history/Limit stored chats`, `Performance/Limit mounted worktrees`, `tillerctl/Control socket`, `Agent Skill` — a coherent, well-organised settings page in the port's own established row/card idiom. Three toggles are ON (`Resume agent sessions`, `Limit stored chats`, `Control socket`) and rendered in the port's coral accent — see below. |
| 10 | `10-settings-permissions` | **PASSED** (adapted) | Correctly does *not* try to reproduce macOS's six `Notifications/Screen Recording/Accessibility/…` rows — those are meaningless on Linux. Instead shows `Browser origin grants` / `Granted browser origins` / `Revoke all` / `No browser origins have been granted.`, a sensible platform-appropriate substitute for the one section of the reference's Permissions page that *does* apply cross-platform (the `Browser origin grants` block is present in the reference too, further down the same page). |
| 11 | `11-settings-appearance` | **PASSED** (structure); **not accented** (segmented control) | `Theme/Appearance` (`System`/`Light`/`Dark` segmented control), `Translucency` toggle, `Interface/Font size`, `Terminal/Font size`, `Files/File icons`, `Agent Colors` — all five card groups present in the same order as the reference. The `Agent Colors` block is *richer* than the reference: instead of one fixed colour swatch per agent, ours shows a row of 8 selectable colour dots per agent (a real picker), so this is arguably a genuine improvement, though it does make the page visually busier than the reference's single-pill-per-agent display. Segmented-control colouring is covered below. |
| 12 | `12-light-settings` | **PASSED** | `click 1219 141` (the `Light` segment) flips the whole app to light mode live, confirmed by `shot`: light page background, light card backgrounds, and every one of the Appearance page's own elements re-rendering in light-appropriate tones (dark-on-light text, borders). |
| 13 | `13-light-main` | **PASSED** | Backed out of Settings (`click 44 52`, the `Back` control) into the main three-pane layout, still in light mode: light sidebar, light tab strip, light Files panel — matches the reference's light-mode chrome in every panel. (The still-blank `Claude Code` chat tab is visible here too, in light mode, which rules out "only broken in dark mode" as an explanation — see below.) |
| 14 | `14-back-dark` | **PASSED** | Re-selecting `Dark` in the segmented control reverts every panel back to the dark palette live, no stale light-mode fragments left behind anywhere I looked (sidebar, tab strip, Files panel, terminal pane all confirmed dark again). |
| 15 | `15-chat-composer-focused` | **UNREACHABLE** | Cannot be produced: there is no composer to click into (see the chat-surface finding). Probed directly — `click 690 650` (where a composer would sit) then `type "hello probe"` — produced no caret, no border, no text anywhere on screen. |
| 16 | `16-chat-streaming` | **UNREACHABLE** | Same root cause; no message can be sent because no composer renders. |
| 17 | `17-chat-response` | **UNREACHABLE** | Same root cause. |
| 21 | `21-context-ring-popover` | **UNREACHABLE** | The context-usage ring lives on the composer row per the reference screenshot; with no composer rendered there is nothing to click. |
| 22 | `22-model-picker` | **UNREACHABLE** | Same: the model picker is a composer-row control. |
| 23 | `23-state-after-esc` | NOT EXERCISED | The reference's own frame 23 is pixel-identical to frame 22 (its Escape apparently didn't close the model picker either, because a macOS Apple-Music permission dialog had stolen focus first) — there is no distinct reference state to reproduce here regardless of our own chat-surface defect. |

## The accent colour, settled with pixels

The task asked me to look at this directly rather than take the provenance write-up's word for it.
I did, on both the "is it blue in the reference" question and the "is it coral in our port"
question, sampling solid fill pixels (not anti-aliased edges) with ImageMagick on both image sets.

**Reference — blue, everywhere the UI marks something as on/selected/active:**
- `Show in usage bar` toggle (ON), `reference/shots/06-settings.png` at (1148,208): solid `#2D6BFA`
  (neighbouring pixels in the same track sample `#2D6AF9`/`#2B65ED` — all the same blue family,
  never coral).
- `Dark` segment selected, `reference/shots/11-settings-appearance.png` at (1120,137): solid `#2D6BFB`.
- Same blue reappears in the model picker's `Recommended` badge and selection checkmark
  (`22-model-picker.png`), the light-mode segmented control (`12-light-settings.png`), and the
  worktree/tab selection tint in `13-light-main.png` — one consistent hue used for every
  "this is the active/on choice" signal in the app, distinct from any agent's own brand colour.

**Our port — coral, at the exact same category of control:**
- `Resume agent sessions on launch` toggle (ON), my `03-15-settings-general.png` at (1288,244):
  solid `#E08B52` — an **exact** match to the literal at
  `rust/crates/tiller_theme/src/lib.rs:296`, `rgb_hex(0xE08B52)` (the dark-appearance `accent`).
  Every ON toggle I photographed (`Resume agent sessions`, `Limit stored chats`, `Control socket`,
  `Show in usage bar` for Claude/Codex) samples to this same hex.
- The `Active` account badge on the AI Providers page samples to the same coral.
- The segmented control's *selected* pill (`System`/`Dark` in dark mode, `Light` in light mode) is
  **not** accented at all: it samples to solid `#343434` against a `#232323` track in dark mode,
  and `#D9D9D9` in light mode — a plain lightness step with no hue, confirmed in code at
  `rust/crates/tiller_ui/src/controls.rs:211`, `.when(active, |this| this.bg(theme.selected_fill))`,
  where `selected_fill` resolves at `tiller_theme/src/lib.rs:472` to `row_hover` — the same
  translucent white/black veil used for a plain row-hover state, not to `accent`/`tab_focus_accent`
  at all. So the segmented control is a *third*, even-flatter treatment: reference bold blue,
  our toggles bold coral, our segmented-control selection genuinely un-accented.

**Verdict on the question asked:** the port's rendering is unambiguously closer to **waku's coral**
than to the **Swift reference's blue**, and this is directly visible, not just a provenance
paper-trail — I sampled solid pixels from both running/rendered surfaces and they differ by hue
entirely (`#E08B52` vs `#2D6BFA`), not merely by shade. It is worth being precise about *where*
this shows up, though, since the reference itself is not monochrome: the reference's own coral
appears too, but only as `Claude Code`'s **brand** colour (its icon, and the sidebar/tab tint on a
row that represents a Claude session specifically — `reference/shots/02-project-expanded.png`'s
selected `Chat` row). The reference keeps a firm separation between "this is Claude's colour" and
"this is the app's functional accent, blue, meaning selected/on/active" — a separation the code's
own tests try to preserve (`accent_is_not_any_agent_brand`, `tiller_theme/src/lib.rs:1549`) but
can't fully honour, precisely because the chosen accent (`#E08B52`) sits close enough to Claude's
own brand orange that the two read as the same colour family in a screenshot, which the Swift
reference never risks because its functional accent is a completely different hue (blue) from
every agent brand colour it has to sit next to.

One additional, smaller and unrelated colour finding, found while sampling: the port's own
titlebar traffic-light dots are desaturated relative to the reference's, and change with the theme
where the reference's do not. Reference (`06-settings.png`, sampled at (17,17)/(38,17)/(58,17)):
solid `#F15F57` red, `#F3C527` yellow, `#4DBE54` green — the standard vivid macOS triad — and
sampling the same three dots in `12-light-settings.png` (light mode) returns the **same three
values**, confirming the reference keeps them theme-invariant. Ours (sampled at the equivalent
dots, (15,19)/(35,19)/(55,19)): `#FFA09A` / `#FFA37D` / `#5EDB8C` in dark mode — pastel
salmon/peach/mint, all three noticeably lighter and less saturated than the reference — and a
**different** `#890418` / `#792C00` / `#00572C` in light mode — dark maroon/brown/forest-green.
So our traffic lights are tinted by the active theme rather than being fixed OS-semantic colours.
Minor (the three functions — close/minimise/maximise — stay distinguishable by hue in both modes),
but a second, independent data point that this port's palette leans toward its own muted/adaptive
theme system even where the reference uses flat, theme-invariant colour.

## The chat surface renders empty — the pass's most consequential finding

Reference frames 01, 15, 16, 17, 21, 22, 23 (a third of the requested set) all show the ACP-backed
`Chat` surface in various states, and every one of them has a visible message composer even at
rest. I could not reproduce a single one, because the equivalent surface in this port renders
nothing at all.

**What I drove:** `+` (1289,48) → `New Chat` (1360,349) → `Claude Code` in the submenu that expands
under it (1360,383). This is a real, first-class menu path present in the port's own UI and its own
test suite (`tab_bar.rs`'s `drawn_new_chat_picker_gates_unavailable_agents_and_emits_the_selected_id`),
distinct from the top-level `Claude Code` menu item — I drove both, and confirmed they are genuinely
different code paths: the top-level item (`NewTabAction::ClaudeCode`) calls `add_agent_tab`, which
launches the *raw CLI* in a PTY (I saw this work correctly — a real `claude`-CLI trust prompt,
`Accessing workspace: … 1. Yes, I trust this folder`, rendered and interactive, in a **Terminal**-
styled pane). `New Chat › Claude Code` instead resolves through `open_chat_agent` →
`add_chat_tab` (`rust/crates/tiller/src/main.rs:6567`), which spawns the agent through the
ACP bridge (`adapter.acp_program()` → `acp_agent_command()` → `Chat::launch_with_command_and_persistence`)
and is the path that actually produces the reference's polished composer/transcript UI
(`TabKind::AgentChat`, `tiller_ui/src/chat.rs`).

**What rendered:** a tab titled `Claude Code` with the correct icon appeared in both the tab strip
and the sidebar's tab list — so the tab *itself* was created successfully. Its content area was
pure background, no visible element of any kind, confirmed across:
- Immediately after creation (`03-08-chat-empty.png`).
- After an 8-second wait (`09-chat-wait.png`).
- After a cumulative 45+ additional seconds of waiting within one invocation (`10-chat-wait2.png`).
- After a **full app restart** (new process, same label/DB — the tab survived via
  `session.restore`, confirmed still present and still blank: `11-restart-check.png`, then another
  45-second wait, `12-chat-longwait.png`).
- In light mode as well as dark (my own `03-19-light-main.png`, taken while the app was in light
  mode after the appearance-switch drive below), ruling out a theme-specific rendering bug.
- Under a direct interaction probe: `click 690 650` (composer's expected position) then
  `type "hello probe"` produced no caret, no border, no text — the pane is not merely
  invisible-but-functional; nothing is there to receive focus (`13-chat-blank-probe.png`).

**What I ruled out before calling this a defect rather than a slow environment:**
- *Not a cold-npx-fetch stall.* The ACP bridge Tiller launches by default is
  `npx -y @agentclientprotocol/claude-agent-acp@latest` (`rust/crates/tiller/src/main.rs:2263`). I
  confirmed from a bare shell that this package is already cached locally
  (`~/.npm/_npx/…/node_modules/@agentclientprotocol` exists) and that `npx` resolves and runs it
  immediately with no network step — so a slow first-run `npx` fetch is not available as an
  innocent explanation here; no network fetch was actually needed.
- *Not a PATH problem for the spawned app.* `wayland-drive.sh` execs the binary via
  `env -u DISPLAY … "$BIN"`, which only clears `DISPLAY` and otherwise inherits the driving shell's
  environment — confirmed `PATH` in that shell includes both `~/.local/bin` (holds `claude`) and
  the nvm Node bin (holds `npx`/`node`).
- *Not an auth problem in general* — the AI Providers page (same process, same session) correctly
  shows `Claude Code` as `Signed in <the real local account>` with live usage-bar data, and the
  sibling terminal-hosted `claude` CLI path (above) authenticated and ran fine in the same
  environment.
- *Not silent because it never tried* — the app's own log (`/tmp/vbar5.log`, both stdout and
  stderr) contains zero lines mentioning `acp`, `chat`, or the agent adapter at all, for a tab whose
  creation code path (`add_chat_tab`) explicitly focuses a composer focus-handle on the next frame
  (`window.on_next_frame(move |window, cx| window.focus(&composer_focus, cx))`,
  `main.rs:6636`) — i.e. the code's own intent is for a composer to exist and take focus
  immediately, independent of whether the agent process has said anything back yet. That it never
  appeared, with no error surfaced anywhere I could find (log, UI, or control socket), is what
  makes this a defect finding rather than "still connecting."

I want to be honest about the edge of my certainty here: I did not attach a debugger or instrument
the Rust process, so I can't name the exact line that swallows this. I also checked whether this
was already a known, since-fixed issue (`docs/linux-rewrite/P117-report.md` documents a real,
previously-fixed "blank chat transcript" bug) — it is not the same bug: P117 was a layout-height
collapse in a *shared* wrapper (`render_group_surfaces`), and I confirmed live that the fix is
still in place (`main.rs:9546`, `.h_full()` with the P117 comment intact) and that the *shared*
wrapper works fine — the Terminal and Changes tabs I opened in the very same session, through the
very same wrapper, rendered their full content correctly. What I found is specific to the
`AgentChat`/ACP tab kind alone, and specific enough (composer included) that it is not just "the
transcript is squashed" — it is "nothing in this tab renders at all."

## Other things worth recording

**Better than the reference:**
- The terminal pane's system-info banner is richer (12 fields including live CPU/GPU/memory/swap
  percentages and a colour-swatch strip) than what the reference frame shows.
- The Changes tab's `Unified`/`Split` diff-view toggle has no equivalent visible in any reference
  frame — a genuine added capability.
- The AI Providers page shows a live, real account email and usage state rather than the
  reference's necessarily-static demo data.
- `Agent Colors` in Appearance is a real per-agent colour *picker* (8 swatches), not just a fixed
  display pill.
- The Permissions page correctly drops six irrelevant macOS-only rows rather than faking them.

**Matches well, no material gap found:**
- Sidebar project/worktree hierarchy, `+`-menu contents and styling (aside from the two extra,
  legitimate items noted above), Settings category list and its selected-row treatment (both apps
  use a neutral light pill there, not accent — this one point of overlap between the two palettes),
  General/Appearance/Permissions page composition, light/dark mode switching, expanded-diff-hunk
  rendering.

**Real structural gaps, not just colour:**
- `Changes` lives in the main tab strip here, versus a permanent segmented toggle beside `Files` in
  the reference's right panel (row 5/19 above).
- No worktree I opened auto-provisioned a `Chat` tab the way the reference's default state implies
  (row 2 above) — every chat tab in this port has to be created explicitly through the `+` menu.
- The chat surface itself (row 1, 15–17, 21–22 above) — the single largest gap found in this pass.

## Harness limitations acknowledged, not counted as app defects

- Three early `Unable to connect to …-sway.sock` / blank-first-frame failures were reused-label
  races the script's own header warns about; I do not count them as evidence of anything about the
  app, and none of the rows above rest on a run that hit one.
- I could not inspect the spawned ACP child process's own stdout/stderr directly (the harness kills
  the app, and any of its children, at the end of every invocation via `trap cleanup EXIT`, so I
  never had a live window to `ps`/`strace` into) — my evidence for "no composer, ever" is therefore
  drawn from the app's own aggregate log plus repeated, long-waited, cross-restart, cross-theme
  screenshots and a direct interaction probe, not a process trace. I have flagged this in the
  finding's own write-up rather than overstating certainty about the exact failure point.
