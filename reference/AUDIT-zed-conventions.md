# Whole-workspace audit vs. Zed conventions — tiller (rust/gpui-rewrite)

Method: static review only, no build (disk was full; confirmed real ENOSPC on `cargo build -j 2`,
reported rather than fought, then the assignment moved to a read/grep-only review per instruction).
`cargo test` was not run either — every finding below is read directly from source, with file:line,
and cross-checked by reading both sides of every call site cited.

Ranked by how much it would cost someone: a crash or silent data-loss ranks over a design smell.

---

## 1. The real app's entire window chrome runs on hardcoded fixture data, disconnected from the
   domain crate built specifically to feed it — the sixth instance of the duplicated/disconnected-
   truth defect

`crates/tiller/src/main.rs:716-735` (the actual `main()` window-construction path, opening the real
1470x833 window) builds every chrome component from a `::fixture()` constructor:

```rust
cx.new(Titlebar::fixture),
cx.new(|cx| Sidebar::fixture(cx)),
tab_bar,                              // built from TabBar::fixture(cx) a few lines up
cx.new(|_| StatusBar::fixture()),
```

`crates/tiller_ui/src/sidebar.rs:1-5` says outright:

> "This first Rust implementation deliberately owns a small fixture model. The real project store
> will be connected by the integrator."

That integration never happened. `crates/tiller_project/src/lib.rs:1-27` describes a complete,
tested domain crate — `Project`, `Worktree`, `Tab`, `Workspace` — whose own doc comment says
`Workspace` holds "the pure tree operations **the sidebar needs**". `tiller_ui::sidebar` has zero
references to `tiller_project` (`grep -n tiller_project crates/tiller_ui/src/sidebar.rs` → no
hits), and `tiller` (the app crate) imports exactly one thing from `tiller_project`:
`use tiller_project::current_branch;` (`crates/tiller/src/main.rs:11`) — nothing else. The
707-line, well-tested `workspace.rs` (add/select/close tabs, filter, expand/collapse) is used
nowhere outside its own crate's tests.

Proof this isn't a defensible "the fixture just looks similar" situation: the exact project names
visible in the shipping app — `"cricchetto-firma-e-digitalizzazione-bff-app"`,
`"Project-Tracker"`, `"source"` — are **string literals** in
`crates/tiller_ui/src/sidebar.rs:120,147,156`. They are not read from the user's `~/Desktop/Progetti`
directory; they are typed into the fixture to match the frozen reference screenshot. Expanding a
project other than the hardcoded "tiller" row, or renaming a real folder on disk, changes nothing
the app shows, because nothing reads the disk for this. `Titlebar::fixture`, `TabBar::fixture`,
`StatusBar::fixture`, and `Settings::fixture` (`crates/tiller_ui/src/{titlebar,tab_bar,status_bar,
settings}.rs`) are in the same state — `grep -n "pub fn fixture" crates/tiller_ui/src/*.rs` returns
six hits and zero non-fixture constructors for any of them.

**What Zed does instead:** every one of Zed's panels (`project_panel`, `outline_panel`, `tab_bar`)
takes a `WeakEntity<Project>` or similar live handle in its constructor and renders directly from
it; fixture/preview data exists only behind `#[cfg(test)]` or a dedicated `story` binary, never in
the path `main()` actually calls. Zed would not ship a project tree that cannot show the user's
real projects.

**Cost:** this is not a bug that fires occasionally — it is the state of the shipping app. Every
review of this codebase that judged it by screenshot (mine included, in an earlier assignment) was
comparing pixel fidelity against a static prop, not against working software.

---

## 2. `TerminalView` spawn failure is `.expect()`-crashed on both the user-triggered and the
   session-restore path — the restore path can make the app fail to even open

`crates/tiller/src/main.rs:328-331` (user clicks "+ → New Terminal"):

```rust
let terminal = cx.new(|cx| {
    TerminalView::with_shell(&working_directory, shell, cx).expect("start terminal tab")
});
```

`crates/tiller/src/main.rs:648-654` — same pattern, but on **session restore at launch**, before a
window is even shown:

```rust
TabContent::Terminal {
    view: cx.new(|cx| {
        TerminalView::new(cwd, cx).expect("start restored terminal tab")
    }),
}
```

`TerminalView::with_shell`/`::new` forks a PTY — this can fail for entirely ordinary reasons: file
descriptor exhaustion (this exact development machine runs many concurrent worktrees and hits this
kind of resource pressure routinely, confirmed independently in an earlier ENOSPC/thread-exhaustion
finding on this same box), a worktree directory that no longer exists because the user deleted or
renamed it since the last session, or a sandbox denial. Either `.expect()` turns that into a full
process abort. The restore-path one is strictly worse: it runs unconditionally for every terminal
tab that was open in the last session, on every launch, before the user has done anything — a
single stale/removed worktree directory in a previous session's saved layout means Tiller can no
longer start at all.

**What Zed does instead:** Zed's terminal panel (`zed-ref/crates/terminal_view`) treats PTY spawn
failure as a `Result` surfaced into the pane as an error state (a message where the terminal would
be), never an `unwrap`/`expect` on the hot path of opening a tab. Session restore in Zed is built to
degrade gracefully per-item — one bad item does not prevent the workspace from opening.

**Cost:** whole-app crash, reachable both by a normal user action (open a terminal under fd
pressure) and passively on every future launch after a worktree is removed.

---

## 3. `SessionStore`'s flush thread has no shutdown path — the one background thread in the app
   crate that doesn't, in a codebase that elsewhere (same workspace) gets this right

`crates/tiller/src/session.rs:343-349`:

```rust
fn spawn_flusher(&self) {
    let inner = self.inner.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(FLUSH_POLL);
        flush_if_due(&inner);
    });
}
```

No `JoinHandle` is kept, no shutdown flag, no way to ever stop this thread — it polls every 25ms for
the lifetime of the process. `SessionStore::open`/`open_with` is called 9 times across
`crates/tiller/src/*.rs` (mostly in tests, per `grep -rn "SessionStore::open"`), so every test that
constructs one leaks another permanent 25ms-polling thread for the rest of that test binary's life.

Contrast this with `crates/tiller_control/src/server.rs:93-104,158-172` in the very same workspace,
which does the same shape of job (a background loop) correctly: a `shutdown: Arc<AtomicBool>`, a
`JoinHandle` stored in the struct, and a `stop()` that flips the flag and **joins** the thread
before returning, called from `Drop`. One crate in this codebase demonstrates the right pattern;
the app crate's own persistence layer doesn't use it.

**What Zed does instead:** any background polling task in Zed either runs as a cancellable
`Task<()>` owned by an `Entity` (dropped/cancelled automatically when the owner is dropped) or, for
a raw OS thread, is paired with an explicit stop signal and `join()`, exactly like
`tiller_control::ControlServer` already does here.

**Cost:** low in production (daemon thread, dies with the process), but it is a real "thread with
no shutdown path" of the exact kind asked about, it is inconsistent with a sibling crate in the same
codebase that already solved it, and it silently leaks a thread per `SessionStore` in every test
that constructs one.

---

## 4. Two crates each define their own `Tab`/`TabKind` — the richer one is dead code

- `crates/tiller_project/src/tab.rs:10-21`: `TabKind` has five variants — `Terminal, AgentChat,
  Browser, Editor, Diff` — and a `Tab { id: TabId, worktree_id: WorktreeId, title, kind }`.
- `crates/tiller_ui/src/tab_bar.rs:22-34`: an unrelated `TabKind` with two variants — `Chat,
  Terminal` — and a `Tab { id: usize, title, kind }`.

These are not the same type reused across crate boundaries; they are two independent enums with
the same name and overlapping purpose, and (per finding #1's grep) `tiller_project`'s five-variant
version — the one that actually models `Browser`/`Editor`/`Diff` tabs — is never constructed outside
its own crate's tests. `crates/tiller/src/main.rs:19` imports `tab_bar::TabKind` (the two-variant
one) and that is the only `TabKind` the running app can ever produce; a `Browser` or `Editor` or
`Diff` tab is structurally impossible in the shipping app even though a whole domain type exists
for it.

**What Zed does instead:** a concept that crosses a UI/domain boundary (e.g. Zed's `ItemHandle`/
`ProjectItem`) is defined once, in the layer that owns it, and the other layer depends on it —
never reimplemented with a different, narrower shape under the same name.

**Cost:** confusing to maintain (a future contributor extending `tiller_project::TabKind` with a
new variant will reasonably assume it reaches the UI; it does not), and it's the same
duplicated-truth shape as the theme bugs, just in the type system instead of at runtime.

---

## 5. `RightPanel`'s file-tree read is only half-backgrounded — it blocks the UI thread exactly
   where the code shows it already knows how to avoid that

`crates/tiller_ui/src/right_panel.rs:216-232` (`refresh`) correctly moves the expensive git-status
work off the main thread:

```rust
self.git_task = Some(cx.spawn(async move |this, cx| {
    let snapshot = cx.background_spawn(async move { load_snapshot(&repo_root) }).await;
    let _ = this.update(cx, |panel, cx| { panel.apply_snapshot(snapshot); ... });
}));
```

But `apply_snapshot` (`right_panel.rs:202-214`), called from inside that `this.update(...)`
closure — which runs back on the main/UI thread, that's what `Entity::update` is for — ends with:

```rust
self.file_tree = read_tree(&self.repo_root, &self.repo_root, &self.changed_paths);
```

`read_tree` (`right_panel.rs:1086-1110`) is a synchronous `std::fs::read_dir` walk. It was left
outside the `background_spawn` block that the same function already uses for the git call one line
above it — so every refresh does the expensive part off-thread and then blocks the UI thread anyway
for the directory listing. The same function is also called directly, unbackgrounded, from
`toggle_file` (`right_panel.rs:343-351`) — the click handler that expands a folder row in the Files
tree — so every folder expansion blocks the render/event thread on a filesystem syscall.

**What Zed does instead:** Zed's project panel never touches the filesystem synchronously in a
click handler or a render-adjacent path; directory scanning happens in the worktree's background
scanner and results arrive as entity updates. The pattern here shows the authors clearly know this
convention (they used it for git status one function up) and didn't apply it consistently to the
directory read in the same code path.

**Cost:** on a large directory, a network/FUSE-backed mount, or under the disk pressure this exact
project's own dev machine experiences, this is a visible hitch or hang triggered by a normal click,
not a rare edge case.

---

## 6. Tests that test the derive macros, not the code — `tiller_project`, three for three

`crates/tiller_project/src/tab.rs:57-71` (`tab_is_a_value_type`), `src/project.rs:46+`
(`project_is_a_value_type`), `src/worktree.rs:59+` (`worktree_is_a_value_type`) all follow the same
shape: construct a value, `.clone()` it, assert the clone equals the original and a couple of
fields read back what was passed in. Every one of these would still pass if the type's actual
behavior were deleted — they exercise `#[derive(Clone, PartialEq)]`, which the compiler already
guarantees, not anything `tiller_project` computes. None of the three crashes, none has a failure
case, none calls a method beyond field access.

**What Zed does instead:** Zed's value-type tests (e.g. `zed-ref/crates/gpui/src/geometry.rs`
tests) exercise actual computed behavior — arithmetic, comparison logic, edge cases — never just
"the struct round-trips through Clone."

**Cost:** these three tests give false confidence; a real regression in `tiller_project` (most of
which, per finding #1, no other crate even calls yet) would not be caught by its own test suite's
coverage of these three types.

---

## Findings considered and ruled out

- **The theme-duplication bug class (four prior instances)**: checked directly —
  `crates/tiller_theme/src/lib.rs:448` is the single `Theme` definition workspace-wide (`grep -rn
  "struct Theme\b"` → one hit), and `crates/tiller_ui/src/settings.rs:116-119`'s `set_theme_mode`
  delegates straight to `Theme::set_mode(mode, cx)` with no independent `mode` field cached in
  `Settings` itself. This class of bug appears genuinely fixed as of this branch.
- `crates/tiller_control/src/server.rs` (thread-per-connection, bounded buffer, no unwrap on I/O)
  and `crates/tiller_activity`/`crates/tiller_markdown` (pure logic, zero production `unwrap`/
  `expect`) were reviewed in a prior pass and remain the best-built crates in the workspace; nothing
  new to add here beyond what that review already covered.
- Cross-crate helper duplication (path shortening, hex color parsing, duration formatting,
  word-boundary matching) was searched for directly and not found beyond the two type-duplication
  cases above — worth stating since the brief specifically asked to look for it and a null result
  is itself informative.

---

## ONE gap — the single worst thing in this codebase

**Finding #1: the shipping app's entire window (sidebar, titlebar, tab bar, status bar) is built
from hardcoded fixture data and was never connected to `tiller_project`, the domain crate written
specifically to back it.** Every other finding here is a real bug with a real cost, but this one is
categorically different: it means the thing under review is not, yet, the application it appears to
be. A visual or behavioral critic exercising the running app — clicking around, comparing
screenshots to the reference — cannot see this, because the fixture was hand-tuned to look right.
Only a whole-codebase read, matching what `tiller_project`'s own doc comment claims against what
`tiller_ui` actually imports, surfaces it. That is exactly why this pass exists.
