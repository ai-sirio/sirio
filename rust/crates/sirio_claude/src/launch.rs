//! How `claude` is invoked.
//!
//! Two callers, two lines. The chat wants a long-lived session that loads
//! the user's own settings — the same hooks, MCP servers, plugins and
//! skills the terminal pane gets. The usage probe wants the opposite: a
//! process that answers one question and leaves no trace.

/// A program invocation: arguments after the executable, and environment
/// variables to set on the child.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LaunchLine {
    /// Arguments, passed without shell parsing.
    pub args: Vec<String>,
    /// Environment variables to set, on top of the inherited environment.
    pub env: Vec<(String, String)>,
}

/// The flags every stream-json session needs, chat or probe.
const STREAM_JSON: [&str; 6] = [
    "-p",
    "--output-format",
    "stream-json",
    "--input-format",
    "stream-json",
    "--verbose",
];

/// Which session a chat launch is about.
///
/// The two are mutually exclusive on the wire — `--resume` already names a
/// session, and `--session-id` beside it would be two answers to one
/// question — so they are one enum rather than two options.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChatSession<'a> {
    /// A session that does not exist yet, under an id the caller chose.
    /// Naming it up front is what lets a tab be resumed even if the CLI
    /// dies before it ever writes an `init` line.
    New(&'a str),
    /// A session the CLI already wrote, to be continued.
    Resume(&'a str),
}

impl LaunchLine {
    /// The chat session.
    #[must_use]
    pub fn chat(session: ChatSession<'_>) -> Self {
        let mut args: Vec<String> = STREAM_JSON.iter().map(|arg| (*arg).to_string()).collect();
        // Route every permission decision to this process over stdio,
        // rather than to a terminal prompt nobody is watching.
        args.push("--permission-prompt-tool".into());
        args.push("stdio".into());
        // Token-level deltas: without this the reply arrives in one lump at
        // the end of the turn.
        args.push("--include-partial-messages".into());
        match session {
            ChatSession::Resume(session_id) => {
                args.push("--resume".into());
                args.push(session_id.to_string());
            }
            // Naming the session before it exists is what makes a tab
            // resumable from the first keystroke: without it the id is
            // only learned from the `init` line, and a CLI that dies
            // before writing one takes the conversation with it.
            ChatSession::New(session_id) => {
                args.push("--session-id".into());
                args.push(session_id.to_string());
            }
        }
        Self {
            args,
            // Checkpointing is what makes `rewind_files` able to restore
            // anything; without it the CLI has no backups to restore from.
            env: vec![(
                "CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING".to_string(),
                "true".to_string(),
            )],
        }
    }

    /// The one-shot usage probe: handshake, ask, exit.
    ///
    /// It writes no session file and loads no settings, so running it every
    /// few minutes in the background cannot alter the user's own state or
    /// start anything their config would otherwise start.
    #[must_use]
    pub fn usage_probe() -> Self {
        let mut args: Vec<String> = STREAM_JSON.iter().map(|arg| (*arg).to_string()).collect();
        args.push("--no-session-persistence".into());
        args.push("--setting-sources".into());
        args.push(String::new());
        Self {
            args,
            env: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SESSION: &str = "5bbcaeb8-e523-4087-b33c-559163f2dc07";

    #[test]
    fn a_chat_launch_is_the_six_flags_and_one_environment_variable() {
        let line = LaunchLine::chat(ChatSession::New(SESSION));
        assert_eq!(
            line.args,
            [
                "-p",
                "--output-format",
                "stream-json",
                "--input-format",
                "stream-json",
                "--verbose",
                "--permission-prompt-tool",
                "stdio",
                "--include-partial-messages",
                "--session-id",
                SESSION,
            ]
        );
        assert_eq!(
            line.env,
            [(
                "CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING".to_string(),
                "true".to_string()
            )]
        );
    }

    #[test]
    fn a_resumed_chat_appends_the_session_id() {
        let line = LaunchLine::chat(ChatSession::Resume("5bbcaeb8-e523-4087-b33c-559163f2dc07"));
        assert_eq!(
            &line.args[line.args.len() - 2..],
            ["--resume", "5bbcaeb8-e523-4087-b33c-559163f2dc07"]
        );
        // `--resume` is the whole answer: a `--session-id` beside it would
        // name a second session the CLI has to choose between.
        assert!(!line.args.iter().any(|arg| arg == "--session-id"));
    }

    #[test]
    fn the_usage_probe_writes_nothing_to_the_users_session_store() {
        let line = LaunchLine::usage_probe();
        // These two are what make the probe inert: no session file, and none
        // of the user's settings, hooks or MCP servers loaded.
        assert!(
            line.args
                .iter()
                .any(|arg| arg == "--no-session-persistence")
        );
        let sources = line
            .args
            .windows(2)
            .find(|pair| pair[0] == "--setting-sources")
            .expect("the probe pins its setting sources");
        assert_eq!(sources[1], "");
        // It never asks for checkpointing: it makes no edits to rewind.
        assert!(line.env.is_empty());
        assert!(!line.args.iter().any(|arg| arg == "--resume"));
    }
}
