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
