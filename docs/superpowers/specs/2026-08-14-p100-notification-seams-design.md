# P100 notification seams design

## Goal

Connect the existing desktop notification backend to explicit control-socket notifications and restore persisted agent identity into the live activity model.

## Design

`AppControlHandler::record_notification` remains responsible for storing the control notification row, then invokes the existing `post_desktop_notification` backend with the caller-supplied title and body. The handler keeps the current success/error contract: a `notify-send` spawn failure is logged by the backend and does not turn a successful control request into a failure. A small injectable poster seam makes the handler path testable without replacing the production backend.

Both tab restoration builders receive the workspace's `AgentActivityModel` while rebuilding tabs. For each restored tab with an `agent_id`, they register that identity against the restored root pane as `pane-{root_id}` through one shared helper. Registration only establishes identity; it does not claim `.running`, so the first real hook/title transition remains authoritative. The initial launch transfers the populated model into `TillerWorkspace`; additive session restore registers directly into the existing model.

## Boundaries and non-goals

- Do not change `NotificationPolicy`; explicit `notification.create` is not an observed activity transition and bypasses transition suppression.
- Do not rewrite or duplicate `notify-send` delivery.
- Do not alter persistence schema, tab icons, notification list/clear semantics, or `INVENTORY-LEDGER.md`.
- Do not infer restored running status.

## Verification

Automated regressions will assert that `notification.create` invokes the poster with the exact title/body and that restore registration makes a persisted agent pane discoverable without a status. Runtime evidence will include a `dbus-monitor` `Notify` call from `notification.create`, plus a real process restart with the restored agent in a background tab and both a suppressed visible case and a delivered background case.
