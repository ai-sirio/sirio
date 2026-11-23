//! What Retry does on an error row (spec §5.3): resend the prompt that failed, resume the
//! agent's conversation and then resend it, put the prompt back in the composer, or only
//! relaunch.

use super::ErrorKind;
use sirio_chat::ImageAttachment;

/// The prompt of the turn in flight, kept in memory (never persisted) so that Retry can send it
/// again.
///
/// `text`, `mention_paths` and `images` are what went on the wire. `draft` and `mentions` are
/// what refills the composer: `assemble_prompt` strips the accepted `@path` tokens from `text`,
/// so the raw draft is kept beside it.
#[derive(Clone, Debug)]
pub(crate) struct LastPrompt {
    pub(crate) text: String,
    pub(crate) mention_paths: Vec<String>,
    pub(crate) images: Vec<ImageAttachment>,
    pub(crate) draft: String,
    pub(crate) mentions: Vec<String>,
}

/// What a row's Retry does now.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RetryAction {
    /// The agent is up and still has the conversation: send the last prompt on the same client.
    Resend,
    /// The client is gone but a relaunch resumes the conversation: relaunch with `--resume`, then
    /// send the last prompt.
    ResumeThenResend,
    /// The client is gone and a relaunch starts a new conversation: relaunch, then put the last
    /// prompt back in the composer.
    RelaunchToComposer,
    /// Only start a new process, as Retry and Restart agent always did.
    Relaunch,
    /// The row has no Retry.
    None,
}

impl RetryAction {
    /// The socket's spelling (`surface.chat.read`, an error row's `retry`). Debug builds only,
    /// like the readout.
    #[cfg(debug_assertions)]
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::Resend => "resend",
            Self::ResumeThenResend => "resume",
            Self::RelaunchToComposer => "composer",
            Self::Relaunch => "relaunch",
            Self::None => "none",
        }
    }
}

/// What the relaunch of a Retry does once the new process is connected.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AfterConnect {
    /// Send the last prompt, if the conversation came back: the launch asked to resume `session`.
    Resend { session: String },
    /// Put the last prompt back in the composer: the conversation cannot come back.
    Composer,
}

/// The line under a queue held after a failed turn.
pub(crate) const QUEUE_HELD_LINE: &str = "Waiting: press Retry or send a message.";

/// The same line once no Retry is left (the prompt went back to the composer, or the failed
/// turn's row was retired or dismissed).
pub(crate) const QUEUE_HELD_LINE_NO_RETRY: &str = "Waiting: send a message.";

/// The first notice of a refill: the composer was empty and got the message back.
pub(crate) const RESTARTED_MESSAGE_BACK: &str =
    "The agent restarted without this conversation. Your message is back in the composer.";

/// The second: the composer held something, so nothing was put back.
pub(crate) const RESTARTED: &str = "The agent restarted without this conversation.";

/// What Retry does on a row of `kind`.
///
/// - `has_last_prompt`: a prompt is held, so a turn was in flight or failed;
/// - `resumes_on_retry`: a relaunch of this agent continues its conversation
///   (`LaunchSpec::resumes_on_retry`);
/// - `session_known`: the chat holds the agent's session id;
/// - `retryable`: the row carries a Retry button.
pub(crate) fn retry_action(
    kind: ErrorKind,
    has_last_prompt: bool,
    resumes_on_retry: bool,
    session_known: bool,
    retryable: bool,
) -> RetryAction {
    if !retryable {
        return RetryAction::None;
    }
    match kind {
        // Informational and unavailable rows have no Retry; neither is ever `retryable`.
        ErrorKind::McpWarning | ErrorKind::Unavailable => RetryAction::None,
        // "Sign in out of band, then retry": a new process, whatever was in flight.
        ErrorKind::AuthRequired => RetryAction::Relaunch,
        // The agent is up and has the conversation. With no prompt held there is nothing to send
        // (a turn end taken as the prompt's cleared it), and relaunching would only kill a working
        // process and resend nothing: the row has no Retry (spec §5.3, final review).
        ErrorKind::TurnFailed if has_last_prompt => RetryAction::Resend,
        ErrorKind::TurnFailed => RetryAction::None,
        // The client is gone.
        ErrorKind::Connection | ErrorKind::Disconnected => {
            if !has_last_prompt {
                RetryAction::Relaunch
            } else if resumes_on_retry && session_known {
                RetryAction::ResumeThenResend
            } else {
                RetryAction::RelaunchToComposer
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One assert per row of the table in spec §5.3, then the kinds the spec leaves unchanged.
    #[test]
    fn retry_action_follows_the_spec_table() {
        // A failed turn, the agent still up: send the same prompt on the same client.
        assert_eq!(
            retry_action(ErrorKind::TurnFailed, true, false, false, true),
            RetryAction::Resend
        );
        // The client was lost mid-turn, the launch resumes, a session id is known.
        assert_eq!(
            retry_action(ErrorKind::Connection, true, true, true, true),
            RetryAction::ResumeThenResend
        );
        // The same, but there is no session id to resume.
        assert_eq!(
            retry_action(ErrorKind::Disconnected, true, true, false, true),
            RetryAction::RelaunchToComposer
        );
        // The same, but the launch cannot resume (ACP).
        assert_eq!(
            retry_action(ErrorKind::Connection, true, false, true, true),
            RetryAction::RelaunchToComposer
        );
        // Nothing was in flight (an idle death, a restored error, a failed launch).
        assert_eq!(
            retry_action(ErrorKind::Disconnected, false, true, true, true),
            RetryAction::Relaunch
        );
        // A failed turn, the agent up, and no prompt to resend (a turn end taken as the prompt's
        // cleared it): relaunching would only kill a working process, so there is no Retry.
        assert_eq!(
            retry_action(ErrorKind::TurnFailed, false, true, true, true),
            RetryAction::None
        );
        // A row with no button.
        assert_eq!(
            retry_action(ErrorKind::Unavailable, true, true, true, false),
            RetryAction::None
        );
    }

    /// Today's Retry on these rows is a plain relaunch (`AuthRequired`: "sign in out of band,
    /// then retry") or nothing (`McpWarning` and `Unavailable` are drawn with no button): a
    /// prompt held, a resumable launch and a known session change neither.
    #[test]
    fn auth_required_and_the_informational_rows_keep_todays_buttons() {
        for has_last_prompt in [false, true] {
            for resumes_on_retry in [false, true] {
                for session_known in [false, true] {
                    assert_eq!(
                        retry_action(
                            ErrorKind::AuthRequired,
                            has_last_prompt,
                            resumes_on_retry,
                            session_known,
                            true
                        ),
                        RetryAction::Relaunch,
                        "AuthRequired relaunches: {has_last_prompt} {resumes_on_retry} {session_known}"
                    );
                    for kind in [ErrorKind::McpWarning, ErrorKind::Unavailable] {
                        assert_eq!(
                            retry_action(
                                kind,
                                has_last_prompt,
                                resumes_on_retry,
                                session_known,
                                false
                            ),
                            RetryAction::None,
                            "{kind:?} has no Retry: {has_last_prompt} {resumes_on_retry} {session_known}"
                        );
                    }
                }
            }
        }
    }

    /// A button that is not drawn does nothing, whatever the kind says.
    #[test]
    fn a_row_without_a_retry_button_has_no_action() {
        for kind in [
            ErrorKind::TurnFailed,
            ErrorKind::Connection,
            ErrorKind::Disconnected,
            ErrorKind::AuthRequired,
        ] {
            assert_eq!(
                retry_action(kind, true, true, true, false),
                RetryAction::None,
                "{kind:?} without a button"
            );
        }
    }
}
