# Wave A slice W06-set — critic verdicts

Adjudicated independently (did not drive or build this slice). HEAD `4073297`,
worktree `tiller-linux`, branch `linux/gpui-waku`.

## `F-SET-19` — ledger line 307, was **half-proven** → **PASSED**

**Verified independently:**

- Recomputed whole-frame grayscale mean on all three new captures with ImageMagick
  (`convert -colorspace Gray -format "%[fx:mean*255]"`), not trusting the driver's numbers:
  - `02-01-appearance-light-desktop.png` (is_dark=true / dark desktop) → **32.62** (driver
    claimed 32.6). Driver's own filename is mislabeled ("light-desktop") but the driver's
    prose already flags this as a labeling mistake, not a data error, and the pixels/number
    agree it's dark.
  - `02-03-appearance-system-desktoplight.png` (is_dark=false / light desktop) → **243.85**
    (driver claimed 243.8).
  - `02-05-appearance-system-desktopdark.png` (is_dark=true / dark desktop, 2nd run) →
    **33.12** (driver claimed 33.1).
- Cross-checked the cited baselines directly: `2026-08-14-light-settings-appearance.png` →
  243.87 (driver cited 243.9); `2026-08-13-cosmic02-dark.png` → 26.66 (driver cited 26.7).
  Both match.
- Opened all three new captures with `Read` and looked. Frame 1 and frame 3 genuinely show a
  dark-themed Settings → Appearance panel with **System** selected in the segmented control;
  frame 2 genuinely shows the same panel light-themed, System still selected. This is real,
  readable pixel evidence, not just a number.
- Confirmed `request`-independent facts the driver relied on don't contradict anything on
  record; the prior explicit-click half (`16-fresh3.png` → `17-fresh3-after-click.png`,
  System → Light, socket read-back `theme:light`) is present on disk under
  `reference/linux-progress/drive-E02-set/` and unchanged by this pass.

**One evidence-hygiene note (not disqualifying):** `02-05-appearance-system-desktopdark.png`
is not a clean single-surface capture — the frame contains a second window's worth of content
(a Projects sidebar listing `tiller`/`rust/gpui-rewrite`/`linux/gpui-waku` and a chat composer)
tiled alongside the Settings window, most likely a leftover/second Wayland toplevel in the same
headless compositor rather than the operator's real desktop bleeding through. It does not affect
the measurement or the visual read: both regions in that frame are dark, so the whole-frame mean
is not being skewed by mixed content, and the Settings panel itself is legible and correct.
Flagging this so the next agent who touches this capture directory doesn't mistake it for a
clean single-window shot.

**Why PASSED and not half-proven still:** The row's own recorded gap was specifically
"System-follows-real-desktop... still unexercised" — the triage plan
(`triage/T3-set-plan.md`) names exactly this as the missing half and prescribes exactly the
drive performed: flip the real desktop's color-scheme, select/relaunch System, confirm the
resolved appearance tracks it, in both directions. That is what happened, with a positive
control (numbers matching known baselines) rather than a bare screenshot. Combined with the
already-on-record explicit System→Light click-through, both halves the row's history ever named
are now closed by genuine live evidence.

**Caveat carried forward, not blocking:** live D-Bus `SettingChanged` subscription (the app
reacting to a desktop theme flip *while already running* in System mode, no re-click) was not
isolated this pass either — `follow_portal` re-triggers on selection/launch, not on a live
signal. This is the same caveat triage flagged as "a real, separate build," not part of this
row's clause.

## `F-SET-24` — ledger line 312, was **half-proven** → **half-proven** (unchanged)

**Verified independently:**

- `grep -rn "request_permission" rust/crates/ --include=*.rs | grep -v browser.rs` returns
  nothing — confirms zero callers of the browser-permission API anywhere outside
  `tiller_ui/src/browser.rs` itself. Inside that file, the only call sites besides the
  method's own definition are two unit tests (`permission_doorhanger_resolves_and_persists_by_
  origin`, lines ~1666/1677). There is no production wiring from real webview page content on
  this lane (there cannot be — the lane renders no page content at all).
- Read `docs/linux-rewrite/WAYLAND-LANE.md` in full. It documents, unambiguously and
  unchanged, "❌ No webview content — the embedded browser needs an X11 window handle and gets a
  Wayland one; its chrome renders, the page does not. Every `F-BRW` row belongs on `DISPLAY=:1`"
  and the matching walkthrough section "The embedded browser does not work on this lane." This
  independently confirms the driver's claim rather than just trusting their citation.
- Opened `04-03-settings-permissions.png` (the row's on-record capture for the empty-state
  half) with `Read`. It genuinely shows a "Browser origin grants" card, subtitle "Origins
  allowed by the browser agent permission prompt.", body text "No browser origins have been
  granted.", and a "Revoke all" control. This is the same capture already on record; nothing
  new here, but it holds up.

**Why half-proven, unchanged, and not `UNREACHABLE`:** the grant/revoke round trip is reachable
on this codebase — via the X11 lane, which this Wayland-only slice is explicitly barred from
using (shared drive-lock mutex, out of scope per this slice's own brief). `UNREACHABLE` is
reserved for "no route can exist on Linux," which is false here — a route exists and is
documented, just not from this lane. The driver's own claim label ("could-not-reach") isn't in
the ledger's vocabulary and was correctly not asserted as a verdict; treating it as a
reconfirmation of the existing `half-proven` (proven: empty state; owed: grant/revoke, needs
X11) is the right mapping. No new capture was taken (`discriminating: false`, correctly self-
reported by the driver) — this pass only re-verified the standing constraint still holds, which
it does.

I did not attempt to drive the X11 lane myself to close this row — that would be re-driving,
outside a critic's role for this task, and `DISPLAY=:1` is a shared global mutex this slice is
explicitly instructed not to touch.
