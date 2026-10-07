//! What the studio does when the computer shuts down
//! (D-2026-10-03-power-off-standby-3): each screen gets the choice its user
//! made for it (`app::standby::at_shutdown` in the core), and nothing else
//! reaches any screen from then on.
//!
//! - Linux: [`watch_shutdowns`] holds a *delay* inhibitor lock of logind on
//!   the bus at the address the composition root gives (the system bus; a
//!   test's private one) for as long as the app runs. On
//!   `PrepareForShutdown(true)` it runs [`shut_down`] with logind's delay
//!   less [`MARGIN`] as the deadline, then closes the lock's descriptor, so
//!   the shutdown goes on; on `false` (cancelled) the final state ends, the
//!   lock is taken again and the live screen comes back as at the app's
//!   start. The final state starts with the announcement, before the delay
//!   is read. Without a bus or logind it says so ([`DiagCode`]) and each
//!   screen keeps its plan B.
//! - Windows: `RunEvent::Exit` while the session ends
//!   ([`bezel_power::session_ending`], `false` elsewhere) runs the same
//!   [`shut_down`], waiting for it, [`SESSION_END`] at most ([`at_exit`]);
//!   quitting the app applies nothing.
//!
//! [`shut_down`] first puts the session in its final state: no frame, no
//! reconnection, no link lent or opened, no storage operation
//! ([`Backend::enter_final_state`]). The running storage job is cancelled
//! and waited for; then the catalog is read again (a choice made by the CLI
//! while the studio runs counts) and each rev C screen whose choice is not
//! `keep` gets it: the live one through its open link, the other awake ones
//! opened for it; one asleep (its system off the bus) is left as it is,
//! never woken. The actions run on a thread of their own, so a screen that
//! hangs does not hold the shutdown past its deadline.

#[cfg(test)]
pub(crate) mod tests;

use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use bezel_core::app::discover_screens;
use bezel_core::app::standby::{Applied, at_shutdown};
use bezel_core::domain::archive::{Catalog, ScreenKey};
use bezel_core::domain::device::DeviceModel;
use bezel_core::domain::discovery::{Screen, ScreenState};
use bezel_core::domain::standby::{Standby, supports};
use bezel_core::ports::ScreenLink;
use bezel_power::{BusAddress, Inhibitor, Logind, PowerError, Shutdown};

use crate::backend::Backend;
use crate::clock;
use crate::commands::Shared;
use crate::diag::{self, DiagCode};
use crate::studio::ForShutdown;

/// Why the studio delays shutdowns, as `systemd-inhibit --list` shows it.
pub const REASON: &str = "Bezel applies each screen's choice for when the computer shuts down";

/// The actions end this long before logind stops waiting for the lock
/// (`InhibitDelayMaxUSec`), so that the lock is released in time.
pub const MARGIN: Duration = Duration::from_millis(500);

/// logind's own default delay, assumed when it cannot be read.
pub const DEFAULT_DELAY: Duration = Duration::from_secs(5);

/// How long the actions may take when Windows ends the session: it ends
/// the process soon after.
pub const SESSION_END: Duration = Duration::from_secs(4);

/// How often the shutdown looks for the live link while it is out.
const LINK_POLL: Duration = Duration::from_millis(10);

/// The longest single wait for logind's next announcement.
const ANNOUNCEMENT_WAIT: Duration = Duration::from_secs(3600);

/// How the app ends, as `RunEvent::Exit` tells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// The session is ending: Windows is shutting down, restarting or
    /// signing out.
    SessionEnding,
    /// The app quit (the tray, the window): nothing is applied.
    Quit,
}

impl Exit {
    /// The exit, given whether the session is ending
    /// ([`bezel_power::session_ending`]).
    pub fn of(session_ending: bool) -> Self {
        if session_ending {
            Exit::SessionEnding
        } else {
            Exit::Quit
        }
    }
}

/// How the actions of a shutdown ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    /// Every choice that could be applied was.
    Done,
    /// The deadline came first (a screen hung, or a job did not stop).
    Deadline,
    /// Their thread did not start: nothing was applied.
    NotStarted,
}

/// What `RunEvent::Exit` does: on a session end, [`shut_down`] with
/// [`SESSION_END`] as the deadline, waited for; on a quit, nothing. `None`
/// when nothing was applied (a quit, or the app has no backend yet).
pub fn at_exit(backend: Option<&Shared>, exit: Exit) -> Option<Ending> {
    match (backend, exit) {
        (Some(backend), Exit::SessionEnding) => {
            Some(shut_down(backend, Instant::now() + SESSION_END))
        }
        _ => None,
    }
}

/// Applies each screen's choice as the computer shuts down: the session's
/// final state first (a caller may have entered it already: entering it
/// again changes nothing), then the actions ([`Backend::apply_choices`]) on
/// a thread of their own. Returns once they are done, or at `deadline`
/// (whatever is left goes on, unwaited for). The final state stays until
/// [`Backend::leave_final_state`].
pub fn shut_down(backend: &Shared, deadline: Instant) -> Ending {
    backend.enter_final_state();
    let (done, finished) = mpsc::channel();
    let worker = Arc::clone(backend);
    let spawned = std::thread::Builder::new()
        .name("bezel-shutdown".into())
        .spawn(move || {
            worker.apply_choices(deadline);
            let _ = done.send(());
        });
    if spawned.is_err() {
        diag::report(DiagCode::ShutdownNotApplied);
        return Ending::NotStarted;
    }
    match finished.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok(()) => Ending::Done,
        Err(_) => {
            diag::report(DiagCode::ShutdownDeadline);
            Ending::Deadline
        }
    }
}

/// Watches logind on the bus at `address`, on a thread of its own for the
/// life of the app ([`watch`]). What stops it is said ([`DiagCode`]); off
/// Linux there is no logind and nothing is said.
pub fn watch_shutdowns(backend: Shared, address: BusAddress) {
    let spawned = std::thread::Builder::new()
        .name("bezel-power".into())
        .spawn(move || {
            if let Some(code) = watch(&backend, &address) {
                diag::report(code);
            }
        });
    if spawned.is_err() {
        diag::report(DiagCode::ShutdownWatchNotStarted);
    }
}

/// Holds logind's delay lock and answers its announcements until the bus
/// is lost: why it stopped, `None` off Linux.
fn watch(backend: &Shared, address: &BusAddress) -> Option<DiagCode> {
    let (logind, held) = match hold(address) {
        Ok(held) => held,
        Err(code) => return code,
    };
    let mut lock = Some(held);
    let mut ending = false;
    loop {
        match logind.wait(ANNOUNCEMENT_WAIT) {
            Ok(None) => {}
            // A second announcement of the same shutdown: already applied.
            Ok(Some(Shutdown::Starting)) if ending => {}
            Ok(Some(Shutdown::Starting)) => {
                let arrived = Instant::now();
                ending = true;
                // Nothing reaches a screen from the announcement on, also
                // while logind's delay is read (a bus busy at shutdown can
                // take seconds to answer).
                backend.enter_final_state();
                shut_down(backend, deadline_after(arrived, &logind));
                // Closing the descriptor lets the shutdown go on. Released,
                // not dropped: off Linux an `Inhibitor` has nothing to close
                // (and `drop` of it is `clippy::drop_non_drop` there).
                if let Some(held) = lock.take() {
                    held.release();
                }
            }
            Ok(Some(Shutdown::Cancelled)) => {
                let resume = std::mem::take(&mut ending);
                if resume {
                    backend.leave_final_state();
                }
                if lock.is_none() {
                    lock = logind
                        .inhibit(REASON)
                        .inspect_err(|e| say(unwatched(e)))
                        .ok();
                }
                if resume {
                    backend.restore_live(clock::now());
                }
            }
            Err(error) => return unwatched(&error),
        }
    }
}

/// The deadline of the actions of a shutdown announced at `arrived`:
/// logind's delay (`InhibitDelayMaxUSec`, [`DEFAULT_DELAY`] when it cannot
/// be read) less [`MARGIN`], counted from the announcement, as logind
/// counts it, not from the answer to reading the delay.
fn deadline_after(arrived: Instant, logind: &Logind) -> Instant {
    let delay = logind.delay_max().unwrap_or_else(|_| {
        diag::report(DiagCode::ShutdownDelayNotRead);
        DEFAULT_DELAY
    });
    arrived + delay.saturating_sub(MARGIN)
}

/// The connection to logind at `address` and its delay lock, taken; what
/// to say when there is none.
fn hold(address: &BusAddress) -> Result<(Logind, Inhibitor), Option<DiagCode>> {
    let logind = Logind::connect(address).map_err(|e| unwatched(&e))?;
    let lock = logind.inhibit(REASON).map_err(|e| unwatched(&e))?;
    Ok((logind, lock))
}

/// What to say when logind cannot be watched because of `error`: each
/// screen then keeps its plan B. Nothing off Linux, where there is no
/// logind.
fn unwatched(error: &PowerError) -> Option<DiagCode> {
    match error {
        PowerError::NoBus { .. } => Some(DiagCode::NoSystemBus),
        PowerError::NoLogind => Some(DiagCode::NoLogind),
        PowerError::Call { .. } => Some(DiagCode::ShutdownDelayRefused),
        PowerError::Disconnected => Some(DiagCode::SystemBusLost),
        PowerError::Unsupported => None,
    }
}

/// Says `code`, when there is one.
fn say(code: Option<DiagCode>) {
    if let Some(code) = code {
        diag::report(code);
    }
}

/// What the studio says of the action of a shutdown on one screen
/// (`None`: nothing went wrong): a failure, or an album the screen restarted
/// into whose record was not updated (review W2 of iteration 3), which is
/// not a failed choice.
fn said_of(applied: &bezel_core::Result<Applied>) -> Option<DiagCode> {
    match applied {
        Ok(Applied::AlbumNotRecorded(_)) => Some(DiagCode::ShutdownAlbumNotRecorded),
        Ok(_) => None,
        Err(_) => Some(DiagCode::ShutdownChoiceFailed),
    }
}

/// The choice recorded in `catalog` for a screen of `model` (the studio
/// knows its screens by model: [`ScreenKey::new`]); `keep` without one.
fn choice_for(catalog: &Catalog, model: &DeviceModel) -> Standby {
    catalog
        .screen(&ScreenKey::new(model.id))
        .map(|record| record.standby.clone())
        .unwrap_or_default()
}

/// Whether the shutdown opens `screen` to apply its choice: an awake rev C
/// screen (one asleep is never woken) of a known model whose choice is not
/// `keep`.
fn worth_opening(screen: &Screen, catalog: &Catalog) -> bool {
    screen.state() == ScreenState::Awake
        && screen
            .model()
            .is_some_and(|model| supports(model) && choice_for(catalog, model) != Standby::Keep)
}

impl Backend {
    /// The final state of a shutdown starts (D-2026-10-03-power-off-standby-3):
    /// no operation gets the screens and the running job is asked to stop
    /// ([`crate::storage::StorageState::enter_final_state`]), and the
    /// session draws no frame, lends no link, connects nothing again and
    /// puts no screen live ([`crate::studio::Studio::enter_final_state`]).
    pub fn enter_final_state(&self) {
        self.storage.enter_final_state();
        self.studio().enter_final_state();
    }

    /// The shutdown was cancelled: the final state ends. The caller shows
    /// the theme live again ([`Self::restore_live`]).
    pub fn leave_final_state(&self) {
        let unwanted = self.studio().leave_final_state();
        drop(unwanted);
        self.storage.leave_final_state();
    }

    /// The actions of a shutdown, in the final state, until `deadline`: the
    /// running job stopped and waited for, then the catalog read again and
    /// each choice applied (the live screen through its link, the other
    /// awake rev C screens opened for it). A job that does not stop in time
    /// leaves every screen as it is.
    fn apply_choices(&self, deadline: Instant) {
        if !self.storage.wait_until_idle(deadline) {
            diag::report(DiagCode::ShutdownJobNotStopped);
            return;
        }
        let mut live_links = Vec::new();
        while let Some(link) = self.live_link_for_shutdown(deadline) {
            live_links.push(link);
        }
        let catalog = match self.storage.archive().load() {
            Ok(catalog) => catalog,
            Err(_) => {
                diag::report(DiagCode::ShutdownCatalogNotRead);
                return;
            }
        };
        let had_live = !live_links.is_empty();
        for mut link in live_links {
            if Instant::now() < deadline {
                self.apply(link.as_mut());
            }
        }
        let screens = discover_screens(self.bus.as_ref()).unwrap_or_else(|_| {
            diag::report(DiagCode::ShutdownScreensNotListed);
            Vec::new()
        });
        for screen in screens {
            if Instant::now() >= deadline {
                break;
            }
            let live = had_live
                && screen
                    .address()
                    .is_some_and(|address| self.studio().is_live(&address.0));
            if live || !worth_opening(&screen, &catalog) {
                continue;
            }
            match self.connector.connect(&screen) {
                Ok(mut link) => self.apply(link.as_mut()),
                Err(_) => diag::report(DiagCode::ShutdownChoiceFailed),
            }
        }
    }

    /// Applies the choice the catalog records for the screen behind `link`
    /// (keyed by its model, [`ScreenKey::new`]), which the core reads again
    /// there ([`at_shutdown`] takes no choice from the studio); `keep` sends
    /// nothing. What went wrong is said ([`said_of`]). The catalog is locked
    /// only while it is read or saved, so a screen that hangs holds no other
    /// command ([`crate::storage::ArchivePerCall`]).
    fn apply(&self, link: &mut dyn ScreenLink) {
        let key = ScreenKey::new(link.identity().model.id);
        let applied = at_shutdown(link, &mut self.storage.archive_per_call(), &key);
        if let Some(code) = said_of(&applied) {
            diag::report(code);
        }
    }

    /// The live link, once it is back from showing a frame, a storage job
    /// or an attempt to connect it again; `None` without one, or at
    /// `deadline`.
    fn live_link_for_shutdown(&self, deadline: Instant) -> Option<Box<dyn ScreenLink>> {
        loop {
            match self.studio().take_for_shutdown() {
                ForShutdown::Link(link) => return Some(link),
                ForShutdown::Nothing => return None,
                ForShutdown::Out => {}
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            std::thread::sleep(LINK_POLL.min(deadline - now));
        }
    }
}
