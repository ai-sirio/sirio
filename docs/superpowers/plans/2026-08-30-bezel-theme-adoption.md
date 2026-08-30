# Bezel Theme Adoption Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `bezel::theme` the single source of truth for Sirio's colours, typography, spacing and radii, reducing `sirio_theme` to what bezel has no concept of.

**Architecture:** `sirio_theme::Theme` already implements `Deref<Target = ThemeColors>`, so call sites write `theme.title` rather than `theme.colors.title`. The migration renames Sirio's tokens to bezel's names while values stay frozen (Phase 1), then repoints that `Deref` at `bezel::theme::Theme` so values change while no call site is touched (Phase 2), then removes what is now redundant (Phase 3). Each phase has a mechanical gate that the other phase cannot pass, which is why they never merge.

**Tech Stack:** Rust 2024 edition, gpui (`bezel-gpui` 0.3.6), `bezel` 0.1.3, standard `#[test]`, `Scripts/ci.sh`.

**Spec:** `docs/superpowers/specs/2026-08-30-bezel-theme-adoption-design.md`

## Global Constraints

- `bezel` is pinned `=0.1.3`. Do not bump it. After this work the pin protects appearance, not just API — treat any future bump as a visual change to review.
- Anything building `sirio_terminal` needs **Zig exactly 0.15.2** on PATH. This machine has 0.16.0 only, so `Scripts/ci.sh` fails fast. Phases 1 and 2 do not need it: `sirio_ui` has no `sirio_terminal` dependency, so use `cargo test -p sirio_theme -p sirio_ui`. Phase 3 needs Zig fixed first.
- Commit messages follow Conventional Commits, lower-case imperative subject.
- Layout measurements do not change (spec decision T2). `bottom_bar_height` stays 40px; only its *derivation* changes from a literal to a ratio.
- Never touch user-global config. Nothing in this plan does, but the rule stands.
- The invariant tests listed in the spec's Testing section must be green at the end of every task. They cite no hex and are the value-independent oracle.

---

## File Structure

**Created:**

- `Scripts/gate-no-value-change.sh` — Phase 1 gate. Compares the multiset of numeric literals across the whole Rust tree between two revs.
- `Scripts/gate-theme-only.sh` — Phase 2 gate. Asserts no file outside `rust/crates/sirio_theme/` changed.
- `docs/THEME-PROVENANCE.md` — replaces the dead `docs/linux-rewrite/THEME-PROVENANCE.md` reference. Records where each value now comes from (spec risk R4).

**Modified:**

- `rust/crates/sirio_theme/src/lib.rs` — the bulk of the work. `ThemeColors` is renamed then removed; `Theme` gains a `bezel::theme::Theme` and repoints its `Deref`.
- `rust/crates/sirio_ui/src/*.rs` — ~704 colour call sites renamed; `loading.rs` loses its private bridge; `titlebar.rs` rebased off `cosmic`.
- `rust/crates/sirio/src/main.rs` — `register_fonts` deleted, the two `AppearanceMode` match blocks collapsed.
- `rust/crates/sirio_terminal/src/lib.rs` — 47 theme references renamed.
- `rust/Cargo.toml` — `bezel` gains the three geist features.

**Deleted:**

- `rust/crates/sirio_theme/src/cosmic/` (10 files, 1243 lines)
- `rust/assets/fonts/` (3 TTFs plus `OFL.txt`)

---

# Phase 1 — Rename. Values frozen.

### Task 1: The Phase 1 gate script

**Files:**
- Create: `Scripts/gate-no-value-change.sh`

**Interfaces:**
- Produces: `Scripts/gate-no-value-change.sh <base-ref>` — exits 0 when no numeric literal changed between `<base-ref>` and the working tree, 1 otherwise. Used as the closing step of every Phase 1 task.

- [ ] **Step 1: Write the script**

```bash
#!/usr/bin/env bash
# Phase 1 gate for the bezel theme adoption (see
# docs/superpowers/plans/2026-08-30-bezel-theme-adoption.md).
#
# A rename moves names, never numbers. This compares the multiset of numeric
# literals across the whole Rust tree at <base-ref> against the working tree:
# a pure rename leaves it identical. Comparing multisets rather than diff
# hunks means a moved or reordered line is not a false positive.
set -euo pipefail

BASE="${1:?usage: gate-no-value-change.sh <base-ref>}"
# No \b here: git grep uses POSIX ERE, which has no word-boundary escape --
# `\b` silently matches nothing and the gate would pass on everything. The
# boundaries are not needed anyway: the same pattern runs over both snapshots,
# so any partial match is partial identically on both sides.
PATTERN='0x[0-9A-Fa-f]+|[0-9]+\.[0-9]+f?'

before="$(mktemp)"; after="$(mktemp)"
trap 'rm -f "$before" "$after"' EXIT

git grep -hoE "$PATTERN" "$BASE" -- 'rust/crates/*.rs' | sort | uniq -c > "$before"
git grep -hoE "$PATTERN" -- 'rust/crates/*.rs' | sort | uniq -c > "$after"

if diff -u "$before" "$after" > /dev/null; then
    echo "GATE OK: no numeric literal changed since $BASE"
    exit 0
fi

echo "GATE FAILED: numeric literals moved since $BASE" >&2
echo "--- $BASE" >&2
diff -u "$before" "$after" >&2 || true
exit 1
```

- [ ] **Step 2: Make it executable and verify it passes on a clean tree**

Run:
```bash
chmod +x Scripts/gate-no-value-change.sh
Scripts/gate-no-value-change.sh HEAD
```
Expected: `GATE OK: no numeric literal changed since HEAD`

- [ ] **Step 3: Verify it fails when a number moves**

Run:
```bash
sed -i.bak 's/const VEIL_FAINT: f32 = 0.05;/const VEIL_FAINT: f32 = 0.06;/' \
    rust/crates/sirio_theme/src/lib.rs
Scripts/gate-no-value-change.sh HEAD; echo "exit=$?"
mv rust/crates/sirio_theme/src/lib.rs.bak rust/crates/sirio_theme/src/lib.rs
```
Expected: `GATE FAILED: numeric literals moved since HEAD` and `exit=1`.

A gate that cannot fail is not a gate — this step is what proves it works, and is the reason it is separate from Step 2.

- [ ] **Step 4: Commit**

```bash
git add Scripts/gate-no-value-change.sh
git commit -m "chore: add phase 1 gate for theme rename"
```

---

### Task 2: Settle the `frame_surface` review row

**Files:**
- Read: `rust/crates/sirio_theme/src/lib.rs:376-380`
- Read: `/Users/enzopiopalmisano/.cache/fx/bezel-gallery-analysis/crates/theme/src/paint.rs`
- Modify: `docs/superpowers/specs/2026-08-30-bezel-theme-adoption-design.md`

**Interfaces:**
- Consumes: nothing.
- Produces: a settled Table 1 row for `frame_surface`, which Task 5 renames.

This is the single row the spec marks **review**. It is decided by comparing values, not by argument.

- [ ] **Step 1: Read Sirio's current value**

Run:
```bash
sed -n '376,380p' rust/crates/sirio_theme/src/lib.rs
```
Expected: `frame_surface` is `softened(frame_fallback, 0.88)` in dark and `softened(frame_fallback, 0.82)` in light — i.e. the frame fallback at 88% / 82% opacity.

- [ ] **Step 2: Read bezel's `band`**

Run:
```bash
grep -n -B12 'pub fn band_for' \
    /Users/enzopiopalmisano/.cache/fx/bezel-gallery-analysis/crates/theme/src/paint.rs
```

- [ ] **Step 3: Record the decision in the spec**

If `band` is the translucent window-frame material, replace the `frame_surface` row's Note column with the resolved reasoning and drop the **review** marker. If it is not — if `band` turns out to mean a horizontal band rather than the frame material — mark `frame_surface` as retained in `SirioColors` instead, and add it to the Group C table alongside `terminal_surface`.

Whichever way it goes, write one sentence saying which value settled it. Do not leave the marker in place.

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/specs/2026-08-30-bezel-theme-adoption-design.md
git commit -m "docs: settle frame_surface mapping against bezel's band"
```

---

### Task 3: Un-alias `git_untracked` from `gauge`

**Files:**
- Modify: `rust/crates/sirio_theme/src/lib.rs` (the `ThemeColors { .. }` literal in `for_appearance`)
- Test: `rust/crates/sirio_theme/src/lib.rs` (its `mod tests`)

**Interfaces:**
- Consumes: nothing.
- Produces: `git_untracked` as a binding of its own rather than an alias of `gauge`. Phase 2 Task 13 moves its value; this task only separates it.

This is the only structural change in the whole migration. It is here rather than in Phase 2 because Phase 2's gate forbids structural changes, and C1 requires `git_untracked` to end up somewhere `gauge` does not.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `rust/crates/sirio_theme/src/lib.rs`:

```rust
#[test]
fn git_untracked_is_its_own_binding_not_the_gauge_blue() {
    // C1 moves untracked off the gauge binding to a neutral. Splitting the
    // alias is structural and lands in phase 1; the value moves in phase 2.
    // Until then both are the same colour, so this test asserts the *binding*
    // exists separately by checking the field is reachable without gauge.
    let dark = ThemeColors::for_appearance(Appearance::Dark);
    let light = ThemeColors::for_appearance(Appearance::Light);
    assert_eq!(dark.git_untracked, dark.gauge, "phase 1 keeps the value");
    assert_eq!(light.git_untracked, light.gauge, "phase 1 keeps the value");
}
```

- [ ] **Step 2: Run it to confirm it passes today**

Run: `cd rust && cargo test -p sirio_theme git_untracked_is_its_own_binding -- --nocapture`
Expected: PASS. This test documents the pre-split state; Task 13 inverts its assertions.

- [ ] **Step 3: Introduce the separate binding**

In `for_appearance`, immediately after the `gauge` binding, add:

```rust
// C1: untracked leaves the gauge blue for a neutral in phase 2. Bound
// separately here so that move is a value change, not a structural one.
let git_untracked = gauge;
```

and change the struct literal line `git_untracked: gauge,` to `git_untracked,`.

- [ ] **Step 4: Run the tests**

Run: `cd rust && cargo test -p sirio_theme`
Expected: PASS, including the new test and all provenance tests unchanged.

- [ ] **Step 5: Run the gate**

Run: `Scripts/gate-no-value-change.sh HEAD`
Expected: `GATE OK`

- [ ] **Step 6: Commit**

```bash
git add rust/crates/sirio_theme/src/lib.rs
git commit -m "refactor: bind git_untracked separately from gauge"
```

---

### Task 4: Rename the text ladder

**Files:**
- Modify: `rust/crates/sirio_theme/src/lib.rs`
- Modify: `rust/crates/sirio_ui/src/*.rs`, `rust/crates/sirio/src/main.rs`, `rust/crates/sirio_terminal/src/lib.rs`

**Interfaces:**
- Consumes: Task 1's gate script.
- Produces: the fields `text`, `text_muted`, `text_faint`, `text_dim` on `ThemeColors`. Tasks 5–8 assume these names exist.

Renames, from spec Table 1 and Table 2:

| Old field(s) | New field |
|---|---|
| `title`, `title_selected`, `tab_focus_accent`, `selection_ring`, `primary_text_color`, `caret`, `code_text` | `text` |
| `subtitle`, `panel_focus_ring` | `text_muted` |
| `meta` | `text_faint` |
| `text_ghost` | `text_dim` |

`title_selected`, `tab_focus_accent`, `selection_ring`, `primary_text_color`, `caret` and `code_text` are all aliases of the `text` binding, so they collapse into `text` and cease to exist as fields.

- [ ] **Step 1: Record the base rev**

Run:
```bash
git rev-parse HEAD > /tmp/phase1-base
cat /tmp/phase1-base
```

- [ ] **Step 2: Rename the struct fields and their bindings**

In `rust/crates/sirio_theme/src/lib.rs`: rename the `text_secondary` binding to `text_muted` and `text_tertiary` to `text_faint`; rename the `text_ghost` binding to `text_dim`. In the `ThemeColors` struct, delete the seven alias fields and keep a single `pub text: Rgba`, plus `text_muted`, `text_faint`, `text_dim`. Carry each deleted field's doc-comment content into the surviving field's doc where it says something the survivor's does not.

- [ ] **Step 3: Rename the call sites**

Run:
```bash
cd rust/crates
for pair in \
    "title_selected:text" "tab_focus_accent:text" "selection_ring:text" \
    "primary_text_color:text" "code_text:text" "title:text" "caret:text" \
    "subtitle:text_muted" "panel_focus_ring:text_muted" \
    "meta:text_faint" "text_ghost:text_dim"; do
    old="${pair%%:*}"; new="${pair##*:}"
    grep -rl "theme\.$old\b" sirio_ui/src sirio/src sirio_terminal/src \
        --include="*.rs" 2>/dev/null \
      | xargs -r sed -i '' "s/theme\.$old\b/theme.$new/g"
done
```

Order matters: `title_selected` must be rewritten before `title`, or `theme.title_selected` becomes `theme.text_selected`. The loop above is already in a safe order — do not reorder it.

- [ ] **Step 4: Build and fix the remainder**

Run: `cd rust && cargo build -p sirio_theme -p sirio_ui`
Expected: errors only where a field was reached other than through `theme.`, e.g. a destructuring pattern or a `colors.title` access. Fix each by hand; there should be a handful.

- [ ] **Step 5: Update the tests**

Rename the same fields inside `mod tests`. Assertion *values* must not change — only the field names they read.

- [ ] **Step 6: Run the tests**

Run: `cd rust && cargo test -p sirio_theme -p sirio_ui`
Expected: PASS, all provenance tests included.

- [ ] **Step 7: Run the gate**

Run: `Scripts/gate-no-value-change.sh "$(cat /tmp/phase1-base)"`
Expected: `GATE OK`

- [ ] **Step 8: Commit**

```bash
git add rust/crates
git commit -m "refactor: rename the text ladder to bezel names"
```

---

### Task 5: Rename the surface ladder

**Files:**
- Modify: `rust/crates/sirio_theme/src/lib.rs`
- Modify: `rust/crates/sirio_ui/src/*.rs`, `rust/crates/sirio/src/main.rs`, `rust/crates/sirio_terminal/src/lib.rs`

**Interfaces:**
- Consumes: Task 4's renamed fields.
- Produces: `bg`, `surface`, `surface_raised`, `input_bg`, `element_active`, `solid`, `on_solid`, `band`, `terminal_surface`.

| Old field(s) | New field |
|---|---|
| `frame_fallback`, `canvas` | `bg` |
| `panel_surface`, `background`, `chat_surface`, `chrome_tint`, `sidebar` | `surface` |
| `raised`, `composer`, `primary_pill_bg`, `card_fill` | `surface_raised` |
| `inset`, `filter_field_bg`, `code_inset_fill` | `input_bg` |
| `selected_fill`, `selection_fill`, `primary_action_bg` | `element_active` |
| `inverse` | `solid` |
| `on_inverse` | `on_solid` |
| `frame_surface` | `band`, or retained — per Task 2 |
| `terminal_surface` | unchanged; retained |

- [ ] **Step 1: Rename the struct fields and their bindings**

As Task 4 Step 2, for the table above. `terminal_surface` keeps its name and gains a doc line saying it is retained because bezel has no terminal-surface concept.

- [ ] **Step 2: Rename the call sites**

Run:
```bash
cd rust/crates
for pair in \
    "frame_fallback:bg" "canvas:bg" \
    "panel_surface:surface" "chat_surface:surface" "chrome_tint:surface" \
    "background:surface" "sidebar:surface" \
    "primary_pill_bg:surface_raised" "card_fill:surface_raised" \
    "composer:surface_raised" "raised:surface_raised" \
    "filter_field_bg:input_bg" "code_inset_fill:input_bg" "inset:input_bg" \
    "primary_action_bg:element_active" "selection_fill:element_active" \
    "selected_fill:element_active" \
    "inverse:solid" "on_inverse:on_solid"; do
    old="${pair%%:*}"; new="${pair##*:}"
    grep -rl "theme\.$old\b" sirio_ui/src sirio/src sirio_terminal/src \
        --include="*.rs" 2>/dev/null \
      | xargs -r sed -i '' "s/theme\.$old\b/theme.$new/g"
done
```

`on_inverse` must precede `inverse`, and `selection_fill` must precede `selected_fill`. The order above is safe.

Note `theme.sidebar` — check by hand that no unrelated field starts with `sidebar` other than `sidebar_border`, which Task 6 handles. `\b` protects it, but confirm with `grep -rn 'theme\.sidebar' rust/crates` before and after.

- [ ] **Step 3: Build and fix the remainder**

Run: `cd rust && cargo build -p sirio_theme -p sirio_ui`

- [ ] **Step 4: Update the tests**

Same rule: names change, numbers do not.

- [ ] **Step 5: Run the tests**

Run: `cd rust && cargo test -p sirio_theme -p sirio_ui`
Expected: PASS

- [ ] **Step 6: Run the gate**

Run: `Scripts/gate-no-value-change.sh "$(cat /tmp/phase1-base)"`
Expected: `GATE OK`

- [ ] **Step 7: Commit**

```bash
git add rust/crates
git commit -m "refactor: rename the surface ladder to bezel names"
```

---

### Task 6: Rename the veil ladder

**Files:**
- Modify: `rust/crates/sirio_theme/src/lib.rs`
- Modify: `rust/crates/sirio_ui/src/*.rs`, `rust/crates/sirio/src/main.rs`, `rust/crates/sirio_terminal/src/lib.rs`

**Interfaces:**
- Consumes: Task 5's renamed fields.
- Produces: `border`, `border_strong`, `element_hover`, `selection`, `code_wash`, and the two `wash`-derived fields.

| Old field(s) | New field |
|---|---|
| `border`, `hairline` | `border` |
| `border_strong`, `rail_task`, `rail_edit`, `rail_tool` | `border_strong` |
| `panel_border`, `tab_chip_underline`, `sidebar_border` | `border` |
| `row_hover` | `element_hover` |
| `overlay`, `chat_row_hover` | `overlay` (becomes `wash(0.05)` in Phase 2) |
| `overlay_strong` | `overlay_strong` (becomes `wash(0.12)` in Phase 2) |
| `selection` | `selection` |
| `code_wash`, `diff_hunk_background` | `code_wash` |
| `tree_guide` | `tree_guide` (retained; derived from `border` in Phase 2) |

`panel_border` is an opaque hex today while `border` is a veil, so these are two different values collapsing onto one name. **This is the one rename in Phase 1 that cannot preserve both values.** Keep them as two fields for now — rename `panel_border` to `border_opaque` — and collapse it in Phase 2 Task 13, where a value change is allowed. Renaming it to `border` here would move a number and the gate would catch it, which is the gate working correctly.

- [ ] **Step 1: Rename the struct fields and their bindings**

Per the table, with `panel_border` → `border_opaque` as noted.

- [ ] **Step 2: Rename the call sites**

Run:
```bash
cd rust/crates
for pair in \
    "hairline:border" \
    "rail_task:border_strong" "rail_edit:border_strong" "rail_tool:border_strong" \
    "tab_chip_underline:border_opaque" "sidebar_border:border_opaque" \
    "panel_border:border_opaque" \
    "chat_row_hover:overlay" "row_hover:element_hover" \
    "diff_hunk_background:code_wash"; do
    old="${pair%%:*}"; new="${pair##*:}"
    grep -rl "theme\.$old\b" sirio_ui/src sirio/src sirio_terminal/src \
        --include="*.rs" 2>/dev/null \
      | xargs -r sed -i '' "s/theme\.$old\b/theme.$new/g"
done
```

`chat_row_hover` must precede `row_hover`. The order above is safe.

- [ ] **Step 3: Build and fix the remainder**

Run: `cd rust && cargo build -p sirio_theme -p sirio_ui`

- [ ] **Step 4: Update the tests**

`the_veil_ladder_is_geometric` and `veil_backed_structural_washes_are_neutral` read these fields. Rename what they read; leave their assertions alone.

- [ ] **Step 5: Run the tests**

Run: `cd rust && cargo test -p sirio_theme -p sirio_ui`
Expected: PASS

- [ ] **Step 6: Run the gate**

Run: `Scripts/gate-no-value-change.sh "$(cat /tmp/phase1-base)"`
Expected: `GATE OK`

- [ ] **Step 7: Commit**

```bash
git add rust/crates
git commit -m "refactor: rename the veil ladder to bezel names"
```

---

### Task 7: Rename the status hues

**Files:**
- Modify: `rust/crates/sirio_theme/src/lib.rs`
- Modify: `rust/crates/sirio_ui/src/*.rs`, `rust/crates/sirio/src/main.rs`, `rust/crates/sirio_terminal/src/lib.rs`

**Interfaces:**
- Consumes: Task 6's renamed fields.
- Produces: `success`, `warning`, `danger`, `danger_muted`, `diff_add`, `diff_del`, `favorite`.

| Old field(s) | New field |
|---|---|
| `tab_done`, `git_staged` | `success` |
| `diff_addition` | `diff_add` |
| `tab_needs_input`, `git_modified`, `rail_question` | `warning` |
| `tab_error`, `git_conflict` | `danger` |
| `diff_deletion` | `diff_del` |
| `danger_soft` | `danger_muted` |
| `favorite` | `favorite` (retained — full-chroma warning, its own binding) |
| `diff_addition_background`, `diff_deletion_background` | unchanged for now; derived in Phase 2 |

- [ ] **Step 1: Rename the struct fields and their bindings**

- [ ] **Step 2: Rename the call sites**

Run:
```bash
cd rust/crates
for pair in \
    "tab_done:success" "git_staged:success" \
    "tab_needs_input:warning" "git_modified:warning" "rail_question:warning" \
    "tab_error:danger" "git_conflict:danger" \
    "diff_addition_background:diff_add_bg" "diff_addition:diff_add" \
    "diff_deletion_background:diff_del_bg" "diff_deletion:diff_del" \
    "danger_soft:danger_muted"; do
    old="${pair%%:*}"; new="${pair##*:}"
    grep -rl "theme\.$old\b" sirio_ui/src sirio/src sirio_terminal/src \
        --include="*.rs" 2>/dev/null \
      | xargs -r sed -i '' "s/theme\.$old\b/theme.$new/g"
done
```

Each `*_background` entry must precede its shorter sibling. The order above is safe.

- [ ] **Step 3: Build and fix the remainder**

Run: `cd rust && cargo build -p sirio_theme -p sirio_ui`

- [ ] **Step 4: Update the tests**

`the_state_hues_are_the_ones_the_swift_app_shipped` and `favorite_is_the_warning_hue_at_full_chroma` read these. Names only.

- [ ] **Step 5: Run the tests**

Run: `cd rust && cargo test -p sirio_theme -p sirio_ui`
Expected: PASS

- [ ] **Step 6: Run the gate**

Run: `Scripts/gate-no-value-change.sh "$(cat /tmp/phase1-base)"`
Expected: `GATE OK`

- [ ] **Step 7: Commit**

```bash
git add rust/crates
git commit -m "refactor: rename the status hues to bezel names"
```

---

### Task 8: Rename `accent` to `brand_coral`, and `gauge` to `accent`

**Files:**
- Modify: `rust/crates/sirio_theme/src/lib.rs`
- Modify: `rust/crates/sirio_ui/src/*.rs`, `rust/crates/sirio/src/main.rs`

**Interfaces:**
- Consumes: Task 7's renamed fields.
- Produces: `brand_coral` (Sirio's coral, retained) and `accent` (formerly `gauge`, becoming bezel's neutral accent in Phase 2). `file_link` follows `accent`.

The order is load-bearing: `accent` must vacate the name before `gauge` takes it, or the two briefly collide.

- [ ] **Step 1: Rename `accent` to `brand_coral` first**

Run:
```bash
cd rust/crates
grep -rl 'theme\.accent\b' sirio_ui/src sirio/src --include="*.rs" \
  | xargs -r sed -i '' 's/theme\.accent\b/theme.brand_coral/g'
```

Then rename the binding and the field in `sirio_theme/src/lib.rs`, keeping its whole doc-comment — it explains the two invariants that hold it.

- [ ] **Step 2: Build to confirm `accent` is now free**

Run: `cd rust && cargo build -p sirio_theme -p sirio_ui`
Expected: PASS with no reference to `theme.accent` remaining. Confirm:
```bash
grep -rn 'theme\.accent\b' rust/crates --include="*.rs" || echo "accent is free"
```

- [ ] **Step 3: Rename `gauge` to `accent`**

Run:
```bash
cd rust/crates
grep -rl 'theme\.gauge\b' sirio_ui/src sirio/src --include="*.rs" \
  | xargs -r sed -i '' 's/theme\.gauge\b/theme.accent/g'
```

Then rename the binding and field. `file_link` remains an alias of it; `git_untracked` stays on its own binding from Task 3.

- [ ] **Step 4: Update the tests**

`accent_clears_contrast_on_its_own_surface` and `accent_is_not_any_agent_brand` now read `brand_coral`. Rename what they read and rename the test functions to `brand_coral_clears_contrast_on_its_own_surface` and `brand_coral_is_not_any_agent_brand`, so the names keep matching what they assert.

- [ ] **Step 5: Run the tests**

Run: `cd rust && cargo test -p sirio_theme -p sirio_ui`
Expected: PASS

- [ ] **Step 6: Run the gate**

Run: `Scripts/gate-no-value-change.sh "$(cat /tmp/phase1-base)"`
Expected: `GATE OK`

- [ ] **Step 7: Commit**

```bash
git add rust/crates
git commit -m "refactor: rename accent to brand_coral and gauge to accent"
```

---

### Task 9: Drop the `.colors` prefix

**Files:**
- Modify: the 21 sites reaching `theme.colors.*`

**Interfaces:**
- Consumes: Tasks 4–8.
- Produces: every colour reached through `Deref`, so Phase 2 can repoint it in one place.

- [ ] **Step 1: Find them**

Run:
```bash
grep -rn 'theme\.colors\.' rust/crates --include="*.rs"
```
Expected: 21 matches.

- [ ] **Step 2: Rewrite them**

Run:
```bash
cd rust/crates
grep -rl 'theme\.colors\.' . --include="*.rs" \
  | xargs -r sed -i '' 's/theme\.colors\./theme./g'
```

Then check for the other spellings — `colors.` reached from a local binding, or `.colors` passed as a whole struct:
```bash
grep -rn '\.colors\b' rust/crates --include="*.rs"
```
Any remaining site that passes `colors` as a value rather than reaching through it must keep working; `ThemeColors` still exists at this point.

- [ ] **Step 3: Build and test**

Run: `cd rust && cargo test -p sirio_theme -p sirio_ui`
Expected: PASS

- [ ] **Step 4: Run the gate**

Run: `Scripts/gate-no-value-change.sh "$(cat /tmp/phase1-base)"`
Expected: `GATE OK`

- [ ] **Step 5: Commit**

```bash
git add rust/crates
git commit -m "refactor: reach theme colours through Deref everywhere"
```

---

### Task 10: Close Phase 1

**Files:**
- Modify: `docs/superpowers/plans/2026-08-30-bezel-theme-adoption.md` (tick the Phase 1 boxes)

- [ ] **Step 1: Run the gate against the phase base**

Run: `Scripts/gate-no-value-change.sh "$(cat /tmp/phase1-base)"`
Expected: `GATE OK`. This is the phase's closing evidence: every name moved, no number did.

- [ ] **Step 2: Confirm the provenance tests never changed their assertions**

Run:
```bash
git diff "$(cat /tmp/phase1-base)" -- rust/crates/sirio_theme/src/lib.rs \
  | grep -E '^[+-].*assert' | grep -E '0x[0-9A-Fa-f]{6}' || echo "no hex moved in assertions"
```
Expected: `no hex moved in assertions`

- [ ] **Step 3: Run the full available suite**

Run: `cd rust && cargo test -p sirio_theme -p sirio_ui -p sirio`
Expected: PASS. `sirio_terminal` is excluded — it needs Zig 0.15.2, see Global Constraints.

- [ ] **Step 4: Commit the tick-through**

```bash
git add docs/superpowers/plans/2026-08-30-bezel-theme-adoption.md
git commit -m "docs: close phase 1 of the bezel theme adoption"
```

---

# Phase 2 — Swap. Structure frozen.

### Task 11: The Phase 2 gate script

**Files:**
- Create: `Scripts/gate-theme-only.sh`

**Interfaces:**
- Produces: `Scripts/gate-theme-only.sh <base-ref>` — exits 0 when no file outside `rust/crates/sirio_theme/` changed.

- [ ] **Step 1: Write the script**

```bash
#!/usr/bin/env bash
# Phase 2 gate for the bezel theme adoption.
#
# Phase 2 changes values, never call sites. If a file outside sirio_theme moved,
# a value change is hiding somewhere a reviewer cannot separate it from a
# refactor -- which is the exact failure the phase split exists to prevent.
set -euo pipefail

BASE="${1:?usage: gate-theme-only.sh <base-ref>}"

stray="$(git diff --name-only "$BASE" -- 'rust/' \
    | grep -v '^rust/crates/sirio_theme/' || true)"

if [[ -z "$stray" ]]; then
    echo "GATE OK: only sirio_theme changed since $BASE"
    exit 0
fi

echo "GATE FAILED: files outside sirio_theme changed since $BASE" >&2
printf '%s\n' "$stray" >&2
exit 1
```

- [ ] **Step 2: Make it executable and verify it passes on a clean tree**

Run:
```bash
chmod +x Scripts/gate-theme-only.sh
Scripts/gate-theme-only.sh HEAD
```
Expected: `GATE OK: only sirio_theme changed since HEAD`

- [ ] **Step 3: Verify it fails when another crate moves**

Run:
```bash
echo "// gate probe" >> rust/crates/sirio_ui/src/lib.rs
Scripts/gate-theme-only.sh HEAD; echo "exit=$?"
git checkout rust/crates/sirio_ui/src/lib.rs
```
Expected: `GATE FAILED` listing `rust/crates/sirio_ui/src/lib.rs`, and `exit=1`.

- [ ] **Step 4: Commit**

```bash
git add Scripts/gate-theme-only.sh
git commit -m "chore: add phase 2 gate for theme value swap"
```

---

### Task 12: Repoint `Deref` at `bezel::theme::Theme`

**Files:**
- Modify: `rust/crates/sirio_theme/src/lib.rs:1201-1250` (the `Theme` struct, its `Deref`, `for_appearance`)
- Modify: `rust/crates/sirio_theme/Cargo.toml` (add the `bezel` dependency)

**Interfaces:**
- Consumes: Phase 1's renamed fields.
- Produces: `sirio_theme::Theme` holding a `bezel::theme::Theme` and dereferencing to it; `SirioColors` holding what bezel has no token for. Tasks 13–17 build on this.

- [ ] **Step 1: Record the base rev**

Run:
```bash
git rev-parse HEAD > /tmp/phase2-base
cat /tmp/phase2-base
```

- [ ] **Step 2: Add the dependency**

In `rust/crates/sirio_theme/Cargo.toml`, under `[dependencies]`:

```toml
bezel = { workspace = true }
```

- [ ] **Step 3: Write the failing test**

Add to `mod tests`:

```rust
#[test]
fn theme_colours_come_from_bezel() {
    // The swap's defining property: Sirio's surface IS bezel's surface, not a
    // copy that happens to agree today.
    let sirio = Theme::dark();
    let bezel = bezel::theme::Theme::dark();
    assert_eq!(sirio.surface, bezel.surface);
    assert_eq!(sirio.text, bezel.text);
    assert_eq!(sirio.border, bezel.border);
}
```

- [ ] **Step 4: Run it to verify it fails**

Run: `cd rust && cargo test -p sirio_theme theme_colours_come_from_bezel`
Expected: FAIL — either a type mismatch (`Rgba` vs `Hsla`) or unequal values.

- [ ] **Step 5: Add `SirioColors` and repoint the `Deref`**

```rust
/// The colours bezel has no token for. Everything else reaches
/// `bezel::theme::Theme` through [`Theme`]'s `Deref`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SirioColors {
    /// Terminal surface. bezel has no terminal-surface concept: the terminal is
    /// paper-white in light and its own dark well in dark, independent of the
    /// shell's panel hierarchy.
    pub terminal_surface: Hsla,
    /// Sirio's brand coral. No role paints it; the Coral entry of the
    /// agent-colour picker reads it, and two invariants hold it — it clears AA
    /// on its own surface, and it is not any agent's brand.
    pub brand_coral: Hsla,
}

impl Deref for Theme {
    type Target = bezel::theme::Theme;

    fn deref(&self) -> &Self::Target {
        &self.bezel
    }
}
```

`Theme` gains `pub bezel: bezel::theme::Theme` and `pub sirio: SirioColors`, and loses `colors: ThemeColors`. `for_appearance` becomes a call to `bezel::theme::Theme::dark()` / `light()` plus construction of `SirioColors`.

- [ ] **Step 6: Run the test**

Run: `cd rust && cargo test -p sirio_theme theme_colours_come_from_bezel`
Expected: PASS

- [ ] **Step 7: Run the invariants**

Run: `cd rust && cargo test -p sirio_theme`
Expected: the invariant tests PASS; the provenance tests FAIL, because their values are now bezel's. That is the intended state — Task 16 retargets them. Record which ones fail:

```bash
cd rust && cargo test -p sirio_theme 2>&1 | grep -E '^test .* FAILED' > /tmp/phase2-expected-failures.txt
cat /tmp/phase2-expected-failures.txt
```

If any *invariant* test is in that list, stop: a value has broken a design rule, and that is a real failure rather than an expected one.

- [ ] **Step 8: Commit**

```bash
git add rust/crates/sirio_theme
git commit -m "refactor: derive theme colours from bezel"
```

---

### Task 13: Land the derived and retained tokens

**Files:**
- Modify: `rust/crates/sirio_theme/src/lib.rs`

**Interfaces:**
- Consumes: Task 12's `SirioColors`.
- Produces: `overlay`, `overlay_strong`, `tree_guide`, `diff_add_bg`, `diff_del_bg`, `border_opaque` resolved, and `git_untracked` on its neutral.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn git_untracked_is_a_neutral_not_the_accent() {
    // C1: untracked leaves the blue. It is quieter than the chromatic
    // staged / modified / conflict states, so it reads as "not yet tracked"
    // rather than as a status.
    let dark = Theme::dark();
    assert_eq!(dark.git_untracked, dark.text_faint);
    assert_ne!(dark.git_untracked, dark.accent);
}
```

Delete `git_untracked_is_its_own_binding_not_the_gauge_blue` from Task 3 — it documented the pre-split state and this test replaces it.

- [ ] **Step 2: Run it to verify it fails**

Run: `cd rust && cargo test -p sirio_theme git_untracked_is_a_neutral`
Expected: FAIL — `git_untracked` still carries the old gauge value.

- [ ] **Step 3: Resolve each remaining token**

```rust
// bezel has no rung at 0.05 or 0.12; wash() is its interactive-state helper
// and takes the alpha directly, so Sirio's faint and mid rungs survive as
// calls rather than as tokens.
overlay: bezel::theme::wash(0.05),
overlay_strong: bezel::theme::wash(0.12),
// A guide is an edge, so it scales with the surround like every other
// hairline rather than holding a fixed alpha.
tree_guide: bezel::theme::hairline(0.12),
// The same move bezel makes for diff_hunk_bg: the accent at low alpha.
// `softened` is Sirio's own Rgba helper and these are Hsla after the swap,
// so set the alpha through struct update instead of converting twice.
diff_add_bg: Hsla { a: 0.12, ..bezel.diff_add },
diff_del_bg: Hsla { a: 0.12, ..bezel.diff_del },
```

`border_opaque` collapses into `border`: delete the field and point its call sites at `border`. This is the value change Task 6 deferred, and it is allowed here.

`git_untracked` takes `text_faint`.

- [ ] **Step 4: Run the test**

Run: `cd rust && cargo test -p sirio_theme git_untracked_is_a_neutral`
Expected: PASS

- [ ] **Step 5: Run the invariants**

Run: `cd rust && cargo test -p sirio_theme`
Expected: same failure set as `/tmp/phase2-expected-failures.txt`, no invariant among them.

- [ ] **Step 6: Run the gate**

Run: `Scripts/gate-theme-only.sh "$(cat /tmp/phase2-base)"`
Expected: `GATE OK`

- [ ] **Step 7: Commit**

```bash
git add rust/crates/sirio_theme
git commit -m "refactor: resolve derived and retained colour tokens"
```

---

### Task 14: Switch to bezel's fonts

**Files:**
- Modify: `rust/Cargo.toml:65`
- Modify: `rust/crates/sirio_theme/src/lib.rs:1001-1062` (the family candidate lists)
- Modify: `rust/crates/sirio/src/main.rs:14908-14921` (delete `register_fonts`)
- Delete: `rust/assets/fonts/`

**Interfaces:**
- Consumes: Task 12's bezel-backed `Theme`.
- Produces: Geist on every platform, with real 500/600/700 faces.

This task touches files outside `sirio_theme`, so **the Phase 2 gate does not apply to it**. Run it as its own commit, and resume gating from the next task. The exception is deliberate: deleting a font asset cannot be done from inside `sirio_theme`, and bundling it with a value change would hide one in the other.

- [ ] **Step 1: Write the failing test**

In `rust/crates/sirio_theme/src/lib.rs`, replace `macos_keeps_the_apple_faces` with:

```rust
#[test]
fn geist_leads_on_every_platform() {
    // B3 makes the bundled face the face everywhere: bezel ships Geist with
    // real 500/600/700 statics, which the cosmic-text path needs because it
    // rasterizes variable fonts at their default instance only.
    assert_eq!(UI_FAMILY_CANDIDATES[0], "Geist");
    assert_eq!(CODE_FAMILY_CANDIDATES[0], "Geist Mono");
}
```

- [ ] **Step 2: Run it to verify it fails on macOS**

Run: `cd rust && cargo test -p sirio_theme geist_leads_on_every_platform`
Expected: FAIL with `assertion failed: left == right`, left `"SF Pro"`.

- [ ] **Step 3: Enable the bezel font features**

In `rust/Cargo.toml`, change line 65 to:

```toml
bezel = { version = "=0.1.3", default-features = false, features = [
    "geist-sans",
    "geist-mono",
    "geist-weights",
] }
```

- [ ] **Step 4: Collapse the family candidate lists**

Delete both `#[cfg(target_os = "macos")]` blocks for `UI_FAMILY_CANDIDATES` and `CODE_FAMILY_CANDIDATES`, and drop the `#[cfg(not(target_os = "macos"))]` attribute from the survivors. `TERMINAL_FAMILY_CANDIDATES` keeps its per-OS split — the terminal needs a Nerd Font and that is unrelated.

- [ ] **Step 5: Delete Sirio's own registration**

Remove `register_fonts` from `rust/crates/sirio/src/main.rs` together with its `#[cfg(not(target_os = "macos"))]` call in `main`, and delete the assets:

```bash
git rm -r rust/assets/fonts
```

bezel registers its own faces; Sirio no longer needs to.

- [ ] **Step 6: Run the test**

Run: `cd rust && cargo test -p sirio_theme geist_leads_on_every_platform`
Expected: PASS

- [ ] **Step 7: Verify the app still finds a face**

Run: `cd rust && cargo build -p sirio`
Expected: builds. Then launch it and confirm text renders — a missing registration shows as blank or fallback glyphs, which no test catches.

- [ ] **Step 8: Commit**

```bash
git add rust/Cargo.toml rust/crates/sirio_theme rust/crates/sirio
git commit -m "refactor: take fonts from bezel on every platform"
```

---

### Task 15: Re-express spacing and radii as ratios

**Files:**
- Modify: `rust/crates/sirio_theme/src/lib.rs:516-672` (`Spacing`, `Radii`)

**Interfaces:**
- Consumes: Task 12.
- Produces: `Spacing` and `Radii` with identical values, derived from bezel's constants.

Per spec decision T2, **no value moves**. Only its derivation does.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn radii_are_ratios_of_bezel_base_radius() {
    // T2: the values do not move, but they stop being independent constants.
    // Bezel's Brand::radius moves the whole ladder together; a literal cannot
    // follow it.
    use bezel::theme::Theme as BezelTheme;
    let radii = Radii::default();
    assert_eq!(radii.code_block, px(BezelTheme::button_radius()));
    assert_eq!(radii.user_pill, px(BezelTheme::surface_radius()));
}
```

`code_block` is 8.0 and `button_radius()` is 8.0; `user_pill` is 12.0 and `surface_radius()` is 12.0. Both already agree, which is why they are the test's anchors.

- [ ] **Step 2: Run it to verify it fails**

Run: `cd rust && cargo test -p sirio_theme radii_are_ratios_of_bezel_base_radius`
Expected: FAIL — `Radii::default` still returns literals, so the import is unused and the equality is coincidental rather than derived.

- [ ] **Step 3: Rewrite `Radii::default`**

Each radius becomes a ratio of `BASE_RADIUS`, choosing the ratio that reproduces the current number:

`BASE_RADIUS` and the `SPACE_*` steps are **associated consts on bezel's
`Theme`**, not free constants, so they need qualifying. Import once at the top of
the module: `use bezel::theme::Theme as BezelTheme;`

```rust
// The values are unchanged (T2); what changes is that they now follow
// Brand::radius instead of standing alone. Ratios that do not land on one of
// bezel's five named corners get an explicit multiplier rather than being
// rounded to the nearest named one -- rounding would move the UI, which T2
// forbids.
chip: px(BezelTheme::BASE_RADIUS * 0.5),          // 4.0
chip_active: px(BezelTheme::BASE_RADIUS * 0.625), // 5.0
control: px(BezelTheme::BASE_RADIUS * 0.75),      // 6.0
shell_panel: px(BezelTheme::BASE_RADIUS * 0.875), // 7.0
row_card: px(BezelTheme::BASE_RADIUS * 0.875),    // 7.0
code_block: px(BezelTheme::BASE_RADIUS),          // 8.0
toast: px(BezelTheme::BASE_RADIUS * 1.25),        // 10.0
user_pill: px(BezelTheme::BASE_RADIUS * 1.5),     // 12.0
composer: px(BezelTheme::BASE_RADIUS * 1.625),    // 13.0
```

- [ ] **Step 4: Rewrite `Spacing::default` the same way**

Against `SPACE_XS` 4.0, `SPACE_SM` 8.0, `SPACE_MD` 12.0, `SPACE_LG` 16.0:

```rust
shell_gap: px(BezelTheme::SPACE_XS),                     // 4.0
shell_outer_inset: px(BezelTheme::SPACE_XS),             // 4.0
card_corner_radius: px(BezelTheme::SPACE_MD * 0.5),      // 6.0
card_gap: px(BezelTheme::SPACE_MD * 0.833_333_3),        // 10.0
card_shadow_radius: px(BezelTheme::SPACE_LG * 1.125),    // 18.0
card_shadow_y_offset: px(BezelTheme::SPACE_MD * 0.5),    // 6.0
title_strip_height: px(BezelTheme::SPACE_LG * 3.0),      // 48.0
traffic_light_inset: px(BezelTheme::SPACE_LG * 0.875),   // 14.0
title_strip_icon_size: px(BezelTheme::SPACE_LG * 0.875), // 14.0
titlebar_control_spacing: px(BezelTheme::SPACE_MD * 0.5), // 6.0
bottom_bar_height: px(BezelTheme::SPACE_LG * 2.5),       // 40.0
menu_width: px(BezelTheme::SPACE_LG * 15.0),             // 240.0
hairline_thickness: px(BezelTheme::SPACE_XS * 0.25),     // 1.0
compact_action: px(BezelTheme::SPACE_LG * 1.5),          // 24.0
```

`titlebar_control_frame` is a `Size<Pixels>`:
`size(px(BezelTheme::SPACE_LG * 1.625), px(BezelTheme::SPACE_LG * 1.625))` — 26.0 square.

- [ ] **Step 5: Run the test and the value guards**

Run: `cd rust && cargo test -p sirio_theme`
Expected: `radii_are_ratios_of_bezel_base_radius` PASSes, and `radii_match_waku` and `spacing_and_typography_match_waku` **still pass unchanged** — they assert the numbers, and the numbers did not move. If either fails, a ratio is wrong.

- [ ] **Step 6: Run the gate**

Run: `Scripts/gate-theme-only.sh "$(cat /tmp/phase2-base)"`
Expected: `GATE OK`

- [ ] **Step 7: Commit**

```bash
git add rust/crates/sirio_theme
git commit -m "refactor: derive spacing and radii from bezel constants"
```

---

### Task 16: Retarget the provenance tests and write the provenance doc

**Files:**
- Modify: `rust/crates/sirio_theme/src/lib.rs` (`mod tests`)
- Create: `docs/THEME-PROVENANCE.md`

**Interfaces:**
- Consumes: Tasks 12–15.
- Produces: a green suite and a live provenance record.

This is spec risk R4. The old `docs/linux-rewrite/THEME-PROVENANCE.md` no longer exists, so the record has already been lost once; retargeting the tests without rewriting it loses it a second time.

- [ ] **Step 1: List what still fails**

Run: `cd rust && cargo test -p sirio_theme 2>&1 | grep -E '^test .* FAILED'`
Expected: the set recorded in `/tmp/phase2-expected-failures.txt`, and nothing else.

- [ ] **Step 2: Retarget each provenance test**

Each becomes an assertion that Sirio's value *is* bezel's, rather than that it is a recorded hex. For example, `dark_palette_matches_recorded_provenance` becomes:

```rust
#[test]
fn dark_palette_comes_from_bezel() {
    // The palette is no longer sampled from reference frames; it is bezel's,
    // and this test exists to catch a bezel bump silently restyling the app
    // (risk R1).
    let sirio = Theme::dark();
    let bezel = bezel::theme::Theme::dark();
    assert_eq!(sirio.bg, bezel.bg);
    assert_eq!(sirio.surface, bezel.surface);
    assert_eq!(sirio.surface_raised, bezel.surface_raised);
    assert_eq!(sirio.text, bezel.text);
    assert_eq!(sirio.text_muted, bezel.text_muted);
    assert_eq!(sirio.text_faint, bezel.text_faint);
    assert_eq!(sirio.border, bezel.border);
    assert_eq!(sirio.border_strong, bezel.border_strong);
    assert_eq!(sirio.success, bezel.success);
    assert_eq!(sirio.warning, bezel.warning);
    assert_eq!(sirio.danger, bezel.danger);
}
```

Do the same for the light palette. `intellij_shell_palette_matches_the_approved_reference` and `the_state_hues_are_the_ones_the_swift_app_shipped` are deleted — the reference they name is no longer what Sirio paints, and a retargeted version would duplicate the test above.

- [ ] **Step 3: Write the provenance doc**

Create `docs/THEME-PROVENANCE.md` recording, for each of the three categories: colours come from `bezel::theme::Theme::dark()`/`light()` at the pinned `=0.1.3`; radii and spacing are ratios of `BASE_RADIUS` and `SPACE_*` reproducing the values recorded in the previous provenance document; and the two `SirioColors` entries are Sirio's own, with the reason each is not in bezel. State that a `bezel` bump moves the first category and must be reviewed as a visual change.

- [ ] **Step 4: Run the full suite**

Run: `cd rust && cargo test -p sirio_theme -p sirio_ui`
Expected: PASS, no failures.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/sirio_theme docs/THEME-PROVENANCE.md
git commit -m "test: retarget palette provenance to bezel"
```

---

### Task 17: Sync the appearance mirror in one place

**Files:**
- Modify: `rust/crates/sirio_theme/src/lib.rs` (`Theme::install`, `Theme::set_mode`)
- Modify: `rust/crates/sirio_ui/src/loading.rs:71-92` (delete the private bridge)

**Interfaces:**
- Consumes: Task 12.
- Produces: `bezel::theme::set_current_appearance` called from exactly one place.

This is spec risk R2. bezel's `ink`, `wash` and `hairline` are free functions with no `cx`; they read a process-wide mirror that defaults to Dark. Today only `loading.rs` syncs it, for the loaders. After Task 13 the whole palette paints through it.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn installing_a_theme_syncs_bezels_appearance_mirror() {
    // bezel's free paint helpers read a process-wide mirror, not the theme
    // handed to them. If install does not push to it, the light theme paints
    // dark hairlines -- and nothing else in the suite would notice.
    let _guard = bezel::theme::lock_appearance();

    Theme::light().sync_appearance();
    assert_eq!(
        bezel::theme::current_appearance(),
        bezel::theme::Appearance::Light
    );

    Theme::dark().sync_appearance();
    assert_eq!(
        bezel::theme::current_appearance(),
        bezel::theme::Appearance::Dark
    );
}
```

`lock_appearance` is bezel's own guard for exactly this — the mirror is process-wide, so two tests touching it concurrently would flake.

- [ ] **Step 2: Run it to verify it fails**

Run: `cd rust && cargo test -p sirio_theme installing_a_theme_syncs`
Expected: FAIL — `no method named sync_appearance`.

- [ ] **Step 3: Add the method and call it from `install`**

```rust
impl Theme {
    /// Push this theme's appearance into bezel's process-wide mirror.
    ///
    /// bezel's `ink`, `wash` and `hairline` are free functions with no `cx`, so
    /// they cannot read the theme they are painting for. Every path that
    /// installs or changes a theme must call this, or those helpers keep
    /// painting for the previous appearance.
    pub fn sync_appearance(&self) {
        bezel::theme::set_current_appearance(match self.appearance {
            Appearance::Light => bezel::theme::Appearance::Light,
            Appearance::Dark => bezel::theme::Appearance::Dark,
        });
    }
}
```

Call it at the end of both `Theme::install` and `Theme::set_mode`.

- [ ] **Step 4: Run the test**

Run: `cd rust && cargo test -p sirio_theme installing_a_theme_syncs`
Expected: PASS

- [ ] **Step 5: Delete the private bridge in `loading.rs`**

Remove `sync_bezel_appearance` and `bezel_theme` from `rust/crates/sirio_ui/src/loading.rs`. Their callers take `&Theme` and now reach bezel's palette through `Deref`, so `loaders::orb(..., &theme, ...)` works directly.

This step touches `sirio_ui`, so run it as its own commit and note that the Phase 2 gate does not cover it — same exception as Task 14.

- [ ] **Step 6: Run the tests**

Run: `cd rust && cargo test -p sirio_theme -p sirio_ui`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add rust/crates/sirio_theme rust/crates/sirio_ui
git commit -m "fix: sync bezel's appearance mirror from theme install"
```

---

### Task 18: Close Phase 2 with a visual review

**Files:**
- Modify: this plan (tick the Phase 2 boxes)

- [ ] **Step 1: Build and run both apps side by side**

Run:
```bash
cd rust && cargo run -p sirio &
cd /Users/enzopiopalmisano/.cache/fx/bezel-gallery-analysis && cargo run -p gallery &
```

- [ ] **Step 2: Check the four things no test covers**

- `file_link` — clickable file paths no longer carry a colour signal (C1). Confirm they are still discoverable by underline or hover. If they are not, this is the behaviour change the spec flagged, and it needs an underline adding.
- Hairlines in **light** mode — bezel scales them by 1.35 where Sirio did not. Confirm separators read as seams, not as lines.
- `border_opaque`'s collapse into `border` — panel borders went from opaque to translucent. Confirm panels still separate.
- Progress bars and quota meters — now neutral rather than blue. Confirm they still read as quantity.

- [ ] **Step 3: Run everything available**

Run: `cd rust && cargo test -p sirio_theme -p sirio_ui -p sirio`
Expected: PASS

- [ ] **Step 4: Commit the tick-through**

```bash
git add docs/superpowers/plans/2026-08-30-bezel-theme-adoption.md
git commit -m "docs: close phase 2 of the bezel theme adoption"
```

---

# Phase 3 — Collapse.

### Task 19: Install Zig 0.15.2

**Files:** none — environment only.

**Interfaces:**
- Produces: a runnable `Scripts/ci.sh`. Every remaining task depends on it.

Spec risk R5. This machine has Zig 0.16.0; `libghostty-vt-sys` needs exactly 0.15.2, and a *newer* Zig fails too.

- [ ] **Step 1: Confirm the current state**

Run: `zig version`
Expected: `0.16.0` — the wrong version.

- [ ] **Step 2: Install 0.15.2 alongside**

This needs the user: ask them to run the install themselves, since it touches the machine rather than the repo. Suggest they type `! brew install zig@0.15` in the session, or fetch the 0.15.2 tarball from ziglang.org/download and put it first on PATH. Do not uninstall 0.16.0 — other projects may need it.

- [ ] **Step 3: Verify**

Run: `zig version && Scripts/ci.sh`
Expected: `0.15.2`, then `CI OK`.

If `Scripts/ci.sh` fails for reasons unrelated to Zig, record them before continuing — per `CLAUDE.md` the macOS gate has never been green, so a pre-existing failure here is expected and is not caused by this work.

---

### Task 20: Remove `cosmic` and rebase the titlebar

**Files:**
- Delete: `rust/crates/sirio_theme/src/cosmic/` (10 files)
- Modify: `rust/crates/sirio_theme/src/lib.rs:40` (the `pub mod cosmic;`) and the `cosmic` field on `Theme`
- Modify: `rust/crates/sirio_ui/src/titlebar.rs:73,104,671-676`
- Modify: `rust/crates/sirio_ui/examples/registry_browse_proto.rs:1004-1013`

**Interfaces:**
- Consumes: Phase 2's bezel-backed `Theme`.
- Produces: a `sirio_theme` with no COSMIC tokens.

- [ ] **Step 1: Find every consumer**

Run:
```bash
grep -rn 'cosmic' rust/crates --include="*.rs" | grep -v 'sirio_theme/src/cosmic'
```
Expected: 59 sites, concentrated in `titlebar.rs`, plus the example.

- [ ] **Step 2: Rewrite `titlebar.rs`**

The three tokens it reads map as:

```rust
// was: cosmic.containers.background
let bar = theme.surface;
// was: cosmic.semantic.icon_button
let icon_button = theme.element_hover;
// was: px(cosmic.radii.radius_xs[0])
let control_radius = px(bezel::theme::Theme::control_radius());
```

Update the module doc-comment at `titlebar.rs:73`, which currently explains the COSMIC mapping, to say the bar now reads bezel tokens like every other surface.

- [ ] **Step 3: Rewrite the example**

`registry_browse_proto.rs` uses `theme.cosmic.spacing.xs` for padding. Replace with `px(bezel::theme::Theme::SPACE_XS)`.

- [ ] **Step 4: Delete the module**

```bash
git rm -r rust/crates/sirio_theme/src/cosmic
```

Remove `pub mod cosmic;` from `lib.rs:40`, the `cosmic` field from `Theme`, its construction in `for_appearance`, and the three `*_cosmic_*` tests.

- [ ] **Step 5: Build and test**

Run: `cd rust && cargo test -p sirio_theme -p sirio_ui`
Expected: PASS

- [ ] **Step 6: Check the titlebar on Linux**

The COSMIC tokens existed for Pop!_OS adherence, so this is where the accepted cost of B3 becomes visible. If a Linux machine is not available, say so rather than claiming it was checked.

- [ ] **Step 7: Commit**

```bash
git add rust/crates
git commit -m "refactor: drop COSMIC tokens for bezel's"
```

---

### Task 21: Collapse the appearance enums

**Files:**
- Modify: `rust/crates/sirio_theme/src/lib.rs:46-56` (delete `ThemeMode`)
- Modify: `rust/crates/sirio_persistence/src/model.rs:334`
- Delete: `rust/crates/sirio_project/src/ui.rs:3` (`AppearanceMode`)
- Modify: `rust/crates/sirio/src/main.rs:14639,14676,3275-3277`

**Interfaces:**
- Consumes: Task 20.
- Produces: two appearance enums instead of four — the persisted one and bezel's.

- [ ] **Step 1: Write the failing test**

In `rust/crates/sirio_persistence/src/model.rs`'s tests:

```rust
#[test]
fn appearance_mode_round_trips_through_bezel() {
    // The persisted enum is the only one that keeps a serde contract; bezel's
    // is what the theme reads. This is the single conversion point, replacing
    // the two match blocks that used to live in main.rs.
    for mode in [
        AppearanceMode::System,
        AppearanceMode::Light,
        AppearanceMode::Dark,
    ] {
        assert_eq!(AppearanceMode::from(bezel::theme::appearance::AppearanceMode::from(mode)), mode);
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd rust && cargo test -p sirio_persistence appearance_mode_round_trips`
Expected: FAIL — the `From` impls do not exist.

- [ ] **Step 3: Add the conversions**

Add `From<AppearanceMode> for bezel::theme::appearance::AppearanceMode` and its inverse in `sirio_persistence`. Its serde representation does not change — the stored strings `"system"`, `"light"`, `"dark"` stay exactly as they are, so existing session databases keep working.

- [ ] **Step 4: Delete the duplicates**

Remove `sirio_theme::ThemeMode` and `sirio_project::ui::AppearanceMode`, and replace the two `match` blocks at `main.rs:14639` and `:14676` with the `From` impls. `main.rs:3275-3277` maps the mode to a display string — point it at the persisted enum.

`Theme::for_mode` and `Theme::set_mode` take `bezel::theme::appearance::AppearanceMode` instead of `ThemeMode`.

- [ ] **Step 5: Run the test and the suite**

Run: `cd rust && cargo test -p sirio_persistence -p sirio_theme -p sirio_ui`
Expected: PASS, including `system_mode_follows_window_appearance`.

- [ ] **Step 6: Verify a stored preference still loads**

Launch the app, set the appearance to Light, quit, relaunch. Expected: it opens Light. A serde break would show here and in no test.

- [ ] **Step 7: Commit**

```bash
git add rust/crates
git commit -m "refactor: collapse the appearance enums onto bezel's"
```

---

### Task 22: Close the work

**Files:**
- Modify: `CLAUDE.md` (the crate description of `sirio_theme`)
- Modify: this plan

- [ ] **Step 1: Run the full gate**

Run: `Scripts/ci.sh`
Expected: `CI OK`. If Zig is still wrong, this is blocked on Task 19 — say so rather than skipping it.

- [ ] **Step 2: Confirm what `sirio_theme` became**

Run: `wc -l rust/crates/sirio_theme/src/*.rs`
Expected: substantially below the starting 4357 lines, holding only `SirioColors`, `Typography`, `Spacing`, `Radii`, `AgentBrandColor`, `graph_lane`, `BrowserChrome`, `WindowsCaption` and the terminal families.

- [ ] **Step 3: Update `CLAUDE.md`**

Its crate-boundary diagram lists `sirio_theme` as a leaf with no local dependencies. It now depends on `bezel`. Update the diagram and add a line saying colours, spacing and radii come from `bezel::theme`, so a future reader does not go looking for a palette that is no longer there.

- [ ] **Step 4: Commit**

```bash
git add CLAUDE.md docs/superpowers/plans/2026-08-30-bezel-theme-adoption.md
git commit -m "docs: record bezel as the theme source"
```

---

## Self-Review Notes

**Spec coverage.** Every spec section maps to a task: the mapping tables drive Tasks 4–8 and 13; the fonts section is Task 14; T2's ratio requirement is Task 15; the three phases are the three parts; the two gates are Tasks 1 and 11; the testing section's two test families are split across Tasks 10 and 16. Risks R1 through R5 are covered by Task 16 Step 2 (R1 — the retargeted test exists to catch a bezel bump), Task 17 (R2), Task 2 (R3's remaining review row), Task 16 Step 3 (R4) and Task 19 (R5).

**Two deliberate gate exceptions.** Tasks 14 and 17 touch files outside `sirio_theme` during Phase 2, so the Phase 2 gate does not apply to them. Both say so in their own body, and both are isolated commits. The alternative — deferring them to Phase 3 — would leave the app painting through a stale appearance mirror for a whole phase, which is worse than a documented exception.

**One ordering hazard, stated three times.** The `sed` loops in Tasks 4–8 rename prefixes before their shorter siblings (`title_selected` before `title`, `on_inverse` before `inverse`, `chat_row_hover` before `row_hover`, `diff_addition_background` before `diff_addition`). Each loop is already in a safe order and each task says not to reorder it.

**What Task 3 and Task 13 share.** Task 3 adds a test asserting `git_untracked == gauge`; Task 13 deletes it and adds one asserting the opposite. This is intentional — the first documents that Phase 1 changed structure without changing value, and it has to be removed when the value moves. Task 13 Step 1 says so explicitly.
