//! Pure helpers that keep the code surface's text and the editor model in step.

use std::ops::Range;

/// The smallest edit that turns `old` into `new`: the byte range of `old` to
/// replace and the text to put there. Applying it to `old` gives `new`.
/// `None` when the two texts are equal.
pub(crate) fn minimal_edit(old: &str, new: &str) -> Option<(Range<usize>, String)> {
    if old == new {
        return None;
    }
    // Matching chars, not bytes: a matched char has the same encoding on
    // both sides, so every boundary this finds is a char boundary.
    let prefix: usize = old
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum();
    // Counted in what is left after the prefix, so the suffix cannot reach
    // back into it.
    let suffix: usize = old[prefix..]
        .chars()
        .rev()
        .zip(new[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum();
    let range = prefix..old.len() - suffix;
    let text = new[prefix..new.len() - suffix].to_string();
    Some((range, text))
}

#[cfg(test)]
mod tests {
    //! Failure list, written before the code:
    //!
    //! 1. Equal texts produce an edit, which would be a no-op undo step.
    //! 2. A change in the middle replaces more than the changed span, so the
    //!    caret and the folds of the untouched text are lost.
    //! 3. A shared prefix or suffix ends inside a UTF-8 sequence, so the
    //!    replacement splits a character: `è` and `é` share their lead byte,
    //!    `à` and `Ġ` share their trailing byte.
    //! 4. The shared prefix and suffix overlap, so bytes are dropped or
    //!    duplicated: `aa` → `aaa` must insert one `a`, not two.
    //! 5. A side that is empty is mishandled: no old text is a pure insert,
    //!    no new text is a pure delete.
    //! 6. A text that shares nothing with the other is not replaced whole.
    //! 7. Applying the returned edit to `old` does not give `new`, whatever
    //!    the range looked like. Every test below applies the edit.

    use super::minimal_edit;
    use std::ops::Range;

    /// Applies an edit the way the editor will: replace the range of `old`
    /// with the text. Slicing panics on a split character, so a bad range
    /// fails here as well as in the comparison.
    fn apply(old: &str, edit: &(Range<usize>, String)) -> String {
        let (range, text) = edit;
        format!("{}{}{}", &old[..range.start], text, &old[range.end..])
    }

    #[test]
    fn identical_texts_need_no_edit() {
        assert_eq!(minimal_edit("same\n", "same\n"), None);
        assert_eq!(minimal_edit("", ""), None);
    }

    #[test]
    fn an_insert_in_the_middle_is_one_small_edit() {
        let old = "one\ntwo\nthree\n";
        let new = "one\ntwo\nNEW\nthree\n";
        let edit = minimal_edit(old, new).expect("the texts differ");
        assert_eq!(edit, (8..8, "NEW\n".to_string()));
        assert_eq!(apply(old, &edit), new);
    }

    #[test]
    fn an_edit_never_splits_a_character() {
        // `è` (C3 A8) and `é` (C3 A9) share their lead byte: the prefix stops before it.
        let edit = minimal_edit("aè", "aé").expect("the texts differ");
        assert_eq!(edit, (1..3, "é".to_string()));
        assert_eq!(apply("aè", &edit), "aé");
    }

    #[test]
    fn a_shared_trailing_byte_never_splits_a_character() {
        // `à` (C3 A0) and `Ġ` (C4 A0) share their trailing byte: the suffix stops after it.
        let edit = minimal_edit("à", "Ġ").expect("the texts differ");
        assert_eq!(edit, (0..2, "Ġ".to_string()));
        assert_eq!(apply("à", &edit), "Ġ");
    }

    #[test]
    fn a_full_rewrite_replaces_everything() {
        let old = "alpha\nbeta\n";
        let new = "gamma";
        let edit = minimal_edit(old, new).expect("the texts differ");
        assert_eq!(edit, (0..old.len(), new.to_string()));
        assert_eq!(apply(old, &edit), new);
    }

    #[test]
    fn a_shared_prefix_and_suffix_never_overlap() {
        // The one shared `a` is counted once, from either end.
        let edit = minimal_edit("aa", "aaa").expect("the texts differ");
        assert_eq!(edit, (2..2, "a".to_string()));
        assert_eq!(apply("aa", &edit), "aaa");

        // The prefix takes all of `abc`, leaving no suffix to count again.
        let edit = minimal_edit("abcabc", "abc").expect("the texts differ");
        assert_eq!(edit, (3..6, String::new()));
        assert_eq!(apply("abcabc", &edit), "abc");
    }

    #[test]
    fn an_empty_side_is_a_pure_insert_or_delete() {
        let edit = minimal_edit("", "abc").expect("the texts differ");
        assert_eq!(edit, (0..0, "abc".to_string()));
        assert_eq!(apply("", &edit), "abc");

        let edit = minimal_edit("abc", "").expect("the texts differ");
        assert_eq!(edit, (0..3, String::new()));
        assert_eq!(apply("abc", &edit), "");
    }
}
