# H5-drive — critic verdicts

Independent verification of the H5-drive report (builder commit `7f59ba21`/`9ae8ec20`). This critic
did not build any of these rows; every claim below was re-driven or re-traced from scratch, not
inherited.

## `F-CORE-AUTH-03` — `PASSED` (deviation flagged for human ruling, not treated as a defect)

Independently re-drove the full VERIFY sequence myself, live, over three separate app processes
(`TILLER_WL_LABEL=h5critic1`, `TILLER_CREDENTIALS=/tmp/h5critic1-creds.json`, own scratch file, not
the builder's): resized the nested output past the window's own broken-scroll floor
(`swaymsg output HEADLESS-1 resolution 1715x1400`, confirming the builder's separate scroll-defect
note independently — `scroll` also no-ops here) to reach the OpenCode Go card, clicked the Session
cookie field, typed a cookie, clicked Save. `cat`ing the file directly (not trusting the UI alone)
showed `{"opencode-go-cookie":"mycritictestcookieVALUE1"}` at mode `0600`. **Killed and restarted**
the app (fresh `wayland-drive.sh` invocation, same label/env — a real new process, not an in-memory
reset) and captured the AI Providers page with zero interaction before the screenshot: OpenCode Go
read "Signed in" on first paint. Clicked Clear in that same live process; file became `{}`, status
flipped to "Not signed in". Restarted a third time and confirmed "Not signed in" persisted with no
interaction. This is the row's exact VERIFY clause (save/read/persist/delete/restart), independently
reproduced end to end, matching the builder's own drive.

Deviation independently confirmed, not just read off the report: `credentials.rs` stores plaintext
JSON at `0600`, not an encrypted store, against the contract's third PLATFORM option ("an explicitly
chosen *encrypted* store"). The VERIFY mechanics pass regardless of which of the three PLATFORM
options is judged to apply — marking `PASSED` on that basis, with the encrypted-vs-plaintext question
carried forward verbatim as a deviation for a human to rule on (per the row's own instruction not to
silently treat it as satisfied), not folded into the verdict as a failure.

## `F-CORE-FILE-03A` — `half-proven` (code trace independently re-verified line by line; live XDND still undrivable)

Did not accept the report's tracing at face value — reread every file it cites, at the vendored
pinned Zed checkout under `~/.cargo/git/checkouts/zed-a70e2ad075855582/c05e346/crates/gpui_linux` and
`gpui`, not the report's prose. Confirmed independently: `wl_data_device::Event::Enter` reads
`text/uri-list` through one pipe into one background task, then a single
`.lines().filter_map(Url::parse)...collect()` into a `SmallVec` — no per-file concurrency exists to
race; `Event::Drop` carries only `{position}`, no paths. `window.rs`'s `FileDropEvent::Entered` stores
that same `SmallVec` once as `Arc::new(paths.clone())` in `cx.active_drag`; `Submit` is a plain
synthetic `MouseUp` carrying no path data. `tiller_terminal/src/lib.rs`'s
`.on_drop::<gpui::ExternalPaths>` (now at line 1708, drifted slightly from the report's 1586 after an
unrelated intervening commit — same handler) calls `paths.paths().to_vec()`, and
`tiller_project::terminal_file_drop` is a plain order-preserving `.iter().map(...).join(" ")`. Every
link is confirmed single-pass and order-preserving, independently, not on the report's word.

Also found and ran a **pre-existing** test the report did not cite,
`a_drawn_terminal_accepts_a_real_external_paths_drop_with_several_files`
(`tiller_terminal/src/lib.rs`), which simulates a real two-file `ExternalPaths` GPUI drop (distinct
ordered paths, one with spaces) through the actual mouse-event dispatch path and asserts order is
preserved end to end — `cargo test -p tiller_terminal a_drawn_terminal_accepts_a_real_external_paths_drop_with_several_files`
passes. This is stronger than pure code reading (it exercises GPUI's real drop dispatch), but it is
still a GPUI-internal simulated drag, not a real compositor-delivered XDND drop — the row's
"resolves slowly" scenario and the actual OS-level gesture remain unexercised, matching the builder's
own "half-proven" call and its precise, buildable-not-vague description of the missing instrument
(a `wl_data_device_manager`-based DND source client). No code changed by this row; verdict unchanged.

## `F-CHAT-05` — `half-proven` (test independently reproduced; merits of the wording decision affirmed)

Ran the new test in isolation, from a clean build: `cargo test -p tiller_ui
offline_enter_never_discards_the_typed_draft` — 1 passed, 0 failed. Independently re-read `send()`'s
offline branch (`self.client.is_none()`) and grepped every `self.composer =` write site in
`chat.rs` (`control_compose`, `new_conversation`, `send`'s own post-online-check draft-consume, and
`commit_queued_item`, which only fires while `self.streaming`): none of the four run on a failed
offline retry, confirming the draft genuinely cannot be cleared on that path, independent of the
test's own assertions. The offline placeholder text ("Agent offline — reconnecting when you send…")
is present at `chat.rs:6299`.

Verdict held at `half-proven`, not raised to `PASSED`: the contract's literal "confirm the editor is
disabled" clause is still not what the code does (the editor stays enabled by design), so the row
does not match its own written VERIFY text word-for-word. What changed this pass is that the
data-loss half of the concern — the actually dangerous failure mode — is now proven false by a
deterministic, state-inspecting test rather than resting on a single contested live observation. The
builder's P131-grounded explanation for why the wave-G critic saw a "cleared" draft (stale capture
during the async reconnect race, not a real code path) is plausible and consistent with P131's own
findings, but wasn't independently re-driven live this pass — the deterministic test is treated as
stronger evidence for the specific question asked ("can Return ever discard the draft") than another
screenshot would be, per this wave's own capture-staleness findings. Recommend the ledger carry the
builder's proposed reword ("confirm a failed offline Send never discards what the user typed") for a
human to accept, rather than leaving the literal disabled-editor wording in place against code that
deliberately doesn't implement it.
