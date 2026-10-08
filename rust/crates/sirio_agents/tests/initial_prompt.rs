//! The launch prompt reaches every CLI as one argument, byte for byte,
//! through `sh -c`, which stands in for the POSIX shell (`$SHELL -lc`) a
//! pane runs its command in.
#![cfg(unix)]

use std::process::Command;

use sirio_agents::ALL;

const PROMPT: &str = "Read the file .sirio/handoff/101-comments-20261008-101500.md.\nIt's \"quoted\", costs $HOME and `ticks`.";

/// The arguments the CLI would have received, as the shell parsed them.
fn argv_of(command: &str) -> Vec<String> {
    // One NUL per argument: `printf '%s\0' "$@"` alone would print one empty
    // field for a command with no arguments.
    let shim = "() { for a in \"$@\"; do printf '%s\\0' \"$a\"; done; }";
    let shims = format!(
        "claude{shim}; codex{shim}; opencode{shim}; pi{shim}; omp{shim};"
    );
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!("{shims} {command}"))
        .output()
        .expect("sh runs");
    assert!(output.status.success(), "the command did not parse: {command}");
    let text = String::from_utf8(output.stdout).expect("utf-8");
    if text.is_empty() {
        return Vec::new();
    }
    // Every argument is printed with its NUL terminator, so drop exactly the
    // last one; an empty argument in the middle is still an argument.
    text.strip_suffix('\0')
        .expect("every argument ends in NUL")
        .split('\0')
        .map(str::to_string)
        .collect()
}

#[test]
fn every_adapter_passes_the_prompt_as_one_argument() {
    for adapter in ALL {
        let command = adapter.command_with_prompt("/w t", "pane-7", "/bin/sirioctl", PROMPT);
        let argv = argv_of(&command);
        assert_eq!(argv.last().map(String::as_str), Some(PROMPT), "{}: {command}", adapter.id());
        assert_eq!(argv.iter().filter(|arg| arg.as_str() == PROMPT).count(), 1, "{}", adapter.id());
    }
}

#[test]
fn the_prompt_follows_the_adapters_own_launch_arguments() {
    for adapter in ALL {
        let plain = argv_of(&adapter.command("/w t", "pane-7", "/bin/sirioctl"));
        let prompted = argv_of(&adapter.command_with_prompt("/w t", "pane-7", "/bin/sirioctl", PROMPT));
        assert!(prompted.starts_with(&plain), "{} keeps its hooks and overrides: {prompted:?}", adapter.id());
    }
}
