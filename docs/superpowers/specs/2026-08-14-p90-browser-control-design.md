# P90 browser control honesty design

## Goal

Make the browser control socket truthful without implementing browser automation. Every
`browser.*` request must either perform the behavior that is currently implemented or return an
explicit error naming the requested method and why it is unsupported.

## Scope and capability policy

The capability list advertises only methods that have a real action:

- `browser.open`: create a browser tab from a required URL and return its surface id, normalized
  URL, and current title.
- `browser.navigate`: navigate an existing browser surface when a URL/address/href is supplied.
- `browser.act`: update the browser driving indicator when `driving` or `agentDriving` is supplied.

The seven observation/automation stubs (`get`, `screenshot`, `snapshot`, `wait`, `eval`, `console`,
and `errors`) remain dispatchable but are not advertised and return explicit unsupported errors.
Unsupported `browser.act` verbs and `browser.navigate` history verbs (`back`, `forward`, and
`reload`) follow the same error path. No F-BRW verdicts or inventory rows are edited.

## Dispatch and action flow

`ControlAction::Browser` gains the same `ControlReply` channel used by the other queued actions.
The control handler validates only request shape that can be determined without the UI, enqueues
the method and parameters, then waits for the UI action result. The UI worker calls
`handle_browser_action` and sends either the returned pairs or an error through the reply channel.

`browser.open` rejects a missing or blank URL/address before tab creation. The existing
`add_browser_tab` path remains responsible for creating the surface. After creation, the new
surface is read to return its stable pane/surface id, normalized address, and page title.

Non-`open` methods search for an existing browser surface and fail if none exists; they never create
a tab as a side effect. Navigation continues to use the existing `BrowserSurface` URL submission.
The driving flag continues to use the existing surface state mutation, while any other `act`
payload is rejected explicitly.

## Errors

All unsupported or invalid requests use `ControlResponse::failure` through the existing queued
action path. Error text includes the exact method and a reason, distinguishing unsupported
behavior from an unknown control method and from a missing browser surface. Successful responses
remain string key/value pairs, matching the control protocol.

## Tests and verification

Rewrite the current browser dispatch test so it asserts that only the three supported methods are
advertised, unsupported methods return `ok: false` with method-specific errors, and a browser
request is queued with a reply rather than receiving a fabricated immediate success. Add coverage
for the missing `browser.open` URL and the no-surface side-effect guard where the existing test
fixtures allow it. Run the focused Rust tests and then prove the final responses over the live Unix
socket with a raw client, recording the actual responses in the task report.
