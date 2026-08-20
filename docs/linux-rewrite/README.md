# The Linux rewrite

This directory is the historical record of porting Tiller from a native macOS/Swift app to
the Rust/gpui Linux app that ships today — inventory specs, adjudication logs, critic
findings, and everything else produced along the way. It is kept in full, including the 622
references to `.swift` paths in its specs, because those pointers are how a row in the
inventory cites the exact source it was ported from.

```bash
# reproduces that figure (the count excludes this README, which mentions the
# extension while describing it — counting itself would make the number drift
# every time this paragraph is edited)
grep -rIo '\.swift' docs/linux-rewrite/ --exclude=README.md | wc -l
```

## The Swift original was removed

The Swift/Xcode project (`App/`, `AppTests/`, `Packages/`, `Tiller.xcodeproj/`) was removed
from this repository on **2026-08-20**, once the Rust/gpui port covered the inventory this
directory tracks.

The last commit containing the full Swift tree is:

```
5430d7bfdb4a295be8ce072526ae5108259b80f8
```

The Swift sources are still retrievable from git history at that commit — they were removed
from the working tree, not from history. To read any Swift file this directory's specs cite,
run:

```bash
git show 5430d7bfdb4a295be8ce072526ae5108259b80f8:Packages/TillerCore/Sources/TillerCore/Workspace/WorkspaceLayoutTransition.swift
```

(substituting whatever `.swift` path a given inventory row points at). That command was run
against this repository before being written down here, and it printed the file's contents —
it is not a guess at what git history should support.

To browse the whole Swift tree as it stood at that commit (rather than one file at a time)
without touching your current working tree, add a separate worktree pinned to that commit:

```bash
git worktree add /tmp/tiller-swift-5430d7bf 5430d7bfdb4a295be8ce072526ae5108259b80f8
```

## Other documents that still describe the macOS build

This directory is not the only place holding macOS-era text. Roughly 65 dated planning and
spec documents under `plans/` and `docs/superpowers/` — written between July and August 2026,
all of them *before* the removal — contain present-tense instructions like `xcodegen generate`
and `xcodebuild test -project Tiller.xcodeproj -scheme Tiller`. Files under
`docs/visual-reviews/` are the same.

**Those were deliberately left as written.** They are dated records of what was planned and
decided at the time, and editing them to describe a build that did not exist yet would
falsify the record rather than correct it — the same reasoning that keeps this directory's
own `.swift` citations intact. None of them is reachable from `README.md`, `CLAUDE.md` or
`AGENTS.md`, so nothing routes a newcomer into them expecting runnable commands.

The rule for reading them: **a command in a dated plan is a historical artefact, not an
instruction.** The current build is `Scripts/ci.sh` (and `Scripts/ci-linux.sh` for the
fuller gate); nothing in this repository builds with Xcode any more.
