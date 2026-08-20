# The Linux rewrite

This directory is the historical record of porting Tiller from a native macOS/Swift app to
the Rust/gpui Linux app that ships today — inventory specs, adjudication logs, critic
findings, and everything else produced along the way. It is kept in full, including its
~4,376 references to `.swift` paths, because those pointers are how a row in the inventory
cites the exact source it was ported from.

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
