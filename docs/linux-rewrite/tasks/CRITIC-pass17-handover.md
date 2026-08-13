# CRITIC pass 17 — and the handover of the role

## Why the role moved

At 00:15 on 2026-08-14 both `pi` panes died on the same error:

```
429 {"type":"GoUsageLimitError","message":"Monthly usage limit reached. Resets in 10 days.
To continue using this model now, enable usage from your available balance: ..."}
```

It is account-level, not model-level: `deepseek-v4-flash` (`pi`, builder) and `deepseek-v4-pro`
(`pireview`, critic) both hit it within minutes. There is no `auth.json` under
`~/.local/share/opencode/`, so **no alternative provider is configured** and neither pane can be
switched to another model. The only two exits are a paid-balance opt-in on the user's account or
waiting ten days. **The orchestrator will not spend the user's money while they are asleep**, so
both panes are gone for the night.

`pireview` was the critic, and the project's completion condition is defined in terms of it: *done
only when the full-app critic ticks every inventory entry by exercising it live*. With `pireview`
dead, verification stops permanently unless the role moves.

**The role moves to `fable`.** This is a deviation from the letter of the user's rule ("the critic
runs on pireview") and it is recorded here so it is visible rather than quiet. It honours the
rule's purpose, which is independence: `fable` has never written a line of Rust in this project —
it has only audited documents — so it is disqualified from judging **nothing**. That is a stronger
independence position than `pireview` ever held.

## What the critic is

You start fresh, without the builder's reasoning, and you find out for yourself.

1. **You compile, launch and drive the app yourself.** Not the builder's screenshot, not the
   builder's test output, not the builder's word.
2. **You exercise the function.** Connecting a real ACP agent, sending a message, watching it
   stream — that is what proves a chat row. Reading the code that would stream is not.
3. **A feature you have not successfully tried does not exist.** This is the whole standard. A row
   whose builder swears it works and which you could not make work is not `PASSED`.
4. **Only you change a verdict.** No builder may promote its own row, and neither may the
   orchestrator.

## The verdict vocabulary

`PASSED` · `half-proven` · `FAILED — absent` · `FAILED — defective` · `UNREACHABLE` ·
`N/A — platform` · `NOT EXERCISED`

`builder-claimed, unverified` is **not** `PASSED`. It is the bucket that exists precisely so that a
claim can be recorded without being believed.

Ledger row format, exactly:

```
| `F-XXX-NN` | VERDICT | evidence | source |
```

IDs stay backticked. Some end in a letter (`F-CORE-FILE-03A`).

## Two false-verdict shapes already proven in this project

Both were found tonight, and both would have passed a careless check:

- **The dead control.** Built, tested, and reached by nothing — a call that was never connected
  (`F-PRJ-04`, `F-SET-09/16/22`). Repeatedly *a disconnected call, not a missing surface*.
- **The unreachable chord.** `chat.rs` bound `cmd-a`/`cmd-c`; GPUI's platform modifier is ⌘ on
  macOS but **Super on Linux**, so a Linux user pressing Ctrl+C got nothing — while a harness
  synthesising the *bound* chord would have marked it `PASSED`. Drive the input a real user on this
  platform would produce, not the input the source declares.

## Pass 17 — what to do, in order

1. **The 33 rows marked built-but-unproven.** `pireview` was part-way into these when it died. They
   are the cheapest ticks on the board: the code is claimed to exist, so each one is a drive-and-look
   rather than an investigation. Take them first.
2. **Finish the `rclick` verification `pireview` started.** See below.
3. Then the remaining `NOT EXERCISED` and `half-proven` rows.

## The harness, and one unfinished check

`Scripts/linux-drive.sh` launches the app, drives it, and photographs the result. Read its header.
Helpers inside your actions snippet: `key`, `type`, `click`, `rclick`, `shot`.

`rclick` (button 3) **was added by the orchestrator tonight and has never been confirmed to work.**
Its history matters: right-click "not reaching the app" was diagnosed as an XWayland/XTEST
restriction and written into `ENVIRONMENT.md` as established fact. That was wrong — the script
simply had no button-3 path at all. The helper now exists, mirroring `click` verbatim.

**Verify it before trusting any verdict that rests on it.** A helper the orchestrator wrote is not
evidence. `pireview` was doing this correctly when it died: it was reading gpui's X11 client
(`crates/gpui_linux/src/linux/x11/client.rs`, `Event::XinputButtonPress` at :1138) to establish
whether XTEST button 3 actually arrives as an XI2 event GPUI dispatches. Finish that, then drive a
real right-click. The app binds 22 `MouseButton::Right` handlers across `main.rs`, `right_panel.rs`,
`sidebar.rs` and `tiller_terminal/lib.rs`, so if a verified right-click still does nothing, **that
is a finding, not a restriction.**

Also read `docs/linux-rewrite/ENVIRONMENT.md` before your first build — it is the measured-facts
file (cargo is not on every PATH, `pkill -x tiller` never `pkill -f`, no headless critic because
blade needs DRI3, the shared cargo target).

## A ruling retracted — do not re-apply it

`F-SID-16`, `F-SID-17` and `F-TAB-18` stay **`FAILED — absent`**. That is correct.

The orchestrator had queued a change to "no referent / N/A" after grepping `SidebarView.swift` for
`onDrag|onDrop|ReorderScope` and finding nothing. The referent exists and is complete:
`App/RowReorder.swift` (UTType `it.tiller.row-drag`, `onDrag`/`onDrop`, `enum ReorderScope`) wired
at `SidebarView.swift:50` (projects), `:55` (worktrees), `:600` and `TabBarView.swift:221` (tabs).
The grep could not match by construction, because the call sites use a project-local extension
method, `.reorderable(model:id:scope:)` — the orthodox vocabulary sits one indirection away.

The general rule, which applies to you every time you are about to write `absent`: **a search that
finds nothing is a fact about the query, not about the code.** Search from the referent's side —
start at the file that would implement it and ask what cites it.

One correction to make when you next touch those rows: their evidence column reads *"drag reorder
removed by design"*. That is false. Nothing was removed and no such decision was taken; it simply
has not been built. An evidence string that asserts a decision nobody made is worse than an empty
one, because it closes the work instead of flagging it.

## The denominator

It stays **389** for continuity, and the ledger's headline stays `N/389`.

`FABLE-12` measured that the inventory never saw the ACP subsystem at all and proposed +14 rows for
it (plus 17 more, disputable). Those 14 are real and the orchestrator accepts the finding — but the
user pinned 389 and is asleep, so the number is **not** moved behind their back. Record the ACP rows
in a clearly marked appendix and report progress as "N/389, plus 14 newly-found ACP rows not yet in
the denominator". Both numbers visible, the decision left to the user.
