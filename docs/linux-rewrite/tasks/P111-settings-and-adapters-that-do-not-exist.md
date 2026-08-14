# P111 — five absent rows, and two of them may not be rows at all

**Owner: `fable`, as builder.** Worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`,
branch `linux/gpui-waku`.

## The five

You censused this bucket yourself in `P105`, so you already know these are the residue: the rows
where the needle really did come back empty.

| row | what the clause wants | census needle |
|---|---|---|
| `F-SET-12` | enter / save / clear a cookie, and a workspace-ID override control | `grep -niE "cookie\|workspace" settings.rs` → zero cookie hits; whole-tree → only `tiller_usage` read-side commentary and `extract_workspace_id` (opencode_go.rs:43) |
| `F-SET-13` | an Ollama provider card, its bar segment, cookie UI, refresh | `grep -rni "ollama"` → only `UsageProvider::OllamaCloud => LocalAccountState::NoLocalStore` (account.rs:115-117); `ProviderKind` is Claude/Codex/OpenCodeGo (settings.rs:625-629) |
| `F-SET-17` | an agent-registry error state, surface and retry | `discover_availability()` is an infallible `Vec<AgentAvailability>` over the fixed adapter list (tiller_agents/src/lib.rs:215-217, **no `Result`**) |
| `F-AGENT-OPENCODE-03` | the `opencode run --pure` summarizer command | no summarizer-command generator exists in the port; the only summarizer code is the settings `SummarizerChoice` |
| `F-AGENT-OMP-03` | the `omp --print --no-tools` summarizer command | same, **plus** the launch name `omp` doesn't match the distribution's `oh-my-pi` (that second layer is `OMP-01`) |

Clauses and their `VERIFY:` lines are in `docs/linux-rewrite/01-inventory-app.md`, each with a `SRC:`
pointer into Tiller's Swift. The Swift is the functional contract; COSMIC governs how it looks.

## Decide before you build — two of these may be `N/A — platform`

`F-SET-12` and `F-SET-13` are the interesting ones, and I want a judgement, not compliance.

The census found `account.rs:152-163` saying **on this platform there is no credential store** — the
cookie lives in the macOS Keychain. So a "save cookie" control on Linux either needs a store this
port doesn't have, or it is honestly not portable. There are three possible answers and only you can
pick:

1. There is a Linux facility that fits (Secret Service / `libsecret`, or a documented on-disk store
   with the security properties spelled out) — then build it and say what you chose.
2. The row is genuinely `N/A — platform` — then say so **with the specific missing facility named**,
   and write it up so `pireview` can mark it. That is a real outcome, not a dodge.
3. The row is partly portable — the workspace-ID override in `F-SET-12` looks independent of any
   credential store — then build that half and mark the other half explicitly.

**Do not build a cookie field that silently drops the value on quit.** A control that appears to work
and doesn't is the exact failure this project keeps paying for; absent is better.

`F-SET-17` has the same shape from the other end: the registry cannot error *by construction*,
because `discover_availability()` returns no `Result`. Making an error state renderable therefore
means making discovery fallible first. If you judge that discovery genuinely cannot fail on Linux,
say that and mark the row — but check before concluding, because "the fixed adapter list can't fail"
and "probing whether each CLI exists can't fail" are different claims.

## Stay out of these files

`chat.rs` and `composer.rs` are being edited right now by `codex11` (`P107`); `sidebar.rs`, the
terminal and changes surfaces are `codex12`'s (`P110`). Yours are `settings.rs`, `status_bar.rs`,
`tiller_agents/`, `tiller_usage/`. If a row pulls you outside that set, say so before you edit.

## Proving it

Green tests are the floor, never the proof. Every row that renders needs a capture.
`Scripts/wayland-drive.sh <outdir> '<actions>'` takes no lock and runs in parallel — set
`TILLER_WL_LABEL=fable`. Read `WAYLAND-LANE.md` first (five traps). No synthetic input on that lane:
drive over the control socket, or record `owed: gesture — <exact gesture>` for the `DISPLAY=:1`
holder (`sonnet`, on `P109`).

The two adapter rows are different in kind — they are command *generators*, so a unit test over the
generated argv is genuinely the right proof, and the thing to get right is the argv itself. Check
`ShellQuote.swift`'s `jsonStringLiteral` discipline in the Swift original before assuming quoting is
free.

## What to produce

Working code where you build, an argued verdict where you don't, and
`docs/linux-rewrite/P111-report.md` with per row: what you built or why not, the test, the capture,
and **which conjuncts you did not exercise.**

Commit per row, path-scoped, never `git add -A`. `grep '??'` before calling a row done.

## The rules

- **Do not edit `INVENTORY-LEDGER.md`.** `pireview` owns it; your report is the input.
- Do not idle on an approval gate — `ENVIRONMENT.md` §"Working with the orchestrator".
