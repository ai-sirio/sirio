//! The Codex adapter, ported from `SirioAgents/CodexAdapter.swift`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use toml_edit::DocumentMut;

use crate::error::PrepareError;
use crate::shell_quote::{json_string_literal, shell_quote};
use crate::{GlobalHookInstall, write_atomic};

/// Adapter for OpenAI's Codex CLI.
///
/// `prepare` is a no-op because Codex configuration is global — and this
/// crate never touches it on a launch. Hook behaviour is delivered via the
/// `-c` CLI override in the command string instead. Only the explicit
/// Settings → Install Hooks action (`install_global_hooks`) writes the
/// same `notify` argv into the user's `config.toml`.
pub struct CodexAdapter;

/// Codex's user config file under `~/.codex` (or `$CODEX_HOME`).
const CONFIG_FILE_NAME: &str = "config.toml";

impl super::AgentAdapter for CodexAdapter {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn display_name(&self) -> &'static str {
        "Codex"
    }

    fn has_native_hooks(&self) -> bool {
        true
    }

    fn prepare(
        &self,
        _worktree_path: &str,
        _pane_id: &str,
        _sirioctl_path: &str,
    ) -> Result<(), PrepareError> {
        // Codex config is global; prepare must not touch it, so there is
        // nothing to write.
        Ok(())
    }

    fn install_global_hooks(
        &self,
        home: &Path,
        environment: &BTreeMap<String, String>,
        sirioctl_path: &str,
    ) -> Result<GlobalHookInstall, PrepareError> {
        // `$CODEX_HOME` relocates Codex's config, the same precedence
        // `codex` itself applies (and `sirio_usage` reads for `auth.json`).
        let codex_home = environment
            .get("CODEX_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".codex"));
        std::fs::create_dir_all(&codex_home)?;
        let config_path = codex_home.join(CONFIG_FILE_NAME);

        // The user's config is hand-written TOML — comments, tables, key
        // order all theirs. `toml_edit` sets the one key and keeps the
        // rest; a file that no longer parses is left alone rather than
        // rewritten from what this crate could salvage.
        let mut document = match std::fs::read_to_string(&config_path) {
            Ok(text) => text.parse::<DocumentMut>().map_err(|error| {
                PrepareError::UnparseableUserConfig {
                    path: config_path.clone(),
                    reason: error.to_string(),
                }
            })?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
            Err(error) => return Err(error.into()),
        };

        // Codex runs `notify` as a plain argv (no shell) with the JSON
        // payload appended, so no `--session`: the pane comes from the
        // `SIRIO_PANE_ID` the process inherited, and outside a Sirio pane
        // sirioctl exits silently.
        let mut argv = toml_edit::Array::new();
        for argument in [sirioctl_path, "notify", "--status", "needs-input"] {
            argv.push(argument);
        }
        document["notify"] = toml_edit::value(argv);
        write_atomic(&config_path, document.to_string().as_bytes())?;
        Ok(GlobalHookInstall::Written(config_path))
    }

    fn command(&self, _worktree_path: &str, pane_id: &str, sirioctl_path: &str) -> String {
        format!("codex -c {}", self.notify_override(pane_id, sirioctl_path))
    }

    fn resume_command(
        &self,
        _worktree_path: &str,
        pane_id: &str,
        sirioctl_path: &str,
        session_ref: &str,
    ) -> Option<String> {
        Some(format!(
            "codex -c {} resume {}",
            self.notify_override(pane_id, sirioctl_path),
            shell_quote(session_ref)
        ))
    }

    fn summarizer_command(&self, prompt: &str) -> Option<String> {
        Some(format!(
            "codex exec --output-last-message /dev/stdout {}",
            shell_quote(prompt)
        ))
    }
}

impl CodexAdapter {
    /// Builds the `notify=[...]` override: the sirioctl invocation as a JSON
    /// array of JSON string literals, shell-quoted as a whole.
    ///
    /// Codex parses `-c key=value` overrides as TOML, so the literals MUST
    /// be built with [`json_string_literal`] — slash-escaped `\/` is not a
    /// valid TOML escape and would make Codex fail silently at config load.
    fn notify_override(&self, pane_id: &str, sirioctl_path: &str) -> String {
        let args = [
            sirioctl_path,
            "notify",
            "--session",
            pane_id,
            "--status",
            "needs-input",
        ];
        let json_args = args
            .iter()
            .map(|arg| json_string_literal(arg))
            .collect::<Vec<_>>()
            .join(",");
        shell_quote(&format!("notify=[{json_args}]"))
    }
}
