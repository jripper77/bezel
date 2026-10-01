//! Messages the backend sends to the UI as a stable code with named
//! arguments (D-2026-09-30-release-polish-6): the UI translates the code and
//! fills in the arguments; `message` is the English text, for logs and for a
//! code the UI does not know. `tests/ui/fixtures/backend-codes.json` lists
//! every code with its arguments; a test here keeps it in step and the UI's
//! tests check that both languages translate each one.

use std::collections::BTreeMap;
use std::fmt;

use bezel_core::BezelError;
use bezel_themes::import::ImportWarning;
use serde::ser::SerializeStruct as _;
use serde::{Serialize, Serializer};

/// Defines [`ErrorCode`]: each variant with its stable code and its English
/// sentence, where `{name}` stands for the argument `name`.
/// [`ErrorCode::ALL`] lists every variant.
macro_rules! error_codes {
    ($($variant:ident = $code:literal => $english:literal,)+) => {
        /// Why a UI command failed.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum ErrorCode {
            $(#[doc = $english] $variant,)+
        }

        impl ErrorCode {
            /// Every code, in declaration order.
            pub const ALL: &'static [ErrorCode] = &[$(ErrorCode::$variant,)+];

            /// The stable name the UI translates.
            pub const fn code(self) -> &'static str {
                match self {
                    $(ErrorCode::$variant => $code,)+
                }
            }

            /// The English sentence; `{name}` stands for the argument `name`.
            pub const fn english(self) -> &'static str {
                match self {
                    $(ErrorCode::$variant => $english,)+
                }
            }
        }
    };
}

error_codes! {
    // ------------------------------------------ the core's `BezelError` --
    ScreenNotFound = "screenNotFound" => "screen not found: {screen}",
    AccessDenied = "accessDenied" => "access denied to {address}: {reason}",
    InUse = "inUse" => "{address} is in use by {holders}",
    Timeout = "timeout" => "timeout talking to {detail}",
    Hung = "hung" => "the screen stopped responding: {detail}",
    InvalidInput = "invalidInput" => "invalid input: {detail}",
    Transport = "transport" => "transport error: {detail}",
    Unsupported = "unsupported" => "not supported: {detail}",
    Cancelled = "cancelled" => "cancelled",
    NotConfirmed = "notConfirmed" => "{detail} needs confirmation",
    Refused = "refused" => "refused: {detail}",
    ThemeFile = "themeFile" => "theme file: {detail}",
    SizeMismatch = "sizeMismatch"
        => "{file} was stored with {stored} bytes, not the file's {expected}: the stored size \
            differs; delete it and send it again",
    // ------------------------------------------------------ the studio --
    Busy = "busy" => "a storage operation is using the screen; wait for it to end or cancel it",
    Stale = "stale" => "this upload is no longer prepared; drop the file again",
    Live = "live" => "turn live mode off to play or stop files: the theme covers them",
    NoVideo = "noVideo" => "the live screen is not missing the theme's video",
    VideoNotInTheme = "videoNotInTheme" => "{asset} is not in the theme",
    NotInLibrary = "notInLibrary" => "{location} is not in the theme library; import it instead",
    NotPicked = "notPicked" => "{location} was not picked to save to",
    FileTooLarge = "fileTooLarge" => "{file} is {size} MiB; the limit is {limit} MiB",
    FileError = "fileError" => "{file}: {reason}",
    NotAnImage = "notAnImage" => "{file} is not an image",
    NotMedia = "notMedia" => "{file} is not a video, an animated GIF or a picture Bezel can use",
    NoScreenChosen = "noScreenChosen" => "no screen chosen",
    NoScreenAddress = "noScreenAddress" => "the screen has no address",
    BrightnessRange = "brightnessRange" => "brightness is 0 to 100",
    UnknownOrientation = "unknownOrientation" => "unknown orientation \"{orientation}\"",
    UnknownMedium = "unknownMedium" => "{medium}: expected internal or sd",
    NoModel = "noModel" => "no model to size the theme",
    InvalidTheme = "invalidTheme" => "invalid theme: {detail}",
    ThemeMisfit = "themeMisfit"
        => "this theme is {theme} but the screen is {screen} in this orientation",
    UnknownLanguage = "unknownLanguage" => "unknown language \"{language}\"",
    InvalidHost = "invalidHost" => "\"{host}\" is not a host name or an IP address",
    InvalidFolder = "invalidFolder" => "\"{folder}\" is not a folder",
    System = "system" => "system error: {detail}",
}

impl ErrorCode {
    /// The names of the arguments the sentence takes, in order of first use.
    pub fn params(self) -> Vec<&'static str> {
        placeholders(self.english())
    }
}

/// The `{name}` placeholders of `text`, once each, in order of first use.
fn placeholders(text: &'static str) -> Vec<&'static str> {
    let mut names = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            break;
        };
        if !names.contains(&&after[..end]) {
            names.push(&after[..end]);
        }
        rest = &after[end + 1..];
    }
    names
}

/// Why a UI command failed: an [`ErrorCode`] and the values of its
/// arguments. The UI gets `{code, args, message}`, `message` being the
/// English sentence (`Display`), and `udevCommand` when a command fixes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiError {
    code: ErrorCode,
    args: Vec<(&'static str, String)>,
    udev_command: Option<String>,
}

/// Result of a UI command.
pub type UiResult<T> = Result<T, UiError>;

impl UiError {
    /// An error of `code`; add each argument its sentence takes with
    /// [`Self::arg`].
    pub fn new(code: ErrorCode) -> Self {
        Self {
            code,
            args: Vec::new(),
            udev_command: None,
        }
    }

    /// Names the command that installs the udev rule, which fixes a denied
    /// port on Linux (never run by the app).
    #[must_use]
    pub fn with_udev_command(mut self, command: String) -> Self {
        self.udev_command = Some(command);
        self
    }

    /// The command that installs the udev rule, when it fixes this error.
    pub fn udev_command(&self) -> Option<&str> {
        self.udev_command.as_deref()
    }

    /// Sets the argument `name` (one of the code's [`ErrorCode::params`]).
    #[must_use]
    pub fn arg(mut self, name: &'static str, value: impl fmt::Display) -> Self {
        debug_assert!(
            self.code.params().contains(&name),
            "{name} is not an argument of {}",
            self.code.code()
        );
        self.args.retain(|(n, _)| *n != name);
        self.args.push((name, value.to_string()));
        self
    }

    /// Something the operating system or the app's framework refused.
    pub fn system(detail: impl fmt::Display) -> Self {
        Self::new(ErrorCode::System).arg("detail", detail)
    }

    /// A file that could not be read or written.
    pub fn file(file: impl fmt::Display, reason: impl fmt::Display) -> Self {
        Self::new(ErrorCode::FileError)
            .arg("file", file)
            .arg("reason", reason)
    }

    /// Its code's stable name.
    pub fn code(&self) -> &'static str {
        self.code.code()
    }

    /// The value of the argument `name`.
    pub fn value(&self, name: &str) -> Option<&str> {
        self.args
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// The English sentence, with the arguments filled in.
impl fmt::Display for UiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut rest = self.code.english();
        while let Some(start) = rest.find('{') {
            let after = &rest[start + 1..];
            let Some(end) = after.find('}') else {
                break;
            };
            f.write_str(&rest[..start])?;
            let name = &after[..end];
            match self.value(name) {
                Some(value) => f.write_str(value)?,
                None => write!(f, "{{{name}}}")?,
            }
            rest = &after[end + 1..];
        }
        f.write_str(rest)
    }
}

impl std::error::Error for UiError {}

impl Serialize for UiError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let args: BTreeMap<&str, &str> = self.args.iter().map(|(n, v)| (*n, v.as_str())).collect();
        let fields = 3 + usize::from(self.udev_command.is_some());
        let mut out = serializer.serialize_struct("UiError", fields)?;
        out.serialize_field("code", self.code())?;
        out.serialize_field("args", &args)?;
        out.serialize_field("message", &self.to_string())?;
        if let Some(command) = &self.udev_command {
            out.serialize_field("udevCommand", command)?;
        }
        out.end()
    }
}

/// The core's errors, each with its own code.
impl From<BezelError> for UiError {
    fn from(e: BezelError) -> Self {
        let detail = |code, detail: String| Self::new(code).arg("detail", detail);
        match e {
            BezelError::ScreenNotFound(screen) => {
                Self::new(ErrorCode::ScreenNotFound).arg("screen", screen)
            }
            BezelError::AccessDenied { address, reason } => Self::new(ErrorCode::AccessDenied)
                .arg("address", address)
                .arg("reason", reason),
            BezelError::InUse { address, holders } => Self::new(ErrorCode::InUse)
                .arg("address", address)
                .arg("holders", holders.join(", ")),
            BezelError::Timeout(d) => detail(ErrorCode::Timeout, d),
            // The UI offers the restart (D-2026-09-30-release-polish-13).
            BezelError::Hung(d) => detail(ErrorCode::Hung, d),
            BezelError::InvalidInput(d) => detail(ErrorCode::InvalidInput, d),
            BezelError::Transport(d) => detail(ErrorCode::Transport, d),
            BezelError::Unsupported(d) => detail(ErrorCode::Unsupported, d),
            BezelError::Cancelled { .. } => Self::new(ErrorCode::Cancelled),
            BezelError::NotConfirmed(d) => detail(ErrorCode::NotConfirmed, d),
            BezelError::Refused(refusal) => detail(ErrorCode::Refused, refusal.to_string()),
            BezelError::ThemeFile(d) => detail(ErrorCode::ThemeFile, d),
            // The UI says what to do and offers the delete.
            BezelError::SizeMismatch { path, sent, stored } => Self::new(ErrorCode::SizeMismatch)
                .arg("file", path)
                .arg("stored", stored)
                .arg("expected", sent),
        }
    }
}

/// One import warning as the UI gets it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WarningDto {
    /// The warning's code (`bezel_themes::import::WarningCode`).
    pub code: &'static str,
    /// Its arguments, by name.
    pub args: BTreeMap<&'static str, String>,
    /// The English sentence.
    pub message: String,
}

impl From<&ImportWarning> for WarningDto {
    fn from(w: &ImportWarning) -> Self {
        Self {
            code: w.code().code(),
            args: w.args().iter().cloned().collect(),
            message: w.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::{MismatchDto, RefusalDto};
    use bezel_core::domain::geometry::Size;
    use bezel_core::domain::media::{MediaFormat, MediaKind, Mismatch, VideoCodec};
    use bezel_core::domain::storage::{FileName, NameError, Refusal};
    use bezel_themes::import::{LAYER_NAMES, WarningCode};
    use serde_json::{Value, json};

    /// The fixture the UI's tests read.
    const FIXTURE: &str = include_str!("../../tests/ui/fixtures/backend-codes.json");

    fn sorted<T: Ord>(mut items: Vec<T>) -> Vec<T> {
        items.sort_unstable();
        items
    }

    /// One refusal of each kind the preflight gives.
    fn refusals() -> Vec<Refusal> {
        vec![
            Refusal::InvalidName(NameError::Empty),
            Refusal::WrongExtension {
                name: FileName::parse("a.gif").unwrap(),
                accepted: &["png"],
            },
            Refusal::WrongKind {
                location: MediaKind::Image,
                file: None,
            },
            Refusal::WrongProfile(Vec::new()),
            Refusal::NeedsConverter(Vec::new()),
            Refusal::EmptyFile,
            Refusal::TooLarge { bytes: 2, limit: 1 },
            Refusal::ConvertedTooLarge { bytes: 2, limit: 1 },
            Refusal::NoCard,
            Refusal::NoSpace {
                needed: 2,
                free: 1,
                candidates: Vec::new(),
            },
        ]
    }

    /// One difference of each kind a file may have from a screen's profile.
    fn mismatches() -> Vec<Mismatch> {
        vec![
            Mismatch::Format {
                found: MediaFormat::Mp4,
                accepted: &[MediaFormat::Jpeg],
            },
            Mismatch::Codec(VideoCodec::Other),
            Mismatch::PixelFormat(None),
            Mismatch::BFrames,
            Mismatch::Audio,
            Mismatch::Resolution {
                expected: Size::new(480, 1920),
                found: None,
            },
        ]
    }

    /// What the fixture must hold, from the codes of this crate and its
    /// dependencies.
    fn expected() -> Value {
        let errors: BTreeMap<&str, Vec<&str>> = ErrorCode::ALL
            .iter()
            .map(|c| (c.code(), sorted(c.params())))
            .collect();
        let warnings: BTreeMap<&str, Vec<&str>> = WarningCode::ALL
            .iter()
            .map(|c| (c.code(), sorted(c.params())))
            .collect();
        let refusals = refusals()
            .iter()
            .map(|r| RefusalDto::from(r).code)
            .collect();
        let mismatches = mismatches()
            .iter()
            .map(|m| MismatchDto::from(m).code)
            .collect();
        json!({
            "errors": errors,
            "importLayers": sorted(LAYER_NAMES.to_vec()),
            "importWarnings": warnings,
            "mismatches": sorted::<&str>(mismatches),
            "refusals": sorted::<&str>(refusals),
        })
    }

    #[test]
    fn the_ui_fixture_lists_every_code_with_its_arguments() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        let expected = expected();
        assert_eq!(
            fixture,
            expected,
            "tests/ui/fixtures/backend-codes.json is out of date; it should read:\n{}",
            serde_json::to_string_pretty(&expected).unwrap()
        );
    }

    #[test]
    fn codes_are_unique_and_their_arguments_named() {
        let codes: std::collections::BTreeSet<&str> =
            ErrorCode::ALL.iter().map(|c| c.code()).collect();
        assert_eq!(codes.len(), ErrorCode::ALL.len());
        assert_eq!(ErrorCode::FileTooLarge.params(), ["file", "size", "limit"]);
        assert!(ErrorCode::Busy.params().is_empty());
    }

    #[test]
    fn errors_carry_code_arguments_and_english() {
        let e = UiError::new(ErrorCode::NotInLibrary).arg("location", "/x.bezeltheme");
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            json!({
                "code": "notInLibrary",
                "args": {"location": "/x.bezeltheme"},
                "message": "/x.bezeltheme is not in the theme library; import it instead",
            })
        );
        assert_eq!(UiError::system("gone").to_string(), "system error: gone");
        let denied = UiError::from(BezelError::AccessDenied {
            address: "/dev/ttyACM1".into(),
            reason: "Permission denied".into(),
        })
        .with_udev_command("sudo install x".into());
        let json = serde_json::to_value(&denied).unwrap();
        assert_eq!(json["udevCommand"], "sudo install x");
        assert_eq!(denied.udev_command(), Some("sudo install x"));
        let file = UiError::file("/a.png", "denied");
        assert_eq!(
            (file.code(), file.value("reason")),
            ("fileError", Some("denied"))
        );
        assert_eq!(
            UiError::new(ErrorCode::ThemeMisfit).to_string(),
            "this theme is {theme} but the screen is {screen} in this orientation",
            "a missing argument shows its name"
        );
    }

    #[test]
    fn a_failed_size_check_has_its_own_code() {
        let core = BezelError::SizeMismatch {
            path: bezel_core::domain::storage::RemotePath::parse("internal/video/clip.mp4")
                .unwrap(),
            sent: 2000,
            stored: 1990,
        };
        let english = core.to_string();
        let e = UiError::from(core);
        assert_eq!(e.code(), "sizeMismatch");
        assert_eq!(
            (e.value("file"), e.value("stored"), e.value("expected")),
            (Some("internal/video/clip.mp4"), Some("1990"), Some("2000"))
        );
        assert_eq!(e.to_string(), english, "the core's own sentence");
        let other = UiError::from(BezelError::Transport("the cable is out".into()));
        assert_eq!(other.code(), "transport");
    }

    #[test]
    fn warnings_carry_code_arguments_and_english() {
        let w = ImportWarning::new(WarningCode::UnknownColorName).arg("color", "Nope");
        let json = serde_json::to_value(WarningDto::from(&w)).unwrap();
        assert_eq!(
            json,
            json!({
                "code": "unknownColorName",
                "args": {"color": "Nope"},
                "message": "the color name \"Nope\" is unknown",
            })
        );
    }
}
