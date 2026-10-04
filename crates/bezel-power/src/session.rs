//! Whether the user's session is ending (Windows).

/// Whether Windows is ending the session (shutting down, restarting or
/// signing out), as `GetSystemMetrics(SM_SHUTTINGDOWN)` says; the vendor's
/// app asks the same when its process is told to end. Always `false` off
/// Windows, so that the studio calls it without a `cfg`
/// (D-2026-10-03-power-off-standby-3 (2) and -7).
pub fn session_ending() -> bool {
    imp::session_ending()
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_SHUTTINGDOWN};

    #[allow(
        unsafe_code,
        reason = "Win32 FFI: only GetSystemMetrics tells that the session is ending"
    )]
    pub(super) fn session_ending() -> bool {
        // SAFETY: GetSystemMetrics takes one integer by value, reads system
        // state, touches no memory of ours and cannot fail (0 = no).
        let shutting_down = unsafe { GetSystemMetrics(SM_SHUTTINGDOWN) };
        shutting_down != 0
    }
}

#[cfg(not(windows))]
mod imp {
    pub(super) fn session_ending() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::session_ending;

    #[test]
    fn the_session_is_not_ending_while_the_tests_run() {
        assert!(!session_ending());
    }
}
