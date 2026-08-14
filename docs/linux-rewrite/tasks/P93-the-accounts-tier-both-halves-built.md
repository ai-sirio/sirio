# P93 — the accounts tier, where both halves are already built and neither is connected

**Owner: `codex11`.** Start when `P89` is finished. Worktree
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.

Four rows, one feature: `F-SET-14`, `F-CORE-AUTH-01`, `F-SET-15`, `F-SET-11`.

`settings.rs` is yours. **You need no cross-crate change** — `tiller_ui` already depends on
`tiller_usage` (`tiller_ui/Cargo.toml`), so you can call the parser directly. `account.rs` is
`codex12`'s and should not need an edit; if you find one is required, say so rather than making it.

## Start here — the two dead halves

This is the rare piece where the work is almost entirely wiring. Audited 2026-08-14:

**The surface exists.** `Add Account` renders per provider (`settings.rs:1494`, ids
`add-claude`/`codex`/`opencode-account`) through `controls::button_maybe`. Its handler
`on_manage_account` (field `:625-641`, builder `:799`) has **exactly one caller in the workspace**,
at `settings.rs:3188`, **inside a test**. The doc comment even states the failure mode: *"Unset, the
button renders muted and does not respond to clicks."* In the running app it is always unset.

**The parser exists.** `AgentAccountIdentity::parse_claude_json` (`tiller_usage/src/account.rs:50`)
reads Claude's account JSON into `{logged_in, email, organization}` and deliberately does not treat
malformed input as a logged-out claim. `parse_codex_identity` (`:67`) takes the first nonempty
credential line. Both are tested. Both have **zero app callers** — the only references are the
definition and two tests, which is why `F-CORE-AUTH-01` moved to `UNREACHABLE` tonight.

So the parser has no caller and the button has no handler, and they are each other's missing half.
Connect them first: read the account file, parse it, render the identity on the provider card. That
alone closes `F-CORE-AUTH-01` and is the shortest path to something a critic can exercise.

## `F-SET-14` — what "Add Account" means on this platform

Clause: *Manage agent accounts with Add Account, re-authenticate, and remove. VERIFY: open an
account-capable provider, add an account, exercise browser-login waiting/cancel/retry,
re-authenticate an account, and remove it.*

The browser-login flow is from the macOS original. **Find out what the installed CLIs actually
offer** before building anything — `claude` and `codex` are both installed (`ENVIRONMENT.md`) and
their login story is whatever they implement, not what the Swift app did.

**Do not reach for `N/A — platform`.** That verdict is self-sealing and this project has already had
to reopen a batch of rows that used it — the excuse outlives the premise. If part of this clause has
no Linux equivalent, build the part that does, and write down the premise you are asserting so the
next pass can re-test it. A stated premise can expire loudly; an exemption cannot.

## `F-SET-15` — investigate before building

Clause: *With multiple accounts, select System default and another account, and confirm the active
badge moves to the selected row.*

Its ledger evidence is three words: "no multi-account model". **Establish whether one can exist here
before writing UI.** Does any installed CLI support more than one account at once, and is there
anywhere to persist a selection? If the answer is no, a written finding saying so is the correct
deliverable and a badge over a list that can only ever hold one row is not — it would survive being
looked at, which is the worst property a fake feature can have.

## `F-SET-11` — seven states, four visuals

Clause names seven: *Reading, Active, Stale, Not found, Logged out, Timed out, Error.*

Measured at pass 11: Loading/Loaded/Stale render distinctly with dimming, and **all four unavailable
reasons collapse to the same `'—'`**. The states are modelled and the *rendering* throws the
distinction away — check `LocalAccountState` (`account.rs:27`) for what the model already carries
before adding to it. This is the most self-contained of the four rows and a reasonable second commit
after the wiring.

## The rules

- **Inspiration, never code.** waku, Zed and orca are to look at, not to copy. A critic finding
  transplanted code counts as a gap, always.
- **Code plus a green test is `NOT EXERCISED`, never `PASSED`.** You are building, not judging, and
  this piece is *made of* dead-but-tested code — a passing test is exactly what these two halves
  already had.
- **Do not edit `INVENTORY-LEDGER.md`.** Report what you built and what you exercised.
- **Do not touch `chat.rs`, `composer.rs` or `tiller_acp/**`** while `P91` is in flight — see the
  amendment at the end of `OWNERSHIP.md`.
- `git status --short | grep '??'` before you finish.

## Done means

1. `Add Account` responds to a click in the running app, and the provider card shows a real parsed
   identity — the first thing that must be true, since it is the one both rows depend on.
2. `F-SET-11`'s four unavailable reasons render distinguishably.
3. `F-SET-14` and `F-SET-15` either built or reported unbuildable **with the evidence and the
   premise stated**.
4. Exercised live, not only under test: launch the app, open Settings, click the control, and say
   what you observed. Take the drive lock (`ENVIRONMENT.md` §"Holding the lock yourself") — `fable`
   and `codex12` are both driving.
