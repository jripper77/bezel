//! Independent serial delivery workers. The session lends each link once,
//! so there is at most one outstanding frame per display and no stale queue.
use std::collections::BTreeMap;
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::diag::{self, DiagCode};
use crate::studio::Delivery;
use bezel_core::{BezelError, Result};

pub(crate) type Completion = Arc<dyn Fn(Delivery, &Result<()>) + Send + Sync>;
struct Worker {
    sender: mpsc::SyncSender<Delivery>,
    thread: JoinHandle<()>,
}

pub(crate) struct Workers {
    workers: BTreeMap<String, Worker>,
    completed: Completion,
}

impl Workers {
    pub(crate) fn new(completed: Completion) -> Self {
        Self {
            workers: BTreeMap::new(),
            completed,
        }
    }

    pub(crate) fn submit(
        &mut self,
        mut delivery: Delivery,
    ) -> std::result::Result<(), (Box<Delivery>, BezelError)> {
        self.workers
            .retain(|_, worker| !worker.thread.is_finished());
        let key = delivery.key().to_owned();
        if let Some(worker) = self.workers.get(&key) {
            match worker.sender.try_send(delivery) {
                Ok(()) => return Ok(()),
                Err(mpsc::TrySendError::Disconnected(returned)) => delivery = returned,
                Err(mpsc::TrySendError::Full(returned)) => {
                    return Err((
                        Box::new(returned),
                        BezelError::Transport("a frame is already pending for this display".into()),
                    ));
                }
            }
            self.workers.remove(&key);
        }
        let (sender, receiver) = mpsc::sync_channel::<Delivery>(1);
        let completed = Arc::clone(&self.completed);
        let thread = match std::thread::Builder::new()
            .name(format!("bezel-display-{key}"))
            .spawn(move || {
                // Idle workers expire; closing the session drops the sender.
                while let Ok(mut delivery) = receiver.recv_timeout(Duration::from_secs(30)) {
                    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        delivery.present()
                    }))
                    .unwrap_or_else(|_| {
                        Err(BezelError::Transport(
                            "screen delivery worker panicked".into(),
                        ))
                    });
                    completed(delivery, &outcome);
                    if outcome.is_err() {
                        diag::report(DiagCode::LiveFrameFailed);
                    }
                }
            }) {
            Ok(thread) => thread,
            Err(error) => {
                return Err((
                    Box::new(delivery),
                    BezelError::Transport(format!(
                        "could not start screen delivery worker: {error}"
                    )),
                ));
            }
        };
        match sender.try_send(delivery) {
            Ok(()) => {
                self.workers.insert(key, Worker { sender, thread });
                Ok(())
            }
            Err(
                mpsc::TrySendError::Disconnected(returned) | mpsc::TrySendError::Full(returned),
            ) => Err((
                Box::new(returned),
                BezelError::Transport("screen delivery worker stopped".into()),
            )),
        }
    }
}
