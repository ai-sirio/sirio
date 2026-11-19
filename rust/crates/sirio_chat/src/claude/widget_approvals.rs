//! The one-shot approvals that let a widget's `mcp_call` through the CLI's own permission ask.
//!
//! The user presses Allow on a card Sirio drew; the CLI then asks its own `can_use_tool` for the
//! call under a `toolu_mcp_call_` id, and Sirio answers it from here instead of drawing a second
//! card. An approval is for one tool and one input, is used once, and lapses with the call's
//! answer or after [`APPROVAL_TTL`]. An ask nothing here covers is denied.

use std::time::{Duration, Instant};

use serde_json::Value;

/// How long a grant lasts without the call answering. The CLI asks within a moment of
/// receiving the call, so this is a bound on a mistake, not on a person.
pub(crate) const APPROVAL_TTL: Duration = Duration::from_secs(10);

#[derive(Debug)]
struct Grant {
    /// The request id of the `mcp_call` the grant was made for.
    call: String,
    tool: String,
    input: Value,
    expires: Instant,
}

/// The grants that are live, oldest first.
#[derive(Debug, Default)]
pub(crate) struct WidgetApprovals {
    grants: Vec<Grant>,
}

impl WidgetApprovals {
    /// Approves one ask for `tool` with exactly `input`, until `call` is answered.
    pub(crate) fn grant(&mut self, call: &str, tool: &str, input: &Value, now: Instant) {
        self.prune(now);
        self.grants.push(Grant {
            call: call.to_string(),
            tool: tool.to_string(),
            input: input.clone(),
            expires: now + APPROVAL_TTL,
        });
    }

    /// Whether an ask for `tool` with `input` is approved, using up the oldest grant that covers
    /// it. The input is compared as JSON, so key order is not part of it.
    pub(crate) fn take(&mut self, tool: &str, input: &Value, now: Instant) -> bool {
        self.prune(now);
        let Some(at) = self
            .grants
            .iter()
            .position(|grant| grant.tool == tool && grant.input == *input)
        else {
            return false;
        };
        self.grants.remove(at);
        true
    }

    /// Drops what `call` granted: its answer came, so nothing is left to approve.
    pub(crate) fn revoke(&mut self, call: &str) {
        self.grants.retain(|grant| grant.call != call);
    }

    fn prune(&mut self, now: Instant) {
        self.grants.retain(|grant| grant.expires > now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{Duration, Instant};

    // Failure list for `take`.
    //
    // It may wrongly return true for: a different input; a different tool; a second take of one
    // grant; a take at exactly the TTL; a revoked grant; no grant at all.
    // It may wrongly return false for: the same input with its keys in another order; a take
    // just inside the TTL; two grants of one tool and input, which allow exactly two takes.

    #[test]
    fn an_approval_is_used_once_for_its_exact_call() {
        let now = Instant::now();
        let mut approvals = WidgetApprovals::default();
        assert!(!approvals.take("mcp__apps__act", &json!({"step": 1}), now), "no grant");
        approvals.grant("c1", "mcp__apps__act", &json!({"step": 1, "x": {"a": 1, "b": 2}}), now);
        assert!(!approvals.take("mcp__apps__act", &json!({"step": 2}), now), "other input");
        assert!(!approvals.take("mcp__apps__report", &json!({"step": 1}), now), "other tool");
        assert!(
            approvals.take("mcp__apps__act", &json!({"x": {"b": 2, "a": 1}, "step": 1}), now),
            "key order is not part of the input"
        );
        assert!(
            !approvals.take("mcp__apps__act", &json!({"step": 1, "x": {"a": 1, "b": 2}}), now),
            "a second take of one grant"
        );
    }

    #[test]
    fn an_approval_lapses_or_is_revoked() {
        let start = Instant::now();
        let input = json!({"step": 1});
        let mut approvals = WidgetApprovals::default();
        approvals.grant("c1", "mcp__apps__act", &input, start);
        assert!(!approvals.take("mcp__apps__act", &input, start + APPROVAL_TTL), "at the TTL");
        approvals.grant("c2", "mcp__apps__act", &input, start);
        let just_inside = APPROVAL_TTL - Duration::from_millis(100);
        assert!(approvals.take("mcp__apps__act", &input, start + just_inside), "inside the TTL");
        approvals.grant("c3", "mcp__apps__act", &input, start);
        approvals.revoke("c3");
        assert!(!approvals.take("mcp__apps__act", &input, start), "revoked");
    }

    #[test]
    fn two_identical_calls_get_one_approval_each() {
        let now = Instant::now();
        let input = json!({"step": 1});
        let mut approvals = WidgetApprovals::default();
        approvals.grant("c1", "mcp__apps__act", &input, now);
        approvals.grant("c2", "mcp__apps__act", &input, now);
        assert!(approvals.take("mcp__apps__act", &input, now));
        assert!(approvals.take("mcp__apps__act", &input, now));
        assert!(!approvals.take("mcp__apps__act", &input, now));
    }
}
