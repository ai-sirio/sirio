//! Diagram source in, SVG out: the Markdown Preview's Mermaid and PlantUML.
//!
//! No gpui and no bezel here, so rendering, timeouts and the server protocol
//! are tested without a window; `sirio_ui` owns scheduling and state. Design:
//! `docs/superpowers/specs/2026-09-23-markdown-rich-preview-design.md` §3.

mod svg;

use std::borrow::Cow;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};

/// The largest source the Preview renders (design §3).
pub const MAX_SOURCE_BYTES: usize = 64 * 1024;

/// Part of every Mermaid cache key: the renderer's name and pinned version.
/// Move it together with the `=` pin in this crate's Cargo.toml.
const MERMAID_RENDERER: &str = "mermaid-rs-renderer 0.3.1";

/// Which language a diagram fence is written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DiagramKind {
    Mermaid,
    PlantUml,
}

impl DiagramKind {
    /// The kind a fence's info string names, by its first word, ignoring case.
    pub fn from_fence(info: &str) -> Option<Self> {
        let word = info.split_whitespace().next()?;
        match word.to_ascii_lowercase().as_str() {
            "mermaid" => Some(Self::Mermaid),
            "plantuml" | "puml" | "uml" => Some(Self::PlantUml),
            _ => None,
        }
    }

    /// How a note names the kind.
    pub fn label(self) -> &'static str {
        match self {
            Self::Mermaid => "Mermaid",
            Self::PlantUml => "PlantUML",
        }
    }

    fn renderer_identity(self) -> &'static str {
        match self {
            Self::Mermaid => MERMAID_RENDERER,
            Self::PlantUml => "plantuml",
        }
    }
}

/// The colours a Mermaid diagram is drawn in, as CSS colour strings. Built by
/// `sirio_ui` from the app's palette so this crate never names bezel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    pub dark: bool,
    pub background: String,
    pub text: String,
    pub node_fill: String,
    pub node_border: String,
    pub line: String,
    pub label_background: String,
}

impl Palette {
    fn fingerprint(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}",
            self.dark,
            self.background,
            self.text,
            self.node_fill,
            self.node_border,
            self.line,
            self.label_background
        )
    }
}

/// Everything a render needs besides the source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    pub palette: Palette,
    /// The configured PlantUML server. `None` or blank means none.
    pub plantuml_server: Option<String>,
    /// The Markdown file's directory: PlantUML's working directory.
    pub working_dir: PathBuf,
    /// The directory PlantUML's `!include` is confined to (the worktree).
    pub include_root: PathBuf,
}

/// A rendered diagram.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Svg {
    /// The SVG, its root sized at twice the logical size (design §3).
    pub markup: String,
    pub logical_width: u32,
    pub logical_height: u32,
}

/// Why a diagram was not rendered. Every variant degrades one diagram to its
/// code block plus [`DiagramError::note`]; none takes the Preview down.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiagramError {
    Syntax { message: String, line: Option<u32> },
    NotAvailable,
    Timeout { seconds: u64 },
    TooLarge,
    Server { status: u16, message: String },
    Io(String),
}

impl DiagramError {
    /// The one muted line the Preview shows under the diagram's code block.
    pub fn note(&self, kind: DiagramKind) -> String {
        let label = kind.label();
        match self {
            Self::Syntax {
                message,
                line: Some(line),
            } => format!("{label} diagram is invalid (line {line}): {message}"),
            Self::Syntax {
                message,
                line: None,
            } => format!("{label} diagram is invalid: {message}"),
            Self::NotAvailable => {
                "PlantUML is not available: `plantuml` is not on PATH and no server is configured"
                    .to_string()
            }
            Self::Timeout { seconds } => format!("{label} timed out after {seconds} s"),
            Self::TooLarge => format!("{label} diagram is larger than 64 KiB and was not rendered"),
            Self::Server { status, message } => {
                format!("PlantUML server answered {status}: {message}")
            }
            Self::Io(message) => format!("{label} diagram could not be rendered: {message}"),
        }
    }
}

/// The cache key of one diagram: SHA-256 over the kind's renderer, the
/// palette (Mermaid only — PlantUML keeps its own colours) and the source.
pub fn cache_key(kind: DiagramKind, source: &str, palette: &Palette) -> String {
    let mut hasher = Sha256::new();
    hasher.update(kind.renderer_identity().as_bytes());
    hasher.update([0]);
    if kind == DiagramKind::Mermaid {
        hasher.update(palette.fingerprint().as_bytes());
    }
    hasher.update([0]);
    hasher.update(source.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// The per-user directory diagrams are cached in (design §3), read through
/// `env` so tests pass their own. `None` when the platform's variables are
/// missing or relative.
pub fn cache_dir(env: &dyn Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let absolute = |key: &str| {
        env(key)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    let base = if cfg!(target_os = "macos") {
        absolute("HOME").map(|home| home.join("Library").join("Caches").join("Sirio"))
    } else if cfg!(windows) {
        absolute("LOCALAPPDATA").map(|dir| dir.join("Sirio").join("cache"))
    } else {
        absolute("XDG_CACHE_HOME")
            .map(|dir| dir.join("Sirio"))
            .or_else(|| absolute("HOME").map(|home| home.join(".cache").join("Sirio")))
    };
    base.map(|dir| dir.join("diagrams"))
}

/// A diagram already rendered into `dir` under `key`: its path and logical
/// width (half the width the file states, see [`Svg::markup`]).
pub fn cached(dir: &Path, key: &str) -> Option<(PathBuf, u32)> {
    let path = dir.join(format!("{key}.svg"));
    let markup = std::fs::read_to_string(&path).ok()?;
    let (width, _) = svg::root_size(&markup)?;
    Some((path, (width / 2.0).ceil() as u32))
}

/// Writes `svg` into `dir` under `key`, through a temporary name and a
/// rename, so gpui can never read half a file. The temporary name is unique
/// per write, so two renders of one diagram racing each other both land whole.
pub fn store(dir: &Path, key: &str, svg: &Svg) -> std::io::Result<PathBuf> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{key}.svg"));
    let temporary = dir.join(format!(
        ".{key}-{}-{}.part",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&temporary, &svg.markup)?;
    if let Err(error) = std::fs::rename(&temporary, &path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(path)
}

/// A PlantUML source ready for `plantuml`: wrapped in `@startuml`/`@enduml`
/// unless it opens with an `@start…` of its own, as GitHub and GitLab do.
pub(crate) fn plantuml_document(source: &str) -> Cow<'_, str> {
    if source.trim_start().starts_with("@start") {
        Cow::Borrowed(source)
    } else {
        Cow::Owned(format!("@startuml\n{}\n@enduml\n", source.trim_end()))
    }
}

/// A scratch directory per test, unique by construction (same shape as
/// `sirio_markdown::scratch_dir`).
#[cfg(test)]
pub(crate) fn scratch_dir(prefix: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

#[cfg(test)]
pub(crate) fn test_palette(dark: bool) -> Palette {
    let (background, text) = if dark {
        ("#1e1e1e", "#eeeeee")
    } else {
        ("#fafafa", "#222222")
    };
    Palette {
        dark,
        background: background.into(),
        text: text.into(),
        node_fill: "#dddddd".into(),
        node_border: "#888888".into(),
        line: "#666666".into(),
        label_background: background.into(),
    }
}

#[cfg(test)]
pub(crate) fn test_options(root: &Path) -> Options {
    Options {
        palette: test_palette(false),
        plantuml_server: None,
        working_dir: root.to_path_buf(),
        include_root: root.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::path::PathBuf;

    #[test]
    fn fences_name_their_kind() {
        assert_eq!(
            DiagramKind::from_fence("mermaid"),
            Some(DiagramKind::Mermaid)
        );
        assert_eq!(
            DiagramKind::from_fence("Mermaid"),
            Some(DiagramKind::Mermaid)
        );
        assert_eq!(
            DiagramKind::from_fence("plantuml"),
            Some(DiagramKind::PlantUml)
        );
        assert_eq!(
            DiagramKind::from_fence("PlantUML"),
            Some(DiagramKind::PlantUml)
        );
        assert_eq!(DiagramKind::from_fence("puml"), Some(DiagramKind::PlantUml));
        assert_eq!(DiagramKind::from_fence("uml"), Some(DiagramKind::PlantUml));
        assert_eq!(DiagramKind::from_fence("rust"), None);
        assert_eq!(DiagramKind::from_fence(""), None);
    }

    #[test]
    fn a_plantuml_source_without_start_is_wrapped() {
        assert_eq!(
            plantuml_document("A -> B: hi\n"),
            "@startuml\nA -> B: hi\n@enduml\n"
        );
    }

    #[test]
    fn a_plantuml_source_with_its_own_start_is_left_alone() {
        let mindmap = "@startmindmap\n* a\n@endmindmap\n";
        assert_eq!(plantuml_document(mindmap), mindmap);
        let indented = "  @startuml\nA -> B\n@enduml";
        assert_eq!(plantuml_document(indented), indented);
    }

    #[test]
    fn the_cache_key_is_stable_hex() {
        let key = cache_key(DiagramKind::Mermaid, "flowchart TD", &test_palette(false));
        assert_eq!(
            key,
            cache_key(DiagramKind::Mermaid, "flowchart TD", &test_palette(false))
        );
        assert_eq!(key.len(), 64);
        assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn the_cache_key_changes_with_kind_and_source() {
        let palette = test_palette(false);
        let base = cache_key(DiagramKind::Mermaid, "a", &palette);
        assert_ne!(base, cache_key(DiagramKind::PlantUml, "a", &palette));
        assert_ne!(base, cache_key(DiagramKind::Mermaid, "b", &palette));
    }

    #[test]
    fn only_mermaid_keys_follow_the_palette() {
        let (light, dark) = (test_palette(false), test_palette(true));
        assert_ne!(
            cache_key(DiagramKind::Mermaid, "a", &light),
            cache_key(DiagramKind::Mermaid, "a", &dark)
        );
        assert_eq!(
            cache_key(DiagramKind::PlantUml, "a", &light),
            cache_key(DiagramKind::PlantUml, "a", &dark)
        );
    }

    #[test]
    fn notes_read_as_the_design_writes_them() {
        use DiagramKind::{Mermaid, PlantUml};
        let syntax = DiagramError::Syntax {
            message: "bad".into(),
            line: None,
        };
        assert_eq!(syntax.note(Mermaid), "Mermaid diagram is invalid: bad");
        let at_line = DiagramError::Syntax {
            message: "bad".into(),
            line: Some(3),
        };
        assert_eq!(
            at_line.note(PlantUml),
            "PlantUML diagram is invalid (line 3): bad"
        );
        assert_eq!(
            DiagramError::NotAvailable.note(PlantUml),
            "PlantUML is not available: `plantuml` is not on PATH and no server is configured"
        );
        assert_eq!(
            DiagramError::Timeout { seconds: 15 }.note(PlantUml),
            "PlantUML timed out after 15 s"
        );
        assert_eq!(
            DiagramError::Server {
                status: 400,
                message: "Bad Request".into()
            }
            .note(PlantUml),
            "PlantUML server answered 400: Bad Request"
        );
        assert_eq!(
            DiagramError::TooLarge.note(Mermaid),
            "Mermaid diagram is larger than 64 KiB and was not rendered"
        );
        assert_eq!(
            DiagramError::Io("disk full".into()).note(Mermaid),
            "Mermaid diagram could not be rendered: disk full"
        );
    }

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
        move |key| {
            pairs
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| OsString::from(value))
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_linux_cache_follows_xdg_then_home() {
        assert_eq!(
            cache_dir(&env(&[("XDG_CACHE_HOME", "/x"), ("HOME", "/h")])),
            Some(PathBuf::from("/x/Sirio/diagrams"))
        );
        assert_eq!(
            cache_dir(&env(&[("XDG_CACHE_HOME", "relative"), ("HOME", "/h")])),
            Some(PathBuf::from("/h/.cache/Sirio/diagrams"))
        );
        assert_eq!(cache_dir(&env(&[])), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_macos_cache_is_under_library_caches() {
        assert_eq!(
            cache_dir(&env(&[("HOME", "/Users/me")])),
            Some(PathBuf::from("/Users/me/Library/Caches/Sirio/diagrams"))
        );
    }

    #[test]
    fn a_stored_diagram_is_found_again_with_its_logical_width() {
        let dir = scratch_dir("diagram-store");
        let svg = svg::double_for_hidpi(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"40.5\" height=\"20\"></svg>",
        )
        .expect("a sized root");
        let path = store(&dir, "abc", &svg).expect("store");
        assert_eq!(path, dir.join("abc.svg"));
        assert_eq!(cached(&dir, "abc"), Some((dir.join("abc.svg"), 41)));
        assert_eq!(cached(&dir, "missing"), None);
        let leftovers = std::fs::read_dir(&dir)
            .expect("read dir")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".part"))
            .count();
        assert_eq!(
            leftovers, 0,
            "the temporary file is renamed, not left behind"
        );
    }
}
