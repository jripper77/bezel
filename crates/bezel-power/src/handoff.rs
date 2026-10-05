//! Cooperative ownership between the Windows light runtime and Studio.
//! OS locks release on process exit, including crashes; no PID is killed.
use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

/// A writable per-user coordination folder, keyed by the installation path.
pub fn runtime_directory(executable: &Path) -> io::Result<std::path::PathBuf> {
    use std::hash::{DefaultHasher, Hasher};
    let installation = executable
        .parent()
        .ok_or_else(|| io::Error::other("no installation directory"))?
        .canonicalize()?;
    let mut hasher = DefaultHasher::new();
    hasher.write(installation.to_string_lossy().to_lowercase().as_bytes());
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "no user application data directory",
            )
        })?;
    let directory = std::path::PathBuf::from(base)
        .join("io.github.slipalison.bezel/runtime")
        .join(format!("{:016x}", hasher.finish()));
    std::fs::create_dir_all(&directory)?;
    Ok(directory)
}

fn claim(directory: &Path, name: &str) -> io::Result<Option<File>> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join(name))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

/// Keep this handle for the lifetime of the light tray process.
pub fn light_instance(directory: &Path) -> io::Result<Option<File>> {
    claim(directory, "bezel-light-instance.lock")
}

/// Keep this handle until every light screen connection has been dropped.
pub fn light_screen(directory: &Path) -> io::Result<Option<File>> {
    claim(directory, "bezel-screen-control.lock")
}

/// Whether Studio currently owns (or is waiting for) the display.
pub fn studio_active(directory: &Path) -> io::Result<bool> {
    Ok(claim(directory, "bezel-studio-session.lock")?.is_none())
}

/// Held by Studio until full exit, including time spent hidden in the tray.
pub struct StudioSession {
    _studio: File,
    _screen: File,
    returns_to_light: bool,
    directory: std::path::PathBuf,
}

impl StudioSession {
    /// An existing Light stays idle until this session releases the screen.
    pub fn start_light(
        &self,
        executable: &Path,
        arguments: &[std::ffi::OsString],
    ) -> io::Result<()> {
        if light_instance(&self.directory)?.is_none() {
            return Ok(());
        }
        let mut command = std::process::Command::new(executable);
        command
            .args(arguments)
            .current_dir(
                executable
                    .parent()
                    .ok_or_else(|| io::Error::other("no Light installation directory"))?,
            )
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        let mut child = command.spawn()?;
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            if light_instance(&self.directory)?.is_none() {
                return Ok(());
            }
            if let Some(status) = child.try_wait()? {
                return Err(io::Error::other(format!("Bezel Light exited: {status}")));
            }
            if Instant::now() >= until {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Bezel Light did not start",
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Stop the waiting Light too when the user explicitly chooses Quit.
    pub fn stop_light(&self) -> io::Result<()> {
        if light_instance(&self.directory)?.is_some() {
            return Ok(());
        }
        std::fs::write(self.directory.join("bezel-light-quit"), b"")?;
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            if light_instance(&self.directory)?.is_some() {
                return Ok(());
            }
            if Instant::now() >= until {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Bezel Light did not quit",
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Whether Light was already running when Studio opened.
    pub fn returns_to_light(&self) -> bool {
        self.returns_to_light
    }
}

/// Ask light to yield and wait for its normal serial cleanup before continuing.
pub fn enter_studio(directory: &Path, timeout: Duration) -> io::Result<StudioSession> {
    let returns_to_light = light_instance(directory)?.is_none();
    let studio = claim(directory, "bezel-studio-session.lock")?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Studio already owns the screen",
        )
    })?;
    let until = Instant::now() + timeout;
    loop {
        if let Some(screen) = light_screen(directory)? {
            return Ok(StudioSession {
                _studio: studio,
                _screen: screen,
                returns_to_light,
                directory: directory.to_path_buf(),
            });
        }
        if Instant::now() >= until {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Light has not released the screen",
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::SystemTime;

    fn directory() -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("bezel-handoff-{}-{unique}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn studio_waits_for_light_then_light_can_resume_after_full_exit() {
        let directory = directory();
        let light = light_screen(&directory).unwrap().unwrap();
        let (tx, rx) = mpsc::channel();
        let worker_directory = directory.clone();
        let worker = std::thread::spawn(move || {
            let studio = enter_studio(&worker_directory, Duration::from_secs(3)).unwrap();
            tx.send(()).unwrap();
            std::thread::sleep(Duration::from_millis(100));
            drop(studio);
        });
        let until = Instant::now() + Duration::from_secs(2);
        while !studio_active(&directory).unwrap() {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(rx.try_recv().is_err(), "Studio must not open a busy screen");
        drop(light);
        rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(light_screen(&directory).unwrap().is_none());
        worker.join().unwrap();
        assert!(!studio_active(&directory).unwrap());
        assert!(light_screen(&directory).unwrap().is_some());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn closing_studio_reuses_light_and_launch_failure_keeps_ownership() {
        let directory = directory();
        let session = enter_studio(&directory, Duration::ZERO).unwrap();
        let missing = directory.join("missing-light.exe");
        assert!(session.start_light(&missing, &[]).is_err());
        assert!(studio_active(&directory).unwrap());
        assert!(light_screen(&directory).unwrap().is_none());
        let light = light_instance(&directory).unwrap().unwrap();
        session.start_light(&missing, &[]).unwrap();
        drop(light);
        drop(session);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn explicit_quit_stops_waiting_light_before_releasing_studio() {
        let directory = directory();
        let light = light_instance(&directory).unwrap().unwrap();
        let session = enter_studio(&directory, Duration::ZERO).unwrap();
        let folder = directory.clone();
        let worker = std::thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(3);
            while !folder.join("bezel-light-quit").exists() {
                assert!(Instant::now() < until);
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(studio_active(&folder).unwrap());
            drop(light);
        });
        session.stop_light().unwrap();
        worker.join().unwrap();
        assert!(studio_active(&directory).unwrap());
        drop(session);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn timeout_clears_studio_request_and_duplicate_light_is_rejected() {
        let directory = directory();
        let instance = light_instance(&directory).unwrap().unwrap();
        assert!(light_instance(&directory).unwrap().is_none());
        let screen = light_screen(&directory).unwrap().unwrap();
        assert!(enter_studio(&directory, Duration::ZERO).is_err());
        assert!(!studio_active(&directory).unwrap());
        drop(screen);
        drop(instance);
        assert!(light_instance(&directory).unwrap().is_some());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
