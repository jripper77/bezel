//! The proof that the user asked (D-2026-10-01-gif-sticker-search-3), on
//! the GIF state's side: making a GIF source, and every [`super::Gifs`]
//! operation that makes or uses one (search, preview, collect), takes a
//! [`UserAsked`], and the only way to get one outside this module is
//! [`UserAsked::of`], from the [`Request`] of a command Tauri is running.
//!
//! Tauri makes a [`Request`] only while it dispatches an invocation to a
//! command (its fields are private, it has no constructor, and the
//! `InvokeMessage` it is read from is made only by Tauri). So the app's
//! setup, a thread it starts, or a call to a command function from Rust
//! has none to give: a warm-up through the GIF state, its source factory
//! or a command function does not compile.
//!
//! What the type does not stop is code that makes Tauri dispatch an
//! invocation the window never sent, which then carries a real
//! [`Request`] (an `InvokeRequest` handed to `WebviewWindow::on_message`
//! with `AppHandle::invoke_key`, or a script run in the window that calls
//! `invoke` or presses Search), nor a second `KlipyClient` made and asked
//! apart from the GIF state. The source guard,
//! `tests::nothing_in_the_app_forges_an_invocation` in `lib.rs`
//! (D-2026-10-01-gif-sticker-search-10), reads the studio's production code
//! as a syntax tree and refuses, by identifier (raw names and the tokens of
//! macro calls included), the APIs that do so: `eval`,
//! `eval_with_callback`, `with_webview`, `on_message`, `invoke_key`,
//! `InvokeRequest`, `initialization_script` and `js_init_script`; and it
//! accepts one `KlipyClient::new`, in the source factory (`klipy_source`).
//! Code written to get past it otherwise (generated code, another crate
//! doing the forging) is left to code review.

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
