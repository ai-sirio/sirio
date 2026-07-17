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
