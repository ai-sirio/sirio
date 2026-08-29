//! Terminal escape-sequence normalization shared by activity detectors.

/// Removes ANSI CSI and OSC sequences from terminal text.
///
/// CSI sequences end at a final byte in `@`..`~`; OSC sequences end at BEL or
/// the ST two-byte terminator (`ESC \\`). Ordinary text and unknown escape
/// sequences are preserved so this helper cannot silently discard content it
/// does not understand.
pub fn strip_ansi(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] != 0x1b || index + 1 >= bytes.len() {
            let character = input[index..]
                .chars()
                .next()
                .expect("byte index is on a UTF-8 boundary");
            output.push(character);
            index += character.len_utf8();
            continue;
        }

        match bytes[index + 1] {
            b'[' => {
                index += 2;
                while index < bytes.len() {
                    let byte = bytes[index];
                    index += 1;
                    if (0x40..=0x7e).contains(&byte) {
                        break;
                    }
                }
            }
            b']' => {
                index += 2;
                while index < bytes.len() {
                    match bytes[index] {
                        0x07 => {
                            index += 1;
                            break;
                        }
                        0x1b if bytes.get(index + 1) == Some(&b'\\') => {
                            index += 2;
                            break;
                        }
                        _ => index += 1,
                    }
                }
            }
            _ => {
                let character = input[index..]
                    .chars()
                    .next()
                    .expect("byte index is on a UTF-8 boundary");
                output.push(character);
                index += character.len_utf8();
            }
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::strip_ansi;

    #[test]
    fn strips_csi_and_both_osc_terminators() {
        assert_eq!(
            strip_ansi("\x1b[31mred\x1b[0m\x1b]0;title\x07 plain\x1b]0;title\x1b\\"),
            "red plain"
        );
    }

    #[test]
    fn preserves_unicode_and_unknown_escape_sequences() {
        assert_eq!(strip_ansi("π \x1bX title"), "π \x1bX title");
    }
}
