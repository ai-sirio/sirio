# Hidden-Pane Rendering — Investigation Spike Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (- [ ]) syntax for tracking.

## Goal

Investigate whether libghostty exposes a documented, tested API to suspend/resume rendering for a `TerminalSurface` without data loss. This is a time-boxed investigation spike — no implementation is committed unless a safe API is proven and the AC7 sentinel gate passes. The fallback is explicitly no code change.

## Architecture

No code changes are made during this spike. The investigation produces a decision record with only two acceptable outcomes:

1. **A proven, documented, tested no-loss API exists** → a safe implementation plan is written for a later change.
2. **No such API exists** → retain current rendering behavior. No rendering-lifecycle change is made.

## Tech Stack

- Xcode 16+, Instruments (GPU template, Metal template)
- libghostty-spm (as currently integrated)
- No new dependencies

## Global Constraints

- **No undocumented or untested internal API.** Using an undocumented or untested internal API is not acceptable.
- **No disconnecting and reconnecting the surface.** The safety and no-loss properties of this approach are unproven.
- **No `alphaValue = 0` workaround.** CA rendering is not the dominant cost, and the surface continues to process input.
- **Process execution, PTY byte capture, scrollback, content signal, and agent detection remain fully active** regardless of the rendering state.
- **B4 cannot ship unless AC7 verification passes.** If verification fails, retain current rendering behavior.

---

## Investigation Protocol

### Step 1: Profile current hidden-pane GPU cost

**Instruments configuration:**
1. Launch Tiller with 10 worktrees, 20 panes (2 per worktree).
2. Open Instruments with the **GPU template** (Metal, Core Animation).
3. Select one worktree (2 panes visible). The other 18 panes are hidden (`opacity(0)`).
4. Record for 30 seconds with no user interaction.
5. Record for 30 seconds with heavy output in 4 hidden panes (`git clone` in each).
6. Switch to a different worktree (making the previously visible panes hidden and vice versa).
7. Record for another 30 seconds.

**Metrics to capture:**
- Metal render pass count per frame
- GPU time per frame (total and per-surface if available)
- Core Animation commit count
- Tiler utilization

**Threshold for action:** If hidden panes account for > 5% of GPU time per frame, further investigation is warranted. If < 5%, the optimization is not worth the risk.

### Step 2: Search libghostty API for suspend/resume

**Search targets:**
- `TerminalSurface` public API (in `GhosttyTerminal` module)
- `InMemoryTerminalSession` public API
- `TerminalView` public API
- libghostty-spm README and documentation
- libghostty source (if available in the SPM checkout)

**API patterns to look for:**
- `suspend()`, `resume()`, `pause()`, `stop()`, `start()`
- `isActive`, `isVisible`, `setNeedsDisplay`
- `freeze()`, `thaw()`
- Any delegate or notification for visibility changes

**Documentation sources:**
- `Packages/libghostty-spm/Sources/GhosttyTerminal/` (if checked in)
- `Packages/libghostty-spm/README.md`
- `Packages/libghostty-spm/Documentation/` (if present)

### Step 3: Evaluate candidate API

For each candidate API found in Step 2, evaluate:

| Criterion | Required |
|-----------|----------|
| Documented in public API | Yes |
| Tested in libghostty test suite | Yes (or evidence of production use) |
| No data loss on resume | Yes (proven by test or documentation) |
| Thread-safe | Yes (or documented threading model) |
| Available on macOS 15+ | Yes |

### Step 4: AC7 sentinel sequence verification

If a candidate API passes Step 3, design and run the AC7 verification:

**Automated sentinel sequence:**
1. Feed a known sentinel sequence to a hidden pane (e.g., `seq 1 1000` with unique markers at first and last line).
2. Use `tillerctl panel.read` or a package-level test harness to assert:
   - First sentinel line is present.
   - Last sentinel line is present.
   - Expected count of lines (1000).
   - Lines are in order.
   - Byte-level scrollback data matches exactly.
3. If UI/libghostty verification must remain integration/manual, pair it with automated raw-byte tests at the `ScrollbackBuffer` level.

**Gate:** B4 cannot ship unless this verification passes. If verification fails, retain current rendering behavior.

### Step 5: Decision record

Write the decision to `docs/superpowers/notes/hidden-pane-rendering-decision-2026-07-17.md` with:

```markdown
# Hidden-Pane Rendering Decision

**Date:** 2026-07-17
**Status:** [implement | no-change]

## Investigation Summary

[Brief description of what was investigated]

## Candidate APIs Found

| API | Source | Documented | Tested | No-Loss Proven |
|-----|--------|------------|--------|----------------|
| ... | ... | Yes/No | Yes/No | Yes/No |

## GPU Profile Results

- Hidden pane GPU time: [X]% of total
- Threshold: 5%
- [Exceeds / Within] threshold

## AC7 Verification

- Sentinel sequence: [pass / fail]
- Byte-level scrollback: [match / mismatch]

## Decision

[Safe implementation plan in a later change | No source change]

## Rationale

[Why this decision was made]
```

---

## Acceptance Criteria

| AC | Verification |
|----|-------------|
| AC7 | No lost bytes — hidden pane shows all accumulated output. If B4 is implemented, AC7 sentinel gate must pass. If B4 is not implemented, AC7 is verified by existing `ScrollbackBuffer` tests. |
| AC12 | `Scripts/ci.sh` prints `CI OK` (no code changes, so this is trivially satisfied) |

## Commit Messages

No code changes are expected from this spike. If a decision record is written:

```bash
git add docs/superpowers/notes/hidden-pane-rendering-decision-2026-07-17.md
git commit -m "docs: record hidden-pane rendering investigation results"
```
