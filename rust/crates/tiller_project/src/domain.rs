use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Deterministic Linux project defaults. An explicit environment override is
/// useful for tests and portable installations; XDG data is the normal base.
pub fn default_project_base() -> PathBuf {
    if let Some(path) = std::env::var_os("TILLER_PROJECTS_DIR").filter(|value| !value.is_empty()) {
        return PathBuf::from(path);
    }
    if let Some(data) = std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        return PathBuf::from(data).join("Tiller/projects");
    }
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Tiller/projects"))
        .unwrap_or_else(|| PathBuf::from("Tiller/projects"))
}

/// Chooses the branch base and parent directory for a new worktree.
pub fn resolve_worktree_defaults(
    project_root: &Path,
    explicit_base_branch: Option<&str>,
    primary_branch: Option<&str>,
    explicit_location: Option<&Path>,
) -> (String, PathBuf) {
    let branch = explicit_base_branch
        .filter(|branch| !branch.trim().is_empty())
        .or(primary_branch)
        .unwrap_or("HEAD")
        .to_string();
    let parent = explicit_location.map(Path::to_path_buf).unwrap_or_else(|| {
        project_root
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| project_root.to_path_buf())
    });
    (branch, parent)
}

/// Moves a known item before a target, or to the end when no target is given.
/// Unknown ids and no-op moves leave the sequence untouched.
pub fn move_item<T: Eq + Copy>(items: &mut Vec<T>, item: T, before: Option<T>) -> bool {
    let Some(from) = items.iter().position(|candidate| *candidate == item) else {
        return false;
    };
    let target = before.and_then(|target| items.iter().position(|candidate| *candidate == target));
    if before.is_some() && target.is_none() {
        return false;
    }
    let value = items.remove(from);
    let destination = match target {
        Some(target) if target > from => target - 1,
        Some(target) => target,
        None => items.len(),
    };
    if destination == from {
        items.insert(from, value);
        return false;
    }
    items.insert(destination, value);
    true
}

/// Numeric tab selection: 1-based, with 9 selecting the last tab.
pub fn numeric_tab_selection(number: u8, count: usize) -> Option<usize> {
    if count == 0 || number == 0 {
        return None;
    }
    let index = if number == 9 {
        count - 1
    } else {
        usize::from(number - 1)
    };
    (index < count).then_some(index)
}

/// A moving tab index that wraps at either edge.
pub fn move_tab(index: usize, count: usize, direction: i8) -> Option<usize> {
    if count == 0 || index >= count {
        return None;
    }
    Some(if direction < 0 {
        if index == 0 { count - 1 } else { index - 1 }
    } else if index + 1 == count {
        0
    } else {
        index + 1
    })
}

/// Throttles automatic tab naming by both elapsed time and transcript growth.
#[derive(Debug, Default)]
pub struct AutoNamingThrottle {
    last_request: Option<Instant>,
    last_transcript_len: usize,
}

impl AutoNamingThrottle {
    pub const MIN_INTERVAL: Duration = Duration::from_secs(30);
    pub const MIN_GROWTH: usize = 200;

    /// Whether a generated-name request should fire now. The first run is
    /// never throttled — both gates apply only once a request was recorded,
    /// mirroring the reference `AutoNamingThrottle.swift`.
    pub fn should_request(&self, now: Instant, transcript_len: usize) -> bool {
        let Some(last) = self.last_request else {
            return true;
        };
        now.duration_since(last) >= Self::MIN_INTERVAL
            && transcript_len.saturating_sub(self.last_transcript_len) >= Self::MIN_GROWTH
    }

    pub fn record_request(&mut self, now: Instant, transcript_len: usize) {
        self.last_request = Some(now);
        self.last_transcript_len = transcript_len;
    }
}

/// A value-type once gate for restore/setup callbacks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OnceGate(bool);

impl OnceGate {
    pub fn fire(&mut self, action: impl FnOnce()) -> bool {
        if self.0 {
            return false;
        }
        self.0 = true;
        action();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_prefer_explicit_values_then_primary_and_sibling() {
        let (branch, parent) =
            resolve_worktree_defaults(Path::new("/home/me/tiller"), None, Some("main"), None);
        assert_eq!(branch, "main");
        assert_eq!(parent, PathBuf::from("/home/me"));
        let (branch, parent) = resolve_worktree_defaults(
            Path::new("/home/me/tiller"),
            Some("develop"),
            Some("main"),
            Some(Path::new("/worktrees")),
        );
        assert_eq!(
            (branch, parent),
            ("develop".into(), PathBuf::from("/worktrees"))
        );
    }

    #[test]
    fn ordering_ignores_unknown_and_noop_moves() {
        let mut ids = vec![1, 2, 3];
        assert!(!move_item(&mut ids, 9, Some(1)));
        assert!(!move_item(&mut ids, 2, Some(2)));
        assert!(move_item(&mut ids, 1, Some(3)));
        assert_eq!(ids, vec![2, 1, 3]);
        assert!(move_item(&mut ids, 1, None));
        assert_eq!(ids, vec![2, 3, 1]);
    }

    #[test]
    fn tab_order_wraps_and_numeric_selection_validates() {
        assert_eq!(move_tab(0, 3, -1), Some(2));
        assert_eq!(move_tab(2, 3, 1), Some(0));
        assert_eq!(numeric_tab_selection(1, 3), Some(0));
        assert_eq!(numeric_tab_selection(9, 3), Some(2));
        assert_eq!(numeric_tab_selection(4, 3), None);
    }

    /// F-CORE-DOM-06: pins the two edges a clamping reimplementation gets
    /// wrong. Position 9 always means "the last tab", even with more than
    /// nine tabs open -- not "the ninth tab" -- and a position beyond the
    /// group is a rejected selection (`None`, i.e. no change), not a silent
    /// clamp to the last tab.
    #[test]
    fn position_nine_always_means_the_last_tab_and_overflow_is_rejected_not_clamped() {
        assert_eq!(numeric_tab_selection(9, 12), Some(11));
        assert_eq!(numeric_tab_selection(5, 3), None);
    }

    #[test]
    fn auto_naming_requires_first_run_or_both_throttles() {
        let start = Instant::now();
        let mut throttle = AutoNamingThrottle::default();
        assert!(
            throttle.should_request(start, 199),
            "the first run is exempt from both gates"
        );
        assert!(throttle.should_request(start, 200));
        throttle.record_request(start, 200);
        assert!(!throttle.should_request(start + Duration::from_secs(31), 399));
        assert!(!throttle.should_request(start + Duration::from_secs(29), 401));
        assert!(throttle.should_request(start + Duration::from_secs(31), 400));
    }

    #[test]
    fn once_gate_runs_only_the_first_callback() {
        let mut gate = OnceGate::default();
        let mut calls = 0;
        assert!(gate.fire(|| calls += 1));
        assert!(!gate.fire(|| calls += 1));
        assert_eq!(calls, 1);
    }
}
