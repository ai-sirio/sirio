//! The installed CLIs still take an initial prompt the way
//! `command_with_prompt` passes it. Each prints `SKIP:` without its binary
//! and is on the `ci` and `pr` profiles' skip list: what a runner has
//! installed must not decide a gate.

use std::process::Command;

fn help_of(program: &str) -> Option<String> {
    let path = sirio_agents::find_executable_on_path(program)?;
    let output = Command::new(path).arg("--help").output().ok()?;
    Some(format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

fn check(program: &str, needle: &str) {
    let Some(help) = help_of(program) else {
        println!("SKIP: {program} is not on PATH");
        return;
    };
    assert!(help.contains(needle), "{program} --help no longer says {needle:?}:\n{help}");
}

#[test]
fn claude_takes_an_initial_prompt_argument() {
    check("claude", "[prompt]");
}

#[test]
fn codex_takes_an_initial_prompt_argument() {
    check("codex", "[PROMPT]");
}

#[test]
fn opencode_takes_an_initial_prompt_flag() {
    check("opencode", "--prompt");
}

#[test]
fn pi_takes_initial_messages() {
    check("pi", "[messages...]");
}

#[test]
fn omp_takes_an_initial_prompt_argument() {
    check("omp", "Interactive mode with initial prompt");
}
