# Wave G slice G3-gaps-b — critic verdicts

Independently re-verified at HEAD (`c1ecc7c`) by fresh grep, not by trusting the brief's
pre-filled "latest critic evidence" text or the builder's report. All three rows confirm as
genuinely absent features with no wired route; static grep is the correct instrument here since
the claim under test is non-existence of a call site, not runtime behaviour.

## F-CORE-WSP-04 — FAILED — absent

`grep -rn "LayoutCommand\|classify_layout_command" rust/crates --include=*.rs` returns 15 hits,
all inside `tiller_project/src/layout.rs` plus the bare re-export at
`tiller_project/src/lib.rs:65`. Zero hits under `rust/crates/tiller/src/`. The app's real
split/tab state (`OpenTab.panes: PaneNode<TabContent>` in `main.rs`) is a separate, independently
mutated structure; nothing constructs or applies a `LayoutCommand`. Matches builder's own
"blocked" claim.

## F-CORE-WSP-08 — FAILED — absent

`grep -rn "WorkspaceTabViewState" rust/crates --include=*.rs` returns 5 hits, all inside
`layout.rs`/its `lib.rs` re-export, zero under `crates/tiller/src/`. Cross-checked the type it
would need to feed: `SessionTabState` (`session.rs:79`) has exactly three fields (`root_id`,
`pane_events`, `scrollback`) — no `editor_caret`/`editor_folds`/`chat_draft` field or accessor
exists anywhere in `session.rs`, `chat.rs`, or `editor.rs` (grep for those identifiers returns
zero writer/reader hits; the only chat persistence found is an unrelated
`save_chat_transcript`/`load_chat_transcript` pair, not part of this type). Restart-survival pass
bar is unreachable by construction.

## F-TAB-11 — FAILED — absent

`TerminalContextItem` (`context_menu.rs:30`) has exactly `label`/`action`/`route`, no
`enabled`/`disabled_reason` field; `ITEMS` is a flat `const [TerminalContextItem; 12]`; `items()`
(`context_menu.rs:99`) takes no parameters. Confirmed the render site
(`tiller_terminal/src/lib.rs:1471`, `for (index, item) in context_menu::items().iter()...`) never
branches on a disabled state — every item is uniformly clickable. `split_disabled_reason`
(`panes.rs:218`) carries `#[allow(dead_code)]`, is `pub(crate)`, and its only non-definition
references are its own three `#[cfg(test)]` tests (`panes.rs:798/818/831`) — no call from
`context_menu.rs`, `lib.rs`, or `main.rs`. Matches builder's own "blocked" claim, including the
identified semantics ambiguity (`tab_count` meaning) as the actual blocker, not a missed call
site.
