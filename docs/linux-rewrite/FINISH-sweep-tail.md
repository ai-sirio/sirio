# FINISH-sweep-tail — thirteen half-proven rows nobody else owns (lane wf-sweep)

Fresh critic pass, 2026-08-18, this host (x86_64 desktop, COSMIC/Wayland, per `ENVIRONMENT.md`'s
2026-08-18 section). Evidence standard: `EVIDENCE-STANDARD.md` — a verdict without a named,
replayable transcript is not a verdict. Lane: `Scripts/wayland-drive.sh`, binary pinned per
`ENVIRONMENT.md`:

```bash
cargo build --manifest-path rust/Cargo.toml
cp rust/target/debug/tiller /tmp/wf-sweep-tiller && export TILLER_WL_BIN=/tmp/wf-sweep-tiller
```

`TILLER_WL_LABEL=wf-sweep`, `TILLER_DB=/tmp/wf-sweep.sqlite`. Screenshots referenced below are
committed under `reference/linux-progress/wf-sweep/`.

## A trap this pass hit and is recording for the next one

Every row here needed a right-click context menu, then a click on one of its items. The FIRST
attempt at every such gesture, done as `rightclick <x> <y>` immediately followed by `click <x> <y>`
with nothing between them, silently failed: the click visibly landed on the correct item (hover
highlight showed in the screenshot) but the menu never closed and no side effect occurred — the
click was accepted by the compositor and delivered, but arrived before the freshly-opened menu's
subtree was truly hit-testable (the same "~2 real frames before linking" issue `wayland-drive.sh`'s
own comments document for `tab_bar.rs`'s `deferred(...)` menus, evidently shared by the sidebar's
own context menu). A visible `sleep 1` between `rightclick` and the item `click` fixed it
consistently for every row below. **Do not trust a bare `rightclick`+`click` pair with nothing
between them; the menu opening and the item being clickable are not the same frame.**

Also hit repeatedly this pass: `MESA: error: ZINK: failed to choose pdev` / `Io error: Broken pipe`
at app startup — `ENVIRONMENT.md`'s documented GPU/compositor contention from ~5-6 concurrent
sibling lanes sharing `/dev/dri/renderD128`. Fully environmental (confirmed via `ps aux` showing
`wf-chg`, `wf-tab`, `wf-rest`, `wf-rest2`, `wf-act` all alive at once); resolved by a clean
kill+retry loop, never a code concern.

## Rows

### F-SID-08 — PASSED (upgraded)

Clause: right-click a non-Git project, choose **Initialize repository**, confirm the project
changes to Git-backed behavior. The half already proven (wave F) was the underlying action via the
Project Settings sheet's button; the context-menu entry point itself was unproven.

Live drive: added a real non-git folder project (`/home/enzopalmisano/wf-sweep-nongit`, confirmed
empty, no `.git`, via `ls -la` before). Right-clicked its sidebar row — menu showed **Initialize
Git repository** enabled (a sibling git-backed project's menu in the same drive showed it disabled
with reason "Git is already initialized", confirming the menu reads real per-project git state).
Clicked it (`sleep 1` between open and click — see trap above). Result, both halves of a hard
discriminator:

- **Disk**: `/home/enzopalmisano/wf-sweep-nongit/.git` now exists (`find` before: nothing under the
  folder; `find` after: `.git` present) — a real `git init` ran.
- **UI**: the row transformed from a plain folder project (no branch child) into an expandable
  git-backed project with a `master` worktree row and a `Primary` badge, matching the sibling
  project's own shape — `reference/linux-progress/wf-sweep/f-sid-08-context-menu-init-git.png`.

Both "confirm the project changes to Git-backed behavior" and the specific context-menu entry
point (not the Project Settings sheet) are now driven.

### F-CHAT-25 — PASSED (upgraded)

Clause: trigger a question, enter text and click Send; repeat with a listed option; repeat with
Cancel, confirming answered/cancelled states. The unproven leg named in the brief: "AskUserQuestion
... which a sibling explicitly did not re-drive." Reading the existing suite found two solid named
tests already covering two of the three arms — `a_text_answer_leaves_the_surface_and_clears_the_pending_bar`
and `cancel_on_a_question_closes_it_without_an_answer` (`rust/crates/tiller_ui/src/chat.rs`) — but
no test anywhere exercises a **listed option** click. Tracing the render code
(`Entry::Permission` in `chat.rs`) and the fixture confirmed why: `chat_fixture.py`'s existing
`question` mode always sends `"options": []` at the wire top level specifically so the client is
forced onto the free-text field (its own docstring says so); nothing in the fixture ever sent a
structured question with populated wire options, so the "render clickable pills instead of a text
field" branch (`else if let Some(input) = text_input ... } else { for option in options { ...
permission-option-<id> ... } }`) had no test at all.

Closed it properly rather than routing around it: added a `question-options` mode to
`rust/crates/tiller_ui/tests/fixtures/chat_fixture.py` (purely additive — a new
`request_question_with_options()` sending real wire `options: [blue, green]` alongside the
`rawInput.questions[...]` shape, no existing mode touched) and a new named test
`a_listed_option_leaves_the_surface_and_clears_the_pending_bar` in `chat.rs`, sibling to the two
above. It asserts `question-answer-input` (the text field) is **absent**, both
`permission-option-blue` and `permission-option-green` pills are drawn, clicks the Blue pill with
`cx.simulate_click`, and confirms: the card records `resolved == "Blue"`, `pending-question-bar`
disappears, and the agent's echoed reply (`"You picked: blue"`) lands in the transcript — the full
round trip, not just the click registering.

```
cargo test --manifest-path rust/Cargo.toml -p tiller_ui a_listed_option_leaves_the_surface_and_clears_the_pending_bar
test chat::tests::a_listed_option_leaves_the_surface_and_clears_the_pending_bar ... ok
```

Re-ran the two sibling tests plus the full `chat::` module (`cargo test -p tiller_ui --lib chat::`)
to confirm nothing regressed: 76 passed, 1 unrelated failure
(`stopping_via_click_with_a_queued_item_still_sends_it`) that reproduces only under full-suite
concurrency and passes clean in isolation — a pre-existing flake, not something this change
touched (confirmed by running it alone: `ok`).

All three arms of F-CHAT-25 now have named, replayable, real-event-dispatching evidence: text
answer, listed option, and cancel.

