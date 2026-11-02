//! Which agent, if any, a terminal's foreground program is.
//!
//! The failure list this unit was written against (every line is an assertion
//! below). The process `name` is `""` unless shown.
//!
//! Resolves:
//! - `["claude"]`, `["/home/u/.local/bin/claude","--resume"]`, `["codex.exe"]`
//! - `node` running a package's script, `npx`/`bunx`/`pnpm dlx`/`uv run`
//!   launching one, a shell `-c` wrapper (with and without `"$@"`), a
//!   `node_modules/.bin` shim
//! - `mise x`/`mise exec` followed by `--` and the program (every agent on
//!   a mise machine is a wrapper that execs through it, so the host can read
//!   `mise` for a moment)
//! - `(["node","/opt/x/cli.js"], "claude")`: node's process title
//!
//! Refuses:
//! - `[]`, a login shell, an editor with `claude.md` open, a script that only
//!   mentions an agent as an argument, a renamed binary (its identity comes
//!   from the report's `app=`), a runtime with no script, an empty `sh -c`,
//!   and `mise use -g claude` (it installs, it does not run)

use crate::model::CATALOG_IDS;

/// How many wrappers (a shell, a runtime, a launcher, `mise x`) are peeled off
/// before the word that is left is judged.
const MAX_UNWRAPS: usize = 4;

const SHELLS: [&str; 6] = ["sh", "bash", "zsh", "fish", "dash", "ksh"];
const RUNTIMES: [&str; 5] = ["node", "bun", "deno", "python", "python3"];
const LAUNCHERS: [&str; 6] = ["npx", "npx-cli", "npm-cli", "bunx", "pnpx", "uvx"];
/// Package managers that launch a program through one of `LAUNCHER_VERBS`.
const MANAGERS: [&str; 5] = ["npm", "pnpm", "bun", "yarn", "uv"];
const LAUNCHER_VERBS: [&str; 4] = ["exec", "x", "dlx", "run"];
const SCRIPT_EXTENSIONS: [&str; 5] = [".js", ".mjs", ".cjs", ".ts", ".py"];

/// Registry packages that install an agent. omp is a native binary.
const PACKAGES: [(&str, &str); 4] = [
    ("@anthropic-ai/claude-code", "claude"),
    ("@openai/codex", "codex"),
    ("opencode-ai", "opencode"),
    ("@earendil-works/pi-coding-agent", "pi"),
];

/// The catalog agent a terminal's foreground program is, from its argv and its
/// process `name`. The name is tried only when the argv gives nothing.
pub fn agent_for_program(argv: &[String], name: &str) -> Option<&'static str> {
    resolve_argv(argv).or_else(|| catalog_id(base(name)))
}

/// The catalog agent an OSC 7501 report's `app=` field names.
pub fn agent_for_app(app: &str) -> Option<&'static str> {
    match app.trim().to_ascii_lowercase().as_str() {
        "claude-code" | "claude" => Some("claude"),
        "codex" => Some("codex"),
        "opencode" => Some("opencode"),
        "pi" => Some("pi"),
        "omp" | "oh-my-pi" => Some("omp"),
        _ => None,
    }
}

fn resolve_argv(argv: &[String]) -> Option<&'static str> {
    let mut words: Vec<String> = argv.to_vec();
    for unwrapped in 0..=MAX_UNWRAPS {
        if let Some(agent) = judge(words.first()?) {
            return Some(agent);
        }
        if unwrapped == MAX_UNWRAPS {
            return None;
        }
        words = unwrap_once(&words)?;
    }
    None
}

/// Whether one word is an agent: its file name, the package it lives in under
/// `node_modules/`, its script name, or a package spec.
fn judge(word: &str) -> Option<&'static str> {
    if let Some(agent) = catalog_id(base(word)) {
        return Some(agent);
    }
    let slashed = word.replace('\\', "/");
    if let Some((_, inside)) = slashed.rsplit_once("node_modules/") {
        return package_agent(package_name(inside));
    }
    catalog_id(stem(base(word))).or_else(|| package_agent(strip_version(word)))
}

/// Peels one wrapper off `words`: the words that are left to judge, or `None`
/// when `words[0]` is not a wrapper (or wraps nothing).
fn unwrap_once(words: &[String]) -> Option<Vec<String>> {
    let head = base(words.first()?);
    let rest = &words[1..];
    if SHELLS.contains(&head) {
        return unwrap_shell(words);
    }
    if head == "mise" && matches!(rest.first().map(String::as_str), Some("x" | "exec")) {
        // `mise x tool@v -- program args`: what follows the first `--` runs.
        let after = rest.iter().position(|w| w == "--")? + 1;
        return Some(rest[after..].to_vec());
    }
    // Before the runtimes: `bun` is both, and `bun x pkg` launches a package.
    if let Some(skip) = launcher_prefix(head, rest) {
        return Some(skip_flags(&rest[skip..]).to_vec());
    }
    if RUNTIMES.contains(&head) {
        return unwrap_runtime(head, rest);
    }
    None
}

/// `sh -c TEXT [$0 args...]`. Only the flag right after the shell is looked at.
fn unwrap_shell(words: &[String]) -> Option<Vec<String>> {
    let flag = words.get(1)?;
    let is_command_flag = flag.starts_with('-')
        && !flag.starts_with("--")
        && flag.ends_with('c')
        && flag.chars().skip(1).all(|c| c.is_ascii_alphabetic());
    if !is_command_flag {
        return None;
    }
    let text = words.get(2)?;
    if text.contains("\"$@\"") {
        // The wrapper runs its own arguments: `sh -c '"$@"' $0 prog args`.
        return Some(words.get(4..)?.to_vec());
    }
    let command: Vec<String> = text
        .split_whitespace()
        .skip_while(|w| *w == "exec" || is_assignment(w))
        .map(str::to_string)
        .collect();
    Some(command)
}

/// `node [flags] script`, `deno run [flags] script`, `python -m module`.
fn unwrap_runtime(runtime: &str, rest: &[String]) -> Option<Vec<String>> {
    let python = runtime.starts_with("python");
    let mut i = 0;
    while let Some(word) = rest.get(i) {
        if python && word == "-m" {
            return Some(rest.get(i + 1..)?.to_vec());
        }
        if word.starts_with('-') || (runtime == "deno" && word == "run") {
            i += 1;
        } else {
            break;
        }
    }
    Some(rest[i..].to_vec())
}

/// How many words after the launcher name belong to it, or `None` when
/// `head` does not launch anything.
fn launcher_prefix(head: &str, rest: &[String]) -> Option<usize> {
    let head = stem(head);
    if LAUNCHERS.contains(&head) {
        return Some(0);
    }
    let verb = rest.first()?;
    (MANAGERS.contains(&head) && LAUNCHER_VERBS.contains(&verb.as_str())).then_some(1)
}

fn skip_flags(words: &[String]) -> &[String] {
    let flags = words.iter().take_while(|w| w.starts_with('-')).count();
    &words[flags..]
}

/// The file name after the last `/` or `\`, minus a leading `-` (a login
/// shell's argv[0]) and a trailing `.exe` in any case.
fn base(word: &str) -> &str {
    let name = word.rsplit(['/', '\\']).next().unwrap_or(word);
    let name = name.strip_prefix('-').unwrap_or(name);
    match name.len().checked_sub(4) {
        Some(at) if name.is_char_boundary(at) && name[at..].eq_ignore_ascii_case(".exe") => {
            &name[..at]
        }
        _ => name,
    }
}

fn stem(name: &str) -> &str {
    SCRIPT_EXTENSIONS
        .iter()
        .find_map(|ext| name.strip_suffix(ext))
        .unwrap_or(name)
}

/// `@scope/name/rest` or `name/rest` to the package's own name.
fn package_name(path: &str) -> &str {
    let segments = if path.starts_with('@') { 2 } else { 1 };
    let mut end = path.len();
    for (n, (at, _)) in path.match_indices('/').enumerate() {
        if n + 1 == segments {
            end = at;
            break;
        }
    }
    &path[..end]
}

/// A package spec without its trailing `@version`.
fn strip_version(spec: &str) -> &str {
    match spec.rfind('@') {
        Some(at) if at > 0 => &spec[..at],
        _ => spec,
    }
}

fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty()
            && !name.starts_with(|c: char| c.is_ascii_digit())
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

fn catalog_id(name: &str) -> Option<&'static str> {
    CATALOG_IDS.iter().copied().find(|id| *id == name)
}

fn package_agent(package: &str) -> Option<&'static str> {
    PACKAGES
        .iter()
        .find_map(|(name, agent)| (*name == package).then_some(*agent))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn resolves_every_launcher_form() {
        let cases: &[(&[&str], &str, &str)] = &[
            (&["claude"], "", "claude"),
            (&["/home/u/.local/bin/claude", "--resume"], "", "claude"),
            (&["codex.exe"], "", "codex"),
            (
                &[
                    "node",
                    "/usr/lib/node_modules/@anthropic-ai/claude-code/cli.js",
                ],
                "",
                "claude",
            ),
            (
                &[
                    "node",
                    "/x/lib/node_modules/npm/bin/npx-cli.js",
                    "@anthropic-ai/claude-code@latest",
                ],
                "",
                "claude",
            ),
            (&["npx", "-y", "@openai/codex"], "", "codex"),
            (&["bunx", "opencode-ai"], "", "opencode"),
            (
                &["pnpm", "dlx", "@earendil-works/pi-coding-agent"],
                "",
                "pi",
            ),
            (
                &[
                    "/bin/zsh",
                    "-ic",
                    "\"$@\"; exec \"$0\" -i",
                    "/bin/zsh",
                    "omp",
                ],
                "",
                "omp",
            ),
            (&["sh", "-c", "exec codex --yolo"], "", "codex"),
            (&["bash", "-c", "FOO=1 claude"], "", "claude"),
            (&["node", "/x/node_modules/.bin/codex"], "", "codex"),
            (&["uv", "run", "claude"], "", "claude"),
            // node's process title is the program's own name.
            (&["node", "/opt/x/cli.js"], "claude", "claude"),
            // mise execs the program in place, so for a moment it is `mise`.
            (&["mise", "x", "pi", "--", "pi"], "", "pi"),
            (
                &["mise", "exec", "node@22", "--", "claude", "--resume"],
                "",
                "claude",
            ),
        ];
        for (words, name, expected) in cases {
            let words = argv(words);
            assert_eq!(
                agent_for_program(&words, name),
                Some(*expected),
                "{words:?} (name {name:?})"
            );
        }
    }

    #[test]
    fn refuses_what_is_not_an_agent() {
        let cases: &[(&[&str], &str)] = &[
            (&[], ""),
            (&["-zsh"], ""),
            (&["vim", "claude.md"], ""),
            (&["node", "server.js", "claude"], ""),
            // A renamed binary: identity then comes from `app=`.
            (&["mycc"], "mycc"),
            (&["python3", "-m", "http.server"], ""),
            (&["sh", "-c", ""], ""),
            (&["node"], ""),
            (&["mise", "use", "-g", "claude"], ""),
        ];
        for (words, name) in cases {
            let words = argv(words);
            assert_eq!(
                agent_for_program(&words, name),
                None,
                "{words:?} (name {name:?})"
            );
        }
    }

    #[test]
    fn app_field_names_an_agent_only_when_it_is_one() {
        assert_eq!(agent_for_app("Claude-Code "), Some("claude"));
        assert_eq!(agent_for_app("aider"), None);
    }
}
