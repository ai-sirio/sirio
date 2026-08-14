/// Linux uses GPUI's `platform` modifier for the Super key. On macOS the same
/// GPUI bit is the Command key, so keeping this decision at the terminal
/// boundary makes the owning pane's routing platform-independent.
pub fn opens_terminal_link(platform_modifier: bool) -> bool {
    platform_modifier
}

pub fn url_at_column(line: &str, column: usize) -> Option<String> {
    let starts = ["https://", "http://", "file://"];
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let start = starts
            .iter()
            .find(|prefix| line[index..].starts_with(**prefix))
            .map(|prefix| index + prefix.len());
        let Some(end_start) = start else {
            index += line[index..].chars().next().map_or(1, char::len_utf8);
            continue;
        };
        let mut end = end_start;
        while end < bytes.len() && !bytes[end].is_ascii_whitespace() {
            end += 1;
        }
        if (index..end).contains(&column) {
            let mut url = line[index..end].to_string();
            while matches!(url.chars().last(), Some('.') | Some(',') | Some(';')) {
                url.pop();
            }
            return (!url.is_empty()).then_some(url);
        }
        index = end;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_link_gesture_is_platform_modifier_only() {
        assert!(opens_terminal_link(true));
        assert!(!opens_terminal_link(false));
    }

    #[test]
    fn plain_urls_are_clickable_without_consuming_trailing_punctuation() {
        let line = "open https://example.test/docs, then";
        assert_eq!(
            url_at_column(line, 10),
            Some("https://example.test/docs".to_string())
        );
        assert_eq!(url_at_column(line, 0), None);
    }
}
