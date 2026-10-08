//! Application and media context shared by Studio and the unattended runtime.
use super::sensor::Snapshot;

/// Metadata published by an operating-system media session.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaSession {
    /// Application identifier or substring filter.
    pub source: String,
    /// Track title.
    pub title: String,
    /// Track artist.
    pub artist: String,
    /// True only during active playback.
    pub playing: bool,
    /// Position in seconds.
    pub position: f64,
    /// Duration in seconds.
    pub duration: f64,
    /// Bounded PNG thumbnail bytes.
    pub cover: Vec<u8>,
}
/// A media display; no playback actions are sent to applications.
#[derive(Debug, Clone, PartialEq)]
pub struct Player {
    /// Application identifier or substring filter.
    pub source: String,
    /// Display album artwork.
    pub show_cover: bool,
    /// Display timeline and progress bar.
    pub show_progress: bool,
    /// Display the application identifier.
    pub show_source: bool,
    /// Hide the widget when playback stops.
    pub hide_when_stopped: bool,
    /// Label when no session exists.
    pub empty_text: String,
}
/// State condition used to override a card face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerSource {
    /// Executable is running.
    Process,
    /// Executable is absent from a successful process scan.
    ProcessClosed,
    /// Executable owns the foreground window.
    Foreground,
    /// Matching media session is playing.
    MediaPlaying,
}
/// Rules with a higher priority win; ties keep their saved order.
#[derive(Debug, Clone, PartialEq)]
pub struct CardTrigger {
    /// Application condition.
    pub source: TriggerSource,
    /// Executable name or media source filter.
    pub app: String,
    /// Target face index.
    pub face: usize,
    /// Higher values win, 0 to 100.
    pub priority: u32,
    /// Delay before restoring the previous face, 0 to 300.
    pub return_seconds: u32,
}
/// Case-insensitive executable basename without its optional extension.
pub fn app_name(value: &str) -> String {
    let lower = value.trim().to_lowercase();
    lower.strip_suffix(".exe").unwrap_or(&lower).to_owned()
}
/// Media sources may be packaged app IDs, hence a substring filter.
pub fn session<'a>(snapshot: &'a Snapshot, source: &str) -> Option<&'a MediaSession> {
    let name = app_name(source);
    snapshot
        .media
        .iter()
        .filter(|s| name.is_empty() || s.source.to_lowercase().contains(&name))
        .max_by(|a, b| {
            a.playing
                .cmp(&b.playing)
                .then_with(|| b.source.cmp(&a.source))
        })
}
impl CardTrigger {
    /// Evaluate a rule against one measured snapshot.
    pub fn matches(&self, snapshot: &Snapshot) -> bool {
        let app = app_name(&self.app);
        match self.source {
            TriggerSource::Process => {
                snapshot.activity_available && snapshot.applications.contains(&app)
            }
            TriggerSource::ProcessClosed => {
                snapshot.activity_available && !snapshot.applications.contains(&app)
            }
            TriggerSource::Foreground => snapshot
                .foreground
                .as_ref()
                .is_some_and(|s| app_name(s) == app),
            TriggerSource::MediaPlaying => snapshot
                .media
                .iter()
                .any(|s| s.playing && (app.is_empty() || s.source.to_lowercase().contains(&app))),
        }
    }
}
