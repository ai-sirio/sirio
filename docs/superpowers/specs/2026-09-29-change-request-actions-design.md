# Acting on a change request — design

**Date:** 2026-09-29
**Status:** proposed
**Programme:** part **B2** of **B1 → B2 → B3 → C**. A reads the forge, B1
brought the diff into Sirio, **B2 (this document)** lets the user act on a
change request from its tab, B3 draws inline review threads in the diff, C
takes a change request into a worktree and hands it to an agent.
**Scope of this document:** every write the detail tab offers, on GitHub and
GitLab — comment, review (approve, request changes), close and reopen, draft
↔ ready, edit, merge, reviewers, labels, CI re-run — and reading a CI job's
log inside Sirio. Built as three slices (§11), each with its own plan and
pull request.

## §0 Intent

The user runs agents in worktrees. After A and B1 they can see where a change
request stands and read its diff without leaving Sirio; to *do* anything about
it — answer a reviewer, approve, fix the description, re-run a red job after
reading why it failed, merge — they still go to the browser.

Success: on a GitHub pull request and on a GitLab merge request, from Sirio
alone, a change request goes from open to merged — commented on, reviewed,
its metadata fixed, a failed job read and re-run, then merged — over whichever
means the user chose in Settings (the forge's CLI or a token), and Sirio never
offers a write the forge says the signed-in user may not make.

## §1 Decisions taken

Settled with the user before this document was written:

| Question | Decision |
|---|---|
| Scope | The user's list: comment, approve / request changes, merge strategies, close / reopen, draft ↔ ready, edit, reviewers and labels, re-run CI, CI logs. |
| Delivery | **One spec, three slices in order** — B2a conversation, B2b merge and metadata, B2c CI (§11). Each slice has its own plan and pull request. |
| Confirmation | **Merge only.** Every other action acts on click: a comment and a review have their own submit button, close/reopen and draft/ready are undone from the forge. |
| CI logs | A **read-only tab** in the Secondary half (§7.4). |
| Write path | **One door** — `Action` and `ForgeClient::act` in `sirio_forge` (§4). Not one public method per action, not a host-mediated `perform`. |
| Writes on the control socket | **Debug builds only** (§10). A release binary has no verb that writes to a forge. |
| State after a write | **The forge is the truth**: the header is re-read, nothing is updated optimistically (§5). |
| Deleting a comment | **Out.** It is irreversible, and only merge asks for confirmation. |
| *Edit* | The change request's title, description and target branch, and the text of the user's own comments. |

## §2 Where it stops today

- `Transport` has **one method**, `post_graphql` (`sirio_forge/src/transport.rs`).
  `CliTransport` runs `<cli> api --hostname H --include --method POST graphql
  --input -`; `TokenTransport` posts to the GraphQL endpoint with
  `Authorization: Bearer`. Nothing can call a REST path.
- `ForgeClient` (`client.rs`) is read-only: `list`, `to_review_count`,
  `for_branch`, `header`, `commits`, `checks`, `files`, `creation_url`, and a
  cached `viewer`.
- `ChangeHeader` carries **no permissions**, no labels, no allowed merge
  methods and no mergeability. A timeline comment has no id. `Check` is name,
  status, group, duration and URL — **no job or run id**, which a log and a
  re-run both need.
- `ChangeRequestSource` (`sirio_ui/src/forge_source.rs`) already hands the UI a
  `ForgeClient` through `client_for`; the host owns the means and the token.
  Writing needs nothing new from it.
- Settings → Git hosting names the **read** scopes as the minimum
  (`read_api` on GitLab; read access to pull requests, checks and metadata on
  GitHub — `change_request_style::token_scopes`). A token saved today is
  refused on its first write.
- `ForgeError` (`error.rs`) has nine variants, none of which says "the forge
  understood and refused, for this reason".
- The control socket's `surface.change_request*` verbs (`sirio_control`,
  dispatched from `sirio/src/main.rs`) are compiled into **every** build; A's
  and B1's end-to-end scripts drive the UI through them, and
  `surface.change_requests.token` stores a token. All of them read or navigate;
  none writes to a forge.
- The UI already has what writing needs: `bezel::ui::input::TextField` (the
  chat composer's multi-line field, `Shape::Grow { min: 3, max: 12 }`),
  `modal::render_modal` with `ModalSpec`, `selectable_text`, and B1's read-only
  file tab. **No function turns ANSI colour codes into styled text** outside
  the terminal pane, which draws through `libghostty-vt`.
- `change_request_tab.rs` is 2,700 lines with its tests. New surfaces go in
  sibling modules (§4), not into it.

## §3 Vocabulary

**Action** — one write the user asks of a forge about a change request, as the
`Action` enum names it. **Capabilities** — what the forge says the signed-in
user may do to *this* change request *now*, read with the header (§6). Both
join `CONTEXT.md` under *Forges*, and its *Forge* entry stops saying Sirio only
*reads* change requests. **Slice** — B2a, B2b or B2c.

## §4 Architecture

### `sirio_forge` — no new crate

- `action` — `Action`, `ActionOutcome` and `ForgeClient::act(&self, number,
  &Action) -> Result<ActionOutcome, ForgeError>`, **the one public door for a
  write.** Everything cross-cutting happens in it once: the capability check
  before anything goes on the wire, the `sirio_perf` span (a content-free
  `&'static str`, never the body), the rate-limit stop of A §9, the error
  classification. `github::act` and `gitlab::act` hold each forge's builders,
  pure functions from an `Action` to the request; a builder that cannot
  express an action returns `ForgeError::Unsupported`.
- `Action`:

  | Variant | Carries |
  |---|---|
  | `Comment` | `body` |
  | `Review` | `verdict` (`Approve`, `RequestChanges`, `Comment`), `body` |
  | `Close`, `Reopen`, `MarkReady`, `ConvertToDraft` | — |
  | `Edit` | `title`, `body`, `target_branch`, each `Option` — only what changed |
  | `EditComment` | the comment's id, `body` |
  | `Merge` | `method`, `commit_title`, `commit_message`, `delete_branch`, `when_checks_pass`, and `expected_head` — the sha the user saw |
  | `CancelAutoMerge` | — |
  | `SetReviewers`, `SetLabels` | what to add, what to remove |
  | `Rerun` | `FailedInRun(run)` or `Job(job)` |

  `ActionOutcome` is `Done`, or `Done` with a warning the UI shows beside the
  result (§5, *delete branch*).
- `Transport` gains `request(&self, RestRequest) -> Result<ApiResponse,
  ForgeError>`, `RestRequest { method, path, body, follow_redirects }`. The
  token means resolves `path` against `https://api.github.com/`,
  `https://H/api/v3/` or `https://H/api/v4/`; the CLI means runs
  `<cli> api --hostname H --include --method M <path>`. Responses go through
  the same interpretation as GraphQL's, so the errors of A §10 hold unchanged.
  It carries the log request, the GitHub re-run and any GitLab mutation an
  older server lacks in GraphQL (§13). As A's overrides do, the loopback
  base for tests is honoured **only in debug builds**. `follow_redirects:
  false` is what the log request uses: GitHub answers it with a redirect to a
  signed URL on another host, which `job_log` then fetches itself with a plain
  GET carrying **no credential**, over https only in a release build — never
  through a `Transport`, whose requests are bound to the forge's host and
  its token.
- `Capabilities` — a field of `ChangeHeader`, mapped in `mapping.rs` (§6). The
  header also gains `labels`, and each timeline comment an id and whether the
  user may edit it.
- Candidate reads, for the pickers: `reviewer_candidates(number, text)` and
  `label_candidates(text)`. Each candidate carries whatever id its forge's
  mutation needs.
- `Check` gains `job: Option<CheckJob>` — `{ job_id, run_id }` — present only
  for a GitHub Actions job and a GitLab CI job (§7.3). `job_log(&CheckJob)`
  returns `Log { bytes, truncated, complete }` through `Transport::request`.
- `ForgeError` gains `Rejected`, `HeadMoved` and `Unsupported` (§9).

### `sirio_ui`

- `change_request_actions.rs` — the header's action buttons and the per-action
  state machine (idle, in flight, failed with its message).
- `change_request_composer.rs` — the composer and the edit-in-place fields.
- `change_request_merge.rs` — the merge strip and its confirmation.
- `change_request_pickers.rs` — the reviewer and label selectors.
- `ci_log_tab.rs` and `ansi_log.rs` — the log tab, and the pure function
  behind it that turns log text into styled spans and folding groups.
- `ChangeRequestTab` calls them and emits `ChangeRequestTabEvent::Changed(
  ChangeRef)` after a write, so the host refreshes the right panel's list at
  once instead of at its next 60 s tick.

### `sirio` (host)

- `TabKind::CiLog` and its persistence (§8).
- The debug-only control verbs (§10).

### Dependency graph

No new crate and no new edge. `CLAUDE.md`'s crate diagram stays as it is; its
*Change requests* section gains one paragraph on the write door.

## §5 Actions

| Action | GitHub | GitLab |
|---|---|---|
| Comment | GraphQL `addComment` | `createNote` |
| Review — approve | `addPullRequestReview`, event `APPROVE` | approve (GraphQL if the server has it, else REST `…/approve`) |
| Review — request changes | `addPullRequestReview`, event `REQUEST_CHANGES` | the review state "requested changes", only where the server has it |
| Review — comment | `addPullRequestReview`, event `COMMENT` | `createNote` |
| Close, reopen | `closePullRequest`, `reopenPullRequest` | `mergeRequestUpdate`, state |
| Draft ↔ ready | `convertPullRequestToDraft`, `markPullRequestReadyForReview` | `mergeRequestSetDraft` |
| Edit | `updatePullRequest` (title, body, base) | `mergeRequestUpdate` |
| Edit own comment | `updateIssueComment`; `updatePullRequestReview` for a review's body | `updateNote` |
| Merge | `mergePullRequest` (`mergeMethod`, `commitHeadline`, `commitBody`, `expectedHeadOid`), then `deleteRef` if asked | `mergeRequestAccept` (`sha`, `squash`, `shouldRemoveSourceBranch`, commit messages) |
| Merge when checks pass | `enablePullRequestAutoMerge` | `mergeRequestAccept` with an auto-merge strategy |
| Cancel auto-merge | `disablePullRequestAutoMerge` | cancel the auto-merge (REST if GraphQL has none) |
| Reviewers | `requestReviews` (`union: false` replaces the set, which is how one is removed) | reviewers mutation, version-gated |
| Labels | `addLabelsToLabelable`, `removeLabelsFromLabelable` | `mergeRequestSetLabels` |
| Re-run failed jobs | REST `POST /repos/{o}/{r}/actions/runs/{run}/rerun-failed-jobs` | `pipelineRetry` |
| Re-run one job | REST `POST /repos/{o}/{r}/actions/jobs/{job}/rerun` | `jobRetry` |
| Job log | REST `GET /repos/{o}/{r}/actions/jobs/{job}/logs` | REST `GET /projects/:id/jobs/:job/trace` |

The names come from each forge's public schema; §13 makes verifying every one
against the live schemas the first task of the plan, with the fallback decided.

Rules that hold for every action:

- **Nothing the forge would refuse is sent.** `act` checks the action against
  `Capabilities` first — a merge method the repository does not allow, a review
  on one's own pull request — and refuses with no request.
- **A comment, and a review that is not an approval, needs a non-empty body**
  (GitHub itself requires one for *Request changes*); an approval may be
  empty; a title may not be empty. The UI enforces it before calling `act`.
- **A write is never retried on its own.** A comment posted twice is worse than
  a comment not posted. After a `Network` failure the outcome of a send is
  unknown: the tab re-reads the header, and only then says "could not confirm —
  look at the conversation before sending again", keeping the text.
- **The forge is the truth.** After a successful action the tab re-reads the
  header and its timeline and redraws from them; no state is patched locally.
  A failed action leaves everything as it was, plus its message.
- **Merge sends the head the user saw** — `Revisions::head_sha` from the header
  the strip was drawn from — as `expectedHeadOid` / `sha`. Someone pushing
  between the screen and the click makes the forge refuse; the tab shows
  `HeadMoved` and reloads, so no commit is merged unseen. Before sending, `act`
  also compares that sha with a fresh read of the head, because a refusal's
  wording is not a contract.
- **Delete branch** is a flag of the accept on GitLab. On GitHub it is a second
  call, `deleteRef`, after a merge that succeeded, and only for a head in the
  same repository. If only that call fails the result is `Done` with the
  warning "merged; deleting the branch failed: …" — the merge is never reported
  as failed.
- **The busy state is per action kind**: while a comment is in flight its
  button is disabled, so a double click cannot post twice.

## §6 Capabilities

```
Capabilities {
    can_comment, can_approve, can_request_changes,
    can_edit, can_change_state, can_toggle_draft,
    can_edit_reviewers, can_edit_labels, can_rerun_checks,
    merge: MergeCapability {
        verdict:              Ready | WaitingOnChecks | Blocked(reason),
        methods:              the merge / squash / rebase the repository allows,
        default_method,
        can_auto_merge, auto_merge_enabled,
        delete_branch_default,
    },
}
```

- **GitHub** reads the pull request's `viewerCan…` fields, `mergeable`,
  `mergeStateStatus`, `autoMergeRequest`, and the repository's
  `mergeCommitAllowed`, `squashMergeAllowed`, `rebaseMergeAllowed`,
  `autoMergeAllowed` and `deleteBranchOnMerge`. **GitLab** reads
  `userPermissions`, `detailedMergeStatus`, `squashOnMerge`,
  `shouldRemoveSourceBranch`, `autoMergeEnabled` and the available auto-merge
  strategies.
- `Blocked` names its reason — conflicts, review required, checks failing,
  behind the target, draft — and an unknown status becomes
  `Blocked(Other(<the forge's own word>))`: the forge decides, and Sirio never
  invents a verdict it cannot back. `WaitingOnChecks` is what turns *Merge*
  into *Merge when checks pass* where the repository allows it.
- **A capability the forge did not report is not offered** — except *Comment*,
  which is offered unless the forge says the conversation is locked. On a
  GitLab answering with the baseline query the permission fields that server
  lacks read as "not reported" and their buttons stay hidden (§13). The
  forge's refusal is shown either way: a capability is a courtesy, the server
  has the last word.
- Own pull request: no *Approve*, on GitHub (`viewerDidAuthor`); GitLab's
  setting for self-approval is read where the server reports it.
- Admin bypass of a block (`viewerCanMergeAsAdmin`) is not offered (§12).

## §7 Interface

### §7.1 The detail tab

```
[ (pr) #578 feat(ui): PR view in the right...  x ]
+------------------------------------------------------------------+
| feat(ui): PR view in the right panel #578                        |
|   [edit] [draft/ready] [close] [browser] [refresh]               |
| (Open) epalmisano wants to merge feat/pr-view -> main - 2h ago   |
| Reviewers: bob (approved)  carol (requested)  [+]  Labels: ui [+]|
+------------------------------------------------------------------+
| Ready to merge - squash v                            [ Merge ]   |
+------------------------------------------------------------------+
| Conversation 3 | Commits 5 | Checks 3/7 | Files 12               |
|  ... timeline ...                                                |
|  +------------------------------------------------------------+  |
|  | Leave a comment...                                         |  |
|  +------------------------------------------------------------+  |
|                                  [ Comment v ]                   |
+------------------------------------------------------------------+
```

- **Header actions** are icons with tooltips, like the rest of the app. One
  button per state change that applies: *Close* or *Reopen*, *Ready for
  review* or *Convert to draft*. *Edit* opens the title, the description and
  the target branch in place, in the same `TextField`s as the composer, with
  *Save* and *Cancel*. A button appears only when its capability is true.
- **Reviewers and labels** sit under the meta line. `+` opens a picker that asks
  the forge for candidates — assignable users and the project's labels —
  with a search debounced 300 ms like the list's; picking and closing sends
  one `SetReviewers` or `SetLabels`.
- **The merge strip** is fixed under the header, so it shows on every inner tab:
  one merges while looking at *Checks*. It shows the verdict from
  `Capabilities` (*Ready to merge*, or *Blocked: 1 review required*, with the
  reason), the method as a selector over `methods` only, and *Merge*. A
  blocked change request keeps the strip and disables *Merge*; it is never
  hidden. With `WaitingOnChecks` and `can_auto_merge` the button reads *Merge
  when checks pass*, and once enabled the strip says so and offers *Cancel
  auto-merge*.
- **The merge confirmation** — the only one in the app — is a `Modal`: the
  method, the commit title and message (editable for merge and squash), a
  *Delete branch* checkbox preset from `delete_branch_default`, the target
  branch and the head sha being merged. *Merge* in the modal is the send.
- **The composer** closes the Conversation: a growing `TextField` in Markdown
  source and one split button. *Comment* is the default; its menu holds
  *Approve* and *Request changes*, each only when its capability is true. On
  GitHub the three are one review with an event; on GitLab *Approve* and
  *Comment* are two calls, and *Request changes* exists only where the server
  has it. Cmd/Ctrl+Enter sends.
- **Editing one's own comment**: a pencil on a comment or review body the user
  may edit turns it into a `TextField` with *Save* and *Cancel*. There is no
  delete (§1).
- **Every action, the same way:** its button is disabled while it is in
  flight; the typed text is cleared only on success, so a failure keeps it,
  with the message and its remedy beside the action (§9); a success redraws
  from the re-read header. The composer's text, an open edit and a picker live
  in the tab's entity: they survive switching inner tabs, not quitting Sirio.

### §7.2 Checks

A failed or canceled check has *Re-run* (icon) when `can_rerun_checks`. A group
— the workflow on GitHub, the stage on GitLab — has *Re-run failed*. A re-run
is an action like the others: no confirmation, in flight, then the checks and
the header re-read.

### §7.3 Which checks have a log

Only a GitHub Actions job and a GitLab CI job — the checks that carry a `job`.
A third-party check run and a GitHub `StatusContext` keep today's behaviour: the
click opens the check's URL in the browser. The others get *View log*.

### §7.4 The log tab

`TabKind::CiLog`, in the Secondary half, read-only, keyed by (`ChangeRef`, job).
Opening the same job again focuses its tab.

- The log is drawn as a virtualised list of lines — a large log is tens of
  thousands — and its text is selectable through `selectable_text`.
- `ansi_log` turns SGR colour and style codes into spans (16, 256 and true
  colour, bold, dim, underline, reset), drops every other escape sequence, and
  folds a group: `##[group]` … `##[endgroup]` on GitHub, `section_start` …
  `section_end` on GitLab. A carriage-return progress line keeps its last
  state.
- *Jump to first error* where the log marks one (`##[error]` on GitHub).
- The download is bounded: the **tail** is kept, and a log cut says so with
  *Open in browser*. It is never truncated silently.
- While the job runs, the log reloads every few seconds, **only while the tab
  is visible** (A §9). A running job whose log the forge has not published
  says "not available until the job finishes" and tries again with the tab's
  refresh.
- A log never reaches the disk and never reaches `sirio_perf`; the tab
  persists only its identity (§8). A forge that hides a secret masks it in the
  log; Sirio does not add anything to that.

## §8 Identity and persistence

- `TabKind::CiLog`: `default_pane() == Secondary`,
  `can_move_between_panes() == false`, `appears_in_sidebar() == false`. The
  three exhaustive matches, `persisted_kind` and `kind_from_persisted` force
  each answer to be written.
- `TabRecord::kind` = `"ci_log"`. `SessionTabState` gains `ci_log:
  Option<PersistedCiLog>` — `{ change_request: PersistedChangeRef, job_id,
  name }` — `#[serde(default)]`, so sessions written before read as they did.
- On restore the tab appears at once under its saved title and loads when
  shown. A job that can no longer be reached (404, host disconnected, log
  expired) keeps its tab with the error, *Retry* and *Close* — never dropped
  silently (A §8).
- Nothing else is new on disk: the composer's text is not saved, so an unsent
  comment does not survive a quit.

## §9 Errors

`ForgeError`, forge → UI. A's variants keep their meaning (A §10); an action
adds three (`Rejected`, `HeadMoved`, `Unsupported`) and sharpens three
(`Forbidden`, `Network`, `NotFound`).

| Variant | Typical cause | Shown as |
|---|---|---|
| `Forbidden` on a write | the token lacks a write scope; GitHub SAML SSO not authorised | "This token cannot write" naming the scope to add — `api` on GitLab; `repo` (classic) or *Pull requests: write* (fine-grained) on GitHub — with a link to Settings, or the SSO link |
| `Rejected { host, message }` | the forge understood and refused: conflicts, a required review, a protected branch, a validation | the forge's own message, sanitised, beside the action; the form keeps its text |
| `HeadMoved` | the head is not the sha the user saw | "The branch changed since you opened this" + *Reload* |
| `Unsupported { what }` | an older GitLab lacks a mutation | "Not available on this version of GitLab" — its button was hidden anyway (§6) |
| `Network` after a send | the outcome is unknown | the header is re-read, then "could not confirm — look at the conversation before sending again" (§5) |
| `NotFound` on a write | a label, reviewer or comment vanished | the message + *Reload* |

- A GraphQL `errors[]` on a mutation, and a REST 4xx body, are classified by
  one pure function; an unrecognised one is `Rejected` with the forge's words —
  never `UnexpectedResponse`, which stays for a body that cannot be read.
- A rate-limit answer stops writes exactly as it stops reads (A §9).
- The token appears nowhere: not in an error, a log, a trace or a captured
  stderr. A comment's or a log's content never reaches `sirio_perf`, whose
  labels stay content-free `&'static str`.
- Settings → Git hosting separates the scope to *read* from the scope to *act*
  (§2) and says which one the saved token has when the forge reports it
  (GitHub's `X-OAuth-Scopes` on a classic token; a fine-grained one does not
  report them, and the page says so).

## §10 Testing

End to end, per the project convention; every run ends in an artifact.

1. **`Scripts/Tests/test-forge-actions-e2e.sh`** → `FORGE ACTIONS E2E OK`, on
   the shape of `test-forge-diff-e2e.sh`: an isolated Sirio (`SIRIO_DB`) whose
   worktree's remote is the fake forge, GitHub **and** GitLab, one stage per
   slice, each added by its slice.
   - `Scripts/Tests/fake_forge.py` learns mutations and the REST paths of §5.
     Its request log — already one line per request with the operation and
     the variables — is the artifact that says what was **sent**. After a
     mutation it answers the header read from `<Operation>.after.<Mutation>.json`
     when that fixture exists, so a redraw is proved against what the forge
     says, not against what Sirio hoped.
   - Sirio is driven over the control socket. The verbs that **write** —
     `surface.change_request.act` with the action and its parameters — are
     compiled only under `cfg(debug_assertions)`, as A's and B1's test
     overrides are honoured only in debug builds: a release binary answers
     "unknown method" and `sirioctl capabilities` does not list them. A
     write over the socket would otherwise let any process in an agent's pane
     act on the forge with the user's credentials; hand-off to agents is C's
     decision, not a side effect of B2's test hook. The verb calls the same
     `perform` the buttons call, so it proves everything but the pixel click.
     Navigation and reads — `surface.change_request.read` extended with the
     capabilities, the strip's verdict and each action's state, and
     `surface.ci_log.open` / `.read` (lines, folds, first error, truncated) —
     follow the existing `surface.*` pattern and are in every build.
   - It proves, per slice and per forge:
     - each action **on the wire**: exactly the expected mutation or REST
       call with the expected variables in the fake's log, a comment's body
       byte for byte — newlines, quotes, non-ASCII — and a merge carrying the
       fixture's head sha;
     - **gating**: with a fixture whose viewer may not merge, the strip reports
       *Blocked* and its reason, and a `merge` act sent anyway is refused
       **with no request in the log**; on the viewer's own pull request there
       is no approval to send;
     - **the failures**: a 403 naming a missing scope shows the scope and keeps
       the typed text; a 422 shows the forge's message; a moved head shows
       `HeadMoved` and reloads; a mutation whose connection the fake drops
       after reading the request (a `Network` failure, with no 20 s wait)
       shows "could not confirm" after a re-read and **is not sent a second
       time**; two sends of the same action while the first is in flight — the
       fake holds its answer back so they overlap — log **one** mutation (the
       busy state); a rate-limited host receives no write until its reset;
     - **the merge flow**: with checks pending and auto-merge allowed the
       strip offers *Merge when checks pass* and then *Cancel auto-merge*;
       *delete branch* on GitHub is a second call, and a fixture that fails
       only that call gives a merged result with the warning;
     - **CI**: *Re-run failed* and *Re-run job* reach the fake as the REST calls
       of §5; the log fixture — ANSI colour, groups, an `##[error]`, a
       carriage-return progress line — reaches the tab with the expected
       folds and first error; a log over the cap arrives truncated and says
       so; the log request is redirected to another host name on the loopback
       (`localhost` for `127.0.0.1`), and the fake logs that the redirected
       request **carried no `Authorization` header** — under the token means
       and under both CLIs; a running job's log is fetched again while the tab is
       visible and **not** while it is hidden.
   - Both real CLIs run the comment, merge and re-run stages — GraphQL and
     REST over `gh api` and `glab api` — the way A's script runs them; a CLI
     that is not installed makes its stage print `SKIP:`.
   - Artifacts under `--out-dir`: the socket transcript, the fake forges'
     request logs, each scenario's report keys, and PID-matched window
     captures unless `--state-only`.
2. **Live conformance** (`sirio_forge/tests/forge_live.rs`, extended): the read
   side runs the new header fields, the candidate reads and the check ids
   against github.com and a public gitlab.com project. The write side runs no
   write: it **introspects the live schema** and asserts that every mutation
   §5 names, with every input field Sirio sends, exists. This is what turns
   §13's first row into a repeatable artifact. It SKIPs without credentials and
   joins the release gate's skip list.
3. **Unit tests only for pure units that must be proven in isolation**, each
   with every way it can fail written before the code:
   - mapping a forge's answer to `Capabilities` — unknown enum values, missing
     fields, the baseline GitLab answer, the own-pull-request rule;
   - classifying a mutation's GraphQL `errors[]` and a REST 4xx body into a
     `ForgeError`;
   - `ansi_log`: a sequence split across chunks, a malformed SGR, 256 and true
     colour, a reset, a carriage return, a group that never closes, a line
     longer than any screen;
   - keeping the tail of a download and reporting the cut.
   Request builders are **not** unit-tested: the end-to-end log of what was
   sent is the assertion, and a test that restates a builder's output would
   only pin its internals.

## §11 Slices

Each slice is planned and merged on its own, and leaves Sirio working.

- **B2a — foundations and the conversation.** `Transport::request`;
  `Action` and `act`; `Capabilities` as far as the slice reads them; the three
  new errors and Settings' scope text; the debug verbs, the fake forge's
  mutations and the script with its first stage. UI: the header's state
  actions and *Edit*, the composer with *Approve* and *Request changes*, and
  editing one's own comments.
- **B2b — merge and metadata.** The merge strip and its confirmation, the
  methods, *Merge when checks pass* and *Cancel auto-merge*, *Delete branch*;
  the reviewer and label pickers with their candidate reads.
- **B2c — CI.** `CheckJob` on `Check`; *Re-run* and *Re-run failed*;
  `job_log`; the log tab with `ansi_log`, the tail cap and the redirect rule;
  `TabKind::CiLog` and its persistence.

## §12 Out of scope

- Deleting a comment (§1).
- Merging past a block as an administrator (`viewerCanMergeAsAdmin`, GitLab's
  skip-the-pipeline options).
- Creating a change request through the API — A's *Create on the forge* stays a
  URL.
- Cancelling a run or a pipeline.
- Inline review threads, replying to and resolving them, suggested changes,
  and a review drafted over several files — **B3**.
- Assignees, milestones, reactions.
- A write verb in a release build's control socket, and handing a change
  request to an agent — **C**.
- Notifications, and badges on sidebar rows (A §12).

## §13 Verified in the plan, with the fallback already decided

| Point | Fallback if the check fails |
|---|---|
| Every mutation of §5 exists with the input fields Sirio sends, on github.com and gitlab.com (the live introspection test of §10.2) | the action goes through `Transport::request` where the forge has a REST equivalent; where it has none, it is not offered on that forge and §5 says so |
| GitLab *approve*, *request changes* and reviewer editing exist on the oldest server the baseline targets (15.0) | the capability reads false and the button stays hidden; `Unsupported` if an action arrives anyway |
| GitHub's job log answers with a redirect to a signed URL, and the token does not follow it to that host | the token means does not follow the redirect and fetches the `Location` with **no** `Authorization` (§4). `gh api` and `glab api` cannot be told not to follow one: what each does on a 302 is checked, and if a CLI follows it and hands back the log, that is used as it is — the CLI owns its credential. §10's assertion that the signed-URL request carried no `Authorization` runs under every means |
| A running GitHub job's log is served, or is a 404 until the job ends | the tab says "not available until the job finishes" and retries with its refresh |
| `rerun-failed-jobs` needs a token permission a fine-grained token may lack | `Forbidden`, naming *Actions: write* |
| `expectedHeadOid` and `sha` make the forge refuse a moved head | `act` compares the head with a fresh read before sending (§5), so the guard does not depend on the wording of a refusal |
| The values of `mergeStateStatus` and `detailedMergeStatus` map onto `MergeVerdict` | an unknown value is `Blocked(Other(word))` |
| `requestReviews(union: false)` removes a reviewer on GitHub | REST `DELETE …/requested_reviewers` |
| Which permission fields the GitLab baseline queries can keep | the ones it cannot are "not reported" and their buttons hidden (§6) |
| A log of a hundred thousand lines scrolls in the virtualised list | the cap is lowered until it does, and stays stated in the tab |

## §14 Revised while planning and building slice B2a (2026-09-29)

Each item supersedes the sentence of the spec it names. Everything here was
built and run; §14.2 lists what was not.

### §14.1 What changed

1. **GitLab's approval is REST.** §5's row says "GraphQL if the server has it,
   else REST". gitlab.com's schema has no `mergeRequestApprove` (checked
   2026-09-29), so the approval is `POST projects/:id/merge_requests/:iid/approve`
   through `Transport::request`. `mergeRequestRequestChanges` does exist there.
2. **Buttons, not icons, and three buttons, not a split button.** §7.1's icons
   with tooltips and the composer's split button: the vendored Zed icon set has no
   pencil or draft glyph and bezel has no menu. The header's actions are labelled
   buttons, and the composer has *Comment*, *Approve* and *Request changes*.
3. **Edit is a card at the top of the Conversation**, with the three fields,
   *Save* and *Cancel* — not in place under the header (§7.1).
4. **One write in flight per tab**, not per action kind (§5): stricter, and it
   keeps a *Close* from racing a *Comment*.
5. **`Capabilities.can_change_state`** is one flag whose meaning follows the
   state (close on an open change request, reopen on a closed one), and each
   action is checked against the *state* as well (§6). `ForgeError::HeadMoved`
   exists, and nothing produces it until B2b's merge.
6. **Every action pre-flights.** `act` reads the change request's node id, state
   and permissions with one small query before it checks and sends
   (`ChangeRequestActionContext`, `MergeRequestActionContext`, with a baseline
   variant on GitLab); the header's own capabilities only decide what is drawn.
7. **The UI's modules are children of `change_request_tab`**
   (`change_request_tab/{actions,edit,composer}.rs`), not siblings (§4): they
   need the tab's private fields.
8. **The debug-only verb is guarded at dispatch** with `cfg!(debug_assertions)`
   — compiled in, unreachable in a release build, which answers "unknown
   method" and does not list it — rather than removed with `#[cfg]` (§10).
9. **The live test sends the documents**, with variables that name nothing, and
   requires no error to carry `extensions` (a validation error does; a runtime
   "not found" does not) — instead of introspecting the schema (§10.2, §13).
   GitLab's probes use iid and id `0`, which no row has.
10. **Settings reports the saved token's scopes** (§9): `TokenScopes`, read from
    GitHub's `X-OAuth-Scopes` (`GET user`) and GitLab's
    `personal_access_tokens/self`; a token that reports none is "scopes not
    reported".
11. **The `edit-comment` control verb drops any open comment editor before
    starting one**, so a refused start leaves nothing to reuse (the first
    shape let a refused edit of a non-editable entry save the wrong
    comment).
12. **A send whose outcome carries a warning keeps the words in the
    composer.** On GitLab the approval or the request for changes went
    through while the comment beside it did not; §7.1 clears text only when
    its write succeeded.
13. **Saving a comment edit whose text is unchanged sends nothing**,
    compared after `normalize` as the header edit is (§7.1).

### §14.2 Not verified

- `glab` is not installed on the machine this was built on: the `cli` stage's
  GitLab half (a GraphQL comment through `glab api`, and the REST approval)
  printed `SKIP:`. `gh` ran for real.
- No window frame was captured (no display): the header buttons, the edit card,
  the composer and the status line were seen only through the control socket's
  report keys, and the Cmd/Ctrl+Enter chord was bound but never pressed.
- A release build's absence of `surface.change_request.act` was checked once
  by hand (2026-09-29, Task 8): after `cargo build --release -p sirio --bin
  sirio -p sirio_control --bin sirioctl`, `sirioctl capabilities --json` lists
  79 methods without it and `sirioctl surface change-request act close`
  answers `unknown control method` (exit 1).
- The GitLab live test skips without `glab` or `SIRIO_FORGE_LIVE_GITLAB_TOKEN`;
  its documents were sent by hand to gitlab.com on 2026-09-29 and accepted.
- The two `ActChangeRequest` enforcement arms in `sirio`'s `#[cfg(windows)]`
  test helpers were added by hand; they neither compile nor run on Linux, so
  they are checked only by the macOS/Windows CI.

## §15 Slice B2c on Ely (2026-10-04)

`docs/superpowers/specs/2026-10-03-change-requests-on-ely-design.md` moved
every change-request surface to Ely and left B2c to its own brainstorm (its
§11). Settled with the user on 2026-10-04; each item supersedes the sentence
of this spec it names, and everything else in §4–§13 about B2c stands — the
forge side (`CheckJob`, `Rerun`, `job_log`, the redirect carrying no
credential, the tail), `TabKind::CiLog` and its persistence, and the proof.

### §15.1 Checks (supersedes §7.2, §7.3)

- **Checks are grouped by run**: a header per workflow (GitHub) or stage
  (GitLab) — a disclosure chevron, the group's name, its count — with its jobs
  under it, failed first, then running, queued and settled. A group with
  nothing failed or running starts folded. Checks with no group (a GitHub
  `StatusContext`, a third-party check run) sit together under *Other checks*.
- ***Re-run failed*** is an `IconButton` on the header of a GitHub workflow
  whose run has a failed or canceled job (`Rerun::FailedInRun`). GitLab retries
  failed jobs per **pipeline**, not per stage, so there it sits once, on a row
  above the stages naming the pipeline; its stages have none.
- **Each job row**: a click opens its log in Sirio (§15.2) — the same job again
  focuses its tab; a check with no `job` keeps opening its URL in the browser.
  On the right, an `IconButton` *Re-run* on a failed or canceled job
  (`Rerun::Job`) and an *Open in browser* when the check has a URL.
- **Who may re-run**: `Capabilities.can_rerun_checks`, from the repository's
  `viewerPermission` (write or above) on GitHub and the pipeline's
  `userPermissions.updatePipeline` on GitLab; a job the forge says is not
  retryable (`retryable` on GitLab, a run still in progress on GitHub) has no
  button. A re-run is an action like the others (§5): no confirmation, one
  write in flight per tab (§14.1 item 4), then the checks and the header are
  re-read.

### §15.2 The log tab (supersedes §7.4's drawing)

- **Ely chrome**: a toolbar of `IconButton`s — *Refresh*, *Jump to first
  error* (disabled when the log marks none), *Copy log*, *Open in browser* —
  over the job's name and status; loading is a `Skeleton`; every failure is a
  `Callout` with *Retry* (and *Close*, §8).
- **The body is a gpui `uniform_list` of lines, never wrapped**: every row the
  same height, a horizontal scroll for long lines, a line-number gutter. Ely's
  only virtual list (`lists/long.rs`, upstream only) measures each row and is
  not used for a log of a hundred thousand lines.
- **Groups fold** with Ely's disclosure chevron on the group's first line; a
  group's header carries a *Copy* `IconButton` for the group's text. An
  unclosed group runs to the end of the log.
- **Selection is per line** through `selectable_text`, each line with its own
  `.id(..)`; copying more than a line goes through *Copy log* and a group's
  *Copy*, which copy the text without escape sequences.
- **Colours**: the sixteen SGR colours are the theme's `ely.ansi` palette, the
  terminal's own; 256-colour and true-colour codes are drawn as given.
- **The tail cap is 4 MiB**, cut at a line boundary; a cut log says how much it
  kept and offers *Open in browser*.
- **A running job's log reloads every 5 s**, only while the tab is visible,
  and stops once the job is settled.

### §15.3 Socket

- Every build: `surface.ci_log.open` (by the check's index or job id in an open
  change request tab) and `surface.ci_log.read` — the title, the job's status,
  the line count, the folds, the first error's line, `truncated`, `complete`,
  and whether the tab is reloading.
- Debug builds only, through the existing `surface.change_request.act` door:
  `rerun-job --job ID` and `rerun-failed --run ID`.

### §15.4 Revised while building

The B2c implementation and its proof made these changes to the plan. The
controller's ruling on control strings supersedes the earlier multiline
strategy; §15.1–§15.3 remain the drawing contract.

**Forge and transport**

- `RestRequest.log`, rather than a new `follow_redirects` field, identifies
  log reads. The token transport disables their redirects; the signed URL
  is fetched separately without a credential. `gh` follows the redirect
  itself, and its request to the fake signed-log host carried no
  `Authorization`. Its log call uses `--allow-escape-sequences`.
- GitHub's job is retryable only when its check suite reports `COMPLETED`;
  a failed job in a still-running suite has no *Re-run*. GitLab keeps its
  `retryable` answer. Probe output adds `JOB` (job id, run id, retryable)
  and `RERUN` (the viewer's permission).
- Generated readonly headers explicitly set GitHub `viewerPermission` to
  `READ` and GitLab `updatePipeline` to false. Inheriting the writable
  fixture's permissions would falsely expose re-runs.
- A log download is bounded at **64 MiB**, before the **4 MiB** retained
  tail. Both token and CLI means reject a body above the download bound;
  CLI stdout is bounded too, with room for HTTP framing. Non-log CLI reads
  retain their existing behavior.
- If the retained tail begins inside an overlong line, its cut advances
  past a UTF-8 character or a partial CSI, control string or ordinary
  escape. A fragment such as `31mred` must not become visible text.

**Parsing**

- GitLab's first error is the first rendered line starting `ERROR: `;
  an `ERROR:` inside a line is not a marker. Leading `ESC[0K` is removed
  before recognizing GitLab's section markers so those sections fold.
- Control strings (OSC, DCS, SOS, PM, APC) and ordinary escape sequences
  with multiple intermediates are consumed as whole sequences. A control
  string ends at ST, at BEL for OSC, or at the end of its line, whichever
  comes first. An unterminated introducer must leave later lines visible,
  including a later `ERROR: ` that *Jump to first error* can find. This is
  the controller's correction to the initial multiline consumption.
- Carriage return clears overwritten text and spans while retaining the
  current SGR style. Spans use UTF-8 byte ranges. The color assertion for
  `42;103` was corrected: 103 supersedes 42 and selects bright yellow,
  `Ansi(11)`, rather than green.

**Log tab and host integration**

- The log tab shares `Slot::begin` and `Slot::finish` at crate visibility.
  Its retained vertical scrollbar is created lazily on first render,
  because `ScrollbarState` needs `Painter::of(cx)` and the tab's constructor
  has no context. An empty published log supplies no width-sample row.
- The sidebar's exhaustive match had to gain the log arm before the UI
  crate could build. The sidebar and host use the existing `SquareTerminal`
  glyph; the tab remains in the Secondary half.
- Ely's actual interfaces use `Severity::Danger`, a vector of text
  highlights, and inherited foreground from a surrounding `div` for
  selectable marker text. The CLI borrows its owned parsed job argument
  for the `ci_log_open` request builder.
- Refresh failure retains the last good log but reports `state=error`.
  Unreachable, failed and stale states, including stale unpublished logs,
  use an Ely `Callout` with selectable text, *Retry* and *Close*.
- The socket's `top` report uses a pending `DeferredScrollToItem` index
  before the painted scroll position. State-only E2E proves the logical
  jump request; the actual viewport placement needs a framed run.

**Checks and actions**

- A queued-only group starts folded, as §15.1 requires; queued jobs still
  sort ahead of settled jobs. Group keys have separate run and group
  namespaces, so a bare workflow name `CI#1` cannot alias `CI` run 1, and
  a stage named `other` cannot share *Other checks*' disclosure override.
- A re-run's `Forbidden` message needs the action kind, now passed through
  the error formatter. The existing candidate-read caller supplies its
  own kind and retains its previous remedy. The control protocol comment
  and CLI help include `rerun-job` and `rerun-failed`, as the plan's step
  required despite omitting those files from its file list.
- Refreshed groups are sorted afresh: after GitHub job 2 queues, running
  *Lint* precedes *CI*. The E2E pins the complete refreshed row string;
  replacing just `failed` with `queued` would preserve the wrong order.
- The readonly UI assertion starts from a reset, failed job, so absence
  of *Re-run* proves permission gating rather than a queued job's lack of
  retryability. The raw action is refused locally and sends no re-run.
  After restoring the good token, the harness reselects and loads *Checks*
  before opening a log.

**Proof and compatibility**

- Permission tests use the existing state-taking builders on open change
  requests. Existing action test literals gained `can_rerun_checks: false`;
  adding that public field did not change those actions. Task 2 added one
  library test and extended another, so its count rose from 125 to 126,
  not by two. The grouping filter was widened to run all four named tests,
  rather than just the one whose name starts with `check`.
- The unknown-job E2E selects *Checks* before requesting job 999 and checks
  the lookup error, avoiding a false pass on the active-tab type guard.
  Both forges also prove fold toggling, restoration of visible lines and
  nonempty group copy, which the original E2E snippet omitted.
- Quit/restore waits for the owned app PID, removes that harness's old
  socket inode, then launches again. It shows the right-panel list before
  changing credentials or reopening a change request after restore.
- Commands run in the implementation worktree with artifacts under the
  exported cache `TMPDIR`; the diff harness's hardcoded `/tmp` template
  was changed to honor it. The wire-only `test-forge-e2e.sh` has no
  `--state-only` flag; the UI-bearing scripts use it. The required nextest
  suite uses empty `gh`/`glab` configurations and unsets forge tokens, so
  its live conformance bodies print `SKIP:` rather than touch real forges.
  The first Task 1 suite was interrupted before any live test ran; its
  cancellations were not product failures, and the isolated suite passed.
- The diff harness waits for its initial unsigned host probe to settle
  before the socket saves a token, matching the visible sign-in flow and
  the forge UI E2E. Saving earlier allowed the old probe to cache an
  unsigned resolution after the token save had invalidated it; the list
  could load while the detail was unreachable. The full regression
  exposed this twice, in its 401 and hanging-fetch scenarios.
- The actions harness accepts `--appearance light|dark` and seeds each
  isolated CI/UI database through `appearance_seed`. State-only proof
  establishes re-runs, bounded log reads, parser output, folds, copy and
  restoration. Drawing and the running-log timer's visible, hidden, slow
  and rate-limited branches are proven by the framed run.
- After the final review: a tick never starts a second read of a log while
  one is in flight, and spends its drawn mark only on a read that starts,
  so a rate-limited log resumes after the reset by itself. The signed URL
  is fetched https-only at every hop in a release build. A restored tab
  keeps its link only as `https://` on its change request's host, and its
  name without control characters, capped at 120.

### §15.5 Not verified

- Selection inside a log line, dragging the scrollbars, and *Jump to first
  error* in a log long enough to scroll. The dark and light frames, the
  drawn jump in a log that fits, and the reload timer (visible, hidden,
  slow, rate-limited and resuming after the reset) were seen on 2026-10-05
  (testing document, *What was seen*).
- The `glab` paths, including log reads and GitLab CLI writes: `glab` is
  not installed on the implementation machine. Token means on both fake
  forges and the installed `gh` means are exercised.
- A live GitHub running job's log, or any live forge write. A fake running
  GitHub job proves the unpublished-log state; live conformance is
  deliberately skipped with isolated credentials.
- macOS, Windows and Wayland. The `#[cfg(windows)]` test helpers were
  updated for the log pane but neither compiled nor executed on Linux.
- Overlapping a socket token save with initial host resolution. The diff
  harness now follows the visible sign-in order; the production resolution
  cache's handling of that overlapping socket sequence is unchanged.
