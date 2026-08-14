# P118 report — project icon and notification boundaries

P118 was already source-wired in this checkout before this verification pass:

- project settings propagation/persistence is present in `28a41fa` (`ProjectIconPicker` →
  `Sidebar` → `ProjectSettingsChanged` → `ProjectCatalog` → sidebar refresh);
- notification delivery is present in `a5d09d7` (`notification.create`/`notify --title` →
  `record_notification` → `post_desktop_notification` via `notify-send`).

I added no overlapping source hunk. The live drives below verify that those existing paths cross
their boundaries. `rust/crates/tiller_ui/src/chat.rs` was not opened or modified.

## Contract decision — Cluster B

`notification.create` is a delivery API with inbox history, not an inbox-only API. The inventory
rows require a user notification to be posted, while `notification.list` separately covers the
in-memory history. `record_notification` therefore records the row and invokes the desktop poster.

## F-PRJ-13 — project icon reset and colour

**Changed:** No additional source change was needed. The picker callback now reaches
`Sidebar::apply_icon_change`; the sidebar emits `ProjectSettingsChanged`, and the app persists
`ProjectIcon::persisted_parts()` before refreshing the sidebar. Reset uses the same callback path.

**Drove:** Wayland lane, using the fixture database selected by `TILLER_WL_LABEL=p118-a-reset2`:

```text
TILLER_WL_LABEL=p118-a-reset2 Scripts/wayland-drive.sh reference/linux-progress/p118-reset2-retry2 'shot existing; move 300 121; click 300 121; shot settings-open; click 80 431; sleep 2; shot reset-settled' 2
```

`reference/linux-progress/p118-reset2-retry2/04-reset-settled.png` shows the folder glyph,
coral tint, and the full `Colour` row after Reset. The preceding live capture
`reference/linux-progress/p118-reset/05-green-tint.png` shows the green tint selected without
clipping. After relaunching the same fixture database:

```text
TILLER_WL_LABEL=p118-a-reset2 Scripts/wayland-drive.sh reference/linux-progress/p118-reset2-restart 'shot reset-restart-sidebar' 1
SHOT reference/linux-progress/p118-reset2-restart/02-reset-restart-sidebar.png (1715x972 · 9843 colours)
```

That restarted sidebar capture shows the folder icon. The text-observable persisted row is:

```text
('p-c1fd7a5bbfd541af', 'tiller', 'icon', 'folder', 'coral')
```

The earlier selected-glyph drive similarly persisted `icon_value='git-branch'`; its restart
capture is `reference/linux-progress/p118-restart/02-restart-sidebar.png` and the SQLite query
returned:

```text
('p-c1fd7a5bbfd541af', 'tiller', 'icon', 'git-branch', 'coral')
```

The clipped Colour row did not survive the wiring fix; it is not a separate remaining defect.

## F-PRJ-15 — project SF Symbol/glyph selection

**Changed:** No additional source change was needed. The selected grid glyph is propagated by the
same callback and persisted through the project catalog.

**Drove:**

```text
TILLER_WL_LABEL=p118-a-select Scripts/wayland-drive.sh reference/linux-progress/p118-select 'ctl project.add path=/home/enzopalmisano/Scrivania/Progetti/tiller-linux; shot project-added; move 300 121; click 300 121; shot settings-open; click 106 274; shot glyph-selected; click 45 510; shot sidebar-after-close' 1
```

The captures show the git-branch selection ring in
`reference/linux-progress/p118-select/04-glyph-selected.png` and the git-branch icon in the
sidebar after Close in `reference/linux-progress/p118-select/05-sidebar-after-close.png`. A
relaunch against the same SQLite fixture kept that icon; see the persisted row and
`reference/linux-progress/p118-restart/02-restart-sidebar.png` above.

## F-AUTO-06 — `notification.create` delivery

**Changed:** No additional source change was needed. `record_notification` already invokes the
configured `notification_poster` after storing the notification.

**Drove:** The preserved live command was:

```text
TILLER_SOCKET=/tmp/p118-notify-proof-9NvbpV/tiller.sock rust/target/debug/tillerctl notify --title 'P118 DBus proof' --body 'notification.create delivery'
```

The CLI output was empty (success). The preserved `dbus-monitor` output in
`reference/linux-progress/p118-notify/dbus-monitor.txt` contains the following exact delivery:

```text
method call time=1786722732.336375 sender=:1.2016 -> destination=:1.60 serial=9 path=/org/freedesktop/Notifications; interface=org.freedesktop.Notifications; member=Notify
   string "Tiller"
   uint32 0
   string ""
   string "P118 DBus proof"
   string "notification.create delivery"
   array [
   ]
   array [
      dict entry(
         string "urgency"
         variant             byte 1
      )
      dict entry(
         string "sender-pid"
         variant             int64 2421532
      )
   ]
   int32 -1
```

This is non-empty `org.freedesktop.Notifications.Notify` traffic with the requested title and
body, rather than the empty capture that created the ledger row.

## F-CTRL-NOTIFY-03 — control notification posts

**Changed:** Same delivery path as F-AUTO-06; no separate change was needed.

**Drove:** The command above uses the compiled `tillerctl` control client and the live Tiller
control socket. The D-Bus capture proves the control handler did more than return `ok:true`: it
caused `notify-send` to issue a real `Notify` method call containing the exact title/body.

## Tests

These are test results, not substitutes for the live proof:

- `cargo test --manifest-path rust/Cargo.toml -p tiller_ui project_settings_changes_update_the_row_and_emit_a_durable_edit -- --nocapture` passed before the shared-worktree drift;
- the existing picker reset/tint tests cover the component seam;
- the current rerun is blocked by an unrelated concurrent edit in
  `rust/crates/tiller_ui/src/status_bar.rs`: `OpenCodeGoUsageFetcher::fetch()` now requires
  `Option<&str>`, but the call site still invokes `fetch()` with zero arguments. I did not modify
  that file or the concurrent `chat.rs` work.
