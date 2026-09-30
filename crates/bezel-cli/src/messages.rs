//! What a command tells the user on stderr while it works: confirmation
//! summaries, warnings and progress.

use std::fmt;
use std::io::{self, Write};

/// The user's messages, written with `write!` and `writeln!` (which return
/// nothing here). A message that cannot be written is never dropped
/// silently: the first failure is kept, later messages are skipped, and
/// [`Messages::check`] turns it into the command's error. A command checks
/// before it changes anything the user must read about first, and again
/// when it ends.
pub(crate) struct Messages<'a> {
    out: &'a mut dyn Write,
    failure: Option<io::Error>,
}

impl<'a> Messages<'a> {
    /// Messages written to `out` (stderr, or a buffer in tests).
    pub(crate) fn new(out: &'a mut dyn Write) -> Self {
        Self { out, failure: None }
    }

    /// Writes `args`: what `write!` and `writeln!` call.
    pub(crate) fn write_fmt(&mut self, args: fmt::Arguments<'_>) {
        if self.failure.is_none() {
            self.failure = self.out.write_fmt(args).err();
        }
    }

    /// Pushes out what was written (a line redrawn in place).
    pub(crate) fn flush(&mut self) {
        if self.failure.is_none() {
            self.failure = self.out.flush().err();
        }
    }

    /// True once a message could not be written.
    pub(crate) fn failed(&self) -> bool {
        self.failure.is_some()
    }

    /// `Ok` while every message was written; otherwise the first failure,
    /// as the error that ends the command.
    pub(crate) fn check(&self) -> anyhow::Result<()> {
        match &self.failure {
            None => Ok(()),
            Some(e) => Err(anyhow::anyhow!(
                "could not write to the terminal (stderr): {e}"
            )),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A stderr that takes `room` bytes, then fails like a closed pipe.
    pub(crate) struct Closing {
        pub(crate) room: usize,
        pub(crate) taken: Vec<u8>,
        pub(crate) flushes: usize,
    }

    impl Closing {
        pub(crate) fn after(room: usize) -> Self {
            Self {
                room,
                taken: Vec::new(),
                flushes: 0,
            }
        }
    }

    impl Write for Closing {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let n = buf.len().min(self.room - self.taken.len());
            if n == 0 {
                return Err(io::Error::from(io::ErrorKind::BrokenPipe));
            }
            self.taken.extend_from_slice(&buf[..n]);
            Ok(n)
        }

        fn flush(&mut self) -> io::Result<()> {
            if self.taken.len() >= self.room {
                return Err(io::Error::from(io::ErrorKind::BrokenPipe));
            }
            self.flushes += 1;
            Ok(())
        }
    }

    #[test]
    fn messages_are_written_until_the_first_failure_which_is_kept() {
        let mut out = Vec::new();
        let mut log = Messages::new(&mut out);
        writeln!(log, "one {}", 1);
        write!(log, "two");
        log.flush();
        assert!(!log.failed());
        log.check().unwrap();
        assert_eq!(out, b"one 1\ntwo");

        let mut closing = Closing::after(4);
        let mut log = Messages::new(&mut closing);
        log.flush();
        writeln!(log, "first line");
        assert!(log.failed());
        writeln!(log, "skipped");
        log.flush();
        let err = log.check().unwrap_err().to_string();
        assert_eq!(err, "could not write to the terminal (stderr): broken pipe");
        assert_eq!(log.check().unwrap_err().to_string(), err, "kept");
        assert_eq!(
            (closing.taken.as_slice(), closing.flushes),
            (&b"firs"[..], 1)
        );

        let mut closed = Closing::after(0);
        let mut log = Messages::new(&mut closed);
        log.flush();
        assert!(log.check().is_err(), "a failed flush counts too");
    }
}
