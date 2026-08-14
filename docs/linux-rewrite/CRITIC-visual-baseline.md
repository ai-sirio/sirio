# Critic — visual baseline, our Linux build vs waku

Judged by the orchestrator, which is the only participant in this project that can see images.
The builder agents and the functional critic are all text-only models; `pi` said so itself in
`03-visual-bar-and-gpui-patterns.md`, where its entire waku palette is source-derived and was
never checked against a pixel. This document is that check.

**Frames compared**

| Ours | `reference/linux-progress/01-renders-on-linux.png` — 1715×972, first successful Linux render |
| Reference | `_tiller-refs/waku/website/public/app-screenshot-light.png` and `-dark.png` — 2266×1752 |

Compared like for like: our build currently starts in light, so the light waku frame is the
fair comparison. That our build starts light *at all* is a defect, tracked in P1.

---

## Dimension by dimension

**Information architecture.** Ours: three columns — projects tree, tabbed centre, Files/Changes
inspector — plus a status bar. waku: two columns, no inspector, no tab bar. waku's centre column
is one continuous reading surface with the conversation title as its only header.

**Density.** Ours packs ~24px rows with uniform small type and puts a control on nearly every
row: disclosure triangles, per-tab close buttons, per-activity close buttons, a segmented
Files/Changes toggle, a refresh glyph, a gear. waku's sidebar has *two* controls above the fold
("New Task", "Search"), each on a generous row, and one gear pinned bottom-left. Its session
rows carry no buttons at all — just title, project chip, elapsed time.

**Typography.** Ours uses one size for nearly everything and separates meaning by colour and
box. waku separates by *weight and space*: bold lead-ins inside body text, ~1.6 line-height, and
a real step between 11.5px UI chrome and 13.5px body. waku's transcript reads like a document;
ours reads like a form.

**Colour.** Ours is cool — blue-tinted near-white ground, saturated blue accent on the active
segment. waku is warm — a near-white with a faint warm cast, warm grey text, and a single
terracotta/coral accent (`#C85F44` light, `#E2795B` dark) used only for inline code and links.
The accent is rare in waku, which is what gives it weight; ours spends its accent on a toggle.

**Window shape.** waku: rounded corners with a soft drop shadow, no hard outer border. Ours:
square corners, hard edges.

**Composer.** Not visible in our frame (the terminal tab was active), so this is judged against
the old macOS shots rather than the Linux build. waku's composer is the visual anchor of the
screen: a large card, a roomy `Do anything…` placeholder, and a single row of labelled chips —
model, effort, permission scope, mode — with a circular send button. Tiller's is a short box
with small unlabelled affordances crowded along its bottom edge.

---

## THE SINGLE BIGGEST GAP

**Density and type scale — the app is still built to IDE proportions, and waku is built to
reading proportions.**

Naming palette first would be the easy answer and the wrong one. Colour can be swapped in an
afternoon and would change nothing about why these two screenshots do not belong to the same
family: one crams four panes, a tab strip and forty controls into 1470pt, the other gives a
conversation the whole width and shows perhaps eight controls in total. Retint our current
layout with waku's exact coral and warm greys and it will still read as a dense IDE wearing
someone else's colours.

So the first move is the row heights, the type steps, the line-height, and the chrome budget —
how many controls are permitted to be visible at rest. Palette follows; it is cheap once the
proportions are right, and it is misleading before.

**For P1 specifically:** apply the measured scale from `03-visual-bar-and-gpui-patterns.md`
§A.2 — 11.5px UI / 13.5px body, the 4/6/7/8/12/13 radius scale, 48px bars — and treat every
always-visible control as something that must justify itself. That is the axis on which the
next screenshot will be judged.

## Not yet judged

The composer, chat transcript, changes/diff and model picker have no Linux frame yet
in the new design. They are unjudged, not passing. (Settings was on this list until
2026-08-14; see below.)

---

# 2026-08-14 — settings clears the bar, the main window does not

Frames, both from the Wayland lane (`TILLER_WL_LABEL=orchvis`, 1715×972, fresh private DB, binary
built 17:12):

| `reference/linux-progress/2026-08-14-orchvis-main.png` | main window at rest |
| `reference/linux-progress/2026-08-14-orchvis-settings-general.png` | Settings → General |

`2026-08-13-cosmic02-dark.png` is superseded and should stop being cited as current. It is not a
COSMIC reference — it is *our* app a day ago, and it rendered almost nothing: no tab labels, no
file rows, no project header, no window controls. Everything below is measured against the bar
this document set, not against that frame.

## Settings passes the density test, which is the one that mattered

The judgement above said palette was the cheap answer and **proportions** were the real axis: row
heights, type steps, chrome budget. Settings → General now clears it, and it is the first surface
in this port that looks like it belongs to the target family rather than to an IDE.

- **Grouped rounded cards under small muted section labels** — About / Agents / Automation / Chat
  history / Performance / tillerctl. That is COSMIC Settings' own structure, not an approximation
  of it.
- **Three legible type steps**: section label, row title, dimmer subtitle. The hierarchy carries
  without rules or weight tricks.
- **Reading proportions, not IDE proportions.** Two-line rows run ~62px, single-line ~46px. This is
  the specific thing the previous entry said would decide it.
- **Chrome budget at rest: eight controls.** Five nav items, Back, and the content. Nothing else is
  competing for attention.

The coral accent on the toggles is almost certainly correct rather than a waku leftover:
`cosmic::live` reads the installed COSMIC theme, and Pop!_OS's accent is warm. Worth one check that
it *tracks* — change the desktop accent and confirm the toggles follow — but do not "fix" it to
blue on the assumption that COSMIC means blue.

## The main window still reads as an IDE, and now the mismatch is internal

The old complaint was that our app and waku did not belong to the same family. That is no longer
the interesting comparison. **The new one is that our own two surfaces don't.**

At rest, on a brand-new profile with no projects, the main window shows: a titlebar, a projects
sidebar with a header and a filter field, a tab strip, a terminal, a 410px file tree listing thirty
rows, and a status bar. Settings earns its eight controls. The main window opens with well over
forty, most of them describing nothing — the sidebar is empty, and the file tree is the widest and
densest object on screen before the user has asked for a file.

Two specifics worth acting on, in order:

1. **The Files panel is open by default and takes 24% of the width.** Nothing in the 389 rows
   requires it to start open. It is the single largest contributor to the chrome budget and the
   easiest to reclaim.
2. **The empty projects sidebar has no empty state** — header, `+`, filter field, then blank. I
   checked: no inventory row covers a no-projects state, so this is **outside the frozen contract
   and must not be built as if it were a row.** It is recorded here as a visual observation for
   whenever scope is next revisited, nothing more.

## What this does not judge

Both frames are dark. Light mode is unjudged. The chat transcript and composer are unjudged — and
note that they cannot currently be reached over the socket for capture at all, because
`surface.chat.*` drives a chat that is never rendered (`P107`).

---

# 2026-08-14 17:40 — a completed chat turn renders nothing

**This supersedes the "cannot be reached over the socket" note above.** Since `P107` the socket
does drive the rendered chat, and since `P112` the Wayland lane can click and type. Both were used
here. What the eye found is worse than unreachability.

## The reproduction

Two independent drives, two different messages, identical result. Binary built 17:34, which
includes every chat commit through `c23da36`.

```bash
TILLER_WL_LABEL=orchvis3 Scripts/wayland-drive.sh /tmp/orchvis3-shots '
  ctl project.add path=/home/enzopalmisano/Scrivania/Progetti/tiller-linux
  ctl surface.chat.open >/dev/null
  ctl tab.select index=1
  ctl surface.chat.send surfaceId=default-chat text=Reply_with_the_single_word_ORCHVIS
  sleep 70
  ctl surface.chat.read surfaceId=default-chat
  shot chat-70s
'
```

`surface.chat.read` at 70 s, verbatim:

```json
{"composerText":"","queuedText":"","status":"completed","surfaceId":"default-chat",
 "transcript":"[{\"kind\":\"user\",\"text\":\"Reply_with_the_single_word_ORCHVIS\"},
                {\"kind\":\"assistant\",\"text\":\"ORCHVIS\"},
                {\"kind\":\"turn\",\"text\":\"17:39\"}]"}
```

The agent really ran and really answered. `/tmp/orchvis3-shots/02-chat-70s.png` is the frame taken
immediately after that read.

## What the frame shows

Everything *except* the content is correct, which is why no test caught it:

- the Chat tab carries a completed-turn **✓**
- the composer has reset to its `Message...` placeholder
- the mode pill reads `Ask`, the model pill `Opus Plan Mode XHIGH`
- the context ring reads `5%`, the stop button has reverted to the send arrow
- during the turn (`/tmp/orchvis2-shots/06-chat-after-wait.png`) the same chrome read
  `● working`, `40%`, a stop square, an orange tab dot, and `Activity 1 running`

**And the transcript is empty.** No user message, no `ORCHVIS`, no `17:39` turn marker. The
composer card sits at the top of the pane with the worktree path beneath it and roughly 700 px of
nothing below — the layout the pane uses when it believes it has no entries.

## Why this matters more than one row

The runtime status of the rendered chat *does* follow the socket, so `P107` genuinely connected
something. But the entries the view draws from are not the entries `chat.read` returns. Every row
whose evidence is "the transcript shows X" is unprovable until this is fixed, and any such row
already marked `PASSED` on a drawn test rather than a live frame should be treated as suspect.

**Hypothesis, deliberately not asserted:** status is being taken from the ACP session while entries
are appended to a collection the view does not read. Test it, do not take it.

## A second content region with the same smell

`/tmp/orchvis2-shots/03-changes-surface.png` — the Changes surface loads real data
(`Local changes (50)`, `Changed (3)` with per-file `−11 +146` counts, `Untracked (47)`) but the list
is **clipped to roughly 150 px**, cutting the `Untracked` header mid-row and leaving ~600 px empty
below it. The `surface.changes.open` reply captured two seconds earlier read
`loading:"true", ready:"false"` with all counts `0`.

Two scrollable content regions, one drawn at zero height and one at a height computed before its
async load arrived. Whether that is one root cause or two is exactly the question worth asking, and
`P113-triage.md` currently clusters neither.

## Still unjudged

Light mode. `theme` reads `system` in `surface.settings.read` and resolves dark here; there is no
control method that sets a settings value, so light needs the Appearance control clicked.

---

# 2026-08-14 17:55 — light mode, judged by measurement after my eye got it wrong

**Light mode is sound.** It is also the first surface closed by an orchestrator-driven synthetic
click, which is worth recording on its own.

## The click

```
click 1062 141        # the "Light" segment, at 1400x900
```

`surface.settings.read` before: `theme = system`. After: `theme = light`. The cursor is visible at
the click point in the capture. `P112`'s pointer works on a control I picked myself, not only on the
one it was demonstrated with.

Capture: `reference/linux-progress/2026-08-14-light-settings-appearance.png`.

## What I got wrong

My first read of the frame was that the settings nav column — AI Providers, Agents, General,
Permissions — was **washed out to the point of a light-mode contrast bug**, while the content column
beside it stayed crisp. It genuinely looks that way.

So I measured instead of reporting it. Darkest glyph pixel against local background, converted to a
WCAG ratio:

| region | dark | light |
|---|---|---|
| settings nav labels | **6.65:1** | **5.11:1** |
| content labels (Theme, Interface) | **12.58:1** | **12.63:1** |

**The nav is deliberately de-emphasised in both themes.** It is roughly half the content column's
contrast in dark too, so light did not regress it — light is modestly lower (6.65 → 5.11) and still
clears the 4.5:1 AA floor for normal text. There is no defect here.

The one thing worth watching: at 5.11:1 the light-mode nav is the **lowest-contrast text in the
app** and the closest to the floor. It is a fine place to stop, not a fine place to go lower.

## Why this entry exists in this shape

A washed-out impression was about to become a reported defect, on the strength of the same kind of
looks-right/looks-wrong judgement this project already pays for elsewhere. Two `convert` crops
settled it in under a minute. **When a visual claim can be measured, measure it** — the tooling is
`convert <png> -crop WxH+X+Y +repage -colorspace Gray -format "%[fx:minima*255] %[fx:maxima*255]"`,
and it is available on this machine.

## Evidence now committed rather than left in /tmp

- `reference/linux-progress/2026-08-14-light-settings-appearance.png`
- `reference/linux-progress/2026-08-14-chat-completed-turn-empty.png` — the 17:40 finding
- `reference/linux-progress/2026-08-14-changes-list-clipped.png` — the clipped Changes list

---

# 2026-08-14 18:05 — the Changes list clipping is confirmed, with a positive control

The 17:40 entry raised the clipped Changes list but could not rule out a resize artifact, because
`shot` alternates the output size to force a repaint. **It is not an artifact.** It reproduces at
both resolutions after a 12-second settle.

Measured rather than eyeballed — standard deviation of a region is zero only if nothing is drawn in
it:

| region (centre column) | 1715×972 | 1400×900 |
|---|---|---|
| rows band, y=130–230 | stddev **20.0** | stddev **24.0** |
| below the list, y=250–370 | stddev 4.0 | stddev **0.00** |
| below the list, y=400–700 | stddev **0.00** | stddev **0.00** |
| **control** — Files panel, y=400–700 | stddev **22.0** | — |

The control is the part that makes this evidence rather than an impression: the same measurement,
over the same rows, in the panel beside it, reads 22.0. **The detector is alive; the centre column
is empty.** (A passing positive control bounds false negatives only — it proves the measurement can
see content, not that it would catch every kind of content.)

So the Changes surface draws its header, its four action links, `Changed (3)` with three real file
rows and per-file `−11 +146` counts, then cuts `Untracked (47)` mid-row and draws **nothing** for
the remaining ~600 px.

Command, for replay:

```bash
convert <png> -crop 950x300+340+400 +repage -colorspace Gray \
        -format "%[fx:standard_deviation*255] %[fx:mean*255]" info:-
```

Captures: `reference/linux-progress/2026-08-14-changes-clipped-settled-{1715,1400}.png`.

`P117` owns the question of whether this and the empty chat transcript share one root cause.

---

## 18:09 — `F-PRJ-13`/`F-PRJ-15` really are fixed, but not for the reason the evidence said

Two separate findings from auditing `codex12`'s `P118` close-out. The verdict it wanted is right.
The evidence it left could not have established it.

### The codex panes can read images — that question is closed

`STATE.md` recorded an open question about whether the codex panes are text-only, after a
`• Viewed Image` line appeared in `codex12`'s transcript. `P118-report.md` claims
`p118-reset2-retry2/04-reset-settled.png` "shows the folder glyph, coral tint, and the full `Colour`
row after Reset." I read that frame. **Every clause is accurate** — selected folder glyph with a
coral selection ring, seven unclipped swatches, the Reset button, and the project id
`p-c1fd7a5bbfd541af` bottom-left matching the SQLite row the report quotes. It is a description of a
frame, not a confabulation. Treat `codex*` visual claims as first-hand, and keep judging them anyway.

The `F-PRJ-13` sub-defect of a **clipped `Colour` row** is also gone: seven swatches and the Reset
button below them all draw.

### The gap: the reset target and the default are the same value

`project_identity.rs:107-114` — `impl Default for ProjectIcon` returns
`Symbol(ProjectGlyph::Folder)` with `tint: AgentAccentColor::Coral`, commented "F-PRJ-13's reset
target: the plain folder glyph, undyed."

Every committed `P118` Cluster A capture exercises **Reset**, and Reset's target *is* the default.
So `p118-reset2-restart/02-reset-restart-sidebar.png` — a coral folder in the sidebar after a
restart — is exactly the frame an app that **persists nothing at all** would also produce. The
capture set never shows the sidebar carrying a non-default icon, and the sidebar is the whole claim:
`F-PRJ-15` says the grid's output is discarded, `F-PRJ-13` names an unwired `on_change`.
`03-settings-open.png` does show a non-default branch glyph selected — but that frame is the
settings card, and selection inside the card was never the part in doubt.

This is not a wrong verdict. It is a verdict whose proof does not discriminate against the null
hypothesis, which is the same failure mode as a green test over a transcript that draws nothing.

### The round trip, driven with a non-default icon

Captures in `reference/linux-progress/p118-orch-roundtrip/`. Sidebar coordinates are stable across
both of `shot()`'s resolutions because the column is fixed-width.

```bash
TILLER_WL_LABEL=orchp118b Scripts/wayland-drive.sh /tmp/orchp118b '
  ctl project.add path=/home/enzopalmisano/Scrivania/Progetti/tiller
  shot sidebar-default
  move 150 121          # the settings gear is .invisible() until row hover, sidebar.rs:2255
  sleep 1
  click 300 120         # the gear
  sleep 2
  shot settings-open
  click 106 267         # the git-branch glyph — NOT the default
  sleep 1
  click 171 371         # the green swatch — NOT the default
  sleep 1
  shot settings-chosen
  click 45 510          # Close
  sleep 2
  shot sidebar-after-close
'
# then, same label so /tmp/orchp118b.sqlite survives, and deliberately no project.add:
TILLER_WL_LABEL=orchp118b Scripts/wayland-drive.sh /tmp/orchp118c 'sleep 3; shot sidebar-after-restart; ctl project.list'
```

| step | frame | what it shows |
|---|---|---|
| default | `01-sidebar-default-coral-folder.png` | `tiller` row: **coral folder** |
| chosen | `03a-settings-branch-green-crop.png` | branch glyph ringed; every glyph re-tinted **green** live |
| **closed** | `04a-sidebar-after-close-crop.png` | `tiller` row: **green branch** — the selection crossed into the sidebar |
| **restarted** | `05a-sidebar-after-restart-crop.png` | `tiller` row: **green branch**, with no `project.add` in the drive |

Both halves now discriminate: the post-close frame differs from the default frame, and the
post-restart frame reproduces the chosen value rather than the default one. `project.list` returned
the project on the restarted instance without being asked to add it, so the project row itself is
durable too.

**For `pireview`:** `F-PRJ-13` and `F-PRJ-15` are `PASSED` on this evidence, not on the committed
`P118` captures. `project.list` does **not** expose `icon`/`tint` (`main.rs:465-482`), so no socket
assertion exists for this row and the frame is the only instrument — which is why it had to be
driven rather than read.

### The generalisation worth keeping

**When a row's fix and its default are the same value, a capture of that value proves nothing.**
Drive the row to a state the system would never reach on its own, then look. This is the visual
sibling of a positive control, and it is cheap: one extra click on a glyph nobody would have picked.
