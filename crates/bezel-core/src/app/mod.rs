//! Use cases, generic over the driven ports.

mod runtime;
mod screens;
pub mod storage;

pub use runtime::ThemeRuntime;
pub use screens::{choose_screen, discover_screens, open_screen};
