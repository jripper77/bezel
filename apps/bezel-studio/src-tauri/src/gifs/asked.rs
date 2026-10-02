//! The proof that the user asked (D-2026-10-01-gif-sticker-search-3):
//! nothing reaches KLIPY at start or without a user action, by
//! construction. Making a GIF source, and every [`super::Gifs`] operation
//! that makes or uses one (search, preview, collect), takes a
//! [`UserAsked`], and the only way to get one outside this module is
//! [`UserAsked::of`], from the [`Request`] of a command the window invoked.
//!
//! Tauri makes a [`Request`] only while it dispatches an invocation to a
//! command (its fields are private, it has no constructor, and the
//! `InvokeMessage` it is read from is made only by Tauri), so the app's
//! setup, a thread it starts, or a call to a command function from Rust
//! has none to give: a warm-up search at start does not compile.

use tauri::ipc::Request;

/// A user action reached the backend: a command the window invoked. Not
/// `Clone`, not `Default`, no public field.
#[derive(Debug)]
pub struct UserAsked {
    _invoked: (),
}

impl UserAsked {
    /// The proof carried by `_request`, the invocation of the command that
    /// is running.
    #[must_use]
    pub fn of(_request: &Request<'_>) -> Self {
        Self { _invoked: () }
    }

    /// A user action as a test of the GIF state plays it. Built for tests
    /// only, and visible only inside `gifs`: neither the app's setup nor its
    /// commands can name it, in a test build or not.
    #[cfg(test)]
    pub(in crate::gifs) const fn in_a_test() -> Self {
        Self { _invoked: () }
    }
}
