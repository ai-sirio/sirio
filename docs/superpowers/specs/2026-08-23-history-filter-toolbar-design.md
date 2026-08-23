# History filter toolbar — Design

Date: 2026-08-23
Branch: `main`

## Goal

Put an IntelliJ-style filter toolbar above the git history in the right panel:
a "Text or hash" field with regex and case-sensitivity toggles, Branch / User /
Date / Paths dropdowns, and an IntelliSort toggle. Eight controls, full parity
with the reference screenshot.

Today the History view runs one fixed query — `git log --branches --date-order
--skip=N -n500` (`tiller_git/src/log.rs:40`) — with no predicates at all.

## Prerequisite, already landed

The right panel is now user-resizable between 220 and 640px
(`docs/superpowers/specs/2026-08-23-panel-resize-design.md`). That is why this
toolbar is designed responsive rather than against a fixed 405px: the width is
no longer a constant to lay out against.

## Decisions

- **Filtering happens git-side.** Every predicate becomes a `git log`
  argument; the UI never filters a loaded vector. Two findings forced this and
  they interlock: git applies predicates *before* `--skip`/`-n`, and the
  fragile invariant in `GitHistory` is `skip = self.commits.len()`
  (`history.rs:77`), which assumes `commits` is exactly a prefix of the log.
  Filter git-side and that invariant stays true. Filter client-side and it
  breaks, along with `exhausted` (derived from `commits.len() < CHUNK`) and the
  pagination trigger inside `uniform_list`.
- **The graph column is hidden whenever a filter is active.** On a filtered
  set the rows are not contiguous in history, so the lanes would connect
  commits that are not parent and child. Hiding is the only option that does
  not draw a false structure, and it gives the subject back ~30px exactly when
  results need reading. This is also what IntelliJ does.
- **Text search matches the whole commit message**, body included, because
  that is what `--grep` does and there is no subject-only flag. A row that
  matched on body text alone is annotated with the matching snippet, so no
  result is inexplicable.
- **The bulk log format stays at six fields.** See "Annotating a match"
  below — this is the decision the measurement changed.
- **IntelliSort is `--date-order` ↔ `--topo-order`.** Not a direction toggle.
  Under `--topo-order` a merged branch's commits stay contiguous under the
  merge instead of being interleaved by date with other lines of history,
  which is IntelliJ's behaviour. Being a query flag, it composes with
  pagination; a view-level sort would only reorder the loaded prefix and lie
  the same way client-side filtering does.
- **A changed filter is a full reset**, never an incremental narrowing: bump
  `generation`, clear `commits` and `rows`, `exhausted = false`, reload from
  `skip = 0`.

## Architecture

### 1. `LogFilter` — the pure seam

```rust
// tiller_git/src/log.rs
pub struct LogFilter {
    pub text: Option<String>,
    pub regex: bool,           // -E when true, -F when false
    pub case_sensitive: bool,  // -i when false
    pub branches: Vec<String>, // --branches=<glob>, repeated
    pub authors: Vec<String>,  // --author=<p>, repeated (git ORs them)
    pub since: Option<String>,
    pub until: Option<String>,
    pub paths: Vec<PathBuf>,   // pathspec, after `--`
    pub topo_order: bool,
}

impl LogFilter {
    /// The `git log` arguments this filter means. Builds nothing else and
    /// runs nothing.
    pub fn args(&self) -> Vec<String> { … }
}
```

`GitLog::commits(repo, skip, limit, &LogFilter)` consumes it. Every flag
combination is then testable without git and without a window — the same seam
that made `resolve_panel_widths` testable in the panel-resize work.

What each control compiles to, and what it costs:

| control | git | note |
|---|---|---|
| text | `--grep=<p>` | whole message; O(walk), not O(page) |
| regex on/off | `-E` / `-F` | |
| case | `-i` when insensitive | also applies to `--author` |
| Branch | `--branches=<glob>` | the real performance lever: fewer reachable commits |
| User | `--author=<p>` repeated | O(walk) |
| Date | `--since` / `--until` | the only filter that prunes the traversal |
| Paths | pathspec after `--` | slowest; history simplification applies |
| IntelliSort | `--topo-order` | |

### 2. Annotating a match without paying for it

A row that matched on body text needs the body to show why. The obvious
route — add `%b` to `LOG_FORMAT` (`log.rs:8`) — was measured on this repo and
rejected:

- output for 500 commits grows from **91,034 to 467,385 bytes (5.1×)**;
- `%b` becomes a seventh field, so the parser's `fields.len() != 6` guard
  (`log.rs:89`) must change or **every record is silently dropped**;
- a commit body may legitimately contain 0x1e or 0x1f — git forbids only NUL —
  and those are precisely this format's record and field separators. Zero
  occurrences in this repo today; not a guarantee;
- 5× output moves the read closer to `GitCommandResult`'s truncation cap.

The alternative costs nothing and needs no parser change. When a text filter is
active, for each **visible** row:

```
does the subject already contain the match?
  yes → no annotation needed, cost zero
  no  → the match is necessarily in the body (git returned this row, and the
        subject does not explain it) → fetch that one body with
        `git show -s --format=%b <sha>`, cached by sha
```

The deduction is the point: nothing has to ask git why a row was included. Cost
is bounded by the visible rows — roughly twenty — instead of 467KB per chunk of
500, most of which would never be read.

### 3. "Text or hash"

An input of **seven or more hexadecimal characters** is tried as a commit
first, via `git rev-parse --verify <input>^{commit}`; if it resolves, that
commit is the result, otherwise the input falls back to text search. Seven is
git's own default abbreviation length, and below it collisions with real words
are common — `cafe`, `face`, `dead` are all valid hex.

Prefix search is not expressible in `git log` at all, which is why this is a
resolution step rather than a predicate.

### 4. Responsive layout

A second pure function, mirroring the first:

```rust
fn toolbar_layout(panel_width: f32) -> ToolbarLayout  // OneRow | TwoRows | Collapsed
```

| band | shape |
|---|---|
| ≥ 470px | one row: field + `.*` + `Cc` + four chips + IntelliSort |
| 260–470px | two rows: field and toggles above, chips and IntelliSort below |
| < 260px | two rows, the four chips collapsed into one `Filters (n)` popup |

The thresholds are derived, not chosen: the four chips measure ~257px together
and the field needs ~190px to stay readable, so one row needs ~455px plus
margin. At the panel's 220px minimum, 204px remain usable — a 150px field plus
two 24px toggles.

### 5. Where each dropdown's options come from

| dropdown | source | cost |
|---|---|---|
| Branch | `git branch --format=%(refname:short)` — local branches, multi-select. An empty selection means `--branches`, i.e. today's behaviour | one cheap call, refreshed with the tree |
| User | the distinct `%an` values among the commits **currently loaded**, plus free-text entry | zero extra calls |
| Date | fixed presets — Today, Last 7 days, Last 30 days, Custom — passed to `--since`/`--until` as git's own relative strings (`"7 days ago"`) rather than computed timestamps | zero |
| Paths | free-text pathspec, plus a shortcut that fills it from the selected commit's changed files | zero |

The User list is the one that needs stating plainly: it is derived from what is
loaded, so it is a convenience list, not the repository's full author set. The
exhaustive alternative — `git log --format=%an --branches | sort -u` — is a
complete walk of every reachable commit, the very cost the person filtering is
trying to avoid. The dropdown therefore labels itself as authors *in the loaded
history*, and free-text entry covers anyone not in it. A list that quietly
claims completeness it does not have would be worse than one that admits the
limit.

No directory picker for Paths: a pathspec is more expressive than a picker, and
git validates it.

### 6. Loading, debounce and empty states

The text field debounces at **250 ms** on a gpui timer; Enter forces
immediately. Every keystroke would otherwise start a full walk of every
reachable commit. The timer is a gpui timer and not `std::thread::sleep`
because only the former can be advanced by `background_executor.advance_clock`
— the same lesson the panel-resize debounce paid for.

`EmptyReason` (`history.rs:22`) gains a third variant, `NoMatches`. Without it
a filter with no results reports *"No commits yet"* on a repository full of
commits.

### 7. Where the code lives

`history.rs` is already 708 lines; the toolbar and filter model would push it
past 1100.

| file | responsibility |
|---|---|
| `tiller_git/src/log.rs` | `LogFilter` and `args()` — the translation to git arguments |
| `right_panel/history_toolbar.rs` | **new** — toolbar rendering and `toolbar_layout` |
| `right_panel/history.rs` | unchanged remit: state, loading, rows |

## Testing

| test | what it pins |
|---|---|
| `LogFilter::args()` per flag | `-F` vs `-E`, `-i`, repeated `--author`, pathspec after `--`, `--topo-order` |
| `toolbar_layout` at 259/260 and 469/470 | the boundaries, not the middles of the bands |
| hash disambiguation | 6 hex → text, 7 hex → resolution attempt, non-hex → text |
| filter change resets | `generation` bumped, `commits` cleared, reload from `skip = 0` |
| filter with no results | reports `NoMatches`, **not** `NoCommits` |
| drawn: graph column | disappears while a filter is active |
| `advance_clock(250ms)` | three keystrokes produce one query |

Testing thresholds at their boundaries is the difference between a threshold
test and a decorative one: a layout exercised at 300 and 500px passes whether
the boundary is at 400 or at 470.

## Accepted limitation

`--skip=N` is O(N): git generates and discards N commits per page, and each
page re-walks from scratch. Deep pagination is therefore already slow today and
stays exactly as slow with filters active. This spec does not address it — it
neither improves nor worsens it, and fixing it means replacing offset
pagination with a commit-cursor, which is its own piece of work.
