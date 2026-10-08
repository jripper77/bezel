//! Safe recovery when the tray loop resumes after a long pause.
use bezel_core::domain::discovery::Screen;
use std::time::{Duration, SystemTime};

pub struct PauseWatch {
    previous: SystemTime,
}
impl PauseWatch {
    pub fn new(now: SystemTime) -> Self {
        Self { previous: now }
    }
    /// The tray polls every 100 ms. A ten-second gap requests one recovery.
    /// This also covers a stalled event loop; backward clock changes do not fire.
    pub fn poll(&mut self, now: SystemTime) -> bool {
        let gap = now.duration_since(self.previous).unwrap_or_default();
        self.previous = now;
        gap >= Duration::from_secs(10)
    }
}

/// Both the wake MCU and display endpoint identify the same physical screen.
pub fn physical_key(key: &str, screens: &[Screen]) -> String {
    screens
        .iter()
        .find(|screen| {
            screen.display.as_ref().is_some_and(|e| e.address.0 == key)
                || screen.wake.as_ref().is_some_and(|e| e.address.0 == key)
        })
        .and_then(Screen::address)
        .map_or_else(|| key.to_owned(), |a| a.0.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pause_requests_one_recovery_and_ignores_backward_clock_changes() {
        let start = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let mut watch = PauseWatch::new(start);
        assert!(!watch.poll(start + Duration::from_millis(100)));
        assert!(watch.poll(start + Duration::from_secs(1800)));
        assert!(!watch.poll(start + Duration::from_secs(1801)));
        assert!(!watch.poll(start));
    }
    #[test]
    fn wake_and_display_keys_share_one_worker() {
        let screens =
            bezel_core::app::discover_screens(&bezel_devices::FakeBus::turing_88()).unwrap();
        let screen = &screens[0];
        let display = &screen.display.as_ref().unwrap().address.0;
        let wake = &screen.wake.as_ref().unwrap().address.0;
        assert_eq!(
            physical_key(display, &screens),
            physical_key(wake, &screens)
        );
        assert_eq!(physical_key("missing", &screens), "missing");
    }
}
