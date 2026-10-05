//! Which majors meet (spec §6.4). The minor is additive and never refuses.

pub const PROTOCOL_MAJOR: u32 = 1;
pub const PROTOCOL_MINOR: u32 = 0;

/// The majors a peer built at `current` speaks: its own and, once there is
/// one, the previous (spec §1, "the app speaks majors N and N−1").
pub fn majors_spoken(current: u32) -> Vec<u32> {
    if current > 1 {
        vec![current, current - 1]
    } else {
        vec![current]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Negotiation {
    Accept { major: u32 },
    Refuse { host_major: u32 },
}

pub fn negotiate(host_major: u32, client_majors: &[u32]) -> Negotiation {
    if client_majors.contains(&host_major) {
        Negotiation::Accept { major: host_major }
    } else {
        Negotiation::Refuse { host_major }
    }
}

/// The major this build acts as. The override is read by callers only in
/// debug builds (`SIRIO_HOST_PROTOCOL_MAJOR`), so a release binary cannot be
/// talked into another major.
pub fn effective_major(override_value: Option<&str>) -> u32 {
    override_value
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|major| *major > 0)
        .unwrap_or(PROTOCOL_MAJOR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_of_the_current_major_is_accepted() {
        assert_eq!(negotiate(3, &[3, 2]), Negotiation::Accept { major: 3 });
    }
    #[test]
    fn a_host_one_major_behind_is_accepted() {
        assert_eq!(negotiate(2, &[3, 2]), Negotiation::Accept { major: 2 });
    }
    #[test]
    fn a_newer_host_is_refused() {
        assert_eq!(negotiate(4, &[3, 2]), Negotiation::Refuse { host_major: 4 });
    }
    #[test]
    fn a_host_two_majors_behind_is_refused() {
        assert_eq!(negotiate(1, &[3, 2]), Negotiation::Refuse { host_major: 1 });
    }
    #[test]
    fn a_client_that_speaks_nothing_is_refused() {
        assert_eq!(negotiate(1, &[]), Negotiation::Refuse { host_major: 1 });
    }
    #[test]
    fn major_one_has_no_previous_major() {
        assert_eq!(majors_spoken(1), vec![1]);
        assert_eq!(majors_spoken(2), vec![2, 1]);
    }
    #[test]
    fn the_override_takes_only_positive_integers() {
        assert_eq!(effective_major(None), PROTOCOL_MAJOR);
        assert_eq!(effective_major(Some("2")), 2);
        assert_eq!(effective_major(Some("0")), PROTOCOL_MAJOR);
        assert_eq!(effective_major(Some("two")), PROTOCOL_MAJOR);
        assert_eq!(effective_major(Some("")), PROTOCOL_MAJOR);
    }
}
