# Wave A slice W03-chat — 6 rows needing no source change

Triage says each of these needs only exercising, or that its verdict looks
wrong. **Change no code. Do not edit the ledger.**

## `F-CHAT-13` — ledger line 162, currently **NOT EXERCISED**

- **Triage says:** reclassify
- **Approach:** grep -rn ExternalPaths crates/ is empty repo-wide; chat.rs has zero .on_drop of any kind. This is code-absent, confirmable without any driver tooling, not NOT EXERCISED as currently recorded — should read FAILED — absent like F-CHAT-35. Build: an ExternalPaths drop target, the attach overlay, chip/message wiring, and a rejection state for unsupported types.
- **Evidence on record:** Wayland-lane '+' control reached (cursor confirmed on it, 4 attempts) but produced no popup/dialog in any capture -- confirmed by direct frame inspection, not the driver's prose. Independently re-confirmed wayland-virtual-pointer.c has no drag-offer/wl_data_device call. Same instrument-unreachable verdict as the prior X-lane pass, now cross-checked on a second lane.

## `F-CHAT-18` — ledger line 167, currently **FAILED — absent**

- **Triage says:** reclassify
- **Approach:** A fully click-wired (toggle_context_popover on context-ring's on_click), Escape-dismissable, drawn-tested (context_ring_shows_reported_usage_and_escape_dismisses_popover) context popover already exists at HEAD, directly contradicting the 'opens nothing' verdict — captured in the same pre-h_full-fix P109 batch as F-CHAT-02. Re-drive first. Real residual gap once re-verified: ContextUsage carries only used/size/cost, no separate input/output/cache breakdown the clause names.
- **Shared cause:** Same pre-h_full-fix P109 drive batch as F-CHAT-02 and F-CHAT-16 (commit 2f3a1b4, 18 min before fix 23af26c).
- **Evidence on record:** Live-driven: context-indicator click opens nothing, tested both before and after a send — no percent/token/cost popover ever appears. shots/45,55.

## `F-CHAT-20` — ledger line 169, currently **half-proven**

- **Triage says:** exercise
- **Approach:** push_entry already checks list_state.is_following_tail() before re-pinning (GPUI ListState/FollowMode primitives) -- the mechanism exists. Only the Wayland-lane instrument lacks a scroll/axis primitive; the X11 lane's xdotool click 4/5 wheel event is untried and is the natural next attempt.
- **Evidence on record:** Follow half re-confirmed live (02-f20-mid-stream.png), adds no new proof over pass 17. Independently re-grepped both wayland-drive.sh's DSL and wayland-virtual-pointer.c and confirmed neither exposes a scroll/axis primitive -- manual-scroll-ownership half remains genuinely unreachable on this lane's current tooling, not merely unattempted.

## `F-CHAT-25` — ledger line 174, currently **NOT EXERCISED**

- **Triage says:** exercise
- **Approach:** Entry::Permission.text_input / render_question_answer_row already implement the free-text answer path and are drawn-tested; option-answer and cancel/dismiss halves of the same mechanism are already live-proven via F-CHAT-24. Blocked only by this build's Claude Code CLI never raising a real AskUserQuestion over ACP (falls back to plain chat text). Try Pi's ui/select or Codex's adapter.
- **Evidence on record:** Confirmed live and directly (not inferred): this installed Claude Code CLI build reports 'no AskUserQuestion tool available... isn't wired up here' over ACP and asks the question as plain chat text instead -- no Entry::Permission/question card ever created, nothing to click. Environmentally blocked for this CLI, not a code-absent finding; Pi's ui/select flagged as the best untried lead for a follow-up pass.

## `F-CHAT-26` — ledger line 175, currently **half-proven**

- **Triage says:** exercise
- **Approach:** pending-question-bar / Show / scroll_to_reveal_item all exist and are correct in source. Every capture in the drive shows the Terminal tab foregrounded, not Chat (tab-bar crop confirmed) -- re-drive with the Chat tab explicitly foregrounded before each capture.
- **Shared cause:** Same wrong-tab-foregrounded drive-sequencing bug as F-CHAT-27.
- **Evidence on record:** Backend half proven live: a real ACP plan-mode turn produced a genuine Entry::Permission{resolved:None,expired:false} row over surface.chat.read (status:pending), matching pending_question() (chat.rs:1490) -- confirmed against source, not just driver prose. Visual half NOT proven as claimed: every capture in the drive (tab-bar crop checked) shows the Terminal tab foregrounded, not Chat, so the driver's stddev pixel-b

## `F-CHAT-27` — ledger line 176, currently **half-proven**

- **Triage says:** exercise
- **Approach:** expire_unanswered()/control_entry_row and the 'No answer — the turn ended' render text both confirmed present in source on the expired branch of Entry::Permission. Same wrong-tab (Terminal, not Chat) capture bug as F-CHAT-26 -- re-drive with Chat foregrounded.
- **Shared cause:** Same wrong-tab-foregrounded drive-sequencing bug as F-CHAT-26.
- **Evidence on record:** Data-model half proven live: surface.chat.stop flipped a live permission entry pending->expired and appended a TurnFooter cancelled row, matching expire_unanswered()/control_entry_row exactly (cross-checked in chat.rs source: render arm at 3839-3846, mirror at 5448-5449, call site ~1479-1512). Visual half NOT proven: the claimed 'post-expiry frame' 03-after-stop.png is actually the Terminal tab foregrounded (tab-bar 

