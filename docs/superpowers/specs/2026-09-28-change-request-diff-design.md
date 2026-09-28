# A change request's diff and files inside Sirio — design

**Date:** 2026-09-28
**Status:** proposed
**Programme:** part **B1** of the change-request programme, revised with the
user on 2026-09-28. A (read-only list and detail tab,
`2026-09-27-change-requests-design.md`) is merged as #578. What follows is,
in order: **B1** (this document) — the change request's diff inside the
detail tab and its files in Sirio's editor; **B2** — acting on a change
request (comment, approve, merge, close, draft, reviewers, CI); **B3** —
inline review threads in the diff; **C** — a change request into a worktree
and to an agent. Each has its own spec. The order replaces A's "A → C → B".
**Scope of this document:** stop sending the user to the forge to read a
change request's code. The detail tab's *Files* becomes the diff, rendered by
the existing Changes surface; a file opens in Sirio's editor; a commit and a
line comment open inside Sirio too.

## §0 Intent

A shows where a change request stands, but every look at its code — a file,
the diff, a commit that is not local, a line a reviewer commented on — leaves
Sirio for the browser. The user wants to read a change request's code where
they read everything else: the diff in the detail tab, a file in the editor.
B1 is also the foundation B3 anchors inline threads to: the exact revisions
the forge reports, present locally.

Success: on a GitHub repository and on a self-managed GitLab, opening *Files*
of a change request someone else opened shows its diff — the same files and
counts the forge shows — without the user fetching anything by hand; *Open in
editor* shows the file as it is in the change request; clicking a commit or a
line comment never opens a browser unless fetching failed and the user chose
the forge as the remedy.

## §1 Decisions taken

Settled with the user before this document was written:

| Question | Decision |
|---|---|
| Programme | Split into **B1 → B2 → B3 → C**, each with its own spec; this document is B1. |
| What *Open in editor* shows | **Hybrid.** When the tab's worktree has HEAD at the change request's head, the local file, editable. Otherwise the file as it is at the head, **read-only**, in a *snapshot* tab that says so. |
| Where the diff lives | **Inside the *Files* inner tab**, like the forges' own "Files changed": expandable rows, Unified/Split, *Open in editor* per file. B3 draws its threads there. |
| How the commits arrive | **Fetched automatically**, when *Files* (or a commit, or a file) needs revisions that are not local, into a Sirio-owned ref namespace, without prompts and without touching `FETCH_HEAD` or the user's branches. The refs go when the last tab using them closes. |
| Approach | **Local git with the revisions the forge reports** (approach 1): the forge names base and head; git fetches them; one `git diff base...head` feeds the existing Changes surface; `git show head:path` feeds the snapshot. Not the forges' diff APIs (truncated patches, no context expansion, a call per file), and no API fallback. |

## §2 Where it stops today

- `ChangeRequestTab` (`sirio_ui/src/change_request_tab.rs`) renders *Files*
  from `ForgeClient::files` — path, kind, `+/−` — and a click calls
  `cx.open_url` on the forge's files page (`render_files`).
- A commit click emits `ChangeRequestTabEvent::OpenCommit`; the host's
  `open_change_request_commit` (`sirio/src/main.rs`) opens `add_commit_tab`
  when `sirio_git::object_exists` says the object is local, and the forge
  otherwise. Nothing is fetched.
- A line comment renders as "on `path:line`" and is not clickable.
- Neither header query reads the change request's revisions:
  `queries/github/header.graphql` has no `baseRefOid`/`headRefOid`,
  `queries/gitlab/header*.graphql` no `diffRefs`.
- `ChangesTab` (`sirio_ui/src/changes.rs`) has
  `ChangesSource::{WorkingTree, Commit(sha)}`. `load_commit_snapshot` runs
  `commit_files` and then one `git show` **per file**
  (`sirio_git::commit_diff_entry`).
- `FileView` (`sirio_ui/src/file_view.rs`) always reads a path from disk,
  watches it and saves it. `Editor::from_buffer(path, text)`
  (`sirio_ui/src/editor.rs`) already builds an editor from a string.
- Sirio never runs `git fetch`; `sirio_git` has no fetch function.
- `sirio_git::remote_url` reads `config --get remote.<name>.url`: no
  `insteadOf` rewrite, by design — a forge is recognised by the URL the user
  configured.
- The History view walks `--branches` (`sirio_git/src/log.rs`), not `--all`.
- An Editor tab whose `SessionTabState::editor_path` is empty is dropped at
  restore (`restored_editor_path`).

## §3 Vocabulary

Added to `CONTEXT.md` under *Forges*:

- **Revisions** — the pair of commits a change request's diff is taken
  between, as the forge reports them: its **base** and its **head**. The
  diff is `base...head` (from their merge base).
- **Snapshot** — a file as it is at one revision, shown read-only in an
  editor tab because it is not what the worktree has on disk.

## §4 Architecture

No crate gains a dependency; the graph in `CLAUDE.md` is unchanged.

### `sirio_forge`

- `model::Revisions { base_sha, head_sha, start_sha: Option<String> }` and
  `ChangeHeader::revisions: Option<Revisions>`. GitHub: `baseRefOid`,
  `headRefOid`. GitLab: `diffRefs { baseSha headSha startSha }`, in the
  baseline variant too (§12). `None` when the forge does not say; unknown or
  malformed values are `None`, never an error. `start_sha` (GitLab's target
  tip when the diff was computed) is unused by B1 and read now because B3's
  line positions need it.
- `ChangeRef::head_ref()` — `refs/pull/N/head` on GitHub,
  `refs/merge-requests/N/head` on GitLab — the ref the forge keeps for every
  change request, forks included.

### `sirio_git`

- `fetch_refs(repo, remote, refspecs, timeout)` — the one fetch Sirio runs
  (§6). Never prompts, never outlives its timeout.
- `range_files(repo, base, head)` and `range_diff(repo, base, head)` — the
  change request's files and their diffs from **two** git processes whatever
  the file count (`--name-status -M` and `--patch -M`, both `base...head`),
  the patch split per file and each piece parsed by the existing
  `parse_diff`.
- `show_blob(repo, sha, path)` — a file's bytes at a revision.
- `refs_under(repo, prefix)` and `delete_ref(repo, name)` — for §6.4.

### `sirio_ui`

- `ChangesSource::Range { base, head }` and `ChangesTab::for_range`.
  Immutable like `Commit`: no staging, no poll loop — a revision never
  changes. Loads through `range_files`/`range_diff`.
- `ChangesTab::focus_line(path, line)` — `focus_path`, then scroll to the
  row carrying `line` on the new side, expanding a collapsed context band
  that hides it.
- `FileView::snapshot(label, path, bytes)` — built on `Editor::from_buffer`.
  Read-only: no save, no file monitor, no language server; a bar reads
  "Read-only · #578 at a1b2c3d" and, when the path exists in the worktree,
  offers *Open local copy*.
- `ChangeRequestTab` learns its worktree (the host passes it to `new` and
  `restored`). Its *Files* embeds a `ChangesTab` in Range mode (§7.1) and
  routes that surface's `OpenFile` to the host. New events:
  `OpenFile { path, line: Option<u32> }` and the existing `OpenCommit`, both
  carrying the tab's `ChangeRef` and revisions to the host.
- `ChangeRequestSource` gains
  `fetch_revisions(worktree, &ChangeRef, &Revisions) -> Result<(), RevisionError>`;
  blocking, run on the background executor like the rest of the seam.

### `sirio` (host)

- `ForgeHub::fetch_revisions`: picks the remote (§6.1), fetches only what is
  missing (§6.2), one fetch at a time per change request (§6.3).
- `open_change_request_file`: the hybrid choice of §5.3.
- `open_change_request_commit`: fetch, then `add_commit_tab` (§5.4).
- Owns the refs' lifetime (§6.4).

## §5 Data flow

```
header (on focus, 60 s while CI runs) ──► revisions {base, head}
                                              │
Files shown ──► both local? ──no──► host: fetch_revisions ──► refs/sirio/…
                    │ yes                       │ failed → error + A's list
                    ▼                           ▼
        ChangesTab::for_range(base, head) ◄─────┘
                    │
   file row ──► Open in editor ──► host: worktree HEAD == head?
                                     ├─ yes → add_file_tab(path), reveal line
                                     └─ no  → snapshot tab (head; base if deleted)
```

### §5.1 Opening *Files*

On first display, as every inner tab loads in A. Off the UI thread,
`object_exists` for base and head; when either is missing, the host fetches
(§6) while the tab shows A's list under "Fetching #578's commits for the
diff…" (§7.1). Then `ChangesTab::for_range(worktree, base, head)`. The
*Files* count in the inner strip stays the header's `changed_files`.

### §5.2 A new push

A header refresh that reports a different `head_sha` (or `base_sha`) rebuilds
the Range surface for the new revisions — fetching again if needed — keeping
the paths that were expanded, and shows one quiet line, "Updated to head
b2c3d4e".

### §5.3 *Open in editor*

The Range surface's per-file `OpenFile(path)` reaches the change request tab,
which asks the host with its `ChangeRef` and revisions. The host reads the
worktree's HEAD off the UI thread (`head_sha`):

- **HEAD == `head_sha`** → `add_file_tab(worktree/path)`, the ordinary
  editable file, revealed at the line when there is one.
- **otherwise** → a snapshot tab of `path` at `head_sha` — at `base_sha` for
  a file the change request deletes, titled "deleted in #578". One snapshot
  tab per `(sha, path)`: opening it again focuses it.

### §5.4 Commits

A commit click fetches the revisions first when the commit is not local (a
change request's commits are ancestors of its head), then `add_commit_tab`.
When the fetch fails, the tab shows the error with *Open on the forge* as the
remedy; it no longer opens the forge by itself.

### §5.5 Line comments

"on `path:line`", in a comment or under a review, becomes a link: it selects
*Files* and calls `focus_line(path, line)`. A comment the forge marks
outdated — no current line — focuses the file only.

### §5.6 Checks

Unchanged: a check opens its log on the forge. CI logs inside Sirio are a
candidate for B2.

## §6 Fetching and the refs

### §6.1 Which remote

The one whose configured URL (`remote_url`, no `insteadOf`) parses to the
`ChangeRef`'s host and project: `upstream` first, then `origin`, then the
rest by name — the preference A's list already has. None matches:
`RevisionError::NoMatchingRemote { expected: "host/project" }`.

### §6.2 What

Only when `base_sha` or `head_sha` is missing locally. A worktree's own
change request usually has both, and then nothing is fetched and no ref is
made. Otherwise one `git fetch --no-tags --no-write-fetch-head <remote>` with
two refspecs:

- `+<ChangeRef::head_ref()>:refs/sirio/change-requests/<remote>/<N>/head`
- `+refs/heads/<target branch>:refs/sirio/change-requests/<remote>/<N>/base`

If a revision is still missing afterwards — the target was force-pushed, the
forge dropped the ref — `RevisionError::RevisionGone { sha }`.

### §6.3 Never a prompt, never a hang

The fetch runs with `GIT_TERMINAL_PROMPT=0`, `GCM_INTERACTIVE=never`,
`SSH_ASKPASS_REQUIRE=never`, a null stdin and, on Unix, in a new session
with no controlling terminal, so `ssh` cannot open `/dev/tty`. After 120 s it
is killed: `RevisionError::FetchTimedOut`. Credential helpers that do not
prompt — the OS keychain, libsecret, `gh auth git-credential` — keep working.

The stderr shown in `FetchFailed` is its last lines with every URL's
`user:password@` removed: a remote URL may carry a token, and the rule of A
§10 holds — no token in anything shown, logged or traced.

One fetch at a time per `(repository, remote, number)`: a second request
waits for the first and shares its result.

### §6.4 Who owns the refs

The refs live in the repository's common directory, so every worktree of
the project sees them, and they belong to the tabs that need them: a change
request's detail tab and its snapshot tabs, in any worktree. When the last
of them closes, the host deletes both refs. At startup, after the session is
restored, a sweep of `refs/sirio/change-requests/` deletes every ref no tab
recorded in the session store holds — which covers a crash or a kill. Sirio
deletes only inside that namespace. The objects stay until the user's own
`git gc`.

The refs are invisible to Sirio's History (`--branches`); an external
`git log --all` shows them, which the *Change requests* paragraph of
`CLAUDE.md` records.

## §7 Interface

### §7.1 *Files*

A's list remains, as the rendering of every state where there is no diff yet:

| State | Shown |
|---|---|
| Fetching | A's list, headed "Fetching #578's commits for the diff…" |
| Diff ready | the Range `ChangesTab` in place of the list |
| Fetch failed | A's list (a click opens the forge, as today), headed by the error with *Retry* |
| No revisions from the forge | A's list, headed "The forge does not report this change request's revisions" |

The Range surface is the Changes surface as it is — sections, Unified/Split,
expandable context, keyboard selection — with the staging controls absent as
in a commit view, and *Open in editor* as each file's action. Its header line
reads `base a1b2c3d … head b2c3d4e`. Right-click on a file adds *Open on the
forge* and *Copy path*.

### §7.2 The snapshot tab

- Title `lib.rs @ #578`; the file's icon with a lock.
- The bar: "Read-only · #578 at a1b2c3d", *Open local copy* when
  `worktree/path` exists.
- Typing does nothing; Ctrl+S says "Read-only: open the local copy to edit".
  Find, go to line, copy and Markdown Preview work. No language server.
- A binary or oversized file takes the states the editor already has.
- `TabKind::Editor`, the Secondary half, like any file tab.

### §7.3 Errors in place

Each error shows where the action was taken — *Files*, the commit row, the
snapshot tab — with its remedy (§9), and never replaces the Conversation or
another inner tab.

## §8 Persistence

- `SessionTabState` gains `snapshot: Option<PersistedSnapshot>` —
  `{ change_request: PersistedChangeRef, sha, path }` — `#[serde(default)]`.
  A snapshot tab is written with kind `"editor"` and an empty `editor_path`,
  so a build before B1 drops it (§2) instead of opening a local path.
- Restore reads the bytes again with `show_blob`. A missing object triggers
  `fetch_revisions` through the saved `ChangeRef`; if that fails too, the tab
  shows the error with *Retry* and *Close* — never dropped silently (A §8).
- The detail tab already saves its inner tab (`files` included); the Range
  surface's expanded rows are not saved, as a commit view's are not.

## §9 Errors

`RevisionError`, host → UI; each reaches the user with its remedy.

| Variant | Typical cause | Shown as |
|---|---|---|
| `NoMatchingRemote { expected }` | no remote points at the forge project | "No remote points at `host/project`" |
| `FetchFailed { detail }` | git credentials, network | the sanitised stderr tail, *Retry*, *Open on the forge* |
| `FetchTimedOut` | ssh or a credential helper waiting for input | "git fetch did not answer in 120 s", *Retry* |
| `RevisionGone { sha }` | force-push; the forge dropped the ref | the missing sha, *Open on the forge* |
| `Git { detail }` | `git diff` / `git show` failed locally | the detail, *Retry* |

A's `ForgeError`s keep their meaning; a header that cannot load leaves
*Files* on A's list with A's error.

## §10 Testing

End to end, per the project convention; every run ends in an artifact.

1. **`Scripts/Tests/test-forge-ui-e2e.sh`**, extended, for GitHub **and**
   GitLab → `FORGE UI E2E OK`.
   - Setup: a bare repository plays the forge's git side — `main`,
     `refs/pull/7/head`, `refs/merge-requests/7/head` — built with fixed
     dates and identity so its shas are deterministic and the fake forge's
     fixtures can name them. The isolated worktree's `origin` is the fake
     forge's URL, so Sirio recognises the forge; `url.<bare>.insteadOf`
     sends git's fetch to the bare repository. Real git end to end.
   - Driven over the control socket (verbs added where missing), the run
     proves:
     - *Files* goes from fetching to the diff, and its files and counts equal
       a `git diff --numstat base...head` **the script** computes;
     - `for-each-ref refs/sirio/` holds exactly the two refs, `FETCH_HEAD`
       is untouched, no tag was fetched;
     - with the worktree on `main`, *Open in editor* gives a read-only
       snapshot tab whose content hashes to `git show head:path`; after the
       worktree checks out the head, the same action gives the ordinary file
       tab;
     - a commit that was not local opens in a Changes tab; a line comment's
       link focuses its path and line;
     - closing the tabs deletes the refs; an orphan ref planted before launch
       is gone after startup.
   - Failure stages: a remote answering 401 fails **at once**, with no
     prompt; an `ext::` remote that never answers times out (the limit
     lowered by an override honoured only in debug builds, as
     `SIRIO_UPDATE_MANIFEST_URL` is). Both leave A's list with the error.
   - Artifacts under `--out-dir`: socket transcript, ref dumps, content
     hashes, PID-matched window captures.
2. **`Scripts/Tests/test-forge-e2e.sh`** — the probe reads `revisions` from
   both forges' header fixtures, the GitLab baseline included.
3. **Live conformance** (`sirio_forge/tests/forge_live.rs`) already runs the
   real header queries; the new fields ride along.
4. **Unit tests only for pure units that must be proven in isolation**, each
   with every way it can fail written before the code:
   - splitting one multi-file `git diff` into per-file diffs: renames,
     binaries, mode-only changes, spaces, quotes and non-ASCII in paths,
     `\ No newline at end of file`, CRLF, an empty diff;
   - the stderr sanitiser: `user:password@` in `http` and `https` URLs,
     several in one text, no false positive on the scp form `git@host:path`;
   - the choice of remote (§6.1).

## §11 Out of scope

- Anything that writes to a forge, and CI logs in Sirio — **B2**.
- Inline review threads in the diff — **B3**.
- Checking a change request out into a worktree, agents — **C**.
- "Changes since my last review" and picking a commit range inside *Files*.
- Editing a snapshot, applying a suggested change.

## §12 Verified in the plan, with the fallback already decided

| Point | Fallback if the check fails |
|---|---|
| `diffRefs` exists on the GitLab versions the baseline targets (15.0) | the baseline omits it; on such a server *Files* shows A's list with "does not report this change request's revisions" |
| `--no-write-fetch-head` (git 2.29) on the git Sirio finds | on an older git the flag is dropped and `FETCH_HEAD` is written; the E2E asserts on a git that has it |
| `SSH_ASKPASS_REQUIRE` (OpenSSH 8.4) | older ssh ignores it; the session with no controlling terminal and the 120 s kill still bound the fetch |
| `url.<bare>.insteadOf` rewrites the fetch while `remote_url` keeps the forge URL | **verified**: `remote_url` reads `config --get` (§2), which applies no rewrite |
| An `ext::` remote can hang a fetch for the timeout stage | a fake forge HTTP endpoint that accepts and never answers |
| Embedding a `ChangesTab` entity inside the detail tab keeps its keyboard focus and scroll working | open the Range surface as its own Changes tab beside the detail tab, keyed by `ChangeRef` |
