# Wave D slice D-MAIN-1 — verdicts

Critic pass. No code edited under this slice's owned files (`rust/crates/tiller/src/main.rs`,
`rust/crates/tiller_ui/src/browser.rs`, `rust/crates/tiller_ui/src/chat.rs`) — `git status
--porcelain` confirmed clean on those three paths at the end of this pass. The builder's report and
`INTEGRATION.md` were read only for routes and claimed commits, never accepted as proof. Every row
was re-driven live on the Wayland lane (`Scripts/wayland-drive.sh`, binary at HEAD `4560076`),
labels `dm1crit`/`dm1brw07`/`dm1brw09*`/`dm1chat34*`/`dm1chatcascade*`. All scratch instances,
sockets, and `/tmp` scratch files were left under `/tmp` (never under this worktree).

**Trap encountered and worked around, not in `WAYLAND-LANE.md`:** every invocation of
`wayland-drive.sh` — even with `TILLER_WL_KEEP=1` and an unchanged `TILLER_WL_LABEL` — kills any
running instance for that label and launches a brand-new process before running its action block
(`kill_ours` at the top of the script runs unconditionally). `TILLER_WL_KEEP` only skips *teardown*
at the end; it does not make a label idempotent across separate invocations. Any drive that depends
on in-memory state set by an earlier command (e.g. `browser.act driving=true`, then a later `click`
on a resulting doorhanger) must happen inside **one** action-block invocation, or the state is gone
and the click lands on a fresh instance with different geometry. First-round F-BRW-05/07 drives that
split state-setting and clicking across two invocations produced misleading "the pill disappeared"
readings until this was diagnosed and everything was redone single-invocation.

**Machine load note:** this pass ran alongside ~15 other parallel critic instances (`pgrep -c
tiller` peaked at 30, load average >100 on 12 cores). Several `shot` calls came back blank
(`WARN ... is blank or near-blank`) under this contention and were retried with a longer `settle`
argument; blank frames were never used as evidence either way.

## F-AGENT-OMP-03 — UNREACHABLE (re-confirmed, blocked upstream)

Reran `oh-my-pi --version` directly on this box myself: identical `SyntaxError: Unexpected token
':'` at `oh-my-pi.js:176` (a `.js` file containing TypeScript type annotations run under plain
`node`) — the same upstream defect `F-AGENT-OMP-01`/`F-AGENT-OMP-02` are already `UNREACHABLE` for.
`cargo test -p tiller_agents omp_summarizer_command_uses_the_distribution_binary_name` passes and
its assertion body genuinely checks the `omp`→`oh-my-pi` substitution, not a trivial `assert!(true)`.
Independently grepped the whole tree for `summarizer_command`: every non-test call site is the trait
method definition itself (`omp.rs:102`, `opencode.rs:93`, `lib.rs:191`) — zero production callers
anywhere, confirming there is no code path in `main.rs`/`tiller_agents` left to fix. Upstream-blocked,
not a Tiller defect; matches the builder's claim and the prior critic's evidence.

## F-BRW-04 — PASSED

Live, single instance (`dm1crit`): `ctl browser.open url=https://` → `ok:false`, `"browser.open
failed: Only HTTP and HTTPS addresses are supported"`. `ctl browser.navigate
url=http://127.0.0.1:9/dead` on an open tab → `ok:false` within the request's own turnaround,
`"browser.navigate failed: Could not reach 127.0.0.1:9: Connessione rifiutata (os error 111)"`; the
same text rendered live in the Browser tab's red error banner (frame
`03-browser-navigate-dead.png`). Discriminating: the pre-fix default was a silent `ok:true` fallback
to `https://example.com` with no banner at all.

## F-BRW-05 — PASSED

Live, single instance: `ctl browser.act driving=true` rendered an **"Agent driving"** pill next to
the Browser toolbar's Stop button (frame `02-browser-driving-true.png`); `driving=false` cleared it
in the very next frame (`03-browser-driving-false.png`). Discriminating: no pill exists in the
toolbar by default (confirmed in every other Browser-tab frame taken this pass).

## F-BRW-07 — PASSED

Live, **one continuous instance** (`dm1brw07`, to avoid the restart trap above): `browser.act
driving=true` then `browser.navigate url=https://new-origin.example` → `permission:requested` +
doorhanger reading "Allow agent browser access to https://new-origin.example?" with Allow/Deny
(frame `02-01-doorhanger.png`). A real synthetic `click` on Allow (persistent virtual pointer, not a
socket shortcut) dismissed the doorhanger while the "Agent driving" pill stayed correctly set
(`03-02-after-allow.png`). The **same** `browser.navigate` call repeated immediately after no longer
returned `permission:requested` — it proceeded past the gate into the actual navigation attempt
(`browser.navigate failed: Could not resolve new-origin.example:443: ...`, a DNS failure from
F-BRW-04's own reachability probe, expected since the domain is fictitious) — proving the grant took
effect and unblocked the gate rather than merely re-showing it.

## F-BRW-09 — PASSED

`event.modifiers.platform && event.modifiers.shift` gate confirmed at `chat.rs:828` (source read).
Driving live required a real assistant-authored markdown link, since neither the shipped
`acp_fixture.py` modes nor `surface.chat.compose`-posted user text produce one (`compose`d text
renders as literal unlinked text per `WAYLAND-LANE.md`'s own note). Wrote a minimal, standalone
ACP-v1-speaking Python script (`scratchpad/link_fixture.py`, **not** added to the repo — no file
under `rust/` touched) that answers `initialize`/`session/new`/`session/prompt` with an
`agent_message_chunk` containing `[CRITIC_LINK_MARKER](https://critic-verify.example/f-brw-09)`, and
pointed `TILLER_ACP_PROGRAM` at it — a real subprocess speaking the real protocol, not a mock inside
the test binary.

Live, one continuous instance: sent a real turn, the link rendered in the transcript
(`02-chat-rendered.png`). A plain synthetic `click` on the rendered link opened a **new internal
Browser tab** navigated to the exact URL from the fixture (`03-after-plain-click.png`) — the
`ChatEvent::OpenLink` → `add_browser_tab` path. A second turn's identical link, clicked with
**Shift+Logo held** (`platform` on Linux is the Super/Logo key per `gpui::Modifiers`'s own doc
comment; composed as two parallel `wtype -M` holds since `wayland-drive.sh`'s `modclick` only
supports one modifier) produced **no** new Browser tab — the tab strip stayed `Chat`/`Terminal`
(`03-after-shift-logo-click.png`) — the negative, discriminating result matching the code's other
branch. Caveat: this sandboxed lane has no real browser/xdg-portal to independently observe an
external window opening, so the external-open half rests on (a) the source-confirmed if/else being
exhaustive and (b) the live-confirmed absence of the internal-tab path under the modifier chord, not
on directly observing `xdg-open` fire.

## F-CHAT-34 — FAILED — defective (persisted history is destroyed by ordinary session autosave)

The report's own claim ("Live-verified... Chat tab → overflow → Chat History → 'No past chats'")
is, on inspection, only proof of the *empty*-state half (F-CHAT-35) — it never demonstrates a
completed turn surviving into the list. Re-driving that missing half live found a real, reproducible
defect, not a gap in verification effort.

**Root cause, source-read:** `tiller_persistence::db::save_tabs` (`crates/tiller_persistence/src/db.rs:377`)
does `DELETE FROM tab WHERE worktree_id = ?1` then reinserts every tab row — including tabs whose id
and content are unchanged — on **every** session-layout autosave. The schema declares `chat_turn.tab_id
TEXT NOT NULL REFERENCES tab(id) ON DELETE CASCADE`. `schedule_save` (28 call sites in `main.rs`,
firing from ordinary actions: tab creation, `project.add`, focus changes) therefore cascade-deletes
every persisted chat turn for the worktree on almost any routine action, not only on an intentional
tab close.

**Live reproduction, one continuous process (`dm1chatcascade2`), no restart involved:** sent a real
turn through the same real ACP script as F-BRW-09; `surface.chat.read` showed it `completed` with a
real assistant reply. Queried the live sqlite file directly at that instant: `chat_turn` held **1**
row. Issued exactly one further, entirely ordinary action in the same unbroken process —
`ctl tab.select index=1`, needed merely to bring the tab into view, not a delete/close/anything
destructive — and re-queried: `chat_turn` was **0**. `surface.chat.read` in the same moment still
showed the turn (it survives in the live `Chat` entity's in-memory `entries`), so the break is
specifically that the *durable* copy is gone. Opening the composer overflow → Chat History
afterward correctly reflected the now-empty database: "No past chats", **not** because
`load_chat_history`/`chat_sessions` exclude the tab's own session (read `chat_sessions`'s SQL —
no such filter exists) but because the row genuinely was gone.

This is not a rare edge case: `schedule_save` is wired into nearly every workspace action, so in
ordinary use a completed turn has at most a few hundred milliseconds before the next autosave wipes
its durability. The row's own UI code (list/Open/Delete-with-confirm in `chat.rs`) reads correctly
on its own, but the feature's data foundation self-destructs, so "the session is listed" — the
row's core, stated behaviour — could not be observed to work even once across two independent live
attempts. Note for whoever owns this next: the fix belongs in `tiller_persistence::db::save_tabs`
(an upsert that leaves unchanged tab rows alone, or a temporary `PRAGMA defer_foreign_keys`/re-insert
of `chat_turn` in the same transaction), not in this slice's three owned files.

## F-CHAT-35 — PASSED

Live: a genuinely fresh chat tab, verified via the sqlite file to have zero `chat_turn` rows for its
tab id, shows the Chat History popover with **exactly** "No past chats" (frame
`04-03-chat-history-empty.png`), matching the Swift original's string. Discriminating: the same
popover UI would render session rows instead when the database holds any — confirmed structurally
(`chat.rs`'s history-row rendering) and would have been directly observed here too had F-CHAT-34's
persistence not been wiped out from under it.

## Files touched this pass

None under this slice's owned files (`main.rs`, `browser.rs`, `chat.rs`) or anywhere else in
`rust/`. `git status --porcelain` on those three paths is clean. The one file this pass wrote,
`link_fixture.py`, lives entirely under `/tmp/.../scratchpad/` and was never added to the repo.
