use gpui::{
    Action, App, HighlightStyle, ImageSource, KeyBinding, KeyBindingContextPredicate, SharedUri,
};
use std::{ops::Range, path::Path, rc::Rc};

/// A key binding whose keystrokes are read for the host's system: `secondary` is Command on the Mac
/// and Control elsewhere. `KeyBinding::new` reads keystrokes with no system and panics on
/// `secondary`, so bindings that name it, or that are built at run time, go through here.
pub(crate) fn load_key(
    cx: &App,
    keystrokes: &str,
    action: impl Action,
    context: Option<&str>,
) -> KeyBinding {
    let context = context.map(|context| {
        Rc::new(KeyBindingContextPredicate::parse(context).expect("a valid key context"))
    });
    KeyBinding::load(keystrokes, Box::new(action), context, false, None, cx)
        .unwrap_or_else(|error| panic!("bad key binding {keystrokes:?}: {error}"))
}

/// From upstream documents/blocks/media.rs.
pub(crate) fn source(text: &str) -> ImageSource {
    if text.starts_with("http://") || text.starts_with("https://") {
        SharedUri::from(text.to_string()).into()
    } else {
        Path::new(text).into()
    }
}
pub(crate) fn code_colors(text: &str, cx: &App) -> Vec<(Range<usize>, HighlightStyle)> {
    crate::forms::code_highlights(text, cx)
        .into_iter()
        .map(|(range, highlight)| {
            (
                range,
                HighlightStyle {
                    color: Some(highlight.color),
                    background_color: highlight.background,
                    ..HighlightStyle::default()
                },
            )
        })
        .collect()
}

/// From upstream charts/paint.rs.
/// The color series `ix` takes from the chart palette.
pub(crate) fn tint(colors: &Palette, ix: usize) -> Hsla {
    colors.chart[ix % colors.chart.len()]
}

use crate::theme::{ActiveTheme, Palette};
use gpui::{AnyElement, FontWeight, Hsla, IntoElement, SharedString, StyledText};
/// How well a query fits a text, and the byte ranges it covers.
#[derive(Debug, PartialEq)]
pub(crate) struct Fit {
    pub score: i32,
    pub hits: Vec<Range<usize>>,
}

fn fold(c: char) -> char {
    c.to_lowercase()
        .next()
        .expect("a char lowercases to at least one char")
}

/// The query's characters in order within `text`, ignoring case and spaces. Word starts and runs score higher, gaps lower.
pub(crate) fn fuzzy(query: &str, text: &str) -> Option<Fit> {
    let query: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(fold)
        .collect();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut next = 0;
    let end = match query.len() {
        0 => {
            return Some(Fit {
                score: 0,
                hits: Vec::new(),
            });
        }
        len => chars.iter().position(|&(_, c)| {
            next += usize::from(fold(c) == query[next]);
            next == len
        })?,
    };
    // ponytail: the tightest window at the first full match, not the best; score every window if rankings disappoint.
    let mut at = Vec::with_capacity(query.len());
    for ix in (0..=end).rev() {
        if at.len() < query.len() && fold(chars[ix].1) == query[query.len() - 1 - at.len()] {
            at.push(ix);
        }
    }
    at.reverse();
    let starts_word = |ix: usize| {
        ix == 0 || {
            let (before, here) = (chars[ix - 1].1, chars[ix].1);
            !before.is_alphanumeric() || (before.is_lowercase() && here.is_uppercase())
        }
    };
    let mut score = 0;
    let mut hits: Vec<Range<usize>> = Vec::new();
    for (k, &ix) in at.iter().enumerate() {
        let bonus = if starts_word(ix) { 8 } else { 0 };
        score += 16 + if k == 0 { bonus * 2 } else { bonus };
        if k > 0 {
            let gap = (ix - at[k - 1] - 1) as i32;
            score += if gap == 0 { 4 } else { -2 - gap };
        }
        let (from, c) = chars[ix];
        match hits.last_mut() {
            Some(last) if last.end == from => last.end = from + c.len_utf8(),
            _ => hits.push(from..from + c.len_utf8()),
        }
    }
    Some(Fit { score, hits })
}

/// `text` with the letters a query matched drawn in the accent color.
pub(crate) fn marked(text: SharedString, hits: Vec<Range<usize>>, cx: &App) -> AnyElement {
    let mark = HighlightStyle {
        color: Some(cx.theme().colors.accent),
        font_weight: Some(FontWeight::SEMIBOLD),
        ..Default::default()
    };
    StyledText::new(text)
        .with_highlights(hits.into_iter().map(|hit| (hit, mark)))
        .into_any_element()
}
