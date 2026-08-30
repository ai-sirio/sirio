# Bezel release and API verification (Task 1)

Verifies the phase-one assumption in `docs/superpowers/plans/2026-08-30-bezel-loading.md`: that the four pinned Bezel packages resolve at their exact versions, and records the published loading-API surface Task 5 codes against instead of the gallery source.

Probe crate: `cargo new --lib bezel-probe` in `/tmp`, `cargo add` each package with `--no-default-features`, then `cargo doc --no-deps -p bezel`. Removed after this note was written; not part of the workspace.

## Resolved versions

All four resolve at their exact pin. `cargo add ... @=X.Y.Z` succeeded for each with no downgrade/yank warnings against the requested version.

| Requested (pin) | Resolved (`Cargo.lock`) |
|---|---|
| `bezel = "=0.1.3"` | `bezel 0.1.3` |
| `bezel-gpui = "=0.3.6"` | `bezel-gpui 0.3.6+zed.d9ad6a` |
| `bezel-gpui-platform = "=0.3.6"` (feature `font-kit`) | `bezel-gpui-platform 0.3.6+zed.d9ad6a` |
| `bezel-gpui-linux = "=0.3.6"` | `bezel-gpui-linux 0.3.6+zed.d9ad6a` |

The `+zed.d9ad6a` is Cargo build metadata, which semver ignores for the `=0.3.6` match — this is the exact requested version, not a substitute. The plan's `rust/Cargo.toml` pin text (`version = "=0.3.6"`, no build-metadata suffix) resolves correctly against this.

`cargo doc --no-deps -p bezel` completed clean (one unrelated future-incompatibility warning from a transitive dep, `block v0.1.6`, nothing bezel-owned).

## API surface (published, not gallery)

`bezel` (the facade crate, `bezel-0.1.3/src/lib.rs`) re-exports each layer as a peer namespace: `pub use agent; pub use motion; pub use theme; pub use ui;`. So `bezel::ui` is crate `bezel-ui`, `bezel::motion` is crate `bezel-motion`, `bezel::theme` is crate `bezel-theme`, `bezel::agent` is crate `bezel-agent`. Paths below are given as `bezel::<mod>::...`.

### `loaders::orb`

Matches the gallery form almost exactly, published at `bezel::ui::loaders::orb` (`bezel-ui-0.1.3/src/loaders.rs`):

```rust
pub fn orb(
    shape: Orb,
    key: impl Into<SharedString>,
    size_px: f32,
    theme: &Theme,
    painter: Painter,
    cx: &mut App,
) -> impl IntoElement
```

`Orb` is public and lives in the same module (`bezel::ui::loaders::Orb`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orb {
    Cluster,
    Ring,
    Converge,
    Bloom,
}
```

**`Orb::Cluster` is public** — yes, the whole enum and all four variants are public with no `#[non_exhaustive]`.

Only difference from the gallery signature: the id parameter is named `key: impl Into<SharedString>` rather than `id`; same position, same role (per-instance animation-state scoping).

**Distinct from this:** `bezel::agent::orbs::orb::Orb` (`bezel-agent-0.1.3/src/orbs/orb.rs`) is a *different*, unrelated type — a stateful gpui `Render` component (`Orb::new().state(OrbState::Working).size(OrbSize::Avatar)`, 12-variant `OrbState`, its own animation engine and clock). It is not the gallery's `loaders::orb` and Task 5 should not confuse the two; they share only the name "Orb" and both live under the `bezel` facade.

### `theme.progress_bar(fraction)`

Not on `bezel-theme` — published on a `bezel-ui` widget trait, `Controls`, implemented for `Theme`:

```rust
// bezel-ui-0.1.3/src/widgets/controls.rs
pub trait Controls: ThemeExt {
    fn progress_bar(&self, fraction: f32) -> Div { ... } // clamps fraction to 0..=1
    ...
}
impl Controls for Theme {}
```

Re-exported at `bezel::ui::widgets::Controls` (`bezel-ui-0.1.3/src/widgets/mod.rs`: `pub use controls::{Controls, ...};`). Usage needs the trait in scope: `use bezel::ui::widgets::Controls;` then `theme.progress_bar(fraction)` — matches the gallery call shape once imported.

### `popover::redacted_rows`

Matches the gallery form exactly, published at `bezel::ui::popover::redacted_rows` (`bezel-ui-0.1.3/src/popover.rs:1143`):

```rust
pub fn redacted_rows(
    id: &'static str,
    theme: &Theme,
    count: usize,
    painter: Painter,
    cx: &mut gpui::App,
) -> AnyElement
```

(Parameters named `_id`/`_theme` in the body since they're unused there, but the public signature keeps `id`/`theme` — the underscore is a body-local binding choice, not part of the signature.)

### `loaders::mini_gradient_spinner`

Matches the gallery form exactly, published at `bezel::ui::loaders::mini_gradient_spinner` (same file as `orb`):

```rust
pub fn mini_gradient_spinner(
    key: impl Into<SharedString>,
    cell_px: f32,
    painter: Painter,
    cx: &mut App,
) -> impl IntoElement
```

### Shared clock: `motion::PulseClock`

**Not public.** In `bezel-motion-0.1.3/src/lib.rs`:

```rust
#[derive(Default)]
struct PulseClock { ... }       // no `pub` — private to the bezel-motion crate
impl Global for PulseClock {}
```

There is no `bezel::motion::PulseClock` path at all; referencing one is a compile error. The clock is entirely internal, reached only through `cx.default_global::<PulseClock>()` inside `bezel-motion`'s own functions.

**Reduced motion is exposed, but on the public entry point, not on the clock.** The public function every loader in this note routes through is:

```rust
/// Current phase [0,1) of a repeating spec, plus a lease that keeps the
/// calling view re-rendering ... Reduced motion returns a static 0 and
/// schedules nothing.
pub fn pulse_delta(spec: &MotionSpec, painter: Painter, cx: &mut App) -> f32 {
    if cx.reduce_motion() {
        return 0.0;
    }
    ...
}
```

So: no queryable reduced-motion flag or method exists on a public clock type — the freeze-at-phase-0 behavior is baked directly into `pulse_delta`, which reads gpui's own `cx.reduce_motion()` and short-circuits to `0.0` before touching the (private) clock at all. Every loader this note covers (`orb`, `mini_gradient_spinner`, `redacted_rows`, plus `pulse_loader`/`gradient_spinner`) calls `motion::pulse_delta` for its phase, so **all of them already freeze at phase 0 under reduced motion for free** — Task 5's adapter does not need to implement or re-check this itself, only needs to keep routing through `pulse_delta` (or `App::reduce_motion()` directly, same source of truth) rather than rolling its own clock.

## Verdict

Phase-one gate met: all four packages resolve at their exact pinned versions, no substitutions, no fetch/resolution failures. Task 5 can code against the signatures recorded above.
