//! What the `initialize` response says a session can do.
//!
//! The handshake answers with the whole catalogue at once — commands,
//! models, agents, output styles, the current permission mode and the
//! account — so a chat can populate every picker before the user types.

use serde_json::Value;

/// Commands a desktop chat never offers: either the terminal owns their UX,
/// or they act on state a chat tab does not have. Mirrors the list the
/// official ACP wrapper hides, for the same reasons.
const HIDDEN_COMMANDS: [&str; 8] = [
    "clear",
    "cost",
    "keybindings-help",
    "login",
    "logout",
    "output-style:new",
    "release-notes",
    "todos",
];

/// The permission modes Sirio offers. `bypassPermissions` is deliberately
/// absent: the CLI refuses it without `--allow-dangerously-skip-permissions`,
/// which Sirio never passes, so offering it would offer a button that fails.
const OFFERED_MODES: [(&str, &str, &str); 4] = [
    ("default", "Ask", "Ask before anything irreversible"),
    (
        "acceptEdits",
        "Accept edits",
        "Apply file edits without asking",
    ),
    ("plan", "Plan", "Explore and plan without making changes"),
    ("auto", "Auto", "Let the model decide what needs asking"),
];

/// The effort levels the CLI accepts, plus the reset.
const EFFORT_CHOICES: [(&str, &str); 4] = [
    ("default", "Default"),
    ("low", "Low"),
    ("medium", "Medium"),
    ("high", "High"),
];

/// One slash command the session offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandInfo {
    /// Name without the leading slash.
    pub name: String,
    /// What it does.
    pub description: String,
}

/// One model the session can switch to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelInfo {
    /// The value `set_model` takes.
    pub id: String,
    /// The display name.
    pub name: String,
    /// The CLI's own description, when it gave one.
    pub description: Option<String>,
}

/// One permission mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModeInfo {
    /// The value `set_permission_mode` takes.
    pub id: String,
    /// The display name.
    pub name: String,
    /// What choosing it means.
    pub description: String,
}

/// The mode selector's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Modes {
    /// The mode in force.
    pub current_id: String,
    /// Every mode offered.
    pub options: Vec<ModeInfo>,
}

/// One effort level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffortChoice {
    /// The value sent to the CLI.
    pub value: String,
    /// The display name.
    pub name: String,
}

/// The effort selector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Effort {
    /// The option id the surface keys its selection on.
    pub option_id: String,
    /// The levels offered.
    pub choices: Vec<EffortChoice>,
}

/// Who the CLI is signed in as, when it says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AccountInfo {
    /// The signed-in email, when the CLI reports one.
    pub email: Option<String>,
    /// The organization name, when there is one.
    pub organization: Option<String>,
    /// `pro`, `max`, `team`, `enterprise`, or absent for API-key sessions.
    pub subscription_type: Option<String>,
    /// Where the credential came from; `"none"` means there is none.
    pub token_source: Option<String>,
    /// `firstParty`, `bedrock`, `vertex`, `foundry`, `gateway`.
    pub api_provider: Option<String>,
}

/// Everything the handshake told us about this session.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Catalog {
    commands: Vec<CommandInfo>,
    terminal_commands: Vec<String>,
    models: Vec<ModelInfo>,
    current_mode_id: String,
    account: AccountInfo,
}

impl Catalog {
    /// Reads the `initialize` response's payload.
    #[must_use]
    pub fn from_initialize(payload: &Value) -> Self {
        let terminal_commands: Vec<String> = payload
            .get("terminal_slash_commands")
            .and_then(|value| value.as_array())
            .map(|names| {
                names
                    .iter()
                    .filter_map(|name| name.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let commands = payload
            .get("commands")
            .and_then(|value| value.as_array())
            .map(|commands| {
                commands
                    .iter()
                    .filter_map(|command| {
                        Some(CommandInfo {
                            name: command.get("name")?.as_str()?.to_string(),
                            description: command
                                .get("description")
                                .and_then(|text| text.as_str())
                                .unwrap_or_default()
                                .to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let models = payload
            .get("models")
            .and_then(|value| value.as_array())
            .map(|models| {
                models
                    .iter()
                    .filter_map(|model| {
                        Some(ModelInfo {
                            id: model.get("value")?.as_str()?.to_string(),
                            name: model
                                .get("displayName")
                                .and_then(|name| name.as_str())
                                .unwrap_or_default()
                                .to_string(),
                            description: model
                                .get("description")
                                .and_then(|text| text.as_str())
                                .map(str::to_string),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let account = payload
            .get("account")
            .map(|account| AccountInfo {
                email: string_field(account, "email"),
                organization: string_field(account, "organization"),
                subscription_type: string_field(account, "subscriptionType"),
                token_source: string_field(account, "tokenSource"),
                api_provider: string_field(account, "apiProvider"),
            })
            .unwrap_or_default();
        Self {
            commands,
            terminal_commands,
            models,
            current_mode_id: payload
                .get("current_permission_mode")
                .and_then(|mode| mode.as_str())
                .unwrap_or("default")
                .to_string(),
            account,
        }
    }

    /// The commands a chat tab can offer: everything the session advertised,
    /// minus the terminal-bound ones and the hidden list.
    #[must_use]
    pub fn commands(&self) -> Vec<CommandInfo> {
        self.commands
            .iter()
            .filter(|command| !self.terminal_commands.contains(&command.name))
            .filter(|command| !HIDDEN_COMMANDS.contains(&command.name.as_str()))
            .cloned()
            .collect()
    }

    /// The models, in the order the CLI listed them.
    #[must_use]
    pub fn models(&self) -> Vec<ModelInfo> {
        self.models.clone()
    }

    /// The mode selector, with the session's current mode marked.
    #[must_use]
    pub fn modes(&self) -> Modes {
        Modes {
            current_id: self.current_mode_id.clone(),
            options: OFFERED_MODES
                .iter()
                .map(|(id, name, description)| ModeInfo {
                    id: (*id).to_string(),
                    name: (*name).to_string(),
                    description: (*description).to_string(),
                })
                .collect(),
        }
    }

    /// The effort selector. The levels are fixed by the CLI's own flag, not
    /// advertised in the handshake, so they are named here.
    #[must_use]
    pub fn effort(&self) -> Effort {
        Effort {
            option_id: "effort".to_string(),
            choices: EFFORT_CHOICES
                .iter()
                .map(|(value, name)| EffortChoice {
                    value: (*value).to_string(),
                    name: (*name).to_string(),
                })
                .collect(),
        }
    }

    /// The signed-in account, as reported.
    #[must_use]
    pub fn account(&self) -> &AccountInfo {
        &self.account
    }

    /// Whether this session has no usable credential.
    ///
    /// Only a first-party session can be judged from here: Bedrock, Vertex
    /// and the rest authenticate outside the CLI, so a missing token there
    /// says nothing. `system/init`'s `apiKeySource` is **not** the signal —
    /// a logged-in OAuth session reports `"none"` for it too.
    #[must_use]
    pub fn is_logged_out(&self) -> bool {
        let first_party = self
            .account
            .api_provider
            .as_deref()
            .is_none_or(|provider| provider == "firstParty");
        first_party
            && self.account.token_source.as_deref() == Some("none")
            && self.account.email.is_none()
    }
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|field| field.as_str())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    const INITIALIZE_RESPONSE: &str = include_str!("../tests/fixtures/initialize_response.json");

    fn catalog() -> Catalog {
        let line: serde_json::Value =
            serde_json::from_str(INITIALIZE_RESPONSE).expect("fixture is valid JSON");
        let payload = line["response"]["response"].clone();
        Catalog::from_initialize(&payload)
    }

    #[test]
    fn models_keep_the_order_and_the_names_the_cli_gave() {
        let models = catalog().models();
        assert_eq!(
            models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            ["default", "claude-fable-5-1", "sonnet"]
        );
        assert_eq!(models[1].name, "Fable");
        assert_eq!(
            models[0].description.as_deref(),
            Some("Default (recommended)")
        );
    }

    #[test]
    fn the_terminal_and_hidden_commands_never_reach_the_picker() {
        let commands = catalog().commands();
        let names: Vec<&str> = commands
            .iter()
            .map(|command| command.name.as_str())
            .collect();
        assert!(names.contains(&"usage"));
        assert!(names.contains(&"context"));
        // `statusline` is terminal-bound; `login`, `clear` and `todos` are on
        // the hidden list a desktop chat cannot honour.
        for hidden in ["statusline", "login", "clear", "todos"] {
            assert!(!names.contains(&hidden), "{hidden} must not be offered");
        }
    }

    #[test]
    fn the_modes_are_the_four_sirio_offers_with_the_current_one_marked() {
        let modes = catalog().modes();
        assert_eq!(
            modes
                .options
                .iter()
                .map(|mode| mode.id.as_str())
                .collect::<Vec<_>>(),
            ["default", "acceptEdits", "plan", "auto"]
        );
        assert_eq!(modes.current_id, "default");
        // bypassPermissions needs a flag Sirio does not pass, so offering it
        // would be offering something that cannot work.
        assert!(
            !modes
                .options
                .iter()
                .any(|mode| mode.id == "bypassPermissions")
        );
    }

    #[test]
    fn effort_offers_the_three_levels_plus_the_reset() {
        let effort = catalog().effort();
        assert_eq!(effort.option_id, "effort");
        assert_eq!(
            effort
                .choices
                .iter()
                .map(|choice| choice.value.as_str())
                .collect::<Vec<_>>(),
            ["default", "low", "medium", "high"]
        );
    }

    #[test]
    fn a_first_party_session_with_no_token_is_logged_out() {
        let logged_out = Catalog::from_initialize(&serde_json::json!({
            "account": {"tokenSource": "none", "apiProvider": "firstParty"}
        }));
        assert!(logged_out.is_logged_out());

        // A logged-in OAuth session: the account carries an identity.
        let logged_in = Catalog::from_initialize(&serde_json::json!({
            "account": {"email": "dev@example.com", "apiProvider": "firstParty",
                        "subscriptionType": "max"}
        }));
        assert!(!logged_in.is_logged_out());

        // A third-party provider authenticates outside the CLI entirely, so
        // an absent token there is not a logged-out session.
        let bedrock = Catalog::from_initialize(&serde_json::json!({
            "account": {"tokenSource": "none", "apiProvider": "bedrock"}
        }));
        assert!(!bedrock.is_logged_out());
    }
}
