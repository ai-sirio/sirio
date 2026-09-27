# Pull requests and merge requests in the right panel — design

**Date:** 2026-09-27
**Status:** proposed
**Programme:** part **A** of three. A (this document) — forge layer, the
right-panel list and the read-only detail tab. C — take a change request into
a worktree and hand it to an agent. B — review inside Sirio (diff, inline
threads, comment, approve). Built in that order, each with its own spec.
**Scope of this document:** read GitHub pull requests and GitLab merge
requests — github.com, GitHub Enterprise Server, gitlab.com and self-managed
GitLab — list them in a fifth right-panel view, and open one read-only in a
tab in the Secondary half of the centre split.

## §0 Intent

The user runs agents in worktrees and wants to know, without leaving Sirio,
where the work those agents produced stands on the forge — is the pull
request for this worktree green, red, or waiting on a review — and to look
across the repository's open pull requests, including the ones waiting on
their own review. Parts C and B build on the same layer to act on what A only
shows.

Success: on a GitHub repository and on a self-managed GitLab, the right panel
lists open change requests, the current worktree's own is pinned above them
with its CI and review state, and a click opens its description, checks,
reviews and conversation in the Secondary half — authenticated by whichever
means the user chose, the forge's CLI or a personal token.

## §1 Decisions taken

Settled with the user before this document was written:

| Question | Decision |
|---|---|
| Scope | All four jobs — status of my work, repo overview, in-app review, change request → agent — **split into three sub-projects, A → C → B**. This document is A: read-only. |
| Forges | GitHub (github.com and Enterprise Server) and GitLab (gitlab.com and self-managed), from the first release. |
| Authentication means | **The user chooses per host**: the forge's CLI (`gh`, `glab`) or a personal access token. |
| How a host is configured | **Detected automatically**; the result is only the default and Settings can override it. |
| Where the status shows | **Only in the PR view** and the detail tab. No badges on sidebar rows, no notifications. |
| List layout | Mockup **C**: worktree card, then filters as **underlined icon tabs** (the active one shows its label), then two-line rows. |
| Filters | *Mine*, *To review*, *All open*, *Closed & merged* — **icons**, with tooltips. |
| Detail layout | Mockup **A**: fixed header, then inner tabs *Conversation / Commits / Checks / Files*, like the forges themselves. |
| Forge layer shape | **One API layer over two transports** (approach 1): requests built and responses parsed once per forge; the transport is `gh api` / `glab api` or HTTP with a token. |

## §2 Where it stops today

- Sirio has no forge integration. `sirio_git::GitRemote::github_owner_from_url`
  (`sirio_git/src/remote.rs`) recognises `github.com` and nothing else; it
  feeds the project avatar.
- `RightPanel` (`sirio_ui/src/right_panel/mod.rs`) has four views —
  `PanelView::{Files, Diff, History, References}` — in an icon rail. The
  selection is an app-wide GPUI global, because `select_worktree` rebuilds the
  panel entity.
- The panel names what to open and the host owns the tab:
  `RightPanelActionEvent::OpenCommit(sha)` reaches
  `SirioWorkspace::add_commit_tab` (`sirio/src/main.rs`), which pushes an
  `OpenTab` with `TabContent::Changes`, `TabKind::Diff.default_pane()` and
  calls `open_secondary_pane()`.
- Per-kind tab payloads persist in `SessionTabState` (`sirio/src/session.rs`)
  as `#[serde(default)]` fields — `browser_url`, the editor's file — and the
  kind as the string `TabRecord::kind`.
- Secrets: `sirio_usage/src/credentials.rs` is a plaintext JSON store, mode
  `0600`, overridable with `SIRIO_CREDENTIALS`. The OS keyring was considered
  and **deliberately rejected** (unattended test runs mutating the login
  keyring, unlock prompts blocking headless sessions, no hermetic fixture).
- `ureq` 3.4 is already a dependency (`sirio_update`, `sirio_registry`,
  `sirio_diagram`), with its default root store: Mozilla's
  (`RootCerts::WebPki`), not the system's.
- The process PATH is merged with the login shell's at startup
  (`sirio/src/login_path.rs`), so `gh` and `glab` resolve as they would in a
  terminal.
- `glab` has no command that prints a token cleanly (only
  `auth status --show-token`, human-readable) and stores it in the OS keyring
  by default; both `gh api` and `glab api` accept `--hostname`, `--include`
  and GraphQL.

## §3 Vocabulary

**Change request** is the code's neutral word for a GitHub pull request and a
GitLab merge request alike. The UI keeps each forge's own words: *PR* and
`#578` on GitHub, *MR* and `!231` on GitLab. The term is added to
`CONTEXT.md` under a new *Forges* heading, together with **forge** (a hosting
service instance, identified by its host) and **means** (how Sirio
authenticates to a forge: CLI or token).

## §4 Architecture

### `sirio_forge` — new leaf crate

No GPUI, no local dependencies (bar `sirio_perf`, like the other leaves).
Modules:

- `target` — from a remote URL to `ForgeTarget { host, project_path }`: scp
  form, `ssh://`, `https://`, ports, a trailing `.git`, and GitLab subgroups
  (`group/sub/project`). It generalises `github_owner_from_url`, which stays
  where it is for its current caller.
- `transport` — `trait Transport { fn send(&self, ApiRequest) ->
  Result<ApiResponse, ForgeError> }`, where a response is **status + headers +
  body**. Two implementations:
  - `CliTransport { program: Gh | Glab, host }` runs
    `gh api --hostname H --include …` / `glab api --hostname H --include …`,
    body on stdin. The CLI owns authentication, its keyring, OAuth refresh and
    SSO.
  - `TokenTransport { base_url, token }` uses `ureq` with the
    **`platform-verifier`** feature and `RootCerts::PlatformVerifier`, so a
    corporate CA installed in the system trust store is honoured. This applies
    to `sirio_forge`'s client only; the updater, registry and diagram clients
    keep WebPki.
  - The token is passed in by the caller. `sirio_forge` never reads, stores or
    logs it.
- `github` / `gitlab` — request builders and response parsers, pure
  functions.
- `resolve` — the detection chain of §5.
- `model` — the types of §6.
- `ForgeClient` — `Send + Sync`, holds one resolved target, forge and
  transport, and exposes the reads of §6. It is what the host hands to the UI.

### `sirio_ui`

- `right_panel/change_requests.rs` — the fifth view (§7.1), built lazily the
  first time it is shown and dropped when the checkout changes, like History
  and References.
- `change_request_tab.rs` — the detail tab (§7.2).
- Both call a `ForgeClient` supplied by the host, on the background executor,
  guarded by a generation counter so a late response never overwrites newer
  data (the pattern `RightPanel::refresh_generation` already follows).
- `settings` — a new page, *Git hosting* (§5.3).

### `sirio` (host)

- Reads the remotes through `sirio_git`, prefers `upstream` over `origin`,
  runs `sirio_forge::resolve` off the main thread, once per host per session,
  reads the token (if any) from the credential store, and builds the
  `ForgeClient` it pushes into the panel.
- Handles `RightPanelActionEvent::OpenChangeRequest(ChangeRef)` with
  `add_change_request_tab`, on the `add_commit_tab` pattern (§8).

### Dependency graph

`sirio_forge` joins the leaves. `sirio_ui` and `sirio` gain a dependency on
it; nothing else does. `CLAUDE.md`'s crate diagram is updated in the same
change.

## §5 Resolving a forge and authenticating

### §5.1 When

No network call happens until the PR view is first shown. Resolution is
cached **per host for the session**, and dropped when the user changes that
host in Settings or presses *Retry* after an authentication error.

### §5.2 The chain

For a host `H`, the first step that answers wins:

1. **Settings override** — `{host, forge, means}` rows in `AppSettings`
   (`sirio_persistence`), written by the *Git hosting* page.
2. **Known name** — `github.com` is GitHub, `gitlab.com` is GitLab. Only the
   means is still open; the chain continues for it.
3. **CLI** — `gh auth status --hostname H` exiting 0 means GitHub, CLI means;
   `glab auth status --hostname H` exiting 0 means GitLab, CLI means. A program
   that is not on PATH is skipped. For a host whose forge is already known
   (steps 1–2), only that forge's CLI is asked.
4. **Stored token** — a credential-store entry `forge:<H>` means token means;
   the entry records the forge it was saved for.
5. **Unknown forge** — `GET https://H/api/v3/meta`, which GitHub Enterprise
   Server answers without authentication, identifies GitHub. Anything else is
   **not guessed**: the view asks "Is H GitHub Enterprise or GitLab?", and the
   answer is saved as an override (step 1).

A known forge with neither a logged-in CLI nor a token is *not connected*
(§7.1). Anonymous access is not supported in A.

**No silent fallback.** The step from CLI to token happens only while
resolving. When the means in use fails later (401/403, CLI logged out), the
view shows the error and its remedy; it never switches means on its own.

### §5.3 Settings → Git hosting

One row per host seen this session or configured: forge mark, host, forge,
means in use and the account it authenticates as. Actions: *Use CLI*, *Use
token* (paste field), *Forget token*, and for a non-public host the forge
selector. Saving a token verifies it at once — `viewer { login }` on GitHub,
`GET /user` on GitLab — and shows the account. The page and the *not
connected* card both name the minimum scopes: `read_api` on GitLab; read
access to pull requests, checks and metadata on GitHub.

## §6 Model and API calls

### §6.1 Model (`sirio_forge::model`)

| Type | Content |
|---|---|
| `Forge` | `GitHub`, `GitLab` |
| `ChangeRef` | forge, host, project path, number. Displays as `#578` / `!231`. The identity used everywhere (§8). |
| `ChangeState` | `Draft`, `Open`, `Merged`, `Closed` |
| `CiState` | `NoChecks`, `Running { done, total }`, `Passed`, `Failed`, `Canceled` |
| `ReviewState` | `Approved { count }`, `ChangesRequested`, `ReviewRequired`, `None`, plus `requested_from_me: bool` |
| `ChangeSummary` | ref, title, author, state, CI, review, comment count, source → target branch, updated at, web URL. One list row. |
| `ChangeHeader` | summary plus description (Markdown) and reviewers |
| `TimelineItem` | `Comment`, `Review { outcome, body }`, `LineComment { path, line, body }`, `Event` (commits pushed, review requested, merged, closed); unknown kinds become a neutral `Event` |
| `CommitSummary` | sha, title, author, date |
| `Check` | name, status (`Queued`, `Running`, `Passed`, `Failed`, `Canceled`, `Skipped`, `Neutral`), duration, URL, stage (GitLab) |
| `FileChange` | path, previous path, kind (added, modified, deleted, renamed), additions, deletions |

Unknown enum values from either API map to the neutral member; they are never
an error (§10).

### §6.2 GitHub — GraphQL

Endpoint `https://api.github.com/graphql`, or `https://H/api/graphql` on
Enterprise Server. Queries live in `.graphql` files compiled in with
`include_str!`. One query per list filter; for the detail, one query for
header + conversation (with the check rollup), and one per remaining inner tab
issued **the first time that tab is shown**.

### §6.3 GitLab — REST v4

REST rather than GraphQL because it is the more stable surface across the old
self-managed versions enterprise installations run. Endpoints:
`projects/:id/merge_requests` (list, and one MR with `head_pipeline`),
`…/discussions`, `…/approvals`, `pipelines/:id/jobs`, `…/commits`, `…/diffs`,
and `GET /user`. `:id` is the URL-encoded project path. Pagination and totals
come from the `x-next-page` / `x-total` headers — the reason the `Transport`
contract returns headers. The lowest supported GitLab version is fixed in the
plan by checking each endpoint's introduction; below it, the affected section
degrades as in §10.

### §6.4 Filters

| Filter (icon) | GitHub | GitLab |
|---|---|---|
| Mine (`person`) | open, `author:@me` ∪ `assignee:@me` | `opened`, `author_username` ∪ `assignee_username` |
| To review (`eye`, with count) | open, `review-requested:@me` | `opened`, `reviewer_username=<me>` |
| All open (`pull_request`) | `states: OPEN`, by update | `state=opened`, by update |
| Closed & merged (`archive`) | `MERGED, CLOSED`, most recent | `merged` + `closed`, most recent |

Pages of 50, with *Load more* at the end. *Mine* is two queries (author,
assignee) merged, deduplicated by number and sorted by update; *Load more*
advances both cursors. The *To review* count is its own cheap query (GitHub
`issueCount`, GitLab `x-total` at `per_page=1`), refreshed with whichever
filter is active, so the count is right while another filter is shown. The
search field queries the server, debounced 300 ms (`search` with `repo:` on
GitHub, `search=` on GitLab). The current user (`viewer` / `GET /user`) is
read once per host.

### §6.5 The worktree's own change request

The most recent change request whose source branch is the worktree's branch,
preferring an open one. When `origin` and `upstream` differ, the source is
also constrained to `origin`'s project (GitHub `headRepositoryOwner`, GitLab
`source_project_id`), so a same-named branch in someone else's fork does not
match. A detached HEAD has no card. With no match, the card reads "No PR for
`feat/x`" and offers **Create on the forge**, which opens the forge's
pre-filled creation page in the browser — a URL, not an API write.

## §7 Interface

### §7.1 The right-panel view (mockup C)

```
+ [files] [diff] [history] [refs] [PR] ------+   rail: fifth icon, git projects only
| (mark) ai-sirio/sirio                  (r) |   forge mark, project, refresh
| +----------------------------------------+ |
| | THIS WORKTREE - feat/pr-view           | |   worktree card
| | #578 feat(ui): PR view in the right... | |
| | Open - CI 3/7 - approved 1 - 3 comments| |
| +----------------------------------------+ |
|  (person) (eye 1) (pr All open) (archive) (search)   underlined icon tabs;
| ------------------------------------------ |        count on the eye only
| (pr) feat(chat): stream tool output...  x ! 12  two-line rows: title,
|      #576 - bob - 5h                       |   then number, author, age
+--------------------------------------------+
```

- `PanelView::ChangeRequests`, element id `right-panel-tab-change-requests`,
  glyph `pull_request`. Shown only when the project is a git repository.
- Header: the forge mark (from dashboardicons, Apache-2.0, vendored with
  attribution beside `lobehub/`; GitHub's mark rides the tinted path as a
  `currentColor` path, GitLab's keeps its own colours like Gemini and omp),
  the project (`owner/repo`, or `host/group/project` off the public hosts),
  refresh.
- Filters: underlined tabs of icons; the active one adds its label; all have
  tooltips. The count shows on *To review* only, in the accent colour when
  above zero. Default *All open*; the last choice is kept for the session as a
  GPUI global, like `PanelView`.
- Rows: state icon, title, then `#n · author · age`; CI, review and comment
  count on the right. Click opens the detail; right-click offers *Open in
  browser* and *Copy link*.
- Loading: a placeholder only until the first load settles; later refreshes
  keep the rows visible, and a failed refresh keeps them with a "Refresh
  failed · Retry" line — the `settled` rule the Files view follows.
- States that replace the list: *no forge remote*; *unknown forge* (the
  GitHub-or-GitLab question); *not connected* (the `auth login --hostname H`
  command to copy, or *Paste a token*); *error* (§10).

### §7.2 The detail tab (mockup A)

```
[ (pr) #578 feat(ui): PR view in the right... x ]                  tab strip
+----------------------------------------------------------------------+
| feat(ui): PR view in the right panel #578  [Open in browser]    (r)  |
| (Open) epalmisano wants to merge feat/pr-view -> main - 2h ago       |
| Conversation 3 | Commits 5 | Checks 3/7 | Files 12                   |  inner tabs
| -------------------------------------------------------------------- |
| description, then comments, reviews and events, oldest first         |
+----------------------------------------------------------------------+
```

- Tab strip title `#578 title`, with the state icon coloured (open green,
  draft grey, merged purple, closed red); it follows every refresh.
- **Conversation** — the description through the Preview's Markdown path,
  `expand_html` subset included, then comments, reviews with their outcome,
  and events, oldest first. A line comment shows as "on `path:line`" with its
  text and no surrounding diff (that is B). An image that does not load — a
  private repository's attachment — must not break the description; the plan
  checks what the Preview does today and settles on showing its alt text as a
  link to the forge where it does not.
- **Commits** — short sha, title, author, age. Click: if the object exists
  locally (`git cat-file -e`), the existing Changes tab through
  `add_commit_tab`; otherwise the commit on the forge. Nothing is fetched.
- **Checks** — failed first, then running and queued; passed checks fold into
  one expandable "N passed" row. GitLab groups by stage. Click opens the log on
  the forge.
- **Files** — path, kind, `+/−`. Click opens the change request's file view
  on the forge; B replaces this with the in-app diff.
- Inner tabs load on first display and keep their own error state.

## §8 Identity and persistence

- `TabKind::ChangeRequest`: `default_pane() == Secondary`,
  `can_move_between_panes() == false`, `appears_in_sidebar() == false` — it is
  something the user looks at, and the right panel already lists it. The three
  exhaustive matches force each answer to be written.
- `TabRecord::kind` = `"change_request"`. `SessionTabState` gains
  `change_request: Option<PersistedChangeRef>` (forge, host, project,
  number) and the active inner tab, both `#[serde(default)]`, so sessions
  written before read as they did.
- On restore the tab appears at once under its saved title and loads when
  shown. A change request that can no longer be reached (404, host
  disconnected) keeps its tab, showing the error with *Retry* and *Close*; it
  is never dropped silently.
- One tab per `ChangeRef` per worktree: opening one already open in that
  worktree focuses it.

## §9 Refresh and rate limits

- List: when the view becomes visible, then every 60 s while it stays
  visible, plus the refresh button. Nothing while hidden.
- Detail: when the tab gains focus, then every 60 s only while its CI is
  `Running`, plus the refresh button.
- A rate-limit answer (403/429 carrying the forge's rate-limit headers) stops
  all calls to that host until the reset time, shown as "retrying at HH:MM".

## §10 Errors

One `ForgeError` enum; every variant reaches the user with its remedy.

| Variant | Typical cause | Shown as |
|---|---|---|
| `NotInstalled { program }` | CLI means chosen, program not on PATH | "`glab` is not on PATH" + *Use a token* |
| `NotAuthenticated` | 401, or the CLI is not logged in to H | the `auth login --hostname H` command, or *Paste a token* |
| `Forbidden` | missing token scopes; GitHub SAML SSO not authorised (`X-GitHub-SSO`) | the missing scopes / the SSO authorisation link |
| `NotFound` | private repository without access, deleted change request | message + *Retry* / *Close* |
| `RateLimited { reset }` | forge rate limit | "retrying at HH:MM"; no calls until then |
| `Tls` | corporate CA missing from the system trust store | an explanation naming the CA, not a generic network error |
| `Network` | DNS, timeout (HTTP 20 s; CLI 30 s, then killed) | host + detail + *Retry* |
| `UnexpectedResponse` | a structurally broken body | detail + *Retry* |

- **Degrade, don't reject**, as `sirio_claude` does: unknown values become
  neutral, missing optional fields become `None`, and an endpoint an older
  GitLab lacks (404 on `approvals`) disables that section — "not available on
  this version" — not the tab.
- Errors are isolated per list and per inner tab.
- The token never appears in logs, errors, captured stderr or traces.
  Change-request content never reaches `sirio_perf`; its labels stay
  content-free `&'static str`.

## §11 Testing

End to end, per the project convention; every run ends in an artifact.

1. **`Scripts/Tests/test-forge-e2e.sh`** → `FORGE E2E OK`, on the shape of
   `test-update-e2e.sh`. A fake forge on loopback serves fixtures of the real
   GitHub GraphQL and GitLab REST shapes (captured from the live APIs, with
   their provenance recorded next to them). A probe,
   `rust/crates/sirio_forge/examples/forge_probe.rs`, compiled the way the app
   is, drives resolution → list → detail → pagination → 401/403/404/429.
   - Token transport: real.
   - CLI transport: the real `glab`, pointed at the fake forge through a
     per-host `api_protocol: http` in an isolated config directory. The real
     `gh` if it can be pointed at loopback; if the plan finds it cannot, a shim
     that records the invocations stands in, and the artifact says so.
   - The fake base URL comes from an override **honoured only in debug
     builds**, as `SIRIO_UPDATE_MANIFEST_URL` is, so a release binary cannot be
     pointed at another forge by an environment variable.
2. **Live conformance** — a test that SKIPs without credentials and runs the
   real queries against github.com (this repository, through `gh`) and a
   public gitlab.com project. It is what notices the day a query stops
   matching the live schema. It joins the release gate's skip list beside the
   ACP conformance tests.
3. **UI end to end**, on `Scripts/visual-sweep.sh`'s shape: an isolated Sirio
   (`SIRIO_DB`) whose worktree's remote points at the fake forge, driven over
   the control socket — show the view, switch filter, open the detail, switch
   inner tab — leaving a socket transcript and PID-matched window captures
   under `--out-dir`. Socket verbs missing for this are added.
4. **Unit tests only for pure units that must be proven in isolation**:
   remote parsing (subgroups, ports, `ssh://`, scp form, `.git`) and state
   mapping (GitLab pipeline and job statuses, GitHub check rollup, unknown
   values). For each, every way it can fail is written first, then the code.

## §12 Out of scope

- Anything that writes to a forge: comments, approvals, merges, creating a
  change request through the API (A only opens the creation page) — B and C.
- The diff inside Sirio, inline threads — B.
- Checking a change request out into a worktree, handing it to an agent,
  fetching refs — C.
- Badges on sidebar rows, notifications, tray entries.
- CI logs inside Sirio.
- Anonymous access to public repositories.
- Forges other than GitHub and GitLab (Gitea, Bitbucket, Azure DevOps).

## §13 Verified in the plan, with the fallback already decided

| Point | Fallback if the check fails |
|---|---|
| `gh` can be pointed at a loopback forge | recording shim, declared in the artifact (§11.1) |
| Lowest GitLab version for each endpoint | the section degrades on older versions (§10) |
| What the Preview does with an image that fails to load | alt text as a link to the forge (§7.2) |
| Control-socket verbs available for the UI run | add them (§11.3) |
