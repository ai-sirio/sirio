# Critic verdicts — W08-agent (F-AGENT)

Adjudicated by a critic that neither drove nor built this slice. Driver return:
`docs/linux-rewrite/wave-a/W08-agent-evidence.md`, captures under
`reference/linux-progress/wavea-W08-agent/`. HEAD under test: `4073297` (branch HEAD has moved
to `c47bdd9` via sibling docs-only commits since; `git diff --stat 4073297 HEAD -- rust/` is
empty, so the binary under test is unchanged).

## F-AGENT-API-01 (ledger line 458) — verdict: `half-proven` (unchanged, narrower owed half)

Opened all 11 captures. The host-process evidence is real and strong: Codex's `-c
notify=[...]` argv, Pi as a live child, and OpenCode's `panel.list` entry
(`{"agent":"opencode","id":"pane-3","tab":"OpenCode"}`) are all independently checkable facts,
not paraphrases of an app-internal instrument — `04-after-select.png`/`04-after-codex.png` show
a real "Codex" tab and pane created from the palette, and `02-final-panes.png`/`01-baseline.png`
show OpenCode and Pi tabs added to the sidebar in sequence. Confirmed the `UnknownPane` error
text the driver quotes is a real code path (`rust/crates/tiller_control/src/panel.rs:64`), not
fabricated.

One evidentiary defect, not verdict-changing: the row's `captures` list cites
`03-palette-codex.png` as the "typed Codex" moment, but that file is byte-for-byte the same
*unfiltered* palette state as `02-after-palette.png` — no "Codex" text is visible in it. The
directory holds the correct pair (`03-palette-typed.png`, showing "Codex" filtered to "Codex" /
"Codex Here" entries, and `04-after-select.png`, the resulting tab) but neither was cited in the
row's `captures` array. The underlying claim survives on the host-pstree evidence regardless, but
the citation itself is sloppy and should not be trusted at face value in a future pass.

Still not `PASSED`: the ledger's owed half was "per-adapter ID/hook-flag/launch/resume for
Codex/OpenCode/Pi/Oh-My-Pi not individually driven." This drive closed **launch** (with
independent argv/agent-id evidence) for 3 of 4, but:
- Oh-My-Pi remains completely unexercised (same upstream block as OMP-01, confirmed below).
- **Resume** was not driven live for any of the 4 adapters this pass (triage's own approach
  scoped resume out as "already covered by unit tests," which this critic accepts for this row,
  but it means the ledger's named half is still not closed for that clause).
- A new, unresolved anomaly surfaced and was correctly *not* chased down by the driver but is
  worth recording for whoever owns OpenCode next: the OpenCode pane's process was gone from
  `PaneRegistry` (`panel.read` → `unknown pane: pane-3`) shortly after a `panel.list` had just
  shown it alive — a possible launch-stability defect, not adjudicated here since it is outside
  this row's clause.

## F-AGENT-OMP-01 (ledger line 469) — verdict: `NOT EXERCISED` (unchanged)

Independently reran the exact check on this host, not just accepted the driver's prose:
`oh-my-pi --version` and (separately, for OMP-03 below) `oh-my-pi --print --no-tools
"summarize this"` both throw the identical `SyntaxError: Unexpected token ':'` at
`bin/oh-my-pi.js:176` before any argv is read. Same failure, same line, reproduced independently.
Genuinely instrument-blocked upstream of Tiller, not a Tiller defect. Verdict carries forward
unchanged.

## F-AGENT-OMP-02 (ledger line 470) — verdict: `NOT EXERCISED` (unchanged)

Same shared cause as OMP-01, reconfirmed by the same independent rerun above: no oh-my-pi session
can start, so no hook event this row is about can ever fire. Verdict carries forward unchanged.

## F-AGENT-OMP-03 (ledger line 471) — verdict: `half-proven` (changed from `FAILED — absent`)

Triage's "reclassify" and the driver's `partially-exercised` are both correct in substance.
Independently confirmed both facts the driver cites: `omp_summarizer_command_uses_the_
distribution_binary_name` passes (`cargo test -p tiller_agents --lib summarizer`, reran it here,
`0.26s`, no recompile — cache hit), and `oh-my-pi --print --no-tools "summarize this"` — the
*exact* command the code under test generates — was run live by this critic independently of the
driver's own check, producing the identical upstream `SyntaxError` at the identical line. So:
half A (the command-string generator exists, matches the project's executable-name discipline,
and is unit-tested) is genuinely proven, correcting the stale "no summarizer-command generator
exists anywhere" premise; half B (generate-and-run-and-inspect-stdout, the row's literal VERIFY
text) is genuinely blocked, confirmed twice now by two independent testers hitting the same crash
before any argv is parsed. `main.rs` having zero callers of any adapter's `summarizer_command` is
real (confirmed by grep) but is **not** this row's clause — `F-SET-05` (already `PASSED`) already
carries "auto-rename execution itself still has no consumer... out of this row's scope" as its
own, separately tracked gap. `half-proven` is the correct verdict; the owed half is squarely "run
it and read stdout," blocked until oh-my-pi's own TS-transpile bug is fixed upstream.

## F-AGENT-OPENCODE-03 (ledger line 466) — verdict: `PASSED` (changed from `FAILED — absent`, upgraded past the driver's `partially-exercised`)

**Disagreement with the driver — an underclaim, not an overclaim.** The driver stopped at
`cargo test` + grep and concluded `partially-exercised` because "no live spawn was observed" and
"there is no code path that calls it yet" in `main.rs`. That reasoning imports a different row's
scope: the row's clause (`02-inventory-packages.md:142`) is narrowly "OpenCode's noninteractive
summarizer command is `opencode run --pure '<prompt>'`," and its VERIFY step is explicitly
"Generate and run the summarizer command with a known prompt and inspect stdout" — it says
nothing about an automatic in-app trigger. `main.rs`'s missing auto-naming caller is
`F-SET-05`'s already-tracked, separately-scoped gap ("Auto-rename execution itself still has no
consumer... out of this row's scope"), not this row's.

This critic generated and ran the exact command independently, live, on this host (`opencode`
1.18.18, confirmed on `PATH`):

```
$ opencode run --pure "summarize this"
> build · big-pickle
What would you like me to summarize? I don't have any file or content attached — if you meant
something in the repo, point me at it.
```

Exit 0, real stdout, a genuine non-interactive opencode response — not a screenshot, not a
paraphrase, an independently-typed shell command against the installed binary, matching
character-for-character the string `opencode_summarizer_command_is_run_pure_with_a_quoted_
prompt` (`rust/crates/tiller_agents/src/lib.rs:451`, reran it here, passes) asserts Tiller
generates. Both halves of the row's own VERIFY — "the command is `opencode run --pure
'<prompt>'`" and "running it produces inspectable stdout" — are now proven live. `PASSED`.

## Notes for the orchestrator

- **F-AGENT-OPENCODE-03 is the one substantive disagreement in this slice**, and it goes the
  unusual direction: the driver undersold its own finding by gating the verdict on a different
  row's (`F-SET-05`'s) already-separately-tracked gap. Upgraded to `PASSED` on this critic's own
  independent live run of the exact generated command.
- Out-of-scope observation worth flagging for a future dispatch, not adjudicated here: the ledger
  still carries `F-AGENT-OPENCODE-01` and `F-AGENT-OPENCODE-02` as `UNREACHABLE — opencode not
  installed` (pass 9, "command -v empty"). `opencode` is now on `PATH` at version 1.18.18 (same
  binary this slice and W08-agent's own driver both used) — that premise looks stale and those
  two rows may now be drivable.
- F-AGENT-API-01's `captures` array cites a screenshot (`03-palette-codex.png`) that does not
  show what it is cited for (see above) — not verdict-changing here because the pstree evidence
  stands on its own, but the correct pair of screenshots (`03-palette-typed.png`,
  `04-after-select.png`) already exists in the same directory and should be cited instead next
  time this row is touched.
- No rows in this slice were left unreached; all 5 were judged.
