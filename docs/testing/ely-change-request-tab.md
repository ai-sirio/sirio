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

## Delivery 5: the Changes surface

`docs/superpowers/specs/2026-10-03-change-requests-on-ely-design.md` §8 and §9.
The Changes surface (local changes, a commit, a change request's *Files*)
draws with Ely:
- the toolbar's `ToggleGroup` and `IconButton`s;
- sections with a `CountBadge`;
- rows with `GitStatusBadge` and `DiffStat`;
- bands with an `IconButton`;
- line and changed-word washes from Ely's palette;
- `Callout`s for git errors and unavailable diffs, and `Skeleton` loading.

Its engine is unchanged. Discard asks inside the window, through an Ely
`Dialog`. A file row's right-click offers *Copy path* and, on a change
request, *Open on the forge*. A change request's one section is headed
*Changes*. A deleted file's snapshot is titled `gone.txt (deleted in #101)`,
and once restored it never offers a same-named local file. A snapshot tab
wears a lock.

### Run and capture

```bash
export TMPDIR=/home/epalmisano/.cache/st
Xvfb :94 -screen 0 1600x1500x24 > "$TMPDIR/xvfb94.log" 2>&1 &
export VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json
Scripts/Tests/test-changes-e2e.sh --display :94 --out-dir "$TMPDIR/d5-dark"
Scripts/Tests/test-changes-e2e.sh --display :94 --appearance light --out-dir "$TMPDIR/d5-light"
Scripts/Tests/test-forge-diff-e2e.sh --display :94 --out-dir "$TMPDIR/d5-diff"
```

`test-changes-e2e.sh` drives a real repository with one staged, one changed
and one untracked file. Its frames:

- **`local-unified`:** the `A`/`M`/`U` badges and `+N −M` with the five
  dots. The replaced line washes only `20` → `2000`, and the `città` line
  draws its accents.
- **`local-split`:** the same diff in two columns, zipped as before.
- **`error`:** with `.git` moved away, the `Callout` carries git's own
  message and Retry. Moving `.git` back clears it.
- **`discard-dialog`:** the in-window confirmation over a scrim. The
  debug-only `surface changes confirm` then discards, and the script checks
  `git diff` is empty. *Discard all*, closed with `surface changes dialog
  --close`, discards nothing.

`test-forge-diff-e2e.sh`'s frames add:

- **`*-commit`:** a commit, still headed *Staged*.
- **`*-files-diff`:** the change request's *Changes (N)*, with the `R`/`D`/`M`
  badges.
- **`*-snapshot`:** the snapshot tab's lock.

Its step 3 checks the deleted file's title. Step 3b restores both snapshots
beside same-named worktree files: `login.rs` offers its local copy, and the
deleted `gone.txt` does not.

A headless (`--state-only`) run never draws, so the Changes poll never arms.
The script asks for the toolbar's Refresh through `surface changes view
--refresh` wherever it waits for git.

### Found while building it

- The snapshot lock first sat transparent over the file glyph and read as
  noise. It now sits on the page's ground.
- The `.git`-moved error needs a refresh. In a framed run the 1 s poll gives
  one; headless, `view --refresh` does.

### Not established

- macOS: the system sheet is gone, but the dialog is not seen there.
- Windows and Wayland.
- Keyboard use of the view-mode toggle.
- *Open on the forge* opened in a browser: its URL is unit-tested.
- GitLab's per-file anchor: the diffs page opens at its top.
- The Split view's word washes in a narrow pane, where long lines are
  clipped as before.

## B2c: CI re-run and the log tab

`docs/superpowers/specs/2026-09-29-change-request-actions-design.md` §15.
The `ci` stage drives GitHub and GitLab against loopback fake forges. It
checks re-run job/run requests, one write in flight, readonly permission
gating, the refreshed group/row order, log bounds and signed-URL credentials,
folds and copy, *Jump to first error*, expiry/retry, unknown-job refusal,
deduplication and identity restoration across a quit.

### Run and capture

From the repository root, keeping the exported cache `TMPDIR`:

```bash
export TMPDIR=/home/epalmisano/.cache/st
Scripts/Tests/test-forge-actions-e2e.sh --stage ci --state-only --out-dir "$TMPDIR/b2c-state"
```

The controller runs both appearances with Xvfb and lavapipe. Choose a free
X display; `:94` below needs a screen large enough for the complete window.
The first run uses the app's default appearance; the second seeds Light in
each isolated CI/UI database through `sirio_persistence`'s `appearance_seed`
example. The same `--appearance` option accepts `dark` when an explicit dark
seed is wanted.

```bash
export TMPDIR=/home/epalmisano/.cache/st
Xvfb :94 -screen 0 1600x1500x24 -nolisten tcp -noreset > "$TMPDIR/b2c-xvfb.log" 2>&1 &
b2c_xvfb_pid=$!
export VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json
Scripts/Tests/test-forge-actions-e2e.sh --stage ci --display :94 --out-dir "$TMPDIR/b2c-dark"
Scripts/Tests/test-forge-actions-e2e.sh --stage ci --display :94 --appearance light --out-dir "$TMPDIR/b2c-light"
ls "$TMPDIR/b2c-dark/frames" "$TMPDIR/b2c-light/frames"
kill "$b2c_xvfb_pid"
wait "$b2c_xvfb_pid" 2>/dev/null || true
```

Each capture is matched to the app PID. Both artifact directories retain
`transcript.log`, fake-forge request logs, `app-github-ci.log`,
`app-gitlab-ci.log` and `frames/`. Read every generated frame in both modes:

- `github-checks`, `github-checks-rerun`: grouped jobs and the result of
  re-running them.
- `github-log`, `github-log-error`, `github-log-gone`: the log, the jump to
  its first error, and the expired-log Callout with the last good text.
- `github-log-waiting`, `github-log-truncated`: an unpublished running job
  and the retained-tail notice.
- `gitlab-checks`, `gitlab-checks-rerun`: the pipeline-level *Re-run failed*
  above its stages and the result of the write.
- `gitlab-log`, `gitlab-log-error`, `gitlab-log-gone`: the log, its first
  error and its expired-log state.

The framed GitLab half also checks that a visible running log reloads at
least twice over 12 seconds, that hiding it stops reads once the tick
already due has gone out, that a trace slower than the tick is never read
twice at once, and that a rate limit suppresses reads until its reset and
no longer. State-only runs skip those draw-dependent checks.
The socket's pending `top` index proves a logical jump in state-only mode;
inspect `*-log-error` to establish the actual viewport placement.

### What was seen (2026-10-05, Xvfb, lavapipe)

Both appearances ran to `FORGE ACTIONS E2E OK` on `:96` (1600x1500), with
12 PID-matched frames each; every frame was read.

- **Checks.** GitHub groups by workflow run (*CI 1 failed · 2*, *Lint*,
  *Other checks*), with *Re-run failed* on the CI header and Re-run plus
  *Open in browser* on the failed `test` row only; `deploy/preview` (a status
  context) has neither. After the re-run CI folds, since nothing in it is
  failed or running, and running *Lint* sorts first. GitLab draws a
  *Pipeline #45* row carrying *Re-run failed* above the `test`, `build` and
  `deploy` stages; after the write `rspec` shows as created and has no
  Re-run.
- **Log.** The toolbar reads `test  complete` (`lint  running` in amber),
  with Refresh, *Jump to first error*, Copy log and Open in browser. Line
  numbers sit in a dim gutter; folded GitHub groups show a chevron and a
  Copy button. `##[error]` / `ERROR:` lines are red and `$ bundle exec
  rspec` green. GitLab's open section keeps its members, and the
  carriage-return progress line shows only its last state.
- **Jump.** In both fixtures the visible lines fit the viewport, so the jump
  leaves row 0 on top and the error on screen (`scrollable=no`,
  `error_shown=yes`); the first framed run's strict `top` check was wrong
  for that case, not the jump.
- **Gone.** A red *Log unavailable* Callout (`not found on ghe.test`,
  `not found on gitlab.test`) with *Retry* and *Close*, the last good log
  still below it.
- **Waiting.** A running GitHub job shows the blue notice *The log is not
  available until the job finishes.* with *Retry*.
- **Truncated.** *Showing the last 4 MiB of the log; 1 MiB before it are on
  the forge.* with *Open in browser*; the tail's gutter starts at 1, and the
  horizontal scrollbar is drawn along the bottom.
- **Reload (GitLab, drawn).** A visible running log read its trace 3 times
  in 12 s. Once hidden, one read already due went out and then none in the
  next 12 s. With a 7 s trace, no two reads of it were ever in flight at
  once (`/__stats`). Under a 20 s rate limit nothing was read for 12 s, and
  reads resumed on their own after the reset.

### Not seen

- Selection inside a log line, dragging either scrollbar, and a jump in a
  log long enough to scroll: no step drives them.
- The minor flaws the frames show: the horizontal scrollbar overlaps the
  last visible row, and the truncation notice says "1 MiB … are".
- `glab` log and write paths: it is not installed on this machine. Token
  means on both fake forges and the installed `gh` means are exercised.
- A live GitHub running log or any live forge write. The fake proves the
  unpublished state; nextest's live bodies skip with isolated credentials.
- macOS, Windows and Wayland. The Windows-only test helpers were updated,
  but the Linux build does not compile or execute them.
- Saving a token through the socket while its initial host probe still
  runs: the full diff regression exposed an unsigned-cache race twice.
  Its harness now waits for the initial sign-in/unknown-forge state, like
  the visible UI; production handling of overlapping socket calls is
  outside this B2c proof.


## B3a: review threads in the diff

The `github-threads` and `gitlab-threads` scenarios of
`Scripts/Tests/test-forge-diff-e2e.sh` check one Conversation entry per
published thread, current/old/range anchors, resolved folds, outdated
sections, draft exclusion and cards after the Files range is restored.
Standalone Changes sets the global Split mode before the scenario returns
to the change request and checks the same line anchors.

### Run and capture

From the repository root, choose a free X display and software Vulkan:

```bash
export TMPDIR=/home/epalmisano/.cache/st
Xvfb :96 -screen 0 1600x1500x24 -nolisten tcp -noreset > "$TMPDIR/b3a-xvfb.log" 2>&1 &
B3A_XVFB_PID=$!
export VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json
export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json
Scripts/Tests/test-forge-diff-e2e.sh --display :96 --out-dir "$TMPDIR/b3a-framed-dark"
kill "$B3A_XVFB_PID"
```

This script has no `--appearance` flag, so the controller's framed run will
cover only the app's default dark appearance. The controller runs the command
above and reads its PID-matched frames. The B3a implementation run uses
`--state-only`, with artifacts under `$TMPDIR/b3a-t7*`, and takes no frames.

The new frames cover each forge's Conversation, unified and split thread
cards, and an opened resolved thread. The state assertions also check the
folded outdated section and the rebuilt diff after a graceful relaunch.

### What was seen

Framed run of 2026-10-05, dark, Xvfb `:96` at 1600×1500 with lavapipe, after
the review's fix pass (`FORGE DIFF E2E OK`; frames under
`~/.cache/st/b3a-framed/frames/*-threads-*.png`). Both forges read the same:

- **Conversation.** One compact entry per thread with a published comment,
  in time order among the activity: author, "commented", the age, a footnote
  `on src/login.rs:<line> · <n> replies` with `· Outdated` / `· Resolved`
  where so, and the first comment's body. The draft-only GitHub thread has no
  entry.
- **Unified, after `thread --reveal` of the open thread.** The view starts at
  the revealed thread's line (`42 + edited 42`) with its card open under it —
  both comments, avatars, ages, bodies. Below it the GitHub range thread sits
  under line 45 as "lines 44–45", with 44 and 45 drawn out of what was a
  band, then "15 hidden lines". The first framed run, before the fix pass,
  showed the top of the file here with the card below the fold: the reveal
  ran in the same rebuild that spliced the list, when no row was measured,
  and `scroll_to_reveal_item` read the target as already in view.
- **The file row** carries a comment icon and the number of open threads
  drawn in the diff (3 on GitHub, 2 on GitLab). Right under it, "› 1 outdated
  thread", folded. The context band before line 40 shows "22 hidden lines",
  then line 40 and the resolved card folded to one line ("Resolved · alice ·
  0 replies", "1 comment"). The old-side thread sits under the deleted
  `42 - line 42` with an `old` tag.
- **Split.** The same cards under the same rows: the resolved card under
  line 40, the open thread under the zipped `42 - line 42 | 42 + edited 42`
  row. Switching the mode rebuilds the list, so the view is at the top.
- **Resolved opened** (`thread --toggle`): the header gains a green
  "Resolved" tag and the comment shows below it.

Not right yet, recorded as deferred minors in the B3a ledger: card text is
drawn larger than the diff and the Conversation; a thread entry says
"0 replies"; an old-side thread's Conversation entry does not say "old".

### Not seen

- `glab` transport or detection: it is not installed on this machine.
- Live forges: these proofs use loopback fakes; nextest's live tests use
  isolated credentials and print `SKIP:`.
- macOS, Windows or Wayland, or a light appearance for this diff harness.
- A GitLab range: GraphQL reports only its final line.
- Visual appearance during the implementation run; it uses `--state-only`.

## B3b: writing in threads

The `scenario_thread_writes` scenarios of
`Scripts/Tests/test-forge-diff-e2e.sh` (GitHub and GitLab) drive the
gutter composer, a unicode range comment, a reply, a resolve confirmed by
the folded card, and a cancelled old-side composer, over the control
socket in a real, isolated Sirio. The `threads` stage of
`Scripts/Tests/test-forge-actions-e2e.sh` proves the same writes on the
wire. State-only runs capture no frames; the controller's framed run
reads them.

Frames captured per flavour (`<flavour>` is `github` or `gitlab`); the
script's `capture` prefixes each name with the scenario, so on disk they read
`*thread-writes-thread-writes-<flavour>-composer.png` and
`*thread-writes-thread-writes-<flavour>-resolved.png`:

- `…-composer.png`: the open line composer on the `41-43` new-side range,
  before the send.
- `…-resolved.png`: the thread folded after the resolve the forge confirmed.

### Known gaps

- Outdated threads cannot be resolved, replied to or edited in B3b; users do those writes on the forge until B3c.
- A re-read that outdates a thread drops its open reply or edit draft.

### What was seen (2026-10-05, Xvfb, lavapipe, dark)

Run from the repository root, with a private display:

```bash
export TMPDIR=/home/epalmisano/.cache/st
Xvfb :95 -screen 0 1600x1500x24 -nolisten tcp -noreset &
VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json Scripts/Tests/test-forge-diff-e2e.sh --display :95 --out-dir "$TMPDIR/b3b-framed-final"
```

- **Composer**, GitHub and GitLab: a card titled *Comment on src/login.rs
  lines 41–43* sits directly under line 43, inside the diff list, with an
  empty field (*Leave a comment…*), *Cancel* and a *Comment* button that stays
  disabled while the field is empty. The thread cards beside it (below on
  GitHub, above on GitLab) keep their *Reply…* control, plus *Resolve* on GitHub.
- **Resolved**, GitHub and GitLab: after the resolve the forge confirmed, the
  thread folds into one row (*Resolved · alice · 0 r…*, *1 comment*,
  *Unresolve*) above line 41. The open old-side thread at line 42 keeps its
  *old* tag and, on GitHub, *Resolve*.
- **Found and fixed by this run**: the first framed run (on `ac3f0a5c`) showed
  *Unresolve* crossing the card's right border on both forges. The summary
  could not shrink, so the header pushed its action out. The fix
  (`b021296f`) truncates the summary with an ellipsis. On the final run
  (`742a5665`), *Unresolve* sits inside the card with its margin.
- **Not seen**: the gutter *+* is hover-only and every write is driven over the
  socket. So these frames do not show the *+*, a click on *Resolve* (versus
  the header's fold), the field growing with multiline text, or the reply
  field's focus on open. Light appearance was not run; the script has no
  `--appearance`.
