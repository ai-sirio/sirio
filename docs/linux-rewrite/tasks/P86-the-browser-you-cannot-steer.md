# P86 — the browser you can see but cannot steer

**Owner: `codex11`.** Worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch
`linux/gpui-waku`. All of this is `browser.rs`, which is yours.

P83 mounted the browser and it genuinely works: a Browser tab opens from the tab-bar `+`, WebKitGTK
renders a real page, an in-page link navigates, the address field submits, and the tab **survives
quit/relaunch** — I verified every one of those live tonight. This piece is about the three things
that do not work, all found by driving the real app, all measured rather than guessed.

Read `docs/linux-rewrite/INVENTORY-LEDGER.md` rows `F-BRW-01` through `F-BRW-09` first — I rewrote
all nine tonight and each carries its evidence and its limits.

---

## Part 1 — the page is painted outside its pane (this is the big one)

`F-BRW-01`. The native WebKit child is drawn at **729x679 at (331,114)**. The content area it should
fill is **850x792 at (386,133)**. Measured by pixel-scanning the screenshots, not estimated:

| axis | observed | correct | ratio |
|---|---|---|---|
| origin x | 331 | 386 | 0.8575 |
| origin y | 114 | 133 | 0.8571 |
| width | 729 | 850 | 0.8576 |
| height | 679 | 792 | 0.8573 |

**One uniform factor of 0.8576 in all four numbers, about the window's top-left corner.** That is a
coordinate-space conversion, not a forgotten offset — a missing sidebar offset would move the origin
and leave the size correct. The rect was byte-identical in five captures across three separate app
sessions, so it is static, not paint lag.

What the user sees: the page covers **55 px of the sidebar** (the `Primary` badge is clipped to
`Pri`), and the right and bottom of its own pane are left unpainted. It also covers chrome rows
y 114-132, which matters for Part 3 because a native X11 child cannot be z-ordered below the GL
surface.

The suspect is `browser.rs:1176`, `NativeWebViewElement::prepaint`, which hands GPUI's `bounds`
straight into wry's `LogicalPosition`/`LogicalSize`:

```rust
let _ = webview.set_bounds(Rect {
    position: LogicalPosition::new(f32::from(bounds.origin.x), f32::from(bounds.origin.y)).into(),
    size: LogicalSize::new(f32::from(bounds.size.width).max(1.0), ...).into(),
});
```

Two device-independent-pixel conventions are being assumed equal. Find out which side applies the
~1.166 factor and convert explicitly. The X display is 96 DPI at scale 1.0 (`xdpyinfo`), and the
driver never resizes the window — so this is the app's own geometry, not a harness artifact.

**Forbidden:** do not divide by a magic 0.8576, and do not add a constant that happens to line the
page up on this machine. If you cannot find the real conversion, say so and report what you ruled
out — a wrong constant here is invisible until someone runs a different display.

## Part 2 — the address field can only append, never replace

`F-BRW-03`. Return **does** navigate; that half works. But `ctrl+a` does not select the field's
contents and the caret ignores click position — it always lands at end-of-text. So typing a URL
concatenates it onto the existing one. This is the literal string that reached the network:

```
https://www.iana.org/help/example-domainshttps://www.iana.org
```

The field is therefore unusable for its actual purpose the moment it holds anything. Needed:
select-all, a caret that respects where you click, and text selection replaced by typing.

**The in-repo precedent is `chat.rs:1751`** (`event.keystroke.key_char` guarded on
`!modifiers.platform && !modifiers.control`) and `codex11` has just built the same tier for the
editor in `file_view.rs:234` under P84. **Imitate the pattern, do not transplant either file's
code** — every line of Tiller is written from scratch, and a critic finding copied code counts as a
gap, always. On Linux the platform modifier is **Super**, not Control.

## Part 3 — Back: reproduce it before you change anything

`F-BRW-02`. I found a contradiction I could not close from the code, and I would rather hand you the
contradiction than a wrong diagnosis.

**Live:** after a real in-page link navigation, clicking `<` changed nothing — not the page, and not
the address field — after 4 s. Two positive controls say the click landed: the button renders its
hover background in that very screenshot (hit rect approx x 400-435, y 87-112, and I clicked
417,104), and a click at the same y on the address field took focus.

**The address field is the tell.** `navigate_history` sets `address_draft` on success
(`browser.rs:791`), so a working Back would have flipped the field back to `example.com` immediately,
before any repaint. It did not. So `go_back()` returned `None`, so `can_go_back()` was false, so
`history_index` was still 0.

**But that should be impossible.** Both `did_start_navigation` (:374) and `did_finish_navigation`
(:382) route through `record_navigation`, and the address bar *did* update to the iana URL — and the
only paths that write the address are those two. Something in that chain does not run live the way
it reads.

Places worth looking, in order: whether the event drain (:855) actually runs after a navigation that
originates inside the child window rather than from GPUI; the `enabled` flag plumbed into
`browser_button` at :911 and whether a disabled button still renders hover; and whether input near
the chrome's lower edge is being eaten by the misplaced child window from Part 1 — which would make
Part 1 the cause of Part 3, and is the single most interesting possibility here.

**Reproduce it live first.** If Back works when you drive it, say so plainly and move on — I would
rather this row be wrong than have you rebuild something that works. Three audits tonight found
stale FAILED verdicts sending builders after working features; do not become the fourth.

Forward, Reload and Stop were **never exercised** — my second drive died on its timeout. Exercise all
four.

## Part 4 — only if Parts 1-3 land with time to spare

`F-BRW-08` / `F-SET-24`: a Permissions section in `settings.rs` that lists granted origins, revokes
one, and revokes all, plus the empty state. `browser.rs` already exposes `allowed_origins`,
`revoke_origin` and `revoke_all_origins`, and `settings.rs` became yours in tonight's reassignment,
so both halves are on your side of the map — this is work, not a seam. The v11
`browser_origin_grant` table exists and currently holds **0 rows**, because nothing has ever granted
an origin; `F-BRW-06`/`F-BRW-07` stay blocked until the doorhanger is triggered live.

---

## Driving the display

One X pointer, four agents. Take the lock:

```bash
TILLER_DRIVE_LABEL=codex11 TILLER_DRIVE_LOCK_WAIT=900 timeout 900 \
  Scripts/linux-drive.sh reference/linux-progress/p86-<name>.png '<actions>' 8
```

Known coordinates in the 1715x972 window: tab-bar `+` (1219,57) · **New Browser** (1297,167) ·
back `<` (417,104) · forward `>` (458,104) · reload (500,104) · address field (790,104) ·
the example.com "Learn more" link (520,336).

Two traps that have already cost this project passes: give every in-script `shot` a **distinct
filename**, or the driver's own final capture overwrites it and your frames appear out of order; and
take **two** captures after an action, because the first often shows the pre-action frame.

Budget the timeout generously — my 560 s drive died because `cargo build` contended with three other
agents compiling.

## Done means

1. The page rect measured again after your fix and reported as numbers, next to the content area's
   numbers. "Looks right" is not a result.
2. A URL **replaced** in the address field and submitted, with the resulting address bar shown.
3. Back, Forward, Reload and Stop each exercised live, each with its outcome stated — including
   "already worked" if that is what you find.
4. `cargo fmt` and the `tiller_ui` suite green; `git status --short | grep '??'` before you finish.
5. Do **not** edit `INVENTORY-LEDGER.md` — only a critic moves a verdict. Report what you exercised
   and a critic will judge it.
