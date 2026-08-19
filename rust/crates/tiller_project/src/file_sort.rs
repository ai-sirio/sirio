//! Natural-numeric, case-insensitive ordering for file-tree entries — the
//! order a person reads them in, so `file2` comes before `file10`.
//!
//! The Swift original (`Packages/TillerCore/Sources/TillerCore/FileTree.swift`,
//! `sortedNodes`) orders entries directories-first, then by
//! `String.localizedStandardCompare` (Foundation's Finder-style compare:
//! case-insensitive and numeric-run-aware), then falls back to a raw UTF-8
//! byte comparison of the name to make the order total and deterministic.
//! [`compare_file_tree_names`] ports that three-tier comparator; every
//! file-tree sort in this crate and in `tiller_ui`'s right panel should go
//! through it rather than keep its own copy.
//!
//! Diacritic folding here is intentionally bounded to the Latin-1
//! Supplement block (the common accented Latin letters, e.g. À-Ö, Ø-ö,
//! ø-ÿ) so that e.g. `"é"` sorts next to `"e"` instead of after every ASCII
//! name by raw code point. This is not general Unicode normalization: it
//! does not NFD-decompose, does not fold combining marks, and does nothing
//! for non-Latin scripts. There is no unicode-normalization/icu/caseless
//! crate as a direct workspace dependency today; pulling one in for full
//! Unicode collation is a separate, deliberate decision, not a hidden side
//! effect of this fix.

use std::cmp::Ordering;
use std::iter::Peekable;
use std::str::Chars;

/// Directory-first, natural-numeric, case-insensitive comparison of two
/// file-tree entries by name. Ties (identical after folding *and* identical
/// raw bytes — i.e. truly the same name) are `Ordering::Equal`; two entries
/// in the same real directory can't have the same name, so this is already
/// a total order in practice and needs no further tiebreak (the Swift
/// original's fourth-tier `relativePath` tiebreak is therefore dead code in
/// practice and is not ported).
pub fn compare_file_tree_names(
    lhs_is_dir: bool,
    lhs_name: &str,
    rhs_is_dir: bool,
    rhs_name: &str,
) -> Ordering {
    // Directories before files: `true` (is a directory) sorts first.
    match rhs_is_dir.cmp(&lhs_is_dir) {
        Ordering::Equal => {}
        order => return order,
    }
    match natural_case_insensitive_compare(lhs_name, rhs_name) {
        Ordering::Equal => lhs_name.as_bytes().cmp(rhs_name.as_bytes()),
        order => order,
    }
}

/// Case-insensitive, natural-numeric string comparison: runs of ASCII
/// digits compare by numeric value (so `"file2"` < `"file10"`), with
/// leading zeros ignored for the numeric comparison itself (`"7"` and
/// `"007"` compare equal here — exactly as under `localizedStandardCompare`
/// — leaving [`compare_file_tree_names`]'s raw-byte tiebreak, where `'0'` <
/// `'7'`, to order `"007"` before `"7"` deterministically). Non-digit
/// characters compare case-insensitively, with accented Latin-1 Supplement
/// letters folded to their unaccented base letter first (see the module
/// doc comment for the exact bound).
pub fn natural_case_insensitive_compare(a: &str, b: &str) -> Ordering {
    let mut ac: Peekable<Chars<'_>> = a.chars().peekable();
    let mut bc: Peekable<Chars<'_>> = b.chars().peekable();
    loop {
        match (ac.peek().copied(), bc.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ca), Some(cb)) => {
                if ca.is_ascii_digit() && cb.is_ascii_digit() {
                    let da = take_digits(&mut ac);
                    let db = take_digits(&mut bc);
                    match compare_numeric_runs(&da, &db) {
                        Ordering::Equal => continue,
                        order => return order,
                    }
                } else {
                    let la = fold(ca);
                    let lb = fold(cb);
                    match la.cmp(&lb) {
                        Ordering::Equal => {
                            ac.next();
                            bc.next();
                        }
                        order => return order,
                    }
                }
            }
        }
    }
}

fn take_digits(iter: &mut Peekable<Chars<'_>>) -> String {
    let mut digits = String::new();
    while let Some(&c) = iter.peek() {
        if c.is_ascii_digit() {
            digits.push(c);
            iter.next();
        } else {
            break;
        }
    }
    digits
}

/// Compares two runs of ASCII digits by numeric value, ignoring leading
/// zeros. Equal-length trimmed runs compare lexicographically, which for
/// same-length decimal digit strings is the same order as numeric value;
/// different lengths (after trimming leading zeros) order by length, since
/// a longer digit string with no leading zero is always the larger number.
fn compare_numeric_runs(a: &str, b: &str) -> Ordering {
    let a_trimmed = a.trim_start_matches('0');
    let b_trimmed = b.trim_start_matches('0');
    match a_trimmed.len().cmp(&b_trimmed.len()) {
        Ordering::Equal => a_trimmed.cmp(b_trimmed),
        order => order,
    }
}

/// Folds one character for comparison: accented Latin-1 Supplement letters
/// map to their unaccented ASCII base letter (lowercase); everything else
/// is lowercased via `char::to_lowercase`'s first mapped character, which
/// is a no-op for characters with no case.
fn fold(c: char) -> char {
    let base = match c {
        'À'..='Å' | 'à'..='å' => 'a',
        'Æ' | 'æ' => 'a',
        'Ç' | 'ç' => 'c',
        'È'..='Ë' | 'è'..='ë' => 'e',
        'Ì'..='Ï' | 'ì'..='ï' => 'i',
        'Ð' | 'ð' => 'd',
        'Ñ' | 'ñ' => 'n',
        'Ò'..='Ö' | 'ò'..='ö' => 'o',
        'Ø' | 'ø' => 'o',
        'Ù'..='Ü' | 'ù'..='ü' => 'u',
        'Ý' | 'ý' | 'ÿ' => 'y',
        'Þ' | 'þ' => 't',
        'ß' => 's',
        other => other,
    };
    base.to_lowercase().next().unwrap_or(base)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmp(a: &str, b: &str) -> Ordering {
        natural_case_insensitive_compare(a, b)
    }

    #[test]
    fn orders_digit_runs_numerically_not_lexicographically() {
        assert_eq!(cmp("file2", "file10"), Ordering::Less);
        assert_eq!(cmp("file10", "file2"), Ordering::Greater);
        assert_eq!(cmp("file9", "file10"), Ordering::Less);
    }

    #[test]
    fn orders_mixed_letter_digit_runs_by_each_numeric_segment() {
        assert_eq!(cmp("a1b2", "a1b10"), Ordering::Less);
        assert_eq!(cmp("a1b10", "a1b2"), Ordering::Greater);
        // First segment already decides it; second segment is never reached.
        assert_eq!(cmp("a2b1", "a10b1"), Ordering::Less);
    }

    #[test]
    fn leading_zeros_do_not_change_numeric_value() {
        assert_eq!(cmp("007", "7"), Ordering::Equal);
        assert_eq!(cmp("007", "07"), Ordering::Equal);
        assert_eq!(cmp("000", "0"), Ordering::Equal);
        // But a genuinely larger padded number still sorts after a smaller one.
        assert_eq!(cmp("007", "8"), Ordering::Less);
        assert_eq!(cmp("010", "9"), Ordering::Greater);
    }

    #[test]
    fn full_comparator_breaks_the_leading_zero_tie_by_raw_bytes() {
        // "007" and "7" are numerically equal, so compare_file_tree_names
        // falls back to a raw byte compare, where '0' (0x30) < '7' (0x37).
        assert_eq!(
            compare_file_tree_names(false, "007", false, "7"),
            Ordering::Less
        );
    }

    #[test]
    fn is_case_insensitive() {
        assert_eq!(cmp("File2", "file10"), Ordering::Less);
        assert_eq!(cmp("APPLE", "apple"), Ordering::Equal);
        assert_eq!(cmp("Banana", "apple"), Ordering::Greater);
    }

    #[test]
    fn folds_latin1_supplement_accents_to_their_base_letter() {
        // "é" folds to "e", so it sorts where "e" would, not after every
        // ASCII name by raw code point (raw 'é' = U+00E9 is far past 'z').
        assert_eq!(cmp("cafe", "café"), Ordering::Equal);
        assert_eq!(cmp("élan", "elam"), Ordering::Greater); // e-l-a-n vs e-l-a-m: n > m
        assert_eq!(cmp("élan", "fan"), Ordering::Less); // e < f
        assert_eq!(cmp("élan", "dan"), Ordering::Greater); // e > d
    }

    #[test]
    fn non_latin_characters_pass_through_unfolded() {
        // Outside the bounded Latin-1 fold, characters compare by their own
        // (lowercased) code point -- documented, not silently pretended.
        assert_eq!(cmp("日本語", "日本語"), Ordering::Equal);
        assert_ne!(cmp("日本語", "aaa"), Ordering::Equal);
    }

    #[test]
    fn directories_sort_before_files_regardless_of_name() {
        assert_eq!(
            compare_file_tree_names(true, "zzz", false, "aaa"),
            Ordering::Less
        );
        assert_eq!(
            compare_file_tree_names(false, "aaa", true, "zzz"),
            Ordering::Greater
        );
    }

    #[test]
    fn full_ordering_matches_how_a_person_reads_a_directory_listing() {
        let mut names = vec![
            "file10.txt",
            "file2.txt",
            "file1.txt",
            "a1b10",
            "a1b2",
            "007",
            "7",
            "Banana",
            "apple",
        ];
        names.sort_by(|a, b| natural_case_insensitive_compare(a, b));
        assert_eq!(
            names,
            vec![
                "007",
                "7",
                "a1b2",
                "a1b10",
                "apple",
                "Banana",
                "file1.txt",
                "file2.txt",
                "file10.txt",
            ]
        );
    }
}
