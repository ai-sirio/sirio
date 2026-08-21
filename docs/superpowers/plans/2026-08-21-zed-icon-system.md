# Zed Icon System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Tiller's mixed Comet, Phosphor, and macOS SF Symbol icon paths with pinned Zed SVG assets rendered at semantic 12/14/16 px sizes on every platform.

**Architecture:** Keep `tiller_ui::icons::Icon` as the semantic catalog and vendor only the 22 required Zed assets. Rebuild `IconElement` as a lightweight `RenderOnce` adapter over GPUI's stock `svg()` element, retain a canvas-backed cached path only for the full-color Oh My Pi mark, and resolve semantic sizes against Tiller's typography base-size delta. Migrate UI call sites in two green increments before removing the temporary raw-pixel compatibility conversion.

**Tech Stack:** Rust, GPUI pinned at `c05e34637b4f7f100a688bf6ac71cb70877fc8ad`, `tiller_ui`, `tiller_theme`, Cargo tests, vendored SVG assets from Zed commit `875e2a1c458ffcf2bfe90be841eb9893f399b4ff`.

---

## File map

**Create**

- `rust/assets/icons/zed/ATTRIBUTION.md` — upstream commit, source paths, license, and import policy.
- `rust/assets/icons/zed/LICENSES` — byte-identical copy of Zed's icon license file.
- `rust/assets/icons/zed/*.svg` — the 22 pinned SVGs listed in Task 1.

**Modify**

- `rust/crates/tiller_ui/src/icons.rs` — catalog mapping, semantic sizes, renderer, asset source, and focused tests.
- `rust/crates/tiller_ui/src/project_identity.rs` — 14 px semantic picker glyphs.
- `rust/crates/tiller_ui/src/changes.rs` — 12 px disclosures and 14 px file glyphs.
- `rust/crates/tiller_ui/src/status_bar.rs` — 12 px action and provider glyphs; remove Comet-specific wording.
- `rust/crates/tiller_ui/src/titlebar.rs` — 16 px titlebar cluster icons.
- `rust/crates/tiller_ui/src/right_panel.rs` — semantic file, disclosure, surface, close, and empty-state sizes; remove Comet-specific wording.
- `rust/crates/tiller_ui/src/settings.rs` — semantic Settings navigation and picker sizes.
- `rust/crates/tiller_ui/src/chat.rs` — semantic attachment, disclosure, and add sizes.
- `rust/crates/tiller_ui/src/tab_bar.rs` — semantic tab, menu, disclosure, and add sizes.
- `rust/crates/tiller_ui/src/sidebar.rs` — semantic row, brand, disclosure, settings, and close sizes.
- `rust/crates/tiller/src/main.rs` — semantic app-shell tab, close, overflow, and empty-state sizes.
- `THIRD_PARTY_NOTICES.md` — replace live Comet/Phosphor notices with pinned Zed/Lucide provenance while retaining historical and trademark notices.

**Delete after reference checks**

- `rust/crates/tiller_ui/src/sfsymbol.rs` — platform-specific renderer no longer used.
- `rust/assets/icons/comet/` — superseded Comet set.
- Superseded root generic and brand SVGs enumerated in Task 5; retain `agent-pi.svg` and `agent-omp.svg`.

Historical Linux-rewrite task documents remain unchanged because they describe the implementation that existed at those commits.

### Task 1: Vendor and map the pinned Zed icon catalog

**Files:**

- Create: `rust/assets/icons/zed/ATTRIBUTION.md`
- Create: `rust/assets/icons/zed/LICENSES`
- Create: `rust/assets/icons/zed/{ai_claude,ai_open_ai,ai_open_code,archive,chat,chevron_down,chevron_left,chevron_right,close,file,folder,git_branch,lock,plus,public,rotate_cw,screen,settings,sparkle,terminal,threads_sidebar_left_open,threads_sidebar_right_open}.svg`
- Modify: `rust/crates/tiller_ui/src/icons.rs:1-294,538-823`

- [ ] **Step 1: Record the clean focused baseline**

Run:

```bash
cd rust
cargo test -p tiller_ui icons::tests
```

Expected: PASS with a non-zero icon-test count. Record the exact count; do not describe this as the full workspace suite.

- [ ] **Step 2: Write the failing catalog tests**

Replace the path-family, canvas, agent-mark, and old Comet stroke tests in `icons.rs` with this table-driven contract:

```rust
#[test]
fn catalog_uses_the_approved_zed_and_tiller_assets() {
    let expected = [
        (Icon::FolderFill, "icons/zed/folder.svg"),
        (Icon::GitBranch, "icons/zed/git_branch.svg"),
        (Icon::MessageSquare, "icons/zed/chat.svg"),
        (Icon::SquareTerminal, "icons/zed/terminal.svg"),
        (Icon::Close, "icons/zed/close.svg"),
        (Icon::ChevronDown, "icons/zed/chevron_down.svg"),
        (Icon::ChevronRight, "icons/zed/chevron_right.svg"),
        (Icon::ChevronLeft, "icons/zed/chevron_left.svg"),
        (Icon::Settings, "icons/zed/settings.svg"),
        (Icon::RefreshCw, "icons/zed/rotate_cw.svg"),
        (Icon::Plus, "icons/zed/plus.svg"),
        (Icon::File, "icons/zed/file.svg"),
        (Icon::Sparkles, "icons/zed/sparkle.svg"),
        (Icon::Shield, "icons/zed/lock.svg"),
        (Icon::SunMoon, "icons/zed/screen.svg"),
        (Icon::Globe, "icons/zed/public.svg"),
        (Icon::ClaudeCode, "icons/zed/ai_claude.svg"),
        (Icon::Codex, "icons/zed/ai_open_ai.svg"),
        (Icon::OpenCode, "icons/zed/ai_open_code.svg"),
        (Icon::Pi, "icons/agent-pi.svg"),
        (Icon::OhMyPi, "icons/agent-omp.svg"),
        (Icon::SidebarLeft, "icons/zed/threads_sidebar_left_open.svg"),
        (Icon::PanelRight, "icons/zed/threads_sidebar_right_open.svg"),
        (Icon::Archive, "icons/zed/archive.svg"),
        (Icon::Lock, "icons/zed/lock.svg"),
    ];

    assert_eq!(expected.len(), ALL_ICONS.len());
    for (icon, path) in expected {
        assert_eq!(icon.path(), path, "wrong asset for {icon:?}");
    }
}

#[test]
fn every_zed_icon_keeps_its_upstream_16px_canvas() {
    for icon in ALL_ICONS {
        if !icon.path().starts_with("icons/zed/") {
            continue;
        }
        let svg = std::str::from_utf8(icon.svg()).expect("svg is utf-8");
        let has_16px_dimensions = svg.contains("width=\"16\"")
            && svg.contains("height=\"16\"");
        let has_16px_view_box = svg.contains("viewBox=\"0 0 16 16\"");
        assert!(
            has_16px_dimensions || has_16px_view_box,
            "{} must keep Zed's 16px canvas",
            icon.path()
        );
    }
}

#[test]
fn agent_catalog_uses_zed_where_available_and_tiller_for_pi_family() {
    assert_eq!(Icon::ClaudeCode.path(), "icons/zed/ai_claude.svg");
    assert_eq!(Icon::Codex.path(), "icons/zed/ai_open_ai.svg");
    assert_eq!(Icon::OpenCode.path(), "icons/zed/ai_open_code.svg");
    assert_eq!(Icon::Pi.path(), "icons/agent-pi.svg");
    assert_eq!(Icon::OhMyPi.path(), "icons/agent-omp.svg");
}
```

Update `every_icon_has_embedded_svg_payload` so it still requires `<svg` but does not require every upstream Zed file to contain a `viewBox`. Change `view_box_size_parses_embedded_svgs` to expect `16.0` for `FolderFill` and `800.0` for `Pi`.

Delete `directly_mapped_16px_icons_match_the_24px_sets_effective_stroke_weight`; the vendored Zed bytes must not be normalized locally. Replace `monochrome_marks_keep_the_reference_shapes` with:

```rust
#[test]
fn monochrome_agent_marks_use_the_approved_sources() {
    for icon in [Icon::ClaudeCode, Icon::Codex, Icon::OpenCode] {
        let svg = std::str::from_utf8(icon.svg()).expect("svg is utf-8");
        assert!(icon.path().starts_with("icons/zed/"));
        assert!(svg.contains("width=\"16\"") && svg.contains("height=\"16\""));
        assert!(!icon.has_own_colours(), "{icon:?} must follow the theme tint");
    }
    assert_eq!(Icon::Pi.path(), "icons/agent-pi.svg");
    assert!(!Icon::Pi.has_own_colours());
}
```

Keep `chromatic_marks_carry_their_own_colours`, but update its comments so they say Oh My Pi is the sole full-color fallback and remove references to Comet's Claude mark.

- [ ] **Step 3: Run the focused test and retain RED evidence**

Run:

```bash
cd rust
cargo test -p tiller_ui icons::tests
```

Expected: FAIL in `catalog_uses_the_approved_zed_and_tiller_assets`; the current path is `icons/comet/folder.svg` instead of `icons/zed/folder.svg`. This failure proves the test distinguishes the old catalog from the requested catalog.

- [ ] **Step 4: Import the exact pinned upstream files**

Run from the repository root:

```bash
zed_icon_commit=875e2a1c458ffcf2bfe90be841eb9893f399b4ff
zed_icon_names=(
  ai_claude ai_open_ai ai_open_code archive chat chevron_down chevron_left
  chevron_right close file folder git_branch lock plus public rotate_cw screen
  settings sparkle terminal threads_sidebar_left_open threads_sidebar_right_open
)
mkdir -p rust/assets/icons/zed
for icon_name in "${zed_icon_names[@]}"; do
  curl --fail --location --silent --show-error \
    "https://raw.githubusercontent.com/zed-industries/zed/${zed_icon_commit}/assets/icons/${icon_name}.svg" \
    --output "rust/assets/icons/zed/${icon_name}.svg"
done
curl --fail --location --silent --show-error \
  "https://raw.githubusercontent.com/zed-industries/zed/${zed_icon_commit}/assets/icons/LICENSES" \
  --output rust/assets/icons/zed/LICENSES
```

Create `rust/assets/icons/zed/ATTRIBUTION.md` with this content:

```markdown
# Zed icon attribution

The SVG files in this directory were copied byte-for-byte from
`zed-industries/zed` commit
`875e2a1c458ffcf2bfe90be841eb9893f399b4ff`:

https://github.com/zed-industries/zed/tree/875e2a1c458ffcf2bfe90be841eb9893f399b4ff/assets/icons

The accompanying `LICENSES` file is the license notice distributed by Zed
for this icon directory. The icon geometry must remain unchanged; update the
commit and provenance together when refreshing these files.

Agent logos are used nominatively to identify their respective products and
remain trademarks of their owners.
```

- [ ] **Step 5: Rewire `Icon::path`, `Icon::svg`, and `TillerAssets`**

Replace both methods with these complete mappings:

```rust
pub fn path(self) -> &'static str {
    match self {
        Icon::FolderFill => "icons/zed/folder.svg",
        Icon::GitBranch => "icons/zed/git_branch.svg",
        Icon::MessageSquare => "icons/zed/chat.svg",
        Icon::SquareTerminal => "icons/zed/terminal.svg",
        Icon::Close => "icons/zed/close.svg",
        Icon::ChevronDown => "icons/zed/chevron_down.svg",
        Icon::ChevronRight => "icons/zed/chevron_right.svg",
        Icon::ChevronLeft => "icons/zed/chevron_left.svg",
        Icon::Settings => "icons/zed/settings.svg",
        Icon::RefreshCw => "icons/zed/rotate_cw.svg",
        Icon::Plus => "icons/zed/plus.svg",
        Icon::File => "icons/zed/file.svg",
        Icon::Sparkles => "icons/zed/sparkle.svg",
        Icon::Shield => "icons/zed/lock.svg",
        Icon::SunMoon => "icons/zed/screen.svg",
        Icon::Globe => "icons/zed/public.svg",
        Icon::ClaudeCode => "icons/zed/ai_claude.svg",
        Icon::Codex => "icons/zed/ai_open_ai.svg",
        Icon::OpenCode => "icons/zed/ai_open_code.svg",
        Icon::Pi => "icons/agent-pi.svg",
        Icon::OhMyPi => "icons/agent-omp.svg",
        Icon::SidebarLeft => "icons/zed/threads_sidebar_left_open.svg",
        Icon::PanelRight => "icons/zed/threads_sidebar_right_open.svg",
        Icon::Archive => "icons/zed/archive.svg",
        Icon::Lock => "icons/zed/lock.svg",
    }
}

pub fn svg(self) -> &'static [u8] {
    match self {
        Icon::FolderFill => include_bytes!("../../../assets/icons/zed/folder.svg"),
        Icon::GitBranch => include_bytes!("../../../assets/icons/zed/git_branch.svg"),
        Icon::MessageSquare => include_bytes!("../../../assets/icons/zed/chat.svg"),
        Icon::SquareTerminal => include_bytes!("../../../assets/icons/zed/terminal.svg"),
        Icon::Close => include_bytes!("../../../assets/icons/zed/close.svg"),
        Icon::ChevronDown => include_bytes!("../../../assets/icons/zed/chevron_down.svg"),
        Icon::ChevronRight => include_bytes!("../../../assets/icons/zed/chevron_right.svg"),
        Icon::ChevronLeft => include_bytes!("../../../assets/icons/zed/chevron_left.svg"),
        Icon::Settings => include_bytes!("../../../assets/icons/zed/settings.svg"),
        Icon::RefreshCw => include_bytes!("../../../assets/icons/zed/rotate_cw.svg"),
        Icon::Plus => include_bytes!("../../../assets/icons/zed/plus.svg"),
        Icon::File => include_bytes!("../../../assets/icons/zed/file.svg"),
        Icon::Sparkles => include_bytes!("../../../assets/icons/zed/sparkle.svg"),
        Icon::Shield => include_bytes!("../../../assets/icons/zed/lock.svg"),
        Icon::SunMoon => include_bytes!("../../../assets/icons/zed/screen.svg"),
        Icon::Globe => include_bytes!("../../../assets/icons/zed/public.svg"),
        Icon::ClaudeCode => include_bytes!("../../../assets/icons/zed/ai_claude.svg"),
        Icon::Codex => include_bytes!("../../../assets/icons/zed/ai_open_ai.svg"),
        Icon::OpenCode => include_bytes!("../../../assets/icons/zed/ai_open_code.svg"),
        Icon::Pi => include_bytes!("../../../assets/icons/agent-pi.svg"),
        Icon::OhMyPi => include_bytes!("../../../assets/icons/agent-omp.svg"),
        Icon::SidebarLeft => {
            include_bytes!("../../../assets/icons/zed/threads_sidebar_left_open.svg")
        }
        Icon::PanelRight => {
            include_bytes!("../../../assets/icons/zed/threads_sidebar_right_open.svg")
        }
        Icon::Archive => include_bytes!("../../../assets/icons/zed/archive.svg"),
        Icon::Lock => include_bytes!("../../../assets/icons/zed/lock.svg"),
    }
}
```

Remove the duplicated path match from `TillerAssets::load` and resolve through the catalog:

```rust
fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
    Ok(ALL_ICONS
        .iter()
        .copied()
        .find(|icon| icon.path() == path)
        .map(|icon| Cow::Borrowed(icon.svg())))
}
```

Update module and variant comments to describe the Zed source and the two deliberate Tiller fallbacks. Do not change rendering or sizes in this task.

- [ ] **Step 6: Verify byte identity against the pinned source**

Run:

```bash
zed_icon_commit=875e2a1c458ffcf2bfe90be841eb9893f399b4ff
for local_icon in rust/assets/icons/zed/*.svg; do
  icon_name=${local_icon:t}
  curl --fail --location --silent --show-error \
    "https://raw.githubusercontent.com/zed-industries/zed/${zed_icon_commit}/assets/icons/${icon_name}" \
    | cmp - "${local_icon}"
done
curl --fail --location --silent --show-error \
  "https://raw.githubusercontent.com/zed-industries/zed/${zed_icon_commit}/assets/icons/LICENSES" \
  | cmp - rust/assets/icons/zed/LICENSES
```

Expected: no output and exit zero.

- [ ] **Step 7: Run the focused GREEN test**

Run:

```bash
cd rust
cargo test -p tiller_ui icons::tests
```

Expected: PASS with a non-zero icon-test count.

- [ ] **Step 8: Commit the catalog increment**

```bash
git add rust/assets/icons/zed rust/crates/tiller_ui/src/icons.rs
git diff --cached --check
git commit -m "feat: adopt zed icon assets"
```

### Task 2: Use stock GPUI SVG rendering and semantic sizes

**Files:**

- Modify: `rust/crates/tiller_ui/src/icons.rs:1-823`
- Modify: `rust/crates/tiller_ui/src/sidebar.rs:3410-3450`
- Delete: `rust/crates/tiller_ui/src/sfsymbol.rs`

- [ ] **Step 1: Write failing size and color-path tests**

Add these tests to `icons.rs`:

```rust
#[test]
fn semantic_icon_sizes_follow_tiller_typography_scale() {
    let default = tiller_theme::Typography::default_scale();
    assert_eq!(IconSize::XSmall.resolve(default), px(12.0));
    assert_eq!(IconSize::Small.resolve(default), px(14.0));
    assert_eq!(IconSize::Medium.resolve(default), px(16.0));
    assert_eq!(
        IconSize::Custom(px(32.0)).resolve(default),
        px(32.0)
    );

    let enlarged = tiller_theme::Typography::for_base_size(15.5);
    assert_eq!(IconSize::XSmall.resolve(enlarged), px(14.0));
    assert_eq!(IconSize::Small.resolve(enlarged), px(16.0));
    assert_eq!(IconSize::Medium.resolve(enlarged), px(18.0));
}

#[test]
fn only_oh_my_pi_uses_the_full_colour_path() {
    for icon in ALL_ICONS {
        assert_eq!(
            icon.has_own_colours(),
            icon == Icon::OhMyPi,
            "unexpected full-colour icon: {icon:?}"
        );
    }
}

#[test]
fn constructor_keeps_the_semantic_size_until_render() {
    let element = IconElement::new(Icon::FolderFill, IconSize::Small);
    assert_eq!(element.size, IconSize::Small);
}
```

Delete the old `layout_size_matches_constructor` test, which inspects the custom element's removed `Interactivity` storage. Delete `sf_symbols_map_only_on_macos_and_never_for_marks`; the final API must not expose `system_symbol` on any target.

- [ ] **Step 2: Run the focused test and retain RED evidence**

Run:

```bash
cd rust
cargo test -p tiller_ui icons::tests
```

Expected: compilation FAIL because `IconSize` is not yet defined.

- [ ] **Step 3: Add `IconSize` with a temporary raw-pixel conversion**

Add this near `Icon`:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IconSize {
    XSmall,
    Small,
    Medium,
    Custom(Pixels),
}

impl IconSize {
    pub fn resolve(self, typography: tiller_theme::Typography) -> Pixels {
        let delta = f32::from(typography.base_size) - 13.5;
        match self {
            Self::XSmall => px((12.0 + delta).max(6.0)),
            Self::Small => px((14.0 + delta).max(6.0)),
            Self::Medium => px((16.0 + delta).max(6.0)),
            Self::Custom(size) => size,
        }
    }
}

// Transitional only: Tasks 3 and 4 remove every raw-pixel caller, then
// delete this impl so the compiler enforces semantic sizes.
impl From<Pixels> for IconSize {
    fn from(size: Pixels) -> Self {
        Self::Custom(size)
    }
}
```

Change `Icon::element` and `IconElement::new` temporarily to accept `impl Into<IconSize>` so existing callers stay green during the migration.

- [ ] **Step 4: Replace the custom generic painter with a `RenderOnce` adapter**

Replace `IconElement`'s custom `Element`/`InteractiveElement` implementation, `paint_tinted_svg`, and `paint_system_symbol` with this structure:

```rust
#[derive(IntoElement)]
pub struct IconElement {
    style: StyleRefinement,
    icon: Icon,
    size: IconSize,
}

impl IconElement {
    pub fn new(icon: Icon, size: impl Into<IconSize>) -> Self {
        Self {
            style: StyleRefinement::default(),
            icon,
            size: size.into(),
        }
    }
}

impl Styled for IconElement {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for IconElement {
    fn render(self, window: &mut Window, cx: &mut App) -> AnyElement {
        let size = self.size.resolve(tiller_theme::Theme::get(cx).typography);
        if self.icon.has_own_colours() {
            let icon = self.icon;
            let mut element = canvas(
                |_, _, _| {},
                move |bounds, _, window, cx| paint_agent_mark(icon, bounds, window, cx),
            )
            .size(size)
            .flex_none();
            element.style().refine(&self.style);
            element.into_any_element()
        } else {
            let mut element = svg()
                .size(size)
                .flex_none()
                .path(self.icon.path())
                .text_color(window.text_style().color);
            element.style().refine(&self.style);
            element.into_any_element()
        }
    }
}
```

Import `AnyElement`, `IntoElement`, `Refineable as _`, `RenderOnce`, `StyleRefinement`, `canvas`, `px`, and `svg`. Retain `paint_agent_mark`, its cache, and `view_box_size`; delete the generic painter, SF painter, `system_symbol`, the conditional module declaration, and SF-only imports.

Change `Icon::element` to delegate to the transitional constructor:

```rust
pub fn element(self, size: impl Into<IconSize>) -> IconElement {
    IconElement::new(self, size)
}
```

- [ ] **Step 5: Preserve sidebar hover visibility on the owning `Div`**

`group_hover` is a `Div` interaction, not a generic `Styled` method. Replace the disclosure branch in `sidebar.rs` with:

```rust
Some(icon) => div()
    .invisible()
    .group_hover(hover_group.clone(), |element| element.visible())
    .child(IconElement::new(icon, theme.typography.footnote).text_color(theme.meta))
    .into_any_element(),
```

This preserves the existing hover behavior while allowing `IconElement` to be a lightweight stock-SVG adapter.

- [ ] **Step 6: Delete the SF Symbol module and verify there is no source path left**

```bash
git rm rust/crates/tiller_ui/src/sfsymbol.rs
rg -n "system_symbol|sfsymbol|paint_system_symbol" rust/crates/tiller_ui/src
```

Expected: `rg` returns no matches.

- [ ] **Step 7: Run GREEN tests**

```bash
cd rust
cargo test -p tiller_ui icons::tests
cargo test -p tiller_ui
```

Expected: both commands PASS; the focused command reports a non-zero icon-test count.

- [ ] **Step 8: Commit the renderer increment**

```bash
git add rust/crates/tiller_ui/src/icons.rs rust/crates/tiller_ui/src/sidebar.rs
git add -u rust/crates/tiller_ui/src/sfsymbol.rs
git diff --cached --check
git commit -m "refactor: unify icon rendering across platforms"
```

### Task 3: Migrate `tiller_ui` callers to semantic sizes

**Files:**

- Modify: `rust/crates/tiller_ui/src/project_identity.rs:680-695`
- Modify: `rust/crates/tiller_ui/src/changes.rs:1110-1240`
- Modify: `rust/crates/tiller_ui/src/status_bar.rs:350-435`
- Modify: `rust/crates/tiller_ui/src/titlebar.rs:350-530`
- Modify: `rust/crates/tiller_ui/src/right_panel.rs:580-1200`
- Modify: `rust/crates/tiller_ui/src/settings.rs:2120-2220,3350-3390`
- Modify: `rust/crates/tiller_ui/src/chat.rs:4260-5585,6635-6660`
- Modify: `rust/crates/tiller_ui/src/tab_bar.rs:350-475,650-680`
- Modify: `rust/crates/tiller_ui/src/sidebar.rs:3270-3610`

- [ ] **Step 1: Import `IconSize` in every affected module**

Extend each existing `crate::icons::{...}` import rather than adding a second icon import. Keep the imports formatted by `cargo fmt`.

- [ ] **Step 2: Apply the approved semantic mapping**

Use this mapping for every `IconElement::new` or `icon.element` call in the listed files:

| File/context | Size |
| --- | --- |
| `project_identity.rs` picker glyph | `IconSize::Small` |
| `changes.rs` chevrons | `IconSize::XSmall` |
| `changes.rs` file glyph | `IconSize::Small` |
| `status_bar.rs` settings, refresh, and provider marks | `IconSize::XSmall` |
| `titlebar.rs` entire cluster | `IconSize::Medium` |
| `right_panel.rs` chevrons and close actions | `IconSize::XSmall` |
| `right_panel.rs` file/surface glyphs | `IconSize::Small` |
| `right_panel.rs` no-worktree illustration | `IconSize::Custom(theme.typography.large_title)` |
| `settings.rs` back and category glyphs | `IconSize::Small` |
| `settings.rs` summarizer chevron | `IconSize::XSmall` |
| `chat.rs` chevrons and add action | `IconSize::XSmall` |
| `chat.rs` file/attachment glyph | `IconSize::Small` |
| `tab_bar.rs` tab/menu/agent glyphs and add action | `IconSize::Small` |
| `tab_bar.rs` chevrons | `IconSize::XSmall` |
| `sidebar.rs` project, worktree, tab, and agent marks | `IconSize::Small` |
| `sidebar.rs` disclosures, settings, and close actions | `IconSize::XSmall` |

For example:

```rust
IconElement::new(Icon::ChevronRight, IconSize::XSmall).text_color(theme.meta)
IconElement::new(category.glyph(), IconSize::Small).text_color(theme.title)
icon.element(IconSize::Medium).text_color(color)
```

Keep emoji and local PNG sizing unchanged; they are not SVG catalog icons.

- [ ] **Step 3: Inspect every remaining Tiller UI call site**

Run:

```bash
rg -n "IconElement::new|icon\.element\(" rust/crates/tiller_ui/src
```

Expected: every SVG call in the nine modified modules passes `IconSize::XSmall`, `Small`, `Medium`, or an explicit `Custom(...)`. The only raw-pixel compatibility callers remaining are in `rust/crates/tiller/src/main.rs`.

- [ ] **Step 4: Format and test the UI crate**

```bash
cd rust
cargo fmt --all
cargo test -p tiller_ui
```

Expected: PASS.

- [ ] **Step 5: Commit the UI migration**

```bash
git add rust/crates/tiller_ui/src/{project_identity,changes,status_bar,titlebar,right_panel,settings,chat,tab_bar,sidebar}.rs
git diff --cached --check
git commit -m "refactor: apply semantic icon sizes in ui"
```

### Task 4: Migrate the app shell and make semantic sizes mandatory

**Files:**

- Modify: `rust/crates/tiller/src/main.rs:9025-9040,9230-9250,9320-9340,10200-10220,10280-10300`
- Modify: `rust/crates/tiller_ui/src/icons.rs:140-340`

- [ ] **Step 1: Import `IconSize` into the app shell**

Change the existing import to:

```rust
icons::{Icon, IconElement, IconSize},
```

- [ ] **Step 2: Replace the five raw app-shell sizes**

Use these exact mappings:

```rust
IconElement::new(Icon::SquareTerminal, IconSize::Custom(px(32.0)))
IconElement::new(icon, IconSize::Small)
IconElement::new(Icon::Close, IconSize::XSmall)
IconElement::new(Icon::ChevronDown, IconSize::XSmall)
IconElement::new(Icon::SquareTerminal, IconSize::Custom(px(32.0)))
```

Preserve all existing colors, handlers, and surrounding layout.

- [ ] **Step 3: Remove the temporary raw-pixel compatibility API**

Delete `impl From<Pixels> for IconSize`. Make both constructors strict:

```rust
pub fn element(self, size: IconSize) -> IconElement {
    IconElement::new(self, size)
}

pub fn new(icon: Icon, size: IconSize) -> Self {
    Self {
        style: StyleRefinement::default(),
        icon,
        size,
    }
}
```

The next compile is the enforcement check: any missed raw `Pixels` or typography token produces a type error.

- [ ] **Step 4: Verify the strict API across both crates**

```bash
cd rust
cargo test -p tiller_ui
cargo test -p tiller
```

Expected: both PASS. Any type mismatch identifies a missed semantic-size call site and must be corrected before continuing.

- [ ] **Step 5: Confirm no icon is sized directly from typography**

```bash
rg -n -U "IconElement::new\([\s\S]{0,160}(typography\.|px\()|icon\.element\([\s\S]{0,80}(typography\.|px\()" \
  rust/crates/tiller_ui/src rust/crates/tiller/src/main.rs
```

Expected: no matches. `IconSize::Custom(px(32.0))` is allowed; if it appears in this broad search, inspect it and record that it is one of the two approved empty-state uses.

- [ ] **Step 6: Commit the strict semantic API**

```bash
git add rust/crates/tiller/src/main.rs rust/crates/tiller_ui/src/icons.rs
git diff --cached --check
git commit -m "refactor: require semantic icon sizes"
```

### Task 5: Remove superseded assets and update notices

**Files:**

- Modify: `THIRD_PARTY_NOTICES.md:1-130`
- Modify: `rust/crates/tiller_ui/src/right_panel.rs:1240-1275`
- Modify: `rust/crates/tiller_ui/src/status_bar.rs:465-485`
- Delete: `rust/assets/icons/comet/`
- Delete: the explicit root asset list below

- [ ] **Step 1: Prove the old assets have no live source references**

Run:

```bash
rg -n "icons/comet|sparkle-thin|shield-thin|sun-dim-thin|agent-(claude|claudecode|codex|opencode)" \
  rust/crates rust/Cargo.toml Scripts
```

Expected: only stale prose comments in `right_panel.rs` and `status_bar.rs`; no `include_bytes!`, asset path, or executable reference. Update those comments to describe the Zed catalog before deleting files.

- [ ] **Step 2: Delete only the confirmed superseded assets**

Run:

```bash
git rm -r rust/assets/icons/comet
git rm \
  rust/assets/icons/arrow-clockwise-thin.svg \
  rust/assets/icons/caret-down-thin.svg \
  rust/assets/icons/caret-left-thin.svg \
  rust/assets/icons/caret-right-thin.svg \
  rust/assets/icons/chat-circle-thin.svg \
  rust/assets/icons/file-thin.svg \
  rust/assets/icons/folder-fill.svg \
  rust/assets/icons/gear-thin.svg \
  rust/assets/icons/git-branch-thin.svg \
  rust/assets/icons/globe-thin.svg \
  rust/assets/icons/plus-thin.svg \
  rust/assets/icons/shield-thin.svg \
  rust/assets/icons/sparkle-thin.svg \
  rust/assets/icons/sun-dim-thin.svg \
  rust/assets/icons/terminal-window-thin.svg \
  rust/assets/icons/x-thin.svg \
  rust/assets/icons/agent-claude.svg \
  rust/assets/icons/agent-claudecode.svg \
  rust/assets/icons/agent-codex.svg \
  rust/assets/icons/agent-opencode.svg
```

Do not delete `rust/assets/icons/agent-pi.svg` or `rust/assets/icons/agent-omp.svg`.

- [ ] **Step 3: Update third-party notices**

Replace the live Comet section with this section:

```markdown
## Zed icons

The UI icons in `rust/assets/icons/zed/` were copied byte-for-byte from
[`zed-industries/zed`](https://github.com/zed-industries/zed) commit
`875e2a1c458ffcf2bfe90be841eb9893f399b4ff`. Full file-level provenance is
recorded in `rust/assets/icons/zed/ATTRIBUTION.md`; the upstream icon license
is copied in `rust/assets/icons/zed/LICENSES`.

> Lucide License
>
> ISC License
>
> Copyright (c) for portions of Lucide are held by Cole Bemis 2013-2022 as
> part of Feather (MIT). All other copyright (c) for Lucide are held by
> Lucide Contributors 2022.
>
> Permission to use, copy, modify, and/or distribute this software for any
> purpose with or without fee is hereby granted, provided that the above
> copyright notice and this permission notice appear in all copies.
>
> THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
> WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
> MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY
> SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
> WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION
> OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN
> CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
```

Remove the live Phosphor section because no Phosphor SVG remains bundled. In the Material Icon Theme historical paragraph, replace “the Linux app renders file-type icons from the comet set above instead” with “the current app renders its generic file glyphs from the pinned Zed set above instead.”

Replace the Simple Icons/SVG Logos live-asset paragraphs with this accurate statement:

```markdown
## Agent marks

The Claude, OpenAI, and OpenCode marks are the pinned SVG assets documented
in `rust/assets/icons/zed/ATTRIBUTION.md`. The Pi and Oh-My-Pi marks in
`rust/assets/icons/agent-pi.svg` and `rust/assets/icons/agent-omp.svg` are
project-owned SVG translations of the vector coordinates formerly stored in
`App/AgentIcon.swift`, readable at commit `5430d7bf`.
```

Retain the existing trademark notice.

- [ ] **Step 4: Verify asset reachability and tests after deletion**

```bash
rg -n "icons/comet|sparkle-thin|shield-thin|sun-dim-thin" \
  rust/crates rust/Cargo.toml Scripts THIRD_PARTY_NOTICES.md
cd rust
cargo test -p tiller_ui icons::tests
cargo test -p tiller
```

Expected: `rg` has no matches; both test commands PASS. References in historical `docs/linux-rewrite/` files and the approved design document are intentionally retained.

- [ ] **Step 5: Commit cleanup and notices**

```bash
git add THIRD_PARTY_NOTICES.md rust/crates/tiller_ui/src/right_panel.rs rust/crates/tiller_ui/src/status_bar.rs
git add -u rust/assets/icons
git diff --cached --check
git commit -m "chore: remove superseded icon assets"
```

### Task 6: Run repository gates and inspect the real UI

**Files:**

- Verify only; do not commit screenshots or generated build output.

- [ ] **Step 1: Check formatting and final diff hygiene**

```bash
cd rust
cargo fmt --all -- --check
cd ..
git diff --check
git status --short
```

Expected: formatting and diff checks exit zero. `git status` contains no unexpected file; `.superpowers/` remains ignored.

- [ ] **Step 2: Run the required repository gate**

```bash
Scripts/ci.sh
```

Expected: exit zero and literal final marker `CI OK`. Report the command and marker exactly.

- [ ] **Step 3: Run the comprehensive Linux gate**

```bash
Scripts/ci-linux.sh
```

Expected: every executed stage passes. Report any documented `SKIP` or `BLOCKED` stage as such; do not convert it into a pass or describe this command as fully green when a stage did not run.

- [ ] **Step 4: Launch the production UI path**

At execution time, load the `computer-use` skill before interacting with the native app. Run:

```bash
Scripts/build-dev.sh
```

Expected: build succeeds and a real Tiller window launches. A successful build alone is not visual verification.

- [ ] **Step 5: Inspect the approved surfaces in light and dark themes**

Using the real app, inspect and capture evidence for:

1. titlebar sidebar/back/forward/add/right-panel controls at 16 px;
2. project sidebar rows and agent marks at 14 px;
3. tab and file-tree glyphs at 14 px;
4. disclosure, close, status settings, and refresh controls at 12 px;
5. Settings categories, including `lock` for Permissions and `screen` for Appearance;
6. Zed Claude/OpenAI/OpenCode marks and Tiller Pi/Oh My Pi marks;
7. Oh My Pi's preserved gradient;
8. unchanged hover, click, focus, and tooltip behavior for icon-only controls.

Repeat in light and dark themes. If the current environment exposes a non-default UI base size, inspect one enlarged setting and confirm XSmall/Small/Medium advance by the same typography delta. Save temporary screenshots outside tracked paths or under ignored `.superpowers/`.

- [ ] **Step 6: Record final scope and evidence**

```bash
git status --short --branch
git log --oneline -5
git diff HEAD~5..HEAD --stat
```

Report:

- exact focused RED failure;
- focused GREEN test command and count;
- `tiller_ui`, `tiller`, `Scripts/ci.sh`, and `Scripts/ci-linux.sh` outcomes;
- actual visual surfaces and themes observed;
- any unavailable platform or scale as unverified;
- final commit list and changed-file scope;
- residual warnings or documented skips.
