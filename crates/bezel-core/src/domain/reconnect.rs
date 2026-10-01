//! A live screen whose link fails mid-run comes back by itself (T-7.11,
//! after D-2026-09-30-release-polish-13): the link is dropped and the
//! screen connected again after a wait, a few times at most. Connecting a
//! rev C screen that hung restarts it through its MCU; one that comes back
//! under another device name is found again by identity
//! ([`super::discovery::find_again`]). The waiting is the caller's: a stop
//! asked meanwhile ends it at once.

use std::time::Duration;

use super::error::BezelError;

/// The waits before each attempt to reconnect: 2 s, 5 s, then 10 s. After
/// the last attempt failed, live mode stops with the error that stopped it.
pub const RECONNECT_WAITS: [Duration; 3] = [
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
];

/// Whether a live screen that failed with `error` may come back by
/// connecting it again: its link broke or stalled, it did not answer in
/// time, or it is not on the bus yet (restarting). Anything else (a port
/// another program holds, a denied port, a frame of the wrong size) would
/// fail again the same way.
pub fn worth_reconnecting(error: &BezelError) -> bool {
    matches!(
        error,
        BezelError::Hung(_)
            | BezelError::Transport(_)
            | BezelError::Timeout(_)
            | BezelError::ScreenNotFound(_)
    )
}

/// The attempts to reconnect one failed link: how long to wait before each
/// ([`RECONNECT_WAITS`]), and when to give up.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reconnect {
    made: usize,
}

impl Reconnect {
    /// No attempt made yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Attempts there are in all.
    pub const fn attempts() -> usize {
        RECONNECT_WAITS.len()
    }

    /// The attempt under way or last made (1 for the first; 0 before any).
    pub fn attempt(&self) -> usize {
        self.made
    }

    /// Counts the next attempt and says how long to wait before it; `None`
    /// once every attempt was made: give up.
    pub fn next_wait(&mut self) -> Option<Duration> {
        let wait = RECONNECT_WAITS.get(self.made).copied()?;
        self.made += 1;
        Some(wait)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_attempts_after_two_five_and_ten_seconds() {
        let mut r = Reconnect::new();
        assert_eq!((r.attempt(), Reconnect::attempts()), (0, 3));
        let waits: Vec<u64> = std::iter::from_fn(|| r.next_wait())
            .map(|w| w.as_secs())
            .collect();
        assert_eq!(waits, [2, 5, 10]);
        assert_eq!(r.attempt(), 3);
        assert_eq!(r.next_wait(), None, "then it gives up");
    }

    #[test]
    fn only_link_failures_are_worth_a_reconnect() {
        for e in [
            BezelError::Hung("stalled".into()),
            BezelError::Transport("broken pipe".into()),
            BezelError::Timeout("the screen".into()),
            BezelError::ScreenNotFound("/dev/ttyACM1".into()),
        ] {
            assert!(worth_reconnecting(&e), "{e}");
        }
        for e in [
            BezelError::InUse {
                address: "/dev/ttyACM1".into(),
                holders: vec!["another program".into()],
            },
            BezelError::AccessDenied {
                address: "/dev/ttyACM1".into(),
                reason: "denied".into(),
            },
            BezelError::InvalidInput("frame size".into()),
            BezelError::Unsupported("x".into()),
        ] {
            assert!(!worth_reconnecting(&e), "{e}");
        }
    }
}
