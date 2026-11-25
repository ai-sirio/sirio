//! What Retry does on an error row (spec §5.3): resend the prompt that failed, resume the
//! agent's conversation and then resend it, put the prompt back in the composer, or only
//! relaunch.

use super::ErrorKind;
use sirio_chat::ImageAttachment;

/// The shell context a prompt carried (spec 5.4): the finished `!` blocks that were pending when
/// it was sent, written in Claude Code's tags, and which blocks they were.
#[derive(Clone, Debug)]
pub(crate) struct ShellCarry {
    /// The blocks, by their id, that went with the prompt: they are marked sent, and a hand-back
    /// puts them back in pending.
    pub(crate) ids: Vec<u64>,
    /// What goes ahead of the prompt's text on the wire, with no trailing newline; one empty line
    /// separates it from the text.
    pub(crate) text: String,
}

/// The prompt of the turn in flight, kept in memory (never persisted) so that Retry can send it
/// again.
///
/// `text`, `mention_paths` and `images` are what went on the wire. `draft` and `mentions` are
/// what refills the composer: `assemble_prompt` strips the accepted `@path` tokens from `text`,
/// so the raw draft is kept beside it. `shell` is the context that went ahead of `text`, kept
/// apart from it so that the user's row, the history and every reader of what was typed stay
/// clean; Retry resends it as it was and collects nothing new.
#[derive(Clone, Debug)]
pub(crate) struct LastPrompt {
    pub(crate) text: String,
    pub(crate) mention_paths: Vec<String>,
    pub(crate) images: Vec<ImageAttachment>,
    pub(crate) draft: String,
    pub(crate) mentions: Vec<String>,
    pub(crate) shell: Option<ShellCarry>,
}

/// The prompt Continue sends.
pub(crate) const CONTINUE_PROMPT: &str = "Continue";

impl LastPrompt {
    /// What a Continue holds as the turn in flight: the word, as sent and as it would refill the
    /// composer, with no mentions and no images.
    pub(crate) fn continue_prompt() -> Self {
        Self {
            text: CONTINUE_PROMPT.to_string(),
            mention_paths: Vec::new(),
            images: Vec::new(),
            draft: CONTINUE_PROMPT.to_string(),
            mentions: Vec::new(),
            shell: None,
        }
    }
}

/// Why a turn stopped short, when Continue can pick it up from there.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContinueReason {
    /// The turn hit the model's token limit.
    TokenLimit,
    /// The turn hit the agent's turn-request limit.
    TurnLimit,
    /// The user pressed Stop.
    Stopped,
}

impl ContinueReason {
    /// The reason for a turn end's stop reason, `None` for every end Continue does not offer. The
    /// strings are the ones `turn_end_label` matches.
    pub(crate) fn from_stop_reason(reason: &str) -> Option<Self> {
        match reason {
            "MaxTokens" => Some(Self::TokenLimit),
            "MaxTurnRequests" => Some(Self::TurnLimit),
            "Cancelled" => Some(Self::Stopped),
            _ => None,
        }
    }
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

/// Why a press of Retry or Continue does nothing, in the words the control socket gives for the
/// causes the two share.
pub(crate) const TURN_RUNNING: &str = "a turn is running";
pub(crate) const NOT_CONNECTED: &str = "the agent is not connected";
pub(crate) const CONNECTING: &str = "the agent is connecting";
pub(crate) const QUEUE_HELD: &str = "the queue is held";

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

/// What a turn end's Continue does now, from Retry's own table (no second one):
/// `retry_action` on the kind of failure the agent's state stands for, with a prompt held and a
/// button drawn.
///
/// - `reason`: why the turn stopped short, `None` for any other end (no Continue);
/// - `client_up`: the agent's process is still connected, so the turn failed as `TurnFailed` does;
///   otherwise the connection is gone, as `Connection` is;
/// - `resumes_on_retry`, `session_known`: as in [`retry_action`].
///
/// A relaunch that would start a new conversation has no Continue: it is `None`, not Retry's
/// `RelaunchToComposer`, because "Continue" means nothing there.
pub(crate) fn continue_action(
    reason: Option<ContinueReason>,
    client_up: bool,
    resumes_on_retry: bool,
    session_known: bool,
) -> RetryAction {
    if reason.is_none() {
        return RetryAction::None;
    }
    let kind = if client_up {
        ErrorKind::TurnFailed
    } else {
        ErrorKind::Connection
    };
    match retry_action(kind, true, resumes_on_retry, session_known, true) {
        RetryAction::RelaunchToComposer => RetryAction::None,
        action => action,
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

    /// Continue and Retry are one table: the agent up resends, a relaunch that resumes a known
    /// session resumes then resends, and every other relaunch has no Continue (it would start a
    /// new conversation, and "Continue" means nothing there).
    #[test]
    fn continue_follows_retrys_table() {
        for reason in [
            ContinueReason::TokenLimit,
            ContinueReason::TurnLimit,
            ContinueReason::Stopped,
        ] {
            for client_up in [false, true] {
                for resumes_on_retry in [false, true] {
                    for session_known in [false, true] {
                        let expected = if client_up {
                            RetryAction::Resend
                        } else if resumes_on_retry && session_known {
                            RetryAction::ResumeThenResend
                        } else {
                            RetryAction::None
                        };
                        assert_eq!(
                            continue_action(
                                Some(reason),
                                client_up,
                                resumes_on_retry,
                                session_known
                            ),
                            expected,
                            "{reason:?}: up {client_up}, resumes {resumes_on_retry}, \
                             known {session_known}"
                        );
                    }
                }
            }
        }
    }

    /// A turn that ended any other way has no Continue, whatever the agent's state.
    #[test]
    fn no_reason_no_continue() {
        for client_up in [false, true] {
            for resumes_on_retry in [false, true] {
                for session_known in [false, true] {
                    assert_eq!(
                        continue_action(None, client_up, resumes_on_retry, session_known),
                        RetryAction::None,
                        "up {client_up}, resumes {resumes_on_retry}, known {session_known}"
                    );
                }
            }
        }
    }

    /// Only the three ends the transcript labels as cut short offer Continue: a finished turn, a
    /// refusal, an error and an unknown end do not.
    #[test]
    fn only_three_ends_continue() {
        assert_eq!(
            ContinueReason::from_stop_reason("MaxTokens"),
            Some(ContinueReason::TokenLimit)
        );
        assert_eq!(
            ContinueReason::from_stop_reason("MaxTurnRequests"),
            Some(ContinueReason::TurnLimit)
        );
        assert_eq!(
            ContinueReason::from_stop_reason("Cancelled"),
            Some(ContinueReason::Stopped)
        );
        for reason in ["EndTurn", "Refusal", "error_during_execution", ""] {
            assert_eq!(
                ContinueReason::from_stop_reason(reason),
                None,
                "{reason:?} does not continue"
            );
        }
    }
}
