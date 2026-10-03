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
