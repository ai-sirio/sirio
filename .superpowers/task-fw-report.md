# Final fix wave report — Markdown Preview

## Implemented

- **F1:** Run local PlantUML with `PLANTUML_SECURITY_PROFILE=SANDBOX`; removed the ALLOWLIST configuration and `include_root` plumbing; updated the real sandbox test, spec, and `CLAUDE.md`.
- **F2:** Require PlantUML 1.2023.9; use the configured remote server for `NotAvailable` or `Unsandboxed`, retaining `Unsandboxed` when no server is configured.
- **F3:** Use a parsable version banner as the verdict regardless of exit status; parse the supplied 1.2026.8 banner.
- **F4:** Replaced the invalid-source fixture with `A -> B: hi\nthis is not plantuml (((`.
- **F5:** Preserve renderer SVG markup, derive logical size from its root, use the `svg-v2` cache-key salt, and treat cached root width as logical width. Updated design and `CLAUDE.md`.
- **F6:** A queued PlantUML render compares current and captured `DiagramSettings` after its turn; on mismatch it removes `Pending`, notifies, and returns. Added a deterministic GPUI executor regression test.
- **F7:** Opened the 0.26.0 cycle using `Scripts/set-workspace-version.sh 0.26.0`; Cargo check updated and committed `rust/Cargo.lock`.
- **F8:** Clarified self-closing HTML docs, added removal-content tests for iframe/object/embed/form/svg, instrumented `expand_html`, and corrected settings-effect documentation. The new embed test exposed and fixed the void-element synthetic-close issue.

## TDD evidence

### RED

- `cd rust && cargo nextest run -p sirio_diagram the_child_environment_is_sandboxed` — failed as expected before the SANDBOX change: actual `ALLOWLIST`, expected `SANDBOX`; 0 passed, 1 failed, 47 filtered out.
- `cd rust && cargo nextest run -p sirio_diagram an_unsandboxed_plantuml_falls_back_to_a_configured_server` — failed before fallback was implemented: `Err(Unsandboxed { version: "1.2020.2" })`; 0 passed, 1 failed, 49 filtered out.
- `cd rust && cargo nextest run -p sirio_diagram a_version_banner_is_authoritative_even_when_exit_status_is_nonzero` — failed before changing `check_version`; 0 passed, 1 failed, 50 filtered out.
- `cd rust && cargo nextest run -p sirio_diagram svg` — test-first interface change failed to compile because `as_rendered` had not yet been implemented (six expected unresolved-function errors).
- `cd rust && cargo nextest run -p sirio_markdown removed_with_its_content` — exposed the embed bug: 5 passed, 1 failed; actual `Text("before secret after")`, expected `Text("before  after")`.

F4 is a test-fixture correction; the corrected real-PlantUML syntax-error test passed on its first run. F6's deterministic GPUI regression test was added after the implementation, then run successfully; no pre-implementation RED run was recorded for F6.

### GREEN and requested verification

- `PATH=/tmp/claude-1000/-home-epalmisano--herdr-worktrees-sirio-worktree-clear-meadow-552a/3f680aa3-8453-441e-817c-ed30681887d3/scratchpad/puml/bin:$PATH cargo nextest run -p sirio_diagram` — **51 tests run: 51 passed, 0 skipped**. The real PlantUML tests ran, including `real_plantuml_is_sandboxed_and_offline` (3.714 s); no skip lines.
- `cargo nextest run -p sirio_diagram` — **51 tests run: 51 passed, 0 skipped**. With plain PATH the real-test helper returns early when PlantUML is unavailable; Nextest counts those early returns as PASS and suppresses their captured `SKIP` messages.
- `cargo nextest run -p sirio_markdown -p sirio_ui` — **978 tests run: 977 passed, 1 failed**. The sole failure is the brief's known pre-existing `sirio_ui settings::tests::control_socket_row_shows_resolved_path_and_toggles`, at `settings.rs:5028` (`the row reflects the disabled state`).
- `cargo nextest run -p sirio settings` — **24 tests run: 24 passed, 0 skipped**.
- `cargo clippy -p sirio_diagram -p sirio_markdown --all-targets -- -D warnings` — completed successfully; clean.
- `cargo check --workspace --all-targets` — completed successfully. Existing unrelated dead-code/unused warnings remain.
- Additional focused checks: `cargo nextest run -p sirio_markdown` — **62 passed**; `cargo nextest run -p sirio_ui a_queued_plantuml_render_is_dropped_when_settings_change` — **1 passed**.
- F7 command `cargo check --workspace` — completed successfully.

## Files changed

- `CLAUDE.md`
- `docs/superpowers/specs/2026-09-23-markdown-rich-preview-design.md`
- `rust/Cargo.toml`
- `rust/Cargo.lock`
- `rust/crates/sirio_diagram/src/lib.rs`
- `rust/crates/sirio_diagram/src/plantuml.rs`
- `rust/crates/sirio_diagram/src/svg.rs`
- `rust/crates/sirio_diagram/src/mermaid.rs`
- `rust/crates/sirio_diagram/src/server.rs`
- `rust/crates/sirio_ui/src/markdown_preview.rs`
- `rust/crates/sirio_ui/src/file_view.rs`
- `rust/crates/sirio_markdown/src/html.rs`
- `.superpowers/task-fw-report.md` (this report)

## Cargo.lock diff

No dependency or crate was added. F7 explicitly required committing the lockfile regenerated from the workspace version change; its complete diff is:

```diff
diff --git a/rust/Cargo.lock b/rust/Cargo.lock
index a8630660..96989bd2 100644
--- a/rust/Cargo.lock
+++ b/rust/Cargo.lock
@@ -7224,7 +7224,7 @@ checksum = "8ee5873ec9cce0195efcb7a4e9507a04cd49aec9c83d0389df45b1ef7ba2e649"
 
 [[package]]
 name = "sirio"
-version = "0.25.1"
+version = "0.26.0"
 dependencies = [
@@ -7265,7 +7265,7 @@ dependencies = [
 
 [[package]]
 name = "sirio_acp"
-version = "0.25.1"
+version = "0.26.0"
 dependencies = [
@@ -7297,7 +7297,7 @@ dependencies = [
 
 [[package]]
 name = "sirio_apply"
-version = "0.25.1"
+version = "0.26.0"
 dependencies = [
@@ -7306,7 +7306,7 @@ dependencies = [
 
 [[package]]
 name = "sirio_claude"
-version = "0.25.1"
+version = "0.26.0"
 dependencies = [
@@ -7314,7 +7314,7 @@ dependencies = [
 
 [[package]]
 name = "sirio_control"
-version = "0.25.1"
+version = "0.26.0"
 dependencies = [
@@ -7328,7 +7328,7 @@ dependencies = [
 
 [[package]]
 name = "sirio_diagram"
-version = "0.25.1"
+version = "0.26.0"
 dependencies = [
@@ -7347,7 +7347,7 @@ dependencies = [
 
 [[package]]
 name = "sirio_icons"
-version = "0.25.1"
+version = "0.26.0"
 
 [[package]]
 name = "sirio_lsp"
@@ -7390,7 +7390,7 @@ dependencies = [
 
 [[package]]
 name = "sirio_project"
-version = "0.25.1"
+version = "0.26.0"
 dependencies = [
@@ -7399,7 +7399,7 @@ dependencies = [
 
 [[package]]
 name = "sirio_registry"
-version = "0.25.1"
+version = "0.26.0"
 dependencies = [
@@ -7414,7 +7414,7 @@ dependencies = [
 
 [[package]]
 name = "sirio_release"
-version = "0.25.1"
+version = "0.26.0"
 dependencies = [
@@ -7428,7 +7428,7 @@ dependencies = [
 
 [[package]]
 name = "sirio_syntax"
-version = "0.25.1"
+version = "0.26.0"
 dependencies = [
@@ -7451,7 +7451,7 @@ dependencies = [
 
 [[package]]
 name = "sirio_terminal"
-version = "0.25.1"
+version = "0.26.0"
 dependencies = [
@@ -7469,7 +7469,7 @@ dependencies = [
 
 [[package]]
 name = "sirio_theme"
-version = "0.25.1"
+version = "0.26.0"
 dependencies = [
@@ -7481,7 +7481,7 @@ dependencies = [
 
 [[package]]
 name = "sirio_ui"
-version = "0.25.1"
+version = "0.26.0"
 dependencies = [
@@ -7523,7 +7523,7 @@ dependencies = [
 
 [[package]]
 name = "sirio_update"
-version = "0.25.1"
+version = "0.26.0"
 dependencies = [
```

## Self-review findings

Read the complete branch diff once. Changes are limited to the requested implementation/spec/docs/lock/report files. Confirmed: `include_root` and ALLOWLIST value-generation code are gone; SANDBOX/env scrubbing and process-tree kill remain; the minimum version is enforced before local rendering; banner parsing precedes exit-status handling; all SVG call sites use as-rendered markup; old cache files are separated by `svg-v2`; the queued-settings check occurs after the PlantUML turn and before rendering; and workspace version/lock entries are 0.26.0.

## Concerns

The requested combined `sirio_markdown`/`sirio_ui` run retains the brief's one known pre-existing settings failure. Workspace check emitted unrelated existing warnings. No dependencies were added. The full `Scripts/ci.sh`/`Scripts/ci-linux.sh` gates were not run, per the brief.

## Commits

One commit for each group, in order:

1. `86b1213b` `fix(diagram): run plantuml under the sandbox profile`
2. `6996f7e3` `fix(diagram): require plantuml 1.2023.9 for the sandbox`
3. `8c019a8f` `fix(diagram): read the plantuml version whatever its exit status`
4. `8f55e3b7` `test(diagram): use an invalid source for the real syntax-error test`
5. `4562d863` `fix(diagram): leave svg sizing to gpui`
6. `c8e30c7e` `fix(ui): drop queued diagram renders when the settings change`
7. `0e498951` `chore: move the workspace to 0.26.0`
8. F8 report-bearing commit: `chore: tidy the markdown preview wave`
