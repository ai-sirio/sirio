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
