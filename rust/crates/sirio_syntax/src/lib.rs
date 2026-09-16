//! The grammars Sirio highlights that bezel-syntax does not carry.
//!
//! `bezel-syntax` ships seven: Rust, Python, TypeScript/TSX, JSON, Go, Bash
//! and TOML. Sirio's editor recognises twenty-four languages, so a Java, C,
//! Swift or YAML file arrived at the source view already parsed, already
//! measured, already laid out — and painted in one colour, because nothing
//! had classified a token. This crate is the other seventeen.
//!
//! It is an *addition to* bezel, not a replacement for it. bezel-syntax
//! documents the extension point in its own module docs — "a language the
//! table does not carry is a [`Lang::new`] `static` of your own, highlighted
//! through [`Lang::highlight`] — the same path the built-in rows take" — and
//! that is exactly what happens below. The consequence worth stating: spans
//! from a grammar added here are `bezel::theme::HighlightKind` values
//! indistinguishable from bezel's own, so they reach
//! [`SyntaxPalette::color`](theme::SyntaxPalette::color) and the theme with
//! no second code path and no second palette to keep in step.
//!
//! [`highlight`] is the whole surface. It answers bezel first, so a language
//! bezel carries keeps bezel's hand-tuned query; ours are the grammar
//! authors' own `highlights.scm`, with their capture names translated by
//! [`captures`].
//!
//! Its own crate, and a leaf one, for the reason the repo's other leaves
//! exist: seventeen tree-sitter grammars are seventeen C libraries, and
//! `sirio_ui` — which everything above it recompiles behind — should not be
//! the package that owns them.

use std::ops::Range;
use std::sync::OnceLock;

use syntax::lang::Lang;
use syntax::tree_sitter_language::LanguageFn;

pub use theme::HighlightKind;

mod captures;

/// One grammar, and the work needed to turn it into something bezel can run.
///
/// The query is assembled and the [`Lang`] built on first use rather than in
/// the `static`, because [`Lang::new`] wants a `&'static str` and the query
/// this crate hands it does not exist until [`captures::rewrite`] has run.
/// One leak per language the user actually opens, once per process.
struct Grammar {
    /// Reported by tree-sitter when the query fails to compile, so it is the
    /// name that has to be recognisable in that message.
    name: &'static str,
    /// The fence tags and editor language tags this row answers to. Sirio
    /// asks with [`sirio_ui::editor::Language::fence_tag`]'s output; the
    /// extension-shaped aliases are for Markdown fences, which are written
    /// by hand and use whatever their author felt like.
    aliases: &'static [&'static str],
    grammar: LanguageFn,
    /// Query texts, concatenated in order. More than one when a grammar's
    /// shipped query is a *supplement* to another's — the `; inherits: c`
    /// convention, which the crates strip. Order is load-bearing:
    /// `tree-sitter-highlight` lets the last pattern matching a node win
    /// (`highlight.rs`, "keep iterating over any later highlighting
    /// patterns"), so the inherited query goes first and the specific one
    /// overrides it.
    query: &'static [&'static str],
    lang: OnceLock<&'static Lang>,
}

impl Grammar {
    fn lang(&'static self) -> &'static Lang {
        self.lang.get_or_init(|| {
            let query: &'static str =
                Box::leak(captures::rewrite(&self.query.join("\n")).into_boxed_str());
            &*Box::leak(Box::new(Lang::new(
                self.name,
                self.aliases,
                self.grammar,
                query,
            )))
        })
    }
}

/// Every language Sirio adds. One row per grammar; the order is the order of
/// [`sirio_ui::editor::Language`]'s own declaration, so the two lists can be
/// read side by side.
///
/// An array rather than a `&[Grammar]` slice: a `static` holding a reference
/// to a temporary that contains a `OnceLock` is rejected outright (E0492),
/// and rightly — the array has to *be* the static for the locks to be the
/// program's rather than a promoted temporary's. The written-out length is
/// the price, and the compiler names it the moment a row is added.
static GRAMMARS: [Grammar; 15] = [
    Grammar {
        name: "markdown",
        aliases: &["markdown", "md"],
        grammar: tree_sitter_md::LANGUAGE,
        // The block grammar only. Markdown is split in two upstream, and the
        // inline half needs an injection to reach — machinery bezel
        // deliberately has none of. Block alone still carries the structure a
        // reader scans for: headings, fences, list marks, link targets.
        query: &[tree_sitter_md::HIGHLIGHT_QUERY_BLOCK],
        lang: OnceLock::new(),
    },
    Grammar {
        name: "yaml",
        aliases: &["yaml", "yml"],
        grammar: tree_sitter_yaml::LANGUAGE,
        query: &[tree_sitter_yaml::HIGHLIGHTS_QUERY],
        lang: OnceLock::new(),
    },
    Grammar {
        name: "c",
        aliases: &["c", "h"],
        grammar: tree_sitter_c::LANGUAGE,
        query: &[tree_sitter_c::HIGHLIGHT_QUERY],
        lang: OnceLock::new(),
    },
    Grammar {
        name: "cpp",
        aliases: &["cpp", "c++", "cc", "cxx", "hpp", "hh", "hxx"],
        grammar: tree_sitter_cpp::LANGUAGE,
        // C's query first: see `Grammar::query`. Without it a C++ file loses
        // comments, strings, numbers and every keyword C and C++ share —
        // which is most of them.
        query: &[
            tree_sitter_c::HIGHLIGHT_QUERY,
            tree_sitter_cpp::HIGHLIGHT_QUERY,
        ],
        lang: OnceLock::new(),
    },
    Grammar {
        name: "swift",
        aliases: &["swift"],
        grammar: tree_sitter_swift::LANGUAGE,
        query: &[tree_sitter_swift::HIGHLIGHTS_QUERY],
        lang: OnceLock::new(),
    },
    Grammar {
        name: "kotlin",
        aliases: &["kotlin", "kt", "kts"],
        grammar: tree_sitter_kotlin_sg::LANGUAGE,
        query: &[tree_sitter_kotlin_sg::HIGHLIGHTS_QUERY],
        lang: OnceLock::new(),
    },
    Grammar {
        name: "java",
        aliases: &["java"],
        grammar: tree_sitter_java::LANGUAGE,
        query: &[tree_sitter_java::HIGHLIGHTS_QUERY],
        lang: OnceLock::new(),
    },
    Grammar {
        name: "ruby",
        aliases: &["ruby", "rb"],
        grammar: tree_sitter_ruby::LANGUAGE,
        query: &[tree_sitter_ruby::HIGHLIGHTS_QUERY],
        lang: OnceLock::new(),
    },
    Grammar {
        name: "php",
        aliases: &["php"],
        // The grammar that understands a file opening in HTML and switching
        // at `<?php`, which is what a .php file on disk actually is.
        grammar: tree_sitter_php::LANGUAGE_PHP,
        query: &[tree_sitter_php::HIGHLIGHTS_QUERY],
        lang: OnceLock::new(),
    },
    Grammar {
        name: "html",
        aliases: &["html", "htm"],
        grammar: tree_sitter_html::LANGUAGE,
        query: &[tree_sitter_html::HIGHLIGHTS_QUERY],
        lang: OnceLock::new(),
    },
    Grammar {
        name: "css",
        aliases: &["css"],
        grammar: tree_sitter_css::LANGUAGE,
        query: &[tree_sitter_css::HIGHLIGHTS_QUERY],
        lang: OnceLock::new(),
    },
    Grammar {
        name: "sql",
        aliases: &["sql"],
        grammar: tree_sitter_sequel::LANGUAGE,
        query: &[tree_sitter_sequel::HIGHLIGHTS_QUERY],
        lang: OnceLock::new(),
    },
    Grammar {
        name: "xml",
        aliases: &["xml"],
        grammar: tree_sitter_xml::LANGUAGE_XML,
        query: &[tree_sitter_xml::XML_HIGHLIGHT_QUERY],
        lang: OnceLock::new(),
    },
    Grammar {
        name: "lua",
        aliases: &["lua"],
        grammar: tree_sitter_lua::LANGUAGE,
        query: &[tree_sitter_lua::HIGHLIGHTS_QUERY],
        lang: OnceLock::new(),
    },
    Grammar {
        name: "zig",
        aliases: &["zig"],
        grammar: tree_sitter_zig::LANGUAGE,
        query: &[tree_sitter_zig::HIGHLIGHTS_QUERY],
        lang: OnceLock::new(),
    },
];

/// Find the grammar a tag names, by bezel's own tag rules: the first word,
/// case-folded.
fn resolve(tag: &str) -> Option<&'static Grammar> {
    let tag = tag
        .split([' ', ','])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    GRAMMARS
        .iter()
        .find(|grammar| grammar.aliases.contains(&tag.as_str()))
}

/// Classify `source` as `tag`, in bytes, in document order. `None` when no
/// grammar — bezel's or this crate's — answers to the tag, which is the
/// caller's signal to render plain text.
///
/// bezel is asked first. Its seven queries are written against its own
/// vocabulary by hand and are better than a translated one; this crate's job
/// is the languages it has nothing to say about.
pub fn highlight(source: &str, tag: &str) -> Option<Vec<(Range<usize>, HighlightKind)>> {
    if let Some(spans) = syntax::highlight(source, tag) {
        return Some(spans);
    }
    resolve(tag)?.lang().highlight(source)
}

/// Whether some grammar answers to `tag`. Cheap: no parse, no query compile.
pub fn is_supported(tag: &str) -> bool {
    syntax::lang::resolve(tag).is_some() || resolve(tag).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str, tag: &str) -> Vec<HighlightKind> {
        let mut kinds: Vec<HighlightKind> = highlight(source, tag)
            .unwrap_or_else(|| panic!("no grammar answered to `{tag}`"))
            .into_iter()
            .map(|(_, kind)| kind)
            .collect();
        kinds.sort_by_key(|kind| format!("{kind:?}"));
        kinds.dedup();
        kinds
    }

    /// The test that earns the crate. A query that will not compile against
    /// its grammar makes `Lang::highlight` return `None`, and the file view
    /// renders plain text — the exact appearance of a language nobody added.
    /// Nothing else in the system can tell those two apart.
    #[test]
    fn every_query_compiles_against_its_grammar() {
        for grammar in GRAMMARS.iter() {
            assert!(
                grammar.lang().compiled().is_some(),
                "`{}`'s highlights query does not compile against its grammar",
                grammar.name
            );
        }
    }

    #[test]
    fn no_two_grammars_claim_the_same_tag() {
        // `resolve` takes the first match, so a duplicate would silently
        // shadow — and the shadowed row would only ever be found by reading
        // this file.
        let mut seen: Vec<&str> = Vec::new();
        for grammar in GRAMMARS.iter() {
            for alias in grammar.aliases {
                assert!(
                    !seen.contains(alias),
                    "`{alias}` is claimed twice, the second time by `{}`",
                    grammar.name
                );
                seen.push(alias);
            }
        }
    }

    #[test]
    fn bezel_keeps_the_languages_it_carries() {
        // Its queries are written against its own vocabulary by hand. If a
        // row here ever claimed `rust`, this crate would quietly replace a
        // better highlighter with a translated one.
        for tag in [
            "rust",
            "python",
            "typescript",
            "tsx",
            "json",
            "go",
            "bash",
            "toml",
        ] {
            assert!(
                resolve(tag).is_none(),
                "`{tag}` is bezel's; adding a row for it downgrades it"
            );
            assert!(is_supported(tag));
        }
    }

    #[test]
    fn java_is_classified_and_not_left_as_plain_text() {
        // The case that prompted the crate.
        let kinds = kinds(
            "// a comment\npublic class Main { String greeting = \"hi\"; int n = 42; }",
            "java",
        );
        for expected in [
            HighlightKind::Comment,
            HighlightKind::Keyword,
            HighlightKind::String,
            HighlightKind::Number,
        ] {
            assert!(
                kinds.contains(&expected),
                "java: no {expected:?} in {kinds:?}"
            );
        }
    }

    #[test]
    fn cplusplus_inherits_what_it_shares_with_c() {
        // Comments, strings and numbers are all in C's query, not C++'s. If
        // the concatenation is dropped this is what goes missing.
        let kinds = kinds(
            "// a comment\n#include <string>\nint main() { auto s = \"hi\"; return 0; }",
            "cpp",
        );
        for expected in [
            HighlightKind::Comment,
            HighlightKind::String,
            HighlightKind::Number,
        ] {
            assert!(
                kinds.contains(&expected),
                "cpp: no {expected:?} in {kinds:?}"
            );
        }
    }

    #[test]
    fn a_rewritten_capture_reaches_the_palette_as_its_translated_kind() {
        // Lua writes `@repeat` and `@field`, which bezel does not know. Left
        // alone they would arrive as `Variable` — the plain-text colour —
        // which is why the rewrite exists at all.
        let kinds = kinds("for i = 1, 10 do print(t.field) end", "lua");
        assert!(
            kinds.contains(&HighlightKind::Keyword),
            "lua: `for`/`do` must arrive as a keyword, not as plain text: {kinds:?}"
        );
        assert!(
            kinds.contains(&HighlightKind::Property),
            "lua: `t.field` must arrive as a property, not as plain text: {kinds:?}"
        );
    }

    /// Swift, SQL and Zig write `@spell` as a second capture on the node
    /// they just captured as `@comment`, and the last capture wins. Shipping
    /// their queries unaltered paints every comment in those three languages
    /// the colour of ordinary text — which is what "syntax highlighting does
    /// not work here" looks like from the outside.
    #[test]
    fn a_comment_survives_the_spell_capture_that_shares_its_node() {
        for (tag, source) in [
            ("swift", "// a comment\nlet x = 1\n"),
            ("sql", "-- a comment\nselect 1;\n"),
            ("zig", "// a comment\nconst x = 1;\n"),
        ] {
            let spans = highlight(source, tag).unwrap_or_else(|| panic!("{tag}"));
            let comment = spans
                .iter()
                .find(|(range, _)| range.start == 0)
                .unwrap_or_else(|| panic!("{tag}: the comment is classified at all: {spans:?}"));
            assert_eq!(
                comment.1,
                HighlightKind::Comment,
                "{tag}: the comment is a comment, not {:?}; spans={spans:?}",
                comment.1
            );
        }
    }

    /// One representative token per remaining language. Not a test of the
    /// grammar — that is the grammar author's job — but of the wiring: the
    /// right `LanguageFn` against the right query, reaching a kind that is
    /// not the fallback.
    #[test]
    fn every_language_classifies_a_representative_token() {
        let cases: &[(&str, &str, HighlightKind)] = &[
            ("markdown", "# Title\n", HighlightKind::Keyword),
            ("yaml", "key: value\n", HighlightKind::Property),
            ("c", "/* hi */\nint n = 1;\n", HighlightKind::Comment),
            ("swift", "let greeting = \"hi\"\n", HighlightKind::String),
            (
                "kotlin",
                "fun main() { val n = 42 }\n",
                HighlightKind::Number,
            ),
            ("ruby", "# a comment\nputs 'hi'\n", HighlightKind::Comment),
            (
                "php",
                "<?php $name = \"sirio\"; ?>\n",
                HighlightKind::String,
            ),
            ("html", "<p class=\"x\">hi</p>\n", HighlightKind::Tag),
            ("css", "a { color: red; }\n", HighlightKind::Tag),
            ("sql", "select 1 from t;\n", HighlightKind::Keyword),
            ("xml", "<?xml version=\"1.0\"?><a/>\n", HighlightKind::Tag),
            ("lua", "-- a comment\nlocal x = 1\n", HighlightKind::Comment),
            (
                "zig",
                "const std = @import(\"std\");\n",
                HighlightKind::Keyword,
            ),
        ];
        for (tag, source, expected) in cases {
            let kinds = kinds(source, tag);
            assert!(
                kinds.contains(expected),
                "{tag}: expected {expected:?}, got {kinds:?}"
            );
        }
    }

    #[test]
    fn an_unknown_tag_is_none_rather_than_an_empty_classification() {
        // `None` and `Some(vec![])` mean different things to the caller:
        // "render this as text" and "this parsed to nothing".
        assert!(highlight("hello", "brainfuck").is_none());
        assert!(!is_supported("brainfuck"));
    }

    #[test]
    fn spans_are_byte_ranges_inside_the_source() {
        // The file view slices the line with them; one past the end panics.
        let source = "public class Main {}\n";
        for (range, _) in highlight(source, "java").expect("java") {
            assert!(range.end <= source.len(), "{range:?} escapes {source:?}");
            assert!(source.is_char_boundary(range.start));
            assert!(source.is_char_boundary(range.end));
        }
    }
}
