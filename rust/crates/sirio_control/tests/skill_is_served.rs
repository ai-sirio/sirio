//! The skill is checked against a release build (spec §4.4): every `sirioctl`
//! command in its code blocks must parse, and the method it would send must be
//! one a release build serves.

use std::process::Command;

use sirio_control::served::served_methods;

/// The arguments of each `sirioctl` command the skill's code blocks show, without
/// the program name. A command starts at each `sirioctl` word, so `$(sirioctl …)`
/// and `trap '…'` count; it ends at an unquoted `|`, `;`, `&` or `)`, or at the
/// quote that encloses it.
fn skill_commands(skill: &str) -> Vec<Vec<String>> {
    let mut commands = Vec::new();
    let mut in_block = false;
    for line in skill.lines() {
        if line.trim_start().starts_with("```") {
            in_block = !in_block;
            continue;
        }
        if !in_block || line.trim_start().starts_with('#') {
            continue;
        }
        for (start, _) in line.match_indices("sirioctl") {
            let before = line[..start].chars().next_back();
            let after = line[start + "sirioctl".len()..].chars().next();
            let starts_word =
                before.is_none_or(|c| c.is_whitespace() || matches!(c, '(' | '\'' | '"'));
            let ends_word = after.is_none_or(char::is_whitespace);
            if starts_word && ends_word {
                let mut words = command_words(&line[start..], enclosing_quote(&line[..start]));
                words.remove(0);
                commands.push(words);
            }
        }
    }
    commands
}

/// The quote still open at the end of `prefix`, if any: `trap '` leaves `'` open.
fn enclosing_quote(prefix: &str) -> Option<char> {
    let mut quote = None;
    for c in prefix.chars() {
        quote = match quote {
            None if c == '\'' || c == '"' => Some(c),
            Some(open) if c == open => None,
            other => other,
        };
    }
    quote
}

/// The shell words at the start of `source`. Quotes group, and `"$NAME"` reads as
/// `placeholder`, the value the shell would fill in.
fn command_words(source: &str, enclosing: Option<char>) -> Vec<String> {
    let mut words = Vec::new();
    let mut word: Option<String> = None;
    let mut quote: Option<char> = None;
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        match quote {
            Some(open) if c == open => quote = None,
            Some('"') if c == '$' => {
                while chars
                    .peek()
                    .is_some_and(|next| next.is_alphanumeric() || *next == '_')
                {
                    chars.next();
                }
                word.get_or_insert_with(String::new).push_str("placeholder");
            }
            Some(_) => word.get_or_insert_with(String::new).push(c),
            None if Some(c) == enclosing => break,
            None if matches!(c, '|' | ';' | '&' | ')') => break,
            None if c == ' ' || c == '\t' => {
                if let Some(done) = word.take() {
                    words.push(done);
                }
            }
            None if c == '\'' || c == '"' => {
                quote = Some(c);
                word.get_or_insert_with(String::new);
            }
            None => word.get_or_insert_with(String::new).push(c),
        }
    }
    assert!(quote.is_none(), "unbalanced quote in `{source}`");
    if let Some(done) = word {
        words.push(done);
    }
    words
}

/// The method a `--dry-run` of `words` names, or the failure to produce one.
fn dry_run_method(words: &[String], socket: &str) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_sirioctl"))
        .arg("--dry-run")
        .args(words)
        .env("SIRIO_PANE_ID", "pane-placeholder")
        .env("SIRIO_WORKTREE_ID", "worktree-placeholder")
        .env("SIRIO_SOCKET", socket)
        .output()
        .expect("sirioctl runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "`sirioctl {}` exited {:?}: {stderr}",
        words.join(" "),
        output.status.code()
    );
    let request: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("the dry run prints one JSON request");
    request["method"]
        .as_str()
        .expect("the request names a method")
        .to_string()
}

#[test]
fn every_command_the_skill_names_is_served_by_a_release_build() {
    let skill = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../skills/sirio/SKILL.md"
    ))
    .expect("the skill is readable");
    let commands = skill_commands(&skill);
    assert!(
        commands.len() >= 20,
        "the parse found {} commands in the skill; a parse that matches nothing must not pass",
        commands.len()
    );

    let served = served_methods(false);
    for words in &commands {
        let method = dry_run_method(words, "/nonexistent/sirio-skill-check.sock");
        assert!(
            served.iter().any(|served| *served == method),
            "`sirioctl {}` sends {method}, which a release build does not serve",
            words.join(" ")
        );
    }
}

#[test]
fn dry_run_contacts_nothing() {
    let socket = "/nonexistent/sirio-dry-run.sock";
    assert_eq!(dry_run_method(&["ping".to_string()], socket), "system.ping");
    assert_eq!(
        dry_run_method(
            &[
                "panel".to_string(),
                "create".to_string(),
                "--cmd".to_string(),
                "codex".to_string(),
            ],
            socket,
        ),
        "panel.create"
    );
}
