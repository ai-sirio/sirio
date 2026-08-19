# F-CORE-AUTH — finish-line critic pass (independent re-drive)

Fresh, independent critic pass over **F-CORE-AUTH-01/02/03** (Package tier / TillerCore domain
sub-group: auth-related domain logic). Run 2026-08-19 on this host, against the warm binary at
`/dev/shm/tt/debug/tiller` (built from the checked-out `linux/gpui-waku` worktree). I did not write
this code, and I did not take the ledger's existing `PASSED` verdicts as truth — every row below was
re-driven live this pass, through my own isolated instance (label prefix `sweep19auth*`, outdir
`/dev/shm/sweep-19-F-CORE-AUTH`), plus my own fresh run of the unit-test suites and one
external-crate reproduction I wrote to pin the defect below to the exact real bytes involved.

Contract text quoted below is `docs/linux-rewrite/02-inventory-packages.md`'s F-CORE-AUTH-01/02/03
entries, not the ledger's prose evidence column.

**Headline: F-CORE-AUTH-01's own ledger `PASSED` is wrong.** The Claude identity parser silently
drops the organization field for the real Claude CLI's actual JSON shape, and the project's own
original design spec — captured live, with the real field name — proves this isn't a matter of
interpretation. Full reproduction below.

## Setup used

```
cd rust && CARGO_TARGET_DIR=/dev/shm/tt CARGO_PROFILE_DEV_DEBUG=none \
  cargo test -p tiller_usage --test p99_account_identity
cd rust && CARGO_TARGET_DIR=/dev/shm/tt CARGO_PROFILE_DEV_DEBUG=none \
  cargo test -p tiller_usage --lib account::
```
Both run **today, by me**: 3/3 `p99_account_identity` pass, 6/6 `account::tests` pass.

Live drive: real installed `claude` CLI (2.1.235, logged in as `e.palmisano@reply.it` — the
host's own real account, read-only, nothing written to it), real installed `codex` CLI (present,
not logged in — a genuine negative control, not a fixture). A throwaway git fixture at
`/dev/shm/sweep19auth-fixture` stood in for the project/worktree the app needed to reach Settings
through. For F-CORE-AUTH-03, a scratch `TILLER_CREDENTIALS` file under `/dev/shm` was used for
every write — the real user's credential store was never touched.

**Harness note, not an app defect, disclosed up front:** the Settings detail pane does not respond
to synthetic wheel-scroll (`scroll` in `wayland-drive.sh`) on this screen — I independently
reproduced the exact gap `wave-h/H5-drive-report.md` already recorded (`scroll 700 700 -5` at the
AI Providers page was a byte-identical no-op both before and after, confirmed by diffing the two
capture files). `wayland-drive.sh`'s nested compositor has no window-resize verb of its own, so
instead of fighting the missing scroll I drove `swaymsg -s "$SWAYSOCK" output HEADLESS-1 resolution
<W>x<H>` directly from inside my action block (a legitimate use of the `$SWAYSOCK`/`click`/`grim`
primitives the script already exports into that shell — I did not edit the script file itself),
mirroring what H5 did on X11 with `xdotool windowsize`. That let every row below be driven by a
real pointer click on real rendered pixels, with no card left off-screen. This host was also under
severe, variable load (30+ concurrent `tiller` processes, load average 51 on 12 cores) — three
consecutive attempts at the default 10s settle produced a genuinely blank first frame
(`MESA: error: ZINK: failed to choose pdev`, never recovering within the settle window); raising
`SETTLE` to 40s made every subsequent boot render cleanly (4600+ colours). Recorded as environment
load, per this project's own established convention for this exact failure shape, not a lane or app
defect — nothing below rests on a frame captured during a fast-settle attempt.

## Rows

| row | verdict | evidence |
|---|---|---|
| F-CORE-AUTH-01 | FAILED — defective | Contract: "Account identity parsing reports Claude logged-in/email/organization fields from JSON and uses the first nonempty Codex credential line as identity." The **logged-in + email** half and the **Codex** half both hold: live screenshot `04-03-settings-ai-providers.png` shows Claude Code's card reading "● Signed in e.palmisano@reply.it" (the real, live-read state) and Codex's card reading "○ Not signed in" in the same frame (negative control — the real `codex` CLI on this host is genuinely unauthenticated); `AgentAccountIdentity::parse_claude_json`/`parse_codex_identity` are confirmed live as the actual functions behind that render by direct code trace (`tiller_ui/src/settings.rs:520-568`'s `discover_claude_identity`/`discover_codex_identity`), and `p99_account_identity.rs`'s `codex_identity_is_the_first_nonempty_line` covers the Codex clause exhaustively at the unit level. **The organization clause is defective**, proven three independent ways: (1) this host's real `claude auth status` — the exact command `discover_claude_identity` shells out to — prints `"orgName":"e.palmisano@reply.it's Organization"`, not `organization`/`organizationName`/`organization_name`, the only three keys `account.rs:56-59`'s `first_string` call searches; (2) I fed that exact captured JSON to the real `AgentAccountIdentity::parse_claude_json` through a throwaway external crate (`/dev/shm/sweep19auth-repro`, depending on `tiller_usage` by path, deleted after use) — output: `AgentAccountIdentity { logged_in: true, email: "e.palmisano@reply.it", organization: None }`; (3) the live screenshot itself shows only the bare email, no "· <org>" suffix `format_account_identity` would append if organization were `Some`. The project's own original design spec, `docs/superpowers/plans/2026-07-08-agent-account-management.md`, independently pins the real key: `claude auth status` — prints JSON: `{"loggedIn":true,"email":"...","orgName":"...", ...}` (exact real output, captured this session)`, its own `ClaudeAuthStatus: Decodable` type declares `orgName: String?`, and its own test asserts `identity?.orgName == "e.palmisano@reply.it's Organization"` — so this isn't a case of the row's wording being stricter than what was ever intended; the port lost a field its own spec named correctly. |
| F-CORE-AUTH-02 | PASSED | Contract: "A macOS skill-install action opens Terminal.app through AppleScript and runs `npx skills add e-palmisano/tiller --skill tiller -a claude-code,codex,opencode,pi -y`." PLATFORM clause (AppleScript/Terminal.app are macOS-only) is satisfied by the Linux equivalent: `WorkspaceAction::InstallSkill` opens an in-app terminal tab and runs the shell directly (`main.rs:2267`'s `skill_install_shell` → `TerminalShell::WithArguments`), a non-GUI terminal-launch policy exactly as the PLATFORM note calls for. Driven live, all the way through: resized the Settings > General page tall via the compositor trick above, clicked the real "Install Skill" button at its real rendered pixel — the card immediately grew a "Installing… running in a new terminal tab." status line (`03-install-skill-clicked.png`). Went Back to the worktree and found a genuine new terminal tab titled "Install Skill" (`panel.list` returned it as `pane-0`) showing the real interactive `npx` prompt: `Need to install the following packages: skills@1.5.23  Ok to proceed? (y)`. Typed `y` + Enter through the real virtual keyboard and the **real** `skills` CLI took over: `claude-code_2-1-234_agent  Agent detected — installing non-interactively`, `Source: https://github.com/e-palmisano/tiller.git`, `Cloning repository…` (`04-after-y-confirm.png`) — the literal `e-palmisano/tiller` argument from `agent_skill_install_command()` reached a genuinely running external tool and was correctly parsed as the repo source. Confirmed no side effect survived: no `~/.claude/skills/tiller` (or any `e-palmisano` path) exists on this host afterward, and no `npx`/`skills`/`git clone` process was left running — `wayland-drive.sh`'s environ-matched cleanup reaped the whole descendant tree when the lane exited. |
| F-CORE-AUTH-03 | PASSED | Contract: "`KeychainCredentialStore` gets, sets, and deletes usage credentials under the `com.tiller.usage` service." PLATFORM clause (macOS Keychain vs. Linux) is satisfied by `CredentialStore` (`tiller_usage/src/credentials.rs`) — plaintext JSON at mode `0600` under XDG data dir, an explicit, documented, deliberate choice (see Defects below for the one open question this still leaves). VERIFY ("save, read, replace, and delete a provider credential, restart, and confirm persistence and deletion") driven live and completely, through the OpenCode Go card, with `TILLER_CREDENTIALS` pointed at a scratch file the whole time: **save** — typed a scratch cookie into the real masked field, clicked the real Save button; status flipped "Not signed in" → "Signed in", "Show in usage bar" auto-flipped on, field cleared back to its placeholder (`03-after-save.png`); the scratch file, read directly (not just trusted from the UI), now held exactly `{"opencode-go-cookie":"auth=Fe26.2**sweep19authScratchCookieValue"}`, i.e. the round trip is byte-exact, not just "some value appeared". **delete** — clicked the real Clear button in the same session; status flipped back to "Not signed in"; the file became exactly `{}`. **persistence across restart** — a *second*, genuinely fresh app process (new label, new PID, zero interaction beyond opening Settings > AI Providers) launched with the same scratch `TILLER_CREDENTIALS` (pre-seeded directly on disk, bypassing the UI, with a different scratch cookie) showed OpenCode Go as "Signed in" on first paint, sourced entirely from disk (`02-fresh-restart-read.png`) — proving the read half survives a real process boundary, not just in-memory state. This independently reproduces `wave-h/H5-drive-report.md`'s prior finding rather than merely re-citing it. |

## Defects

**F-CORE-AUTH-01 — Claude organization is never parsed from the real CLI's output.**
`tiller_usage/src/account.rs:56-59`:
```rust
let organization = first_string(
    account,
    &["organization", "organizationName", "organization_name"],
);
```
`claude auth status`'s real key is `orgName` (confirmed live on this host, and independently pinned
in the project's own original spec, `docs/superpowers/plans/2026-07-08-agent-account-management.md`
line 42 and its `ClaudeAuthStatus.orgName` decoder) — none of the three candidate keys match it, so
`organization` is `None` for every real Claude Code installation, and the Settings "AI Providers"
card shows a bare email forever, never the `email · organization` form `format_account_identity`
is written to produce.

**Reproduction (replayable without any of my scratch files, which are ephemeral under `/dev/shm`):**
```
$ claude auth status
{
  "loggedIn": true,
  ...
  "orgName": "e.palmisano@reply.it's Organization",
  ...
}
```
Feed that exact text to `tiller_usage::AgentAccountIdentity::parse_claude_json` (e.g. from a
throwaway crate depending on `tiller_usage` by path, or by temporarily extending
`p99_account_identity.rs` with a case using the key `orgName` instead of `organizationName`) and
observe `organization: None` — while `claude_identity_parses_representative_json_shapes` in the
same test file passes today only because every one of its fixtures already uses a key from the
matched list (`organizationName`, `organization_name`, `organization`), never the real CLI's
`orgName`. That test suite is internally consistent and green; it just never encodes the shape the
real tool actually emits.

**Fix shape** (not applied — this is a review, not a repair): add `"orgName"` to the key list at
`account.rs:58`. One-line, well-contained; the existing test file's structure (`P99 exercise of
F-CORE-AUTH-01`) is exactly where a case using the literal captured JSON above belongs.

**F-CORE-AUTH-03 — plaintext-vs-encrypted-store deviation, carried forward, not re-opened.**
The row's third VERIFY option is "an explicitly chosen *encrypted* store"; `CredentialStore` is
plaintext JSON at mode `0600` (its own doc comment defends this explicitly, arguing a
Secret-Service/keyring store is actively hostile to this project's parallel-test/headless-CI needs).
I independently confirmed the deviation is real by reading the scratch file mid-drive: the cookie
sat in it as literal plaintext (`{"opencode-go-cookie":"auth=Fe26.2**sweep19authScratchCookieValue"}`),
not any encrypted or hashed form. This is the same finding `wave-h/H5-drive-report.md` already
raised and left for a human ruling; I re-drove and re-confirmed it rather than re-asserting it
unverified, and I have nothing to add beyond that confirmation — still a human tradeoff call, not a
functional break, and not why the row is marked PASSED above (both read and write halves of the
row's own VERIFY text are unconditionally satisfied).

## What I could not reach, and why

- **The live Claude/Codex `Add Account` OAuth login flow** (the "Accounts" subsection's "Add
  Account" button, visible on every provider card in my screenshots) spawns a real system browser
  OAuth handshake per `tiller_ui/src/settings.rs`'s `launch_account_login`. I did not click it: doing
  so would have opened a real browser against `claude.ai`/real `chatgpt.com` OAuth and risked mutating
  a real session under this host's real logged-in account, which is out of scope for what F-CORE-AUTH's
  three rows actually ask (that flow belongs to F-SET-14/F-SET-15, not this section) and not something
  a read-only review pass should trigger. Not needed for any of the three rows above — the "Accounts"
  subsection's presence is incidental context in my screenshots, not part of what F-CORE-AUTH-01/02/03
  contract for.
- **A live positive Codex identity string** (Codex actually logged in, showing a real `Logged in
  using…` line). This host's real `codex` CLI is genuinely unauthenticated, and I deliberately did not
  fabricate credentials to force a positive path, since `discover_codex_identity` shells out to the
  real `codex login status` binary and a garbage/fixture `auth.json` would only prove the CLI rejects
  garbage, not that the parser correctly reads a real positive line. This clause instead rests on
  `p99_account_identity.rs::codex_identity_is_the_first_nonempty_line`'s exhaustive unit coverage
  (first-line selection, blank/whitespace-line skipping, trimming, all-empty input) — the same
  standard of evidence the row's own VERIFY text describes ("provide representative … Codex credential
  text … and inspect the parsed identity"), just not re-proven live on top of that since there is no
  real positive fixture on this host to drive it through honestly.
