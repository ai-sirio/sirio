# P93 accounts tier design

## Context

The Linux Settings surface already renders provider cards and an `Add Account`
button, but the button has no runtime handler. The usage crate already owns the
local account parsers, but the Settings view does not call them. The installed
CLIs provide their own authentication flows; Tiller does not own a separate
credential store.

The work must not touch `chat.rs`, `composer.rs`, or `tiller_acp/**` while P91
is active, and must preserve unrelated dirty changes already present in the
shared worktree.

## Design

1. Provider cards derive their displayed identity from the provider's real
   local state. Claude reads its credentials JSON and uses
   `AgentAccountIdentity::parse_claude_json`. Codex checks its auth file and
   parses the output of `codex login status` with `parse_codex_identity`.
   OpenCode Go remains an honest Linux `Unknown`/no-local-store state because
   its original cookie store is macOS-specific.

2. `Add Account` launches the installed provider CLI's own interactive login
   flow in an external terminal. Claude uses `claude auth login`, Codex uses
   `codex login`, and OpenCode uses `opencode auth login` where the installed
   CLI supports it. The app does not copy credentials, implement browser OAuth,
   or pretend that OpenCode's generic provider login is an OpenCode Go account.
   The provider state is refreshed after the terminal process exits. Existing
   host callbacks remain usable for tests and embedding.

3. The account list remains a single `System default` row. Investigation found
   no installed CLI/account store that supplies a selectable Tiller-owned set
   of accounts, and the persistence model has no account-selection field.
   F-SET-15 is therefore reported as an evidence-backed Linux limitation,
   rather than represented by a fake list or badge.

4. Usage state keeps the existing distinction between `Loading`, `Loaded`, and
   `Stale`. Add `TimedOut` as an unavailable reason for a timeout with no prior
   value; a timeout with a prior value remains `Stale`. The Settings/status-bar
   renderer must show distinct text for all four unavailable reasons:
   `not found`, `logged out`, `timed out`, and `error`.

## Verification

- Unit/render tests cover Claude and Codex identity discovery, account-action
  command selection, and all four unavailable reason labels.
- Focused Rust tests run before the full workspace gate, using the repository's
  cargo path workaround when needed.
- A live run takes the documented display lock, opens Settings, clicks the
  provider action, observes the external terminal login flow, and records what
  is actually observed. No ledger file is edited.
