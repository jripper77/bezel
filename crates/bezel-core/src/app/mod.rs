//! Use cases, generic over the driven ports.

mod runtime;
mod screens;
pub mod storage;

pub use runtime::{HostVideo, MissingVideo, ThemeRuntime, VideoState, device_video_name};
pub use screens::{
    choose_screen, discover_devices, discover_screens, leave_desktop_mode, open_screen,
};
