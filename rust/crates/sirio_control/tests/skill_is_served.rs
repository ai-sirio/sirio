//! The skill is checked against a release build (spec §4.4): every `sirioctl`
//! command in its code blocks must parse, and the method it would send must be
//! one a release build serves.

use std::process::{Command, Output};

use sirio_control::served::{BROWSER_NOT_SUPPORTED, served_methods};

fn read_skill() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../skills/sirio/SKILL.md"
    ))
    .expect("the skill is readable")
}

/// The lines of the skill's code blocks that are not comments.
fn code_block_lines(skill: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut in_block = false;
    for line in skill.lines() {
        if line.trim_start().starts_with("```") {
            in_block = !in_block;
        } else if in_block && !line.trim_start().starts_with('#') {
            lines.push(line);
        }
    }
    lines
}

/// Every `sirioctl` occurrence on those lines: the parse must find each one, so
/// `$(sirioctl …)` counts once per occurrence.
fn sirioctl_occurrences(skill: &str) -> usize {
    code_block_lines(skill)
        .iter()
        .map(|line| line.matches("sirioctl").count())
        .sum()
}

/// The arguments of each `sirioctl` command the skill's code blocks show, without
/// the program name. A command starts at each `sirioctl` word, so `$(sirioctl …)`
/// and `trap '…'` count; it ends at an unquoted `|`, `;`, `&` or `)`, or at the
/// quote that encloses it.
fn skill_commands(skill: &str) -> Vec<Vec<String>> {
    let mut commands = Vec::new();
    for line in code_block_lines(skill) {
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

/// Runs the built `sirioctl` with `args`, pointed at `socket`, in a pane's environment.
fn run_sirioctl<S: AsRef<str>>(args: &[S], socket: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sirioctl"))
        .args(args.iter().map(AsRef::as_ref))
        .env("SIRIO_PANE_ID", "pane-placeholder")
        .env("SIRIO_WORKTREE_ID", "worktree-placeholder")
        .env("SIRIO_SOCKET", socket)
        .output()
        .expect("sirioctl runs")
}

/// The request a leading `--dry-run` of `words` prints: exit 0, and exactly one
/// JSON line.
fn dry_run_request<S: AsRef<str>>(words: &[S], socket: &str) -> serde_json::Value {
    let mut args = vec!["--dry-run"];
    args.extend(words.iter().map(AsRef::as_ref));
    let output = run_sirioctl(&args, socket);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "`sirioctl {}` exited {:?}: {}",
        args.join(" "),
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        stdout.lines().count(),
        1,
        "`sirioctl {}` must print exactly one line, got: {stdout}",
        args.join(" ")
    );
    serde_json::from_str(stdout.trim()).expect("the dry run prints one JSON request")
}

/// The method a leading `--dry-run` of `words` names.
fn dry_run_method<S: AsRef<str>>(words: &[S], socket: &str) -> String {
    dry_run_request(words, socket)["method"]
        .as_str()
        .expect("the request names a method")
        .to_string()
}

#[test]
fn every_command_the_skill_names_is_served_by_a_release_build() {
    let skill = read_skill();
    let commands = skill_commands(&skill);
    let occurrences = sirioctl_occurrences(&skill);
    assert!(
        occurrences > 0,
        "the skill's code blocks show no sirioctl line, so the check would pass on nothing"
    );
    assert_eq!(
        commands.len(),
        occurrences,
        "the parse must find every sirioctl occurrence on the skill's code-block lines"
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

/// `surface trace read` reads only, so a release build serves it, and what
/// `sirioctl` sends names the chat it reads.
#[test]
fn the_trace_read_verb_is_served_by_a_release_build_and_names_its_chat() {
    let request = dry_run_request(
        &["surface", "trace", "read", "chat-7"],
        "/nonexistent/sirio-trace-read.sock",
    );
    assert_eq!(request["method"], "surface.trace.read");
    assert_eq!(request["params"]["surfaceId"], "chat-7");
    assert!(served_methods(false).contains(&"surface.trace.read"));
}

#[test]
fn dry_run_contacts_nothing() {
    let socket = "/nonexistent/sirio-dry-run.sock";
    assert_eq!(dry_run_method(&["ping"], socket), "system.ping");
    assert_eq!(
        dry_run_method(&["panel", "create", "--cmd", "codex"], socket),
        "panel.create"
    );
    // Both of these are sends: `version` asks the app for its capabilities, and
    // `session-ref` reports to the pane it runs in. Each passes the gate.
    assert_eq!(dry_run_method(&["version"], socket), "system.capabilities");
    assert_eq!(
        dry_run_method(&["session-ref", "--ref", "r"], socket),
        "session.ref"
    );
}

/// `--dry-run` after the command is an option, read like any other, so it does not
/// move the values around it. Where `--input` wants its value, the dry run finds
/// none and previews nothing, as if `hello` were the input.
#[test]
fn dry_run_is_an_option_not_a_value_shift() {
    let socket = "/nonexistent/sirio-option-shift.sock";
    let shifted = run_sirioctl(
        &[
            "panel",
            "write",
            "--id",
            "X",
            "--input",
            "--dry-run",
            "hello",
        ],
        socket,
    );
    assert_eq!(
        shifted.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&shifted.stderr)
    );
    assert!(
        shifted.stdout.is_empty(),
        "nothing may be previewed: {}",
        String::from_utf8_lossy(&shifted.stdout)
    );
    assert_eq!(
        String::from_utf8_lossy(&shifted.stderr),
        "sirioctl: Missing required option '--input'\n"
    );

    // After its value, the flag works as it always has: the request is previewed.
    let trailing = run_sirioctl(
        &["panel", "write", "--id", "X", "--input", "hi", "--dry-run"],
        socket,
    );
    assert!(
        trailing.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&trailing.stderr)
    );
    let request: serde_json::Value =
        serde_json::from_slice(&trailing.stdout).expect("one JSON request");
    assert_eq!(request["method"], "panel.write");
    assert_eq!(request["params"]["input"], "hi");
}

/// The first-column names of the skill's `not_supported` table: every backquoted
/// name in its rows, from the table's heading to the next `## ` section.
fn not_supported_table(skill: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_table = false;
    for line in skill.lines() {
        if line.starts_with("### `not_supported` table") {
            in_table = true;
        } else if in_table && line.starts_with("## ") {
            break;
        } else if in_table && line.starts_with('|') {
            let first_cell = line.split('|').nth(1).unwrap_or_default();
            names.extend(first_cell.split('`').skip(1).step_by(2).map(str::to_string));
        }
    }
    names
}

/// The skill's `not_supported` table names exactly the browser actions that
/// `sirioctl` refuses, and each one is refused as `not_supported` with no socket.
#[test]
fn the_not_supported_table_is_what_sirioctl_refuses() {
    let mut table = not_supported_table(&read_skill());
    let mut refused: Vec<String> = BROWSER_NOT_SUPPORTED
        .iter()
        .map(|verb| verb.to_string())
        .collect();
    table.sort();
    refused.sort();
    assert_eq!(
        table, refused,
        "the skill's not_supported table and BROWSER_NOT_SUPPORTED must name the same actions"
    );

    let socket = "/nonexistent/sirio-not-supported.sock";
    for verb in BROWSER_NOT_SUPPORTED.iter().copied() {
        let output = run_sirioctl(&["browser", verb, "placeholder"], socket);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(1), "browser {verb}: {stderr}");
        assert!(
            stderr.starts_with(&format!("sirioctl: not_supported: browser.{verb}")),
            "browser {verb} must be refused as not_supported: {stderr}"
        );
    }
}
