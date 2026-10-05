//! Whether a host exists, from three observations (spec §5.2). The rule this
//! encodes: loss of contact is never evidence that a process is dead.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handshake {
    Completed,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockState {
    Held,
    Free,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessObservation {
    NoRecord,
    Gone,
    SameStart,
    DifferentStart,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Live,
    Unverifiable,
    Absent,
}

pub fn verdict(handshake: Handshake, lock: LockState, process: ProcessObservation) -> Verdict {
    match (handshake, lock, process) {
        (Handshake::Completed, _, _) => Verdict::Live,
        (Handshake::Failed, LockState::Held, _) => Verdict::Unverifiable,
        (Handshake::Failed, LockState::Free, ProcessObservation::SameStart) => {
            Verdict::Unverifiable
        }
        (
            Handshake::Failed,
            LockState::Free,
            ProcessObservation::NoRecord
            | ProcessObservation::Gone
            | ProcessObservation::DifferentStart,
        ) => Verdict::Absent,
    }
}

pub fn observe_process(recorded_start: u64, current_start: Option<u64>) -> ProcessObservation {
    match current_start {
        None => ProcessObservation::Gone,
        Some(start) if start == recorded_start => ProcessObservation::SameStart,
        Some(_) => ProcessObservation::DifferentStart,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_observation_has_one_verdict() {
        use Handshake::*;
        use LockState::*;
        use ProcessObservation::*;
        use Verdict::*;
        let cases = [
            // A host that answers is live, whatever the files say.
            (Completed, Held, SameStart, Live),
            (Completed, Free, NoRecord, Live),
            (Completed, Free, Gone, Live),
            (Completed, Held, DifferentStart, Live),
            // Silent but the lock is held: starting or hung — never replace it.
            (Failed, Held, NoRecord, Unverifiable),
            (Failed, Held, Gone, Unverifiable),
            (Failed, Held, SameStart, Unverifiable),
            (Failed, Held, DifferentStart, Unverifiable),
            // Lock free but the recorded process is still the same process.
            (Failed, Free, SameStart, Unverifiable),
            // Lock free and the recorded process is gone or was replaced.
            (Failed, Free, NoRecord, Absent),
            (Failed, Free, Gone, Absent),
            (Failed, Free, DifferentStart, Absent),
        ];
        for (h, l, p, expected) in cases {
            assert_eq!(verdict(h, l, p), expected, "{h:?} {l:?} {p:?}");
        }
    }

    #[test]
    fn a_recycled_pid_is_a_different_process() {
        assert_eq!(
            observe_process(100, Some(100)),
            ProcessObservation::SameStart
        );
        assert_eq!(
            observe_process(100, Some(250)),
            ProcessObservation::DifferentStart
        );
        assert_eq!(observe_process(100, None), ProcessObservation::Gone);
    }
}
