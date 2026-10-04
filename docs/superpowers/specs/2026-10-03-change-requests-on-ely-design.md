# Change requests on Ely, and merge with metadata (B2b) — design

**Date:** 2026-10-03
**Status:** proposed
**Programme:** the change-request surfaces move from hand-drawn GPUI and bezel
widgets to Ely GPUI Components, and slice **B2b** of
`2026-09-29-change-request-actions-design.md` (merge, reviewers, labels) is
built on Ely. Five deliveries (§3), each with its own plan and pull request.
**Builds on:** A (`2026-09-27-change-requests-design.md`), B1
(`2026-09-28-change-request-diff-design.md`), B2
(`2026-09-29-change-request-actions-design.md`, slice B2a merged as #592).
**Related:** the bezel → Ely migration (SP1 merged as #600,
`2026-10-01-sirio-theme-on-ely-palette-design.md`); this programme is a
vertical slice of it (§10).

## §0 Intent

The user asked for the change-request features not yet implemented, starting
with B2b — the merge strip, its confirmation, auto-merge, *Delete branch*,
the reviewer and label pickers — and for those surfaces to be **Ely only, no
bezel**. Asked how far "Ely only" reaches, they chose the whole of it: the
change-request list in the right panel, the detail tab, and `ChangesTab`,
which the tab's *Files* embeds and which the commit view and the local
changes view share.

Success:

- On a GitHub pull request and a GitLab merge request, from Sirio alone, a
  change request is merged — with the method the repository allows, its
  commit message edited, its branch deleted, or set to merge when its checks
  pass and then cancelled — and its reviewers and labels are changed. B2's
  success sentence (§0 there) holds for everything but CI, which is B2c.
- The change-request list, the detail tab and `ChangesTab` draw only Ely
  components and Ely icons, with the exceptions §2 names, and behave exactly
  as before: every existing end-to-end script passes with its steps
  unchanged.
- Every delivery ends with window captures under Xvfb, so for the first time
  these surfaces are **seen**, not only read through report keys (A, B1 and
  B2a recorded "never seen on screen").

## §1 Decisions taken

Settled with the user before this document was written:

| Question | Decision |
|---|---|
| Next step in the programme | **B2b**, before B2c, B3 and C. |
| Library | **Ely only** for every new and migrated surface. No bezel widget. |
| Reach | The right panel's *Change requests* view, the detail tab (A, B1, B2a surfaces included) and `ChangesTab`. |
| `ChangesTab` | **Redraw on Ely, same engine** (§8): git's hunks, lazy diffs, the virtualised list, bezel-syntax colours and the honesty contract stay. Not Ely's `DiffViewer`. |
| Delivery | **Five deliveries**: 1 foundations → 2 detail tab → 3 B2b; 4 list and 5 `ChangesTab` independent of 2 and 3 (§3). |
| B2's forge side | Unchanged from B2 §4–§6, §9, §10, §13. This document replaces only B2's interface choices (§7.1 there, and §14.1 item 2). |

## §2 What stays off Ely, and why

- **Markdown** stays on bezel-markdown, through
  `Chat::render_markdown_document_with_link_override` → `SelectableMarkdown`.
  Ely's `MarkdownRenderer` is not selectable; the migration programme kept
  bezel-markdown for that reason.
- **Code colour** stays on bezel-syntax (`Theme::syntax_palette`). Ely has no
  tree-sitter; its `code_colors` is lexical and knows no language.
- **Text selection** stays Sirio's own `selectable_text`. Ely has no
  selectable text; `selectable_text` is not bezel.
- **`Theme::to_bezel_theme`** stays: Markdown and syntax still read bezel's
  tokens.
- **Shared helpers** other surfaces use — `controls::sidebar_text_field`,
  `controls::sidebar_tooltip`, `modal.rs` — stay; these surfaces stop
  calling them.

## §3 Deliveries

| # | Delivery | Depends on | Visible change |
|---|---|---|---|
| 1 | Foundations: vendor the missing Ely modules (§4) | — | none |
| 2 | The detail tab on Ely (§5) | 1 | the tab |
| 3 | B2b on Ely (§6) | 2 | merge strip, dialog, pickers |
| 4 | The list on Ely (§7) | 1 | the right panel's view |
| 5 | `ChangesTab` on Ely (§8) | 1 | every diff in the app |

Each delivery leaves Sirio working and is merged on its own. 4 and 5 may run
before, after or beside 2 and 3. A leftover of A or B1 goes with the delivery
that already opens its file (§9).

## §4 Delivery 1 — foundations

Ely is vendored at upstream revision `e17e31a6` with only the closure the
chat needs (`rust/vendor/ely-gpui-component/LOCAL-CHANGES.md`). Ely is
already initialised app-wide — `sirio_ui::ely::init` calls `init_chat`,
which installs Ely's theme and its form key bindings — so a surface outside
the chat can use any vendored component today. Already vendored and enough
for most of this programme: `Button`, `IconButton`, `SplitButton`,
`ContextMenu`, `Menu`/`MenuItem`, `Popover`, `Dialog`, `ConfirmDialog`,
`Select`, `ListBox`, `Checkbox`, `TextInput`, `Input`, `PasswordInput`,
`Badge`, `CountBadge`, `Tag`, `Avatar`, `AvatarGroup`, `Timeline`,
`Callout`, `InlineMessage`, `Skeleton`, `Tooltip`, and the git icons
(`GitPullRequest`, `GitPullRequestDraft`, `GitPullRequestClosed`, `GitMerge`,
`GitBranch`, `GitCommitHorizontal`).

Vendored by this delivery, from the same revision:

- `navigation/tabs.rs` — `Tabs`, the strip with its panel; and whatever of
  its imports (`forms::{Pick, step}`, `motion::{glide, measure_item, …}`) the
  current closure lacks.
- `git/badges.rs` — `GitStatusBadge`, `DiffStat`. Its `lists::GitStatus`
  comes in as a type of `git/` on its own, not with the `lists/` catalogue.

Not vendored: `git/viewer.rs` (`DiffViewer`, §1), `git/review.rs`
(`PullRequestCard`, `ReviewComment` — B3 decides on the latter), the rest of
`git/` and `lists/`. Each file brought in, and each local change to it, is
recorded in `LOCAL-CHANGES.md`.

**Proof.** `sirio_ui/examples/ely_chat_probe.rs` is the existing native
probe; a new `sirio_ui/examples/ely_forge_probe.rs` beside it draws `Tabs`,
`GitStatusBadge` and `DiffStat` with Sirio's theme under Xvfb and saves the
frames. Nothing in the app changes.

## §5 Delivery 2 — the detail tab

**Drawing changes, behaviour does not.** The tab's state (`Slot`,
`RangeState`, `ActionState`, the one-write-in-flight rule of B2 §14.1),
`perform`, the keys of `report()` and every `debug_selector` an end-to-end
script reads stay. `test-forge-ui-e2e.sh`, `test-forge-diff-e2e.sh` and
`test-forge-actions-e2e.sh` pass with their steps unchanged.

| Part | Today | On Ely |
|---|---|---|
| Title and number | hand-drawn, `selectable_text` | the same text, Ely's type scale; still selectable |
| State | hand-drawn badge | `Badge` with the state's git icon and a `Tone` |
| Open in browser, Refresh | hand-drawn buttons | `IconButton` with tooltip |
| B2a's actions | labelled buttons (B2 §14.1 item 2) | `IconButton` with tooltip, as B2 §7.1 first asked: `GitPullRequestClosed` / reopen, `GitPullRequestDraft` / ready, `Pencil` for *Edit* |
| Reviewers | text | `AvatarGroup` / `Avatar`, the review outcome as tone |
| Inner tabs | hand-drawn strip | `Tabs`; counts as `CountBadge` |
| Conversation | hand-drawn rows | `Timeline` / `TimelineItem`; title, time and body are elements, so `selectable_text` and the selectable Markdown go inside unchanged |
| Composer | bezel `TextField`, three buttons | `TextInput::multi_line(3, 12)` and a `SplitButton`: *Comment*, with *Approve* and *Request changes* in its menu (B2 §7.1). Each menu row exists only when its capability is true; with neither, a plain primary `Button` *Comment*. Cmd/Ctrl+Enter still sends |
| Edit card, comment editor | bezel `TextField` | `TextInput`; *Save* a primary `Button` with `.loading()` while in flight |
| Action status, errors | hand-drawn line | `InlineMessage` / `Callout` with a `Severity` |
| Commits, Checks | hand-drawn rows | rows composed of Ely primitives (`Icon`, `Badge`, `Ellipsis`); no list component is vendored for them |
| *Files* frame | hand-drawn snapshot bar and failure surface | Ely surface; the failure as a `Callout` with *Retry*. The rows are delivery 5 |

B2 §14.1 item 2 — labelled buttons and three composer buttons, because "the
vendored Zed icon set has no pencil or draft glyph and bezel has no menu" —
is **superseded**: Ely has both. Nobody restores the three buttons believing
they follow §14.

**Proof.** The three scripts above, unchanged, and their window captures
(`--out-dir`, without `--state-only`) for GitHub and GitLab under Xvfb — the
first frames of the tab ever captured.

## §6 Delivery 3 — B2b

### Forge

As B2 §4–§6 and §13 say, with B2a's lessons (B2 §14.1):

- `Capabilities` gains `can_edit_reviewers`, `can_edit_labels` and
  `merge: MergeCapability { verdict, methods, default_method,
  can_auto_merge, auto_merge_enabled, delete_branch_default }`;
  `MergeVerdict` is `Ready | WaitingOnChecks | Blocked(BlockReason)`, an
  unknown `mergeStateStatus` / `detailedMergeStatus` is
  `Blocked(Other(<the forge's word>))`.
- `ChangeHeader` gains `labels`; a `Reviewer` gains the id its forge's
  mutation needs.
- `Action` gains `Merge { method, commit_title, commit_message,
  delete_branch, when_checks_pass, expected_head }`, `CancelAutoMerge`,
  `SetReviewers { add, remove }`, `SetLabels { add, remove }`, each with a
  content-free `kind()`.
- The pre-flight of `act` (B2 §14.1 item 6) extends to the merge: it reads
  the head afresh and refuses with `HeadMoved` before anything is sent; the
  forge's own `expectedHeadOid` / `sha` guard stays behind it.
- *Delete branch*: a flag of GitLab's accept; on GitHub a `deleteRef` after a
  successful merge, only for a head in the same repository; if only that
  call fails the outcome is `Done` with the warning "merged; deleting the
  branch failed: …".
- Candidate reads `reviewer_candidates(number, text)` and
  `label_candidates(text)`, each candidate carrying its mutation's id.
- §13 of B2 holds: the live test sends every new document with ids that
  name nothing (B2 §14.1 item 9); where a GitLab mutation is missing, the
  capability reads false and `act` answers `Unsupported`.

### Interface

```
| feat(ui): PR view in the right panel #578     (o) [x] [~] [/] [↗] [⟳] |
| (Open) epalmisano · feat/pr-view → main · updated 2h ago             |
| Reviewers: (b)(c) [+]     Labels: [ui] [+]                           |
+----------------------------------------------------------------------+
| ✓ Ready to merge                       [ Squash ▾ ]  [ Merge ]       |
+----------------------------------------------------------------------+
| Conversation 3 | Commits 5 | Checks 3/7 | Files 12                   |
```

- **The merge strip** sits between the header and the `Tabs`, so it shows on
  every inner tab. Left, the verdict as an `InlineMessage` (*Ready to
  merge*; *Blocked: 1 review required*, naming the reason). Right, a
  `Select` over `methods` only, and a primary `Button` *Merge* — *Merge when
  checks pass* with `WaitingOnChecks` and `can_auto_merge`. Blocked, the
  button is disabled and the strip stays. With auto-merge enabled the strip
  reads *Auto-merge enabled · <method>* and offers *Cancel auto-merge*. A
  merged or closed change request has no strip.
- **The confirmation** is an Ely `Dialog`, the only confirmation of a write
  in the app (B2 §1): title *Merge #N*; detail the target branch and the
  head sha being merged; a `TextInput` for the commit title and a
  `multi_line` one for the message (merge and squash only); a `Checkbox`
  *Delete branch* preset from `delete_branch_default`; *Cancel*, and *Merge*
  as the primary action with `.loading()` while in flight. Escape and the
  scrim close it; focus returns to the strip. *Merge* in the dialog is the
  send; a `HeadMoved` closes it, shows "The branch changed since you opened
  this" and reloads.
- **Reviewers and labels** under the meta line: reviewers as an
  `AvatarGroup`, labels as `Tag`s, each followed by an `IconButton` `Plus`
  when its capability is true. It opens a `Popover` holding a `TextInput`
  observed with the list's 300 ms debounce, which asks the forge for
  candidates (not a local filter: the first page of an organisation's
  assignable users may not hold the one sought), and a `ListBox::multiple()`
  of them with the current set selected. **Closing the popover sends one
  `SetReviewers` or `SetLabels` carrying the difference**, and nothing when
  there is none. GitHub's `requestReviews(union: false)` replaces the whole
  set: one send per click would let a second click read a stale set.
- Every write behaves as B2a's do: disabled while in flight, the forge is the
  truth after it, a failure keeps what was typed and shows its message and
  remedy (B2 §9).

### Proof

- `test-forge-actions-e2e.sh` gains stages `merge` and `metadata`, GitHub and
  GitLab, on B2 §10's shape: each mutation on the wire with the fixture's
  head sha; a blocked change request sends nothing; a moved head gives
  `HeadMoved`; auto-merge enabled then cancelled; a fixture failing only
  `deleteRef` gives the merged result with its warning; several picks in a
  picker log one `Set*`; a 403 names the scope. `fake_forge.py` learns the
  mutations and `<Operation>.after.<Mutation>.json` fixtures for them. The
  `cli` stage runs a merge through the real `gh` (and `glab`, §9).
- Window captures of the strip in each verdict, the dialog and an open
  picker.
- `forge_live.rs` sends the new documents.
- Unit tests, every failure written before the code: the mapping of
  `mergeStateStatus` / `detailedMergeStatus` and the repository's allowed
  methods onto `MergeCapability` — unknown values, missing fields, GitLab's
  baseline answer, a repository allowing no method Sirio knows.

## §7 Delivery 4 — the list

Scope: the content of the right panel's *Change requests* view. The panel's
view switcher and its Files, History and References views stay as they are.

| Part | Today | On Ely |
|---|---|---|
| Filters | hand-drawn strip, label on the active one only, underline | `Tabs` with icon and tooltip — the same component as the detail tab's inner tabs; the to-review count as `CountBadge` |
| Search | hand-drawn icon + `sidebar_text_field` | `IconButton` + a clearable `Input` over `TextInput`, same debounce |
| Row | hand-drawn | a compact row (#599) of Ely primitives: the state's git icon in its `Tone`, the title with `Ellipsis`, the meta line, CI and review as icons. Not `PullRequestCard`, which is a card and would undo the sidebar's density |
| Row menu | bezel `popover` + `motion::Fade` | `ContextMenu` with *Open in browser*, *Copy link* |
| Branch card, *Create on the forge* | hand-drawn | Ely surface + `Button` |
| Unknown host, not connected, rate-limited | hand-drawn text | `Callout` with a `Severity` + `Button` |
| Token field | `sidebar_text_field` — the token shows in clear | `PasswordInput`; *Checking the token…* / its failure as `InlineMessage` |
| Loading | text | `Skeleton` rows |

Icons come from Ely's `IconName`, not from `crate::sidebar::icons`.

**Proof.** `test-forge-ui-e2e.sh` unchanged — the selectors
`change-request-row-N`, `change-requests-filter-*`, `change-request-menu*`
and the report keys are kept — and captures of the list with rows, empty,
not connected, unknown host and rate-limited. The two leftovers of §9 that
belong here add their steps.

## §8 Delivery 5 — `ChangesTab`

**The engine stays**: the sources (`WorkingTree`, `Commit`, `Range`), git's
hunks through `sirio_git`, lazy per-file diffs and their 10 MiB cap, gpui's
virtualised `list`, the context bands, the unified and split layouts that
already exist, bezel-syntax colours, the honesty contract at the top of
`changes.rs`, the keys of `surface.changes.read`, and `perf_baseline`.

| Part | Today | On Ely |
|---|---|---|
| Toolbar | hand-drawn buttons, `bezel::ui::tooltip` | `IconButton`s with tooltip; unified/split as a single-choice `ToggleGroup` (already vendored; upstream's `SegmentedControl` is not brought in) |
| Section header | hand-drawn on `bezel::theme::ink` | an Ely row with a `CountBadge` |
| File row | status letter via `git_status_style`, `+N −M` as text | `GitStatusBadge` and `DiffStat` |
| File icon | `bezel_icons::DOCUMENT` | an Ely `IconName` |
| Diff lines | hand-drawn washes | line and changed-word washes from Ely's palette, as Ely's `DiffViewer` draws them; code colour from bezel-syntax (§2) |
| "N hidden lines" band | hand-drawn | an Ely row with an `IconButton` to expand |
| Git error, diff unavailable | text + Retry | `Callout` with `Severity::Error` + *Retry* `Button` |
| Loading | generic loader | `Skeleton` |
| Discard confirmation | `window.prompt` — the system's own dialog | `ConfirmDialog::destructive()`, inside the window |

The Discard confirmation is the one change of behaviour in this delivery: on
macOS a system sheet becomes an in-window dialog, which an end-to-end run can
now drive and capture.

Two features B1 deferred land here, because the components now exist:

- **Right-click on a file row** (B1 §7.1): a `ContextMenu` with *Copy path*
  for every source, and *Open on the forge* for a `Range`.
- **A `Range`'s section is headed *Changes (N)***, not *Staged (N)* (B1
  §13, *Not built*); the host's reader of `surface.changes.read` changes with
  it in the same commit.

**Proof.**

- `changes.rs`'s tests stay green, the drawn ones included
  (`drawn_changes_rows_expand_sections_and_context_bands`).
- `perf_baseline`'s counts do not rise. They count builds, flattened rows and
  payload copies, not time, so a per-row allocation added by the redraw shows
  deterministically; no wall-clock test is added.
- `test-forge-diff-e2e.sh` unchanged.
- Captures of the three sources — local changes, a commit, a change
  request's *Files* — unified and split, the error state and the Discard
  dialog.

## §9 Leftovers of A and B1, by delivery

| Leftover | From | Delivery |
|---|---|---|
| `set_filter` while rate-limited clears the rows | A | 4 |
| `RateLimited` without a reset time does not pause | A | 4 |
| The tab's CI timer is not rescheduled after a rate limit | A | 2 |
| Lock glyph (`IconName::Lock`) on a read-only snapshot tab | B1 §7.2 | 5 |
| A deleted file's snapshot titled *deleted in #N*; `PersistedSnapshot` gains `deleted` (`#[serde(default)]`), so a restored one no longer offers *Open local copy* for a same-named local file | B1 §5.3 | 5 |

The three of A are genuine gaps in behaviour coverage: each gets an
end-to-end step (the fake forge already answers with a rate limit). The two of
B1 show in the captures and the report keys.

`glab` is not installed on the machine B2a was built on, so the `cli` stage's
GitLab half printed `SKIP:`. Installing it is a precondition the plan states,
not a design choice; absent, the stage keeps printing `SKIP:`, never a false
pass.

## §10 The bezel → Ely migration

- This programme takes out of SP2–SP7's scope: the *Change requests* view,
  the detail tab, `ChangesTab` and the snapshot tab's title and glyph. Its
  memory and SP2's brainstorm start from that.
- Left to SP2–SP7: `sidebar_text_field`, `sidebar_tooltip`, the right
  panel's view switcher and other views, Settings, the host's `modal.rs`
  sheets, and everything else bezel still draws.
- `CLAUDE.md` changes with delivery 2: *External references* stops saying
  bezel builds every non-chat surface, and names these; the *Change
  requests* section gains a paragraph on B2b's merge and pickers with
  delivery 3.

## §11 Out of scope

- **B2c** — CI re-run and the log tab — and **B3** — inline review threads,
  where Ely's `ReviewComment` is weighed — each from its own brainstorm.
  **C** — a change request into a worktree and an agent.
- Ely's `DiffViewer`, `PullRequestCard`, `ChangesList`, `CommitList`.
- Everything B2 §12 leaves out: deleting a comment, admin merge past a
  block, creating a change request through the API, assignees, milestones,
  reactions, a write verb in a release build's control socket.
- Any change to the right panel outside its *Change requests* view.

## §12 Verified in the plan, with the fallback already decided

| Point | Fallback if the check fails |
|---|---|
| `Tabs` and `git/badges.rs` vendor with a small closure | bring in the missing helpers file by file, each recorded in `LOCAL-CHANGES.md`; never a whole upstream module for one type |
| Ely's `Timeline` takes a selectable Markdown body without breaking selection | the body is drawn beside the rail, outside `TimelineItem`, with the rail kept |
| `SplitButton`'s menu can hide a row per capability | build its `Menu` per render with only the allowed rows |
| `Dialog` composes with the tab's focus and the text-selection sink (`root_focus`) | the dialog's fields own focus while it is open; Ctrl+C in it copies the field's selection, never swallowed (CLAUDE.md, *Text you can select and copy*) |
| `ListBox::multiple()` reports the whole selection on change | the popover keeps its own set and diffs it on close |
| A gpui `list` row built from Ely components keeps `perf_baseline`'s counts | the row keeps Ely's tokens and icons but draws its badge inline, without the component |
| `ConfirmDialog` can be driven over the control socket for the Discard capture | a debug-only `surface.changes.confirm` verb, guarded as B2 §14.1 item 8 guards its write verb |
| Xvfb renders Ely's components (SP1's sweep did) | captures run where a display exists, and the delivery says which frames were not taken |
