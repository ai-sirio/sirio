# P61 — The editor save path: the model is already built, nothing calls it

**You are codex11, pane `w1:p2`.** Your context was just reset, so this brief is everything you need.

## P59 landed, and the structural half is the part that matters

You un-quarantined all three tests and `tiller_terminal` is **19/19 green**. The seam you chose —
polling a scheduler-owned `try_recv` instead of waking GPUI from the PTY thread — is the right shape:
it removes the cross-thread wake entirely rather than trying to make the deterministic scheduler
tolerate one. `codex12` had to give up that coverage in P55; you gave it back.

And `TILLER_PANE_ID` now reaches the child PTY, so Layer A can actually fire for a real agent pane
for the first time.

Two corrections to your report, neither of them your fault:

- **The `tiller` bin compiles.** It was genuinely broken when you looked — `pi` was mid-edit on
  `SettingsSnapshot` — and the fields have since landed. `cargo build -p tiller` is green, two
  warnings. Your observation was true when made and is no longer true; that is what a moving tree
  does, not a mistake.
- You declined `F-TERM-SCR-02` and `F-TERM-PTY-06` as "non convenienti". Accepted — but say *why* in
  one line next time. "Not worth it" that a later pass cannot evaluate turns into a row nobody
  revisits.

## The piece: `F-EDIT`, and most of it is wiring, not building

Eleven `F-EDIT` rows are FAILED — absent. The ledger says the editor cannot save, cannot reload, and
has no conflict banner. A mechanical sweep says something more specific.

`Scripts/dead-models.py` enumerates every `pub fn` with no caller outside its own definition and
tests. In `tiller_markdown/src/document.rs` it finds **four, and they are exactly the missing
feature**:

```
set_text          out=0 in=0 test=2   tiller_markdown/src/document.rs:85
refresh_from_disk out=0 in=0 test=3   tiller_markdown/src/document.rs:92
has_conflict      out=0 in=0 test=1   tiller_markdown/src/document.rs:77
is_deleted        out=0 in=0 test=1   tiller_markdown/src/document.rs:81
```

Mutate, reload from disk, detect a conflict, detect deletion — built, unit-tested, and called by
nothing. `F-EDIT-06` ("no save path"), `F-EDIT-05` ("no Reload/Keep banner") and `F-EDIT-04` ("no ⌘S
binding") are very likely not *absent* at all. They are unwired.

**Verify that before you rely on it.** The sweep is a triage list, not a verdict, and it has already
produced two convincing near-misses: `save_projects` looked like proof that nothing persists projects
until you notice the app calls the singular `save_project` everywhere, and `tiller_git/actions.rs`
has three parallel APIs where eight functions are dead and staging works fine. A zero-caller count on
a **name** is not a zero-caller count on a **feature**. Check whether some other path already saves
before you conclude none does.

### Take this cluster

- **`F-EDIT-06` save, `F-EDIT-04` ⌘S, `F-EDIT-05` the Reload/Keep banner.** One coherent story:
  buffer → disk, disk → buffer, and what happens when both changed. The `has_conflict` /
  `is_deleted` predicates already exist to answer the third.
- **`F-EDIT-08`** — `add_file_tab` pushes unconditionally, so opening the same file twice makes two
  tabs. Cheap, and it will annoy the critic within thirty seconds of live use if it is still there.

### The judgement call, and it is yours

**What happens when the file changed on disk and in the buffer?** Reload and lose the edit, keep and
risk staleness, or refuse to save and force a choice. The ledger calls for a "Reload/Keep banner",
which implies the third — but decide it deliberately and say what you chose, because a save path
that silently picks one is how people lose work. Then make the banner and the save path read the
**same** predicate; two paths that can disagree about whether a file is in conflict is how "it
warned me and then overwrote it anyway" gets built.

### Leave these

`F-EDIT-01`/`03`/`07` (Code/Preview switch, language detection), `F-EDIT-10`/`11`/`12` (file-row
context menu, copy path, drag), `F-EDIT-02` (formatting toolbar). Their own pieces. Note that
`is_plain_text` and `format_markdown` are also dead — likely the same story for the preview cluster,
which makes it a good next piece but not this one.

One more for your report, not for fixing: **`tiller_ui/src/editor.rs:375 from_buffer` has out 0,
in 0, `test=12`.** Twelve tests exercising a constructor no production code calls. If your wiring
ends up using it, say so — that single line would turn twelve dead tests into live ones.

## One constraint that is new tonight

The UI is moving to the **Pop!_OS COSMIC** design language; `sonnet` is building the token layer in
`tiller_theme` now. Take colours, spacing and radii from `tiller_theme::Theme` — **never hardcode a
literal.** If the token you need does not exist yet, use the nearest one and name the gap.

## Evidence

Follow `docs/linux-rewrite/EVIDENCE-STANDARD.md`: on this ledger, rows judged by reading ran ~21%
false, rows judged by executing ~0%.

A save path is provable the honest way — **write a real file, edit it, save, read the bytes back off
disk.** Do that. An assertion that a buffer's dirty flag flipped proves nothing about whether
anything reached the filesystem, and that is precisely the gap between what these four functions do
and what the rows claim.

For the banner and the ⌘S binding, **named drawn tests** (`TestAppContext` / `VisualTestContext`,
`.debug_selector(id)`, full `run_until_parked()` pump). A banner that renders but whose Reload button
is a no-op closes nothing — test the action, not the presence.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.
- **Yours: `tiller_markdown/**` and `tiller_ui/src/editor.rs` and `file_view.rs`** — assigned
  explicitly so nobody else takes them.
- **Do not edit** `tiller_ui/src/settings.rs`, `sidebar.rs`, `chat.rs`, `status_bar.rs` (`pi`),
  `tiller_ui/src/tab_bar.rs` and `tiller/src/main.rs` and `tiller_control/**` (`codex12`),
  `tiller_theme/**` (`sonnet`). If ⌘S must be registered in `main.rs`, **say so in your report** and
  let `codex12` add it.
- The gate is `Scripts/ci-linux.sh`. It may be red for reasons that are not yours — name them
  separately rather than absorbing them.
- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
- Mark rows `builder-claimed, unverified`, never `PASSED`. **Proceed without asking for approval.**

## Reporting

**12 lines or fewer**: whether the four dead functions really were the missing feature or whether
something else already saves, your conflict decision and the single predicate both paths share, the
round-trip test that reads bytes back off disk, whether ⌘S needed `main.rs`, the dedupe, whether
`from_buffer` came alive, tests by name, the gate with not-yours failures named separately, and the
honest remainder.
