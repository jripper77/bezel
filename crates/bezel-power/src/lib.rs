//! Bezel power: the adapter that tells the studio the machine is shutting
//! down, so that each screen gets the choice its user made for it
//! (D-2026-10-03-power-off-standby-3).
//!
//! - Linux: [`Logind`] talks to systemd-logind on the bus at the
//!   [`BusAddress`] it is given (the system bus in production, a private
//!   one in tests), and to nobody else but the bus itself for `Hello` and
//!   one `AddMatch`. It takes a *delay* inhibitor lock for `shutdown`
//!   ([`Logind::inhibit`]), reads how long logind waits for such a lock
//!   ([`Logind::delay_max`]) and reports `PrepareForShutdown`
//!   ([`Logind::wait`]). Dropping the [`Inhibitor`] closes its descriptor,
//!   which lets the shutdown go on.
//! - Windows: [`session_ending`] says whether the session is ending
//!   (`GetSystemMetrics(SM_SHUTTINGDOWN)`). It is `false` everywhere else,
//!   so its callers need no `cfg` (D-2026-10-03-power-off-standby-7).
//! - Off Linux, [`Logind::connect`] is [`PowerError::Unsupported`].
//!
//! With the `fake` feature, on Linux, the `fake` module starts a private
//! `dbus-daemon`, a fake logind on it and a monitor of that bus, for the
//! tests of the crates that use this one.
//!
//! `unsafe` is denied; the one exception is the `GetSystemMetrics` call
//! (Windows only), allowed in the smallest scope with its `// SAFETY:` line.
#![deny(unsafe_code)]

#[cfg(all(target_os = "linux", any(test, feature = "fake")))]
pub mod fake;
mod logind;
mod session;

pub use logind::{BusAddress, Inhibitor, Logind, Shutdown};
pub use session::session_ending;

/// Why talking to logind failed. Never a panic: without a bus or without
/// logind the studio goes on with the plan B stored in each screen.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PowerError {
    /// Nothing answers as a D-Bus bus at the address.
    #[error("no D-Bus bus at {address}: {reason}")]
    NoBus {
        /// The address that was tried.
        address: String,
        /// What libdbus said.
        reason: String,
    },
    /// The bus answers, but logind is not on it.
    #[error("logind is not on the bus")]
    NoLogind,
    /// The connection to the bus was lost.
    #[error("the connection to the bus was lost")]
    Disconnected,
    /// A call was answered with an error, or with something else than
    /// logind's documented reply.
    #[error("{call} failed: {reason}")]
    Call {
        /// The D-Bus method called.
        call: &'static str,
        /// The error's name and message, or what the reply held instead.
        reason: String,
    },
    /// There is no logind on this platform.
    #[error("logind exists only on Linux")]
    Unsupported,
}
