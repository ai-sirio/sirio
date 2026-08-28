//! `"#RRGGBB"` / `"#RRGGBBAA"` parsing shared by the transcribed COSMIC
//! defaults ([`super::palette`]) and the live on-disk reader
//! ([`super::live`]) — both consume the same string shape COSMIC itself
//! writes (`cosmic-theme`'s `ColorRepr::Hex`, via `hex_color::rgba`).

use gpui::Rgba;

/// Parses a `#RRGGBB` or `#RRGGBBAA` string into an sRGB [`Rgba`].
///
/// Returns `None` for anything else — callers fall back to a transcribed
/// default rather than panic on a malformed or future config format.
pub(crate) fn parse(s: &str) -> Option<Rgba> {
    let s = s.strip_prefix('#')?;
    let component = |range: std::ops::Range<usize>| -> Option<f32> {
        u8::from_str_radix(s.get(range)?, 16)
            .ok()
            .map(|v| v as f32 / 255.0)
    };
    let r = component(0..2)?;
    let g = component(2..4)?;
    let b = component(4..6)?;
    let a = if s.len() >= 8 { component(6..8)? } else { 1.0 };
    Some(Rgba { r, g, b, a })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_eight_digit_hex_with_alpha() {
        let c = parse("#63D0DFFF").expect("valid hex");
        assert!((c.r - 0x63 as f32 / 255.0).abs() < 1e-6);
        assert!((c.g - 0xD0 as f32 / 255.0).abs() < 1e-6);
        assert!((c.b - 0xDF as f32 / 255.0).abs() < 1e-6);
        assert!((c.a - 1.0).abs() < 1e-6);
    }

    #[test]
    fn parses_six_digit_hex_as_opaque() {
        let c = parse("#161616").expect("valid hex");
        assert!((c.a - 1.0).abs() < 1e-6);
    }

    #[test]
    fn parses_partial_alpha() {
        let c = parse("#FFFFFF33").expect("valid hex");
        assert!((c.a - 0x33 as f32 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn rejects_malformed_input() {
        assert!(parse("not a color").is_none());
        assert!(parse("#GGGGGG").is_none());
        assert!(parse("123456").is_none());
    }
}
