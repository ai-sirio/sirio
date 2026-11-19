# Upstream Ely, vendored verbatim

- Source: https://github.com/ELY-Open-Source/Ely-GPUI-Components
- Rev: `8d7723ec` (never moved)
- Crates: `ely-core`, `ely-chat`, `ely-agent`, `ely-editor`, `ely-git`, `ely-documents`,
  `ely-collab`, `ely-charts`, under `crates/`.
- File set taken from each crate: `Cargo.toml`, `src/`, and `icons/`, `assets/` and `LICENSE` where
  present. Never `AGENTS.md` or `examples/`. `LICENSE` at this directory's root is upstream's.
- Nothing in Sirio depends on these crates yet.
- Check: `artifacts/ely-sync/drivers/check-vendor.sh` extracts each crate at the rev, diffs it
  against this directory and fails on any differing path not named below. It prints
  `VENDOR CHECK OK`.

## Changed paths

Every path below is a path the check accepts as different from upstream. Anything else differing
is a failure.

### Manifests (all eight)

`crates/<crate>/Cargo.toml` for each crate:

- `x.workspace = true` becomes upstream's workspace value, literally (package fields, third-party
  dependencies). `[lints]` tables are dropped. `publish = false` is added.
- `gpui` and `gpui_platform` stay `workspace = true`, now Sirio's workspace entries. Each crate gains
  a `[target.'cfg(target_os = "linux")'.dependencies]` section enabling gpui's `wayland` and `x11`,
  the same section the old `ely-gpui-component` carries.
- `ely-core`: `default = []` (was `["fonts"]`), and the `five_icons` `[[example]]` is dropped with
  `examples/`.

- `crates/ely-core/Cargo.toml`
- `crates/ely-chat/Cargo.toml`
- `crates/ely-agent/Cargo.toml`
- `crates/ely-editor/Cargo.toml`
- `crates/ely-git/Cargo.toml`
- `crates/ely-documents/Cargo.toml`
- `crates/ely-collab/Cargo.toml`
- `crates/ely-charts/Cargo.toml`

### Ignored tests

Two `ely-documents` tests of upstream's `e71b644b` need upstream's patched pulldown-cmark. Each
gains `#[ignore = "needs upstream's patched pulldown-cmark; step 2"]` and nothing else.

- `crates/ely-documents/src/markdown/tex/tests.rs`
- `crates/ely-documents/src/tree/tests/inline.rs`
