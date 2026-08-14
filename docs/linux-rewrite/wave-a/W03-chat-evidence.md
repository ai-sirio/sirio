# W03-chat evidence

## F-CHAT-13 — ledger line 162

- **Claim:** exercised-broken (code-absent; supports triage reclassify to FAILED — absent)
- **Drove:** `grep -rn ExternalPaths rust/crates/` (repo-wide, no hits) and `grep -n "on_drop\|ondrop" rust/crates/tiller_ui/src/chat.rs` (no hits, confirmed chat.rs is at `rust/crates/tiller_ui/src/chat.rs`). Confirmed the attach UI is picker-only: `attach_image` (chat.rs:1960) and `attach_button` (chat.rs:5108) exist, wired to a click-driven image picker, not a drop target.
- **Observed:** No `ExternalPaths` type or drag-and-drop handling anywhere in `rust/crates/`. `chat.rs` has zero `on_drop` callbacks. This corroborates the record's independent finding that `wayland-virtual-pointer.c` has no drag-offer/`wl_data_device` call, and the prior live attempt where the '+' control was reached (cursor confirmed) but produced no popup/dialog. Three independent checks (two live-drive passes on different lanes, one direct source grep) now agree: the feature is source-absent, not merely unreached by this lane's tooling.
- **Discriminating:** yes — this is a source-level absence check, not a default-state capture; a `grep` hit for `ExternalPaths` or `on_drop` would have falsified the "absent" claim.
