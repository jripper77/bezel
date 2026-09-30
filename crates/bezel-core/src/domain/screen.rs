//! A connected screen: what it is, and the values it accepts.

use super::device::DeviceModel;

/// Backlight level in percent (0..=100).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Brightness(u8);

impl Brightness {
    /// Full brightness.
    pub const MAX: Brightness = Brightness(100);

    /// A level; `None` above 100.
    pub const fn new(percent: u8) -> Option<Self> {
        if percent <= 100 {
            Some(Self(percent))
        } else {
            None
        }
    }

    /// The level in percent.
    pub const fn percent(self) -> u8 {
        self.0
    }

    /// The level scaled to `0..=max`, rounded to nearest.
    pub fn scaled(self, max: u16) -> u16 {
        ((u32::from(self.0) * u32::from(max) + 50) / 100) as u16
    }
}

/// Who answered the handshake.
#[derive(Debug, Clone, PartialEq)]
pub struct ScreenIdentity {
    /// The resolved catalog model.
    pub model: &'static DeviceModel,
    /// Firmware/handshake string, when the device reports one.
    pub firmware: Option<String>,
}

/// Confirms an operation that cannot be undone (deleting a file, formatting,
/// rebooting, flashing firmware). Only a human-facing adapter may say `Yes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirm {
    /// The user confirmed.
    Yes,
    /// Not confirmed.
    No,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_range_and_scaling() {
        assert!(Brightness::new(101).is_none());
        let b = Brightness::new(25).unwrap();
        assert_eq!(b.percent(), 25);
        assert_eq!(b.scaled(255), 64);
        assert_eq!(Brightness::MAX.scaled(255), 255);
        assert_eq!(Brightness::new(0).unwrap().scaled(255), 0);
        assert_eq!(Brightness::new(50).unwrap().scaled(102), 51);
    }
}
