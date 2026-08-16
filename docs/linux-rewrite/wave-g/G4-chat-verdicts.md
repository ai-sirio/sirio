# G4-chat — critic verdicts

Critic pass, 2026-08-16. Independent of the builder; live-driven via `Scripts/wayland-drive.sh`
against a scratch project (`/tmp/g4chat-critic/proof`), fresh from this repo's tree at HEAD. Screens
referenced below are ephemeral (`/tmp/g4chat-critic/...`), not committed — reproducible via the steps
described in each row.

## `F-EDIT-07` — PASSED

Live-verified independently, reproducing the builder's exact route. Created a scratch git repo with a
`README.md` containing an inline code span and a fenced ` ```bash ` block (`# a comment line` +
`echo "hello world"`), added it as a project over `project.add`, selected the worktree, double-clicked
`README.md` in Files (opened straight into Preview). 4x `convert -crop -resize 400%` on the rendered
block shows three visibly distinct colors: `# a comment line` in the dim/meta gray tone, `echo` in
plain white/bold, `"hello world"` in green — matching the builder's claim exactly and confirming the
prior "flat single color" defect is gone. Screens: `/tmp/g4chat-critic/shots3/02-double-click-readme.png`,
cropped zoom `/tmp/g4chat-critic/zoomed.png`.

## `F-CHAT-05` — half-proven

Drove past the exact obstacle the builder reported blocked them (composer `type` "not registering"):
in my session, `type`/`key Return` worked reliably and repeatedly across many real turns. Reached a
genuine offline state live (Codex ACP agent refuses to launch without auth: "could not launch ACP
agent: ACP agent requires authentication") and confirmed the composer shows the distinct
`Agent offline — reconnecting when you send...` placeholder, matching the offline branch's intent.
However, typing into that composer was NOT refused — I typed `hello offline test` and it was accepted
and displayed; pressing Return silently cleared the draft (matches chat.rs's own documented
"typing + Enter genuinely inert -> silent reconnect-and-retry" comment) rather than visibly disabling
the field. This is a real discrepancy against the row's literal "confirm the editor is disabled"
clause for the offline sub-case specifically (the permission-wait sub-case's disabling remains
test-only this pass, not independently live-driven). Net: placeholder half newly live-confirmed;
disabled half newly live-contradicted for offline (a pre-existing, documented tradeoff, not a
regression this pass) — kept at half-proven with materially stronger evidence than before.

## `F-CHAT-20` — half-proven

Sent three real ACP turns (haiku, a 40-item list, a 60-item list) to a live Claude Code chat and
confirmed the transcript correctly followed streamed output to the tail every time — each turn settled
with its last line visible and the composer idle, never stuck mid-scroll. Attempted to independently
prove the second half (manual scroll-away decouples the tail-follow, and re-pinning at the bottom
resumes it) by scrolling up mid-stream and capturing across a wait; captures taken seconds apart during
active streaming repeatedly returned frames with an identical stale progress percentage and stale
transcript content, consistent with a capture-pipeline staleness issue in this harness rather than
proof either way about the app's scroll-decouple behavior. The follows-new-output half is now live
confirmed; the decouple/re-pin half stays unconfirmed live this pass (code review only, as before).

## `F-CHAT-33` — half-proven

Made a fourth independent live attempt (the builder and two prior sessions made three). Configured a
genuinely broken `.mcp.json` (`mcpServers.broken-server.command` pointing at a nonexistent binary) in
the scratch project, opened a brand-new Claude Code ACP chat tab against it (fresh process, so it would
read the new config on launch), and sent a real turn. No MCP warning banner appeared in the transcript
after the tab connected or after the turn completed. This is a fourth negative data point but remains
inconclusive rather than proof of absence: I have no visibility into whether the Claude Code CLI's ACP
server actually wrote a stderr line matching `looks_like_mcp_warning`'s pattern for this specific
failure mode (unreachable binary vs. e.g. an unreachable network MCP server), so I cannot rule out the
detector working correctly against a warning shape this config never produced. `looks_like_mcp_warning`
and its call site remain unmodified in the tree. Left at half-proven.
