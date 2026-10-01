//! Use cases, generic over the driven ports.

pub mod gifs;
pub mod manager;
mod runtime;
mod screens;
pub mod storage;

pub use crate::domain::media::device_video_name;
pub use runtime::{
    DEFAULT_SLOWEST_REFRESH, HOST_VIDEO_FPS, HostVideo, MissingVideo, ThemeRuntime, VideoState,
};
pub use screens::{
    choose_screen, connect_screen, discover_devices, discover_screens, leave_desktop_mode,
    open_screen, reopen_screen, restart_screen,
};
