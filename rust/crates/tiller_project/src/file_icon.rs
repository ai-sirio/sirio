//! Logical icon identity for a file-tree entry, ported from
//! `Packages/TillerCore/Sources/TillerCore/FileIconKey.swift`: a two-stage
//! lookup so adding an icon theme costs one table, not one per extension.
//!
//! [`FileIconKey::for_file_name`] mirrors the Swift `key(forFileName:)`
//! exactly: an exact, case-insensitive full-filename match first
//! (`Dockerfile`, `.gitignore`, …), then a case-insensitive extension
//! lookup on `path.extension()` — which, like Foundation's
//! `NSString.pathExtension`, treats a name that starts with `.` and has no
//! *other* `.` as having no extension at all (`.bashrc` and `.editorconfig`
//! are therefore [`FileIconKey::File`], not shell or settings — the
//! original does not special-case them, and this is not an invented gap:
//! read `FileIconKey.swift` before "fixing" it). An unmatched name falls
//! back to [`FileIconKey::File`], exactly as the original falls back.
//!
//! [`FileIconKey::for_directory_name`] mirrors `key(forDirectoryName:)`: a
//! case-insensitive exact match against a fixed set of well-known directory
//! names, defaulting to [`FileIconKey::Folder`].

/// Logical icon identity for a file-tree entry. See the module docs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FileIconKey {
    // Files
    Swift,
    C,
    Cpp,
    CSharp,
    Java,
    Kotlin,
    Python,
    Ruby,
    Rust,
    Go,
    JavaScript,
    TypeScript,
    React,
    Vue,
    Html,
    Css,
    Sass,
    Json,
    Yaml,
    Toml,
    Xml,
    Markdown,
    Text,
    Pdf,
    Image,
    Video,
    Audio,
    Font,
    Archive,
    Shell,
    Sql,
    Database,
    Docker,
    Git,
    Log,
    Env,
    Settings,
    Lock,
    Makefile,
    // Folders
    Folder,
    FolderSrc,
    FolderTests,
    FolderDocs,
    FolderGithub,
    FolderNodeModules,
    FolderDist,
    FolderScripts,
    FolderConfig,
    FolderAssets,
    FolderPublic,
    FolderPackages,
    FolderVscode,
    FolderGit,
    FolderLib,
    FolderTools,
    // Fallbacks
    File,
    #[allow(dead_code)] // No table entry produces this — see the module docs.
    Symlink,
}

impl FileIconKey {
    /// Resolves a file's logical icon key from its name: exact-name table
    /// first, then case-insensitive extension lookup; unknown names fall
    /// back to `File`.
    pub fn for_file_name(name: &str) -> Self {
        let lower = name.to_lowercase();
        if let Some(exact) = exact_file_name(&lower) {
            return exact;
        }
        let Some(extension) = std::path::Path::new(&lower).extension() else {
            return Self::File;
        };
        file_extension(&extension.to_string_lossy()).unwrap_or(Self::File)
    }

    /// Resolves a directory's logical icon key from its name
    /// (case-insensitive); unknown names fall back to `Folder`.
    pub fn for_directory_name(name: &str) -> Self {
        directory_name(&name.to_lowercase()).unwrap_or(Self::Folder)
    }
}

fn exact_file_name(lower: &str) -> Option<FileIconKey> {
    use FileIconKey::*;
    Some(match lower {
        "dockerfile" => Docker,
        "makefile" => Makefile,
        ".gitignore" | ".gitattributes" | ".gitmodules" => Git,
        ".env" => Env,
        "license" => Text,
        _ => return None,
    })
}

fn file_extension(extension: &str) -> Option<FileIconKey> {
    use FileIconKey::*;
    Some(match extension {
        "swift" => Swift,
        "c" | "h" => C,
        "cpp" | "cc" | "cxx" | "hpp" => Cpp,
        "cs" => CSharp,
        "java" => Java,
        "kt" | "kts" => Kotlin,
        "py" => Python,
        "rb" => Ruby,
        "rs" => Rust,
        "go" => Go,
        "js" | "mjs" | "cjs" => JavaScript,
        "ts" | "mts" => TypeScript,
        "jsx" | "tsx" => React,
        "vue" => Vue,
        "html" | "htm" => Html,
        "css" => Css,
        "scss" | "sass" | "less" => Sass,
        "json" | "jsonc" => Json,
        "yml" | "yaml" => Yaml,
        "toml" => Toml,
        "xml" | "plist" => Xml,
        "md" | "markdown" => Markdown,
        "txt" => Text,
        "pdf" => Pdf,
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "heic" | "ico" | "icns" => Image,
        "mp4" | "mov" | "mkv" | "avi" => Video,
        "mp3" | "wav" | "aac" | "flac" | "m4a" => Audio,
        "ttf" | "otf" | "woff" | "woff2" => Font,
        "zip" | "tar" | "gz" | "tgz" | "rar" | "7z" | "dmg" => Archive,
        "sh" | "bash" | "zsh" | "fish" => Shell,
        "sql" => Sql,
        "sqlite" | "db" => Database,
        "log" => Log,
        "env" => Env,
        "lock" => Lock,
        "conf" | "ini" | "cfg" => Settings,
        _ => return None,
    })
}

fn directory_name(lower: &str) -> Option<FileIconKey> {
    use FileIconKey::*;
    Some(match lower {
        "src" | "sources" => FolderSrc,
        "test" | "tests" | "__tests__" | "spec" => FolderTests,
        "docs" | "doc" => FolderDocs,
        ".github" => FolderGithub,
        "node_modules" => FolderNodeModules,
        "dist" | "build" | "out" => FolderDist,
        "scripts" | "script" => FolderScripts,
        "config" | ".config" | "configs" => FolderConfig,
        "assets" | "resources" => FolderAssets,
        "public" => FolderPublic,
        "packages" => FolderPackages,
        ".vscode" => FolderVscode,
        ".git" => FolderGit,
        "lib" | "libs" => FolderLib,
        "tools" => FolderTools,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Table-driven against `FileIconKey.swift`'s three tables, read
    /// verbatim rather than reconstructed from what "seems right" for a
    /// file-tree icon.
    #[test]
    fn for_file_name_matches_the_original_exact_and_extension_tables() {
        let cases: &[(&str, FileIconKey)] = &[
            // Exact names (case-insensitive).
            ("Dockerfile", FileIconKey::Docker),
            ("dockerfile", FileIconKey::Docker),
            ("Makefile", FileIconKey::Makefile),
            (".gitignore", FileIconKey::Git),
            (".GitIgnore", FileIconKey::Git),
            (".gitattributes", FileIconKey::Git),
            (".gitmodules", FileIconKey::Git),
            (".env", FileIconKey::Env),
            ("LICENSE", FileIconKey::Text),
            ("license", FileIconKey::Text),
            // Extensions, one per language/kind family.
            ("main.swift", FileIconKey::Swift),
            ("a.c", FileIconKey::C),
            ("a.h", FileIconKey::C),
            ("a.cpp", FileIconKey::Cpp),
            ("a.hpp", FileIconKey::Cpp),
            ("a.cs", FileIconKey::CSharp),
            ("Main.java", FileIconKey::Java),
            ("a.kt", FileIconKey::Kotlin),
            ("a.py", FileIconKey::Python),
            ("a.rb", FileIconKey::Ruby),
            ("a.rs", FileIconKey::Rust),
            ("a.go", FileIconKey::Go),
            ("a.js", FileIconKey::JavaScript),
            ("a.mjs", FileIconKey::JavaScript),
            ("a.ts", FileIconKey::TypeScript),
            ("a.jsx", FileIconKey::React),
            ("a.tsx", FileIconKey::React),
            ("a.vue", FileIconKey::Vue),
            ("index.html", FileIconKey::Html),
            ("a.css", FileIconKey::Css),
            ("a.scss", FileIconKey::Sass),
            ("a.json", FileIconKey::Json),
            ("a.yml", FileIconKey::Yaml),
            ("a.yaml", FileIconKey::Yaml),
            ("Cargo.toml", FileIconKey::Toml),
            ("a.xml", FileIconKey::Xml),
            ("Info.plist", FileIconKey::Xml),
            ("README.md", FileIconKey::Markdown),
            ("notes.txt", FileIconKey::Text),
            ("doc.pdf", FileIconKey::Pdf),
            ("shot.PNG", FileIconKey::Image),
            ("clip.mp4", FileIconKey::Video),
            ("song.mp3", FileIconKey::Audio),
            ("face.ttf", FileIconKey::Font),
            ("bundle.zip", FileIconKey::Archive),
            ("build.tar.gz", FileIconKey::Archive),
            ("deploy.sh", FileIconKey::Shell),
            ("query.sql", FileIconKey::Sql),
            ("data.sqlite", FileIconKey::Database),
            ("app.log", FileIconKey::Log),
            ("service.env", FileIconKey::Env),
            ("Cargo.lock", FileIconKey::Lock),
            ("app.conf", FileIconKey::Settings),
            // Unknown extension and no extension both fall back to File.
            ("a.rlib", FileIconKey::File),
            ("README", FileIconKey::File),
            // The original's `pathExtension` treats a name that starts
            // with `.` and has no *other* `.` as extension-less — these
            // are not in the exact-name table either, so they fall back,
            // exactly as the original falls back (not a Terminal/Settings
            // glyph, however tempting that heuristic looks).
            (".bashrc", FileIconKey::File),
            (".editorconfig", FileIconKey::File),
            // A dotfile with a *second* dot does have an extension under
            // the same rule, and it is looked up normally.
            (".env.local", FileIconKey::File), // "local" is not a known extension
        ];
        for (name, expected) in cases {
            assert_eq!(
                FileIconKey::for_file_name(name),
                *expected,
                "for_file_name({name:?})"
            );
        }
    }

    #[test]
    fn for_directory_name_matches_the_original_table_and_falls_back_to_folder() {
        let cases: &[(&str, FileIconKey)] = &[
            ("src", FileIconKey::FolderSrc),
            ("Sources", FileIconKey::FolderSrc),
            ("tests", FileIconKey::FolderTests),
            ("__tests__", FileIconKey::FolderTests),
            ("docs", FileIconKey::FolderDocs),
            (".github", FileIconKey::FolderGithub),
            ("node_modules", FileIconKey::FolderNodeModules),
            ("dist", FileIconKey::FolderDist),
            ("build", FileIconKey::FolderDist),
            ("scripts", FileIconKey::FolderScripts),
            ("config", FileIconKey::FolderConfig),
            ("assets", FileIconKey::FolderAssets),
            ("public", FileIconKey::FolderPublic),
            ("packages", FileIconKey::FolderPackages),
            (".vscode", FileIconKey::FolderVscode),
            (".git", FileIconKey::FolderGit),
            ("lib", FileIconKey::FolderLib),
            ("tools", FileIconKey::FolderTools),
            // Unknown directory names fall back to the default folder.
            ("random-dir", FileIconKey::Folder),
            ("Sidebar", FileIconKey::Folder),
        ];
        for (name, expected) in cases {
            assert_eq!(
                FileIconKey::for_directory_name(name),
                *expected,
                "for_directory_name({name:?})"
            );
        }
    }
}
