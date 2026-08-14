# Evidence — E11-git+tab+win (F-GIT, F-TAB, F-WIN)

Driven from worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch
`linux/gpui-waku`, HEAD `4073297`, Wayland lane label `drive-E11-git+tab+win`.

## F-GIT-REMOTE-01 (ledger line 489, half-proven)

Missing half per current ledger note: `github_owner` (`tiller_git/src/remote.rs:76`) is claimed
to have **0 app references** — proven only by unit test, not by any live UI path.

Re-checked by grep across the whole tree (`rust/`, all crates including `tiller_ui`/`tiller`):

```
grep -rn "github_owner" rust/ --include=*.rs
```

Hits: `tiller_git/src/lib.rs:62` (re-export), `tiller_git/src/remote.rs:13,15,20,76,77`
(definition + internal call from the instance-method wrapper to
`github_owner_from_url`), and `tiller_git/tests/p41_git_behaviors.rs:186,190,194,212,225`
(unit tests only). **No caller anywhere outside `tiller_git` itself** — not in `tiller_ui`,
not in the `tiller` app crate, not in `TillerControl`. This confirms the ledger note exactly:
the function is dead code from the app's perspective, reachable only through its own crate's
test suite.

This is not a gap the Wayland lane (or any live drive) can close, because there is no UI
control, no control-socket method, and no code path in the running app that calls this
function — it is unreachable by construction, not merely undemonstrated. Ran the specific
unit test as corroboration that the tested behavior itself is not broken:

```
cd rust && cargo test -p tiller_git --test p41_git_behaviors remote_parsing_supports_github_ssh_https_and_project_suffixes
-> test remote_parsing_supports_github_ssh_https_and_project_suffixes ... ok
```

**Claim: could-not-reach.** The missing half (`github_owner` exercised via the app) cannot be
driven from any input path, live or socket — the function has zero callers outside its own
crate's tests. The half already proven (`project_name`, 8 app references incl.
`tiller_ui/src/project_forms.rs:704`) stands as previously recorded; not re-driven here since
the ledger already treats it as proven and this pass targeted only the missing half.

## F-TAB-08 (ledger line 124, half-proven)

Missing half per current ledger note: the "Other agents…" / "No supported agent found on
PATH" fallback that renders when bare-PATH launch finds no ACP-capable agent — clicking it is
claimed to NOT open Agents settings (no click handler).

Source read first (`tiller_ui/src/tab_bar.rs`): `render_chat_agent_item` (the normal per-agent
row) attaches `.on_click(move |_, _, cx| entity.update(cx, |this, cx| this.emit_chat_agent(id,
cx)))`. `render_chat_empty` (the fallback shown when `available_chat_agents.is_empty()`) has an
`.id("new-chat-empty")` and `.debug_selector` but **no `.on_click` at all** — structurally
confirms the missing half before any drive.

Live-drove it on the Wayland lane, `TILLER_WL_LABEL=drive-E11-git+tab+win`, launching the app
with `PATH=/usr/bin:/bin` (strips `~/.local/bin/claude`, `~/.local/bin/codex`,
`~/.nvm/.../bin/opencode`, `~/.nvm/.../bin/pi` — confirmed via `which` before the drive — while
leaving `grim`/`sway`/`swaymsg`/`wtype`/`gcc`/`wayland-scanner` reachable at `/usr/bin`, which
the drive script itself needs). Sequence (captures in
`reference/linux-progress/drive-E11-git+tab+win/f-tab-08f/`):

1. `ctl project.add path=.../tiller-linux` — added/selected the project.
2. `click 1290 50` on the tab-bar `+` → `03-menu-open.png` shows the full add-tab menu (New
   Terminal, Changes, New Browser, per-agent rows, Split Claude Code, `New Chat ›`).
3. `click 1035 348` on the `New Chat` row → `04-picker-open.png` shows the submenu expanded
   with **only** the fallback: `Other agents…` / `No supported agent found on PATH` — no agent
   rows, confirming the bare-PATH condition actually took effect for the chat picker (even
   though the top-level per-agent terminal/split items still list all 5 agents, since those
   don't gate on PATH the same way).
4. `click 1345 400` — squarely inside the fallback row's bounds (row spans roughly
   x=1284–1450, y=372–432 at this capture's 1715×972 resolution; cursor lands at the row's
   trailing edge in the frame) → `05-after-fallback-click.png`: **the menu is unchanged** —
   same items, same "Other agents…" fallback still showing, no Settings surface, no
   navigation, no visual change beyond the pre-existing hover state. Clicking the fallback is a
   no-op.

**Claim: exercised-broken.** Drove the exact gesture (bare-PATH launch → open New Chat →
click the "Other agents…" fallback) and it does nothing — no Agents settings surface opens.
This is the row's previously-unproven half; the first half (fallback renders with correct
wording under bare PATH) was already proven at pass 15 and is corroborated again here by
`04-picker-open.png`.

## F-WIN-07 (ledger line 59, half-proven)

Missing half per current ledger note: explicit `session.restore` returns `restoredCount:0`
(confirmed working half: relaunch auto-restores everything), and "the required History
menu/⇧⌘O is absent" (unproven half, from P106/P108).

Re-checked both at HEAD `4073297`:

1. **Source grep, current HEAD** — `grep -rniE "previous launch|history" rust/crates/tiller/src
   rust/crates/tiller_ui/src --include=*.rs`, filtered for non-test hits: every "history" hit
   is either browser back/forward history (`tiller_ui/src/browser.rs`) or an unrelated
   `Vec<T>` borrow named `history` in a settings-picker test (`project_identity.rs`). **No
   "History" menu, no "previous launch" string, no restore-session UI route anywhere in the
   app or UI crates.** This is the same conclusion P106/P108 reached, reconfirmed against the
   current tree rather than assumed stale.

2. **Live socket drive** (Wayland lane, fresh instance,
   `reference/linux-progress/drive-E11-git+tab+win/f-win-07/`): `ctl system.capabilities`
   lists all 50 control methods this build exposes — no `history.*` method, no
   `session.list`/`session.recent` or similar exists at the socket layer either, so there is
   no non-visual route to a "previous launch" picker to drive around the missing menu.
   `ctl workspace.list` on this already-populated DB shows both worktrees `mounted:"true"`
   (state carried over from earlier drives in this session, itself informal corroboration that
   mount state persists across relaunches). `ctl session.restore` on this live, already-fully-
   mounted state again returned `{"path":".../tiller-linux","restoredCount":"0"}` — identical
   to the P106/P108 finding, now reproduced independently on a different instance/DB.

**Claim: could-not-reach** for the missing half specifically (a History menu / ⇧⌘O binding to
click or press) — there is nothing in the running app, in any menu, or on the control socket
that this half could be driven through; its absence is exhaustively confirmed by source grep
and by the live `system.capabilities` method list, not by a UI gesture that failed. The
already-proven half (`session.restore` → `restoredCount:0` despite working relaunch-time
restore) is reaffirmed live in this pass with fresh capture evidence in `f-win-07/`.
