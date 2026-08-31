# Bezel gallery adoption — design

Date: 2026-08-31
Status: approved (sub-project 1 detailed; sub-projects 2–3 to be specced when reached)

## Goal

Adopt bezel as the source of truth for Sirio's UI, in both code and appearance
("full gallery adoption"): wherever bezel — or a pattern demonstrated in its
gallery (`apps/gallery/src` in [crabtalk/bezel](https://github.com/crabtalk/bezel)) —
covers a Sirio component, Sirio uses the bezel primitive and takes on the
gallery's look. Where the gallery differs from Sirio's current appearance, the
gallery wins; no effort is spent reconstructing the old look.

Key facts established during design:

- The gallery's pattern files (`diff`, `transcript`, `editor`, `terminal`,
  `orbs`, `avatar`, `document`, `syntax`, `agent`) are **copy-me example
  compositions**, not exported components. Their own doc comments say so
  explicitly ("Copy this file", "this produced no library code"). Adoption
  means porting those compositions into Sirio on top of bezel primitives, not
  swapping in library types.
- The primitives those patterns need (`scroll::follow`/`FollowState`,
  `widgets::Takeover`, `widgets::step_row*`, `popover`, `tree`, `table`,
  `list`, `icons`) already exist in bezel 0.1.3, but the gallery on `main`
  tracks bezel 0.1.4 and its unified surface model, plus an unpublished
  `markdown` crate (Sirio keeps `sirio_markdown` for now).
- bezel 0.1.4 requires `bezel-gpui ^0.3.8`; the workspace pins the gpui family
  at `=0.3.6`, so adopting 0.1.4 is a workspace-wide gpui bump, not just a UI
  dependency chore.

## Current state

`sirio_ui` files already built on bezel (6): `controls`, `loading`, `modal`,
`project_identity`, `settings`, `titlebar`. Hand-rolled (~17, the largest):
`chat` (616K), `sidebar` (303K), `settings` (partially), `changes`, `browser`,
`file_view`, `tab_bar`, `status_bar`, `editor`, `composer`, `icons`, `orbit`,
`project_forms`, `right_panel/`.

## Decomposition — three sub-projects, in order

1. **Foundation bump** — bezel `=0.1.3` → `=0.1.4`, gpui family `=0.3.6` →
   `=0.3.8`, vendored XDND fix re-ported. No component rewrites. Detailed
   below.
2. **Generic widgets** — adopt `input`, `combobox`, `tooltip`, `popover`,
   `table`, `list`, `scroll`, `loaders`, `menubar`, `stats`, `hover_card`
   wherever `sirio_ui` hand-rolls an equivalent (forms, settings, status bar,
   tab bar, …). Own spec when reached.
3. **Identity patterns** — port the gallery compositions: `transcript` → chat,
   `diff` → changes, `tree` → sidebar, `editor`/`document` → editor/file_view,
   `orbs` → orbit, `avatar` → project_identity. Own spec when reached.

Each sub-project gets its own spec → plan → implementation cycle. This
document is the umbrella decision record plus the full design for sub-project
1.

## Sub-project 1: foundation bump

Chosen approach: **two stages on one branch, one commit per stage**, so every
regression has a single cause — stage A can only introduce gpui
runtime/API regressions, stage B can only introduce bezel appearance/API
changes.

> **Revision (2026-08-31, during execution):** stage separation proved
> impossible — bezel-ui 0.1.3 declares `bezel-gpui ^0.3.6` but does not
> compile against 0.3.8 (`GlassEffect` gained fields `edge`, `edge_aa`,
> `edge_width` and three more; bezel-ui 0.1.3's `material.rs:245`
> initializer misses them). An intermediate gpui-0.3.8 + bezel-0.1.3 state
> does not build and must never be committed. Stages A and B collapse into
> **one commit**, gated by the stage-B visual review. Work order inside the
> stages is unchanged. Alternatives considered and rejected: a single combined bump
(conflates compile fallout with visual changes, hard to bisect) and dropping
the vendored fork (reintroduces the fixed XDND slow-provider race,
F-CORE-FILE-03A).

### Stage A — gpui family `=0.3.6` → `=0.3.8`

bezel stays at `=0.1.3` (its `bezel-gpui ^0.3.6` requirement is satisfied by
0.3.8, verified against the published manifest). The `+zed.82aeef` build
metadata on the 0.3.8 release is ignored by Cargo version comparison, so the
pins are written `=0.3.8`.

1. `rust/Cargo.toml`: bump the `gpui` and `gpui_platform` pins to `=0.3.8`.
2. `rust/vendor/gpui_linux/`: replace the tree with the published
   `bezel-gpui-linux` 0.3.8 source, then re-apply the `PendingDrop` mechanism
   to `src/linux/wayland/client.rs` by hand — upstream 0.3.8 does **not**
   contain the fix (verified: no `PendingDrop`/`pending_drop` in the published
   source). Drift between the vendored 0.3.6 `client.rs` and upstream 0.3.8 is
   ~162 lines, so this is a re-diff, not a rewrite. The `tests` module's
   regression coverage moves along with it. The manifest keeps the same
   field-for-field mirroring rules its own comment documents.
3. Re-sync `vendor/gpui_linux/Cargo.lock` with `rust/Cargo.lock` via
   `cargo update --precise` per drifted package — never by hand-editing.
4. Guard against the documented patch trap: after the bump,
   `cargo tree --target x86_64-unknown-linux-gnu -i bezel-gpui-linux` must
   resolve to the path source with no "patch … was not used" warning.
5. Fix compile fallout per crate, leaves first (`cargo build -p <crate>`).
   `sirio_terminal` needs Zig exactly 0.15.2 on PATH.
6. Update version references in `rust/vendor/README.md` (0.3.6 → 0.3.8).

### Stage B — bezel `=0.1.3` → `=0.1.4`

1. `rust/Cargo.toml`: bump the `bezel` pin to `=0.1.4`.
2. Fix API fallout in the direct consumers: `sirio_theme` (re-exports
   `bezel::theme`; its `Deref` to `ThemeColors` stays as is) and the six
   bezel-based `sirio_ui` files. The headline 0.1.4 change is the unified
   frost/glass surface model (`SurfaceSpec`); every Sirio use of frost or
   glass surfaces gets its signatures checked and its appearance reviewed.
3. Update `docs/THEME-PROVENANCE.md` if 0.1.4 changes any token or derived
   measurement — the pin is on appearance as much as API.
4. **Visual review (mandatory)**: launch via `Scripts/build-dev.sh`; the user
   confirms the surfaces (titlebar, modals, frost/glass panels) are acceptable
   under the new model. 0.1.4 is the new visual truth; no reconstruction of
   the 0.1.3 look.
5. No component rewrites in this stage — those are sub-projects 2 and 3.

### Verification

- Per-crate `cargo build -p` / `cargo test -p` during iteration; workspace
  gates (`Scripts/ci.sh`, `Scripts/ci-linux.sh`) run **only on the user's
  explicit request**.
- Standalone vendored-crate tests:
  `cargo test --manifest-path rust/vendor/gpui_linux/Cargo.toml`.
- Test failures are compared against the known-red baseline on `main` as
  *lists, not counts* (7 pre-existing failures plus load-sensitive flakes).
- The live XDND reproduction (`Scripts/wayland-drive.sh xdnd … --delay-ms
  400`) is Linux-only; on macOS the vendored `tests` module is the available
  regression coverage, and the live check is recorded as deferred to the next
  Linux run rather than silently skipped.
- Two commits (stage A, stage B) so regressions bisect to one cause.

### Risks

- **Vendored fork re-port**: the standing fork is the one place needing hand
  merging. Mitigated by the small measured drift and the fork's own tests.
- **Patch silently unused**: cargo falls back to the registry crate with only
  a warning if the patch candidate becomes infeasible. Mitigated by the
  `cargo tree -i` check above.
- **Surface model appearance shift**: expected and accepted by decision (the
  gallery look wins); gated by the mandatory visual review, not by code.

## Out of scope

- Any `sirio_ui` component rewrite (sub-projects 2–3).
- The gallery's unpublished `markdown` crate; chat rendering stays on
  `sirio_markdown` until it ships.
- Release/tag work; the macOS `CI OK` tag gate is untouched.
