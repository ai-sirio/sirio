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

## Report durable and user-visible status

Attach durable progress to the current worktree and use notifications for user-visible completion:

```bash
tillerctl worktree set --workspace "$TILLER_WORKTREE_ID" --comment 'Implemented and verified'
tillerctl notify --title 'Done' --body 'Worker completed'
```
