# Bezel Foundation Bump Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bump the gpui family `=0.3.6` → `=0.3.8` (re-porting the vendored XDND `PendingDrop` fix), then bezel `=0.1.3` → `=0.1.4`, in two bisectable commits.

**Architecture:** Two stages on one branch. Stage A (Tasks 1–4) touches only dependency plumbing: the vendored `bezel-gpui-linux` fork is re-based onto 0.3.8 with the `PendingDrop` fix re-applied, then the workspace pins move and compile fallout is fixed per crate. Stage B (Tasks 5–6) bumps the bezel facade and ends in a mandatory user visual review of the new unified frost/glass surface model. One commit per stage — deliberately **not** frequent commits, so any regression bisects to exactly one cause (gpui runtime vs bezel appearance).

**Tech Stack:** Rust workspace under `rust/`, cargo `[patch.crates-io]` vendoring, bezel 0.1.x / bezel-gpui 0.3.x from crates.io.

**Spec:** `docs/superpowers/specs/2026-08-31-bezel-gallery-adoption-design.md` (sub-project 1).

## Global Constraints

- NEVER run `Scripts/ci.sh` or `Scripts/ci-linux.sh` — workspace gates run only on the user's explicit request. Iterate with `cargo build -p <crate>` / `cargo test -p <crate>`.
- Zig **exactly 0.15.2** must be on PATH for anything that builds `sirio_terminal` (newer fails too). Check with `zig version` before those steps.
- Never hand-edit any `Cargo.lock` — locks move only via `cargo update` / normal builds.
- Pins are exact: stage A ends with `gpui`/`gpui_platform` at `=0.3.8` and `bezel` still `=0.1.3`; stage B ends with `bezel` at `=0.1.4`. The published 0.3.8 version string is `0.3.8+zed.82aeef`; build metadata is ignored by Cargo comparison, so the pins are written `=0.3.8`.
- Test failures are compared to the baseline as **lists of test names, not counts** — `main` already has known-red tests plus load-sensitive flakes.
- Commit messages: Conventional Commits, lower-case imperative subject.
- Working tree has unrelated modifications (`AGENTS.md`, `CLAUDE.md`, `rust/crates/sirio_ui/src/sidebar.rs`). Never `git add -A`; stage only the files each commit step names.
- All cargo commands for the workspace run from `rust/` (`cd rust && …`); vendored-crate commands use `--manifest-path` from the repo root.

---

### Task 1: Capture per-crate test baselines

**Files:**
- Create: `/tmp/bezel-bump/baseline-<crate>.txt` (scratch, not committed)

**Interfaces:**
- Consumes: current `main` working tree, unmodified `rust/Cargo.toml`.
- Produces: `/tmp/bezel-bump/baseline-{sirio_theme,sirio_ui,sirio_terminal,sirio,vendored}.txt` — failure lists later tasks diff against.

- [ ] **Step 1: Verify Zig toolchain**

Run: `zig version`
Expected: `0.15.2` exactly. If missing or different, stop and report — do not "fix" by upgrading Zig.

- [ ] **Step 2: Capture baselines for the four gpui-dependent crates**

```bash
mkdir -p /tmp/bezel-bump
cd rust
for c in sirio_theme sirio_ui sirio_terminal sirio; do
  cargo test -p "$c" 2>&1 | tee /tmp/bezel-bump/baseline-$c.txt | tail -3
done
```

Expected: each ends with a `test result:` line. Failures are allowed — this is the baseline, not a gate.

- [ ] **Step 3: Capture the vendored crate baseline**

```bash
cd "$(git rev-parse --show-toplevel)"
cargo test --manifest-path rust/vendor/gpui_linux/Cargo.toml 2>&1 | tee /tmp/bezel-bump/baseline-vendored.txt | tail -3
```

Expected: `test result: ok` — the vendored fork's own tests (including the `PendingDrop` regression tests in `src/linux/wayland/client.rs`'s `mod tests`, line ~3002) are green today.

- [ ] **Step 4: Extract each baseline's failed-test list**

```bash
for f in /tmp/bezel-bump/baseline-*.txt; do
  echo "== $f"; grep -E "^test .* FAILED" "$f" | sort > "${f%.txt}.failed"; cat "${f%.txt}.failed"
done
```

Expected: the `.failed` files exist (possibly empty for some crates). These exact lists are the comparison target in Tasks 4–5.

---

### Task 2: Re-base the vendored `bezel-gpui-linux` fork onto 0.3.8

**Files:**
- Modify: `rust/vendor/gpui_linux/src/` (whole tree replaced with published 0.3.8, then fix re-applied to `src/linux/wayland/client.rs`)
- Modify: `rust/vendor/gpui_linux/Cargo.toml` (version + mirrored dependency fields)
- Modify: `rust/vendor/README.md` (version references)
- Test: the crate's own `mod tests` inside `src/linux/wayland/client.rs`

**Interfaces:**
- Consumes: pristine 0.3.6 source in the local registry at `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/bezel-gpui-linux-0.3.6+zed.d9ad6a/`; vendored fork at `rust/vendor/gpui_linux/`.
- Produces: a vendored crate whose `Cargo.toml` says `version = "0.3.8"`, source byte-for-byte upstream 0.3.8 except the `PendingDrop` mechanism in `client.rs`, standalone tests green. Task 3's `[patch.crates-io]` resolution depends on this version field matching the new pins.

- [ ] **Step 1: Extract the fix as a patch and confirm it is confined to `client.rs`**

```bash
cd "$(git rev-parse --show-toplevel)"
PRISTINE=~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/bezel-gpui-linux-0.3.6+zed.d9ad6a
diff -rq "$PRISTINE/src" rust/vendor/gpui_linux/src | grep -v "^Only in $PRISTINE"
```

Expected: exactly one line, for `src/linux/wayland/client.rs` (the README's claim that nothing else was changed). If more files differ, stop and report — the plan's assumption is wrong.

```bash
mkdir -p /tmp/bezel-bump
diff -u "$PRISTINE/src/linux/wayland/client.rs" rust/vendor/gpui_linux/src/linux/wayland/client.rs > /tmp/bezel-bump/pendingdrop.patch
wc -l /tmp/bezel-bump/pendingdrop.patch
```

Expected: a non-empty patch (several hundred lines including context).

- [ ] **Step 2: Download and extract published 0.3.8**

```bash
cd /tmp/bezel-bump
curl -sL -A sirio-dev "https://static.crates.io/crates/bezel-gpui-linux/bezel-gpui-linux-0.3.8%2Bzed.82aeef.crate" -o v038.crate
tar xzf v038.crate
ls bezel-gpui-linux-0.3.8+zed.82aeef/src
```

Expected: extraction succeeds; `src/` contains a `linux/` tree.

- [ ] **Step 3: Replace the vendored source tree**

```bash
cd "$(git rev-parse --show-toplevel)"
rm -rf rust/vendor/gpui_linux/src
cp -R /tmp/bezel-bump/bezel-gpui-linux-0.3.8+zed.82aeef/src rust/vendor/gpui_linux/src
git -C rust/vendor/gpui_linux status --short | head
```

Expected: `src/` shows as modified. `Cargo.toml`, `Cargo.lock`, `README.md`, `LICENSE-APACHE` untouched so far.

- [ ] **Step 4: Re-apply the `PendingDrop` fix**

```bash
cd "$(git rev-parse --show-toplevel)"
patch --merge -p0 rust/vendor/gpui_linux/src/linux/wayland/client.rs < /tmp/bezel-bump/pendingdrop.patch
grep -c "<<<<<<<" rust/vendor/gpui_linux/src/linux/wayland/client.rs
```

Drift between 0.3.6 and 0.3.8 in this file is ~162 lines, so expect some hunks to conflict. Resolve every `<<<<<<<` marker by hand using this anchor map of what the fix consists of (all in `client.rs`; line numbers from the 0.3.6 fork as orientation only):

- `DragState` struct (~line 371): gains field `pending_drop: PendingDrop`.
- New items directly below `DragState`: `pub(crate) struct PendingDrop(bool)` with `mark()`/`take()` methods and doc comments, and `fn pending_drop_submit_position(drag: &DragState) -> Point<Pixels>` returning the last `Motion`-tracked position, not `Enter`'s entry coordinate.
- `DragState` initializer (~line 973): `pending_drop: PendingDrop::default(),`.
- Data-device `Leave` handler and offer-teardown paths (~lines 2650–2710): call `…drag.pending_drop.take();` so a leave cancels a pending drop.
- `Enter`'s foreground continuation (~line 2724): after paths resolve, `if state.drag.pending_drop.take() { … }` finishes the transfer itself — an `Entered` then a `Submit` at `pending_drop_submit_position(&state.drag)`.
- `Drop` handler (~line 2790): when `DragState::window` is still unset, `state.drag.pending_drop.mark();` instead of bailing out.
- `mod tests` (~line 3002): regression tests including `a_pending_drop_submits_at_the_latest_tracked_position_not_the_entry_point` and a take-resets-flag assertion (~line 3142). Carry the whole module over.

Expected after resolution: `grep -c "<<<<<<<" …` prints `0`, and `grep -c "PendingDrop" rust/vendor/gpui_linux/src/linux/wayland/client.rs` prints a number ≥ 20 (22 in the 0.3.6 fork).

- [ ] **Step 5: Update the vendored manifest to 0.3.8**

In `rust/vendor/gpui_linux/Cargo.toml`, set `version = "0.3.8"` (line ~16). Then re-mirror any dependency-version fields the published 0.3.8 manifest changed:

```bash
cd /tmp/bezel-bump
tar xzOf v038.crate bezel-gpui-linux-0.3.8+zed.82aeef/Cargo.toml > manifest-038.toml
PRISTINE=~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/bezel-gpui-linux-0.3.6+zed.d9ad6a
diff "$PRISTINE/Cargo.toml.orig" manifest-038.toml || diff "$PRISTINE/Cargo.toml" manifest-038.toml
```

For every dependency whose version requirement changed between the two published manifests, update the same dependency in `rust/vendor/gpui_linux/Cargo.toml` to the 0.3.8 value, preserving the vendored manifest's existing style (workspace-style deps written out as literal registry deps, per the comment at the top of that file). **Do not** pin any `bezel-zed-*` dependency to an exact `=` version — the README documents that this silently disables the patch.

Expected: vendored manifest at `version = "0.3.8"`, dependency requirements matching the published 0.3.8 manifest.

- [ ] **Step 6: Run the standalone tests (regenerates the standalone lock)**

```bash
cd "$(git rev-parse --show-toplevel)"
cargo test --manifest-path rust/vendor/gpui_linux/Cargo.toml 2>&1 | tail -5
```

Expected: compiles and `test result: ok`, including the `pending_drop` regression tests. Compile errors here mean a mis-merged hunk in Step 4 — fix `client.rs`, not the tests. `Cargo.lock` beside the manifest updates itself; that is correct and gets re-synced against the workspace lock in Task 4.

- [ ] **Step 7: Update `rust/vendor/README.md` version references**

Replace every `0.3.6` that refers to the vendored release with `0.3.8` (the pin description, the "release `0.3.6`" header line, the lock-honesty section's exception list, and the `0.3.7` as-of-this-writing note if the resolved build-dep chain changed). Re-read the edited README once to confirm the prose still tells the truth about what Step 5 produced.

Expected: `grep -n "0\.3\.6" rust/vendor/README.md` returns only lines (if any) that genuinely still refer to the historical 0.3.6 fix origin.

---

### Task 3: Move the workspace pins to 0.3.8

**Files:**
- Modify: `rust/Cargo.toml:61` (`gpui` pin) and `rust/Cargo.toml:64` (`gpui_platform` pin)
- Modify: `rust/Cargo.lock` (via cargo, not by hand)

**Interfaces:**
- Consumes: Task 2's vendored crate at `version = "0.3.8"`.
- Produces: a lockfile where `bezel-gpui` and `bezel-gpui-platform` resolve to `0.3.8+zed.82aeef` and `bezel-gpui-linux` resolves to the **path source** `rust/vendor/gpui_linux`.

- [ ] **Step 1: Edit the two pins**

In `rust/Cargo.toml` change:

```toml
gpui = { package = "bezel-gpui", version = "=0.3.6", default-features = false }
```
to
```toml
gpui = { package = "bezel-gpui", version = "=0.3.8", default-features = false }
```
and
```toml
gpui_platform = { package = "bezel-gpui-platform", version = "=0.3.6", default-features = false, features = ["font-kit"] }
```
to
```toml
gpui_platform = { package = "bezel-gpui-platform", version = "=0.3.8", default-features = false, features = ["font-kit"] }
```

`bezel = { version = "=0.1.3", … }` stays untouched in this task — bezel 0.1.3's `bezel-gpui ^0.3.6` requirement is satisfied by 0.3.8 (verified against the published manifest).

- [ ] **Step 2: Re-resolve and check the patch is actually used**

```bash
cd rust
cargo tree --target x86_64-unknown-linux-gnu -i bezel-gpui-linux 2>&1 | head -5
```

Expected: the first line names `bezel-gpui-linux v0.3.8` with a path source pointing at `vendor/gpui_linux`, and **no** `patch … was not used in the crate graph` warning anywhere in the output. If the warning appears, the vendored `version` field (Task 2 Step 5) does not satisfy the graph — stop and fix that before continuing; do not work around it by loosening the pins.

- [ ] **Step 3: Confirm resolution of the facade family**

```bash
cd rust
cargo tree -i bezel-gpui 2>&1 | head -3
```

Expected: `bezel-gpui v0.3.8+zed.82aeef`, with `bezel` 0.1.3 among its dependents.

---

### Task 4: Fix compile fallout, verify against baselines, commit stage A

**Files:**
- Modify: whatever `cargo build` reports in `rust/crates/sirio_theme`, `rust/crates/sirio_ui`, `rust/crates/sirio_terminal`, `rust/crates/sirio` (gpui API drift 0.3.6→0.3.8 only)
- Modify: `rust/vendor/gpui_linux/Cargo.lock` (re-sync, via cargo)

**Interfaces:**
- Consumes: Task 3's resolved graph.
- Produces: all four gpui-dependent crates building and testing at their baseline failure lists; the stage A commit.

- [ ] **Step 1: Build leaves first, fix fallout mechanically**

```bash
cd rust
cargo build -p sirio_theme && cargo build -p sirio_ui && cargo build -p sirio_terminal && cargo build -p sirio
```

Fix each error where the compiler points. Rules for these fixes: adapt call sites to renamed/re-signed gpui APIs only; no behavioral rewrites, no refactors, no "improvements" to adjacent code. If an API disappeared with no obvious replacement, search the 0.3.8 source (extracted at `/tmp/bgl-check/bezel-gpui-0.3.8+zed.82aeef/` or re-download as in Task 2 Step 2 with crate name `bezel-gpui`) for the successor before inventing anything. If a change would alter behavior (not just names/signatures), stop and report instead of guessing.

Expected: all four `cargo build -p` invocations succeed.

- [ ] **Step 2: Run per-crate tests and diff failure lists against baseline**

```bash
cd rust
for c in sirio_theme sirio_ui sirio_terminal sirio; do
  cargo test -p "$c" 2>&1 | tee /tmp/bezel-bump/after-a-$c.txt | tail -3
  grep -E "^test .* FAILED" /tmp/bezel-bump/after-a-$c.txt | sort > /tmp/bezel-bump/after-a-$c.failed
  diff /tmp/bezel-bump/baseline-$c.failed /tmp/bezel-bump/after-a-$c.failed && echo "$c: no new failures"
done
```

Expected: `no new failures` for every crate. Any test failing now that was not in the baseline list is a stage A regression — fix it before proceeding (re-run the single test with `cargo test -p <crate> <test_name>`).

- [ ] **Step 3: Re-sync the vendored standalone lock with the workspace lock**

For every package the two locks share, versions must agree (exception: the gpui-family trio itself, now pinned 0.3.8). Find drift and fix it with `--precise`, never by hand:

```bash
cd "$(git rev-parse --show-toplevel)"
python3 - <<'EOF'
import re
def versions(path):
    txt = open(path).read()
    return dict(re.findall(r'name = "([^"]+)"\nversion = "([^"]+)"', txt))
ws, vd = versions("rust/Cargo.lock"), versions("rust/vendor/gpui_linux/Cargo.lock")
for name in sorted(set(ws) & set(vd)):
    if ws[name] != vd[name]:
        print(f"{name}: workspace={ws[name]} vendored={vd[name]}")
EOF
```

For each printed package: `cargo update --manifest-path rust/vendor/gpui_linux/Cargo.toml -p <package> --precise <workspace-version>`. Re-run the script until it prints nothing, then re-run the standalone tests once more:

```bash
cargo test --manifest-path rust/vendor/gpui_linux/Cargo.toml 2>&1 | tail -3
```

Expected: no drift reported, tests still `ok`.

- [ ] **Step 4: Commit stage A**

```bash
cd "$(git rev-parse --show-toplevel)"
git add rust/Cargo.toml rust/Cargo.lock rust/vendor/gpui_linux rust/vendor/README.md rust/crates
git status --short   # confirm AGENTS.md, CLAUDE.md, sidebar.rs pre-existing edits are NOT staged; unstage anything unrelated
git commit -m "chore: bump bezel-gpui family to 0.3.8, re-port xdnd pending-drop fix"
```

Note: `git add rust/crates` stages only fallout fixes from Step 1; if `rust/crates/sirio_ui/src/sidebar.rs` carries the pre-existing unrelated modification, stage files individually instead (`git add <each fallout file>`), leaving `sidebar.rs`'s unrelated hunks out unless Step 1 touched that file — in that case stop and report rather than mixing changes.

Expected: one commit containing only stage A changes.

---

### Task 5: Bump bezel to 0.1.4

**Files:**
- Modify: `rust/Cargo.toml:69` (`bezel` pin), `rust/Cargo.lock` (via cargo)
- Modify (only if the compiler demands it): `rust/crates/sirio_theme/src/*`, the six bezel-based `sirio_ui` files (`controls.rs`, `loading.rs`, `modal.rs`, `project_identity.rs`, `settings.rs`, `titlebar.rs`)
- Modify (if tokens changed): `docs/THEME-PROVENANCE.md`

**Interfaces:**
- Consumes: stage A's graph (gpui family at 0.3.8; bezel 0.1.4 requires `bezel-gpui ^0.3.8`, now satisfied).
- Produces: bezel facade at 0.1.4 workspace-wide, tests at baseline, provenance doc telling the truth about 0.1.4 tokens.

- [ ] **Step 1: Edit the pin**

In `rust/Cargo.toml` change `bezel = { version = "=0.1.3", …` to `bezel = { version = "=0.1.4", …` (feature list unchanged: `geist-sans`, `geist-mono`, `geist-weights`).

- [ ] **Step 2: Build the two bezel consumers**

```bash
cd rust
cargo build -p sirio_theme && cargo build -p sirio_ui
```

Expected: near-zero fallout — every symbol Sirio imports was verified to survive 0.1.4 (`Theme`, `Theme::branded`, `Theme::dark/light`, `Brand`, `Tint`, `Appearance`, `AppearanceMode`, `set_current_appearance`, `lock_appearance`, `current_appearance`, `INK_FILL_SCALE`, `INK_HAIRLINE_SCALE`, `motion::Painter`, `ui::loaders`, `ui::popover`, `ui::widgets::Controls`). The APIs 0.1.4 removed (`set_glass_bevel`, `set_glass_magnify`, `glass_clear`, `glass_bevel`, `glass_magnify`, `glass_dispersion`) have zero call sites in Sirio (verified by literal search). If fallout appears anyway, apply the same mechanical-only rules as Task 4 Step 1.

- [ ] **Step 3: Build and test the rest**

```bash
cd rust
cargo build -p sirio && cargo test -p sirio_theme 2>&1 | tee /tmp/bezel-bump/after-b-sirio_theme.txt | tail -3
cargo test -p sirio_ui 2>&1 | tee /tmp/bezel-bump/after-b-sirio_ui.txt | tail -3
cargo test -p sirio 2>&1 | tee /tmp/bezel-bump/after-b-sirio.txt | tail -3
for c in sirio_theme sirio_ui sirio; do
  grep -E "^test .* FAILED" /tmp/bezel-bump/after-b-$c.txt | sort > /tmp/bezel-bump/after-b-$c.failed
  diff /tmp/bezel-bump/baseline-$c.failed /tmp/bezel-bump/after-b-$c.failed && echo "$c: no new failures"
done
```

Expected: `no new failures` for all three. **Caveat:** `sirio_theme`'s palette tests (`dark_palette_comes_from_bezel`, `light_palette_comes_from_bezel`, `neutral_reproduces_the_palette_shipped_before_base_colours`, `body_text_is_softened_off_bezels_full_contrast`, `washes_follow_bezels_two_rules`, `radii_are_ratios_of_bezel_base_radius`) assert against bezel's shipped values — 0.1.4 changed `palettes.rs`, so some may now fail *legitimately*. For those specific tests, a failure means the assertion's expected values must be updated to 0.1.4's numbers (the tests' whole purpose is to pin Sirio to bezel's palette; bezel moved, the pin follows). Update the expected values, never the mechanism. Any other new failure is a regression to fix.

- [ ] **Step 4: Update `docs/THEME-PROVENANCE.md`**

Read the document. It records which values are bezel's, which are derived, and the three tokens that stay Sirio's. Check each recorded bezel-sourced value against 0.1.4 (the extracted source at `/tmp/bgl-check/bezel-theme-0.1.4/` is the reference — `src/theme/palettes.rs`, `src/theme/layout.rs`, `src/theme/mod.rs`). Update any number or name 0.1.4 changed, and add a line noting the 0.1.4 surface model (`SurfaceStyle`/`SurfaceSpec` unifying frost and glass) if the document describes glass at all.

Expected: the document describes bezel 0.1.4, not 0.1.3.

---

### Task 6: Visual review and stage B commit

**Files:**
- None new — this task gates the stage B commit on the user's eyes.

**Interfaces:**
- Consumes: Task 5's build.
- Produces: the stage B commit; sub-project 1 complete.

- [ ] **Step 1: Launch the dev build**

```bash
Scripts/build-dev.sh
```

Expected: builds, kills any running instance, relaunches the app.

- [ ] **Step 2: User visual review (mandatory gate — do not proceed without it)**

Ask the user to review, specifically: titlebar, modals, and any frost/glass panel, in both light and dark appearance (the 0.1.4 "measured surfaces" model changes how those paint). The gallery/0.1.4 look is the accepted outcome by design decision — the question for the user is "acceptable?", not "identical to before?". Wait for an explicit yes. If the user rejects something, record what and stop — appearance fixes are a design conversation, not an implementation detail.

- [ ] **Step 3: Commit stage B**

```bash
cd "$(git rev-parse --show-toplevel)"
git add rust/Cargo.toml rust/Cargo.lock docs/THEME-PROVENANCE.md
git add rust/crates/sirio_theme rust/crates/sirio_ui   # only if Task 5 touched them; stage individual files, keep unrelated sidebar.rs hunks out
git status --short   # confirm nothing unrelated is staged
git commit -m "feat: adopt bezel 0.1.4 unified frost/glass surfaces"
```

Expected: one commit containing only stage B changes.

- [ ] **Step 4: Record the deferred Linux check**

The live XDND reproduction (`Scripts/wayland-drive.sh xdnd <x1> <y1> <x2> <y2> <file…> --delay-ms 400` against a Terminal pane) cannot run on macOS. Per the spec it is deferred, not skipped: tell the user in the final report that the re-ported fix is covered by the vendored `mod tests` regression tests only, and that the live reproduction must run on the next Linux session before the fork is considered fully re-verified.

Sub-project 1 done; sub-projects 2–3 (generic widgets, identity patterns) each get their own spec before any further work.
