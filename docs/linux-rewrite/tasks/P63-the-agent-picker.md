# P63 — The agent picker, and three one-line truths

**You are codex12, pane `w1:p3`.** Your context was just reset, so this brief is everything you need.

## P60 landed, and you did the thing the brief was actually testing

`tab_is_dirty` is now shared by the dot and both confirmations, with a real definition per kind —
terminal live, editor unsaved, chat streaming. The brief warned that two paths able to disagree about
dirtiness is how "it asked me to confirm but showed nothing" gets built; you took the single
predicate.

You also fixed `cx.observe(&settings, …)` **with** `settings_visibility_toggles_reach_the_usage_bar`.
A one-word fix with no test is indistinguishable from the bug, and you did not ship it that way.

Your token gap is recorded for `sonnet`: `tiller_theme` has no menu-width or geometric-hairline
token.

## First: the workspace is red, and it is not yours

`cargo check --workspace` fails with `E0004` at `tiller_ui/src/sidebar.rs:1460` — `codex11` added
`ActivityStatus::NeedsInput` and an exhaustive match no longer covers it. `codex11` has been told to
fix it under a new standing rule (**whoever widens an enum owns every match arm it breaks, in any
file, but only those arms**). Do not fix it yourself, and do not report it as your failure. If it is
still red when you finish, say so and name it.

## The piece

### 1. `F-TAB-07` and `F-TAB-08` — the New Chat agent picker

The row: *"New Chat → generic `add_chat_tab("Chat")`; no ACP-agent submenu"*, and `F-TAB-08`: *"no
no-agent fallback / 'Other agents…'"*.

**This is on the project's acceptance path, not just the ledger.** The goal states that the critic
finishes by connecting *a real agent of the workspace over ACP*, sending messages, and verifying
streaming and replies. With no picker, there is nothing to choose — the acceptance test cannot be
performed on any agent in particular.

The machinery already exists, so this is mostly wiring:

- `tiller_agents::ALL` is the fixed five-adapter catalog in display order — Claude Code, Codex,
  OpenCode, Pi, Oh-My-Pi — with `id()`, `display_name()`, `has_native_hooks()`, `resume_command()`.
- `main.rs:18` already imports it as `AGENT_CATALOG`, and `main.rs:4017` already takes a
  `&dyn tiller_agents::AgentAdapter`.
- `discover_availability()` returns per-adapter availability, and `tiller_ui/src/settings.rs` already
  consumes it — so "installed or not" is answerable without inventing anything.
- You wired all six `surface.chat.*` methods in P57, so the chat side is yours and already known.

Make New Chat offer the catalog, carry the chosen adapter into the tab, and give `F-TAB-08` its
honest fallback for when nothing is installed. **Use `discover_availability()` rather than listing
all five unconditionally** — offering a menu item that cannot work is the "control that draws but does
nothing" defect this project keeps producing.

### 2. Three one-line truths, all in `main.rs`, all queued for you

**`TILLER_SOCKET_ENABLE` is inert in production.** `with_environment_override` is called only from
its own `#[cfg(test)]` block; the boot path reads saved settings and calls
`control_socket.set_enabled(saved_settings.control_socket_enabled)` (`main.rs:6167`, `:6209`), so the
env var is never consulted. Documented, tested, mirrors macOS, does nothing. One call in the boot path
makes it true. Bears on `F-CORE-SET-01` and the `F-AUTO-01` disable story; no ledger row owns it, so
add one to your report rather than to the ledger.

**`F-EDIT-08` dedupe.** `add_file_tab` (`main.rs:3847` area) pushes unconditionally, so opening the
same file twice yields two tabs. `codex11` correctly left it because the file is yours.

**`F-CHG-22`'s other half.** `AgentStatus::NeedsInput` maps to `ActivityStatus::Idle` at
`main.rs:3026`. `codex11` is adding the `NeedsInput` variant on the panel side; **that mapping is
where the fifth status currently dies**, and until it changes the new variant can never arrive. This
one is genuinely coupled — coordinate through your reports rather than guessing what the other has
done.

### Leave these

`F-TAB-02` (All-Tabs overflow), `F-TAB-18`/`24` (drag), `F-TAB-25`, `F-TAB-27`, `F-TAB-09` (Open
File). Their own pieces.

## The COSMIC constraint

`sonnet` is building the Pop!_OS COSMIC token layer in `tiller_theme` (a `cosmic/` module exists
now). Take colours, spacing and radii from `tiller_theme::Theme` — **never hardcode a literal.** Name
any token you need that does not exist, exactly as you did for menu width in P60; that list is how
`sonnet` knows what to build.

## Evidence

Follow `docs/linux-rewrite/EVIDENCE-STANDARD.md`. **Named drawn tests** (`TestAppContext` /
`VisualTestContext`, `.debug_selector(id)`, full `run_until_parked()` pump).

For the picker, the test that counts **chooses an adapter and asserts the resulting tab carries it** —
a test that only asserts five menu items render proves the menu, not the feature. For
`TILLER_SOCKET_ENABLE`, set the variable and assert the boot path observes it; that is the entire
defect.

Your P60 note that the ⌘W test was "not verifiable after concurrent drift" is honest and it is also
a debt — re-run it once the tree is green and say whether it holds.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.
- **Yours: `tiller/src/main.rs`, `tiller_control/**`, `tiller_ui/src/tab_bar.rs`.**
- **Do not edit** `tiller_ui/src/settings.rs`, `sidebar.rs`, `chat.rs`, `status_bar.rs` (`pi`),
  `changes.rs`, `right_panel.rs`, `editor.rs`, `file_view.rs`, `tiller_markdown/**`, `tiller_git/**`,
  `tiller_terminal/**` (`codex11`), `tiller_theme/**` (`sonnet`).
- **Never copy code from the reference checkouts.** Mark rows `builder-claimed, unverified`, never
  `PASSED`. **Proceed without asking for approval.**

## Reporting

**12 lines or fewer**: the picker and whether availability gates it, what the fallback does when
nothing is installed, `TILLER_SOCKET_ENABLE` proven live by a test that sets it, the dedupe, what you
did with the `main.rs:3026` mapping and what you still need from `codex11`, whether ⌘W re-verified,
tokens `tiller_theme` still lacks, tests by name, the gate with not-yours failures named separately,
and the honest remainder.
