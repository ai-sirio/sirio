# P77 — the clone backend nobody can reach

**Owner: `codex11`.** Six rows, one new file, one named seam.

Read `../ENVIRONMENT.md` before the first build command and `../OWNERSHIP.md` before the first
edit. This brief does not restate either.

## The rows

| row | the ledger's words today |
|---|---|
| `F-PRJ-05` | no clone-from-URL form exists in the Linux build (no clone UI, no clone socket method) — nothing to exercise |
| `F-PRJ-06` | no clone form exists — empty-URL disablement and double-start guards have no surface to live on |
| `F-PRJ-07` | no clone form exists — no failure/retry surface |
| `F-PRJ-08` | no create-new-project form exists — the Add flow only browses folders |
| `F-PRJ-09` | no create form exists — empty-name/duplicate-submission guards have no surface |
| `F-PRJ-10` | no create form exists — no failure surface |

All six say the same thing in six ways: **the surface is missing.** None of them says the logic is
missing, and that distinction is the whole brief.

## Start by reading what is already there

`rust/crates/tiller_git/src/clone.rs` is 62 lines and already contains:

```rust
pub struct GitClone;
impl GitClone {
    pub fn clone<F>(url: &str, destination: &Path, mut on_progress: F) -> Result<(), GitError>
    pub fn parse_progress(line: &str) -> Option<f64>
}
pub fn clone_repository<F>(url: &str, destination: &Path, on_progress: F) -> Result<(), GitError>
```

Clone with a progress callback and a progress parser. **It has exactly one reference in the entire
tree — its own `pub use` in `tiller_git/src/lib.rs:54`. Zero callers. Zero tests.**

So do not write it again. Two things are true of it and both matter:

- **It is unreachable**, which is why three rows read `absent` while the capability exists.
- **It is unproven**, which is worse, because nothing has ever run it. Before you build a form on
  top of it, give it tests — at minimum `parse_progress` against real `git clone --progress` stderr
  (it writes `Receiving objects:  47% (…)` to **stderr**, not stdout) and one clone of a local
  `file://` repo you create in a tempdir. If it turns out broken, that is a finding worth more than
  the form, and it is yours to fix — `tiller_git/**` is your crate.

## This is the sixth time, and that is the point

`F-PRJ-04`, `F-SET-09`, `F-SET-16`, `F-SET-22`, `split_disabled_reason` behind `F-TAB-11`, and now
`clone_repository`. Every one is the same shape: **logic built, sometimes tested, reached by
nothing.** It is the defect this project produces most, because a builder told to "implement clone"
writes a function, watches it compile, and reports done — and *compiles* and *is reachable* are
independent properties with only one of them ever checked.

**A row is written from the user's side on purpose.** `F-PRJ-05` cannot be satisfied by a function;
it is satisfied when somebody typing a URL into this app ends up with a cloned repository. Judge your
own work by that sentence, not by the test suite.

## What to build

### A new file you own: `rust/crates/tiller_ui/src/project_forms.rs`

New file, yours, registered in `tiller_ui/src/lib.rs`. It holds two GPUI surfaces.

**`CloneForm`** — a URL field, a destination, a submit control, a progress indicator, an error area.

- `F-PRJ-06` is two guards and both are testable without a display: **submit is disabled while the
  URL field is empty**, and **a second submit while a clone is in flight does nothing**. Model the
  in-flight state explicitly (an enum, not a `bool` pair) so the second guard is a match arm rather
  than a race.
- `F-PRJ-07` needs the failure to be **visible and recoverable**: when `clone_repository` returns
  `Err`, the form shows what went wrong and offers retry without retyping the URL. A form that
  clears itself on failure fails this row.
- Drive the progress area from `parse_progress`. Do not invent a second parser.

**`CreateForm`** — a name field, a parent location, submit.

- `F-PRJ-09` mirrors `F-PRJ-06`: empty name disables submit, double submission is guarded.
- `F-PRJ-10` mirrors `F-PRJ-07`: the failure is shown, not swallowed. Creating a directory that
  already exists is the easy case to test and the one a user actually hits.
- The create path belongs in `tiller_project/**`, also yours. If it does not exist, write it there
  and call it from the form — do not put filesystem logic in the UI file.

### The seam, named

The `+` control lives in `sidebar.rs`, which is **`codex12`'s file. Do not edit it.**
`start_add_project` at `sidebar.rs:795` currently opens the folder picker directly and emits
`SidebarEvent::AddProject(PathBuf)`.

Your half ends at a **mountable surface with a typed result**. Publish from `project_forms.rs`:

- a constructor for each form that takes no sidebar types,
- an event each form emits carrying the finished path (`Cloned(PathBuf)` / `Created(PathBuf)`),
- and say in your handoff, in one line each, exactly what `codex12` must add.

`codex12` owns Half B: turning `+` into three choices and mounting these two. `F-PRJ-01` is its row,
not yours. **Write the handoff as if the person reading it cannot see your reasoning**, because they
cannot.

## Two standing rules that apply here

**Every line is written from scratch.** waku, orca, t3code and Zed are read for behaviour and
dimensions, never copied — and this is now checked mechanically rather than trusted:

```bash
Scripts/transplant-check.py     # 0 clean · 1 candidates · 2 references missing
```

It runs in seconds. Run it before you hand off. It found two files tonight that were 117 of 119
lines verbatim from Zed, and they were deleted. A form is exactly the kind of surface where a
reference is tempting.

**Whoever widens an enum owns every construction site and match arm it breaks, in any file — but
only those.** Adding a variant to `SidebarEvent` is not covered by that rule, because you are not
widening it: you are publishing your own event type. Keep it that way.

## Done means

Not "it compiles" and not "the tests pass". Done is:

1. `parse_progress` and the clone path have tests that would fail if the backend were broken.
2. Both forms exist as surfaces with their guards as unit tests.
3. A handoff naming Half B in one line per change, precise enough for `codex12` to act without
   asking.
4. `Scripts/transplant-check.py` run, and its result stated.

**The critic will try to clone a repository by typing into this app.** If it cannot reach your form,
the rows stay `absent` no matter how good the code is — that is the failure mode this brief exists
to prevent, and it has already happened five times.
