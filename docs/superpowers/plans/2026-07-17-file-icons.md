# File-Type Icons in Files Sidebar — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** File-type-specific icons in the right-sidebar Files explorer, with a Settings picker between two themes: SF Symbols (default, monochrome) and Material (colored, bundled SVGs from Iconify's `material-icon-theme`).

**Architecture:** Two-stage pure mapping in TillerCore (`file/dir name → FileIconKey → FileIconRef`), colored SVGs committed to `App/Assets.xcassets` at build time, theme choice persisted via `@AppStorage` and read in `FileExplorerView`.

**Tech Stack:** Swift 6, SwiftUI, swift-testing (`@Test`/`#expect`), Xcode asset catalog with SVG + preserve-vector-data, Iconify HTTP API (build-time only, icons committed).

**Spec:** `docs/superpowers/specs/2026-07-17-file-icons-design.md`

## Global Constraints

- macOS 15+, Swift 6, SwiftUI app; pure logic goes in `Packages/TillerCore`, never in `App/` (repo convention).
- Tests first, swift-testing (`@Test` / `#expect`), NOT XCTest.
- No runtime network fetches — icons are committed assets.
- Commit messages: Conventional Commits, lower-case imperative subject.
- `Scripts/ci.sh` must print `CI OK` before the work is considered done. Known flake: TillerTerminal PTY tests may need up to 5-6 retries — rerun until `CI OK`; only investigate if a *TillerCore* or icon-related test fails.
- Asset naming: `mat-<iconify-icon-name>` (e.g. `mat-swift`, `mat-folder-src`).
- Unknown extension → generic file icon; unmapped key in material theme → SF Symbol fallback. Never a blank image.

---

### Task 1: `FileIconKey` mapping in TillerCore

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/FileIconKey.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/FileIconKeyTests.swift`

**Interfaces:**
- Consumes: nothing (leaf).
- Produces: `public enum FileIconKey: Sendable, Hashable, CaseIterable` with
  `static func key(forFileName: String) -> FileIconKey` and
  `static func key(forDirectoryName: String) -> FileIconKey`. Task 2 iterates
  `FileIconKey.allCases`; Task 5 calls both static functions.

- [ ] **Step 1: Write the failing test**

Create `Packages/TillerCore/Tests/TillerCoreTests/FileIconKeyTests.swift`:

```swift
import Testing
import TillerCore

struct FileIconKeyTests {
    @Test func knownExtensionsMap() {
        #expect(FileIconKey.key(forFileName: "main.swift") == .swift)
        #expect(FileIconKey.key(forFileName: "app.tsx") == .react)
        #expect(FileIconKey.key(forFileName: "styles.scss") == .sass)
        #expect(FileIconKey.key(forFileName: "notes.md") == .markdown)
    }

    @Test func extensionMatchIsCaseInsensitive() {
        #expect(FileIconKey.key(forFileName: "photo.JPEG") == .image)
        #expect(FileIconKey.key(forFileName: "Main.SWIFT") == .swift)
    }

    @Test func exactFileNamesBeatExtensions() {
        #expect(FileIconKey.key(forFileName: "Dockerfile") == .docker)
        #expect(FileIconKey.key(forFileName: "Makefile") == .makefile)
        #expect(FileIconKey.key(forFileName: ".gitignore") == .git)
        #expect(FileIconKey.key(forFileName: ".env") == .env)
    }

    @Test func unknownFileFallsBackToGenericFile() {
        #expect(FileIconKey.key(forFileName: "data.xyzabc") == .file)
        #expect(FileIconKey.key(forFileName: "noextension") == .file)
    }

    @Test func specialDirectoriesMap() {
        #expect(FileIconKey.key(forDirectoryName: "src") == .folderSrc)
        #expect(FileIconKey.key(forDirectoryName: "Tests") == .folderTests)
        #expect(FileIconKey.key(forDirectoryName: ".github") == .folderGithub)
        #expect(FileIconKey.key(forDirectoryName: "node_modules") == .folderNodeModules)
    }

    @Test func unknownDirectoryFallsBackToGenericFolder() {
        #expect(FileIconKey.key(forDirectoryName: "Whatever") == .folder)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerCore && swift test --filter FileIconKeyTests`
Expected: compile FAILURE — `cannot find 'FileIconKey' in scope`.

- [ ] **Step 3: Write the implementation**

Create `Packages/TillerCore/Sources/TillerCore/FileIconKey.swift`:

```swift
import Foundation

/// Logical icon identity for a file-tree entry. Two-stage lookup so adding an
/// icon theme costs one table, not one per extension: name -> FileIconKey here,
/// FileIconKey -> renderable ref in FileIconTheme.
public enum FileIconKey: Sendable, Hashable, CaseIterable {
    // Files
    case swift, c, cpp, csharp, java, kotlin, python, ruby, rust, go
    case javascript, typescript, react, vue
    case html, css, sass
    case json, yaml, toml, xml, markdown, text, pdf
    case image, video, audio, font, archive
    case shell, sql, database, docker, git, log, env, settings, lock, makefile
    // Folders
    case folder
    case folderSrc, folderTests, folderDocs, folderGithub, folderNodeModules
    case folderDist, folderScripts, folderConfig, folderAssets, folderPublic
    case folderPackages, folderVscode, folderGit, folderLib, folderTools
    // Fallbacks
    case file, symlink

    public static func key(forFileName name: String) -> FileIconKey {
        let lower = name.lowercased()
        if let exact = exactFileNames[lower] { return exact }
        let ext = (lower as NSString).pathExtension
        guard !ext.isEmpty else { return .file }
        return fileExtensions[ext] ?? .file
    }

    public static func key(forDirectoryName name: String) -> FileIconKey {
        directoryNames[name.lowercased()] ?? .folder
    }

    private static let exactFileNames: [String: FileIconKey] = [
        "dockerfile": .docker,
        "makefile": .makefile,
        ".gitignore": .git,
        ".gitattributes": .git,
        ".gitmodules": .git,
        ".env": .env,
        "license": .text,
    ]

    private static let fileExtensions: [String: FileIconKey] = [
        "swift": .swift,
        "c": .c, "h": .c,
        "cpp": .cpp, "cc": .cpp, "cxx": .cpp, "hpp": .cpp,
        "cs": .csharp,
        "java": .java,
        "kt": .kotlin, "kts": .kotlin,
        "py": .python,
        "rb": .ruby,
        "rs": .rust,
        "go": .go,
        "js": .javascript, "mjs": .javascript, "cjs": .javascript,
        "ts": .typescript, "mts": .typescript,
        "jsx": .react, "tsx": .react,
        "vue": .vue,
        "html": .html, "htm": .html,
        "css": .css,
        "scss": .sass, "sass": .sass, "less": .sass,
        "json": .json, "jsonc": .json,
        "yml": .yaml, "yaml": .yaml,
        "toml": .toml,
        "xml": .xml, "plist": .xml,
        "md": .markdown, "markdown": .markdown,
        "txt": .text,
        "pdf": .pdf,
        "png": .image, "jpg": .image, "jpeg": .image, "gif": .image,
        "svg": .image, "webp": .image, "heic": .image, "ico": .image,
        "icns": .image,
        "mp4": .video, "mov": .video, "mkv": .video, "avi": .video,
        "mp3": .audio, "wav": .audio, "aac": .audio, "flac": .audio,
        "m4a": .audio,
        "ttf": .font, "otf": .font, "woff": .font, "woff2": .font,
        "zip": .archive, "tar": .archive, "gz": .archive, "tgz": .archive,
        "rar": .archive, "7z": .archive, "dmg": .archive,
        "sh": .shell, "bash": .shell, "zsh": .shell, "fish": .shell,
        "sql": .sql,
        "sqlite": .database, "db": .database,
        "log": .log,
        "env": .env,
        "lock": .lock,
        "conf": .settings, "ini": .settings, "cfg": .settings,
    ]

    private static let directoryNames: [String: FileIconKey] = [
        "src": .folderSrc, "sources": .folderSrc,
        "test": .folderTests, "tests": .folderTests,
        "__tests__": .folderTests, "spec": .folderTests,
        "docs": .folderDocs, "doc": .folderDocs,
        ".github": .folderGithub,
        "node_modules": .folderNodeModules,
        "dist": .folderDist, "build": .folderDist, "out": .folderDist,
        "scripts": .folderScripts, "script": .folderScripts,
        "config": .folderConfig, ".config": .folderConfig,
        "configs": .folderConfig,
        "assets": .folderAssets, "resources": .folderAssets,
        "public": .folderPublic,
        "packages": .folderPackages,
        ".vscode": .folderVscode,
        ".git": .folderGit,
        "lib": .folderLib, "libs": .folderLib,
        "tools": .folderTools,
    ]
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerCore && swift test --filter FileIconKeyTests`
Expected: all 6 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/FileIconKey.swift \
        Packages/TillerCore/Tests/TillerCoreTests/FileIconKeyTests.swift
git commit -m "feat: add FileIconKey mapping for file explorer icons"
```

---

### Task 2: `FileIconTheme` + `FileIconRef` in TillerCore

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/FileIconTheme.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/FileIconThemeTests.swift`

**Interfaces:**
- Consumes: `FileIconKey` (Task 1).
- Produces: `public enum FileIconRef: Equatable, Sendable { case system(String); case asset(String) }` and
  `public enum FileIconTheme: String, CaseIterable, Sendable { case sfSymbols, material }` with
  `public var title: String` and `public func iconRef(for key: FileIconKey) -> FileIconRef`.
  Task 4 uses `FileIconTheme.allCases`/`title`/`rawValue`; Task 5 calls `iconRef(for:)`.
  The asset names in `materialAssets` are the contract for Task 3's downloads.

- [ ] **Step 1: Write the failing test**

Create `Packages/TillerCore/Tests/TillerCoreTests/FileIconThemeTests.swift`:

```swift
import Testing
import TillerCore

struct FileIconThemeTests {
    @Test func sfSymbolsThemeIsAlwaysSystem() {
        for key in FileIconKey.allCases {
            guard case .system(let name) = FileIconTheme.sfSymbols.iconRef(for: key) else {
                Issue.record("expected .system for \(key)")
                return
            }
            #expect(!name.isEmpty)
        }
    }

    @Test func materialThemeUsesAssetsForMappedKeys() {
        #expect(FileIconTheme.material.iconRef(for: .swift) == .asset("mat-swift"))
        #expect(FileIconTheme.material.iconRef(for: .folderSrc) == .asset("mat-folder-src"))
        #expect(FileIconTheme.material.iconRef(for: .folder) == .asset("mat-folder-base"))
        #expect(FileIconTheme.material.iconRef(for: .file) == .asset("mat-document"))
    }

    @Test func materialThemeFallsBackToSystemForUnmappedKeys() {
        // symlink deliberately has no material asset (spec: link stays SF in both themes)
        #expect(FileIconTheme.material.iconRef(for: .symlink) == .system("link"))
    }

    @Test func everyKeyResolvesToNonEmptyRefInBothThemes() {
        for theme in FileIconTheme.allCases {
            for key in FileIconKey.allCases {
                switch theme.iconRef(for: key) {
                case .system(let name): #expect(!name.isEmpty)
                case .asset(let name): #expect(name.hasPrefix("mat-"))
                }
            }
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerCore && swift test --filter FileIconThemeTests`
Expected: compile FAILURE — `cannot find 'FileIconTheme' in scope`.

- [ ] **Step 3: Write the implementation**

Create `Packages/TillerCore/Sources/TillerCore/FileIconTheme.swift`:

```swift
import Foundation

/// A renderable icon reference resolved from a FileIconKey by a theme.
public enum FileIconRef: Equatable, Sendable {
    /// SF Symbol name; the view tints it with theme colors.
    case system(String)
    /// Asset Catalog image name; rendered in its own colors, no tint.
    case asset(String)
}

/// Icon theme for the Files explorer. Selected in Settings > Appearance.
public enum FileIconTheme: String, CaseIterable, Sendable {
    case sfSymbols
    case material

    public var title: String {
        switch self {
        case .sfSymbols: "SF Symbols"
        case .material: "Material"
        }
    }

    public func iconRef(for key: FileIconKey) -> FileIconRef {
        switch self {
        case .sfSymbols:
            return .system(Self.sfSymbol(for: key))
        case .material:
            // Missing asset must degrade to monochrome, never to a blank.
            if let asset = Self.materialAssets[key] { return .asset(asset) }
            return .system(Self.sfSymbol(for: key))
        }
    }

    private static func sfSymbol(for key: FileIconKey) -> String {
        switch key {
        case .swift: "swift"
        case .c, .cpp, .csharp, .java, .kotlin, .python, .ruby, .rust, .go,
             .javascript, .typescript, .react, .vue, .html, .xml:
            "chevron.left.forwardslash.chevron.right"
        case .css, .sass: "paintbrush"
        case .json, .yaml, .toml: "curlybraces"
        case .markdown: "text.alignleft"
        case .text, .log: "doc.text"
        case .pdf: "doc.richtext"
        case .image: "photo"
        case .video: "film"
        case .audio: "music.note"
        case .font: "textformat"
        case .archive: "archivebox"
        case .shell: "terminal"
        case .sql, .database: "cylinder.split.1x2"
        case .docker: "shippingbox"
        case .git: "arrow.triangle.branch"
        case .env, .settings: "gearshape"
        case .lock: "lock"
        case .makefile: "hammer"
        case .file: "doc"
        case .symlink: "link"
        case .folder, .folderSrc, .folderTests, .folderDocs, .folderGithub,
             .folderNodeModules, .folderDist, .folderScripts, .folderConfig,
             .folderAssets, .folderPublic, .folderPackages, .folderVscode,
             .folderGit, .folderLib, .folderTools:
            "folder"
        }
    }

    /// Asset names follow "mat-<iconify-icon-name>" from the
    /// material-icon-theme set. .symlink intentionally absent.
    private static let materialAssets: [FileIconKey: String] = [
        .swift: "mat-swift", .c: "mat-c", .cpp: "mat-cpp",
        .csharp: "mat-csharp", .java: "mat-java", .kotlin: "mat-kotlin",
        .python: "mat-python", .ruby: "mat-ruby", .rust: "mat-rust",
        .go: "mat-go",
        .javascript: "mat-javascript", .typescript: "mat-typescript",
        .react: "mat-react", .vue: "mat-vue",
        .html: "mat-html", .css: "mat-css", .sass: "mat-sass",
        .json: "mat-json", .yaml: "mat-yaml", .toml: "mat-toml",
        .xml: "mat-xml", .markdown: "mat-markdown", .text: "mat-document",
        .pdf: "mat-pdf",
        .image: "mat-image", .video: "mat-video", .audio: "mat-audio",
        .font: "mat-font", .archive: "mat-zip",
        .shell: "mat-console", .sql: "mat-database",
        .database: "mat-database", .docker: "mat-docker", .git: "mat-git",
        .log: "mat-log", .env: "mat-tune", .settings: "mat-settings",
        .lock: "mat-lock", .makefile: "mat-makefile",
        .file: "mat-document",
        .folder: "mat-folder-base",
        .folderSrc: "mat-folder-src", .folderTests: "mat-folder-test",
        .folderDocs: "mat-folder-docs", .folderGithub: "mat-folder-github",
        .folderNodeModules: "mat-folder-node", .folderDist: "mat-folder-dist",
        .folderScripts: "mat-folder-scripts",
        .folderConfig: "mat-folder-config",
        .folderAssets: "mat-folder-resource",
        .folderPublic: "mat-folder-public",
        .folderPackages: "mat-folder-packages",
        .folderVscode: "mat-folder-vscode", .folderGit: "mat-folder-git",
        .folderLib: "mat-folder-lib", .folderTools: "mat-folder-tools",
    ]
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerCore && swift test --filter FileIconThemeTests`
Expected: all 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/FileIconTheme.swift \
        Packages/TillerCore/Tests/TillerCoreTests/FileIconThemeTests.swift
git commit -m "feat: add FileIconTheme with sf symbols and material mappings"
```

---

### Task 3: Bundle material-icon-theme SVGs in the asset catalog

**Files:**
- Create: `App/Assets.xcassets/FileIcons/` (group + 54 imagesets, generated by the script below)

**Interfaces:**
- Consumes: asset-name contract from Task 2's `materialAssets` (`mat-<icon>`).
- Produces: 54 asset catalog imagesets named exactly like `materialAssets` values. Task 5's `Image(name)` resolves them.

All 54 icon names below were verified to exist in the Iconify `material-icon-theme` set (2026-07-17). Icons are downloaded once and committed — no runtime fetches.

- [ ] **Step 1: Download SVGs and generate imagesets**

Run from the repo root:

```bash
set -euo pipefail
ICONS="swift c cpp csharp java kotlin python ruby rust go javascript typescript react vue html css sass json yaml toml xml markdown document pdf image video audio font zip console database docker git log tune settings lock makefile folder-base folder-src folder-test folder-docs folder-github folder-node folder-dist folder-scripts folder-config folder-resource folder-public folder-packages folder-vscode folder-git folder-lib folder-tools"
DEST="App/Assets.xcassets/FileIcons"
mkdir -p "$DEST"
cat > "$DEST/Contents.json" <<'EOF'
{
  "info" : { "author" : "xcode", "version" : 1 }
}
EOF
for icon in $ICONS; do
  dir="$DEST/mat-$icon.imageset"
  mkdir -p "$dir"
  curl -sf "https://api.iconify.design/material-icon-theme/$icon.svg" \
    -o "$dir/mat-$icon.svg"
  [ -s "$dir/mat-$icon.svg" ] || { echo "FAILED: $icon"; exit 1; }
  cat > "$dir/Contents.json" <<EOF
{
  "images" : [
    { "filename" : "mat-$icon.svg", "idiom" : "universal" }
  ],
  "info" : { "author" : "xcode", "version" : 1 },
  "properties" : { "preserves-vector-representation" : true }
}
EOF
done
echo "imagesets: $(ls -d "$DEST"/*.imageset | wc -l | tr -d ' ')"
```

Expected output: `imagesets: 54` (no `FAILED:` lines).

Note: the `FileIcons` group's `Contents.json` deliberately has NO
`"provides-namespace"` property — assets must stay addressable as `mat-swift`,
not `FileIcons/mat-swift`.

- [ ] **Step 2: Verify the catalog compiles**

Run: `xcodegen generate && xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug build 2>&1 | tail -5`
Expected: `BUILD SUCCEEDED`.

If actool rejects a specific SVG (unsupported SVG feature), convert only that
icon to PNG instead: render with `qlmanage -t -s 64 <file>.svg -o <dir>` (or
any SVG→PNG tool available), name it `mat-<icon>.png`, replace `"filename"` in
that imageset's `Contents.json` with the PNG name and drop the
`"properties"` block. Same asset name, no code impact.

- [ ] **Step 3: Commit**

```bash
git add App/Assets.xcassets/FileIcons
git commit -m "feat: bundle material-icon-theme svgs for file explorer"
```

---

### Task 4: Settings — theme key + Appearance picker

**Files:**
- Modify: `Packages/TillerCore/Sources/TillerCore/AppSettings.swift` (near line 60, next to the other `appearance.*` keys)
- Modify: `App/AppearanceSettingsView.swift`

**Interfaces:**
- Consumes: `FileIconTheme` (Task 2).
- Produces: `AppSettings.fileIconThemeKey` (= `"appearance.fileIconTheme"`). Task 5 reads it via `@AppStorage`.

- [ ] **Step 1: Add the settings key**

In `Packages/TillerCore/Sources/TillerCore/AppSettings.swift`, after the
`terminalFontSizeRange` line (~line 62), add:

```swift
/// Icon theme for the Files explorer in the right panel. Stores a
/// FileIconTheme raw value; default is sfSymbols.
public static let fileIconThemeKey = "appearance.fileIconTheme"
```

- [ ] **Step 2: Add the picker**

In `App/AppearanceSettingsView.swift`, add the property below the existing
`@AppStorage` properties:

```swift
@AppStorage(AppSettings.fileIconThemeKey) private var fileIconThemeRaw = FileIconTheme.sfSymbols.rawValue
```

and add a section after the existing `Section("Terminal") { ... }`:

```swift
Section("Files") {
    Picker("File icons", selection: $fileIconThemeRaw) {
        ForEach(FileIconTheme.allCases, id: \.rawValue) { theme in
            Text(theme.title).tag(theme.rawValue)
        }
    }
    .pickerStyle(.segmented)
}
```

- [ ] **Step 3: Verify it builds**

Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug build 2>&1 | tail -3`
Expected: `BUILD SUCCEEDED`.

- [ ] **Step 4: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/AppSettings.swift \
        App/AppearanceSettingsView.swift
git commit -m "feat: add file icon theme setting with appearance picker"
```

---

### Task 5: Wire themed icons into `FileExplorerView`

**Files:**
- Modify: `App/RightPanel/FileExplorerView.swift`

**Interfaces:**
- Consumes: `FileIconKey.key(forFileName:)`, `FileIconKey.key(forDirectoryName:)` (Task 1); `FileIconTheme.iconRef(for:)`, `FileIconRef` (Task 2); `AppSettings.fileIconThemeKey` (Task 4); assets (Task 3).
- Produces: user-visible feature; nothing downstream.

Note: `FileExplorerView.swift` has uncommitted working-tree changes from
earlier unrelated work. Do NOT revert them; edit on top and stage only this
file's icon changes together with them if they are already staged — otherwise
`git add` the file as-is (its current diff is part of a separate in-flight
change; if unsure, ask before committing this task).

- [ ] **Step 1: Add theme state to the view**

In `App/RightPanel/FileExplorerView.swift`, after
`@FocusState private var treeFocused: Bool` add:

```swift
@AppStorage(AppSettings.fileIconThemeKey) private var fileIconThemeRaw = FileIconTheme.sfSymbols.rawValue

private var iconTheme: FileIconTheme {
    FileIconTheme(rawValue: fileIconThemeRaw) ?? .sfSymbols
}
```

- [ ] **Step 2: Replace the icon rendering in `fileRow`**

Replace these lines inside `fileRow(_:)`:

```swift
Image(systemName: icon(for: node))
    .foregroundStyle(node.kind.isDirectory ? AppTheme.meta : AppTheme.subtitle)
    .frame(width: 14)
```

with:

```swift
iconView(for: node)
    .frame(width: 14)
```

- [ ] **Step 3: Replace `icon(for:)` with the themed lookup**

Delete the old helper:

```swift
private func icon(for node: FileTreeNode) -> String {
    switch node.kind {
    case .directory: "folder"
    case .symbolicLink: "link"
    case .file: "doc"
    }
}
```

and add in its place:

```swift
@ViewBuilder
private func iconView(for node: FileTreeNode) -> some View {
    switch iconTheme.iconRef(for: iconKey(for: node)) {
    case .system(let name):
        Image(systemName: name)
            .foregroundStyle(node.kind.isDirectory ? AppTheme.meta : AppTheme.subtitle)
    case .asset(let name):
        Image(name)
            .resizable()
            .scaledToFit()
            .frame(width: 14, height: 14)
    }
}

private func iconKey(for node: FileTreeNode) -> FileIconKey {
    switch node.kind {
    case .directory: FileIconKey.key(forDirectoryName: node.name)
    case .symbolicLink: .symlink
    case .file: FileIconKey.key(forFileName: node.name)
    }
}
```

- [ ] **Step 4: Build and run the full gate**

Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug build 2>&1 | tail -3`
Expected: `BUILD SUCCEEDED`.

Run: `Scripts/ci.sh`
Expected: `CI OK` (retry up to 5-6 times if only the known TillerTerminal PTY
flake fails; any FileIcon* or App-target failure must be fixed, not retried).

- [ ] **Step 5: Commit**

```bash
git add App/RightPanel/FileExplorerView.swift
git commit -m "feat: themed file-type icons in files explorer"
```

---

### Task 6: Manual visual check (human)

Not automatable — checklist for the user, to run in the app (⌘R):

- [ ] Files panel with default theme: SF Symbols per type (`.swift` → swift bird, `.json` → curly braces, unknown ext → generic doc).
- [ ] Settings → Appearance → Files → "Material": icons switch live to colored ones without reopening the panel.
- [ ] Special folders (`src`, `Tests`, `docs`, `.github`, `Scripts`) show dedicated colored folder icons in Material.
- [ ] Symlink rows show the `link` icon in both themes.
- [ ] Git status letters and colors on the right of rows unchanged.
