# Wave B slice B6-brw-bar — 5 rows to build

**browser surface and status bar**

## Files you own this wave

- `rust/crates/tiller_ui/src/browser.rs`
- `rust/crates/tiller_ui/src/status_bar.rs`

No other agent will touch these. **You must not edit any file outside this list** —
every other source file belongs to a sibling and an edit there is silently lost or
silently overwrites theirs. If a fix genuinely needs a file you do not own, stop and
report it rather than making the change.

## Rows

### `F-BRW-01` — ledger line 250, currently **FAILED — defective**

- **Triage:** build, size M
- **Files triage expects:** rust/crates/tiller_ui/src/browser.rs
- **Approach:** native_webview_rect (browser.rs:1510) converts GPUI logical-pixel layout bounds to device pixels via to_device_pixels(scale_factor) before calling wry's webview.set_bounds. Observed shrink ratio 729/850=679/792≈6/7 exactly matches the inverse of this file's own test-hardcoded 7.0/6.0 scale factor (:1706) — strong signature of a logical/device pixel inversion, possibly compounded by GTK/X11 widget geometry expecting logical pixels while this code supplies device pixels (double-scaling). Verify window.scale_factor() at capture time, check what wry/GTK actually expects, fix the conversion direction, then re-check the (331,114) offset.
- **Evidence on record:** opens and renders live: tab-bar `+` -> **New Browser** creates a Browser tab with a globe icon, GPUI chrome (`<` `>` `reload`, address field, page title, Stop) and a real WebKitGTK child showing `https://example.com/`; an in-page link navigated to iana.org with both the address field and the page title updating (`orch21-base.png`, `orch21-link.png`). **But the page is painted outside its pane**: p

### `F-BRW-02` — ledger line 251, currently **FAILED — defective**

- **Triage:** build, size S
- **Files triage expects:** rust/crates/tiller_ui/src/browser.rs
- **Approach:** History model (go_back/can_go_back, browser.rs:523-544) and button enablement (browser_button gates both hover style and on_click on the same `enabled` flag, :1190-1192) are both correct — the manifest's hover-landed positive control proves can_go_back() was true and on_click fired. But on_back/on_forward/navigate_history (:918-939) take no Context<Self> and never call cx.notify(), unlike the working on_address_key 'enter' path (:997) which does. Add cx.notify() to the back/forward chain; separately verify the native webview itself actually moved (self.webview populated, wry load_url effective) if the fix doesn't fully resolve it.
- **Evidence on record:** Back did not navigate, and **the code says it should have** — record the contradiction, do not "fix" it blind. Live: after an in-page link navigation to `iana.org/help/example-domains`, clicking `<` left the page *and the address field* unchanged after 4 s (`orch21-back.png`). Two positive controls say the click landed: the button renders its hover background in that same shot (magnified crop; hit

### `F-BRW-03` — ledger line 252, currently **FAILED — defective**

- **Triage:** build, size M
- **Files triage expects:** rust/crates/tiller_ui/src/browser.rs
- **Approach:** AddressEditor model and AddressTextElement's custom click-hit-testing (browser.rs:264-378, :1306-1503) are both correctly implemented, not stubs. Live contradiction most likely explained by pump_web_events (:1021) calling address_editor.set_text() on every WebEvent::PageLoad(Finished) — including repeated sub-resource/iframe 'finished' events — which resets the caret to end-of-text mid-edit. Instrument live to confirm whether PageLoad(Finished) fires repeatedly during editing, and separately confirm ctrl+a actually reaches on_address_key; fix by guarding/debouncing the set_text reset while focused/mid-edit.
- **Evidence on record:** Return **does** navigate — the child loaded the submitted URL and the title became `Page not found` (`orch21-address.png`) — but the field can only ever *append*: `ctrl+a` does not select its contents and the caret ignores click position (always end-of-text), so a typed URL is concatenated onto the existing one, producing `https://www.iana.org/help/example-domainshttps://www.iana.org`. Submitting 

### `F-USE-01` — ledger line 264, currently **FAILED — absent**

- **Triage:** build, size S
- **Files triage expects:** rust/crates/tiller_ui/src/status_bar.rs
- **Approach:** StatusBar has a real on_refresh: Option<Rc<dyn Fn()>> field and builder (status_bar.rs:113,160-162) but it is never invoked anywhere inside impl Render — no refresh control exists in the render tree at all, only the settings gear. Add a sibling icon_button next to the gear that calls self.on_refresh, using the exact same closure pattern already in render().
- **Evidence on record:** FAILED — absent: bar live-confirmed to show settings gear + worktree info, but no refresh affordance anywhere in it under hover/click across many captures, vs. the working "Refresh now" in Settings. Live-confirms prior code-read finding.

### `F-USE-02` — ledger line 265, currently **half-proven**

- **Triage:** build, size S
- **Files triage expects:** rust/crates/tiller_ui/src/status_bar.rs
- **Approach:** Zero .tooltip( calls anywhere in status_bar.rs. provider_segment (status_bar.rs:349-364) shows the unavailable reason only as permanent label text, never as hover content. Add .tooltip(...) to each segment div; decide whether it repeats the visible reason or adds detail (exact error/timestamp).
- **Shared cause:** Unavailable-segment half shares F-USE-03's broken PATH-strip-after-launch precondition.
- **Evidence on record:** half-proven: enabled-segment hover live-confirmed to show no tooltip. Unavailable-segment tooltip case never reached — the PATH-strip meant to force it did not actually flip the segment to unavailable (see F-USE-03).

