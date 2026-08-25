//! The qualified identity of an agent-backed tab, as persisted.
//!
//! `tab.agent_id` today holds a bare adapter id ("codex", "claude", …).
//! Tiller is about to support agents taken from the ACP registry, whose
//! ids live in a different namespace and collide with adapter ids:
//! [`tiller_registry::resolve::registry_id`] returns `"opencode"` for the
//! OpenCode adapter — the one id the registry does not remap to an `-acp`
//! name. A bare string cannot say which namespace it came from, so this
//! value type makes the origin part of the type: `adapter:<id>` and
//! `registry:<id>` are the only two forms that can round-trip, and decoding
//! happens in exactly one place, [`AgentRef::from_db`], instead of being
//! re-implemented wherever the id is read.
//!
//! The five built-in adapters are still the only *producers* of this
//! column today — `AgentRef::Registry` has no producer yet and is correct
//! and deliberate: it keeps the orientation (persisted form, decode point,
//! migration) correct for the day the registry joins, without pretending
//! the app can launch a registry agent yet.

use std::fmt;

/// The qualified agent identity of a tab, serialized in the `agent_id`
/// column as `"adapter:<id>"` / `"registry:<id>"`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentRef {
    /// A built-in catalog adapter, by its adapter id (`AgentAdapter::id`).
    Adapter(String),
    /// An ACP registry agent, by its registry id. No producer yet; the
    /// variant exists so the persisted form and the migration are already
    /// right before registry agents can be launched.
    Registry(String),
}

impl AgentRef {
    /// Wraps a built-in adapter id. This is the producer side for the
    /// persistence path today: nothing constructs a `Registry` yet.
    pub fn adapter(id: impl Into<String>) -> Self {
        Self::Adapter(id.into())
    }

    /// The only decode point for a stored value. A bare unprefixed id is
    /// treated as an `Adapter` (the historical form of this column, where
    /// only adapters could write), so a database that predates v15 still
    /// reads back correctly — the same rule `migrate_v15` applies when
    /// normalizing stored rows.
    pub fn from_db(raw: &str) -> Self {
        if let Some(id) = raw.strip_prefix("adapter:") {
            Self::Adapter(id.to_owned())
        } else if let Some(id) = raw.strip_prefix("registry:") {
            Self::Registry(id.to_owned())
        } else {
            Self::Adapter(raw.to_owned())
        }
    }

    /// The on-disk form: `adapter:<id>` / `registry:<id>`. This is the one
    /// serialization used by the persistence layer (and by v15's migration).
    pub fn to_db_string(&self) -> String {
        self.to_string()
    }

    /// The raw adapter id when this is an `Adapter` ref — the only kind the
    /// shell can currently launch — and `None` for a `Registry` ref. Call
    /// sites that resolve a persisted id against the adapter catalog use
    /// this to get the id they know how to look up; a registry agent has no
    /// launch path yet, so `None` is the honest answer and restores it as
    /// unresolvable.
    pub fn adapter_id(&self) -> Option<&str> {
        match self {
            Self::Adapter(id) => Some(id),
            Self::Registry(_) => None,
        }
    }
}

impl fmt::Display for AgentRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Adapter(id) => write!(f, "adapter:{id}"),
            Self::Registry(id) => write!(f, "registry:{id}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_trip_through_the_db_string_forms() {
        let cases = [
            (AgentRef::adapter("codex"), "adapter:codex"),
            (AgentRef::Registry("custom".into()), "registry:custom"),
        ];
        for (agent_ref, expected) in cases {
            assert_eq!(agent_ref.to_db_string(), expected);
            assert_eq!(AgentRef::from_db(expected), agent_ref);
        }
    }

    #[test]
    fn a_bare_id_from_before_v15_decodes_as_an_adapter() {
        assert_eq!(AgentRef::from_db("opencode"), AgentRef::adapter("opencode"));
    }

    #[test]
    fn adapter_id_is_some_only_for_adapter_refs() {
        assert_eq!(AgentRef::adapter("codex").adapter_id(), Some("codex"));
        assert_eq!(AgentRef::Registry("custom".into()).adapter_id(), None);
    }
}
