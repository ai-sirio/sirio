# P112 — can the Wayland lane be given synthetic input?

**Owner: `pi`.** Worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch
`linux/gpui-waku`. This is an investigation with a real chance of a negative result, and a negative
result written down is a complete deliverable.

## Why this is worth doing

Every gesture in this project queues behind **one** `DISPLAY=:1` drive lock, held by one agent at a
time. `sonnet` holds it now for `P109`'s 34 rows. Meanwhile the Wayland lane
(`Scripts/wayland-drive.sh`) takes no lock, runs in parallel, gives a real toplevel and true `grim`
pixels — and cannot click or type, so it can only ever settle rows reachable over the control
socket. That asymmetry is the project's throughput ceiling. If it falls, the drive backlog
parallelises.

## What is actually known, and what was assumed

Recorded in `WAYLAND-LANE.md`: the seat reports `capabilities: 0, devices: []`, and `wtype`,
`wlrctl` and `swaymsg seat` all **exit 0 and change nothing.**

That is a precise observation with an imprecise conclusion attached. `wtype` exiting 0 means it
bound the protocol and sent events — the failure is downstream of sending. So "the lane has no
synthetic input" may be describing a routing problem as if it were a capability problem, and nobody
has taken it apart.

Three facts that were not available when that note was written:

- `wtype` and `wlrctl` are both installed (`/usr/bin/`).
- sway 1.9 **does** implement `virtual-keyboard-unstable-v1` and `wlr-virtual-pointer-unstable-v1`.
  They are wlroots protocols compiled into the binary — their absence from
  `/usr/share/wayland-protocols/unstable/` is expected and is not evidence.
- `WLR_LIBINPUT_NO_DEVICES=1`, which the lane sets, is a **libinput-backend** variable. Under
  `WLR_BACKENDS=headless` it governs nothing.

## The hypothesis I would test first

**GPUI never creates a `wl_keyboard` at all.** A Wayland client binds `wl_seat` and reacts to the
`capabilities` event; with `capabilities: 0` there is no keyboard and no pointer, so a well-behaved
toolkit never instantiates them. Injected events then have no object to be delivered to — which
looks exactly like "wtype does nothing", at any injection rate, forever.

If that is the mechanism, the interesting consequence is **ordering**: create the virtual keyboard
and pointer *before* the app connects, so the seat already advertises the capability when GPUI binds
it. That is a cheap experiment and it discriminates sharply — if a keyboard created first works and
one created after does not, you have both the cause and the fix in one run.

Test it, don't take it. I may be wrong about which layer drops the event, and the point of the
exercise is to find out where it actually dies, not to confirm me.

Useful discriminators along the way:
- `swaymsg -t get_tree` — does the Tiller toplevel ever hold `"focused": true`? Sway may decline to
  focus anything when the seat has no keyboard.
- `WAYLAND_DEBUG=1` on the app, grepped for `wl_seat` / `wl_keyboard` / `wl_pointer` — this shows
  directly whether the objects are ever created, which settles the hypothesis outright.
- `wlrctl pointer move` / `click` exercises the *pointer* path independently of the keyboard path.
  They can fail for different reasons; test them separately.

## One safety rule, and it is not optional

`WAYLAND_DISPLAY` in this login session points at **the operator's real desktop.** That has already
burned this project once — `grim` silently photographed the wrong compositor until
`wayland-drive.sh` started exporting the nested display. Synthetic input is worse than a stray
screenshot: typing into the real session hits whatever the human left focused.

**Export `WAYLAND_DISPLAY` to the nested instance in every shell before you run any injection tool**,
and verify with `swaymsg -t get_version` that you are talking to *your* sway, not the desktop's.
Never run `wtype` or `wlrctl` with an inherited `WAYLAND_DISPLAY`.

## What to produce

Whichever way it goes, the deliverable is `docs/linux-rewrite/P112-report.md` plus an edit to
`WAYLAND-LANE.md`'s capability table.

**If it works:** a helper in `Scripts/wayland-drive.sh` in the same shape as the existing `ctl` and
`shot` — proven by driving a gesture that the socket cannot reach and photographing the result. One
real gesture end-to-end is the proof; a `wtype` that returns success is not. Say which key and
pointer gestures work and which don't (modifiers, chords like `Shift+Tab`, drag) rather than
declaring input "working" — `herdr` itself has gestures that report success and do nothing, and that
class of false positive is exactly what this project keeps paying for.

**If it doesn't:** replace the current note in `WAYLAND-LANE.md` with what you established — which
layer drops the event, the command that showed it, and what would have to change. That converts a
standing assumption into a settled fact, and it is worth as much as the helper.

## The rules

- **Do not edit `INVENTORY-LEDGER.md`.** Do not touch `rust/` — if the fix appears to need an app-side
  change, that is a finding and a separate task, not this one.
- `chat.rs`/`composer.rs` are live under `codex11`; `sidebar.rs` and the terminal/changes surfaces
  under `codex12`; `settings.rs`/`tiller_agents` under `fable`. You should need none of them.
- Commit path-scoped, never `git add -A`. `grep '??'` before calling it done.
- Do not idle on an approval gate — `ENVIRONMENT.md` §"Working with the orchestrator".
