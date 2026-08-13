# P71 — The failures only stderr sees

**This brief is everything you need; your context was just reset.**

## The shape

`F-PRJ-04` is recorded as *"insertion failures are `eprintln`-only — no error surface in the Linux
build"*. An hour before I read that row, I had found the identical shape in brand-new code:
`codex12`'s `None` branch in `add_chat_tab` refuses a silent agent fallback — correctly — and then
reports the refusal only to stderr, so New Chat → pi/omp/opencode does nothing a user can see.

Two instances of one shape is a reason to count. **37 `eprintln!`** across the app and UI crates, 26
of them in `main.rs`. The worst:

| site | what the user does | what the user is told |
|---|---|---|
| `main.rs` `[files] save failed: {error}` | presses Ctrl+S; the write fails | nothing |
| `main.rs` `[files] could not open the file picker: {error}` | clicks Open File | nothing |
| `main.rs:~2939-2940` `[projects] …` | adds a project that is nested/duplicate, or fails | nothing |
| `main.rs` `[chat] …` `None` branch | picks an agent with no ACP server | nothing |

The save one is the sharpest: `F-EDIT-06` (the save path) was confirmed **built, by exercise**, on
the same day. A working save sits directly beside a failure mode the user cannot perceive.

## The ledger's wording is wrong about the cause, and that matters

`F-PRJ-04` says *"no error surface in the Linux build."* **There is one, it is public, it is
rendered, and it is already used for this exact class of error.**

- `sidebar.rs:274` `notice: Option<String>`
- `sidebar.rs:613` `pub fn set_notice(&mut self, notice: impl Into<String>, cx: &mut Context<Self>)`
- rendered at `sidebar.rs:~1856` under the id `sidebar-notice`
- **already used** at `sidebar.rs:662` for `could not open the folder picker: {error}` — the same
  sentence `main.rs` prints to stderr for a *different* picker

And `main.rs` can reach it: `main.rs:2304` holds `sidebar: Entity<Sidebar>`, and the
`self.sidebar.update(cx, |sidebar, cx| …)` idiom already appears at `:2718`, `:3042`, `:3288`,
`:3294`.

So this is not missing UI. It is a wired surface that four call sites decline to use. Say so in your
report — a row that blames absence when the cause is a disconnected call sends the next builder to
construct something that is already there.

## Two halves, split by owner. Take only yours.

### Half A — `codex11`: give `FileView` the surface `Sidebar` already has

`file_view.rs` has a private `notice(…)` **renderer** (used for `"Loading file…"` and the >1 MiB
case) but **no state field and no public setter**, so nothing outside the file can raise a message.

Mirror `Sidebar`'s exact shape, because a second idiom for the same job is its own defect:

- `notice: Option<String>` on `FileView`
- `pub fn set_notice(&mut self, …, cx: &mut Context<Self>)`, and a way to clear it
- render it through the **existing** `notice(…)` helper — do not invent a second look
- keep every existing constructor compiling untouched, so `codex12` is never blocked by you

**Non-breaking is the whole point.** It is why P66/P67 and P70 ran in parallel without colliding.

### Half B — `codex12`: call the surfaces instead of printing

- `main.rs:~2939-2940` (`add_project`) → `self.sidebar.update(cx, |s, cx| s.set_notice(…, cx))`.
  Both arms: `Ok(false)` is *"already tracked or nested"*, which is a normal thing a user does and
  must be told about, not an error. **This closes `F-PRJ-04` and is roughly two lines.**
- `[files] save failed` and `[files] could not open the file picker` → Half A's setter, **once Half A
  has landed**. If it has not, do the project half and say so; do not add a second notice mechanism.
- The `add_chat_tab` `None` branch → tell the user the adapter has no ACP server. `pi` already built
  the vocabulary: `tiller_agents::AgentAvailability::acp_status_label()` returns `"No ACP server"`,
  and Settings already shows it as a badge. Reuse that string; do not write a third phrasing.

**Leave the rest of the 37 alone.** `[control] …`, `[session] …` and the window-open failure are
diagnostics for whoever runs the binary from a terminal, and turning every one of them into UI is a
different, worse defect. Four sites are in scope. If you think a fifth belongs, name it rather than
doing it.

## Evidence

Follow `docs/linux-rewrite/EVIDENCE-STANDARD.md`. **Named drawn tests** (`TestAppContext` /
`VisualTestContext`, `.debug_selector(id)`, full `run_until_parked()` pump).

The test that counts **provokes the real failure and asserts the message is on screen** — add a
project that is already tracked and assert the sidebar notice renders with that text. A test that
calls `set_notice` directly and asserts it rendered proves the setter, not the wiring, and the wiring
is the entire defect. For the save path, make the write actually fail (a read-only file or a path
that cannot be created) rather than injecting an error.

Note for whoever writes these: `pump_until(..)` in `changes.rs`/`right_panel.rs` ends in `panic!`, so
a wait *is* an assertion — it is a legitimate way to prove an async notice arrived.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`. **Your
  pane starts in the main repo on `rust/gpui-rewrite` — `cd` first.**
- **`codex11` owns:** `tiller_ui/src/file_view.rs` (and its usual set). **`codex12` owns:**
  `tiller/src/main.rs`, `tiller_control/**`, `tab_bar.rs`. Do not cross.
- **Do not edit** `chat.rs`, `settings.rs`, `sidebar.rs`, `status_bar.rs`, `tiller_agents/**` (`pi`);
  `tiller_theme/**`, `controls.rs`, `titlebar.rs`, `composer.rs` (`sonnet`). **`sidebar.rs` is
  `pi`'s — Half B only *calls* its existing public `set_notice`, it does not edit the file.**
- **Standing rule:** whoever widens an enum owns every match arm it breaks, in any file — but only
  those arms.
- Colours, spacing and radii from `tiller_theme::Theme`, never a literal. Visual bar is **Pop!_OS
  COSMIC**, not waku. Name any token you need and lack; that list is how `sonnet` learns.
- **Establish the build state with the gate's own commands**, not a paraphrase:
  `grep -n clippy Scripts/ci-linux.sh` and run exactly what it says. The orchestrator once ran a
  weaker clippy without `-D warnings`, called the tree clean while the gate was red, and overruled
  three agents who were right. Do not inherit that mistake.
- **Never copy code from the reference checkouts.** Mark rows `builder-claimed, unverified`, never
  `PASSED`. **Proceed without asking for approval.**

## Reporting

**12 lines or fewer**: which half you took, the sites you converted and the drawn tests that prove
the message reaches the screen *from the real failure* (by name), that `F-PRJ-04`'s cause was a
disconnected call rather than a missing surface, whether Half A had landed when you needed it, any
fifth `eprintln!` you think belongs in scope named rather than done, the gate run with its own
invocation with not-yours failures named separately, tokens `tiller_theme` still lacks, and the
honest remainder.
