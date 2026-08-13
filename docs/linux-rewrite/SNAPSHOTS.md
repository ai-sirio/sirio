# Snapshots — why they exist and where they are

`~/tiller-linux-snapshots/tiller-linux-src-YYYY-MM-DD-HHMM.tar.gz`

Source only: `target/`, `.git`, `node_modules` and `.build` excluded. ~12 MB, ~1720 entries.

## Why this is not paranoia

`tiller-linux` is a **git worktree**, so its `.git` is a 4 KB pointer file — the real object database
lives in `/home/enzopalmisano/Scrivania/Progetti/tiller/.git` (18 MB) and is **shared with the main
repo**. One object-database loss therefore takes out the history of both trees at once.

That has already happened on this machine. `git log` in the main repo still carries the scar:

```
c63378e chore: recover Rust/GPUI rewrite after local git object database loss
```

Six agents write to this tree in parallel and the commit cadence is measured in hours, so the
uncommitted delta is routinely tens of thousands of lines. On 2026-08-13 20:29 it was **346 files,
56,449 insertions, against a commit 7 hours old**.

A snapshot outside git defends against exactly that failure, and — unlike a commit — needs nobody's
approval and cannot disturb an agent mid-write.

## Taking one

```sh
S=~/tiller-linux-snapshots; mkdir -p "$S"
tar czf "$S/tiller-linux-src-$(date +%Y-%m-%d-%H%M).tar.gz" \
  --exclude=target --exclude=.git --exclude=node_modules --exclude=.build \
  -C ~/Scrivania/Progetti tiller-linux
```

**Verify it before trusting it.** An archive nobody has listed is a belief, not a backup:

```sh
tar tzf <archive> | wc -l                      # expect ~1700+
tar tzf <archive> | grep -c '\.rs$'            # expect ~140+
```

Not `/tmp` — a reboot is part of the disaster this defends against.
