//! What the studio says outside its window, in the log and on the terminal
//! (D-2026-10-01-gif-sticker-search-12): the one module of its production
//! code that prints or logs.
//!
//! Everything it says is fixed text: a [`DiagCode`], a closed list of codes
//! without data. [`report`] takes nothing else, so no value of the app's (the
//! KLIPY key, an invocation's body, a path, an error's text) reaches a log
//! line or the terminal through it; a value does not compile:
//!
//! ```compile_fail,E0308
//! let body = String::from(r#"{"key":"what the window sent"}"#);
//! bezel_studio::diag::report(body);
//! ```
//!
//! A panic says no more ([`hook_panics`], D-2026-10-01-gif-sticker-search-14):
//! from the first line of `main` on, the terminal gets [`DiagCode::Panicked`]
//! and the place in the source code where the panic happened, never its
//! message, whoever panicked (the app, Tauri, a library).
//!
//! The source guard (`tests::nothing_in_the_app_forges_an_invocation` in
//! `lib.rs`) refuses, anywhere else in the studio's production code, the
//! print and log macros, a panic or an assertion that formats a message, and
//! the output streams, and what an invocation is made of; here, it checks
//! that every function takes only a `DiagCode` or a `&'static str` (none
//! needs one) and is not generic, that the codes carry no data, and that
//! this module uses nothing but `tracing` and its own items (no import, no
//! other module's state, no `static`, no macro of its own), but the one
//! panic hook (`std::panic::set_hook` and its `Box::new` in
//! [`hook_panics`]). Anywhere, it refuses another panic hook, and what
//! installs a logger or a tracing subscriber.

/// Something the studio says outside its window. Each code has its own
/// fixed sentence ([`DiagCode::text`]) and says where it goes: the log at a
/// level, or the terminal. What it is about (a theme, a screen, a file, a
/// video) and the error's own text are not said; where a failure has
/// causes told apart by the kind of its error (Tauri's error's variant, an
/// I/O error's kind), each cause has a code of its own, so the terminal
/// still says why in fixed words (review W1 of round 2, iter 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagCode {
    // ------------------------------------------- the terminal (stderr) --
    /// The app stopped on a panic (its own, Tauri's or a library's): said
    /// with the place in the source code where it happened, never with its
    /// message ([`hook_panics`]). A failed start in Tauri's setup (the
    /// folders, the tray, the window and its web view) and a desktop
    /// without a graphical session panic, in Tauri and in tao.
    Panicked,
    /// The app did not start, for another cause than the one below.
    NotStarted,
    /// The app did not start: one of its plugins did not start.
    PluginNotStarted,
    /// The app's folders (settings, data, cache) were not found.
    FoldersNotFound,
    /// The tray icon was not added.
    TrayNotAdded,
    /// The screen and the sensors are simulated (`BEZEL_FAKE=1`).
    Simulated,
    /// The process could not restart itself with WebKitGTK's DMA-BUF
    /// renderer off, for another cause than those below.
    DmabufRendererOn,
    /// The process could not restart itself: its program file is gone.
    DmabufRestartNoFile,
    /// The process could not restart itself: running its program file was
    /// not allowed.
    DmabufRestartDenied,
    /// The thread that refreshes the session did not start, for another
    /// cause than the one below.
    RefreshLoopNotStarted,
    /// The thread that refreshes the session did not start: the system had
    /// no thread or memory to spare.
    RefreshLoopNoResources,
    // ------------------------------------------------------- the window --
    /// The UI was not asked about unsaved edits before the window closed.
    UnsavedEditsNotAsked,
    /// The UI was not asked about unsaved edits before the app quit.
    UnsavedEditsNotAskedBeforeQuitting,
    /// A storage job's progress did not reach the window.
    StorageProgressNotSent,
    /// The main window did not hide.
    WindowNotHidden,
    // --------------------------------------------------------- the tray --
    /// The tray's live item did not follow live mode.
    TrayLiveItemNotUpdated,
    /// Live mode, asked from the tray, failed.
    TrayLiveFailed,
    /// The thread that turns live mode from the tray did not start.
    TrayLiveNotStarted,
    /// The tray's menu was not labelled again.
    TrayMenuNotRelabelled,
    // ----------------------------------------------- the files and themes --
    /// The local copies of what is sent are kept in memory for this run.
    CopiesInMemory,
    /// The settings were not saved.
    SettingsNotSaved,
    /// The udev rule was not written.
    UdevRuleNotWritten,
    /// A theme that cannot be read was left out of the library.
    ThemeSkipped,
    /// The last theme did not open again.
    LastThemeNotReopened,
    /// No blank theme could be made to start with.
    NoStartingTheme,
    /// A bundled theme made for the screen did not open.
    BundledThemeNotOpened,
    /// A theme's thumbnail was not drawn.
    NoThumbnail,
    /// A theme's thumbnail was not kept.
    ThumbnailNotKept,
    /// An old thumbnail of a theme was not removed.
    OldThumbnailNotRemoved,
    /// The thumbnail of a file on a screen (from its local copy) was not
    /// made.
    NoFileThumbnail,
    // -------------------------------------------------- the theme's video --
    /// A copy of the theme's video was not removed.
    VideoCopyNotRemoved,
    /// The theme's video was not probed.
    VideoNotProbed,
    /// The video's poster was not taken again.
    PosterNotRetaken,
    /// The preview does not play the theme's video.
    PreviewNotPlayed,
    /// The preview stopped playing the theme's video.
    PreviewStopped,
    /// The live screen's video background did not start.
    VideoBackgroundNotStarted,
    // ------------------------------------------- the sensors and screens --
    /// The sensor catalog was not read.
    SensorCatalogNotRead,
    /// A sensor sample failed.
    SensorSampleFailed,
    /// Live mode was not resumed after the screen restarted.
    LiveNotResumed,
    /// Live mode was not restored at start.
    LiveNotRestored,
    /// A frame of the live screen failed.
    LiveFrameFailed,
    /// The live screen was lost, and is connected again.
    LiveScreenLost,
    /// The live screen is back.
    LiveScreenBack,
    /// The live screen is not back yet.
    LiveScreenNotBack,
    /// The live screen stopped after a storage job.
    LiveStoppedAfterStorageJob,
}

impl DiagCode {
    /// Every code, in declaration order.
    pub const ALL: [Self; 45] = [
        Self::Panicked,
        Self::NotStarted,
        Self::PluginNotStarted,
        Self::FoldersNotFound,
        Self::TrayNotAdded,
        Self::Simulated,
        Self::DmabufRendererOn,
        Self::DmabufRestartNoFile,
        Self::DmabufRestartDenied,
        Self::RefreshLoopNotStarted,
        Self::RefreshLoopNoResources,
        Self::UnsavedEditsNotAsked,
        Self::UnsavedEditsNotAskedBeforeQuitting,
        Self::StorageProgressNotSent,
        Self::WindowNotHidden,
        Self::TrayLiveItemNotUpdated,
        Self::TrayLiveFailed,
        Self::TrayLiveNotStarted,
        Self::TrayMenuNotRelabelled,
        Self::CopiesInMemory,
        Self::SettingsNotSaved,
        Self::UdevRuleNotWritten,
        Self::ThemeSkipped,
        Self::LastThemeNotReopened,
        Self::NoStartingTheme,
        Self::BundledThemeNotOpened,
        Self::NoThumbnail,
        Self::ThumbnailNotKept,
        Self::OldThumbnailNotRemoved,
        Self::NoFileThumbnail,
        Self::VideoCopyNotRemoved,
        Self::VideoNotProbed,
        Self::PosterNotRetaken,
        Self::PreviewNotPlayed,
        Self::PreviewStopped,
        Self::VideoBackgroundNotStarted,
        Self::SensorCatalogNotRead,
        Self::SensorSampleFailed,
        Self::LiveNotResumed,
        Self::LiveNotRestored,
        Self::LiveFrameFailed,
        Self::LiveScreenLost,
        Self::LiveScreenBack,
        Self::LiveScreenNotBack,
        Self::LiveStoppedAfterStorageJob,
    ];

    /// Its fixed sentence.
    pub const fn text(self) -> &'static str {
        match self {
            Self::Panicked => "the app panicked",
            Self::NotStarted => "the app did not start",
            Self::PluginNotStarted => "the app did not start: a plugin did not start",
            Self::FoldersNotFound => "the app's folders were not found",
            Self::TrayNotAdded => "the tray icon was not added",
            Self::Simulated => "BEZEL_FAKE=1, simulated Turing 8.8\" and sensors",
            Self::DmabufRendererOn => "could not restart with the DMA-BUF renderer off",
            Self::DmabufRestartNoFile => {
                "could not restart with the DMA-BUF renderer off: the app's program file is gone"
            }
            Self::DmabufRestartDenied => {
                "could not restart with the DMA-BUF renderer off: running the app's program file \
                 was not allowed"
            }
            Self::RefreshLoopNotStarted => "refresh loop not started",
            Self::RefreshLoopNoResources => {
                "refresh loop not started: no thread or memory to spare"
            }
            Self::UnsavedEditsNotAsked => "unsaved edits not asked about",
            Self::UnsavedEditsNotAskedBeforeQuitting => {
                "unsaved edits not asked about before quitting"
            }
            Self::StorageProgressNotSent => "storage progress not sent",
            Self::WindowNotHidden => "window not hidden",
            Self::TrayLiveItemNotUpdated => "tray live item not updated",
            Self::TrayLiveFailed => "live mode from the tray failed",
            Self::TrayLiveNotStarted => "live mode from the tray not started",
            Self::TrayMenuNotRelabelled => "tray menu not relabelled",
            Self::CopiesInMemory => "local copies are kept for this run only",
            Self::SettingsNotSaved => "settings not saved",
            Self::UdevRuleNotWritten => "udev rule not written",
            Self::ThemeSkipped => "a theme that cannot be read was skipped",
            Self::LastThemeNotReopened => "last theme not reopened",
            Self::NoStartingTheme => "no starting theme",
            Self::BundledThemeNotOpened => "bundled theme not opened",
            Self::NoThumbnail => "no thumbnail",
            Self::ThumbnailNotKept => "thumbnail not kept",
            Self::OldThumbnailNotRemoved => "old thumbnail not removed",
            Self::NoFileThumbnail => "no thumbnail of a screen file",
            Self::VideoCopyNotRemoved => "copy of the theme video not removed",
            Self::VideoNotProbed => "the theme's video is not probed",
            Self::PosterNotRetaken => "the poster is not taken again",
            Self::PreviewNotPlayed => "the preview does not play the theme's video",
            Self::PreviewStopped => "the preview stops playing the theme's video",
            Self::VideoBackgroundNotStarted => "video background not started",
            Self::SensorCatalogNotRead => "sensor catalog not read",
            Self::SensorSampleFailed => "sensor sample failed",
            Self::LiveNotResumed => "live mode not resumed",
            Self::LiveNotRestored => "live mode not restored",
            Self::LiveFrameFailed => "live screen frame failed",
            Self::LiveScreenLost => "live screen lost; connecting it again",
            Self::LiveScreenBack => "the live screen is back",
            Self::LiveScreenNotBack => "the live screen is not back",
            Self::LiveStoppedAfterStorageJob => "live screen stopped after a storage job",
        }
    }

    /// Where it goes.
    const fn channel(self) -> Channel {
        match self {
            Self::Panicked
            | Self::NotStarted
            | Self::PluginNotStarted
            | Self::FoldersNotFound
            | Self::TrayNotAdded
            | Self::Simulated
            | Self::DmabufRendererOn
            | Self::DmabufRestartNoFile
            | Self::DmabufRestartDenied
            | Self::RefreshLoopNotStarted
            | Self::RefreshLoopNoResources => Channel::Terminal,
            Self::CopiesInMemory => Channel::Error,
            Self::LiveScreenBack => Channel::News,
            _ => Channel::Warning,
        }
    }
}

/// Where a code goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Channel {
    /// The terminal (stderr), where a user who started the app from one
    /// reads it.
    Terminal,
    /// The log, as an error: the app goes on without something it keeps.
    Error,
    /// The log, as a warning.
    Warning,
    /// The log, as news.
    News,
}

/// Says `code`: its sentence on the terminal, or in the log at its level
/// with the code's name.
pub fn report(code: DiagCode) {
    let text = code.text();
    match code.channel() {
        Channel::Terminal => eprintln!("bezel-studio: {text}"),
        Channel::Error => tracing::error!(code = ?code, "{text}"),
        Channel::Warning => tracing::warn!(code = ?code, "{text}"),
        Channel::News => tracing::info!(code = ?code, "{text}"),
    }
}

/// Makes every panic from now on, on any thread, say one line on the
/// terminal: [`DiagCode::Panicked`] and the place in the source code where
/// it happened (its file, from the folder of its crate on, its line and its
/// column), and nothing else: never the panic's message, which may hold a
/// value of the app's (the KLIPY key, a path, an error's text) or of a
/// library's (D-2026-10-01-gif-sticker-search-14). The place is fixed when
/// the app is built; the folders above the crate's are cut, since they are
/// the building machine's (a home folder, for a local build). `main` calls
/// it first; the process's one panic hook.
pub fn hook_panics() {
    std::panic::set_hook(Box::new(|panicked| {
        let said = DiagCode::Panicked.text();
        let Some(at) = panicked.location() else {
            report(DiagCode::Panicked);
            return;
        };
        let parts: Vec<&str> = at.file().split(['/', '\\']).collect();
        let from = parts
            .iter()
            .rposition(|part| *part == "src")
            .map_or(parts.len().saturating_sub(1), |src| src.saturating_sub(1));
        let file = parts.get(from..).unwrap_or_default().join("/");
        eprintln!(
            "bezel-studio: {said} at {file}:{}:{}",
            at.line(),
            at.column()
        );
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_code_has_its_own_fixed_sentence() {
        let mut texts: Vec<&str> = DiagCode::ALL.iter().map(|code| code.text()).collect();
        assert!(
            texts
                .iter()
                .all(|text| !text.is_empty() && !text.contains('{'))
        );
        texts.sort_unstable();
        texts.dedup();
        assert_eq!(texts.len(), DiagCode::ALL.len(), "two codes say the same");
        assert!(
            DiagCode::Simulated
                .text()
                .starts_with(crate::SIMULATION_SWITCH)
        );
    }

    /// What was printed before, each cause of a start that failed that
    /// reaches `main` (reviews W1 of round 2, iters 4 and 5) and a panic go
    /// to the terminal; the rest to the log.
    #[test]
    fn what_the_terminal_shows_is_what_was_printed_before() {
        let on_the_terminal: Vec<DiagCode> = DiagCode::ALL
            .into_iter()
            .filter(|code| code.channel() == Channel::Terminal)
            .collect();
        assert_eq!(
            on_the_terminal,
            [
                DiagCode::Panicked,
                DiagCode::NotStarted,
                DiagCode::PluginNotStarted,
                DiagCode::FoldersNotFound,
                DiagCode::TrayNotAdded,
                DiagCode::Simulated,
                DiagCode::DmabufRendererOn,
                DiagCode::DmabufRestartNoFile,
                DiagCode::DmabufRestartDenied,
                DiagCode::RefreshLoopNotStarted,
                DiagCode::RefreshLoopNoResources,
            ]
        );
        // A cause adds to the sentence said without one.
        for (cause, without) in [
            (DiagCode::PluginNotStarted, DiagCode::NotStarted),
            (DiagCode::DmabufRestartNoFile, DiagCode::DmabufRendererOn),
            (DiagCode::DmabufRestartDenied, DiagCode::DmabufRendererOn),
            (
                DiagCode::RefreshLoopNoResources,
                DiagCode::RefreshLoopNotStarted,
            ),
        ] {
            assert!(
                cause.text().starts_with(&format!("{}: ", without.text())),
                "{cause:?}"
            );
        }
        assert_eq!(DiagCode::CopiesInMemory.channel(), Channel::Error);
        assert_eq!(DiagCode::LiveScreenBack.channel(), Channel::News);
        assert_eq!(DiagCode::SettingsNotSaved.channel(), Channel::Warning);
        for code in DiagCode::ALL {
            report(code);
        }
    }

    /// The switch that makes [`a_panic_says_where_never_what`] the child
    /// that panics.
    const PANICKING: &str = "BEZEL_TEST_PANICKING";

    /// An obvious fake KLIPY key, in the message of each of the child's
    /// panics.
    const KEY: &str = "fake-KLIPY_key-0123456789abcdef";

    /// What the child does: its panics hooked, it panics on a thread of its
    /// own with the key in the message, each way a panic gets one: text,
    /// formatted text, any value, an `unwrap`'s error, an assertion's
    /// operands.
    fn panic_with_the_key() {
        hook_panics();
        let panics: [fn(); 5] = [
            || std::panic::panic_any(KEY),
            || panic!("the key is {KEY}"),
            || std::panic::panic_any(KEY.to_string()),
            || {
                let saved: Result<u8, &str> = KEY.parse().map_err(|_| KEY);
                saved.unwrap();
            },
            || assert_eq!(KEY, "another key"),
        ];
        for panics in panics {
            assert!(std::thread::spawn(panics).join().is_err());
        }
    }

    /// D-2026-10-01-gif-sticker-search-14: a panic says `DiagCode::Panicked`
    /// and where it happened, never its message. The hook is the process's,
    /// so it is proved in a process of its own: the test runs itself again
    /// as a child that panics with the key ([`panic_with_the_key`]), and
    /// reads all that the child printed: one line per panic, the place in
    /// this file from its crate's folder on, and nothing of the key.
    #[test]
    fn a_panic_says_where_never_what() {
        if std::env::var_os(PANICKING).is_some() {
            panic_with_the_key();
            return;
        }
        let name = "diag::tests::a_panic_says_where_never_what";
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([name, "--exact", "--nocapture", "--test-threads=1"])
            .env(PANICKING, "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&child.stdout);
        let stderr = String::from_utf8_lossy(&child.stderr);
        assert!(child.status.success(), "{stdout}\n{stderr}");
        assert!(stdout.contains("1 passed"), "{stdout}");
        let said: Vec<&str> = stderr
            .lines()
            .filter(|line| line.starts_with("bezel-studio:"))
            .collect();
        assert_eq!(said.len(), 5, "{stderr}");
        assert_eq!(stderr.matches("panicked").count(), 5, "{stderr}");
        for line in said {
            let place =
                line.strip_prefix("bezel-studio: the app panicked at src-tauri/src/diag.rs:");
            let numbers: Vec<&str> = place.unwrap_or_default().split(':').collect();
            assert!(
                numbers.len() == 2 && numbers.iter().all(|n| n.parse::<u32>().is_ok()),
                "{line}"
            );
        }
        for printed in [&stdout, &stderr] {
            assert!(
                !printed.contains("KLIPY") && !printed.contains("0123456789"),
                "{printed}"
            );
        }
    }
}
