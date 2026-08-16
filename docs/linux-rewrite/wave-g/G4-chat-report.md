# Wave G slice G4-chat — report

## F-EDIT-07 — fixed

Root cause confirmed exactly as the critic described: `chat.rs`'s `render_markdown_block`
`Block::CodeBlock` arm rendered the fenced body through `render_plain_text`, which builds a
`StyledText` with zero `HighlightStyle`s — every token in a fenced code block, in every language,
rendered in one flat color, while `Inline::Code` (backtick spans) already carried real highlighting.

Fix: reused the editor's own tokenizer instead of inventing a second one. `code_spans` /
`CodeSpan` / `CodeSpanKind` in `file_view.rs` were `fn`/private; made `pub(crate)` so `chat.rs` can
call them. Added `Chat::render_highlighted_code` (mirrors `render_plain_text` but builds
`StyledText::with_highlights` from `code_spans` run per line, offsetting each span's byte range into
the whole block) and `Chat::language_from_fence_tag` (maps a fenced block's info-string tag —
`bash`, `rust`, `py`, etc. — to the editor's `Language` enum, since chat.rs can't reach
`editor.rs::Language::from_path`, which is extension-based, or add a tag-based constructor there
without owning `editor.rs`). Wired into the `CodeBlock` arm in place of the old `render_plain_text`
call for the block body.

Two new `chat.rs` unit tests: `fence_tag_resolves_to_editor_language` (tag→Language mapping) and
`shell_code_block_produces_keyword_and_comment_highlights` (the exact shell comment+keyword case the
critic's pixel inspection caught rendering flat). Both pass; full `chat::tests::` suite (62 tests)
green; `cargo build -p tiller` green.

**Live-verified** (not just unit-tested): built the fix into `target/debug/tiller`, drove it under
`wayland-drive.sh`, created a scratch git repo `/tmp/g4chat-proof` with a `README.md` containing an
inline code span and a fenced ` ```bash ` block (comment line + `echo "hello world"`), added it as a
project, double-clicked `README.md` in the Files panel (opens straight into Preview mode), and
4x-zoomed the rendered fenced block. Result: the `# comment line` renders in the dim meta/gray tone,
the `"hello world"` string literal renders in green (`diff_addition`), and `echo` (not a shell
keyword in the vocabulary) stays plain — three visibly distinct colors where the critic's screenshot
showed one flat color throughout. Screenshot: `/tmp/g4chat-shots/zoomed-code.png` (not committed —
ephemeral drive output; reproducible via the steps above).

Commit: `140f7cf`.

## F-CHAT-05, F-CHAT-20 — not advanced past half-proven; environment findings recorded

Both rows' code was re-read and still matches the prior critic's description: the offline-placeholder
branch in `Chat::render`'s composer_parts (F-CHAT-05) and the `cx.defer` tail-follow re-arm in
`scroll_handler` (F-CHAT-20) are unchanged and still mechanically correct on inspection. I spent this
pass's live-drive budget trying to independently drive both and hit two reproducible environment
obstacles worth recording for whoever picks these up next:

1. **`wayland-drive.sh` restarts the app on every invocation, even with `TILLER_WL_KEEP=1`.**
   `kill_ours TILLER_SOCKET "$SOCK" tiller` runs unconditionally near the top of the script, before
   the `TILLER_WL_KEEP` check (which only guards the exit-time `cleanup()` trap). Every separate
   script call — even against the same `TILLER_WL_LABEL` — kills and relaunches a fresh `tiller`
   process with a new PID. File-view tabs are not session-restorable (confirmed via the app log:
   `tab "README.md" (file) is not restorable in this build; skipped`), so any multi-step scenario
   that needs an open file tab, an open menu, or a live subprocess to persist across steps must be
   expressed as **one** script invocation (raw bash is available inside the action block — I used it
   successfully to discover and kill a chat's underlying ACP agent subprocess by walking
   `/proc/<pid>/environ` for `TILLER_SOCKET=$SOCK` to find this instance's `tiller` PID, then
   `ps -o ppid=` ancestor-walking candidate `claude`/`claude-agent-acp` processes to confirm descent
   from that PID before killing).

2. **Composer text entry (`type`) did not register in this pass**, repeatedly: clicking inside the
   composer's visible bounds (verified against the just-captured screenshot's own coordinates) and
   then `type ping` left the placeholder ("Message...") on screen with no visible change and an
   identical post-type colour count. I could not get a message into the composer to force a real
   agent spawn+kill cycle for F-CHAT-05, nor send several turns to exercise F-CHAT-20's tail-follow.
   The "New Chat" tab-bar menu's submenu (to pick an agent for a fresh ACP-backed Chat tab, as
   opposed to "Claude Code" which opens a plain terminal running the CLI directly) also never
   visibly opened on click, in three attempts. Both symptoms are consistent with the "environment
   instability" the F-CHAT-20 critic already flagged; I did not find a workaround this pass.

What I *did* confirm live: an already-connected worktree's Chat tab (from a project added earlier
in the session, with the agent auto-spawned) showed `idle` / `Opus Plan Mode` and a plain
`Message...` composer — i.e. the *online* branch, not the offline one. Killing that worktree's real
`claude`/`claude-agent-acp` process tree (found via the `/proc` walk above) did produce a
`TransportError` transition in the app (the app log and process tree both confirmed the kill landed
on the right descendant), but by the time I captured the next frame the `wayland-drive.sh` instance
had already been recycled by a subsequent invocation before I could screenshot the resulting
composer state, for the reason in (1) above. No new evidence for or against the offline-placeholder
render in a live capture; the passing `offline_composer_shows_its_own_placeholder` unit test (which
drives the same code path with a genuinely-missing binary) is the strongest evidence that exists so
far.

Both rows: left at half-proven. No files changed for these two.

## F-CHAT-33 — not attempted this pass

`looks_like_mcp_warning` and its call site are confirmed still present and unmodified (`grep` in
`tiller_ui/src/chat.rs`). Per the house rules and the wave brief's own instruction to not spend a
pass re-deriving what's already been tried: this row already has three independent live attempts
across two prior sessions with negative results and no code changes since. Given this pass's live-drive
time was consumed by the composer-typing and menu-flyout obstacles logged under F-CHAT-05/20 above
(which would have blocked a fourth MCP-warning attempt in the same way — no message could be sent
into the composer to trigger an MCP warning), a fourth identical attempt was not made. No files
changed for this row.
