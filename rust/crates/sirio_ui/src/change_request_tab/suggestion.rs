//! Suggested changes in a comment's body (spec §7.3): the ```suggestion
//! block GitHub and GitLab both write, read into parts to draw, and the
//! block *Suggest* writes.

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BodyPart {
    Text(String),
    /// The proposed lines. `above` / `below` are GitLab's `:-A+B`: lines
    /// before and after the anchored one the suggestion also replaces.
    Suggestion { lines: Vec<String>, above: u32, below: u32 },
}

/// A fence line: its indent (at most 3 spaces), its backtick count (at
/// least 3) and what follows the backticks.
fn fence(line: &str) -> Option<(usize, &str)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let ticks = rest.len() - rest.trim_start_matches('`').len();
    (ticks >= 3).then(|| (ticks, &rest[ticks..]))
}

/// An opening ```suggestion fence: its backtick count and GitLab's offsets.
fn opening(line: &str) -> Option<(usize, u32, u32)> {
    let (ticks, info) = fence(line)?;
    let tail = info.trim_end().strip_prefix("suggestion")?;
    if tail.is_empty() {
        return Some((ticks, 0, 0));
    }
    let (above, below) = tail.strip_prefix(":-")?.split_once('+')?;
    Some((ticks, above.parse().ok()?, below.parse().ok()?))
}

fn closes(line: &str, opened: usize) -> bool {
    fence(line).is_some_and(|(ticks, rest)| ticks >= opened && rest.trim().is_empty())
}

/// A comment body around its suggestions.
// Task 10 draws a received suggestion from these parts.
pub(crate) fn split(body: &str) -> Vec<BodyPart> {
    let body = body.replace("\r\n", "\n");
    let lines: Vec<&str> = body.split('\n').collect();
    let mut parts = Vec::new();
    let mut text: Vec<&str> = Vec::new();
    let flush = |text: &mut Vec<&str>, parts: &mut Vec<BodyPart>| {
        if text.iter().any(|line| !line.trim().is_empty()) {
            parts.push(BodyPart::Text(text.join("\n").trim_matches('\n').to_string()));
        }
        text.clear();
    };
    let mut index = 0;
    while index < lines.len() {
        if let Some((ticks, above, below)) = opening(lines[index])
            && let Some(close) = (index + 1..lines.len()).find(|&at| closes(lines[at], ticks))
        {
            flush(&mut text, &mut parts);
            parts.push(BodyPart::Suggestion {
                lines: lines[index + 1..close].iter().map(|line| line.to_string()).collect(),
                above,
                below,
            });
            index = close + 1;
            continue;
        }
        text.push(lines[index]);
        index += 1;
    }
    flush(&mut text, &mut parts);
    parts
}

/// The lines a suggestion replaces, read from the end of the code its thread
/// quoted (B3c revision (e)): the last `count` lines on the new side, each
/// without its one-character prefix. `None` when the hunk is shorter.
// Task 10 reads the "before" lines from the thread's quoted hunk through this.
pub(crate) fn replaced_lines(hunk: &str, count: usize) -> Option<Vec<String>> {
    let new_side: Vec<String> = hunk
        .lines()
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .filter(|line| !line.starts_with("@@") && !line.starts_with('-') && !line.starts_with('\\'))
        .map(|line| line.get(1..).unwrap_or_default().to_string())
        .collect();
    (count > 0 && new_side.len() >= count).then(|| new_side[new_side.len() - count..].to_vec())
}

/// The block *Suggest* inserts: the anchored lines as they read now, in a
/// fence longer than any backtick run they hold.
pub(crate) fn suggestion_block(lines: &[String]) -> String {
    let longest = lines
        .iter()
        .flat_map(|line| line.split(|c| c != '`').map(str::len))
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    format!("{fence}suggestion\n{}\n{fence}", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(words: &str) -> BodyPart {
        BodyPart::Text(words.to_string())
    }

    fn suggested(lines: &[&str]) -> BodyPart {
        BodyPart::Suggestion { lines: lines.iter().map(|line| line.to_string()).collect(), above: 0, below: 0 }
    }

    #[test]
    fn a_body_without_a_suggestion_is_one_text() {
        assert_eq!(split("Looks fine.\n\n```rust\nlet x = 1;\n```"), vec![text("Looks fine.\n\n```rust\nlet x = 1;\n```")]);
    }

    #[test]
    fn a_suggestion_splits_the_text_around_it() {
        assert_eq!(
            split("Use this:\n```suggestion\nlet x = 2;\nlet y = 3;\n```\nThen test it."),
            vec![text("Use this:"), suggested(&["let x = 2;", "let y = 3;"]), text("Then test it.")]
        );
    }

    #[test]
    fn a_longer_fence_holds_a_line_of_three_backticks() {
        assert_eq!(split("````suggestion\n```\nquoted\n```\n````"), vec![suggested(&["```", "quoted", "```"])]);
    }

    #[test]
    fn an_unclosed_suggestion_stays_text() {
        assert_eq!(split("```suggestion\nlet x = 2;"), vec![text("```suggestion\nlet x = 2;")]);
    }

    #[test]
    fn crlf_reads_as_lf() {
        assert_eq!(split("```suggestion\r\nlet x = 2;\r\n```\r\n"), vec![suggested(&["let x = 2;"])]);
    }

    #[test]
    fn an_empty_suggestion_deletes_the_lines() {
        assert_eq!(split("```suggestion\n```"), vec![suggested(&[])]);
    }

    #[test]
    fn gitlab_s_offsets_are_read() {
        assert_eq!(
            split("```suggestion:-1+2\na\n```"),
            vec![BodyPart::Suggestion { lines: vec!["a".into()], above: 1, below: 2 }]
        );
    }

    #[test]
    fn only_the_word_suggestion_opens_one() {
        for body in ["```suggestions\na\n```", "```suggestion please\na\n```", "    ```suggestion\na\n```", "``suggestion\na\n``"] {
            assert!(matches!(split(body).as_slice(), [BodyPart::Text(_)]), "{body:?}");
        }
    }

    #[test]
    fn the_replaced_lines_are_the_hunk_s_last_new_side_lines() {
        let hunk = "@@ -40,4 +40,4 @@\n line 40\n-old 41\n+new 41\n line 42\n line 43";
        assert_eq!(replaced_lines(hunk, 1), Some(vec!["line 43".to_string()]));
        assert_eq!(
            replaced_lines(hunk, 3),
            Some(vec!["new 41".to_string(), "line 42".to_string(), "line 43".to_string()])
        );
        assert_eq!(replaced_lines(hunk, 5), None, "the hunk does not reach that far");
        assert_eq!(replaced_lines(hunk, 0), None);
        assert_eq!(replaced_lines(" a\r\n b\r\n\\ No newline at end of file", 2), Some(vec!["a".to_string(), "b".to_string()]));
    }

    #[test]
    fn the_block_suggest_writes_is_read_back_as_the_same_lines() {
        let plain = vec!["let x = 1;".to_string()];
        assert_eq!(suggestion_block(&plain), "```suggestion\nlet x = 1;\n```");
        let fenced = vec!["```".to_string(), "code".to_string()];
        let block = suggestion_block(&fenced);
        assert!(block.starts_with("````suggestion\n"), "{block}");
        assert_eq!(split(&block), vec![BodyPart::Suggestion { lines: fenced, above: 0, below: 0 }]);
    }
}
