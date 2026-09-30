//! Long operations (convert, upload, verify): progress reports and
//! cooperative cancellation.
//!
//! The core spawns no thread and reads no clock: an adapter doing a long
//! operation reports [`Progress`] through the [`Job`] it was handed and polls
//! the job's [`CancelToken`] between its own steps. The token is the only
//! thing another thread (a Ctrl+C handler, a Cancel button) touches.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::{BezelError, Result};

/// The step of a job a [`Progress`] report belongs to, and the unit of its
/// `done` / `total` counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobPhase {
    /// Converting a local media file into the screen's profile. Counters are
    /// milliseconds of media time written / the source's duration (`total` is
    /// 0 when the duration is unknown).
    Convert,
    /// Sending a file to the screen. Counters are bytes of the file the link
    /// has accepted / the file size (framing and padding bytes do not count).
    Upload,
    /// Checking the stored file against what was sent. Counters are checks:
    /// 0 / 1 when the check starts, 1 / 1 when it passed.
    Verify,
}

impl JobPhase {
    /// Stable machine name.
    pub const fn slug(self) -> &'static str {
        match self {
            JobPhase::Convert => "convert",
            JobPhase::Upload => "upload",
            JobPhase::Verify => "verify",
        }
    }
}

/// How far a job has come in one phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    /// Current phase (also the unit of the counters).
    pub phase: JobPhase,
    /// Units done.
    pub done: u64,
    /// Units in the phase; 0 when unknown.
    pub total: u64,
}

impl Progress {
    /// A report.
    pub const fn new(phase: JobPhase, done: u64, total: u64) -> Self {
        Self { phase, done, total }
    }

    /// Completed fraction of the phase in `0.0..=1.0`; `None` when the total
    /// is unknown.
    pub fn fraction(&self) -> Option<f64> {
        if self.total == 0 {
            return None;
        }
        Some((self.done.min(self.total) as f64) / (self.total as f64))
    }
}

/// Asks a running job to stop. Clones share one flag, so the clone kept by a
/// Ctrl+C handler or a UI button cancels the job holding the original.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// A token that has not been cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation. Idempotent; it cannot be undone.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// True once [`Self::cancel`] was called on this token or a clone.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// What a long operation receives from its caller: where to report
/// progress and the token to poll.
pub struct Job<'a> {
    cancel: &'a CancelToken,
    progress: &'a mut dyn FnMut(Progress),
}

impl fmt::Debug for Job<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Job")
            .field("cancelled", &self.cancel.is_cancelled())
            .finish_non_exhaustive()
    }
}

impl<'a> Job<'a> {
    /// A job reporting to `progress` and cancelled through `cancel`.
    pub fn new(cancel: &'a CancelToken, progress: &'a mut dyn FnMut(Progress)) -> Self {
        Self { cancel, progress }
    }

    /// Forwards a progress report to the caller.
    pub fn report(&mut self, progress: Progress) {
        (self.progress)(progress);
    }

    /// True once the caller asked to stop.
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// `Err(BezelError::Cancelled { partial: None })` once the caller asked to
    /// stop, for `job.checkpoint()?` between steps that leave nothing behind.
    pub fn checkpoint(&self) -> Result<()> {
        if self.is_cancelled() {
            return Err(BezelError::Cancelled { partial: None });
        }
        Ok(())
    }

    /// The token, for an adapter that must watch it from a helper thread of
    /// its own (clone it).
    pub fn token(&self) -> &CancelToken {
        self.cancel
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fraction_is_clamped_and_unknown_without_total() {
        assert_eq!(Progress::new(JobPhase::Upload, 5, 10).fraction(), Some(0.5));
        assert_eq!(
            Progress::new(JobPhase::Upload, 15, 10).fraction(),
            Some(1.0)
        );
        assert_eq!(Progress::new(JobPhase::Convert, 5, 0).fraction(), None);
        assert_eq!(JobPhase::Verify.slug(), "verify");
        assert_eq!(JobPhase::Convert.slug(), "convert");
        assert_eq!(JobPhase::Upload.slug(), "upload");
    }

    #[test]
    fn a_clone_cancels_the_job_and_reports_reach_the_caller() {
        let token = CancelToken::new();
        let remote = token.clone();
        let mut seen = Vec::new();
        let mut sink = |p: Progress| seen.push(p);
        let mut job = Job::new(&token, &mut sink);
        job.report(Progress::new(JobPhase::Upload, 1, 2));
        assert!(job.checkpoint().is_ok());
        assert!(format!("{job:?}").contains("cancelled: false"));
        remote.cancel();
        assert!(job.is_cancelled());
        assert!(job.token().is_cancelled());
        assert_eq!(
            job.checkpoint(),
            Err(BezelError::Cancelled { partial: None })
        );
        assert_eq!(seen, vec![Progress::new(JobPhase::Upload, 1, 2)]);
    }
}
