# Critic pass 4 — judge the six pieces built since your last pass, headless

**You are pireview, pane `w1:p6`. You are the critic.** Your context was just reset, deliberately:
you judge this build without having seen any builder's reasoning. You built none of it.

> **A feature you have not successfully exercised does not exist.**

## Pass 3 produced the two best findings of the day

**You settled the freeze question with the right measurement.** A headless probe: ping ponged, state
changes applied, **RSS flat at 15:55 elapsed — past the 4–13 minute window** — and your 10:18 boot
frame was one colour *from frame one*. Process and state healthy, failure confined to presentation.
That is independent corroboration from someone who did not form the hypothesis, which is worth far
more than the hypothesis was.

**And you found a structural absence nobody had noticed: there is no editor.** `FileView` is a
read-only viewer — no editing, no save, no dirty state — so 8 `F-EDIT` entries fail by absence
rather than by defect. That distinction matters and you drew it correctly.

You also explained something that had been dismissed as correct git behaviour: projects appear
duplicated because **restore re-discovers git worktrees under every project row, and a linked
worktree's root is itself a repo**, while `write_catalog` writes two id conventions into one table.

Both have been routed to the agents who own those files.

## Where and how

Live tree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux` (branch `linux/gpui-waku`).

**Snapshot it and record the snapshot time at the top of your section** — three builders edit that
tree continuously, and your pass-2 report filed two findings that were already fixed because the
tree moved under them. Before filing a finding, check whether the file it names has changed since
you copied it.

**Everything is headless. There is still no display.**

```bash
cp -a /home/enzopalmisano/Scrivania/Progetti/tiller-linux /tmp/critic-pass4
source ~/.cargo/env && cd /tmp/critic-pass4/rust
cargo build -p tiller -p tiller_control
export TILLER_SOCKET=/tmp/critic4.sock
env -u DISPLAY -u WAYLAND_DISPLAY ./target/debug/tiller &
./target/debug/tillerctl current-workspace
```

**Never call an X tool.** `xdpyinfo`, `import`, `xwininfo`, `xprop`, `xdotool` all block forever on
this machine's wedged server — that cost you 1h26m inside a single `import` call. If you must,
`timeout 10` it and treat the timeout as a hard stop.

## The piece: judge what has been built since you last looked

Six pieces landed. **Every one is a builder's claim about its own work, and none has been checked by
anyone else.** That is precisely what you are for. Take them in this order — the first two carry the
most inventory:

1. **The Changes surface is now mounted** as a first-class Diff tab, opened via `+ → Changes`,
   following the selected worktree and persisting across restart. Claimed: 42 tests including a
   shell-level mount guard. Its builder could **not** produce a screenshot or exercise stage/discard
   live. Verify what you can headless: is it reachable, does it bind to the right worktree, does its
   content match `git status --porcelain` on a repository you build yourself with one staged, one
   modified and one untracked file at once?
2. **New socket methods.** `pane.split`, `pane.focus`, `pane.close`, `tab.cycle`, `tab.select`;
   `panel.state` and `panel.scrollback`; and — being wired now — methods to open and inspect Changes
   and Settings. Check `system.capabilities` and then check the converse, which is the more
   revealing direction: **does it advertise anything that does not work, and does anything work that
   it fails to advertise?**
3. **Persistence fixes.** F-PER-03: three split panes should now survive a relaunch (`pane-0`,
   `pane-1`, `pane-2`) — it used to lose one silently. F-PER-01: scrollback should now persist,
   bounded at 256 KiB. **Use a nonce**; nothing weaker distinguishes a restored terminal from a
   fresh empty one.
4. **Terminal state.** `TerminalStateSnapshot` plus scrollback capture/replay. Its builder claims 7
   of 8 previously display-blocked `F-TERM` entries are now state-backed and exactly 1 is genuinely
   pixel-only. **Test that claim** — an entry about a user *seeing* something is at best half-proven
   by a test that the value exists, and if any of those 7 are really pixel entries in disguise, say
   so.
5. **Settings truthfulness.** Provider badges wired to `tiller_agents::discover_availability()`, a
   Control socket row with a resolved path, Permissions gated off non-macOS. On this machine
   `claude`, `codex` and `pi` are installed and **`opencode` and `omp` are not**, so two of five
   must report unavailable. Read them back through the socket if a method exists; otherwise say it
   is unverifiable headless.
6. **`Scripts/ci-linux.sh`.** Run it. Does `CI OK` mean what it claims? It is supposed to separate
   pre-existing drift from new, and to state that it covers nothing visual. A gate that overstates
   itself is worse than none.

Then, if budget remains, continue through `docs/linux-rewrite/02-inventory-packages.md` — **171
entries** of domain-package behaviour, barely touched and headless by nature.

## Verdicts, used precisely

**PASSED** (exercised, with evidence) · **FAILED** (exercised, did not work) · **UNREACHABLE** (a
stated external reason — `opencode`/`omp` absent) · **N/A — platform** · **NOT EXERCISED — blocked
on display**, never approximated and never ticked from reading the source.

And the rule that matters most this pass: **a claim by a builder is a hypothesis.** Several of
today's findings were retired by looking, and two of yours were retired because the tree moved. Both
directions cost the same to check and both are honest outcomes.

## Method

- **Check the whole set before doubting the witness.**
- **Failing to reproduce is not evidence of absence.**
- **Append, never rewrite**: add `## PASS 4 — <area> — <time>` to
  `<live worktree>/docs/linux-rewrite/CRITIC-baseline.md`. Your earlier passes are above it, backed
  up, and must not be touched.
- **You do not fix anything.** No edits to any `rust/**` source.

## Reporting

Reply in **12 lines or fewer**: for each of the six, does it do what its builder claimed — and the
four counts, plus **the single biggest gap** that is not the display. The display is known, it needs
the operator, and naming it again costs a pass. Name the biggest thing *we* can still fix.
