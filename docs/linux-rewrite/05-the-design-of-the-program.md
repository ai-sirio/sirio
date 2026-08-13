# 05 — The design of the program

Written by `fable` (w1:pD) at 15:40 under FABLE-01. This file is the synthesis the reference set
was missing: `00` records what the old app did, `03` measures how waku looks, `04` names what waku
cannot answer — and no document decided what *this* program is. Builders have been making that
synthesis per piece, without eyes: every builder and the critic are text-only models, so the only
participants who have ever seen a frame are the orchestrator and fable. This document makes the
design decisions. Per FABLE-01 they are binding unless the operator overrules; briefs should cite
them by number (D1…D8, J1/J2).

**Evidence discipline.** Claims marked **[frames]** come from pixels I read myself: waku's
`app-screenshot-{dark,light}.png` (2266×1752) and our `/tmp/probe-2.png` (10:25, 1440×833, 8820
colours — the last whole-app capture in existence). Claims marked **[tree]** come from code or the
ledger. No claim is made about pixels newer than 10:25; there are none to read, and none can be
made until the compositor is restarted.

## The thesis

Tiller on Linux is **a chat-first agent workbench**: the unit of the user's attention is *a
conversation with an agent on a worktree*, and the program's job is to let several of those run
side by side without the chrome of an IDE. waku proves the language for the conversation. Tiller
adds the three things waku deliberately lacks — many worktrees, terminals, a diff surface — and
the skipped design act is deciding how those live *inside* a reading-first program rather than
around it.

The two-state rule that resolves most layout questions: the app's **resting state is a reading
surface** — sidebar plus one column, waku's frame; the **workbench state is summoned** — right
panel, splits, terminals appear when asked and recede. The old app kept everything on screen at
once; that, more than palette or type, is the difference between the two frames **[frames]**.

## The verdict on what exists

**[frames]** probe-2 at 10:25 is `00-ui-observed`'s macOS composition wearing `03`'s palette:
three columns all open; a projects disclosure tree whose rows repeat the same grey
`/home/enzopalmisano/…` path subtitle ten times in one viewport; a thin tab strip; the inspector
open by default on a raw directory listing, dotfiles first. waku's dark frame shows a curated
session list — title, project chip, relative time, no controls on the row — one 720px reading
column, and roughly eight visible controls in the whole window; ours shows forty-plus. The
graphite ground, row proportions and neutral selection are genuinely waku's — the token work
landed — but composition, information-per-row and chrome budget are still the old app's.
`CRITIC-visual-baseline.md` predicted exactly this ("a dense IDE wearing someone else's colours");
its IA and chrome-budget verdicts were consumed as a type-scale fix and never became decisions.

**[tree]** The ledger agrees. PASSED concentrates machine-facing (F-CTRL 21, F-AUTO 7, F-AGENT 9);
FAILED — absent concentrates in the human surfaces (F-CHAT 19, F-TAB 15, F-EDIT 11, F-SID 9).
There is no empty state anywhere in `tiller_ui` or the shell, no command palette, no stop control
on a running turn, no queued-typing behaviour — the last two are the first behaviours `00` lists.
The composer, which `03` Part D calls "the hardest single component", got its first dedicated
piece at hour 15 (P44).

## The decisions

**D1 — The sidebar is a session list, not a filesystem.** A row is *a worktree session*: branch
name at 13.5px; project chip, activity state and relative time at 11.5px; the 51px two-line card
anatomy from `03`. **Paths never appear in sidebar rows** — paths are status-bar and tooltip
material. Projects become group headers (waku's Yesterday / This Month position), not disclosure
parents; open tabs stop nesting under worktrees (the tab strip owns them; the activity glyph
stays on the worktree row). `+ New Worktree` is the sidebar's one primary CTA, in waku's New Task
position. This dissolves probe-2's repetition structurally **[frames]**.
VERIFY (drawn): a row contains branch + project + time, contains no `/` path text; one CTA above
the fold; selection is the 6% neutral fill.

**D2 — Commands live in a palette; there is no menu bar.** `03` §C.3 already establishes native
menus do not exist on Linux; waku's answer is the command palette and it is the right one here.
The command layer codex12 built gets exactly three doors: a `ctrl-shift-p` / `ctrl-k` palette
listing every typed action with its chord; right-click context menus on sidebar rows, tabs and
panes; the existing `+` menu. **No in-window menu-bar strip** — permanent chrome for the rarest
actions. This supersedes P46's "the chrome that exposes it is a judgement call"; if P46 landed a
different front door, reconcile toward the palette rather than keeping both.
VERIFY (drawn): palette opens on the chord, filters, and dispatches one sidebar action and one
tab action through the real dispatch path.

**D3 — The composer is the anchor of the chat surface.** waku's anatomy wholesale **[frames]**:
one card, radius 13, `composer` fill, max-w 720, roomy placeholder, a single labelled chip row —
agent, model (drawn-PASSED), mode, context ring (drawn-PASSED) — and a circular send control that
**becomes stop while a turn runs** and shows the queued state while typing mid-turn. The 25
absent F-CHAT entries are built *into* this anatomy, not bolted around it: stop (F-CHAT-07),
queue (F-CHAT-06), slash / mention / attachment surfaces as popovers above the card.
VERIFY: the existing drawn-test pattern, per entry, plus D-CHAT rows below.

**D4 — Empty states are the front door.** Three designed moments in waku's anatomy (icon, 20px
headline, 12.5px description, primary + secondary CTA): app with no project ("Add a project");
worktree with no tab ("Start Claude Code" / "Open terminal"); chat with no turns. Zero exist
today **[tree]**; all three are drawn-testable now.
VERIFY (drawn): per state — headline present, CTA dispatches the real action.

**D5 — Chrome budget: counted, and the right panel rests closed.** waku has no inspector; ours
opens on a directory dump **[frames]**. Files / Activity / Changes stay one summon away (chord,
palette, activity badge). The resting chat state permits **at most 12 visible interactive
controls window-wide** (waku shows ~8 **[frames]**; the number is revisable — the point is that
it exists and is enforced). A brief adding an always-visible control must name what it displaced.
VERIFY (drawn): resting frame's interactive-element count ≤ 12; right panel absent at rest.

**D6 — Tabs stay, and stay quiet.** Tiller genuinely needs tabs — chat, terminal, and Changes as
a peer tab (orca's placement, already adopted, correct). One 11.5px chip row; hover-revealed ✕
(F-TAB-15's pattern); overflow menu at the strip's end; activity conveyed by the existing glyph
states, never by colour fills; no second row; no per-tab paths.

**D7 — Terminals are inset panes, not chrome.** Structure only (appearance is display-debt):
`terminal_surface` inset, hairline split borders, no per-pane toolbars; pane focus shown by the
existing focus contract, not a coloured header.

**D8 — What stops.** No further F-WIN / F-SID leftovers, no browser-tier scaffolding, and no new
socket surface beyond what verification consumes, until J1 below is critic-green. The ledger
remains the truth of verification; it stops being the work queue.

## The rows

Adopt into the ledger as first-class entries, so the contract finally contains the goal's first
requirement: `D-SID-01` row anatomy · `D-SID-02` no paths in rows · `D-SID-03` group headers ·
`D-CMD-01` palette · `D-CMD-02` context menus · `D-CHAT-01` composer anatomy · `D-CHAT-02`
send/stop modal control · `D-CHAT-03` queued typing · `D-EMPTY-01/02/03` · `D-CHROME-01` budget
≤ 12 · `D-CHROME-02` right panel closed at rest · `D-TAB-01` quiet strip. Each carries its VERIFY
clause above. Behaviour halves are drawn-testable today; appearance halves join the display debt
beside the existing seven, and are judged against waku's frames when a display exists — by a
participant who can see.

## The journeys

**J1 — the product's spine:** launch clean → D-EMPTY-01 → add project → D1 sidebar → open
worktree → D-EMPTY-02 → start a Claude chat → D3 composer → streamed turn with tool card and
permission card → **stop it** (D-CHAT-02) → type during the run and see it queue (D-CHAT-03) →
done state on the sidebar row → relaunch, the session restores. **J2 — trust the diff:** an agent
edits → Changes tab → collapsed bands read and expand → stage / unstage / discard live → status
propagates to the sidebar. J1 and J2 become ledger entries themselves, exercised end-to-end in
one critic run — the integration evidence per-entry ticks cannot give, and the only check that
catches the crate-right-surface-wrong class *between* entries rather than inside one.

**The queue rule:** a builder takes the next absent row *on the active journey*, not the largest
coherent block it owns. Breadth resumes when J1 is critic-green.
