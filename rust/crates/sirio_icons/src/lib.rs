//! The Material icon theme, whole.
//!
//! See `docs/superpowers/specs/2026-09-22-material-icon-theme-design.md`.
//! `generated.rs` is written by `Scripts/vendor-material-icons.py` from one
//! pinned upstream commit and is never hand-edited.

mod generated;

use generated::{ASSETS, DIRECTORIES, LIGHT_VARIANTS, STEMS, SUFFIXES};

/// The theme's own fallback for a name it does not know. Upstream calls it
/// `default`; `src/theme.ts` renames it to `file`.
pub const DEFAULT_FILE: &str = "file";
/// The fallback pair for a directory name the theme does not know.
pub const DEFAULT_FOLDER: &str = "folder";
pub const DEFAULT_FOLDER_OPEN: &str = "folder-open";

/// The asset stem for a file name: the whole name first, then the longest
/// dotted tail, then [`DEFAULT_FILE`]. Resolved from the name alone — never
/// from the file's content.
pub fn for_file(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if let Some(stem) = pair_lookup(STEMS, &lower) {
        return stem;
    }
    // `match_indices` runs left to right, so the first dot yields the
    // longest tail and the longest tail is tried first: `app.blade.php`
    // asks for `blade.php` before it asks for `php`.
    for (index, _) in lower.match_indices('.') {
        if let Some(stem) = pair_lookup(SUFFIXES, &lower[index + 1..]) {
            return stem;
        }
    }
    DEFAULT_FILE
}

/// The asset stem for a directory name, in the state the row is drawn in.
/// The two states are separate assets upstream, not one asset rotated.
pub fn for_directory(name: &str, expanded: bool) -> &'static str {
    let lower = name.to_lowercase();
    match DIRECTORIES.binary_search_by(|(key, _, _)| (*key).cmp(lower.as_str())) {
        Ok(index) => {
            let (_, collapsed, open) = DIRECTORIES[index];
            if expanded { open } else { collapsed }
        }
        Err(_) => {
            if expanded {
                DEFAULT_FOLDER_OPEN
            } else {
                DEFAULT_FOLDER
            }
        }
    }
}

/// The embedded SVG for a stem.
pub fn asset(stem: &str) -> Option<&'static [u8]> {
    asset_index(stem).map(|index| ASSETS[index].2)
}

/// The asset path for a stem, as `sirio_ui`'s `Icon::path` reports it.
pub fn asset_path(stem: &str) -> Option<&'static str> {
    asset_index(stem).map(|index| ASSETS[index].1)
}

/// The light-appearance companion for a stem, where upstream ships one.
/// `None` means the one asset serves both appearances.
pub fn light_variant(stem: &str) -> Option<&'static str> {
    LIGHT_VARIANTS
        .binary_search_by(|(key, _)| (*key).cmp(stem))
        .ok()
        .map(|index| LIGHT_VARIANTS[index].1)
}

fn asset_index(stem: &str) -> Option<usize> {
    ASSETS.binary_search_by(|(key, _, _)| (*key).cmp(stem)).ok()
}

fn pair_lookup(table: &'static [(&'static str, &'static str)], key: &str) -> Option<&'static str> {
    table
        .binary_search_by(|(candidate, _)| (*candidate).cmp(key))
        .ok()
        .map(|index| table[index].1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The resolution order, read off the pinned table rather than guessed.
    /// `cargo.toml` is here because it is the case that looks like a name
    /// match and is not one: it resolves through the `toml` suffix.
    #[test]
    fn resolution_order_prefers_names_then_longest_suffix() {
        let cases = [
            ("main.rs", "rust"),
            ("Cargo.toml", "toml"),
            ("Dockerfile", "docker"),
            ("Cargo.lock", "lock"),
            ("deploy.sh", "console"),
            ("README.md", "readme"),
            ("README", "readme"),
            ("notes.txt", "document"),
            ("photo.PNG", "image"),
            ("release.zip", "zip"),
            ("build.tar.gz", "zip"),
            // The longer tail wins: `blade.php` is Laravel, `php` alone is not.
            ("app.blade.php", "laravel"),
            // Repaired by the key remap in the vendoring script; without it
            // the whole git family falls back to the generic mark.
            (".gitignore", "git"),
            (".gitmodules", "git"),
            (".env", "tune"),
            ("service.env", "tune"),
            (".env.local", "tune"),
            (".editorconfig", "editorconfig"),
            // Upstream lists no icon for these, so they fall back. `.bashrc`
            // is *not* a hole left by the old Foundation extension quirk --
            // it is simply absent upstream, and a future reader will assume
            // otherwise unless this case says so.
            (".bashrc", "file"),
            ("gitmodules", "file"),
            ("a.rlib", "file"),
        ];
        for (name, expected) in cases {
            assert_eq!(for_file(name), expected, "for_file({name:?})");
        }
    }

    #[test]
    fn directories_resolve_as_a_collapsed_and_expanded_pair() {
        let cases = [
            ("src", "folder-src", "folder-src-open"),
            ("tests", "folder-test", "folder-test-open"),
            ("docs", "folder-docs", "folder-docs-open"),
            (".github", "folder-github", "folder-github-open"),
            (".git", "folder-git", "folder-git-open"),
            ("node_modules", "folder-node", "folder-node-open"),
            ("target", "folder-target", "folder-target-open"),
            ("random-name", "folder", "folder-open"),
        ];
        for (name, collapsed, expanded) in cases {
            assert_eq!(for_directory(name, false), collapsed, "{name:?} collapsed");
            assert_eq!(for_directory(name, true), expanded, "{name:?} expanded");
        }
    }

    #[test]
    fn lookups_ignore_case_the_way_the_tables_were_folded() {
        assert_eq!(for_file("DOCKERFILE"), "docker");
        assert_eq!(for_file("Main.RS"), "rust");
        assert_eq!(for_directory("Node_Modules", false), "folder-node");
    }

    #[test]
    fn assets_answer_bytes_and_a_path_for_a_known_stem() {
        let bytes = asset("rust").expect("rust is vendored");
        assert!(bytes.starts_with(b"<svg"), "an asset is an svg");
        assert_eq!(asset_path("rust"), Some("icons/material/rust.svg"));
        assert_eq!(asset("no-such-icon"), None);
        assert_eq!(asset_path("no-such-icon"), None);
    }

    /// The pair that makes the light appearance worth supporting at all:
    /// `toml` paints #cfd8dc, which is invisible on a light background, and
    /// `toml` is every Cargo.toml.
    #[test]
    fn light_variants_answer_only_where_upstream_ships_one() {
        assert_eq!(light_variant("toml"), Some("toml_light"));
        assert_eq!(light_variant("bun"), Some("bun_light"));
        assert_eq!(light_variant("rust"), None);
        assert!(asset("toml_light").is_some(), "the companion is vendored too");
    }
}
