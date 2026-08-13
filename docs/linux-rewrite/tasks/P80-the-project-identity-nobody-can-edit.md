# P80 — the project identity nobody can edit

**Owner: `sonnet`.** Six rows, and one of them is already diagnosed.

Read `../SEAMS.md`, `../ENVIRONMENT.md` and `../OWNERSHIP.md` first. **Check the map yourself rather
than trusting a brief's summary of it** — that is not a formality here, see the first section.

## Start with the row you already solved

`F-SET-11` — *see Claude/Codex usage provider Reading, Active, Stale, Not found, Logged out, Timed
out, and Error states.*

You reported this as unreachable: *"root cause found (`status_bar.rs::segment_text`,
`Unavailable(_) => "—"` collapses all reasons) but that file is pi's forbidden file."*

**`status_bar.rs` is yours.** It moved to you at 00:20 on 2026-08-14 when `pi` died, along with
`chat.rs`. `OWNERSHIP.md` has said so since; the brief you were working from was written before that
and went stale, which is exactly why that file says *briefs cite the map, they do not restate it*.

So the diagnosis is right and the fix is inside your boundary. `status_bar.rs:235`:

```rust
ProviderUsageState::Unavailable(_) => format!("{display_name} —"),
```

The row wants **seven distinguishable states** and the wildcard discards the one piece of data that
tells them apart. Nothing else about this row is hard.

## The cluster: five rows, one missing capability

| row | the ledger's words |
|---|---|
| `F-PRJ-13` | no icon colour/reset controls — the settings sheet renders no icon UI at all |
| `F-PRJ-14` | no avatar/GitHub/PNG/favicon controls exist |
| `F-PRJ-15` | no SF-Symbol icon grid (the sfsymbol renderer exists for in-app glyphs; there is no icon picker) |
| `F-PRJ-16` | no emoji picker, no project-icon emoji input |
| `F-PER-07` | project icon/name editing does not exist (`F-PRJ-12..16` all absent), **so there is nothing to persist** |

`F-PER-07` is the reason to take these together: it is not a separate feature, it is these four
becoming persistable. Build the identity and the persistence row comes with it.

**`icons.rs`, `sfsymbol.rs` and `tiller_theme/**` are all yours.** The renderer already exists — the
ledger is explicit that `sfsymbol` works for in-app glyphs and only the *picker* is missing. Read it
before designing anything.

### On `F-PRJ-15` and the platform question

Do not repeat `F-SET-21` here without checking. You ruled that one `N/A — platform` because SF
Symbols is macOS-only — correct for a *system* symbol domain. But `F-PRJ-15` asks for an icon grid,
and this port already renders its own glyph set through `sfsymbol.rs`. **A grid over the glyphs we
actually ship is a real, buildable row**; a grid over Apple's proprietary catalogue is not. Decide
which the inventory means, say which you decided, and build the buildable one if it is the former.
Getting this wrong in either direction is expensive: a wrong `N/A` hides a missing feature, and a
wrong build wastes a night on something the platform cannot show.

### `F-PRJ-14` — read it carefully before building

Avatar, GitHub, PNG, favicon. Four sources for one image. **Fetching a GitHub avatar or a favicon is
network I/O**, which is not a `tiller_ui` concern and is not your crate. If that is what the row
means, build the picker and the local-PNG path, and **register the network half as a seam** rather
than putting an HTTP client in a UI file. Say plainly which parts you built and which you seamed.

## Where the seams are

Two, and both are real:

- **The project settings sheet lives in `sidebar.rs` (`render_project_settings`) — `codex12`'s
  file.** It is currently read-only by design: name, path, repository type, Close, id. Your pickers
  need to be mounted into it. **Do not edit `sidebar.rs`.** Build the pickers as standalone
  components in your own files with typed events, and register Half B in `SEAMS.md` naming exactly
  what `codex12` must add.
- **Persistence is `tiller_persistence` — `codex11`'s crate.** `F-PER-07` needs a settings write
  door. Consuming it is fine; widening its schema is not. If a migration is needed, that is a seam,
  named in `SEAMS.md`, not a reach-across.

`SEAMS.md` is new tonight and it exists because this project kept building Half A and never
dispatching Half B — thirteen ledger rows currently read `FAILED — absent` for code that exists,
including a 1283-line browser no user can open. **An unregistered seam is how your night's work
becomes dead code.**

## Done means

1. `F-SET-11`'s seven states distinguishable, with a test per state.
2. The pickers exist as components with their own tests, and `F-PRJ-15`'s platform question answered
   explicitly either way.
3. Both seams registered in `SEAMS.md` with the change named in one line each.
4. **Commit by explicit path.** Three other agents have uncommitted work in this worktree right now.
   `git add -A` would sweep up `codex11`'s half-written `project_forms.rs` and `codex12`'s in-flight
   `main.rs`. Name your files.
5. `Scripts/transplant-check.py` run and its result stated. An emoji picker and an icon grid are
   exactly the surfaces where a reference is tempting; waku and orca both have them.

**A picker the critic cannot open passes no row.** That is the failure mode this brief is shaped
around, and it has already happened thirteen times.
