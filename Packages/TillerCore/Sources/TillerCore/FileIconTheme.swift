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

    /// Resolves a key to a renderable ref for this theme. Material keys without
    /// a bundled asset fall back to the SF Symbols mapping, so a ref is never
    /// missing; `.symlink` intentionally resolves to the system "link" symbol
    /// in every theme.
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
