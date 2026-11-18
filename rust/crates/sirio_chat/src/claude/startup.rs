//! What the user reads when the CLI could not start.

use sirio_claude::StartupFailure;

/// The sentence for a reason this build knows.
fn sentence(reason: &str) -> Option<&'static str> {
    Some(match reason {
        "org_pin_api_key_conflict" => "Claude Code could not start: your organization requires its own sign-in, but an API key or token is also set.",
        "provider_not_allowed" => "Claude Code could not start: your organization's settings do not allow this provider.",
        "org_verify_failed" => "Claude Code could not start: it could not verify your sign-in's organization. Check the network, or sign in again.",
        "org_pin_mismatch" => "Claude Code could not start: this sign-in belongs to an organization your settings do not allow.",
        "managed_settings_invalid" => "Claude Code could not start: your organization's managed settings are invalid.",
        "remote_settings_required_unavailable" => "Claude Code could not start: required managed settings could not be loaded.",
        "gateway_signin_required" => "Claude Code could not start: the gateway ended this sign-in. Sign in again.",
        "gateway_access_denied" => "Claude Code could not start: the gateway refused this account.",
        "proxy_invalid" => "Claude Code could not start: a proxy setting is not a complete URL.",
        "temp_dir_unusable" => "Claude Code could not start: its temporary directory cannot be used.",
        "cwd_unavailable" => "Claude Code could not start: this folder no longer exists or cannot be read.",
        "shell_tool_missing" => "Claude Code could not start: it found no shell to run commands with.",
        "session_held_by_background" => "Claude Code could not resume this conversation: it is running as a background session.",
        "worktree_resume_refused" => "Claude Code could not resume this conversation: its worktree failed a safety check.",
        "worktree_unverified" => "Claude Code could not resume this conversation: its worktree could not be verified right now. Retry may work.",
        "cli_version_too_old" => "Claude Code could not start: this version is too old. Update Claude Code.",
        "bypass_root" => "Claude Code could not start: it does not allow bypassing permissions when running as root.",
        "org_config_required_unavailable" => "Claude Code could not start: your organization's required settings could not be loaded.",
        "org_config_refused" => "Claude Code could not start: Anthropic refused your organization's settings for this sign-in.",
        "sandbox_required_unavailable" => "Claude Code could not start: a required sandbox cannot run on this machine.",
        "initialize_not_first" => "Claude Code could not start: it did not receive Sirio's handshake first.",
        "config_file_invalid" => "Claude Code could not start: a configuration file is not valid JSON.",
        "managed_settings_not_delivered" => "Claude Code could not start: it expected managed settings in Sirio's handshake.",
        _ => return None,
    })
}

/// The whole message: the reason's sentence, the CLI's own lines, and the
/// configuration file it named. Raw text; the surface sanitizes it where it
/// draws it.
pub(crate) fn startup_text(failure: &StartupFailure) -> String {
    let mut parts: Vec<String> = Vec::new();
    match failure.reason.as_deref() {
        Some(reason) => parts.push(sentence(reason).map_or_else(
            || format!("Claude Code could not start ({reason})."),
            str::to_string,
        )),
        None => parts.push("Claude Code could not start.".to_string()),
    }
    if !failure.errors.is_empty() {
        parts.push(failure.errors.join("\n"));
    }
    if let Some(file) = &failure.invalid_config_file {
        let mut lines = format!("File: {}", file.path);
        if let Some(backup) = &file.backup_path {
            lines.push_str(&format!("\nBackup: {backup}"));
        }
        parts.push(lines);
    }
    parts.join("\n\n")
}
