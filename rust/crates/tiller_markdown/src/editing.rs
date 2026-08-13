/// A byte range in a UTF-8 document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectionRange {
    pub start: usize,
    pub end: usize,
}

impl SelectionRange {
    pub fn new(start: usize, end: usize) -> Option<Self> {
        (start <= end).then_some(Self { start, end })
    }
}

/// Text and selection after a markdown edit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditedText {
    pub text: String,
    pub selection: SelectionRange,
}

/// Wraps the selected bytes with `prefix` and `suffix`.
pub fn wrap_selection(
    source: &str,
    selection: SelectionRange,
    prefix: &str,
    suffix: &str,
) -> Option<EditedText> {
    if !source.is_char_boundary(selection.start) || !source.is_char_boundary(selection.end) {
        return None;
    }
    let mut text = String::with_capacity(source.len() + prefix.len() + suffix.len());
    text.push_str(&source[..selection.start]);
    text.push_str(prefix);
    text.push_str(&source[selection.start..selection.end]);
    text.push_str(suffix);
    text.push_str(&source[selection.end..]);
    Some(EditedText {
        text,
        selection: SelectionRange {
            start: selection.start + prefix.len(),
            end: selection.end + prefix.len(),
        },
    })
}

/// Prefixes every source line touched by the selection.
pub fn prefix_selected_lines(
    source: &str,
    selection: SelectionRange,
    prefix: &str,
) -> Option<EditedText> {
    if !source.is_char_boundary(selection.start) || !source.is_char_boundary(selection.end) {
        return None;
    }
    let first_line = source[..selection.start]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let mut starts = vec![first_line];
    let scan_end = selection.end.min(source.len());
    let mut cursor = first_line;
    while let Some(relative) = source[cursor..scan_end].find('\n') {
        let next = cursor + relative + 1;
        if next >= scan_end {
            break;
        }
        starts.push(next);
        cursor = next;
    }
    let mut text = String::with_capacity(source.len() + starts.len() * prefix.len());
    let mut previous = 0;
    for start in starts {
        text.push_str(&source[previous..start]);
        text.push_str(prefix);
        previous = start;
    }
    text.push_str(&source[previous..]);

    let prefixes_before_start = text_prefix_count_before(
        &starts_for(source, first_line, scan_end),
        selection.start,
        prefix.len(),
    );
    let prefixes_before_end = text_prefix_count_before(
        &starts_for(source, first_line, scan_end),
        selection.end,
        prefix.len(),
    );
    Some(EditedText {
        text,
        selection: SelectionRange {
            start: selection.start + prefixes_before_start,
            end: selection.end + prefixes_before_end,
        },
    })
}

fn starts_for(source: &str, first_line: usize, scan_end: usize) -> Vec<usize> {
    let mut starts = vec![first_line];
    let mut cursor = first_line;
    while let Some(relative) = source[cursor..scan_end].find('\n') {
        let next = cursor + relative + 1;
        if next >= scan_end {
            break;
        }
        starts.push(next);
        cursor = next;
    }
    starts
}

fn text_prefix_count_before(starts: &[usize], offset: usize, prefix_len: usize) -> usize {
    starts
        .iter()
        .filter(|start| **start < offset || (**start == offset && offset > 0))
        .count()
        * prefix_len
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_selection_moves_the_selection_but_keeps_its_content() {
        let result = wrap_selection(
            "hello world",
            SelectionRange::new(6, 11).unwrap(),
            "**",
            "**",
        )
        .unwrap();
        assert_eq!(result.text, "hello **world**");
        assert_eq!(result.selection, SelectionRange::new(8, 13).unwrap());
    }

    #[test]
    fn prefixing_selected_lines_recalculates_both_selection_edges() {
        let result =
            prefix_selected_lines("one\ntwo\nthree", SelectionRange::new(1, 9).unwrap(), "> ")
                .unwrap();
        assert_eq!(result.text, "> one\n> two\n> three");
        assert_eq!(result.selection, SelectionRange::new(3, 15).unwrap());
    }
}
