//! The composer: bezel's `TextField` in the gallery's Composer card, with the
//! `/` and `@` tokens read off the text the way the gallery's `reread` does.
//!
//! Transcribed from `crabtalk/bezel` tag `v0.1.4`,
//! `apps/gallery/src/patterns/agent.rs` (the `Composer` section). A chip used
//! to be one atomic position in a hand-rolled document; now a skill is the
//! `/name ` prefix as text, a file mention is `@path ` as text with the path
//! remembered in `Chat::accepted_mentions`, and an image is an entry in
//! `Chat::attachments` drawn as a strip above the field.

/// The active `/`-token: the whole draft is one unbroken word starting with
/// a slash. Whitespace anywhere ends the token, exactly as the reference
/// `slashTokenRange` did.
pub(crate) fn slash_token(text: &str) -> Option<&str> {
    let rest = text.strip_prefix('/')?;
    (!rest.chars().any(char::is_whitespace)).then_some(rest)
}

/// The active `@`-token behind `caret`: the nearest `@` with nothing but
/// non-whitespace between it and the caret. Returns the byte index of the
/// `@` and the token after it. A read of the text rather than a key handler,
/// so typing, pasting, arrowing back into a word and deleting the `@` all
/// agree without special cases.
pub(crate) fn mention_token(text: &str, caret: usize) -> Option<(usize, &str)> {
    let caret = caret.min(text.len());
    let head = text.get(..caret)?;
    let at = head.rfind('@')?;
    let token = &head[at + 1..];
    (!token.chars().any(char::is_whitespace)).then_some((at, token))
}

/// What the composer hands to the ACP layer: the text with every accepted
/// `@path` token removed, and those paths as mention paths (deduplicated, in
/// the order they appear). A token the user edited no longer matches an
/// accepted path and stays as plain text — the same triple the old chip
/// document produced.
pub(crate) fn assemble_prompt(text: &str, accepted: &[String]) -> (String, Vec<String>) {
    let mut out = String::with_capacity(text.len());
    let mut mention_paths: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find('@') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let token_end = after.find(char::is_whitespace).unwrap_or(after.len());
        let token = &after[..token_end];
        let boundary_before = out.is_empty() || out.ends_with(char::is_whitespace);
        if boundary_before && !token.is_empty() && accepted.iter().any(|path| path == token) {
            if !mention_paths.iter().any(|path| path == token) {
                mention_paths.push(token.to_string());
            }
            let mut skip = token_end;
            if after[token_end..].starts_with(' ') {
                skip += 1;
            }
            rest = &after[skip..];
        } else {
            out.push('@');
            rest = after;
        }
    }
    out.push_str(rest);
    (out, mention_paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_token_is_the_unbroken_leading_word() {
        assert_eq!(slash_token("/"), Some(""));
        assert_eq!(slash_token("/cr"), Some("cr"));
        assert_eq!(slash_token("/cr "), None);
        assert_eq!(slash_token("hi /cr"), None);
        assert_eq!(slash_token(""), None);
    }

    #[test]
    fn mention_token_is_the_at_nearest_behind_the_caret() {
        assert_eq!(mention_token("see @src/ma", 11), Some((4, "src/ma")));
        assert_eq!(mention_token("see @", 5), Some((4, "")));
        assert_eq!(mention_token("see @src done", 13), None);
        assert_eq!(mention_token("see @src done", 8), Some((4, "src")));
        assert_eq!(mention_token("no at here", 10), None);
        assert_eq!(
            mention_token("@a", 99),
            Some((0, "a")),
            "caret past the end clamps"
        );
    }

    #[test]
    fn assemble_prompt_lifts_accepted_tokens_into_mention_paths() {
        let accepted = vec!["src/main.rs".to_string()];
        let (text, paths) = assemble_prompt("fix @src/main.rs please", &accepted);
        assert_eq!(text, "fix please");
        assert_eq!(paths, vec!["src/main.rs".to_string()]);
    }

    #[test]
    fn assemble_prompt_leaves_edited_and_unaccepted_tokens_as_text() {
        let accepted = vec!["src/main.rs".to_string()];
        let (text, paths) = assemble_prompt("fix @src/main.rss and @other", &accepted);
        assert_eq!(text, "fix @src/main.rss and @other");
        assert!(paths.is_empty());
        let (text, paths) = assemble_prompt("mail me@example.com", &["example.com".to_string()]);
        assert_eq!(text, "mail me@example.com", "an @ inside a word is not a token");
        assert!(paths.is_empty());
    }

    #[test]
    fn assemble_prompt_deduplicates_and_keeps_text_order() {
        let accepted = vec!["b.rs".to_string(), "a.rs".to_string()];
        let (text, paths) = assemble_prompt("@a.rs then @b.rs then @a.rs end", &accepted);
        assert_eq!(text, "then then end");
        assert_eq!(paths, vec!["a.rs".to_string(), "b.rs".to_string()]);
    }
}
