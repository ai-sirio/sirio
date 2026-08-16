# I4-settle report

Three rows, each already carrying a recorded diagnosis from prior sweeps. Task: reach each one with a
better instrument, or record the honest terminal verdict with its reason. None left at `half-proven`
by default.

## `F-WIN-09` — titlebar double-click preference — **reached with a better instrument; no code change**

Prior finding (`wave-h/H4-window-verdicts.md`): code correct, but the only harness tried (nested sway)
force-fullscreens its single window regardless of any WM request, making "the window minimizes"
categorically unobservable there. The brief asked to try the X11 lane (`Scripts/linux-drive.sh`,
`DISPLAY=:1`) or a nested sway with more than one window.

Tried the X11 lane. `DISPLAY=:1` runs under a real window manager ("Smithay X WM", confirmed via
`_NET_SUPPORTING_WM_CHECK`) that does **not** force-fullscreen — the app opened at its natural
1715x972, not the display's full 3440x1300, which by itself already falsifies the nested-sway
harness's specific defect for this lane.

Two live, WM-verified, discriminating drives (`Scripts/linux-drive.sh`, one invocation each,
`xdotool click --repeat 2 --delay 100 1` for the double-click — no per-click `swaymsg` round trip
exists on this lane, so the earlier 400ms-apart problem doesn't apply here):

1. `gsettings set … action-double-click-titlebar toggle-maximize`, double-click an empty stretch of
   the titlebar (`700,16`, past the traffic lights/cluster buttons): window geometry went from
   `1715x972` to `3440x1300` and `_NET_WM_STATE` gained `_NET_WM_STATE_MAXIMIZED_HORZ` +
   `_NET_WM_STATE_MAXIMIZED_VERT`.
2. `gsettings set … action-double-click-titlebar minimize`, same gesture: `WM_STATE` flipped from
   `Normal` to `Iconic` and `_NET_WM_STATE` gained `_NET_WM_STATE_HIDDEN` — the ICCCM/EWMH-standard
   signal any real pager or taskbar uses to know a window is minimized, strictly more authoritative
   than a pixel diff (which, correctly, showed no change here: `import -window` reads the window's own
   backing pixmap directly, bypassing compositor-level visibility, so a pixel diff was never going to
   distinguish minimized from not — `WM_STATE`/`_NET_WM_STATE` is the right instrument, not a stronger
   version of the wrong one).

Same gesture, two different system preference values, two different genuine WM-level outcomes —
directly refuting "unobservable" for this lane. Restored the gsetting to its original
(`toggle-maximize`) value afterward. No code change: the row was already correct, it just needed the
right lane and the right property to read.

**howToExercise**: `gsettings set org.gnome.desktop.wm.preferences action-double-click-titlebar
minimize` (or `toggle-maximize`), then `Scripts/linux-drive.sh out.png '...'` with a double-click on
an empty titlebar stretch (window-relative coordinates around `700,16` at this window size) via
`xdotool click --repeat 2 --delay 100 1` inside the driven `bash -c` block. Read the result with
`xprop -id $WID WM_STATE` / `_NET_WM_STATE` (minimize) or `xwininfo -id $WID` geometry (maximize) —
not a screenshot, which cannot distinguish iconified from mapped-but-obscured on this lane.

## `F-CHAT-33` — MCP warning banner — **root cause sharpened past H6's; still not closeable through the default agent; new regression test lands**

H6's diagnosis: wiring was fixed (`discover_mcp_servers`), but three hand-drives against the real ACP
agent (`npx @agentclientprotocol/claude-agent-acp@latest`) with a broken `.mcp.json` produced zero
stderr — "the agent is architecturally inert to this field."

Ran the CLI by hand outside the app again, as instructed, and went one step further than "zero
stderr" — traced *why*, using the same broken `.mcp.json` fed through the real JSON-RPC wire
(`session/new` with a `mcpServers` entry pointing at a nonexistent binary):

1. **Trust gate, not silence.** With the scratch project unapproved (the state every fresh Tiller
   project is actually in — nothing in Tiller's launch path ever runs the interactive `claude`
   approval flow), asking the agent to inspect its own MCP config got back `broken-server:
   /definitely/missing/mcp-nonexistent-binary - ⏸ Pending approval (run `claude` to approve)`. No
   connection is even attempted. This lives in the user's own `~/.claude.json`
   (`projects[cwd].enabledMcpjsonServers`), entirely outside this crate's reach.
2. **Wrong channel, confirmed by forcing the gate open.** Manually pre-approving the same scratch
   project (editing `~/.claude.json` directly — not something an automated test can do to a real
   user's global config) and re-driving the wire: the agent DID attempt a real connection and DID get
   a real failure — `Failed to connect — ENOENT: ENOENT: no such file or directory, posix_spawn
   '/definitely/missing/mcp-nonexistent-binary'`. But it arrived as ordinary `session/update`
   conversational content (the agent's answer to an explicit "list your MCP servers and report
   errors" prompt, after it chose on its own to run `claude mcp list 2>&1` as a Bash tool call) — never
   once on the child process's own stderr, the one channel `drain_stderr`/`looks_like_mcp_warning`
   reads. Nothing was pushed spontaneously either: `session/new`'s response and the immediately
   following commands-list push carry no MCP status before anyone asks.

So `looks_like_mcp_warning`'s vocabulary was never the blocker (H6 already showed this); the deeper
finding is that its whole premise — "a CLI adapter's own stderr text is the only channel this has ever
been observed to use" — is false for this agent. The failure *is* observable, just not on stderr, and
only on request, never spontaneously. A real fix would mean scanning arbitrary agent-authored
conversational/tool-call text for MCP-shaped phrases instead of process stderr — a materially
different and much more false-positive-prone mechanism (ordinary chat about MCP servers would trip
it), and it still wouldn't deliver the contract's implied *spontaneous* banner, since this agent never
volunteers the information unasked.

Added `real_agent_stays_silent_on_stderr_for_an_unapproved_broken_mcp_json` to
`tiller_acp/src/lib.rs` (ignored, network-required, same convention as the existing
`real_agent_completes_startup_before_deadline`) — locks in the one half reproducible from inside the
repo without mutating a real user's global config: an unapproved broken `.mcp.json`, the state every
fresh Tiller project is actually in, produces zero bytes on the agent's stderr. Run live:
`cargo test -p tiller_acp -- --ignored real_agent_stays_silent_on_stderr_for_an_unapproved_broken_mcp_json`
→ passed in 11.7s against the real network agent. Full crate suite: 28 passed, 2 ignored, 0 failed
(`cargo test -p tiller_acp`).

Commit: `ee6c1202`.

**howToExercise**: `cargo test -p tiller_acp -- --ignored real_agent_stays_silent_on_stderr_for_an_unapproved_broken_mcp_json`
(needs network + npx). To see the deeper finding directly: create a scratch git repo with
`.mcp.json` naming a nonexistent-binary stdio server, drive `npx -y
@agentclientprotocol/claude-agent-acp@latest` by hand over stdio JSON-RPC
(`initialize` → `session/new` with the matching `mcpServers` entry → a `session/prompt` asking it to
list its MCP servers), and compare the process's own stderr (empty) against the assistant's reply
(which, once the project is manually approved in `~/.claude.json`, shows the real ENOENT).

## `F-CHAT-05` — offline composer — **live-confirmed on the merits; reword proposed for a human, not implemented literally**

Two prior passes already converged: the dangerous half (draft loss) is disproven
(`offline_enter_never_discards_the_typed_draft`, plus all four `self.composer =` write sites checked
not to run on this path); what remains is a wording mismatch between the contract's "confirm the
editor is disabled" and the code's deliberate choice to keep it enabled with its own placeholder.

Drove it live once more, first-hand, as instructed, before deciding: fresh scratch project,
`TILLER_ACP_PROGRAM` pointed at a nonexistent binary (a genuine offline state, not a mock), one
`wayland-drive.sh` invocation. Confirmed all three claims by eye: the composer shows its own
placeholder ("Agent offline — reconnecting when you send…") while a red `could not launch ACP agent`
banner sits above the transcript; typing lands real text in a visibly enabled field; and pressing
Return neither sends nor clears the draft — it retries the connection (producing a second identical
red banner) while the typed text stays exactly where it was.

**Decision**: did not implement the literal contract. Disabling the editor would mean removing a
working, deliberate retry-and-resend feature to satisfy VERIFY wording that predates this UX, and the
one thing that tradeoff must not cost — the user's draft — is now proven both by a deterministic test
and by this live drive not to be lost. Wrote up the case for a reworded clause instead:
`docs/linux-rewrite/tasks/P132-f-chat-05-offline-composer-reword.md` — proposes "confirm a failed
offline Send never discards what the user typed" for the offline half, and flags that the
permission-wait half of the same row has never actually been live-reached by any pass (a separate,
still-open gap, not resolved by this task). No `rust/` change — the code was already correct.

Commit: `e5f3fc1d`.

**howToExercise**: `TILLER_ACP_PROGRAM=/definitely/missing/binary Scripts/wayland-drive.sh out '
ctl project.add path=<scratch-git-repo>
click <worktree-row>
ctl tab.select index=1
click <composer>
type "draft text"
key Return
shot after.png
'` — the placeholder, the enabled field, and the surviving draft are all visible in one capture.

## Summary

All three rows reached a terminal state this pass: `F-WIN-09` closed with better evidence (X11 lane,
`WM_STATE`/`_NET_WM_STATE`, no code change needed); `F-CHAT-33` gained a materially deeper diagnosis
(trust gate + wrong channel, not vocabulary) with a new regression test locking in the reproducible
half; `F-CHAT-05` got a first-hand live reconfirmation and a written reword proposal for a human,
rather than a third pass re-deriving the same finding.
