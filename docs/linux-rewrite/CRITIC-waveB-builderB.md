# Fresh-critic pass — wave B, builder B (2026-08-18)

Box: x86_64 desktop per `ENVIRONMENT.md` top section, Wayland lane label `jdgB`, binary pinned at
`/tmp/jdgB-tiller` (built from a warm `cargo build --workspace`, exit 0).

## F-CORE-DOM-03 — verdict: PASSED

VERIFY: "Start with no project defaults and observe the proposed project location; repeat with a
Linux replacement root and confirm it is deterministic."

Live-drove the actual Create-Project form (not the socket) on the Wayland lane: clicked the `+`
button next to Projects (293,50), then `Create Project…` in the menu (180,142), and read the
rendered `Parent location` field.

- **No overrides** (`TILLER_PROJECTS_DIR`/`XDG_DATA_HOME` unset, `$HOME=/home/enzopalmisano`):
  form showed `Parent location: /home/enzopalmisano/Tiller/projects`, "Creates
  /home/enzopalmisano/Tiller/projects/" — exactly `$HOME/Tiller/projects`, matching
  `tiller_project::default_project_base()` (`rust/crates/tiller_project/src/domain.rs:6-15`) and
  the Swift original `ProjectDefaults.defaultProjectsRoot()` (`Packages/TillerCore/Sources/
  TillerCore/ProjectDefaults.swift:6-8`) seeding `CreateNewProjectView.parentDir`
  (`App/AddProjectSheet.swift:291`).
- **`TILLER_PROJECTS_DIR=/home/enzopalmisano/CriticOverrideRoot/projects` set** (fresh instance,
  fresh sqlite): form showed `Parent location: /home/enzopalmisano/CriticOverrideRoot/projects`
  verbatim — the override wins deterministically, matching `default_project_base()`'s first
  branch.

Screenshots: `/tmp/jdgB-shots/03-create-form2.png` (no-override case),
`/tmp/jdgB-shots/03-create-form-override.png` (override case) — not committed (scratch host paths
per the drive lane convention; both conjuncts of the VERIFY clause were driven live and read
directly off the rendered form, a discriminator no code-reading or socket call could fake).

Both conjuncts of the VERIFY clause are proven live. `Sidebar::project_form_parent()` calling
`tiller_project::default_project_base()` (`rust/crates/tiller_ui/src/sidebar.rs:1295-1299`) is
real, not merely present in source.

## PORT-1 — verdict: half-proven (but for a different reason than the builder's own commit message)

Builder's own claim: gated `tiller_ui/src/lib.rs`'s `pub mod browser;` to `target_os = "linux"`,
verified natively (build + full `tiller_ui` test suite, 344 passed) and via a native-Linux check
with the gate temporarily flipped to `target_os = "macos"` (excluding the module the way a real
non-Linux target would) — confirming nothing **else inside `tiller_ui` itself** references
`browser::` unconditionally. Builder states plainly this does not exercise wry/raw-window-handle
genuinely absent from a real cross-target dependency graph, and that cross `cargo check` dies in
`libsqlite3-sys` before reaching `tiller_ui` source.

**Reproduced the `libsqlite3-sys` wall myself**, independently: `cargo check --manifest-path
rust/Cargo.toml --target aarch64-apple-darwin -p tiller_ui` (this box has the `aarch64-apple-darwin`
rustup target installed) fails in `libsqlite3-sys`'s build script — `cc: error: unrecognized
command-line option '-arch'` — one dependency layer before `tiller_ui` source is ever reached.
Confirms the builder's claim that a green gate on this box is not evidence of anything for PORT-1.

**Then extended the builder's own discriminator one level up, and it fails.** The builder tested
only whether anything inside `tiller_ui` itself references `browser::` unconditionally. It does
not — but the *consumer* crate does. Applied the same "flip the gate to `target_os = "macos"` and
check natively" technique to the whole workspace instead of just `tiller_ui`:

```
sed: #[cfg(target_os = "linux")] -> #[cfg(target_os = "macos")] in tiller_ui/src/lib.rs (temporary,
     reverted with `git checkout --` immediately after; see `git status --short` was clean before
     and after)
cargo check --manifest-path rust/Cargo.toml --workspace
```

Result — two unresolved-import errors that a genuine cross build (or genuine non-Linux target)
would also hit, just one layer later than before the builder's fix:

```
error[E0432]: unresolved import `tiller_ui::browser`
  --> crates/tiller/src/main.rs:39:5
   |
39 |     browser::{BrowserEvent, BrowserSurface, normalize_address},
   |     ^^^^^^^ could not find `browser` in `tiller_ui`

error[E0432]: unresolved import `tiller_ui::browser`
  --> crates/tiller/src/panes.rs:16:17
   |
16 | use tiller_ui::{browser::BrowserSurface, changes::ChangesTab, chat::Chat, file_view::FileView};
   |                 ^^^^^^^ could not find `browser` in `tiller_ui`
```

`crates/tiller/src/main.rs:39` and `crates/tiller/src/panes.rs:16` both `use tiller_ui::browser::…`
with **no `#[cfg(target_os = "linux")]` gate of their own**. PORT-1's fix moved the compile failure
from "wry/raw-window-handle missing from the dependency graph, inside `tiller_ui`" to "unresolved
import, inside `tiller` the binary crate" — better (a clean E0432 instead of a deep dependency-graph
failure), but the `tiller` binary crate itself **still does not compile for a non-Linux target**,
which is the actual shape of the original portability debt item. The module-declaration gate alone
does not close PORT-1; the two call sites that consume `browser::` unconditionally also need a
matching `#[cfg(target_os = "linux")]` (or the imports need to move inside an existing Linux-gated
block) before a cross build could plausibly succeed.

Change was fully reverted before moving on — `git diff rust/crates/tiller_ui/src/lib.rs` empty,
confirmed with `git checkout -- rust/crates/tiller_ui/src/lib.rs` and a clean `git status --short`
on that path both before and after.

**No regression on the Linux browser** — checked as instructed. Native `cargo build --workspace`
after the real fix: exit 0, unaffected (same two pre-existing warnings as `ENVIRONMENT.md` records,
nothing new). Live-drove `Scripts/x11-nested-drive.sh`: `browser.open url=https://example.com` +
`tab.select index=3` + forced repaint rendered the actual page — "Example Domain" heading, body
copy, and the "Learn more" link, not just chrome (`/tmp/jdgB-x11-shots/02-browser-content.png`,
not committed). The browser surface itself is unaffected by the `lib.rs` gate; the finding above is
about cross-target compilability, not the Linux build.

**What is proven**: the module-declaration gate is real, does not regress the native Linux build or
the browser's actual page rendering, and genuinely excludes the module when `target_os` is not
Linux (confirmed both by the builder's own narrower test and by mine on the whole workspace).
**What remains unproven / actually disproven**: that `tiller` — the shipping binary, not just the
`tiller_ui` library — would compile on a non-Linux target. It provably would not, via the two
ungated `use tiller_ui::browser::…` sites this pass found with a real (if simulated) compiler
error, not a grep guess. A true cross-target linker/toolchain for `aarch64-apple-darwin` remains
unavailable on this box (`libsqlite3-sys`'s C build script has no working `cc` for that triple
here), so the very last mile — an actual `cargo build --target aarch64-apple-darwin` succeeding
end to end — is still not exercised by anyone on this hardware.
