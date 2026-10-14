# The update card — offer an update, restart into it

**Date:** 2026-10-14
**Status:** proposed
**Supersedes:** §5.2 "Only Windows restarts the app" and §7.1 "Not a modal" of
`2026-08-29-auto-update-design.md` (commit `d2189a61`), in the narrow ways §1
names. Everything else in that spec stands: the signed manifest, the automatic
download on Stable, the bounded retries, the opt-out, the status-bar indicator.

## §0 Intent

Today an update reaches the user only through a quiet indicator in the status
bar, which opens Settings → General, whose button applies it. On macOS and
Linux nothing restarts afterwards: a toast says "applied — relaunch to use it"
and the user has to quit and reopen Sirio by hand.

The user wants a dialog that **appears when an update is found, at launch or
during a session**, and offers two things: **put it off**, or **restart into
it** — download it if needed, install it, relaunch Sirio.

Decided with the user, in order:

1. **"Later" means this version, this session.** The card does not come back
   for that version until the next launch; the indicator and Settings stay.
   Nothing is persisted.
2. **Running agents are warned, not protected.** Restarting goes through Quit,
   and Quit ends every PTY (`PaneRegistry::shutdown`) until SP2 (#627) turns
   it into a detach. The card counts the agents at work and says they will
   stop; it then offers a third action, *Install without restarting*.
3. **It never takes the keyboard.** A check can finish while the user types
   into a terminal or a composer. The card appears without taking focus, with
   no scrim, and no action answers Enter.
4. **One non-modal card**, not an Ely `Dialog`. Ely's `Dialog` always takes
   focus (`take_focus` in `overlays/dialog.rs`) behind a scrim; a scrim over a
   terminal that still receives the keystrokes is worse than either. A card
   built from Ely parts, docked bottom-right above the status bar indicator,
   serves launch and mid-session alike.

**Success:** at launch or mid-session, a user who has an update in front of
them can restart into it in one click, on every platform, and knows beforehand
whether that stops an agent; a user who says *Later* is not asked again until
the next launch, and loses nothing.

## §1 What changes against the auto-update spec

- **§7.1 "Not a modal."** The card is still not modal — no scrim, no focus,
  the window stays usable — but it is no longer only the indicator: an update
  now *announces itself*. The reason §7.1 gave ("never more important than the
  turn an agent is in") is kept by decisions 2 and 3 above.
- **§5.2 "Only Windows restarts the app."** macOS and Linux now restart **when
  the user asks for it** (*Restart to update*). The reason §5.2 gave — never
  kill live sessions for an update the user did not ask to apply *now* — is
  kept: the restart is always an explicit click, *Install without restarting*
  keeps today's behaviour, and the card says beforehand what will stop.
- **§7.2 the gesture.** The indicator now opens the card instead of Settings.
  Settings → General keeps its button and its per-platform label.

## §2 What the user sees

Labels are English, like the rest of the interface.

### §2.1 When the card appears

Shown when **all** hold:

- updates are enabled (`UpdateState.enabled`) and the compiled channel is not
  Dev (a Dev build reports `Disabled` and never gets here);
- the status is `Available`, `Ready`, `Downloading`, `Installing` or
  `Installed`, or `ApplyFailed` (an apply the card or Settings started). A
  failed background check (`Failed`) never raises the card: it stays Settings'
  "Could not check for updates", as §7.3 of the auto-update spec has it;
- the version is not the one put off with *Later* this session;
- the user has not hidden it with × while work is in flight (§2.3).

At launch it appears when the first check's result lands; mid-session when a
check brings a version other than the one put off. A click on the status bar's
"Update available" indicator shows it again and clears *Later*. The indicator
also stays visible for `Installed`.

### §2.2 Content

- **Title:** `Sirio 0.32.0 is ready` (`Ready` — downloaded and verified) or
  `Sirio 0.32.0 is available` (`Available` — to be downloaded: Nightly, or a
  Stable download skipped on a metered connection).
- **Body:** the release notes on Stable, scrollable past a fixed height;
  `Nightly builds track main` on Nightly, as Settings already says.
- **Warning**, only while at least one agent is at work:
  `2 agents are working — restarting will stop them.` "At work" is
  `AgentStatus::Running` in `AgentActivityModel` (terminals and chats alike,
  `running_agent_ids`), recounted on every render.

### §2.3 Actions

| | macOS / Linux | Windows |
|---|---|---|
| always | **Later** · **Restart to update** (primary) | **Later** · **Update and restart** (primary) |
| an agent at work | also **Install without restarting** | no third action — Inno restarts regardless |

From the click until the outcome, every action is disabled and the card shows
`Downloading Sirio 0.32.0…` (only when nothing is staged yet) then
`Installing…`, over an indeterminate bar: the download reports no progress. The
× hides the card while the work goes on; the indicator brings it back.

### §2.4 Outcomes

- **Restart to update / Update and restart, succeeded:** Sirio saves the
  session and relaunches into the new version (§3.4).
- **Install without restarting, succeeded:** the card becomes
  `Sirio 0.32.0 is installed — restart to use it`, with **Restart now** and
  **Later**. This replaces today's toast "applied — relaunch to use it", and is
  where the Settings button's success lands too.
- **Failed:** `Update failed: <reason>`, with **Retry** and **Later** when the
  failure is retryable (§4), with **Later** alone and the reason in full
  otherwise.

## §3 Architecture

### §3.1 `sirio_ui::update_card` (new)

- `update_card_view(input) -> Option<UpdateCardView>` — **pure**. Input: the
  `UpdateState`, the version put off, whether the card is hidden, the count of
  agents at work, and whether the platform's installer restarts the app
  (`cfg!(windows)`, passed in so every case runs on every OS). Output: `None`
  or the title, body, warning, progress line and an ordered list of actions
  with their enabled flag. Every decision in §2 lives here.
- `UpdateCard` — a `RenderOnce` that draws an `UpdateCardView` with Ely's
  `Button` and the palette's tokens, docked bottom-right above the status bar.
  It holds no state, takes no focus, draws no scrim, and takes five host
  callbacks: `later`, `restart`, `install_only`, `retry`, `hide`. Every element
  carries a `debug_selector` (`update-card`, `update-card-title`,
  `update-card-warning`, `update-card-action-<name>`, …). Text is
  `selectable_text`, except inside buttons.

### §3.2 `sirio_ui::UpdateStatus` (existing, extended)

```rust
pub enum UpdateStatus {
    Disabled, NotDue, Checking, UpToDate,
    Available { version: String, notes: String },
    Ready { version: String, notes: String },
    Failed { message: String },               // a check failed, as today
    Downloading { version: String },          // new
    Installing { version: String },           // new
    Installed { version: String },            // new
    ApplyFailed { version: String, message: String, retryable: bool }, // new
}
```

Today an apply in flight shows `Checking`, which is false, and a failed apply
shows `Failed`, which Settings words as "Could not check for updates". A
separate `ApplyFailed` keeps the two apart without the card needing to know
where a failure came from. A new variant of a
public enum breaks `match`es in other crates' test targets, so the change is
verified with `cargo build --workspace --all-targets`. Settings → General draws
the four new states as a muted line (`Update failed: …` for `ApplyFailed`)
and keeps its button hidden during `Downloading` / `Installing`.

### §3.3 `sirio_apply::relaunch_target() -> Option<PathBuf>` (new)

The path to relaunch after a successful apply, from the same self-location
`apply` uses: the `.app` bundle on macOS (`bundle_of(current_exe)`),
`$APPIMAGE` on Linux, `None` on Windows (Inno relaunches) and wherever
self-location refuses. The path relaunched is therefore by construction the one
just swapped.

### §3.4 The host (`sirio/src/main.rs`)

- `begin_update_apply(after: AfterApply)`, `AfterApply::{Restart, Stay}`, is
  the single door: the card's actions, its *Retry* and the Settings button
  (`Stay`) all go through it. It pushes `Downloading` (only for
  `ApplyPlan::DownloadThenApply`) then `Installing` through the existing
  `push_update_state`, and refuses to start while a previous apply is in
  flight.
- New workspace fields: `update_card_dismissed: Option<String>` (the version
  put off; in memory, so a relaunch clears it), `update_card_hidden: bool`
  (the ×, cleared when the work ends), `update_after_apply: AfterApply`.
- On `Apply` success:
  - `Stay` → `Installed`.
  - `Restart` on macOS / Linux → `Installed` first (the fallback, see §4),
    then `cx.set_restart_path(target)` and `cx.restart()`. gpui's restart
    spawns a script that waits for this pid to exit, then quits through the
    platform, so `on_app_quit` runs as on any Quit: the layout is flushed, LSP
    servers stopped, `PaneRegistry::shutdown` ends the PTYs.
  - `Restart` on Windows → before spawning the installer, the session is saved
    (`schedule_save` + `SessionStore::flush_now`), because Inno closes the app
    from outside and no quit hook can be relied on.
- The status bar indicator's callback opens the card (clears
  `update_card_dismissed` and `update_card_hidden`) instead of Settings.
- The card is drawn in the main shell's render branch only, never over
  Settings.

### §3.5 Removed

The legacy update toast, which no real transport ever fed:
`sirio_project::ui::{UpdateState, UpdateEvent}`, `render_update_toast` and
`dismiss_update_toast`, `ControlAction::UpdateEvent`, the `update.event` socket
method (served in every build; nothing in `Scripts/`, `docs/` or
`sirio_control` calls it) and the two tests that drove it.

### §3.6 Driving it over the socket — debug builds only

`surface update`, compiled in with `cfg!(debug_assertions)` like
`surface changes confirm`; a release build answers "unknown method" and does
not list it.

- `state` — the card's `UpdateCardView` (or none) and the host's status.
- `act later|restart|install|retry|hide|open` — presses that action through
  the same callbacks a click uses (`open` is the indicator).
- `inject <status> [version] [message] [retryable]` — sets the host's status
  for states the network cannot reach on demand.

## §4 Failure and edge cases

**Download or verification failed** (network, signature, staging) →
`ApplyFailed { retryable: true }`. `Updater::download` today refuses a second
attempt on the same version within the interval (`SkippedAlreadyAttempted`),
and the host ignores that answer — so a second click after a failure leaves the
UI on `Checking` forever. That guard exists for **automatic** retries (§7.2 of
the auto-update spec: "a battery bug"). A download the user asked for bypasses
it (`Updater::download_requested`, or a flag on `download`); the poll loop's
automatic download keeps it. A `Skipped*` answer the host does receive now
lands as `ApplyFailed` instead of nothing.

**Apply failed:**

| `ApplyError` | retryable | card |
|---|---|---|
| `InstallNotFound` (a `cargo run`, a copied binary) | no | the message, which names `DOWNLOAD_PAGE_URL` |
| `Launch`, `Swap` (previous version intact) | yes | the message |
| `Rollback` (macOS: the moved-aside copy is the working one) | no | the message in full, selectable — it holds the path to restore by hand |
| `UnsupportedPlatform` | no | the message |

**The restart does not happen.** gpui's `restart` logs and returns without
quitting when it cannot spawn its script. The status is already `Installed`
when it is called, so the user is left with *installed — restart to use it*
rather than an app that seems to have ignored the click.

**Updates disabled mid-flight:** the outcome is dropped (today's behaviour) and
the card disappears with the state.

**A check lands during an apply:** a newer `Available` / `Ready` does not
replace `Downloading` / `Installing`; it is kept as `last_available` /
`last_verified` and shown when the apply ends.

**A check lands after an install without restart:** the running binary is still
the old version, so the next check reports the installed version as `Available`
/ `Ready` again. A check for the version already `Installed` keeps `Installed`;
only a newer version replaces it. A check that fails or is not due keeps
`ApplyFailed` too, so the card's error does not vanish under a poll.

**Double click:** the actions are disabled from the first click to the outcome,
and `begin_update_apply` refuses a second start regardless.

**Manifest text:** notes are drawn as plain text, never Markdown; control
characters are stripped and the shown body is capped (4 KiB) — the manifest is
signed, but the card should not be the place a rendering bug lives.

## §5 Testing

### §5.1 Unit — written before the code

`update_card_view` is the one unit with decisions in it, so its failure modes
are listed first and each becomes a test before the function exists:

- shows with updates disabled, or for `Disabled` / `UpToDate` / `NotDue` /
  `Checking`;
- shows again for the version put off; stays hidden for a *newer* version;
- shows while hidden with × during `Downloading` / `Installing`;
- offers *Install without restarting* on Windows, or with no agent at work;
- leaves any action enabled during `Downloading` / `Installing`;
- offers *Retry* on a non-retryable `ApplyFailed`; raises the card for a
  check's `Failed`;
- names the wrong primary label per platform (`Restart to update` vs
  `Update and restart`);
- shows the warning with zero agents, or a singular/plural mismatch.

Nothing else gets a `#[test]`; the rest is proven end to end.

### §5.2 E2E — `Scripts/Tests/test-update-ui-e2e.sh` → `UPDATE UI E2E OK`

Linux, a private Xvfb, an isolated Sirio (`SIRIO_DB`, `SIRIO_SOCKET`,
`SIRIO_HOST_HOME`), `--out-dir DIR` keeps the socket transcript, the captures
and the logs; `--appearance light|dark`; `--state-only` skips captures.

**Stage `chain` — the real thing.** Sirio is built the way
`test-update-e2e.sh` builds its probe — `SIRIO_RELEASE_CHANNEL=nightly`, a
loopback `SIRIO_UPDATE_MANIFEST_URL`, `SIRIO_RELEASE_ACCEPTED_KEYS` from a key
generated for the run — into a dedicated `CARGO_TARGET_DIR`, so the ordinary
build is not invalidated. The real `sirio-release` signs a manifest for a
version above the compiled one; its artifact is a stand-in AppImage, a script
that drops a marker then runs the same debug binary. Sirio is started through a
stand-in `$APPIMAGE` that does the same without the marker. Asserted:

1. at launch the card shows `is available`, without the focus leaving the
   terminal;
2. `act restart` goes `Downloading` → `Installing`;
3. the process exits and **a different pid** answers on the socket
   (`system.identify`);
4. the new artifact's marker exists;
5. the session's layout was restored.

**Stage `ui` — every state drawn**, PID-matched window captures, light and
dark: available, ready, with the agent warning (a pane made "running" with a
real `sirioctl notify --status running`), downloading, failed with Retry,
refused with the link, installed. Then: *Later* hides the card for that
version, a re-check of the same version does not bring it back, the indicator
does; text typed with `xdotool` while the card is up reaches the terminal pane.
States the network cannot reach on demand go through `surface update inject`.

### §5.3 Not automated, recorded

The relaunch on macOS (`open` of the swapped bundle) and on Windows (Inno's
close and relaunch, plus the session flush before it) are manual rows in
`docs/testing/update-card.md`, the shape `docs/testing/host-detach-probe.md`
has: platform, build, steps, result, date.

## §6 Out of scope

- Persisting *Later* across launches ("skip this version").
- Deferring the restart until agents are idle.
- Keeping agents alive across the restart — that is SP2's (#627).
- Download progress: `Updater`'s fetch returns the whole body.
