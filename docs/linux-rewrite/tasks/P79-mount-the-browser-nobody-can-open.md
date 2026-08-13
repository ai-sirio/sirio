# P79 — mount the browser nobody can open

**Owner: `codex12`.** Nine rows. Almost all of the code already exists and passes its own tests.

Read `../SEAMS.md` (this is the first entry in it), `../ENVIRONMENT.md` and `../OWNERSHIP.md`.

## Nine rows, one cause

Every `F-BRW` row in the ledger — all nine — reads `FAILED — absent` with the identical evidence:
*"no browser surface; browser.\* answers specific unsupported errors"*. **That statement is now
false.** `codex11` built the surface tonight: `tiller_ui/src/browser.rs`, 1283 lines, clippy-clean,
rustfmt-clean, its own tests green.

It is reachable from nothing:

```
BrowserState     0 references outside browser.rs
BrowserSurface   2 references outside browser.rs — both in crates/tiller_ui/examples/
```

`codex11` was right not to mount it. The mount point is `main.rs`, which is yours, and it left the
seam deliberately rather than reach across. **Your half is what turns nine rows.**

## What each row needs, and what already serves it

Do not rebuild any of this. Read `browser.rs` first and wire what is there.

| row | what the inventory demands | already built |
|---|---|---|
| `F-BRW-01` | open a browser tab; chrome, URL, favicon/globe, content | `BrowserSurface` (`Render`), `address()`, `page_title()` — **needs the tab kind and the palette entry, below** |
| `F-BRW-02` | Back, Forward, Reload, Stop | `go_back`, `go_forward`, `reload`, `stop_loading`, `can_go_back`, `can_go_forward` |
| `F-BRW-03` | enter a URL and submit from the address field | `submit_address` |
| `F-BRW-04` | invalid-address and navigation-failure errors | `did_fail_navigation`, `error()`, `BrowserError` |
| `F-BRW-05` | show when an agent is driving | `set_agent_driving`, `agent_driving()` |
| `F-BRW-06` | Allow/Deny prompt for an ungranted origin | `request_permission`, `allow_permission`, `deny_permission`, `PermissionPrompt` |
| `F-BRW-07` | persist an allowed origin **across relaunch** | persistence v11 + `set_allowed_origins`, `allowed_origins` |
| `F-BRW-08` | revoke one origin, or all, **from Permissions settings** | `revoke_origin`, `revoke_all_origins` — **the settings UI is a second seam, see below** |
| `F-BRW-09` | open an HTTP link internally; modifier bypasses to the system browser | `open_link`, `BrowserLinkTarget` |

## The mount

Two things, both in your files.

1. **A browser tab kind and a way to create one.** The pass-8 census recorded what it searched for
   and did not find: `TabKind::Browser` and a command-palette **"New Browser"** entry. That is the
   shape `F-BRW-01` is written against — a user must be able to *open* one.
2. **Route the control methods.** `main.rs:188-193` lists `browser.open`, `browser.navigate`,
   `browser.get`, `browser.screenshot`, `browser.snapshot`, `browser.act`, and they currently answer
   specific unsupported errors. Point them at the live surface. `F-BRW-05` needs
   `browser.act` to set `agent_driving`, and `F-BRW-06` needs an ungranted origin to raise the
   prompt rather than fail.

## One constraint you must not design around after the fact

P72 settled this and it is architectural, not a bug: **the WebKit view is a native child window that
sits above GPUI's GL surface and cannot be reordered.** Anything you draw over the webview rectangle
will not be visible.

So the tab must keep **all interactive chrome outside the webview rectangle**, and the surface must
resize its logical bounds on layout change. `codex11` documented the ruling and the hide/show
fallback in `tasks/P72-the-browser-spike.md` with a screenshot; read it before you place anything.
The Allow/Deny prompt is a **doorhanger anchored in the chrome** for exactly this reason — it is not
a sheet over the content, and that was a deliberate adaptation of the macOS `NSAlert.beginSheetModal`,
verified against the reference rather than guessed.

## The second seam — do not build it, name it

`F-BRW-08` says *"from Permissions settings"*. `settings.rs` is **`sonnet`'s file.** The revoke
logic exists (`revoke_origin`, `revoke_all_origins`); what is missing is a Permissions section that
lists granted origins and calls them.

**Register it in `SEAMS.md` and name what `sonnet` must add, in one line.** Do not edit `settings.rs`.
If you find `F-BRW-08` needs a query you have not exposed — "list current grants" — publish it and
say so; that is the seam's whole content.

## Done means

1. A browser tab can be **opened by a person using the app**, not by an example binary.
2. `F-BRW-07` is proven the only way it can be: allow an origin, **quit, relaunch**, and the grant is
   still there. A field that round-trips in a unit test does not settle this row.
3. `F-BRW-08` registered in `SEAMS.md` as a seam for `sonnet`, with the change named.
4. The binary builds. It is red right now on `main.rs:2654` (`reorder_sidebar`) — your own in-flight
   seam. Clear it.
5. `Scripts/transplant-check.py` run and its result stated. `browser.rs` came from `codex11`, but the
   mount is new code and waku has a browser.

**The critic will open a browser tab and navigate to a real page.** Nine rows depend on it being able
to reach one. Nothing else in this brief matters as much as that sentence.
