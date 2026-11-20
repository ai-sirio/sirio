//! Heading anchors, as GitHub writes them (design §5.7).

use std::collections::HashMap;

/// `title` as a GitHub anchor: lower case, letters, digits, `-` and `_` kept, spaces
/// turned into `-`, the rest dropped. `seen` counts the anchors given so far in the
/// document, so a repeat reads `-1`, `-2`, … in the order the headings come.
pub(crate) fn slug(title: &str, seen: &mut HashMap<String, usize>) -> String {
    let mut base = String::with_capacity(title.len());
    for ch in title.chars() {
        match ch {
            ' ' => base.push('-'),
            '-' | '_' => base.push(ch),
            _ if ch.is_alphanumeric() => base.extend(ch.to_lowercase()),
            _ => {}
        }
    }
    let mut result = base.clone();
    while seen.contains_key(&result) {
        // The counter lives under the plain anchor: a suffixed one is a name of its own.
        let count = seen.entry(base.clone()).or_insert(0);
        *count += 1;
        result = format!("{base}-{count}");
    }
    seen.insert(result.clone(), 0);
    result
}
