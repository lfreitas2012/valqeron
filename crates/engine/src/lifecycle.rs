use tokio::sync::watch;

// ================ STATES ================
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleState {
    Starting,
    Ready,
    Stopping,
    Stopped,
    Failed,
}

impl LifecycleState {
    pub const fn as_str(self) -> &'static str {
        match self {
            LifecycleState::Starting => "starting",
            LifecycleState::Ready => "ready",
            LifecycleState::Stopping => "stopping",
            LifecycleState::Stopped => "stopped",
            LifecycleState::Failed => "failed",
        }
    }
}

impl std::fmt::Display for LifecycleState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ================ TRANSITION TABLE ================
const fn is_valid_transition(from: LifecycleState, to: LifecycleState) -> bool {
    use LifecycleState::{Failed, Ready, Starting, Stopped, Stopping};
    match (from, to) {
        (Starting, Ready)
        | (Starting, Failed)
        | (Ready, Stopping)
        | (Stopping, Stopped)
        | (Stopping, Failed) => true,
        (Starting | Ready | Stopping | Stopped | Failed, _) => false,
    }
}

// ================ ERRORS ================
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("invalid engine lifecycle transition: {from} -> {to}")]
pub struct InvalidTransition {
    pub from: LifecycleState,
    pub to: LifecycleState,
}

// ================ THE AUTHORITY ================
pub struct Lifecycle {
    tx: watch::Sender<LifecycleState>,
}

impl Lifecycle {
    pub fn new() -> (Self, watch::Receiver<LifecycleState>) {
        let (tx, rx) = watch::channel(LifecycleState::Starting);
        (Self { tx }, rx)
    }

    pub fn transition(&self, to: LifecycleState) -> Result<LifecycleState, InvalidTransition> {
        let mut from = LifecycleState::Starting;
        let mut valid = false;
        self.tx.send_if_modified(|state| {
            from = *state;
            valid = is_valid_transition(from, to);
            if valid {
                *state = to;
            }
            valid
        });

        if !valid {
            tracing::error!(
                from = from.as_str(),
                to = to.as_str(),
                "invalid engine lifecycle transition; state unchanged"
            );
            return Err(InvalidTransition { from, to });
        }

        tracing::info!(
            target: "valqeron::audit",
            operation = "lifecycle_transition",
            from = from.as_str(),
            to = to.as_str(),
            "engine lifecycle transition"
        );
        match to {
            LifecycleState::Ready => crate::notify::notify_ready(),
            LifecycleState::Stopping => crate::notify::notify_stopping(),
            LifecycleState::Starting | LifecycleState::Stopped | LifecycleState::Failed => {}
        }
        Ok(from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [LifecycleState; 5] = [
        LifecycleState::Starting,
        LifecycleState::Ready,
        LifecycleState::Stopping,
        LifecycleState::Stopped,
        LifecycleState::Failed,
    ];

    const LEGAL: [(LifecycleState, LifecycleState); 5] = [
        (LifecycleState::Starting, LifecycleState::Ready),
        (LifecycleState::Starting, LifecycleState::Failed),
        (LifecycleState::Ready, LifecycleState::Stopping),
        (LifecycleState::Stopping, LifecycleState::Stopped),
        (LifecycleState::Stopping, LifecycleState::Failed),
    ];

    #[test]
    fn the_table_is_exactly_the_documented_set() {
        for from in ALL {
            for to in ALL {
                let expected = LEGAL.contains(&(from, to));
                assert_eq!(
                    is_valid_transition(from, to),
                    expected,
                    "{from} -> {to} must be {}",
                    if expected { "legal" } else { "illegal" }
                );
            }
        }
    }

    #[test]
    fn terminal_states_absorb() {
        for terminal in [LifecycleState::Stopped, LifecycleState::Failed] {
            for to in ALL {
                assert!(
                    !is_valid_transition(terminal, to),
                    "{terminal} -> {to} must be illegal"
                );
            }
        }
    }

    #[test]
    fn happy_path_walks_starting_ready_stopping_stopped() {
        let (lifecycle, rx) = Lifecycle::new();
        assert_eq!(*rx.borrow(), LifecycleState::Starting);

        for (to, expected_from) in [
            (LifecycleState::Ready, LifecycleState::Starting),
            (LifecycleState::Stopping, LifecycleState::Ready),
            (LifecycleState::Stopped, LifecycleState::Stopping),
        ] {
            let previous = lifecycle.transition(to);
            assert_eq!(previous, Ok(expected_from));
            assert_eq!(*rx.borrow(), to, "observers see {to}");
        }
    }

    #[test]
    fn boot_failure_goes_straight_to_failed() {
        let (lifecycle, rx) = Lifecycle::new();
        assert_eq!(
            lifecycle.transition(LifecycleState::Failed),
            Ok(LifecycleState::Starting)
        );
        assert_eq!(*rx.borrow(), LifecycleState::Failed);
    }

    #[test]
    fn forced_shutdown_fails_from_stopping() {
        let (lifecycle, rx) = Lifecycle::new();
        assert!(lifecycle.transition(LifecycleState::Ready).is_ok());
        assert!(lifecycle.transition(LifecycleState::Stopping).is_ok());
        assert_eq!(
            lifecycle.transition(LifecycleState::Failed),
            Ok(LifecycleState::Stopping)
        );
        assert_eq!(*rx.borrow(), LifecycleState::Failed);
    }

    #[test]
    fn invalid_transitions_leave_the_state_unchanged() {
        let (lifecycle, rx) = Lifecycle::new();

        // Starting -> Stopping is unrepresentable (no handlers during boot).
        assert_eq!(
            lifecycle.transition(LifecycleState::Stopping),
            Err(InvalidTransition {
                from: LifecycleState::Starting,
                to: LifecycleState::Stopping,
            })
        );
        assert_eq!(*rx.borrow(), LifecycleState::Starting);

        // A terminal state rejects everything, including itself.
        assert!(lifecycle.transition(LifecycleState::Failed).is_ok());
        assert_eq!(
            lifecycle.transition(LifecycleState::Failed),
            Err(InvalidTransition {
                from: LifecycleState::Failed,
                to: LifecycleState::Failed,
            })
        );
        assert_eq!(*rx.borrow(), LifecycleState::Failed);
    }

    #[tokio::test]
    async fn observers_can_await_transitions() {
        let (lifecycle, mut rx) = Lifecycle::new();
        assert!(lifecycle.transition(LifecycleState::Ready).is_ok());
        assert!(rx.changed().await.is_ok(), "change is observable");
        assert_eq!(*rx.borrow_and_update(), LifecycleState::Ready);
    }

    #[test]
    fn transitions_survive_all_observers_dropping() {
        let (lifecycle, rx) = Lifecycle::new();
        drop(rx);
        // send_if_modified must not error with zero receivers: the terminal
        // transitions in `serve` happen after the task manager (and its
        // observer) drained.
        assert!(lifecycle.transition(LifecycleState::Ready).is_ok());
        assert!(lifecycle.transition(LifecycleState::Stopping).is_ok());
        assert!(lifecycle.transition(LifecycleState::Stopped).is_ok());
    }
}
