# P119 triage — what survives a restart

## Finding

**Cross-restart restore is implemented for the application workspace, but the P116
observation was taken before the application had published its restored panes to the
control registry.** It is not one four-row persistence defect.

The repeat against the same `/tmp/pi.sqlite` was deliberately split into an immediate
socket read and a settled read. `panel list` was empty as soon as `ping` succeeded, but
three seconds later it contained the restored Chat, Terminal, and Changes panes. The
control socket is listening before the GPUI workspace has called
`sync_control_panes`; `ping` is therefore not a boot-ready barrier.

| P116 symptom / inventory row | Classification | Evidence | Result |
|---|---|---|---|
| `F-CORE-WSP-08`: no panes after restart | **Restored, but observed too early** | `06-settle-probe.txt`: immediate list empty; after 3 s lists `pane-0` Chat, `pane-1` Terminal, `pane-2` Changes. | Re-drive with a readiness poll; no persistence fix indicated. |
| `F-CORE-SET-01`: Appearance selected, Settings not open after restart | **Transient surface routing, not persisted settings data** | The selected category wrote no `setting` row. Boot starts with `show_settings: false`; the Settings entity is rebuilt with `category: Appearance`. | This probe cannot establish a settings-persistence defect. Re-drive an actual setting value, then explicitly open Settings after relaunch. |
| `F-AGENT-SESSION-01`: native ref cannot be queried because the control pane is gone | **Reference persisted; its control-pane owner is runtime-only** | `session_ref` retained `pane-2401436-1 → p119-native-ref`; the panel did not appear in `tab` or `tab_state`. | Not a workspace-restore failure. The app has no resume integration for that reference. |
| `F-AGENT-SESSION-02`: accepted ref is absent with its panel after restart | **Same runtime-pane/reference split** | Same database evidence and source path as `-01`. | Not unblocked by a generic restore change. |

## What I drove

Headless, isolated instance as required:

```bash
env -u DISPLAY -u WAYLAND_DISPLAY \
  TILLER_SOCKET=/tmp/pi.sock TILLER_DB=/tmp/pi.sqlite \
  rust/target/debug/tiller
```

`reference/linux-progress/p119/restart-probe.sh`, captured in
`04-restart-probe.txt`, is the initial red-capable replay. It created one control
panel, assigned `p119-native-ref`, selected Appearance, quit, and relaunched against
the unchanged database:

```text
assertion pre_panels=4 post_panels=0 settings=not-restored
```

That is a real observation, but it was not yet a valid persistence verdict: it
queried immediately after socket readiness. The follow-up capture
`06-settle-probe.txt` repeated the post-relaunch read at a fixed delay:

```text
immediate panels:                 # no rows
after 3 seconds panels:
pane-0  Chat      Chat      false
pane-1  Terminal  Terminal  false
pane-2  Changes   Changes   true
after 3 seconds settings:
tillerctl: Settings surface is not open
```

This is deterministic on the existing binary: socket readiness precedes publication
of external pane state. The useful re-drive loop is therefore “poll `panel list` for
the expected restored pane(s)”, not “`ping`, then list once”.

## Database evidence

Direct WAL-aware SQLite reads are in `01-fixture-before-drive.txt` and
`04-restart-probe.txt`.

* Before the drive, and before/after the restart, `tab` and `tab_state` held the same
  three persisted workspace surfaces: Chat, Terminal, and Changes. The app had data
  to restore; it was neither lost nor overwritten by quit.
* `session_ref` grew from the preserved P116 row to two rows, including
  `pane-2401436-1 → p119-native-ref`, and both rows survived quit and relaunch.
  Thus `session.ref` **is persisted**.
* The new control pane is absent from `tab` and `tab_state`. It is a
  `PaneRegistry` process object, not a persisted workspace tab.
* `setting` stayed empty because selecting a Settings category is not a settings
  mutation. The visible selection and whether the full-screen Settings surface is
  open are not database fields.

This distinguishes the identical-looking symptoms:

| Object | Written? | Read/recreated on boot? | Conclusion |
|---|---:|---:|---|
| Workspace tabs and pane state | yes | yes | restored; the socket can report zero before publication. |
| Socket-created control pane | no | no | intentionally process-local under the present contract. |
| Settings surface/category | no | no | intentionally process-local routing state. |
| `session_ref` map | yes | loaded into handler | data survives, but no resume target is reconstructed from it. |

## Root causes and locations

### 1. Socket readiness is earlier than control-pane publication — high confidence

`rust/crates/tiller/src/main.rs:1115-1179` makes the distinction explicit:
`panel.create` calls `PaneRegistry::create` directly and `panel.list` reads that
registry directly. It does not enqueue an application/UI action.

Persisted application tabs are read by `session::restore_from`
(`rust/crates/tiller/src/session.rs:924-1022`) from `tab` and `tab_state`, and
recreated by `restore_tabs` (`main.rs:7562-7682`). Their control projection is
published only by `TillerWorkspace::sync_control_panes` (`main.rs:3348-3395`),
which runs through `sync_activity` (`main.rs:3703-`) while the GPUI workspace
starts. The socket accepts `ping` before that sequence completes.

**Builder action:** none for persistence. If the socket contract requires a
boot-ready signal, add one deliberately; otherwise the verifier must poll a state
method that proves the workspace is ready. `F-CORE-WSP-08` needs that re-drive,
not a restore rewrite.

### 2. Settings-surface selection is outside `SettingsSnapshot` — high confidence

`control_select_settings` calls `open_settings` (`main.rs:4734-4743`), which sets
`show_settings` and calls `Settings::select_category` (`main.rs:4668-4681`).
`Settings::select_category` only assigns its in-memory category and notifies
(`rust/crates/tiller_ui/src/settings.rs:1004-1007`). `SettingsSnapshot`
(`settings.rs:954-975`) has persisted user preferences but neither `category` nor
surface visibility; `Settings::with_snapshot` defaults the category to Appearance
(`settings.rs:777-833`).

This explains both the empty `setting` table and “Settings surface is not open”
after restart. It is a separate state-contract question, not evidence that
persisted settings fail to load.

### 3. Native session references persist but are never resumed — high confidence

`SessionStore::save_session_ref` and `load_session_refs` do write/read the SQLite
map (`rust/crates/tiller/src/session.rs:1193-1231`). Boot loads the map at
`main.rs:8110`; the handler uses it only to add `sessionRef` to `system.identify`
responses (`main.rs:910-949`). There is no application call that maps a saved
reference to a restored tab/pane or invokes an adapter resume command.

The persisted `resume_agent_sessions` preference is also only carried through
settings/persistence/UI reporting in this tree; it has no launch-time consumer.
So an app-level “resume the native agent session after restart” feature is absent,
but that is distinct from tab persistence and from the agent-package APIs named by
`F-AGENT-SESSION-01` and `F-AGENT-SESSION-02`.

## Relationship to P113's UI-chat persistence cluster

This is **not** the same cause as P113's `F-PER-01` / `F-PERSIST-DB-05` cluster.
That cluster is a user-facing `ChatView` bypass of transcript/session persistence:
its chat rows are never written from the UI path. Here, workspace tab rows and
`session_ref` rows demonstrably exist and survive the restart. The overlap is only
architectural: both defects arise when a stored object is not connected to the
consumer that should use it.

There is therefore no honest four-row P113-style builder cluster. The evidence
splits into one verifier readiness error (`F-CORE-WSP-08`), one transient UI-state
observation (`F-CORE-SET-01`), and an independent missing native-resume integration
that the two agent-session rows do not themselves require.

## Confidence and limits

**Confidence: high** for the timing/persistence classifications and the source
locations; they are backed by the repeat, the SQLite rows, and the direct boot and
control paths above.

I did **not** establish a fresh-build behavior for the exact source tree: a narrow
`cargo build -p tiller` was blocked by a concurrent incomplete change,
`main.rs:8018` missing `AppSettings.opencode_workspace_id_override` (capture in the
session output; no source was edited). The verified executable is the existing
`rust/target/debug/tiller`, the same headless instrument used by P116. No claim here
says that a future freshly compiled tree has a different result.
