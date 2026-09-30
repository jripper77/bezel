//! Running the external tools: always an argument vector (never a shell),
//! stdin closed, stdout and stderr piped and drained by helper threads so a
//! chatty child never blocks on a full pipe.

use std::ffi::{OsStr, OsString};
use std::io::{self, Read};
use std::path::Path;
use std::process::{Child, ChildStderr, Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// How often a waiting caller looks at the child again.
pub(crate) const POLL: Duration = Duration::from_millis(20);
/// Bytes of stderr kept for error messages (the end of it).
const STDERR_KEEP: usize = 8 * 1024;
/// Characters of stderr quoted in an error message.
const STDERR_QUOTE: usize = 400;

/// What a finished tool printed.
#[derive(Debug)]
pub(crate) struct Output {
    /// Whether it exited with status 0.
    pub(crate) success: bool,
    /// Everything it wrote to stdout (lossy UTF-8).
    pub(crate) stdout: String,
    /// The end of what it wrote to stderr.
    pub(crate) stderr: String,
}

/// `path` as an ffmpeg URL of the `file` protocol, so that a name starting
/// with `-` or containing `:` is never read as an option or a protocol.
pub(crate) fn file_url(path: &Path) -> OsString {
    let mut url = OsString::from("file:");
    url.push(path.as_os_str());
    url
}

/// Starts `program` with `args`: stdin closed, stdout and stderr piped, and
/// on Windows without flashing a console window.
pub(crate) fn spawn<S: AsRef<OsStr>>(program: &Path, args: &[S]) -> io::Result<Child> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_console(&mut command);
    command.spawn()
}

#[cfg(windows)]
fn hide_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    /// `CREATE_NO_WINDOW` from the Win32 process creation flags.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_: &mut Command) {}

/// Runs `program` to completion and collects its output; kills it and fails
/// with `TimedOut` after `timeout`.
pub(crate) fn capture<S: AsRef<OsStr>>(
    program: &Path,
    args: &[S],
    timeout: Duration,
) -> io::Result<Output> {
    let mut child = spawn(program, args)?;
    let stdout = child.stdout.take().map(|mut pipe| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            // Draining only keeps the child from blocking on a full pipe; a
            // read error just means less text for the report, and the exit
            // status decides success.
            let _ = pipe.read_to_end(&mut bytes);
            String::from_utf8_lossy(&bytes).into_owned()
        })
    });
    let stderr = StderrTail::collect(child.stderr.take());
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            stop(&mut child);
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "{} did not finish in {} s",
                    program.display(),
                    timeout.as_secs()
                ),
            ));
        }
        thread::sleep(POLL);
    };
    Ok(Output {
        success: status.success(),
        stdout: stdout.and_then(|h| h.join().ok()).unwrap_or_default(),
        stderr: stderr.finish(),
    })
}

/// Kills `child` and reaps it. Errors are ignored: it may have exited already.
pub(crate) fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// The end of a child's stderr, read by a helper thread.
pub(crate) struct StderrTail(Option<JoinHandle<String>>);

impl StderrTail {
    /// Starts draining `pipe` (nothing to drain when `None`).
    pub(crate) fn collect(pipe: Option<ChildStderr>) -> Self {
        Self(pipe.map(|mut pipe| {
            thread::spawn(move || {
                let mut kept = Vec::new();
                let mut chunk = [0u8; 4096];
                while let Ok(n) = pipe.read(&mut chunk) {
                    if n == 0 {
                        break;
                    }
                    kept.extend_from_slice(&chunk[..n]);
                    if kept.len() > STDERR_KEEP {
                        kept.drain(..kept.len() - STDERR_KEEP);
                    }
                }
                String::from_utf8_lossy(&kept).into_owned()
            })
        }))
    }

    /// Waits for the pipe to close and returns what was kept. Call it only
    /// once the child has exited.
    pub(crate) fn finish(self) -> String {
        self.0.and_then(|h| h.join().ok()).unwrap_or_default()
    }
}

/// The last lines of `stderr`, trimmed for an error message.
pub(crate) fn quote(stderr: &str) -> String {
    let text = stderr.trim();
    if text.is_empty() {
        return "no error message".to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let start = chars.len().saturating_sub(STDERR_QUOTE);
    let tail: String = chars[start..].iter().collect();
    tail.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_and_quotes() {
        assert_eq!(file_url(Path::new("-y.mp4")), OsString::from("file:-y.mp4"));
        assert_eq!(quote("  \n"), "no error message");
        assert_eq!(quote("a\n\n b \n"), "a | b");
        let long = "x".repeat(1000);
        assert_eq!(quote(&long).len(), STDERR_QUOTE);
    }

    #[test]
    fn a_missing_program_is_an_io_error() {
        let err = capture(Path::new("/nonexistent/bezel-ffmpeg"), &["-version"], POLL).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[cfg(unix)]
    #[test]
    fn capture_collects_output_and_times_out() {
        let fakes = crate::fakes::dir();
        let out = capture(
            &fakes.join("ready/ffmpeg"),
            &["-version"],
            Duration::from_secs(10),
        )
        .unwrap();
        assert!(out.success);
        assert!(out.stdout.starts_with("ffmpeg version 9.9-fake"));
        let started = Instant::now();
        let err = capture(&fakes.join("sleeper"), &["x"], Duration::from_millis(100)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(10));
        let noisy = capture(&fakes.join("noisy"), &["x"], Duration::from_secs(10)).unwrap();
        assert!(!noisy.success);
        assert_eq!(noisy.stderr.len(), STDERR_KEEP);
    }
}
