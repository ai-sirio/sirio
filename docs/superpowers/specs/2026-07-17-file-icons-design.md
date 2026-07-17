# File-type icons in the Files right sidebar

**Date:** 2026-07-17
**Status:** Approved

## Goal

Replace the three generic SF Symbols (`folder`/`link`/`doc`) in the right-sidebar
file explorer with file-type-specific icons, selectable between two themes in
Settings → Appearance:

- **SF Symbols** (default) — monochrome, tinted with `AppTheme` colors, zero assets.
- **Material Icon Theme** — colored SVG icons from the Iconify `material-icon-theme`
  set, bundled at build time.

Adding further themes later (vscode-icons, catppuccin) must be mechanical: new
theme case + new logical-key → asset-name table + new assets. No changes to the
extension mapping.

## Non-goals

- No runtime network fetches. Iconify MCP is a build-time tool only; icons are
  fetched once during implementation and committed as assets.
- No per-project or per-worktree icon overrides.
- No icon theming outside the Files explorer (sidebar, tabs, etc. unchanged).

## Architecture

### 1. Mapping layer — `TillerCore` (pure, tested)

Two-stage lookup so each theme costs one table, not one table per extension:

```
FileIconKey.swift
  static func key(forFileName: String) -> FileIconKey     // "Package.swift" -> .swift
  static func key(forDirectoryName: String) -> FileIconKey // "src" -> .folderSrc
```

- `FileIconKey` is an enum of logical keys: ~60 file keys (`swift`, `json`,
  `markdown`, `typescript`, …) + ~15 special-folder keys (`folderSrc`,
  `folderTests`, `folderDocs`, `folderGithub`, `folderNodeModules`, …) +
  fallbacks (`file`, `folder`, `symlink`).
- Extension match is case-insensitive on the last path extension; a small
  exact-filename table handles cases like `Dockerfile`, `Makefile`,
  `package.json`, `.gitignore`. Folder match is exact name, case-insensitive.
- Unknown extension → `.file`; unknown folder → `.folder`.

```
FileIconTheme.swift
  enum FileIconTheme: String, CaseIterable  // .sfSymbols, .material
  func iconRef(for key: FileIconKey) -> FileIconRef

  enum FileIconRef: Equatable {
    case system(String)   // SF Symbol name, tinted by the view
    case asset(String)    // Asset Catalog name, rendered as-is (colored)
  }
```

- `sfSymbols` maps every key to an SF Symbol (e.g. `.swift` → `"swift"`,
  `.json` → `"curlybraces"`, `.image` → `"photo"`, folders → `"folder"` or a
  close variant). Always `.system`.
- `material` maps keys to asset names with a `mat-` prefix (e.g. `.swift` →
  `"mat-swift"`, `.folderSrc` → `"mat-folder-src"`). Keys without a bundled
  asset fall back to the SF Symbols mapping — a missing asset never renders
  as an empty image.

Both files live in `Packages/TillerCore` because the logic is pure string
mapping with no SwiftUI dependency (per repo convention: logic that doesn't
need SwiftUI/AppKit goes in packages).

### 2. Assets — build-time via Iconify MCP

- Fetch the SVGs for the material mapping from the Iconify `material-icon-theme`
  set during implementation (via the Iconify MCP), one asset per mapped key
  that has a distinct icon (~60 files + ~15 folders).
- Store in `App/Assets.xcassets` as image sets with *Preserve Vector Data*,
  `mat-<key>` naming.
- Xcode's asset catalog supports an SVG subset; material icons are simple
  paths and are expected to render. Any icon that renders incorrectly is
  pre-converted to PNG @1x/@2x instead — same asset name, no code impact.
- `xcodegen generate` after adding assets is not required (assets are inside
  the existing `App/Assets.xcassets`), but run it anyway if new files outside
  the catalog are added.

### 3. Settings

- New key in `AppSettings` (TillerCore):
  `public static let fileIconThemeKey = "appearance.fileIconTheme"` with
  default `FileIconTheme.sfSymbols`.
- `AppearanceSettingsView` gains a `Section("Files")` with a `Picker`
  ("File icons") over `FileIconTheme.allCases`, bound via `@AppStorage`,
  matching the existing Theme picker pattern.

### 4. UI — `FileExplorerView`

- Read the theme via `@AppStorage(AppSettings.fileIconThemeKey)`.
- Replace `icon(for:)` with a lookup through
  `FileIconKey` + `FileIconTheme.iconRef(for:)`:
  - `.system(name)` → `Image(systemName: name)` tinted as today
    (`AppTheme.meta` for directories, `AppTheme.subtitle` for files).
  - `.asset(name)` → `Image(name)` with `.renderingMode(.original)`, resized
    to the current 14pt slot. No tint.
- Symlinks keep the `link` SF Symbol in both themes.
- Everything else in the row (chevron, git status, gestures) is untouched.

## Error handling

- Unknown extensions/folders: deterministic fallback to generic file/folder
  icon (handled in the mapping layer, tested).
- Missing asset for a mapped key in the material theme: theme-level fallback
  to the SF Symbols ref (also tested), so a bad asset name degrades to
  monochrome, never to a blank.

## Testing

- `TillerCore` swift-testing suite:
  - extension → key: known extensions, case-insensitivity, exact-filename
    overrides (`Dockerfile`, `package.json`), unknown → `.file`.
  - folder name → key: special folders, unknown → `.folder`.
  - theme mapping: every `FileIconKey` resolves to a non-empty ref in both
    themes; material fallback returns the SF Symbols ref for unmapped keys.
- `Scripts/ci.sh` must print `CI OK`.
- Manual visual check: both themes in the Files panel, theme switch applies
  live, special folders show dedicated icons.
