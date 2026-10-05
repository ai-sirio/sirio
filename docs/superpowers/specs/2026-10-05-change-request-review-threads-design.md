# Review threads in a change request's diff — design

**Date:** 2026-10-05
**Status:** proposed
**Programme:** part **B3** of **B1 → B2 → B3 → C**. A reads the forge, B1
brought the diff into Sirio, B2 lets the user act on a change request from its
tab, **B3 (this document)** draws the review threads in the diff and lets the
user review there, C takes a change request into a worktree and hands it to an
agent.
**Scope of this document:** review threads on GitHub pull requests and GitLab
merge requests. They are read and drawn under their lines in the *Files* diff,
answered, resolved, started on a new line or range, collected into a review
drafted on the forge, and written as suggested changes. Built as three slices
(§11), each with its own plan and pull request. The UI is on Ely only
(`2026-10-03-change-requests-on-ely-design.md`).

## §0 Intent

After B2 the user reads a change request's diff and acts on it from Sirio, but
the review itself still happens in the browser: who said what about which
line, answering it, marking it done, leaving comments of their own across
several files. The Conversation shows each line comment as a footnote, "on
`path:line`", with no thread, no answers and no resolved state.

Success: on a GitHub pull request and on a self-managed GitLab merge request,
the user opens *Files* and sees every discussion under the line it is about.
Resolved discussions are folded in place, and outdated ones sit at the top of
their file with the code they quoted. From Sirio alone the user answers a
thread, resolves it, comments on a new line or range, collects several
comments into one review and submits it with a verdict, and proposes a
suggested change. Sirio never offers a write the forge says the signed-in user
may not make.

## §1 Decisions taken

Settled with the user before this document was written:

| Question | Decision |
|---|---|
| Delivery | **One spec, three slices in order** (§11): **B3a** reads and draws threads, **B3b** replies, resolves and starts a comment, **B3c** adds the drafted review and suggested changes. Each slice has its own plan and pull request. |
| Where threads are drawn | **In the *Files* diff, under their line** (B1 §1: "B3 draws its threads there"), as rows of the Changes surface's own list (§4). Not a side column, and not a separate inner tab. |
| Outdated threads | A **folded "N outdated threads" section under the file's header**. Opened, each shows the code it quoted (the forge's diff hunk) above its comments. They can be answered and resolved there. |
| Resolved threads | **Folded in place**: one compact row, "Resolved · author · N replies", that a click opens. |
| The drafted review | **On the forge**: GitHub's pending review, GitLab's draft notes. It survives a crash and shows in the browser. Sirio reads it like any other state and stops hiding the viewer's pending review. |
| Suggested changes | **Written and drawn**: *Suggest* fills a `suggestion` block, and a received one draws as a small before/after diff. **Applying stays on the forge** (*Open on the forge*). Sirio makes no commit. |
| The Conversation | **One compact entry per thread**, "on `path:line` · N replies · Resolved/Outdated", with its first comment. Clicking it reveals the thread in the diff. Writing happens only in the diff. |
| Starting a comment | **A "+" in the gutter**, on hover. A click comments on that line. A drag, or Shift+click on a second "+", comments on a range. Works in unified and split, on the old side and the new. |
| Discarding a drafted review | **Asks first** (an Ely `Dialog`), the one exception to B2's "merge only" rule (§1 of B2). Discarding deletes written text that cannot be recovered. |

Everything else follows B2: one write door (`Action`, `ForgeClient::act`),
`check_action` refusing what the forge would refuse, writes on the control
socket in debug builds only, no retry, and the forge as the truth after every
write.

## §2 Where it stops today

- `LineComment` (`sirio_forge/src/model.rs`) is `{author, path, line, body,
  at}`. It has no id, no side, no thread, no resolved state and no edit handle.
  GitHub reads it from a review's `comments(first: 50) { path line originalLine
  body }` in the header's timeline. GitLab reads notes that carry a `position {
  filePath newLine oldLine }`. Nothing reads `reviewThreads` or `discussions`.
- The viewer's own `PENDING` review is dropped when the header is parsed
  (`mapping.rs`, `github.rs`).
- `Action` has `Comment`, `Review { verdict, body }` and `EditComment`. There is
  no reply, resolve, new line comment, draft or suggestion.
- The Conversation draws a review's line comments as footnotes and a GitLab
  positioned note as a standalone item. A link calls `ChangeRequestTab::reveal
  (path, line)`, which selects *Files* and calls `ChangesTab::focus_line`.
- `ChangesTab` (`sirio_ui/src/changes.rs`) renders the range diff in a
  `gpui::list`, which measures each item, so rows of any height already work.
  Its `ChangeRow` has `File`, `Hunk`, `ContextBand`, `Line`, `SplitLine` and
  `Unavailable`. Line rows are fixed at 20 px, and there is no slot for anything
  under a line. `reveal_target` matches only `new_line_number`, so an old-side
  line falls back to its file row.
- `Revisions { base_sha, head_sha, start_sha }` is read on both forges.
  `start_sha` is unused so far; B1 read it for B3.
- Ely's `ReviewComment` (upstream `git/review.rs`) is not vendored and not on
  this machine.
- The fake forge has one line comment per forge, with no threads.

## §3 Vocabulary

- **Thread**: a GitHub review thread, or a GitLab discussion on a diff
  position. It holds one or more comments.
- **Anchor**: where a thread or a new comment sits. It is a path, a side (old
  or new), a line, and an optional start line for a range.
- **Outdated**: the forge says the anchor no longer maps onto the current diff.
- **Draft**: the viewer's unsubmitted review on the forge. On GitHub that is
  the pending review; on GitLab, the draft notes.

## §4 Architecture

### `sirio_forge`, with no new crate

- `model::ReviewThread { id: ThreadId, path, side: Side, line: Option<u32>,
  start_line: Option<u32>, outdated, resolved, resolved_by: Option<String>,
  diff_hunk: Option<String>, can_reply, can_resolve, comments:
  Vec<ThreadComment> }`.
- `model::ThreadComment { id, author, body, at, edit: Option<CommentRef>,
  pending }`.
- `Side { Old, New }`. `ThreadId` is an opaque string: GitHub's
  `PullRequestReviewThread` node id, or GitLab's discussion id.
- `ForgeClient::review_threads(number) -> Result<Listing<ReviewThread>,
  ForgeError>`, paged. It is a query of its own rather than part of the header,
  because threads matter only once *Files* is opened.
  - **GitHub**: `pullRequest.reviewThreads { id isResolved isOutdated path line
    startLine originalLine originalStartLine diffSide startDiffSide subjectType
    viewerCanReply viewerCanResolve viewerCanUnresolve resolvedBy { login }
    comments { id author body createdAt viewerCanUpdate diffHunk
    pullRequestReview { id state } } }`.
  - **GitLab**: `mergeRequest.discussions { id resolvable resolved resolvedBy
    notes { id author body createdAt system userPermissions { adminNote
    resolveNote } position { positionType filePath oldPath newLine oldLine
    lineRange diffRefs { baseSha headSha startSha } } } }`. A discussion is
    outdated when its position's `diffRefs.headSha` is not the merge request's
    current head.
  - Every field is checked against the live schema while planning (§13). A
    field an older GitLab lacks follows the existing baseline-query rule.
- `ChangeHeader` gains `draft: Option<Draft { id, comments: u32 }>`, the
  viewer's own pending review or draft notes (B3c).
- `CommentKind` gains `ReviewComment`, so a thread comment is edited through
  the existing `Action::EditComment`.
- `Action` gains `Reply`, `Resolve`, `LineComment` (B3b), `ReviewStart`,
  `ReviewAdd`, `ReviewSubmit`, `ReviewDiscard`, `DraftEdit` and `DraftDelete`
  (B3c). §5 lists them.
- Pure functions in `mapping`, each with unit tests written first:
  `commentable_lines` (§7.4), `gitlab_thread_outdated`, and the thread
  grouping the Conversation uses (§7.5).

### `sirio_ui`

- `ChangesTab` stays forge-neutral. It gains `set_annotations(Vec<Annotation>)`
  with `Annotation { key: u64, path, side, line, start_line, kind: Thread |
  Outdated | Composer }`, and a map from `key` to an `AnyView` its owner
  supplies. An annotation becomes a `ChangeRow::Annotation { key }` placed after
  its anchor row, or under the file header for `Outdated`. `hash_identity`
  covers annotation keys, so the list is spliced only when they change.
- The **gutter "+"** is a `ChangesTab` feature that is off unless its owner
  turns it on with `set_commentable(Commentable)`. A click or drag emits
  `ChangesTabEvent::CommentOn(Anchor)`, and the owner answers with a `Composer`
  annotation.
- `change_request_tab/threads.rs` holds `ThreadView`, one entity per thread
  that owns its expanded state and its reply composer. The anchoring of threads
  to annotations and the Conversation's compact entries live there too.
- `change_request_tab/review.rs` (B3c) holds the review strip, the submit
  dialog and the discard confirmation.
- **The thread card.** Ely upstream's `ReviewComment` is fetched and read
  while planning B3a. It is vendored (recorded in `LOCAL-CHANGES.md`) if it
  fits Sirio's palette and selectable text, as `Tabs` and `Timeline` did.
  Otherwise the card is built from Ely parts already vendored (`IconButton`,
  `Callout`, the badges), any missing part vendored the same way, and
  `SelectableMarkdown`.

### `sirio` (host)

Nothing new beyond wiring the socket verbs (§8). `ChangeRequestSource`
already hands out the `ForgeClient`.

## §5 Actions

Every write goes through `ForgeClient::act` and `ChangeRequestTab::perform`.
After any write the tab re-reads the threads and the header. Nothing is
patched locally, and nothing is retried.

| Action | Slice | GitHub | GitLab |
|---|---|---|---|
| `Reply { thread, body }` | B3b | `addPullRequestReviewThreadReply` | `createNote { noteableId, discussionId }` |
| `Resolve { thread, resolved }` | B3b | `resolveReviewThread` / `unresolveReviewThread` | `discussionToggleResolve` |
| `LineComment { anchor, head, body }` | B3b | `addPullRequestReviewThread` that publishes at once, or REST `POST pulls/N/comments` (§13) | `createDiffNote`, or REST `POST merge_requests/N/discussions` with `line_range` for a range (§13) |
| `EditComment` on a `ReviewComment` | B3b | `updatePullRequestReviewComment` | `updateNote` (exists) |
| `ReviewStart { head }` | B3c | `addPullRequestReview { commitOID }` with no event | none: GitLab's draft exists once it has a note |
| `ReviewAdd { anchor or thread, body }` | B3c | `addPullRequestReviewThread` / `…ThreadReply` with `pullRequestReviewId` | REST `POST draft_notes` (with `position` or `in_reply_to_discussion_id`) |
| `ReviewSubmit { verdict, body }` | B3c | `submitPullRequestReview` (atomic) | REST `bulk_publish`, then B2's approve / request-changes / note |
| `ReviewDiscard` | B3c | `deletePullRequestReview` | REST `DELETE draft_notes/:id` for each |
| `DraftEdit` / `DraftDelete` | B3c | `updatePullRequestReviewComment` / `deletePullRequestReviewComment` | REST `PUT` / `DELETE draft_notes/:id` |

**Refused before sending** (`check_action`, pure, with tests written first):

- an empty body;
- a reply or resolve without `can_reply` / `can_resolve`;
- a line comment or draft comment on a line `commentable_lines` excludes;
- a range that crosses sides or hunks;
- `HeadMoved` when `head` is not the forge's current head (as B2's merge
  does). A position is only meaningful against the revisions the user saw, and
  GitLab needs `base`, `start` and `head` in it.

**GitLab's submit is two steps.** If publishing succeeds and the verdict
fails, the outcome is B2's Warning: published, verdict refused, with the
reason.

## §6 Capabilities

- A thread carries `can_reply` and `can_resolve`. GitHub derives them from
  `viewerCanReply` and `viewerCanResolve`/`viewerCanUnresolve`. GitLab derives
  them from `resolvable` plus the note's `resolveNote`, and from the merge
  request's existing `createNote`.
- New comments and drafts need B2's `can_comment` and a known head. A field
  the forge does not report draws nothing; nothing is offered and then
  refused.
- No new token scope: B2's write scopes (`repo` / `public_repo`; GitLab `api`)
  cover every write here.

## §7 Interface

### §7.1 A thread in the diff (B3a)

- **Anchoring.** A new-side thread attaches to the row whose `new_line_number`
  is its line; an old-side thread uses `old_line_number`. A range attaches to
  its last line and tints the gutter of the lines it covers. In split view the
  card spans the full width with an "old"/"new" tag, because half of the
  Secondary pane is too narrow for prose. A context band never hides an
  anchored line: the band splits around it.
- **Open thread**: a card of comments, each with avatar, author, relative time
  and a Markdown body that can be selected. In B3b its header has *Resolve* and
  its foot has a folded "Reply…" field.
- **Resolved thread**: one row, "Resolved · author · N replies". A click opens
  the card in place.
- **Outdated threads**: a folded "N outdated threads" section right under the
  file header. Opened, each card shows its `diff_hunk` in monospace above its
  comments.
- **File header**: a `CountBadge` with the comment icon and the number of open
  threads. A file with threads does not expand by itself.
- **Not drawn in the diff**: GitHub `subjectType: FILE`, and GitLab
  `positionType` `file` or `image`. They stay in the Conversation.
- **Loading.** Threads are read when *Files* opens and on every Refresh of the
  tab, never polled. A failure shows a `Callout` with *Retry* above the diff;
  the diff itself still shows.

### §7.2 Answering, resolving, starting a comment (B3b)

- **Reply**: "Reply…" opens an Ely multi-line input with *Comment* and
  *Cancel*. Ctrl/Cmd+Enter sends.
- **Resolve / Unresolve** sit in the card's header.
- **The gutter "+"** appears on hover over a commentable line. A click opens a
  composer annotation under the line. A drag, or Shift+click on a second "+",
  selects a range on one side within one hunk.
- **After a send.** On failure the text stays in the composer and the reason
  shows under it. On success the composer closes and the threads are re-read.
  One write is in flight at a time.

### §7.3 The drafted review and suggestions (B3c)

- **Composers** gain *Start a review*, which becomes *Add to review* once a
  draft exists, beside *Comment*.
- **Draft comments** draw with a *Pending* badge and can be edited or deleted.
- **The review strip** shows at the tab's top while a draft exists: "Review in
  progress · N comments", with *Submit review* and *Discard*.
  - *Submit review* reuses B2's composer: a verdict (Comment / Approve /
    Request changes) and a body. When a draft exists, B2's own review composer
    submits that draft instead of a separate review.
  - *Discard* asks first, in an Ely `Dialog`.
- **Suggest** (new-side anchors only) inserts a ```` ```suggestion ```` block
  holding the anchored lines' current text.
- **Drawing a suggestion.** A `suggestion` block in a comment body draws as a
  small diff: the anchored lines in red, the proposed lines in green. An
  outdated thread takes its "before" lines from its `diff_hunk`. There is no
  *Apply*; *Open on the forge* opens the thread.

### §7.4 Which lines are commentable

- **GitHub** accepts a comment only on a line of its own diff hunks, which
  carry 3 lines of context. Sirio's diff carries 24.
  `commentable_lines(hunks, 3)` therefore offers the changed lines and the
  lines within 3 of a change, on each side.
- **GitLab** accepts expanded context lines too. If the plan confirms that
  live (§13), every drawn line is commentable there. Otherwise the GitHub rule
  applies.

### §7.5 The Conversation

- When threads are loaded, a review's line-comment footnotes and GitLab's
  positioned notes collapse into one entry per thread: "on `path:line` · N
  replies · Resolved/Outdated", plus the first comment's text.
- A click calls `reveal_thread(id)`. That selects *Files*, expands the file,
  opens the band or the outdated section, scrolls to the thread and opens it
  if it was folded. A thread no longer in the diff (renamed or removed file)
  falls back to the file header, as today.
- Until threads load, the Conversation draws what it draws today.

## §8 Socket and identity

- **Every build**, because these verbs write nothing:
  - `surface changes read` reports annotation rows (key, path, side, line,
    kind, state);
  - `surface change-request read` reports thread counts (open, resolved,
    outdated) and the draft;
  - `surface change-request thread --reveal ID | --toggle ID | --compose
    PATH:SIDE:LINE[-START] | --cancel` drives the same gestures as the mouse.
- **Debug builds only**, through `surface change-request act`: `reply`,
  `resolve`, `unresolve`, `line-comment`, `edit-comment` for a review comment,
  `review-start`, `review-add`, `review-submit --verdict V`, `review-discard`,
  `draft-edit` and `draft-delete`. A release build answers "unknown method",
  as in B2.
- **Persistence.** Nothing new is saved. Thread state comes from the forge. An
  unsent composer's text is not kept across a quit, the same as B2's
  composers; a draft that matters is on the forge.

## §9 Errors

B2's table (§9 of B2) applies. In addition:

| Error | When | Shown as |
|---|---|---|
| `HeadMoved` | the head changed since the diff was computed | "The branch changed since you opened this" + *Reload* (re-reads, re-anchors) |
| `NotFound` on a thread | it was deleted, or its review discarded elsewhere | the message + *Reload* |
| A rejected position | the forge refuses the line ("not part of the diff") | the forge's reason under the composer; the text is kept |
| GitLab submit Warning | published, verdict refused | B2's warning outcome |

## §10 Testing

**Unit tests**, for pure units only and written first:
- `commentable_lines`;
- anchoring a thread to a row: old and new side, unified and split, a range,
  a line hidden in a band (which must split it), a line not in the diff;
- `gitlab_thread_outdated`;
- parsing a `suggestion` block into before/after lines;
- grouping threads for the Conversation;
- `check_action`'s refusals for every new action.

**E2E** against the loopback fake forges, over the token and `gh`:
- `test-forge-actions-e2e.sh` gains stage `threads` (B3a, B3b). Every read
  and write reaches the wire as the mutation or REST call it means, on both
  forges, with the readonly and `HeadMoved` refusals.
- Stage `review` (B3c): a draft started, added to, submitted with each verdict
  and discarded, and GitLab's Warning.
- Stage `ui` covers, in a real isolated Sirio driven over the socket:
  - threads in the diff, unified and split, including resolved and outdated;
  - the Conversation revealing a thread;
  - replying, resolving, and a line and a range comment through the "+";
  - the review strip, submit and the discard dialog;
  - a drawn suggestion.
- **Fixtures** for each forge: an open thread, a resolved one, an outdated one
  with a `diff_hunk`, a range, an old-side thread, a file-level thread that is
  not drawn, a thread holding a suggestion, and the viewer's draft (B3c).
- **Framed runs** of the `ui` and `threads` stages, dark and light, read by eye
  after each slice: thread rows, resolved, outdated, the composer and the "+",
  the suggestion diff and the review strip.

**Live:** the new documents join `…_answers_every_query_sirio_sends` and
`…_accepts_every_document_sirio_writes_with`, with ids that name nothing. They
stay off both gates and skip without credentials.

## §11 Slices

1. **B3a: read and draw.** Covers `ReviewThread` and `review_threads`, the
   annotation rows of `ChangesTab`, the three thread looks, the Conversation's
   compact entries and `reveal_thread`, the read-only socket verbs, and stage
   `threads` (reads). Ely's `ReviewComment` is decided here.
2. **B3b: answer, resolve, start.** Covers `Reply`, `Resolve`, `LineComment`
   and review-comment edits, the gutter "+" and its ranges, the composers, and
   stage `threads` (writes) plus `ui`.
3. **B3c: the review and suggestions.** Covers `Draft` in the header, the
   review actions, the strip and the discard dialog, *Suggest* and the
   suggestion diff, and stage `review` plus `ui`.

The version stays as it is: the 0.31.0 cycle already has its first `feat:`.

## §12 Out of scope

- Applying a suggested change, through the forge or locally.
- Deleting a published comment (as in B2).
- File-level and image comments in the diff; they stay in the Conversation.
- Reactions, and GitHub's "hide comment".
- Comments on a commit outside the change request's diff (a single commit's
  view).
- Live updates: threads are not polled.
- Handing a thread to an agent: **C**.

## §13 Verified in the plan, with the fallback already decided

Each is checked against the live schemas and APIs while planning, as B2c did:

- **GitHub `addPullRequestReviewThread` without a pending review.** Does it
  publish at once? Fallback: REST `POST /repos/o/n/pulls/N/comments` with
  `commit_id`, `path`, `line`, `side`, `start_line`, `start_side`.
- **GitLab range comments.** Does `createDiffNote`'s position input take a
  `lineRange`? Fallback: REST `POST merge_requests/:iid/discussions` with
  `position[line_range]`.
- **GitLab draft notes.** GraphQL or REST only? Fallback: REST `draft_notes`
  and `bulk_publish`.
- **GitLab expanded-context comments** are accepted. Fallback: the GitHub
  rule (§7.4).
- **The GitHub `reviewThreads` fields** listed in §4, and `isOutdated`
  semantics. Fallback: outdated means `line` is null.
- **GitLab outdated** detection by `diffRefs.headSha`. Fallback: a note whose
  position lines no longer exist in the current diff.
- **Ely's `ReviewComment`.** Fetched from upstream at the vendored revision
  and judged against Sirio's palette and selectable text.


## §14 Revised while planning and building B3a (2026-10-05)

### Facts verified while planning

- GitHub `PullRequestReviewThread` exposes current and original line/range
  anchors, `diffSide`/`startDiffSide` (`LEFT`/`RIGHT`), `subjectType`
  (`LINE`/`FILE`), resolution, outdated state and reply/resolve permissions.
  Its comments expose Markdown bodies, time, diff hunks and review state.
  On zed-industries/zed#17271 an outdated thread had `line: null`,
  `originalLine: 317`, `isOutdated: true`; the quoted hunk ended on that
  original line. A non-collaborator could reply but could not resolve it.
- GitLab's `Discussion` has resolution, notes, `userPermissions.resolveNote`
  and `truncatedDiffLines { oldLine newLine text }`. `DiffPosition` carries
  file paths, old/new lines and `diffRefs { baseSha headSha startSha }`.
  GraphQL has no `lineRange`: a GitLab range is read as its last line.
  Positions of type `file` or `image` stay in the Conversation.
- On open GitLab MRs, positions normally name the current head; an older
  `position.diffRefs.headSha` identifies a discussion GitLab could not move
  forward. Merged MRs often retain older position heads, so this comparison
  over-reports outdated threads there; that limitation is accepted.
- GitLab's truncated hunk text already includes `+`, `-` or space prefixes.
  A server without `truncatedDiffLines` is retried with the threads baseline
  query, without changing the client's shared header/checks baseline flag.
- Ely's upstream `git/review.rs` at the vendored revision `e17e31a6…` was
  inspected during planning. Its `ReviewComment` draws plain `SharedString`
  bodies, always includes a reply input and quotes one code line. It is not
  vendored: B3a needs selectable Markdown and has no reply input. Sirio's
  `ThreadView` follows its header/avatar/author/time/body layout using the
  vendored Ely primitives and Sirio's Markdown path.

### Reads, placement and proof

Threads are read on the tab's first load, manual refresh, retry and after a
write; entering Files starts an idle threads slot. The CI timer refreshes
header/CI state without reading threads. Pending comments remain in the
model; only published comments appear in B3a.

A current thread whose line is outside the drawn diff appears at the top of
its file. A file absent from the range keeps its thread in the Conversation.
Resolved threads fold in place; outdated threads share one folded section
under each file header. Old-side anchors and range lines remain visible in
unified and split views. Published unresolved file-level threads contribute
to `threads_open` as well as `threads_file`, but have no diff card.

`Scripts/Tests/test-forge-e2e.sh` proves the wire reads and GitLab's optional
hunk fallback against loopback forges. `Scripts/Tests/test-forge-diff-e2e.sh`
proves the thread counts, Conversation entries, reveals, unified/split
anchors, resolution folds, draft exclusion and annotation handoff to a range
rebuilt after relaunch. Its mode change opens standalone Changes, sets the
shared `DiffViewMode`, then returns to the change request. B3a's reads are
proved here; `test-forge-actions-e2e.sh` remains the existing write regression
and is where B3b will prove its new writes.

State-only proofs do not establish visual appearance. The controller fills
in the framed observations in `docs/testing/ely-change-request-tab.md`.

### Rulings from the slice ledger

- Task 1: Ruling: Step 2 names another worktree — run the same RED E2E command in the brief's assigned sdd-b3a-impl worktree — cost if wrong: validation would cover a different checkout.
- Task 2: Ruling: Step 1 lists two positional cargo test filters, which cargo rejects — use the plan's own corrected single filter a_gitlab_ — cost if wrong: an intended test may not run; confirm both names on GREEN.
- Task 2: Ruling: Step 5 requests a Side import unused by its parser — import ThreadComment only; gitlab_anchor already returns Side — cost if wrong: compilation would reveal a missing type import.
- Task 3: Ruling: Step 2 marks push_stretch mutable although the closure is never reassigned and mutates no captured state — use let push_stretch to avoid an unused_mut warning — cost if wrong: compilation would reject a future mutable closure call.
- Task 4: Ruling: DiffLine and DiffSideBySideLine carry usize line numbers, while annotation helpers take u32 — convert row numbers with u32::try_from before matching — cost if wrong: a line larger than u32::MAX would be treated as unplaced instead of truncating it.
- Task 4: Ruling: focus_anchor promises old-side reveals, but its design keeps only New lines in reveal_line; band_key_containing also ignores the annotation-split pieces — preserve side in the pending reveal and open the actual containing piece; add one regression beyond the six prescribed tests (76 + 7 = 83) — cost if wrong: direct old-side reveals could scroll to a file instead of their line.
- Task 4: Ruling: splitting context bands also changes the keys Expand All must open, which the plan does not update — share the annotation-aware band-key walk with Expand All and reveal; extend the same extra regression — cost if wrong: Expand All would leave part of an annotated band folded.
- Task 5: Ruling: ThreadView/OutdatedView::toggle tell the owner through cx.defer — annotations_for reads every card's revision, the toggling card included, and reading an entity inside its own update panics — cost if wrong: one frame's delay before the row is measured again.
- Task 5: Ruling: each card keeps its comments' Markdown docs, built when the thread arrives or changes, instead of parsing in render (the timeline's bodies do the same) — cost if wrong: a theme switch recolours card bodies only at the next change of that thread.
- Task 5: Ruling: markdown_doc and open_links stay private — a child module already reaches them through `use super::*` — cost if wrong: none.
- Task 5: Ruling: the quoted-code block uses theme.ely.sunken as its background (the plan names none) — cost if wrong: a colour.
- Task 6: Ruling: the two prescribed test names do not contain the merge_threads filter — nest them in a merge_threads test module so the required command runs both — cost if wrong: test paths only.
- Task 6: Ruling: the reveal_thread snippet drops side/key before the range exists — carry both in pending_reveal and replay focus_anchor; add a regression — cost if wrong: a pending reveal could open the wrong side or file row.
- Task 6: Ruling: reveal_thread silently ignores unknown ids but the socket must return no thread {id} — return Result<(), String> from reveal_thread and let the handler propagate it — cost if wrong: a public signature change.
- Task 6: Ruling: ChangesTab.report().annotations is in input order, but thread_rows requires drawn row order — update annotation_reports in changes.rs (one additional named-file exception) and add a unified/split regression; hidden annotations follow drawn ones — cost if wrong: existing consumers see a different report order.
- Task 6: Ruling: the reveal snippet opens an outdated section for any thread sharing its path, contradicting the file-level fallback — only drawn diff threads can target a section; extend the fold/report regression with a file-level thread on the same path — cost if wrong: a file-level reveal would open an unrelated section.
- Task 7: Ruling: surface.changes.view does not target an embedded ChangesTab — open standalone Changes, set the global Split mode, verify mode=split, return to the change request and assert the same anchors — cost if wrong: the split proof would cover the wrong surface.
- Task 7: Ruling: the GitLab outdated fixture keeps app/models/order.rb while the required section targets src/login.rs — map its filePath too while retaining its old head — cost if wrong: the section would be absent from the test diff.
- Task 7: Ruling: Review Focus 4 requires rows after reopening, but the scenario recipe never reopens — add a graceful quit/relaunch and reveal, then assert current and outdated annotations in the rebuilt range — cost if wrong: one extra launch per forge.
- Task 7: Ruling: the expected unresolved counts omit published unresolved file-level threads, although Task 6 counts all published threads — expect GitHub 5 and GitLab 4; draft-only threads remain excluded — cost if wrong: a count assertion.
- Task 7: Ruling: control_integration TempDir hard-codes /tmp on Linux despite the mandated TMPDIR; the full suite failed opening chat.sqlite there with SQLite disk I/O error — use std::env::temp_dir() on Linux, retain the short /tmp root on macOS for its socket limit; include this test-helper file in the task commit — cost if wrong: Linux callers with an unusually long TMPDIR could exceed the socket-path limit.
- Task 7: Ruling: test-forge-ui-e2e.sh still expects 5 Conversation rows, but the loaded fixture now has 5 activity entries plus 6 published threads — wait for conversation_threads=6 and rows=11; include this harness file despite the task file list — cost if wrong: a Conversation count assertion.

## §15 Revised while planning and building B3b (2026-10-05)

### Facts verified live while planning

- **GitHub mutations** (read-only introspection of github.com):
  - `addPullRequestReviewThread` is described as *"Adds a new thread to a **pending** Pull Request Review"*. Sent without a review it does not publish, so **B3b uses the spec's fallback**: REST `POST repos/{o}/{n}/pulls/{N}/comments` with `body commit_id path line side start_line start_side`, which publishes at once.
  - `AddPullRequestReviewThreadReplyInput { pullRequestReviewThreadId, body, pullRequestReviewId? }`. The review id is "the pending review to which the reply will belong", so without it the reply is published.
  - `ResolveReviewThreadInput { threadId, resolutionReason? }`. `unresolveReviewThread` takes `{ threadId }`.
  - `UpdatePullRequestReviewCommentInput { pullRequestReviewCommentId, body }`.
- **GitLab** (schema dump `~/.cache/st/gl-introspect.json`):
  - `DiffPositionInput { baseSha headSha startSha paths newLine oldLine }` has **no `lineRange`**, so a range goes through REST `POST projects/:id/merge_requests/:iid/discussions` with `position[line_range]`. Its `start`/`end` each need a `line_code` (`"<sha1 of the path>_<old>_<new>"`, in GitLab's diff counters) and a `type` (`"new"` for an added line, otherwise `"old"`).
  - One path for both, ruled here: **every** GitLab line comment goes through that REST call, a single line just without `line_range`. An unchanged line names both `old_line` and `new_line`.
  - `CreateNoteInput` has `discussionId` (a reply), and `DiscussionToggleResolveInput { id, resolve }`.
- **GitLab's diff counters:** a line absent on one side takes the number the next line on that side would have. An added line's old counter is the next base line; `@@ -0,0 +1,3 @@` gives `0`. `sha1("src/login.rs")` is `ba9dbdbb9be33e57e33422db0cc822780b0701cc`.
- **`sha1_smol` 1.0.1** is already in `rust/Cargo.lock` (transitively). Adding it to `sirio_forge` adds one dependency edge.
- **Test infrastructure:**
  - `CannedForge` (`sirio_ui/src/forge_source.rs`, `testing`) answers GraphQL by `operationName` only, and every REST call 404s (Task 6 extends it).
  - `fake_forge.py` routes no REST thread write and logs no REST body (Tasks 2 and 3 extend it).

### Plan revisions, each with its reason

- (a) GitHub line comments use REST, because `addPullRequestReviewThread` only adds to a pending review.
- (b) Every GitLab line comment uses REST `discussions`, because GraphQL has no line range.
- (c) `commentable` lives in `sirio_ui::diff_annotations`, not `sirio_forge::mapping`: `sirio_forge` has no hunks.
- (d) The GitHub rule applies to GitLab too, because expanded context was not confirmed live.
- (e) A renamed file's GitLab position names the head path on both sides, so GitLab may refuse it and the reason shows.
- (f) The socket's compose syntax is `PATH:SIDE:FIRST[-LAST]`.
- (g) The UI proof lives in `test-forge-diff-e2e.sh`, because the composer needs a real diff.

### Rulings from the slice ledger

- Task 1: Ruling (pre-flight): `UpdatePullRequestReviewComment` missing from the github live probes — covered by Task 2 Step 4, which appends that probe; checked at Task 2's review — cost if wrong: one live probe missing.
- Task 2: Ruling (implementer): `--stage ci` run with `--state-only` (headless) — cost if wrong: ci captures unverified here.
- Task 2: Ruling (review): the brief's "act names" meant the parser arms, which exist and are E2E-proven; the stale `forge_probe.rs` module doc is a Minor for the final fix wave — cost if wrong: a doc line.
- Task 3: no new ruling; review clean (minors deferred to the final fix wave).
- Task 4: Ruling (review, plan-mandated): `anchor_in` checks only the range's endpoints; with 24 context lines one drawn hunk can hold two changes whose ±3 windows don't chain, so a range spanning the gap is accepted though GitHub refuses it (spec §7.2/§8: a range crossing hunks is refused) — fix: every index `first..=last` must be commentable, plus a test with two changes ~20 lines apart — cost if wrong: one guard too strict for GitLab ranges across far context.
- Task 5: Ruling: the helper snippet uses `theme.ely.accent_fg`, which does not exist in Ely's palette — use the existing paired foreground `theme.ely.on_accent` with `theme.ely.accent` — cost if wrong: the plus control's foreground contrast/color.
- Task 5: Ruling: adding the two public `ChangesTabEvent` variants makes three current exhaustive listeners fail compilation — add no-op arms in `change_request_tab.rs`, `right_panel/mod.rs`, and `sirio/src/main.rs` until Task 6 wires comment handling — cost if wrong: those listeners can drop comment events before that wiring lands.
- Task 5: Ruling (review, plan-mandated): a gutter press released outside a plus survives; after a previous successful pick, completing a Shift-click emits a second range from the stale press — clear cancelled gesture state and clear it on shift-extension, preserving plus-to-plus drags; add a real mouse RED→GREEN regression — cost if wrong: a valid drag could be cancelled before its endpoint handler.
- Task 6: Ruling: Step 3's expected diagnostic list is stale and incomplete — followed the required order by adding `answer_rest` in Step 1, then observed RED for the missing composer methods, including `cancel_composer` — cost if wrong: the expected diagnostic list differs from the actual compile output.
- Task 6: Ruling: the supplied composer card ID uses an integer literal inferred as `i32`, but `gpui::ElementId` requires `usize` — used `0usize` — cost if wrong: one ID literal would need changing.
- Task 6: Ruling: the supplied `composing()` helper waits for the range and permission but not the per-file diff, while `comment_anchor` requires that diff — focused `a.txt` and waited for its anchor before testing compose behavior — cost if wrong: the test setup does extra focus work; `compose_at` still requires the target diff to be loaded, as the supplied implementation does.
- Task 6: Ruling (review, plan-mandated): synchronous perform refusals are dropped by composer UI handlers — show the refusal beneath the submitting field, retain text and preserve an existing Working action/target — spec composer error and one-write constraints — cost if wrong: an error-state transition could accidentally unlock a real write.
- Task 6: Ruling (review, plan-mandated): retained write_target misroutes a later direct header action failure beneath the composer — associate/reset ownership whenever an action is accepted and cover rejected line-comment followed by failed Close — cost if wrong: a real composer error could be cleared by an unrelated action.
- Task 6: Ruling (review, plan assumption): ordinary failed writes do not re-read header/threads — re-read after completed failed attempts as the global constraint requires, honoring existing rate-limit guards and preserving local draft errors — cost if wrong: unnecessary reads after a rejected write.
- Task 6: Ruling (fix round 1): the handler design discarded synchronous perform errors and displayed errors only through `ActionState::Failed` — keep refused field writes separately by `WriteTarget` and prefer that feedback in the submitting composer while leaving any Working/Failed action state and its task/target untouched — cost if wrong: transient refusal text could outlive the in-flight action; it is cleared when that action completes, on a subsequent accepted action, on cancel, or when the line comment succeeds.
- Task 6: Ruling (fix round 1): the target-only routing lets accepted direct actions inherit a stale composer owner — clear ownership whenever perform accepts an action, then let `start_write` restore a field owner for field-originated writes — cost if wrong: any new field write must continue to use `start_write` to associate its target.
- Task 6: Ruling (fix round 1): the brief says ordinary `finish_action` already re-reads, but its ordinary error branch did not — set the failure first, then use the guarded header/thread reread for ordinary failed attempts while retaining the existing HeadMoved refresh — cost if wrong: failed attempts cause additional forge reads, except while the existing rate-limit guard is active; the binding global constraint requires those reads.
- Task 7: Ruling (implementer out of quota before commit): controller committed the agent's tree unchanged as `a9622641` after re-running focused 65/65 + workspace build — cost if wrong: none.
- Task 8: Ruling (pre-flight): `parse_args` retains booleans in `ParsedArgs.flags` and exposes `flag()` — use `parsed.flag("cancel")` instead of a raw-args scan as the stale brief suggests — cost if wrong: boolean parsing would require an adjustment; concrete existing callers are to be checked by the implementer.
- Task 8: Ruling (pre-flight): the brief's unconditional `people.rs` `Failed` assignment can replace a real `Working` action — preserve Working state/task/target for busy refusals and use the existing field refusal routing, exposing other synchronous refusals without stale ownership — cost if wrong: a refused second request keeps the first action's reporting state until completion, while its direct socket response and submitting field provide the refusal reason.
- Task 8: five implementer rulings (in `task-8-report.md`, reviewer judged all correct): context mandates `parsed.flag("cancel")` over raw-arg scanning — cost if wrong: `--cancel` silently ignored; the unconditional `Failed` assignment would overwrite a real `Working` action — record only when not busy — cost if wrong: E2E reads a misattributed failure or a clobbered in-flight write; `compose_at`/`cancel_composer` were `pub(crate)` but called from the `sirio` crate — widened to `pub` — cost if wrong: the handler cannot compile; the verbatim arms had no refusal recording — wrapped each arm through `record_refusal` — cost if wrong: socket refusals invisible to `action_message`; the `send_*`-level regression did not cover the `control_act` socket boundary — added `windowed_thread` + two socket-level regressions — cost if wrong: a future unconditional-`Failed` edit passes all tests while breaking the E2E contract.
- Task 8: Ruling (review Important): `record_refusal`'s idle branch sets `Failed` but leaves the last write's stale `write_target`, so `line_composer_error` / a thread card shows an unrelated refusal — fix: idle refusal clears `write_target`, then `sync_writes` + notify; add assertion to the socket test — cost if wrong: none (a refusal owns no in-flight write; field refusals ride `write_refusal`).
- Task 9: Ruling: the brief's `scenario_thread_writes` sketch names `FORGE_LOG` and omits the socket read suffixes, `SCENARIO`/`WT` set-up and three-arg `make_worktree` — used the script's real `"$RUN_DIR/$SCENARIO-forge-requests.log"`, `wait_for`/`assert_contains` with `surface change-request read`, `SCENARIO="$flavour-thread-writes"` with `WT="$RUN_DIR/worktree-$SCENARIO"`, and `make_worktree "$WT" "$remote" "$BARE"` copied from `scenario_threads` — cost if wrong: the scenario would read the wrong log or never match a field.
