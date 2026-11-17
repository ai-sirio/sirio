//! Text from an agent, made safe to show a user who is about to approve it.
//!
//! Two rules, kept apart on purpose. `visible_text` never removes a byte of
//! what the user approves: a hidden character is shown as `⟨U+XXXX⟩`, so a
//! command that carries one reads as carrying one. `strip_escape_sequences` is only for
//! prose the agent decorated with colour codes (a reason), never for a
//! command or a path.

/// Whether `character` is one an agent's text must not carry onto the screen
/// unseen: a control character (ESC included), a bidi control that can
/// reorder what follows it, a zero-width or invisible one, a tag character,
/// a line or paragraph separator, or a soft hyphen.
pub(super) fn is_hidden(character: char) -> bool {
    character.is_control()
        || matches!(
            character,
            '\u{00AD}'
                | '\u{061C}'
                | '\u{180E}'
                | '\u{200B}'..='\u{200F}'
                | '\u{2028}'..='\u{2029}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'
                | '\u{2061}'..='\u{2064}'
                | '\u{2066}'..='\u{2069}'
                | '\u{206A}'..='\u{206F}'
                | '\u{FEFF}'
                | '\u{FFF9}'..='\u{FFFB}'
                | '\u{E0000}'..='\u{E007F}'
        )
}

/// `text` with every hidden character shown as `⟨U+XXXX⟩` (at least four hex
/// digits, five or six for an astral code point). `multiline` keeps `\n` and
/// `\t`, for a body that may span lines; a title or a label passes `false`.
pub(super) fn visible_text(text: &str, multiline: bool) -> String {
    let mut shown = String::with_capacity(text.len());
    for character in text.chars() {
        if multiline && matches!(character, '\n' | '\t') {
            shown.push(character);
        } else if is_hidden(character) {
            shown.push_str(&format!("⟨U+{:04X}⟩", u32::from(character)));
        } else {
            shown.push(character);
        }
    }
    shown
}

/// `text` without its ANSI escape sequences: CSI (`ESC [` ... a final byte
/// from `@` to `~`) and OSC (`ESC ]` ... BEL or `ESC \`, to the end of the
/// text when unterminated). Anything else, a lone ESC included, stays, for
/// `visible_text` to show. (Unlike `background.rs`'s `strip_ansi`, which drops
/// a lone ESC: here nothing the user approves may vanish unseen.)
pub(super) fn strip_escape_sequences(text: &str) -> String {
    let mut kept = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('\u{1b}') {
        kept.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        if let Some(sequence) = after.strip_prefix('[') {
            // Parameters and intermediates, then one final byte.
            let end = sequence
                .char_indices()
                .find(|(_, byte)| !matches!(byte, '\u{20}'..='\u{3F}'))
                .filter(|(_, byte)| matches!(byte, '\u{40}'..='\u{7E}'));
            match end {
                Some((index, byte)) => rest = &sequence[index + byte.len_utf8()..],
                None => {
                    kept.push('\u{1b}');
                    rest = after;
                }
            }
        } else if let Some(string) = after.strip_prefix(']') {
            let end = string
                .char_indices()
                .find_map(|(index, byte)| match byte {
                    '\u{7}' => Some(index + 1),
                    '\u{1b}' if string[index + 1..].starts_with('\\') => Some(index + 2),
                    _ => None,
                });
            // Unterminated: the rest of the text was the string.
            rest = end.map_or("", |end| &string[end..]);
        } else {
            kept.push('\u{1b}');
            rest = after;
        }
    }
    kept.push_str(rest);
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_hidden_class_is_shown_with_its_code_point() {
        for (character, shown) in [
            ('\u{0007}', "⟨U+0007⟩"),
            ('\u{007F}', "⟨U+007F⟩"),
            ('\u{0085}', "⟨U+0085⟩"),
            ('\u{00AD}', "⟨U+00AD⟩"),
            ('\u{061C}', "⟨U+061C⟩"),
            ('\u{180E}', "⟨U+180E⟩"),
            ('\u{200B}', "⟨U+200B⟩"),
            ('\u{200F}', "⟨U+200F⟩"),
            ('\u{2028}', "⟨U+2028⟩"),
            ('\u{2029}', "⟨U+2029⟩"),
            ('\u{202A}', "⟨U+202A⟩"),
            ('\u{202E}', "⟨U+202E⟩"),
            ('\u{2060}', "⟨U+2060⟩"),
            ('\u{2061}', "⟨U+2061⟩"),
            ('\u{2064}', "⟨U+2064⟩"),
            ('\u{2066}', "⟨U+2066⟩"),
            ('\u{2069}', "⟨U+2069⟩"),
            ('\u{206A}', "⟨U+206A⟩"),
            ('\u{206F}', "⟨U+206F⟩"),
            ('\u{FEFF}', "⟨U+FEFF⟩"),
            ('\u{FFF9}', "⟨U+FFF9⟩"),
            ('\u{FFFB}', "⟨U+FFFB⟩"),
            ('\u{E0000}', "⟨U+E0000⟩"),
            ('\u{E0041}', "⟨U+E0041⟩"),
            ('\u{E007F}', "⟨U+E007F⟩"),
        ] {
            assert_eq!(
                visible_text(&format!("a{character}b"), true),
                format!("a{shown}b"),
                "U+{:04X}",
                u32::from(character)
            );
        }
    }

    #[test]
    fn an_astral_code_point_takes_five_hex_digits() {
        assert_eq!(visible_text("\u{E0001}", false), "⟨U+E0001⟩");
    }

    #[test]
    fn line_breaks_and_tabs_survive_only_in_multiline() {
        assert_eq!(visible_text("a\nb\tc", true), "a\nb\tc");
        assert_eq!(visible_text("a\nb\tc", false), "a⟨U+000A⟩b⟨U+0009⟩c");
        assert_eq!(visible_text("a\rb", true), "a⟨U+000D⟩b");
    }

    #[test]
    fn ordinary_non_ascii_text_is_kept() {
        assert_eq!(visible_text("Café ☕ 名前", true), "Café ☕ 名前");
        assert_eq!(visible_text("Café ☕ 名前", false), "Café ☕ 名前");
    }

    #[test]
    fn escape_is_shown_never_interpreted() {
        assert_eq!(visible_text("a\u{1b}[31mb", true), "a⟨U+001B⟩[31mb");
        assert_eq!(
            visible_text("x\u{1b}]0;t\u{7}y", false),
            "x⟨U+001B⟩]0;t⟨U+0007⟩y"
        );
    }

    #[test]
    fn strip_escape_sequences_removes_a_csi_sequence_and_only_that() {
        assert_eq!(strip_escape_sequences("risky\u{1b}[31m red\u{1b}[0m"), "risky red");
        // `|` is a legal final byte: the sequence ends there, the text stays.
        assert_eq!(strip_escape_sequences("a\u{1b}[1;2|b"), "ab");
        assert_eq!(strip_escape_sequences("a\u{1b}[1;2|31mb"), "a31mb");
    }

    #[test]
    fn strip_escape_sequences_removes_an_osc_to_its_terminator_or_to_the_end() {
        assert_eq!(strip_escape_sequences("a\u{1b}]0;title\u{7}b"), "ab");
        assert_eq!(strip_escape_sequences("a\u{1b}]0;title\u{1b}\\b"), "ab");
        assert_eq!(strip_escape_sequences("a\u{1b}]0;never ends"), "a");
    }

    #[test]
    fn strip_escape_sequences_leaves_a_lone_escape_for_visible_text() {
        assert_eq!(strip_escape_sequences("a\u{1b}"), "a\u{1b}");
        assert_eq!(strip_escape_sequences("a\u{1b}b"), "a\u{1b}b");
        assert_eq!(strip_escape_sequences("a\u{1b}[31"), "a\u{1b}[31");
        assert_eq!(visible_text(&strip_escape_sequences("a\u{1b}"), false), "a⟨U+001B⟩");
    }
}
