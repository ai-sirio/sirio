//! The root `<svg>` element's size, read without changing the markup.

use std::ops::Range;

use crate::Svg;

/// Returns the SVG as rendered and its logical size. `None` when there is no
/// `<svg` root or no size.
pub(crate) fn as_rendered(markup: &str) -> Option<Svg> {
    let (start, end) = root_tag(markup)?;
    let (width, height) = size_of(&markup[start..end])?;
    Some(Svg {
        markup: markup.to_owned(),
        logical_width: width.ceil() as u32,
        logical_height: height.ceil() as u32,
    })
}

/// The root's width and height, as the file states them.
pub(crate) fn root_size(markup: &str) -> Option<(f64, f64)> {
    let (start, end) = root_tag(markup)?;
    size_of(&markup[start..end])
}

/// The byte range of the root start tag, `<svg` through its `>`.
fn root_tag(markup: &str) -> Option<(usize, usize)> {
    let start = markup.find("<svg")?;
    let end = start + markup[start..].find('>')? + 1;
    Some((start, end))
}

fn size_of(tag: &str) -> Option<(f64, f64)> {
    let from_attributes = attr(tag, "width")
        .and_then(|(_, value)| length(value))
        .zip(attr(tag, "height").and_then(|(_, value)| length(value)));
    from_attributes
        .or_else(|| {
            let (_, view_box) = attr(tag, "viewBox")?;
            let numbers: Vec<f64> = view_box
                .split([' ', ','])
                .filter(|part| !part.is_empty())
                .filter_map(|part| part.parse().ok())
                .collect();
            match numbers.as_slice() {
                [_, _, width, height] => Some((*width, *height)),
                _ => None,
            }
        })
        .filter(|(width, height)| *width > 0.0 && *height > 0.0)
}

/// A length in user units. `px` is accepted; a percentage is not a size.
fn length(value: &str) -> Option<f64> {
    let value = value.trim();
    value
        .strip_suffix("px")
        .unwrap_or(value)
        .trim()
        .parse()
        .ok()
}

/// Attribute `name` in a start tag: the range of the whole `name="…"` and
/// its value. Only a match preceded by whitespace counts, so `width` never
/// matches inside `stroke-width`.
fn attr<'a>(tag: &'a str, name: &str) -> Option<(Range<usize>, &'a str)> {
    let pattern = format!("{name}=");
    let mut from = 0;
    while let Some(found) = tag[from..].find(&pattern) {
        let at = from + found;
        from = at + pattern.len();
        if !tag[..at].ends_with(|c: char| c.is_ascii_whitespace()) {
            continue;
        }
        let quote = tag[from..].chars().next()?;
        if quote != '"' && quote != '\'' {
            continue;
        }
        let value_start = from + 1;
        let value_end = value_start + tag[value_start..].find(quote)?;
        return Some((at..value_end + 1, &tag[value_start..value_end]));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mermaid_root_is_preserved_and_its_logical_size_kept() {
        let markup = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"275.3827\" height=\"336.9915\" viewBox=\"0 0 275.3827 336.9915\"><rect width=\"275.3827\"/></svg>";
        let svg = as_rendered(markup).expect("a sized root");
        assert_eq!(svg.markup, markup);
        assert_eq!((svg.logical_width, svg.logical_height), (276, 337));
    }

    #[test]
    fn svg_markup_is_unchanged() {
        let markup = "<svg stroke-width=\"3\" width=\"10\" height=\"20\"></svg>";
        let svg = as_rendered(markup).expect("a sized root");
        assert_eq!(svg.markup, markup);
    }

    #[test]
    fn a_root_sized_only_by_its_view_box_keeps_a_size() {
        let markup = "<svg viewBox=\"0 0 100 50\"></svg>";
        let svg = as_rendered(markup).expect("a view box");
        assert_eq!(svg.markup, markup);
        assert_eq!((svg.logical_width, svg.logical_height), (100, 50));
    }

    #[test]
    fn a_plantuml_root_keeps_its_style_size_and_prolog() {
        let markup = "<?xml version=\"1.0\"?><svg height=\"123px\" style=\"width:456px;height:123px;background:#FFFFFF;\" viewBox=\"0 0 456 123\" width=\"456px\"><g/></svg>";
        let svg = as_rendered(markup).expect("a sized root");
        assert_eq!(svg.markup, markup);
        assert_eq!((svg.logical_width, svg.logical_height), (456, 123));
    }

    #[test]
    fn markup_without_an_svg_root_is_refused() {
        assert_eq!(as_rendered("<html></html>"), None);
        assert_eq!(as_rendered("<svg></svg>"), None, "no size, no view box");
    }

    #[test]
    fn the_root_size_is_read_as_written() {
        assert_eq!(
            root_size("<svg width=\"12\" height=\"8\"/>"),
            Some((12.0, 8.0))
        );
    }
}
