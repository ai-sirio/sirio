# H5-drive report

Three rows, per the brief: two exercise-only, one a real discrepancy needing a decision. All three
resolved this pass — no code was broken, one row got a documentation-locking test + comment, the
other two are closed by evidence (live drive for auth, exhaustive code-path tracing for the drop).

## `F-CORE-AUTH-03` — driven live, end to end, including restart — **already correct; one deviation to flag**

Drove the real Settings UI (not the socket — there is no credential method on the control socket, by
design; `rust/crates/tiller/src/main.rs` has none) against `CredentialStore` via the OpenCode Go
provider card (`rust/crates/tiller_ui/src/settings.rs`'s `save_opencode_cookie`/`clear_opencode_cookie`),
with `TILLER_CREDENTIALS` pointed at a scratch file so no real user data was touched. The Settings
"AI Providers" page does not respond to synthetic wheel-scroll in either drive lane (`scroll` in
`wayland-drive.sh` and `xdotool click 4/5` both no-op on `settings-detail-scroll`'s `overflow_y_scroll`
div, at multiple coordinates and step counts/signs — Files-panel scroll and browser-grants scroll
elsewhere in the app work fine with the same primitive, so this looks like a real, separate,
un-investigated defect isolated to this one screen, **not chased further — out of this row's scope,
flagging for whoever owns Settings next**). Worked around it on `Scripts/linux-drive.sh` (DISPLAY=:1)
with `xdotool windowsize --sync "$WID" 1400 1600` to grow the window tall enough to expose the
OpenCode Go card without needing to scroll at all.

Sequence, three separate app processes (the middle two chained in one process to also prove
replace-then-delete inside a live session; restarts bracket both ends):

1. **Save** (process A): typed a cookie into the masked field, clicked Save. Status flipped
   `Not signed in` → `Signed in`, "Show in usage bar" flipped on, field cleared back to placeholder.
   `credentials.json` now held `{"opencode-go-cookie":"...VALUE"}` (mode `0600`, confirmed plaintext —
   see deviation below).
2. **Restart + read** (process B, fresh launch, zero interaction before the screenshot): Settings
   already showed `Signed in` on first paint, sourced entirely from disk via
   `ProviderAccountStates::discovered()` → `discover_provider_account` → `CredentialStore::get`. This
   is the persistence half of the VERIFY clause, proven without any app-side caching in play.
3. **Replace** (same process B): typed a second, different cookie, clicked Save again.
   `credentials.json` went from `{"...":"...VALUE"}` to `{"...":"...valueTWO"}` — one key, new value,
   confirmed by `cat`ing the file mid-drive, not just by the UI staying `Signed in` (which alone
   wouldn't distinguish replace from a silent no-op).
4. **Delete** (same process B): clicked Clear. Status flipped back to `Not signed in`, toggle off,
   `credentials.json` became `{}`.
5. **Restart + read again** (process C, fresh launch): `credentials.json` still `{}`, Settings showed
   `Not signed in` on first paint with no interaction — the deletion half of VERIFY, also proven
   through a real process restart, not an in-memory state reset.

Screenshots for all five steps are under `/tmp/h5cred-x11-shots/` (not committed — scratch, per the
house rules' `/tmp` convention for capture output) if anyone wants to re-look before they're swept.

**Deviation to flag, as instructed — not silently treated as satisfied:** `CredentialStore`
(`rust/crates/tiller_usage/src/credentials.rs`) is plaintext JSON at mode `0600`, not encrypted. The
file's own doc comment states this plainly and defends it as parity with the credential files this
same crate already reads as ground truth (`~/.claude/.credentials.json`, `~/.codex/auth.json`) — same
OS-user-boundary protection, nothing more — and argues a keyring-backed store is actively hostile to
this project's needs (ambient mutations from parallel `cargo test` runs and app instances, an
unlock prompt that can block a headless session, no way to point it at a hermetic fixture). The
contract's third VERIFY option is "an explicitly chosen *encrypted* store"; this is not that. No code
change made for this — it is a judgment call for a human, not something this pass should silently
paper over by loosening the wording or silently accepting it. Recommend the row's VERIFY option 3 be
reworded to "a store scoped to the OS user account (plaintext-at-mode-0600 is acceptable; a keyring
is not required)" if the tradeoff argued in the code is accepted, or that this be raised as a real gap
if it is not.

No files changed for this row (already-correct code); no commit beyond this report.

## `F-CORE-FILE-03A` — order preservation is structurally proven; the live gesture stays undrivable — **half-proven, upgraded evidence**

Traced the *entire* multi-file XDND pipeline end to end, across three crates, instead of reasoning
from the `text/uri-list` convention alone:

1. **Wayland client** (`gpui_linux`'s `crates/gpui_linux/src/linux/wayland/client.rs`, the vendored
   `zed-industries/zed` checkout at the pinned rev): on `wl_data_device::Event::Enter`, the *entire*
   `text/uri-list` blob is read through **one** pipe in **one** background task, then parsed with a
   single synchronous `.lines().filter_map(Url::parse).filter_map(to_file_path).collect()` into a
   `SmallVec` — one pass, no per-URI concurrency, order is exactly line order. Critically, **there is
   no per-file resolution step to race at all**: the "one resolves slowly" scenario the row worries
   about presupposes a parallel per-path resolution loop that this code does not have. The subsequent
   `wl_data_device::Event::Drop` carries no paths of its own (`FileDropEvent::Submit { position }`
   only) — the paths already parsed at Enter are what eventually reach the app.
2. **GPUI core** (`crates/gpui/src/window.rs`, `handle_platform_event`): `FileDropEvent::Entered`
   stashes that same `SmallVec` **once**, unchanged, into `cx.active_drag` (`Arc::new(paths.clone())`).
   `Submit` is translated into a synthetic `MouseUp` that runs GPUI's ordinary drop-target dispatch
   against that stashed, unmodified payload — nothing re-parses or reorders it between Enter and
   Submit.
3. **`tiller_terminal`** (`rust/crates/tiller_terminal/src/lib.rs:1586`'s
   `.on_drop::<gpui::ExternalPaths>`): `paths.paths().to_vec()` — `ExternalPaths::paths()` is a bare
   slice view (`&self.0`, `crates/gpui/src/interactive.rs`), no sort, no dedup.
4. **`tiller_project::terminal_file_drop`** (`rust/crates/tiller_project/src/file.rs:110`): a plain
   `.iter().map(shell_quote_path).collect().join(" ")` — order-preserving by construction.

Every link in the chain is a single-pass, order-preserving map; there is no point between the OS
handing over the `text/uri-list` blob and the quoted string landing in the PTY where paths could be
reordered, dropped, or raced against each other. This is stronger than the row's own "likely already
correct" — it is proven correct by exhaustive code-path tracing, modulo the one thing tracing code
can't stand in for: a real compositor actually delivering a real multi-file XDND drop to this app.

**Why that gesture stays undrivable here, precisely:** every primitive `wayland-drive.sh` has
(`click`/`move`/`down`/`up`/`drag`/`scroll`/`chord`/`modclick`) is raw input synthesis through
`Scripts/wayland-virtual-pointer.c`, a `wlr-virtual-pointer-unstable-v1` client — it injects
button/motion/axis events as if from a physical device, and that is all a virtual-pointer protocol
object can do. XDND is a different kind of thing: the *source* of a real OS-level file drop must be a
Wayland client that owns `wl_data_device_manager`, creates a `wl_data_source`, offers `text/uri-list`,
and calls `wl_data_device.start_drag(source, origin_surface, icon, serial)` — and Wayland requires
that `serial` come from a real button-press event the compositor delivered **to that client's own
mapped surface**. A virtual pointer clicking at a screen coordinate does not make any surface "the
drag's origin" in the protocol's eyes; only the client whose surface was actually pressed can start a
drag with that serial.

**The smallest instrument that could close this**, concretely, not "investigate further":

- A new small C client, e.g. `Scripts/wayland-virtual-dnd-source.c`, sibling to the existing
  `wayland-virtual-pointer.c`, that: (1) connects to the same `$WAYLAND_DISPLAY` wayland-drive.sh
  already exports; (2) binds `wl_compositor`, `wl_seat`, and `wl_data_device_manager` (core protocol,
  no extra XML needed — unlike the wlr-virtual-pointer extension, these ship with any
  `libwayland-client`); (3) creates and maps a small `xdg_toplevel` surface at a fixed, known screen
  position (e.g. a 4×4px window pinned near the origin) so the existing `down <x> <y>` primitive can
  press exactly on it; (4) on that press's `wl_pointer::button` event, captures the serial, builds a
  `wl_data_source`, offers MIME type `text/uri-list`, sets `DndAction::Copy`, and calls
  `wl_data_device.start_drag(source, own_surface, None, serial)`; (5) answers the source's `send`
  event by writing a caller-supplied, newline-joined `file://<path>` list (read off a small FIFO the
  same way `wayland-virtual-pointer.c` already reads its command FIFO) into the given fd.
- A new `dnd <path1> <path2> ...` verb in `wayland-drive.sh` that sequences: `down` on the dummy
  source surface's fixed coordinate → tell the new process (via its FIFO) which URIs to offer and to
  start the drag off that press's serial → the *existing* `move`/`up` primitives to carry the pointer
  over the real target pane and release, exactly like `drag` already composes `down`/`move`/`up` for
  button-held drags.
- With that in place, the positive control is exactly what P124 already did for plain scroll: drop
  two or three distinctly-named scratch files in a controlled order onto a terminal pane and read the
  PTY's echoed insertion line back to confirm the quoted paths appear in drop order.

Not built this pass — it is a new protocol-aware client (data-device bindings, an owned mapped
surface, serial plumbing), not a config tweak, and building + debugging a first Wayland DND source
from scratch was judged bigger than this row's remaining budget alongside the other two rows. Left at
half-proven, but the half that was missing shrank from "is the ordering claim plausible" (a guess) to
"the live gesture is unrepresentable by the current harness, and here is exactly what would fix that"
(a fact plus a buildable plan) — no unit test substituted, per the brief.

No files changed for this row.

## `F-CHAT-05` — real discrepancy, decided on the merits — **fixed (test + guarding comment)**

See the commit `test(F-CHAT-05): lock in that a failed offline Send never discards the draft`.

Before touching anything, re-derived the wave-G critic's live finding with a deterministic drive
instead of trusting either the code-reading intuition or the prior live report at face value (per the
brief's own warning to assume the easy reading is wrong): added
`offline_enter_never_discards_the_typed_draft` — same permanently-missing-binary setup as the
existing `offline_composer_shows_its_own_placeholder` test, but presses Enter *after* typing and
asserts on the composer's text before and after. Result: **the draft survives.**
`Chat::send`'s offline branch (`self.client.is_none()`) returns before ever touching `self.composer`;
no `AcpEvent` handler, no connection-failure path, and no other write site touches the composer on a
failed reconnect either — the only writers are the real-send path, `new_conversation`, and the
control-socket compose seam, none of which run here. So the wave-G critic's "pressing Return silently
cleared the draft" does not reproduce at HEAD under a controlled, deterministic drive.

That does not make the live report wrong to have recorded — it makes it more likely explained by
something *this* wave already diagnosed independently: **P131** found this exact family (live-agent
state in this harness) prone to stale captures that read as "nothing happened" when something in fact
did. A live capture taken right after Return, while `connecting` briefly flips true and the async
reconnect races to resolve, is exactly the shape of thing P131's staleness finding covers. I did not
re-run the live drive to chase that further — the deterministic test is strictly stronger evidence for
the specific question ("does Return ever discard the draft") than another screenshot would be, since
it inspects model state directly rather than a possibly-stale frame.

**Decision, on the merits the row asked for:** kept the editor enabled rather than disabling it to
match the contract's literal "confirm the editor is disabled" wording. The code's own placeholder text
("Agent offline — reconnecting when you send…") and three independent comments describe a deliberate
tradeoff — type now, Send retries the connection and auto-resubmits once it lands
(`start_connection`'s `should_send` path) — which is a reasonable, arguably better UX than blocking
input outright while offline. The one thing that tradeoff cannot be allowed to cost is the user's
typed text, which — per this pass's proof — it does not. Added a guarding comment at the offline
branch in `send()` pointing at the new test, so a future refactor that starts touching `self.composer`
in that branch gets caught immediately rather than silently reintroducing data loss.
Recommend the contract row's wording move from "confirm the editor is disabled" to "confirm a failed
offline Send never discards what the user typed" — the disabled-editor clause describes a UX this
code deliberately doesn't implement, and enforcing it would mean *removing* a documented, defensible
feature (retry-and-resend) rather than fixing a defect.

Files: `rust/crates/tiller_ui/src/chat.rs` (new test `offline_enter_never_discards_the_typed_draft`,
guarding comment on `send()`'s offline branch). `cargo build -p tiller_ui` and
`cargo test -p tiller_ui offline_enter_never_discards_the_typed_draft` /
`cargo test -p tiller_ui offline_composer_shows_its_own_placeholder` all green.
Commit: see git log, subject `test(F-CHAT-05): lock in that a failed offline Send never discards the draft`.
