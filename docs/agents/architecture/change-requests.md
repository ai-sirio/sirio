# Change requests: one GraphQL layer, two transports (`sirio_forge`)

`sirio_forge` reads GitHub pull requests and GitLab merge requests —
"change requests" — through GraphQL on both forges, and parses each forge's
answers once. How the request travels is the user's *means*: `CliTransport`
hands it to `gh api` / `glab api --include`, which own authentication;
`TokenTransport` sends it with `ureq` and the platform certificate verifier,
so a self-managed forge behind a corporate CA verifies. The token is handed
in by the caller and never stored or logged by the crate. GitLab queries
that carry a merge request summary have a baseline variant, used from the
first `Field '…' doesn't exist` onward, because GraphQL rejects a whole
query that names a field an older server lacks. `SIRIO_FORGE_TEST_ENDPOINTS`
points hosts at loopback in debug builds only, and `SIRIO_FORGE_FETCH_TIMEOUT_MS`
shortens the fetch's 120 s bound the same way. `Scripts/Tests/test-forge-e2e.sh`
proves the seam; `tests/forge_live.rs` notices the day a query stops
matching the live schema (it SKIPs without credentials and is off both
gates). `docs/superpowers/specs/2026-09-27-change-requests-design.md` has the
design.

The UI reads through one seam, `sirio_ui::forge_source::ChangeRequestSource`,
a GPUI global the host sets once (`sirio/src/forge.rs`, `ForgeHub`): the
right panel's fifth view and the `ChangeRequest` tab in the Secondary half
never see a token or a setting. `ForgeHub` owns the `forge.hosts` setting —
a Settings-screen save re-applies the stored value — and keeps tokens in the
credential store under `forge:<host>`. The list asks nothing before it is
shown and refreshes only while visible. A rate limit pauses every read until
its reset (a minute when the forge names none), and the list looks again by
itself when it ends. `surface.change_requests.search` sets the list's real
search field, including its debounce. A restored tab loads when shown.

The detail tab's *Files* is the diff, read by `sirio_git::range_*` between
the commits `sirio_forge::Revisions` names. When either is not local,
`ForgeHub` makes them local with one non-interactive `git fetch` into
`refs/sirio/change-requests/<remote>/<N>/{head,base}` — invisible to
History's `--branches`, visible to an external `git log --all`. The fetch
runs with git's own credentials (a helper or an ssh key), never with the
token Sirio holds; when the target branch is gone from the forge the head
ref is fetched alone. Those refs only anchor objects against `git gc`, and
are swept — at startup, on a worktree switch and when a tab that held them
closes — of whatever no tab, open or parked with another worktree, holds.
*Open in editor* opens the local file when the worktree is at the head, and
a read-only snapshot tab otherwise; a persisted snapshot's sha and path are
refused before they reach a label or a path. Its Discard asks inside the
window (an Ely `Dialog`), and a row's right-click offers *Copy path* and, on
a change request, *Open on the forge*.
`docs/superpowers/specs/2026-09-28-change-request-diff-design.md` has the
design; `Scripts/Tests/test-forge-diff-e2e.sh` proves it.

**Acting on a change request** goes through one door, `ForgeClient::act(number,
&Action)` (`sirio_forge::action`). It reads the change request's id, state and
permissions afresh, refuses in `check_action` — a pure function — whatever the
forge would refuse, and only then sends the mutation: GraphQL on both forges,
and GitLab's approval as REST (`Transport::request`; GitLab has no approve
mutation). The answer is read as a *write's*: GitHub refuses with HTTP 200 and
a `null` payload, GitLab with the reason in the payload's `errors`, and the
interpreter that reads queries would call either a success. Sirio never
retries a write; after any write the tab re-reads the forge instead of patching
its own state, and a connection dropped after the send reads as "could not
confirm". In the UI, `ChangeRequestTab::perform` is the one function the
buttons, the composer and the edit fields call, and the control socket's
`surface change-request act` verb calls it too — **in debug builds only**: a
release build answers "unknown method" and does not list it, so nothing that
writes to a forge is reachable over the socket. Two live tests, the
`…_accepts_every_document_sirio_writes_with` pair, notice a mutation that stops
matching the forge's schema: they send every document with an id that names
nothing.
`docs/superpowers/specs/2026-09-29-change-request-actions-design.md` has the
design; `Scripts/Tests/test-forge-actions-e2e.sh` proves it.

**CI** (slice B2c, spec §15): a check that is a GitHub Actions or GitLab CI
job carries a `CheckJob` (job and run ids, whether the forge would retry it).
*Re-run* and *Re-run failed* are `Action::Rerun` through `act` — GitHub REST,
GitLab `jobRetry`/`pipelineRetry`; GitLab retries per pipeline, so its *Re-run
failed* sits on the pipeline row, not on a stage. `ForgeClient::job_log` reads
the job's status, then its log: GitHub answers with a redirect to a signed
URL on another host, which the token means fetches itself with **no
credential** (`RestRequest.log` turns redirects off for it) and `gh` follows
the same way; `gh api` prints terminal escapes only with
`--allow-escape-sequences`. Only the last 4 MiB are kept, cut at a line, and
the tab says so. `sirio_ui::ansi_log` turns the text into styled lines and
folding groups without a terminal; `CiLogTab` (`TabKind::CiLog`, `"ci_log"`)
draws it in a `uniform_list` and reloads a running job's log every 5 s only
while it is drawn. The `ci` stage of `test-forge-actions-e2e.sh` proves the
re-runs and log state; drawing and visible reload require its framed run.

**Review threads** (B3) are read by `ForgeClient::review_threads` — GitHub
`reviewThreads`, GitLab diff `discussions`, a GitLab thread outdated when its
position names an older head — and drawn and written in the *Files* diff:
`ChangesTab` takes forge-neutral `Annotation`s (`sirio_ui::diff_annotations`)
and one view per key, splits a context band so an anchored line is never
hidden, and puts a thread whose line is not drawn at the top of its file.
`ChangeRequestTab` owns the `ThreadView`s and replaces the Conversation's
line-comment footnotes with one entry per thread. Threads are read on first
load, a manual refresh, entering *Files* when idle and after a write, never
by the CI timer. The `threads` scenarios of `test-forge-diff-e2e.sh` prove it.
B3b writes through the same door: `Reply`, `Resolve` and `LineComment`
(GitHub REST review comments, which publish at once; GitLab REST
`discussions`, a range with `line_range` and GitLab's `line_code`), each
reply or resolve preceded by a fresh read of that thread's permissions.
`ChangesTab` draws a gutter "+" only when its owner calls `set_commentable`,
on the lines within 3 of a change (`diff_annotations::commentable`), and
emits `CommentOn`; the change request tab owns the one composer and every
thread card's fields.
`surface change-request thread --compose / --cancel / --suggest` serve every
build; `act reply|resolve|unresolve|line-comment|edit-thread-comment|review-add|
review-submit|review-discard|review-discard-confirm|draft-edit|draft-delete`
are debug-only. The `threads` stage of `test-forge-actions-e2e.sh` proves the
wire and `scenario_thread_writes` of `test-forge-diff-e2e.sh` the UI.

B3c drafts a review **on the forge**. On GitHub that is the pending review:
`addPullRequestReview` starts it with its first line comment in the same
mutation. On GitLab it is the draft notes, over REST because GraphQL has none,
and `bulk_publish` followed by B2's verdict submits them; a verdict that fails
after the publish is a warning. `ChangeHeader.draft` is read with the header,
and `act` reads it afresh before every review write, so *Start a review* joins a
draft started elsewhere. The strip, the submit dialog and the discard
confirmation live in `change_request_tab/review.rs`, and B2's *Approve* /
*Request changes* submit the draft while one exists. A ```` ```suggestion ````
block draws as a small diff whose "before" lines are the end of the thread's
`diff_hunk` (`change_request_tab/suggestion.rs`); applying stays on the forge.
Outdated threads have a `ThreadView` each, hosted by their file's
`OutdatedView`, so they take writes and keep a reply across a re-read. `act
review-add|review-submit|review-discard|review-discard-confirm|draft-edit|draft-delete`
are debug-only; `thread --suggest` serves every build. The `review` stage of
`test-forge-actions-e2e.sh` proves the wire, and `scenario_review` of
`test-forge-diff-e2e.sh` the UI. The debug-only `review-open-submit` helper
opens the submit dialog without sending, for the framed test.

**C1** checks a change request out into a worktree. `sirio_ui::handoff`
decides, purely, the local branch (`<source_branch>`, `<owner>/<source_branch>`
for a fork, or `pr-<N>`/`mr-<N>` when the head repository is gone), the push
target (the listed remote, the viewer's own fork remote when a project remote
already names the head repository and the viewer may push to it (and the local
branch of the source name is free or tracks that remote), a `sirio-<owner>-<number>` remote when the
viewer may push to a fork, or none — read-only) and whether to reuse, create or
refuse. Each fork change request has its own remote, because a remote's push
mapping is per remote: two change requests sharing one would make a plain
`git push` from either worktree push both branches. The remote is removed with
the last linked worktree, and only when its single push mapping names that
worktree's branch. `ForgeHub::checkout`
(`sirio/src/forge/checkout.rs`) fetches the head through B1's
`RevisionFetcher` first, so nothing is created before a refusal, then runs the
`sirio_git` steps off the GPUI thread. A fork's local `<owner>/<branch>` gets
`remote.sirio-<owner>-<number>.push = refs/heads/<owner>/<branch>:refs/heads/<branch>`,
because a plain `git push` under `push.default=simple` would not reach
`<branch>`. A reused worktree is fast-forwarded only when it is clean and on
the change request's branch; one switched elsewhere, dirty or diverged is left
as it is and the status line says why. A worktree whose folder was deleted
outside git is unregistered (one entry, never a prune) and created again. The
link worktree → change request lives in `sirio_persistence` (`change_request_link`,
v22, keyed by the exact path the sidebar uses, with the branch it was made for;
the card honours it only while the worktree is on that branch), and the card
reads it with `ForgeClient::summary` before the branch match. *Open in a worktree* is in the tab's action bar and the list
row's menu (which opens the tab); the outcome is the tab's checkout status
line. `surface change-request checkout` acts on the open tab and serves every
build because it touches only local git. `test-change-request-handoff-e2e.sh`
proves it.

**C2** hands that worktree to an agent. *Hand off to an agent* (the tab's
action bar and the list row's menu), a thread card's *Fix with agent* and a
failed check's open one Ely dialog (`change_request_tab/handoff.rs`): purpose
(`comments`, `ci` — only when CI failed —, `review`, `resume`), agent or none,
terminal or chat, instructions, and the worktree C1 would use, previewed with
C1's plan in a dry mode. `ForgeClient::context` reads what the purpose needs
**before** git is touched, so a failed or rate-limited read creates nothing;
then C1 checks out and `sirio_ui::handoff::context::render` writes
`.sirio/handoff/<N>-<purpose>-<YYYYMMDD-HHMMSS>.md` (excluded through the common
dir's `info/exclude`, ten kept, a symlinked `.sirio` refused). A worktree C1
reuses on another branch or a detached HEAD refuses the hand-off (a review
excepted); a dirty or diverged one gets a note. Every forge text — bodies,
comments, hunks, paths, names, logs, a log's error — loses its control
characters, shows bidi controls as `<U+…>`, and sits in an
````untrusted id="<nonce>" source="…" author="@…"```` block whose fence is one
backtick longer than the text's longest run (at least four), followed by
Sirio's line `(end of untrusted block <nonce>)`; the nonce is random per file,
so forge text cannot fake a block's end even to a model that miscounts. Sirio's
own lines name no forge text, not even the branch to push to, and a link stands
outside a block only as printable ASCII. The user's instructions section is
always written. The file is cut at 1 MiB, oldest first, and every part that
cannot be dropped is bounded. `prepare` and `install_skill` read and write the
worktree only through `sirio_agents`' `safe_write`, which refuses a committed
symlink anywhere on the path. A terminal agent is
launched as a restored pane is, `prepare` then `command_with_prompt` (every CLI
takes the two-line prompt as an argument; Windows folds it to one line for
`cmd`), and `TerminalView::start` runs it before its tab is drawn; a chat gets
the prompt through `Chat::send_when_ready`. The agent starts in the hand-off's
worktree even if the user switched meanwhile. Agent and surface are remembered
per project (`handoff_choice`, v23). `surface change-request handoff` serves
every build and acts on the open tab; a start sent while the preview loads is
queued (`handoff_queued`). The C2 scenarios of
`test-change-request-handoff-e2e.sh` prove it with stub CLIs on PATH, and
`sirio_agents/tests/initial_prompt_live.rs` notices the day a CLI stops taking
the prompt.

**Merge, reviewers and labels** (B2b) go through the same door.
`Action::Merge` carries the head the user saw when the confirmation opened, and
`act` refuses `HeadMoved` before sending, ahead of the forge's own
`expectedHeadOid` / `sha` guard. The strip is drawn from `MergeCapability`,
whose `Unreported` verdict draws nothing. GitHub deletes the branch with a REST
`DELETE` after a successful merge, never for a fork's head; if only that call
fails, the outcome is a merge with a warning. GitLab deletes it with a flag of
the accept, offers no rebase (its method is per project), and cancels an
auto-merge over REST, since GraphQL has no mutation for it. A reviewer or label
picker sends one `Set*` when its popover closes; that relies on two vendored
Ely hooks, `Popover::on_close` and `Popover::open` (`LOCAL-CHANGES.md`). The
`merge`, `metadata` and `ui` stages of `test-forge-actions-e2e.sh` prove it.
