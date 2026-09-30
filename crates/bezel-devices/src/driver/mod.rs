//! Drivers: each family's handshake and frame pipeline over a [`crate::wire::Wire`].

use std::time::Duration;

use bezel_core::domain::device::DeviceModel;
use bezel_core::domain::frame::Frame;
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::{BezelError, Result};

pub mod kipye_rev_d;
pub mod turing_rev_a;
pub mod turing_rev_c;
pub mod turing_usb;
pub mod wch;
pub mod weact;
pub mod xuanfang_rev_b;

/// Pauses between protocol steps. The fake used in tests does not sleep.
pub trait Pause: Send {
    /// Waits `d`.
    fn pause(&self, d: Duration);
}

/// Real time.
#[derive(Debug, Clone, Copy, Default)]
pub struct RealTime;

impl Pause for RealTime {
    fn pause(&self, d: Duration) {
        std::thread::sleep(d);
    }
}

/// A transport failure, as the domain names it.
pub(crate) fn io_err(e: std::io::Error) -> BezelError {
    BezelError::Transport(e.to_string())
}

/// Frames must be the size the screen shows in the current orientation.
pub(crate) fn check_frame_size(frame: &Frame, expected: Size) -> Result<()> {
    if frame.size() == expected {
        return Ok(());
    }
    Err(BezelError::InvalidInput(format!(
        "frame is {}x{}, the screen expects {}x{} in this orientation",
        frame.size().width,
        frame.size().height,
        expected.width,
        expected.height
    )))
}

/// [`check_frame_size`] for `model`'s panel in `orientation`.
pub(crate) fn check_frame(
    model: &DeviceModel,
    orientation: Orientation,
    frame: &Frame,
) -> Result<()> {
    check_frame_size(frame, model.panel.in_orientation(orientation))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::domain::frame::Rgba;

    #[test]
    fn frame_size_errors_are_invalid_input() {
        let frame = Frame::filled(Size::new(2, 3), Rgba::BLACK);
        assert!(check_frame_size(&frame, Size::new(2, 3)).is_ok());
        let err = check_frame_size(&frame, Size::new(3, 2)).unwrap_err();
        assert!(matches!(err, BezelError::InvalidInput(_)), "{err}");
        assert!(err.to_string().contains("2x3"), "{err}");
        RealTime.pause(Duration::ZERO);
        assert!(matches!(
            io_err(std::io::Error::other("x")),
            BezelError::Transport(_)
        ));
    }
}
