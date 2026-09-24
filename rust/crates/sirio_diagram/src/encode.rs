//! PlantUML's text encoding: raw DEFLATE, then its own base64 alphabet
//! (`0-9A-Za-z-_`), 3 bytes to 4 characters, the last group zero-padded.

use std::io::Write;

pub(crate) const ALPHABET: &[u8; 64] =
    b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz-_";

pub(crate) fn encode(text: &str) -> String {
    let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
    encoder
        .write_all(text.as_bytes())
        .expect("writing to a Vec cannot fail");
    let compressed = encoder.finish().expect("finishing into a Vec cannot fail");
    let mut out = String::with_capacity(compressed.len().div_ceil(3) * 4);
    for chunk in compressed.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        for value in [
            first >> 2,
            ((first & 0x03) << 4) | (second >> 4),
            ((second & 0x0F) << 2) | (third >> 6),
            third & 0x3F,
        ] {
            out.push(char::from(ALPHABET[usize::from(value)]));
        }
    }
    out
}

/// The inverse, for tests: the encoder is verified by round trip.
#[cfg(test)]
pub(crate) fn decode(encoded: &str) -> Option<String> {
    use std::io::Read;
    let values: Vec<u8> = encoded
        .bytes()
        .map(|byte| ALPHABET.iter().position(|&c| c == byte).map(|p| p as u8))
        .collect::<Option<_>>()?;
    let mut bytes = Vec::with_capacity(values.len() / 4 * 3);
    for group in values.chunks(4) {
        let value = |index: usize| group.get(index).copied().unwrap_or(0);
        bytes.push((value(0) << 2) | (value(1) >> 4));
        bytes.push(((value(1) & 0x0F) << 4) | (value(2) >> 2));
        bytes.push(((value(2) & 0x03) << 6) | value(3));
    }
    let mut text = String::new();
    flate2::read::DeflateDecoder::new(&bytes[..])
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example published on plantuml.com's text-encoding page.
    #[test]
    fn the_documented_example_decodes_to_its_text() {
        assert_eq!(
            decode("Syp9J4vLqBLJSCfFib9mB2t9ICqhoKnEBCdCprC8IYqiJIqkuGBAAUW2rO0LOr5LN92VLvpA1G00")
                .as_deref(),
            Some("Alice -> Bob: Authentication Request\nBob --> Alice: Authentication Response\n")
        );
    }

    /// DEFLATE output legitimately differs between implementations, so the
    /// encoder is checked by round trip, never against fixed bytes.
    #[test]
    fn encoding_round_trips() {
        for text in ["@startuml\nA -> B\n@enduml\n", "Alice -> Bób: ciao ✓", ""] {
            assert_eq!(decode(&encode(text)).as_deref(), Some(text));
        }
    }

    #[test]
    fn encoding_uses_only_the_plantuml_alphabet() {
        assert!(
            encode("A -> B: hello")
                .bytes()
                .all(|byte| ALPHABET.contains(&byte))
        );
    }
}
