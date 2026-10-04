# Verifying the change request tab on Ely

The change-request detail tab (`ChangeRequestTab`) is drawn with Ely
components: `docs/superpowers/specs/2026-10-03-change-requests-on-ely-design.md`
§5, delivery 2. Its behaviour did not change, so the gate is the three
end-to-end scripts that already drive it, with their steps untouched.

## Run it

    Scripts/Tests/test-forge-ui-e2e.sh --state-only
    Scripts/Tests/test-forge-diff-e2e.sh --state-only
    Scripts/Tests/test-forge-actions-e2e.sh --state-only

Frames need a display; software Vulkan makes them work under Xvfb:

    Xvfb :92 -screen 0 1400x1500x24 &
    export VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json
    Scripts/Tests/test-forge-ui-e2e.sh --display :92 --appearance light --window-size 1300x1400
    Scripts/Tests/test-forge-actions-e2e.sh --display :92

`--appearance light|dark` (new) seeds the scratch database through
`sirio_persistence`'s `appearance_seed` example; `--window-size WxH` (new)
resizes the app's window before each capture.

## What was seen (2026-10-03, Xvfb, lavapipe)

- Header: title and number, an *Open on the forge* and a *Refresh* icon
  button, three icon buttons for edit / draft / close, the state as a badge
  (*Open* in green), the meta line.
- Inner tabs: Ely `Tabs` with counts as notes, the underline under the
  selected one, and the CI-state icon on *Checks* only. A narrow pane scrolls
  the strip.
- Conversation: an Ely `Timeline` with an icon per entry and its age at the
  right; a review that asks for changes has an amber icon.
- Composer: a multi-line input and a *Comment* split button with its menu
  chevron (seen at the bottom edge of a tall window).
- A failed write: one red line with a severity icon under the header, its
  words selectable.
- Light mode: a pale ground with the same layout. This needed
  `sync_theme_if_changed` in the tab's `render` (see
  `docs/testing/ely-forge-probe.md`).

## Found while building it

- Ely's `TextInput` keeps CRLF where the old field folded it, so an untouched
  GitHub description read as an edit; the actions script's "untouched edit"
  step caught it. Inputs now normalise line endings on the way in and before
  a comparison.
- gpui's debug build panics on a duplicate accessibility node id. Timeline
  entries that carry several `selectable_text`s outside one id scope need
  indexed ids; only a framed run draws, so only it saw the panic.
- The CI refresh timer was dropped when a refresh was refused by a rate
  limit, and never re-armed (a leftover of A). A unit test now pins it.

## Not established

- **Keyboard use of `Tabs`**: a click does not give its strip focus and Sirio
  binds no Tab traversal. The tab is used with the pointer.
- **The Cmd/Ctrl+Enter chord**: bound by Ely as `InputEvent::Submit` and
  exercised through the same send as the *Comment* button; the real key press
  was not sent.
- **The split button's open menu, the edit card and the comment editor** were
  verified through the control socket and the scripts, not in a frame: the
  tab's visible area ends above them in every window size tried.
- **Framed actions runs are not reliable**: with software rendering, two of
  five full runs stopped at the GitLab half's start-up
  (`state never became 'ready' (last: 'unknown-forge')`) and passed on a
  rerun; `--state-only` passed every time. Not reproduced without frames and
  not explained.
- **Platforms**: Linux X11 under Xvfb only. `glab` is not installed, so the
  scripts' `cli` stage prints `SKIP:` for its GitLab half.

## Delivery 3: merge, reviewers, labels (B2b)

`docs/superpowers/specs/2026-10-03-change-requests-on-ely-design.md` §6. The
forge side is proven on the wire, then the tab in a real Sirio:

    Scripts/Tests/test-forge-actions-e2e.sh --stage merge
    Scripts/Tests/test-forge-actions-e2e.sh --stage metadata
    Scripts/Tests/test-forge-actions-e2e.sh --stage cli
    Scripts/Tests/test-forge-actions-e2e.sh --stage ui --display :93

`merge` sends each merge, auto-merge and cancel to both fake forges and reads
back what they logged: the head the user saw (`expectedHeadOid` / `sha`), the
method, the message; a blocked change request, a moved head and a waiting one
without auto-merge send nothing; GitHub's branch deletion is a REST `DELETE`
after the merge, skipped for a fork's head, and a failed one leaves the merge
done with a warning. `metadata` sends reviewers and labels by the ids each
forge gave, searches candidates by the typed text, and an older GitLab without
`mergeRequestSetReviewers` answers *Unsupported*. `ui` drives the strip, the
dialog and both pickers through the debug-only control verbs (`merge-open`,
`merge-confirm`, `cancel-auto-merge`, `picker-open`, `picker-type`,
`picker-pick`, `picker-close`); `POST /__push` on the fake forge stands for a
push between the dialog and the click, and `POST /__checking` for a forge
still working out whether it can merge (GitHub `UNKNOWN`, GitLab `CHECKING`)
until the next `/__reset`. The fake forge also stands for GitHub having
deleted a merged head itself (`autodeleted`: no warning), a review request
Sirio cannot send back (`mannequin`: a removal is refused), and a GitLab
organisation's labels above the project's own group (offered only with
`includeAncestorGroups: true`).

### What was seen (2026-10-03, Xvfb, lavapipe)

- **The strip**, dark and Light, GitHub and GitLab: *Ready to merge* in green
  with the method `Select` (*Merge commit* preset) and a primary *Merge*;
  *Blocked: a review is required* in amber with both controls disabled;
  *Auto-merge enabled · squash* with *Cancel auto-merge*. In the narrow
  Secondary pane the strip and the reviewer/label row wrap.
- **The dialog**: *Merge #101* / *Merge !201*, the method, target and short head
  sha, the preset title (*Fix the login redirect (#101)* for a squash), an
  empty message, *Delete branch* (ticked on GitLab, whose project says so),
  *Cancel* and *Merge*.
- **The pickers**: the search field, the current reviewers ticked, the found
  candidate with its name as a note.
- (2026-10-04, after the final review) **A refused merge** names its reason
  inside the dialog (*The commit title cannot be empty.*) while the strip's
  button no longer spins; **a forge still checking** shows *Checking whether
  it can merge…* with *Merge* disabled, and the tab turns it into *Ready to
  merge* by itself once the forge has decided.

Not seen, or seen with a remark:

- Ely's `Dialog` text fields draw no border, as Ely's `TextInput` draws them
  elsewhere in this tab. (Its scrim used to hide the app: the theme mapped
  Ely's `backdrop` to Sirio's `overlay`, a hover tint that is opaque on the
  grey ladders. It is now bezel's modal scrim, and the frame shows the app
  dimmed behind the dialog.)
- The method `Select`'s open list, a picker's empty or failed line, and the
  dialog in Light were not captured.
- A reviewer who only reviewed (no pending request) has no id on GitHub and is
  not listed in the picker: `requestReviews` manages pending requests only.
- The GitLab half of the `cli` stage, and with it the REST approval and the
  REST cancel of an auto-merge through `glab`, printed `SKIP:` (no `glab`).
- The framed `ui` run's GitLab start-up flake (above) did not recur in the run
  these frames come from; it did in an earlier one.

## Delivery 4: the list

`docs/superpowers/specs/2026-10-03-change-requests-on-ely-design.md` §7.
The right panel's Change requests view uses Ely tabs, search, row icons,
context menu, callouts, skeletons and a masked token input. The forge's
brand mark stays in the existing icon set, which has the brand glyphs.

### Run and capture

From the repository root, with Xvfb and software Vulkan:

```bash
export TMPDIR=/home/epalmisano/.cache/st
Xvfb :94 -screen 0 1600x1500x24 -nolisten tcp -noreset > "$TMPDIR/d4f-xvfb.log" 2>&1 &
VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json Scripts/Tests/test-forge-ui-e2e.sh --display :94 --out-dir "$TMPDIR/d4f-dark"
VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json Scripts/Tests/test-forge-ui-e2e.sh --display :94 --appearance light --out-dir "$TMPDIR/d4f-light"
ls "$TMPDIR/d4f-dark/frames" "$TMPDIR/d4f-light/frames"
```

Use a free X display. Sirio starts a centered 1470×833 window: the display
must fit it. The plan's existing `:93` was only 1400 pixels wide, so those
first captures cut both outer edges. The complete runs used a private
`:94` at 1600×1500; no application layout change was needed for that crop.
Each capture is matched to the app's PID. The two artifact directories
retain the transcript, app and fake-forge logs as well as `frames/`.

Steps 1–4 still prove sign-in, the saved and trimmed token, filter results,
and the detail tab's conversation and checks. The added steps prove:

- **Search (5):** `surface.change_requests.search` sets the real input;
  its 300 ms debounce reaches the fake forge, a query matching nothing
  empties the list, and clearing it brings the rows back.
- **Known reset (6):** choosing a filter after a rate limit shows its
  callout; another filter sends no read while paused. Rows return by
  themselves when the reset arrives.
- **No reset (7):** a 429 with no reset also pauses reads, including a
  filter change. Native tests confirm that another filter request stays
  paused; the exact one-minute duration is not tested with an elapsed clock.
- **Unknown host (8):** a forge that cannot be identified asks which forge
  it is instead of trying a guessed one.

The loopback fake's hooks are `POST /__ratelimit?seconds=N` (403 with a
reset N seconds ahead), `POST /__throttle` (429 with `Retry-After`, without
a reset), and `POST /__reset` (clear both). A second fake with
`--flavor none` serves the unknown host. Free words in the fake's search
must all occur in the returned row.

The list bounds the actual pause deadline: an absent or expired reset uses
60 seconds, and a reset more than an hour away uses now + 3600 seconds.
Known-reset retry copy uses that same bounded deadline. The native
`a_reset_in_the_past_or_far_future_never_spins` guard reproduced an early
request for a past reset and no retry within an hour for a year-2100 reset.
It now proves no request before the respective 60-second or one-hour pause,
then one resume request. GPUI advances its simulated timer clock; the guard
checks the real wall-clock deadline and moves it by the elapsed interval
before letting the resume callback observe expiry.

### What was seen (2026-10-04, Xvfb, lavapipe)

Both modes have `not-connected.png`, `list.png`, `mine.png`, `empty.png`,
`rate-limited.png`, `unknown-host.png`, `detail-conversation.png` and
`detail-checks.png`; all sixteen complete frames were read.

- The four-choice strip shows the Mine (person), To review (eye), Open
  (pull request) and Closed (archive) icons, with the review count of 2
  kept as a note. There are no tab labels or horizontal scrolling; Mine
  and Open have the expected active underline in their respective frames.
  An active label still exceeded the 220 px floor, so the strip uses icons
  for every choice. A whole-window glyph check failed on the old 320 and
  240 px frames, then passed with all four choices selected in turn at
  320, 240 and 220 px (12 frames). The scratch harness and check are
  `$TMPDIR/ely4-filter-frames.sh` and `$TMPDIR/ely4-filter-frame-check.py`;
  their frames are in `$TMPDIR/ely4-filters-{red,green}-{320,240}/frames`
  and `$TMPDIR/ely4-filters-green-220/frames`.
- The branch card and rows carry green open glyphs; the draft glyph is
  muted, with CI, review and comment icons alongside the rows.
- Sign-in and unknown-host callouts have a blue severity rule and icon;
  the rate-limit callout has an amber rule and a retry time, with Retry.
- Search is visible in the empty frame, with its clear button and
  “Nothing here.” The skeletons are absent once rows exist.
- The login copy button, token field, Save, header controls and row
  metadata fit inside the panel's right edge. The token field has its
  lock and eye controls; it is empty in these frames. The native Enter
  test proves masking with text entered and one submission per Enter.
- Light has a pale ground throughout, including the Ely controls.

The complete dark sign-in frame exposed black login-command text on a
dark wash: the Callout migration had lost the old notice's inherited
foreground. An explicit theme foreground fixed it. A command-only pixel
check failed with 0 visible light ink pixels before the fix and passed
with 349 afterward; the copy glyph was excluded from that check.

### Not seen

- The right-click menu on screen. The native pointer guard opens it and
  proves Copy link copies the clicked row's URL. Outside-click dismissal
  is implemented by Ely, but that guard does not exercise it.
- A pasted token in a frame; masking and Enter were tested natively.
- macOS, Windows or Wayland; these captures use Linux X11 under Xvfb.
- `glab` transport and CLI detection: `test-forge-e2e.sh` printed SKIP
  because `glab` is not installed. Token transport and the available
  `gh` path ran. Forge, diff, actions UI and both framed UI runs passed.

Final review also noted a baseline debounce gap: typing and then clearing
back to the applied query before 300 ms can leave the pending query active.
That comparison and scheduling behavior already existed before this
delivery and was retained under its unchanged-behavior constraint.
