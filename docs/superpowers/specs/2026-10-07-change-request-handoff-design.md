# A change request into a worktree and an agent — design

**Date:** 2026-10-07
**Status:** proposed
**Programme:** part **C** of **B1 → B2 → B3 → C**. A reads the forge, B1
brought the diff into Sirio, B2 acts on a change request from its tab, B3
reviews it in the diff, and **C (this document)** takes a change request into a
worktree and hands it to an agent.
**Scope of this document:** check a GitHub pull request or a GitLab merge
request out into a worktree that pushes back to it (forks included), hand it
to any of Sirio's five agents with a context written for one of four
purposes, and let that agent read the change request's live state from Sirio.
Built as three slices (§11), each with its own plan and pull request. The UI is
on Ely only (`2026-10-03-change-requests-on-ely-design.md`).

## §0 Intent

After B3 the user reads, reviews and acts on a change request from Sirio, but
the work it asks for still starts by hand: find or create a worktree on the
right branch, wire a fork's remote, start an agent, and paste into it the
threads, the failing job's log or the description. Sirio runs agents in
worktrees and already holds all of that, so it should do the wiring.

The user wants this for four purposes, all four from the first release:

- **Fix the review comments** — the agent receives the open threads and
  changes the code they ask for.
- **Fix the failing CI** — the agent receives the failed jobs and their logs.
- **Review** — someone else's change request, often from a fork: the agent
  analyses it and reports, without changing it.
- **Resume** — a change request of the user's own (or of one of their agents)
  whose branch is in no worktree: an agent carries on with it.

Success: from a change request's tab, a row of the list, a thread card or a
red check, the user picks a purpose, an agent and a surface, and an agent
starts in a worktree on the change request's branch, already reading what it
needs, in a worktree whose `git push` lands on the change request — or,
when the forge refuses that, in a worktree that says it is read-only.

## §1 Decisions taken

Settled with the user before this document was written:

| Question | Decision |
|---|---|
| Purposes | All four: fix comments, fix CI, review, resume. |
| Agent and surface | **The user chooses**: any of the five adapters, terminal pane or chat tab (chat only where the adapter has a chat transport), or no agent at all. |
| Push | The worktree's branch **tracks the change request's branch**. For a fork Sirio adds a remote for the fork when the forge lets the maintainer push; otherwise the worktree is read-only and says so. Whoever works in the worktree pushes with git. Sirio has no push button. |
| Entry points | The change request tab, a row of the right-panel list, a thread card (that thread only), a failed check (that job only). |
| Untrusted text | Everything that comes from the forge is fenced as **data, not instructions**, and the dialog warns before handing over a change request the viewer did not author or that comes from a fork. No other restriction. |
| How the context reaches the agent | **A + C**: a context file written into the worktree plus a short launch prompt (A), and read-only control-socket verbs through which the agent reads the live state (C). |

## §2 Where it stops today

- `sirio_git::create_worktree(repo, branch, path, base)` creates a new branch
  or checks out an existing local one. It never starts from a remote branch,
  never sets an upstream, and knows nothing of forks.
- B1's `RevisionFetcher::ensure` (`sirio/src/forge.rs`) fetches any change
  request's head, forks included, into
  `refs/sirio/change-requests/<remote>/<N>/head` — a ref to read, not a branch
  to push from, and swept when no tab holds it.
- `sirio_forge` exposes `source_branch` and `source_owner` but not the head
  repository's clone URLs nor whether the maintainer may push to it.
  `isCrossRepository` is read only into the internal `ActionContext`.
- An agent starts from `AgentAdapter::command` (Claude's is `claude`) in a
  terminal tab, or from `add_chat_tab`; neither takes a prompt. The only
  prompt-at-launch route is `panel.create` with a raw command, which skips
  `prepare` and its hooks.
- The right panel pins "this worktree's change request" by the current branch
  name, filtered by `source_owner` when `origin` differs from the listed
  remote. A fork branch checked out under another name, or a common name such
  as `patch-1`, pins the wrong change request or none.

## §3 Vocabulary

- **Hand-off** — checking a change request out into a worktree and, unless
  the user chose no agent, starting an agent there with a context.
- **Purpose** — one of `comments`, `ci`, `review`, `resume`. A thread card
  narrows `comments` to one thread; a failed check narrows `ci` to one job.
- **Link** — the persisted association between a worktree and a change
  request (`forge`, `host`, `project`, `number`).
- **Push target** — where the worktree's branch pushes: the listed remote, a
  fork remote Sirio added, or none (read-only).
- **Context file** — the Markdown file a hand-off writes under
  `.sirio/handoff/` in the worktree.
- **Untrusted block** — a fenced block holding text that came from the forge.

## §4 Architecture

### `sirio_forge`

- New fields on `ChangeHeader`, read with the header on both forges:
  - `head: HeadRepository { owner, project, http_url, ssh_url,
    cross_repository, maintainer_can_push }` — GitHub `headRepository
    { url sshUrl nameWithOwner }`, `headRepositoryOwner { login }`,
    `isCrossRepository`, `maintainerCanModify`; GitLab `sourceProject
    { fullPath httpUrlToRepo sshUrlToRepo }`, `allowCollaboration`, cross when
    `sourceProjectId != targetProjectId`. `None` when the head repository is
    gone (a deleted fork).
  - `viewer_is_author: bool` — the viewer's login (GitHub `viewer { login }`,
    GitLab `currentUser { username }`) compared with the author's.
- `ForgeClient::context(number, Scope) -> Context`, the read that both the
  context file and the live verbs render: the header, and for `comments` the
  review threads (B3's read), for `ci` the failed checks and each job's log
  (B2c's `job_log`), for `resume` the timeline's last 20 entries. `Scope`
  narrows to one thread or one job. It is a read, so it goes through the
  rate-limit pause like every other read.

### `sirio_git`

- `fetch_branch(repo, remote, branch)` — `git fetch <remote>
  +refs/heads/<branch>:refs/remotes/<remote>/<branch>`, non-interactive, with
  git's own credentials (the B1 rule: never Sirio's token).
- `ensure_remote(repo, name, url) -> RemoteOutcome` — adds the remote, or
  accepts an existing one with the same URL, or refuses one with a different
  URL.
- `create_worktree_tracking(repo, branch, path, upstream)` —
  `git worktree add --track -b <branch> <path> <upstream>`.
- `create_worktree_at(repo, branch, path, commit)` — a branch with no
  upstream, for the read-only case.
- `fast_forward(worktree, upstream)` — `git merge --ff-only`, only on a clean
  worktree.
- `exclude(repo, pattern)` — appends to `$GIT_COMMON_DIR/info/exclude` once.

### `sirio_ui`

- `handoff::plan(facts) -> HandoffPlan` — a pure function from what git and
  the forge report to one decision (§5). Nothing touches disk until the plan
  is decided and shown.
- `handoff::render(context, purpose, push_target) -> String` — the context
  file's Markdown (§6), pure.
- `change_request_tab/handoff.rs` — the dialog, an Ely `Dialog`, opened from
  the action bar, a thread card's *Fix with agent* and a failed check's *Fix
  with agent*. The list row's context menu opens the same dialog without the
  tab.
- `ChangeRequestTabEvent::Handoff(HandoffRequest)` and the list's matching
  event carry the request to the host.

### `sirio_agents`

- `AgentAdapter::command_with_prompt(&self, prompt: &str) -> PromptLaunch`:
  `Argument(String)` when the CLI takes an initial prompt on its command line,
  or `TypeAfterStart` when it does not, in which case the host writes the
  prompt into the pane once the CLI is ready. Every adapter states its form;
  §13 lists what the plan verifies against the real CLIs.

### `sirio` (host)

- `ForgeHub` serves the hand-off: it reads the facts, asks `handoff::plan`,
  runs the git steps off the GPUI thread, records the link, inserts and
  selects the sidebar row, writes the context file, and opens the agent with
  `prepare` and the prompt — a terminal tab through the existing
  `add_terminal_tab_with_shell_and_agent`, or a chat tab whose first message
  is the prompt, sent the way `surface.chat.send` sends one.
- The link lives in `sirio_persistence` next to the worktree's session and is
  dropped when the worktree is removed. The right panel's pinned card reads
  the link first and falls back to the branch-name match.

## §5 From a change request to a worktree (C1)

`handoff::plan` decides in this order; the first rule that applies wins.

1. **A linked worktree exists** for this change request → reuse it.
2. **A worktree has the expected branch checked out** (below) and its
   upstream is the push target → reuse it and record the link.
3. Otherwise **create** one, in the directory the sidebar's New Worktree
   dialog would choose (`derive_worktree_path`).

Reusing: Sirio fetches, then fast-forwards a clean worktree that is behind.
A worktree with uncommitted changes, or one that has diverged, is left as it
is and the dialog says which. A read-only worktree whose fork has since begun
to accept pushes gets its remote, its upstream and its push mapping on reuse.
A worktree whose folder was deleted outside git is forgotten (its one git
registration is removed) and created again. When the last worktree linked to a
change request is removed from the sidebar, its `sirio-…-<number>` remote goes
with it.

The expected branch and push target:

| Change request | Local branch | Push target |
|---|---|---|
| Same repository, source branch on the forge | `<source_branch>` | `<listed remote>/<source_branch>` |
| The viewer's own fork is a remote of the project (`origin`, usually), the viewer may push to the head, and the local `<source_branch>` is absent or already tracks `<remote>/<source_branch>` | `<source_branch>` | that remote, `<source_branch>` — tracks `<remote>/<source_branch>` as in the same repository |
| Fork, maintainer may push | `<owner>/<source_branch>` | remote `sirio-<owner>-<number>` (one per change request), URL in the listed remote's scheme (ssh or https), with exactly one push mapping |
| Fork, maintainer may not push | `<owner>/<source_branch>` | none — read-only |
| Source branch gone | `<owner>/<source_branch>` for a fork, `<source_branch>` otherwise | none — read-only, from the forge's head ref |
| Head repository deleted | `pr-<number>` (GitHub) or `mr-<number>` (GitLab) | none — read-only, from the forge's head ref |

A remote Sirio made for one change request is never the viewer's own fork: the
own-fork row only looks at remotes Sirio did not name `sirio-…`.

A read-only worktree starts from the head commit B1 already knows how to fetch
(`refs/pull/N/head`, `refs/merge-requests/N/head`).

Refusals, all before anything is created, each with its reason in the dialog:
a fetch that fails; a local branch of the expected name that points elsewhere
and is not an ancestor of the head; a `sirio-<owner>-<number>` remote with
another URL; a target directory that already exists. No existing branch is ever
rewritten and no remote Sirio did not make is ever changed. Sirio's own fork
remotes are added (and, with their last worktree, removed) per change request;
the flip to a pushable fork adds the mapping to the one remote of its request.

## §6 The hand-off and its context (C2)

**The dialog**

- *Purpose*: Fix review comments, Fix failing CI, Review, Resume. A thread
  card fixes it to that thread, a failed check to that job.
- *Agent*: the five adapters and *No agent*.
- *Surface*: Terminal or Chat; Chat is disabled for an adapter with no chat
  transport. Agent and surface are remembered per project.
- *Instructions*: optional free text, appended to the context file as the
  user's (trusted) words.
- *Worktree*: one line saying what §5 decided — "reuse `sirio-fix-login`",
  "create `../sirio-feature-x`", "read-only: the fork does not accept pushes".
- *Warning*, when `viewer_is_author` is false or the head is cross-repository:
  the change request's text reaches the agent as data, and the agent runs with
  the user's permissions.
- *Start*. The dialog stays open with the reason when a refusal stops it.

**The context file** — `.sirio/handoff/<N>-<purpose>-<YYYYMMDD-HHMMSS>.md`;
`.sirio/handoff/` goes into the repository's `info/exclude`; the ten newest per
worktree are kept.

Sirio's own text is outside any fence: the task for the purpose, the change
request's number, URL and head sha at hand-off time, where to push (or "do not
push: read-only"), and the `sirioctl` commands of §7 for reading the live
state. Everything from the forge — titles, descriptions, comments, diff hunks,
log lines, author names — is inside an untrusted block:

    ````untrusted source="review thread" author="@someone"
    …text exactly as the forge returned it…
    ````

The fence is one backtick longer than the longest run of backticks in the
text, so no text can close its block early. Terminal escapes are stripped from
logs before they are written.

Per purpose:

- **comments** — the unresolved threads, current before outdated; for each,
  path, line and side, the end of its diff hunk, every comment with its
  author, and any `suggestion` block. The viewer's pending draft comments are
  left out.
- **ci** — each failed job: name, stage, URL, the first error group
  `ansi_log` finds, and the last 2,000 lines of the log, cut at a line.
- **review** — the description, the commits, the changed files with their
  counts, and the base and head shas; the task says to report and change
  nothing. The diff is not copied: the agent runs `git diff base...head`.
- **resume** — the description, the timeline's last 20 entries, the CI state
  and the number of open threads.

**The launch prompt** — two lines: read the context file and do the task it
describes. A terminal agent gets it through `command_with_prompt` after
`prepare`, so hooks and activity detection work as for any agent Sirio starts;
a chat gets it as its first message.

## §7 Reading the live state (C3)

`surface.change_request.context` with `number` and `purpose`, optionally
`thread` or `job`, returns the same Markdown the context file holds, read
afresh through `ForgeClient::context`. `sirioctl change-request context <N>
--purpose comments|ci|review|resume [--thread ID] [--job ID]` prints it.

These verbs only read and are served by every build, like
`surface.change_request.read`. Nothing in C writes to a forge over the socket:
B2's write verbs stay debug-only. The socket's existing peer check (same user)
still applies.

## §8 Socket

- `surface.change_request.checkout {number}` — §5 with no agent; every build,
  since it touches only local git, like `workspace.create`.
- `surface.change_request.handoff {number, purpose, agent, surface,
  thread?, job?, instructions?}` — the whole hand-off; every build, since it
  grants nothing `panel.create` does not.
- `surface.change_request.handoff --open-dialog` — opens the dialog without
  starting, for the framed test.
- `surface.change_request.context` — §7.
- `surface.change_request.read` gains the link and the last hand-off's
  outcome.

## §9 Errors

- A git step that fails after the plan was decided (a racing branch, a full
  disk) stops the hand-off where it is; the dialog names the step. Nothing
  already created is deleted, so the user can see and finish it.
- A forge read that fails or is rate-limited stops the hand-off before git is
  touched.
- An agent whose CLI is not installed: the dialog disables it, with the
  missing program named, as Settings → Agents does.
- A context bigger than 1 MiB is cut, oldest material first, with a note at
  the top saying what was left out.

## §10 Testing

- **Unit, written first** (pure units only):
  - `handoff::render` — an untrusted text containing a run of backticks
    longer than the default fence stays inside its block; terminal escapes are
    gone; the log is cut at a line within the limit; pending drafts are
    absent; the 1 MiB cut drops the oldest material and says so.
  - `handoff::plan` — reuse by link, reuse by branch, create, fork that
    accepts pushes, fork that does not, deleted head repository, branch that
    points elsewhere, remote with another URL, directory taken, dirty and
    diverged worktrees.
  - Branch and remote naming, including owners and branches with characters
    git refuses in a remote name.
- **E2E** — `Scripts/Tests/test-change-request-handoff-e2e.sh` → `HANDOFF E2E
  OK`, against a bare repository, a bare "fork" and the fake forge, in a real,
  isolated Sirio over the control socket; `--state-only` skips captures,
  `--out-dir` keeps the transcript, every repository's state, the context
  files and the captures.
  - C1: same-repository checkout, and a `git push` from it reaching the bare;
    a fork that accepts pushes, and a push reaching the fork's bare; a fork
    that refuses, read-only and with no upstream; a closed change request;
    reuse of a clean worktree (fast-forwarded) and of a dirty one (untouched);
    a name collision refused; the pinned card found through the link.
  - C2: stub `claude`, `codex`, `opencode`, `pi` and `omp` on `PATH` that
    record their arguments; each adapter's command line and the context file
    for each purpose are asserted; a chat's first message reaches the fake ACP
    agent; the warning shows for a fork.
  - C3: every `context` purpose against the file written at hand-off.
- **Live** — `forge_live.rs` reads the new header fields on both forges;
  one test per CLI checks that the installed version accepts its initial
  prompt, and prints `SKIP:` when the CLI is absent, like the agent
  conformance tests.

## §11 Slices

- **C1 — a change request into a worktree**: the head repository fields,
  the `sirio_git` steps, `handoff::plan`, the link and the pinned card, *Open
  in a worktree* in the tab's action bar and the list row's menu,
  `surface.change_request.checkout`.
- **C2 — the hand-off**: the dialog, the four purposes and `handoff::render`,
  the context files, `command_with_prompt` for the five adapters, the chat's
  first message, *Fix with agent* on thread cards and failed checks,
  `surface.change_request.handoff`.
- **C3 — the live read**: `ForgeClient::context` served over the socket and
  `sirioctl change-request context`.

## §12 Out of scope

- A push button, or any push by Sirio.
- Agents writing to the forge through Sirio: replying to, resolving or
  submitting from an agent stays the user's act in the tab.
- Keeping the context file in step with the forge (C3 is how an agent reads
  newer state).
- Removing a worktree when its change request is merged or closed.
- Opening several agents on one change request at once from one dialog.

## §13 Verified in the plan, with the fallback already decided

- Each CLI's initial-prompt form (`claude "<p>"`, `codex "<p>"`, `opencode
  --prompt "<p>"`, `pi "<p>"`, `omp "<p>"`) against the installed versions.
  Fallback: `TypeAfterStart`.
- `maintainerCanModify`, `headRepository { sshUrl }` and `isCrossRepository`
  on github.com, `allowCollaboration` and `sourceProject { sshUrlToRepo }` on
  gitlab.com, by introspection. Fallback for a field the forge lacks: treat
  the fork as read-only.
- That GitHub keeps `refs/pull/N/head` and GitLab `refs/merge-requests/N/head`
  for a closed change request whose fork was deleted. Fallback: the dialog
  says the head is gone and creates nothing.

## §14 Revised while planning and building C1 (2026-10-07)

*Facts verified live while planning* (read-only introspection, 2026-10-07):
github.com `PullRequest` has `maintainerCanModify`, `isCrossRepository`,
`headRepository`, `headRepositoryOwner`, `headRef`; `Repository` has `sshUrl`,
`url`, `nameWithOwner`, `viewerPermission`; gitlab.com `MergeRequest` has
`allowCollaboration`, `sourceProject`, `targetProject`, `sourceProjectId`,
`targetProjectId`, `sourceBranchExists`; `Project` has `fullPath`,
`sshUrlToRepo`, `httpUrlToRepo`, `userPermissions { pushCode }`.

*Revisions:*

- `maintainer_can_push` became `can_push`. GitHub: write access to the head
  repository, or `maintainerCanModify` and write access to the change
  request's repository. GitLab: `pushCode` on the source project, or
  `allowCollaboration` and `pushCode` on the target project —
  `allowCollaboration` admits only members of the target project who may
  merge, so it says nothing about a viewer who is not one.
- `branch_exists` was added (GitHub `headRef`, GitLab `sourceBranchExists`).
- A fork owner's `/` and other characters fold to `-` in the remote name, and
  the worktree directory flattens the branch's `/`. Two owners that fold to
  the same name (`forks/alice`, `forks-alice`) meet a remote whose URL differs,
  and the checkout refuses rather than pushing to the wrong fork.
- C1 has no dialog: the outcome is the tab's checkout status line, and the
  list's menu item opens the tab first.
- The card needed `ForgeClient::summary(number)`.
- A fork's `<owner>/<branch>` cannot push to `<branch>` with a plain
  `git push` under `push.default=simple`, so Sirio adds
  `remote.sirio-<owner>.push = refs/heads/<owner>/<branch>:refs/heads/<branch>`.
- The link is reported by the list's read (`linked`) and the checkout outcome
  by the tab's (`checkout`, `checkout_detail`), not both on
  `surface.change_request.read`.
- `surface.change_request.checkout` takes no `{number}` (§8 said it would): it
  acts on the open change request tab, like `surface.change_request.read` and
  `act`. The list's menu item opens the tab first, so both entry points share
  one path.
- The head is fetched through B1's `RevisionFetcher` before deciding, so
  refusals that depend on ancestry come before anything is created.
- `viewer_is_author` moves to C2, the first slice that uses it.

*Rulings from the slice ledger:*

- When the head repository is deleted, the forge drops its owner too (GitHub
  `headRepository` and `headRepositoryOwner` are null, GitLab `sourceProject`
  is null), so §5's `<owner>/<source_branch>` cannot be built. The local
  branch is the bare `<source_branch>`; when a branch of that name already
  points at commits the head does not reach, the checkout refuses instead of
  touching it.
- Reusing a worktree fetches, then fast-forwards only the branch the worktree
  has checked out and only when it is the change request's branch. A worktree
  switched to another branch is left as it is, and §5's "no existing branch is
  ever rewritten" holds. A read-only worktree is fast-forwarded to the change
  request's head.
- For an existing local branch that needs an upstream, the upstream is set
  before the worktree is created, so a failure leaves nothing a retry would
  refuse. A failure after creation still refreshes the sidebar, so the user
  sees what was made (§9).
- The new GitLab header fields are in the baseline queries too: they predate
  every field the baseline exists to omit (GitLab 12–13 against 15–16).
- The link is keyed by the exact path string the sidebar uses, with no
  canonicalisation; the checkout saves it under the catalog row's path.
- Ruling: C-1 — a fork's remote is `sirio-<owner>-<number>`, carrying exactly one
  push mapping, because `remote.<name>.push` set on a shared remote makes a
  plain `git push` from any worktree push every mapped branch (and force-push
  them). It is removed when its last linked worktree leaves the sidebar, found
  by the `sirio-` prefix and `-<number>` suffix (the link stores no owner; a
  number is unique within one repository). Cost: one remote per checked-out fork
  change request.
- Ruling: I-1 — a read-only reuse of a fork that now accepts pushes adds the
  remote, fetches, sets the upstream (only on the change request's branch and
  only when it has none), then writes the mapping. Cost: none.
- Ruling: I-2 — a worktree git lists whose folder is missing is unregistered with
  `git worktree remove --force` on that one path, never `git worktree prune`,
  which would touch every worktree of the repository. A branch read that fails
  is a failure; a detached HEAD says so ("it is on a detached HEAD"). Cost: one
  git call.
- Ruling: I-3 — the link stores the local branch (v22, `change_request_link.branch`),
  and `connect` honours it only while the worktree is on that branch. A later
  checkout of the same worktree saves the link again with the change request's
  branch, so the link is back in force once the worktree returns to it. Cost:
  one column.
- Ruling: I-4 — the viewer's own fork is found among the project's remotes
  (other than `sirio-…`) by comparing normalised URLs: scheme, user, host case,
  a trailing `/` and `.git` do not count. Its branch is `<source_branch>` and
  it is the push target, even when it does not accept pushes from the viewer
  (git reports that itself). Cost: the normalisation code.
- Ruling: M-1 — the GitHub fork-push fixture gives the viewer READ on the head
  and sets `maintainerCanModify`, so the maintainer grant is what makes it
  pushable; the read-only fixture is the one that withholds it.
- Ruling: M-2 — when the head repository is gone the local branch is `pr-<N>` /
  `mr-<N>`, never the bare source branch (this overturns the earlier bare-name
  ruling). Cost: a name the user did not pick.
- Ruling: M-3 — the "branch has commits the change request does not" refusal
  says to check the branch out or push it, and never to delete it. Cost: none.
- A linked read that fails falls back to the branch match, and its rate-limit
  error still pauses every read (§9). On a detached HEAD a worktree has no card:
  a link is honoured only while the worktree is on its branch (I-3), so the
  linked-on-detached arm of the card is not reached. The card's loading and
  error states are drawn for a linked worktree on its branch.
- `ensure_remote` compares the configured URL (`remote.<name>.url`), not the
  one `git remote get-url` returns after `url.<base>.insteadOf`, so a rewrite
  never reads as a conflicting remote.

## §15 Revised while planning and building C2 (2026-10-08)

*Facts verified while planning* (the installed CLIs' `--help`, 2026-10-08):

| Agent | Version | Initial prompt |
|---|---|---|
| Claude Code | 2.1.293 | `claude [options] [command] [prompt]` |
| Codex | 0.159.2 | `codex [OPTIONS] [PROMPT]` |
| OpenCode | 1.18.33 | `--prompt <prompt>` |
| Pi | 1.0.0 | `pi [options] [--] [@files...] [messages...]` |
| Oh-My-Pi | 18.4.4 | `omp "<prompt>"` |

Every adapter takes the prompt as an argument, so §4's "type it after start"
fallback has no implementer: `AgentAdapter::command_with_prompt` is a required
method returning the command line, and `sirio_agents/tests/initial_prompt_live.rs`
notices the day a CLI stops listing it (it SKIPs without the binary and is off
both gates).

*Revisions decided while planning:*

- `command_with_prompt` takes `command`'s arguments plus the prompt, because
  Codex and omp build their command line from the pane and the worktree.
- `surface.change_request.handoff` acts on the open change request tab and
  takes no `{number}`, like C1's `checkout`; the list's *Hand off to an agent…*
  opens the tab first.
- The context is read before the checkout (§9: a failed or rate-limited read
  touches no git); the checkout then reads the header again.
- *No agent* still checks the change request out and writes the context file.
- The per-project choice lives in a v23 table `handoff_choice(project_id,
  agent, surface)`, not in `ProjectRecord`, whose literals span crates.
- `viewer_is_author` compares `ForgeClient::viewer()` with the summary's author,
  case-insensitively; it is read during the dialog's preview. A viewer read that
  fails for any reason but a rate limit leaves it unknown.
- *Fix failing CI* is offered only when the summary's CI state is failed. The
  default purpose is `ci` when CI failed, else `review` when the viewer is known
  not to be the author, else `resume`.
- C2's context file names no `sirioctl change-request context` command; that
  verb is C3.
- A thread card's *Fix with agent* scopes `comments` to that thread even when it
  is resolved.
- `prepare` failing does not stop the agent; the status line says the hooks were
  not written.

*Rulings from the slice ledger:*

- Windows runs a pane's command through `cmd /C`, which ends a command at a line
  break, so there the launch prompt is folded to one line
  (`shell_quote::one_line`); elsewhere it is passed as written. Cost: a Windows
  agent reads the two lines as one.
- `ForgeClient::context` returns every review thread; the renderer narrows a
  `Scope::Thread` to that one thread (resolved or not) and otherwise keeps the
  unresolved ones. A resume reads nothing extra: the description, the timeline
  and the CI state come with the header.
- A job log that could not be read carries its reason, which may be `gh` or
  `glab` stderr, so the reason is written inside an untrusted block
  (`source="log error"`), never as Sirio's words.
- The 1 MiB bound holds for every input. The droppable parts (threads, jobs,
  timeline entries, the commit and file lists, the description, the title) go
  oldest first, and a job too large on its own has its log halved and is then
  dropped. What is never dropped is bounded: a link is written only when it is
  `http(s)`, at most 2048 bytes and free of whitespace, backticks and control
  characters; a sha only when it is 40 or 64 hex digits; the user's
  instructions are cut at 64 KiB with a note; the title at 1 KiB and the
  description at 256 KiB, each with a note.
- The push line names no forge text: "Push with a plain `git push`: this
  worktree's branch is set up to push to the change request's branch." C1 has
  already set the upstream or the push mapping, and a branch name is
  forge-controlled text that would otherwise sit outside a fence. A read-only
  worktree says "Do not push", with Sirio's own reason.
- `.sirio` and `.sirio/handoff` are refused when either is not a real directory
  (a branch can commit a symlink there); the file is refused when a symlink
  stands at its name; pruning removes only regular `*.md` files directly in
  the folder, the ones beyond the ten newest.
- Failing to add `.sirio/handoff/` to `info/exclude` fails the hand-off, after
  the file is written, and the message says where the file is: an agent could
  otherwise commit it.
- The dialog's preview runs C1's plan with a dry flag: it neither drops a
  missing link nor unregisters a missing worktree, and it may fetch the head
  into `refs/sirio/change-requests/…`, as *Files* does. Each preview carries the
  generation of the open that asked for it, so a late one cannot overwrite a
  newer dialog's.
- Without an agent the surface is the terminal, in the dialog, the socket and
  the remembered choice; reopening the dialog clears the last hand-off's outcome.
  Ely's `RadioGroup` draws no `Choice::note`, so each disabled choice's reason is
  written beside its group.
- A chat's first message waits for the connection (`Chat::send_when_ready`):
  it goes straight to the turn, leaving whatever the user typed meanwhile in the
  composer; while a turn runs it joins the send queue; when the connection
  fails it is put in the composer above what was typed.
- A socket start sent while the preview is loading is queued and runs when the
  preview lands (`handoff_queued`); a refused preview fails it. A socket start
  whose scope differs from the open dialog's is refused rather than retargeted.
- The hand-off's agent starts in the hand-off's worktree, carried with the
  queued action; if that worktree cannot be selected, nothing starts elsewhere.
- A terminal pane starts its process on first draw, so the hand-off's agent
  would not run while its tab was undrawn (another worktree, a hidden window).
  `TerminalView::start` starts it at once; every other pane stays lazy. Its
  first draw resizes the PTY from 80x24.
- A chat refused for an agent (Pi has no chat transport) is reported as not
  started, never as started.
