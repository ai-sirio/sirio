/// Native updater state, independent of whichever Linux update transport is
/// eventually selected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateState {
    Idle,
    Checking,
    Available { version: String },
    Downloading { progress_percent: u8 },
    Installing,
    UpToDate,
    Failed { message: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateEvent {
    CheckStarted,
    Available(String),
    DownloadProgress(u8),
    InstallStarted,
    Finished,
    Failed(String),
    Reset,
}

impl UpdateState {
    pub fn transition(self, event: UpdateEvent) -> Self {
        match event {
            UpdateEvent::CheckStarted => Self::Checking,
            UpdateEvent::Available(version) => Self::Available { version },
            UpdateEvent::DownloadProgress(progress_percent) => Self::Downloading {
                progress_percent: progress_percent.min(100),
            },
            UpdateEvent::InstallStarted => Self::Installing,
            UpdateEvent::Finished => Self::UpToDate,
            UpdateEvent::Failed(message) => Self::Failed { message },
            UpdateEvent::Reset => Self::Idle,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updater_reaches_every_user_visible_state_and_clamps_progress() {
        let state = UpdateState::Idle.transition(UpdateEvent::CheckStarted);
        assert_eq!(state, UpdateState::Checking);
        let state = state.transition(UpdateEvent::Available("1.2".into()));
        assert_eq!(
            state,
            UpdateState::Available {
                version: "1.2".into()
            }
        );
        let state = state.transition(UpdateEvent::DownloadProgress(150));
        assert_eq!(
            state,
            UpdateState::Downloading {
                progress_percent: 100
            }
        );
        assert_eq!(
            state.clone().transition(UpdateEvent::InstallStarted),
            UpdateState::Installing
        );
        assert_eq!(
            state.clone().transition(UpdateEvent::Finished),
            UpdateState::UpToDate
        );
        assert_eq!(
            state.transition(UpdateEvent::Failed("network".into())),
            UpdateState::Failed {
                message: "network".into()
            }
        );
    }
}
