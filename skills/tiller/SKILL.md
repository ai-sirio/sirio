---
name: tiller
description: Use when running inside a Tiller pane to create and manage terminal panels, dispatch worker agents, wait for completion, read output, report status, or leave worktree progress comments through tillerctl.
---
<!-- Machine-managed by Tiller. Do not edit this installed copy. -->

# Tiller terminal orchestration

Use `tillerctl` only inside a terminal launched by Tiller. Establish the control connection and obtain machine-stable identity before orchestrating:

```bash
[ "$TILLER_ENV" = "1" ] || exit 1
tillerctl ping
tillerctl identify --json
```

Treat returned UUIDs as the only orchestration state. Never infer a target from Tiller's visually selected worktree, tab, pane, focus, or layout.

## Create panels and capture UUIDs

Capture every created panel's UUID immediately. `panel create` and `panel split` print that UUID on success; keep it in a variable and use it for every later operation.

```bash
TAB_ID=$(tillerctl panel create --cmd 'codex')
SPLIT_ID=$(tillerctl panel split right --from "$TILLER_PANE_ID" --cmd 'pi')
```

`$TILLER_PANE_ID` identifies the current pane when choosing a split source. It does not authorize visual-selection inference. Use `TAB_ID` for the tab above and `SPLIT_ID` for the split above rather than whichever panel happens to be focused.

## Drive a known panel

All lifecycle operations target an explicitly captured UUID:

```bash
tillerctl panel write --id "$TAB_ID" --input 'Run the assigned task' --enter
tillerctl panel key --id "$TAB_ID" enter
tillerctl panel read --id "$TAB_ID"
tillerctl panel wait --id "$TAB_ID"
tillerctl panel focus --id "$TAB_ID"
tillerctl panel close --id "$TAB_ID"
```

`panel wait` propagates the child process exit code. Preserve it when doing post-wait work so a failed child remains a failure:

```bash
status=0
tillerctl panel wait --id "$TAB_ID" || status=$?
tillerctl panel read --id "$TAB_ID"
tillerctl panel close --id "$TAB_ID"
exit "$status"
```

Close every temporary panel on both success and failure. A shell trap is the safest cleanup when the orchestrator can exit early:

```bash
WORKER_ID=$(tillerctl panel create --cmd 'codex')
trap 'tillerctl panel close --id "$WORKER_ID"' EXIT
status=0
tillerctl panel wait --id "$WORKER_ID" || status=$?
tillerctl panel read --id "$WORKER_ID"
exit "$status"
```

The trap performs cleanup for successful completion, child failure, and an interrupted orchestration path. Do not replace explicit UUID targeting with current-selection assumptions.

## Browser automation

Use the browser surface when an agent needs to inspect or drive a page inside
Tiller. Browser commands target the caller's worktree by default. Keep the
surface identifier returned by `browser open`; UUIDs are accepted as input.
`browser open --id-format both` also returns a short `surface:N` reference.

### Verbs

The V1 browser verbs are:

- `open <url>` creates a surface and returns its identifier, URL, and title.
- `navigate <surface> back|forward|reload` moves the same surface through its
  history or reloads the current page.
- `get <surface> url` reads the current URL without reading page content.
- `get <surface> text [--selector <css>]` reads text from the page or selector.
- `get <surface> html [--selector <css>]` reads HTML from the page or selector.
- `screenshot <surface> [--path <file>]` captures the rendered page.
- `snapshot <surface>` returns a `generation` and interactive nodes with refs
  such as `e1`, `role`, `name`, optional `value`, and `box`. There is no
  `--interactive` flag: this snapshot is always the interactive one. Use
  `--json` when a machine-readable response is needed.
- `act <surface> click|fill|type|press|scroll` acts on a snapshot ref or CSS
  selector. Pass `--generation <generation>` for ref-based actions. `fill` and
  `type` use `--value`, `press` uses `--key`, and `scroll` uses `--delta-x` /
  `--delta-y`. `--snapshot-after` returns a fresh snapshot with the action.
- `wait <surface>` waits for exactly one of `--selector`, `--text`,
  `--url-contains`, `--load-state`, or `--function`; `--timeout-ms` is
  required.
- `eval <surface> <script>` evaluates JavaScript in the page.
- `console <surface> [--since <timestamp>]` returns buffered console entries.
- `errors <surface>` is part of the protocol vocabulary but is currently
  `not_supported`; there is no successful error-log response in V1.

### Ref and generation loop

Always use the following loop after opening a page and after any navigation or
DOM-changing action:

```bash
tillerctl browser open http://127.0.0.1:4173 --json
# Capture the returned `surface` value as SURFACE.
tillerctl browser get "$SURFACE" url
tillerctl browser wait "$SURFACE" --load-state complete --timeout-ms 15000
tillerctl browser snapshot "$SURFACE" --json
# Read generation G and ref e1/e2 from that snapshot.
tillerctl browser act "$SURFACE" click --ref e1 --generation G --snapshot-after
tillerctl browser snapshot "$SURFACE" --json
```

The exact socket contract is `browser.snapshot` followed by
`browser.act` citing `--generation`, then another snapshot. A new navigation
or snapshot makes earlier refs stale. Tiller never heuristically rematches a
stale ref: re-snapshot and use the new generation and refs.

### Origin permissions

`localhost`, `127.0.0.1`, `::1`, and `*.local` are local origins and do not
prompt. On every other origin, the first content-reading verb — including
`get text`, `get html`, `snapshot`, `eval`, `act`, content predicates in
`wait`, `console`, and `screenshot` — waits for a user permission prompt. It
can return `origin_denied` if the user denies the request or no grant is
available. The grant is remembered per worktree and origin until revoked.
`open`, navigation, `get url`, and Tiller's own title/favicon metadata do not
read page content and do not require that grant.

### Error codes and recovery

| Code | Agent action |
|---|---|
| `surface_not_found` | Re-list or reopen the browser surface; do not change an argument that should identify an existing surface. |
| `stale_ref` | Take a new interactive snapshot and retry with its ref and generation. |
| `js_error` | Read the returned `hint`; retry with a simpler page script or use `get text` / `get html` if appropriate. |
| `timeout` | Check the page URL and state, then retry with a sufficient positive timeout. |
| `not_supported` | Change the workflow; the requested capability is not available in Tiller V1. |
| `origin_denied` | Ask the user to grant the worktree/origin permission, or use a local origin. Do not bypass the gate. |
| `invalid_url` | Correct the URL and retry `open`. |
| `invalid_argument` | Correct the named parameter using the accepted values in the error hint. |
| `navigation_unavailable` | Use a valid current history direction or open a URL first. |
| `navigation_failed` | Read the returned `hint`, check the URL/page, and retry only when the cause is addressed. |

### `not_supported` table

These capabilities return `not_supported`, never a silent success:

| Verb/capability | Reason |
|---|---|
| `offline` | WebKit V1 does not expose the required emulation contract. |
| `trace`, `screencast`, `record` | CDP-style tracing or recording is outside the V1 surface API. |
| `network-route`, `network-mock`, `network-intercept` | Network interception is not exposed by this WKWebView surface. |
| `raw-input` | Low-level input injection is not part of the agent API. |
| `download`, `upload` | File transfer flows are outside V1. |
| `pdf`, `viewport` | These controls are outside V1. |
| `devtools`, `extensions`, `bookmarks`, `history` | Browser-wide features are not surface verbs in V1. |
| `errors` | The protocol name is reserved, but error-log retrieval is not implemented. |

## Report durable and user-visible status

Attach durable progress to the current worktree and use notifications for user-visible completion:

```bash
tillerctl worktree set --workspace "$TILLER_WORKTREE_ID" --comment 'Implemented and verified'
tillerctl notify --title 'Done' --body 'Worker completed'
```
