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

/// The value that leaves the model on its own default effort. Not a level
/// the CLI names: on the wire it is a null `effortLevel`.
pub const EFFORT_DEFAULT: &str = "default";

/// Ultracode, offered beside the levels exactly the way the CLI's own
/// `/effort` menu offers it (`low|medium|high|xhigh|ultracode|auto`). It is
/// *not* a level on the wire — see `ControlRequest::set_effort` for the
/// boolean it actually sets.
pub const EFFORT_ULTRACODE: &str = "ultracode";

/// How each level the CLI reports is read out. A level missing from this
/// table still reaches the picker under its own wire spelling: what the
/// session advertises is the authority, this is only its display name.
const EFFORT_LEVEL_NAMES: [(&str, &str); 5] = [
    ("low", "Low"),
    ("medium", "Medium"),
    ("high", "High"),
    ("xhigh", "Extra high"),
    ("max", "Max"),
];

/// What to offer a model that claims effort support without enumerating it.
/// Offering one level too many degrades well — the CLI clamps anything above
/// a model's cap and says which level it used instead — while offering none
/// would retire the selector silently, which is the worse failure.
const EFFORT_LADDER: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

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
    /// The effort levels this model offers, in the CLI's own order, as the
    /// handshake reported them. Empty when the model has no effort at all —
    /// Haiku, as of 2.1.274, which sends neither effort field.
    pub effort_levels: Vec<String>,
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
                            effort_levels: model_effort_levels(model),
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

    /// The effort selector for `model_id`, or `None` when that model has no
    /// effort to select.
    ///
    /// The levels are neither fixed nor universal: the handshake reports
    /// them per model (`supportedEffortLevels`), the CLI builds its own
    /// `/effort` menu the same way, and a model may offer all five, none, or
    /// something in between. Naming them here instead is what kept `xhigh`,
    /// `max` and Ultracode out of the picker while offering three levels on
    /// a model that has none.
    #[must_use]
    pub fn effort_for(&self, model_id: &str) -> Option<Effort> {
        let levels = &self
            .models
            .iter()
            .find(|model| model.id == model_id)?
            .effort_levels;
        if levels.is_empty() {
            return None;
        }
        let mut choices = vec![EffortChoice {
            value: EFFORT_DEFAULT.to_string(),
            name: "Default".to_string(),
        }];
        choices.extend(levels.iter().map(|level| EffortChoice {
            value: level.clone(),
            name: effort_level_name(level),
        }));
        // Ultracode *is* xhigh effort, plus standing dynamic-workflow
        // orchestration, so it is offered exactly where xhigh is — the same
        // gate the CLI's own menu uses.
        if levels.iter().any(|level| level == "xhigh") {
            choices.push(EffortChoice {
                value: EFFORT_ULTRACODE.to_string(),
                name: "Ultracode".to_string(),
            });
        }
        Some(Effort {
            option_id: "effort".to_string(),
            choices,
        })
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

/// The effort ladder a model advertises. `supportedEffortLevels` is the
/// answer whenever it is there; `supportsEffort` alone means a CLI that kept
/// the claim and dropped the list, which falls back rather than reporting no
/// effort at all.
fn model_effort_levels(model: &Value) -> Vec<String> {
    let advertised: Vec<String> = model
        .get("supportedEffortLevels")
        .and_then(|value| value.as_array())
        .map(|levels| {
            levels
                .iter()
                .filter_map(|level| level.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if !advertised.is_empty() {
        return advertised;
    }
    if model.get("supportsEffort").and_then(Value::as_bool) == Some(true) {
        return EFFORT_LADDER
            .iter()
            .map(|level| (*level).to_string())
            .collect();
    }
    Vec::new()
}

fn effort_level_name(level: &str) -> String {
    EFFORT_LEVEL_NAMES
        .iter()
        .find(|(value, _)| *value == level)
        .map_or_else(|| level.to_string(), |(_, name)| (*name).to_string())
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
            ["default", "claude-fable-5-1", "sonnet", "haiku"]
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

    fn values(effort: &Effort) -> Vec<&str> {
        effort
            .choices
            .iter()
            .map(|choice| choice.value.as_str())
            .collect()
    }

    #[test]
    fn effort_offers_every_level_the_model_advertises_plus_ultracode() {
        let catalog = catalog();
        let effort = catalog.effort_for("sonnet").expect("sonnet has effort");
        assert_eq!(effort.option_id, "effort");
        // `xhigh` and `max` are levels the CLI has accepted all along
        // (`--effort low|medium|high|xhigh|max`); Ultracode rides beside
        // them the way the CLI's own `/effort` menu lists it.
        assert_eq!(
            values(&effort),
            [
                "default",
                "low",
                "medium",
                "high",
                "xhigh",
                "max",
                "ultracode"
            ]
        );
        assert_eq!(
            effort
                .choices
                .iter()
                .map(|choice| choice.name.as_str())
                .collect::<Vec<_>>(),
            [
                "Default",
                "Low",
                "Medium",
                "High",
                "Extra high",
                "Max",
                "Ultracode"
            ]
        );
    }

    #[test]
    fn a_model_with_no_effort_has_no_selector() {
        // Haiku, as the CLI really reports it: neither `supportsEffort` nor
        // `supportedEffortLevels`. Offering it three levels was as wrong as
        // hiding two from the models that have five.
        assert!(catalog().effort_for("haiku").is_none());
        assert!(catalog().effort_for("no-such-model").is_none());
    }

    #[test]
    fn a_model_without_xhigh_is_not_offered_ultracode() {
        let catalog = Catalog::from_initialize(&serde_json::json!({
            "models": [{
                "value": "thrifty", "displayName": "Thrifty",
                "supportsEffort": true,
                "supportedEffortLevels": ["low", "medium", "high"],
            }]
        }));
        let effort = catalog.effort_for("thrifty").expect("thrifty has effort");
        assert_eq!(values(&effort), ["default", "low", "medium", "high"]);
    }

    #[test]
    fn a_level_the_table_does_not_name_still_reaches_the_picker() {
        // The protocol is versioned with the CLI and self-updates: a level
        // added tomorrow is offered under its wire spelling rather than
        // dropped for want of a display name.
        let catalog = Catalog::from_initialize(&serde_json::json!({
            "models": [{
                "value": "next", "displayName": "Next",
                "supportedEffortLevels": ["high", "colossal"],
            }]
        }));
        let effort = catalog.effort_for("next").expect("next has effort");
        assert_eq!(values(&effort), ["default", "high", "colossal"]);
        assert_eq!(effort.choices[2].name, "colossal");
    }

    #[test]
    fn a_claim_of_effort_without_a_list_falls_back_to_the_ladder() {
        // Losing the selector outright would be a worse answer than
        // offering a level the CLI will clamp and report clamping.
        let catalog = Catalog::from_initialize(&serde_json::json!({
            "models": [{
                "value": "terse", "displayName": "Terse", "supportsEffort": true,
            }]
        }));
        let effort = catalog.effort_for("terse").expect("terse claims effort");
        assert_eq!(
            values(&effort),
            [
                "default",
                "low",
                "medium",
                "high",
                "xhigh",
                "max",
                "ultracode"
            ]
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
