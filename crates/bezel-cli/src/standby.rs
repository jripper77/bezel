//! `bezel standby`: what each screen does when the computer shuts down
//! (D-2026-10-03-power-off-standby-2, -4, -6 (2)), over the core's
//! `app::standby` use cases:
//! - `show`: each connected screen's recorded choice, the plan B that goes
//!   with it and the choices it offers now (queries only);
//! - `set`: a new choice, which stores the plan B on the screen (OPTIONS,
//!   written whole) and records the choice in Bezel's catalog
//!   (`<data>/bezel/storage`), the one Bezel Studio reads at shutdown;
//! - `album add`: a photo framed for the way the screen stands, sent to the
//!   card's album (`sd/image`) through the storage manager (recorded, with a
//!   local copy, like every upload).
//!
//! The CLI never carries out the choice: Bezel Studio does, when the
//! computer shuts down (D-2026-10-03-power-off-standby-3). Changing the
//! choice needs `--yes` (`Confirm::Yes`): without it the summary is printed
//! and nothing else happens, not even opening the screen or the catalog.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
use bezel_core::BezelError;
use bezel_core::app::manager::Manager;
use bezel_core::app::standby::{self as usecase, StandbyOverview};
use bezel_core::app::storage::{PreparedUpload, UploadRequest, prepare_upload};
use bezel_core::app::{choose_screen, discover_screens, open_screen};
use bezel_core::domain::archive::ScreenKey;
use bezel_core::domain::device::{DeviceModel, Family};
use bezel_core::domain::discovery::Screen;
use bezel_core::domain::frame::Frame;
use bezel_core::domain::framing::VideoFit;
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::job::{CancelToken, Job, Progress};
use bezel_core::domain::media::{ConvertOptions, MediaKind};
use bezel_core::domain::screen::{Brightness, Confirm};
use bezel_core::domain::standby::{
    Choice, Offer, SleepMinutes, Standby, StoredPlanB, Unavailable, supports,
};
use bezel_core::domain::storage::{FileName, Medium, Refusal, StorageLocation};
use bezel_core::ports::{
    ArchiveStore, DeviceBus, MediaLocation, MediaTranscoder, ScreenConnector, ScreenLink,
};
use bezel_media::photo;
use clap::{Args, Subcommand, ValueEnum};

use crate::messages::Messages;
use crate::storage::{NOTHING_SENT, cancelled, screen_error, size_text};
use crate::{OrientationArg, Target};

/// Options of `bezel standby`.
#[derive(Debug, Args)]
pub struct StandbyArgs {
    /// What to do.
    #[command(subcommand)]
    pub action: StandbyAction,
}

impl StandbyArgs {
    /// True for the commands that read or write Bezel's catalog: all but
    /// `set` without `--yes`, which opens nothing.
    pub fn uses_catalog(&self) -> bool {
        !matches!(&self.action, StandbyAction::Set(set) if !set.yes)
    }

    /// What the first Ctrl+C does, said on stderr when it is pressed; `None`
    /// for the commands it simply ends.
    pub fn cancel_note(&self) -> Option<&'static str> {
        match &self.action {
            StandbyAction::Album(AlbumArgs {
                action: AlbumAction::Add(_),
            }) => Some("cancelling the upload..."),
            _ => None,
        }
    }
}

/// The `bezel standby` subcommands.
#[derive(Debug, Subcommand)]
pub enum StandbyAction {
    /// Each connected screen's choice, the plan B that goes with it and the
    /// choices it offers now (asks the screen, changes nothing).
    Show {
        /// Only this screen, by port or USB address (default: every
        /// connected screen).
        #[arg(long, short = 's')]
        screen: Option<String>,
    },
    /// Choose what the screen does when the computer shuts down. Also stores
    /// the plan B on the screen (its start mode and sleep timer, for when
    /// Bezel Studio cannot act) and records the choice for Bezel Studio,
    /// which carries it out. Nothing is sent or recorded without --yes.
    Set(SetArgs),
    /// The photo album of the screen's memory card (sd/image), which the
    /// album choice shows one photo after another. `bezel storage ls
    /// sd/image` lists it; `bezel storage rm` removes a photo.
    Album(AlbumArgs),
}

/// The four choices on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ChoiceArg {
    /// Leave the screen as it is (the default): nothing is sent at
    /// shutdown; undoes the plan B of the others.
    Keep,
    /// Turn the screen off; plan B: its sleep timer (--sleep).
    Off,
    /// Loop a video stored on the screen (--file); plan B: start with the
    /// first video of sd/video.
    Video,
    /// Restart into the photo album of the card (sd/image); plan B: start
    /// with the album.
    Album,
}

impl From<ChoiceArg> for Choice {
    fn from(choice: ChoiceArg) -> Self {
        match choice {
            ChoiceArg::Keep => Choice::Keep,
            ChoiceArg::Off => Choice::Off,
            ChoiceArg::Video => Choice::Video,
            ChoiceArg::Album => Choice::Album,
        }
    }
}

/// Options of `bezel standby set`.
#[derive(Debug, Args)]
pub struct SetArgs {
    /// Screen to use.
    #[command(flatten)]
    pub target: Target,
    /// What the screen does when the computer shuts down.
    #[arg(value_enum)]
    pub choice: ChoiceArg,
    /// With off: the minutes without anything from the computer after which
    /// the screen turns itself off (its sleep timer, the plan B), 1-10
    /// [default: 5].
    #[arg(long, value_name = "MINUTES", value_parser = clap::value_parser!(u8).range(1..=10))]
    pub sleep: Option<u8>,
    /// With video: the stored video, as `bezel storage ls` names it
    /// (internal/video/<name> or sd/video/<name>).
    #[arg(long, value_name = "PATH")]
    pub file: Option<String>,
    /// Backlight level in percent the screen boots with, stored with the
    /// plan B (default: the vendor's, about 67%).
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100))]
    pub brightness: Option<u8>,
    /// Really change the choice.
    #[arg(long)]
    pub yes: bool,
}

/// Options of `bezel standby album`.
#[derive(Debug, Args)]
pub struct AlbumArgs {
    /// What to do with the album.
    #[command(subcommand)]
    pub action: AlbumAction,
}

/// The `bezel standby album` subcommands.
#[derive(Debug, Subcommand)]
pub enum AlbumAction {
    /// Add a photo (JPEG, PNG, BMP, a GIF's first picture) to the album:
    /// stood up by its EXIF orientation, framed for the way the screen
    /// stands and stored as a PNG of the panel's native size (480x1920 on
    /// the 8.8"). Replacing a photo of the same name needs --yes. Ctrl+C
    /// cancels.
    Add(AlbumAddArgs),
}

/// How a photo fills the screen in the album.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AlbumFit {
    /// Fill the screen, cutting what overflows.
    Cover,
    /// Show the whole photo, with black around it.
    Contain,
}

impl From<AlbumFit> for VideoFit {
    fn from(fit: AlbumFit) -> Self {
        match fit {
            AlbumFit::Cover => VideoFit::Cover,
            AlbumFit::Contain => VideoFit::Contain,
        }
    }
}

/// Options of `bezel standby album add`.
#[derive(Debug, Args)]
pub struct AlbumAddArgs {
    /// Screen to use.
    #[command(flatten)]
    pub target: Target,
    /// The photo on this computer.
    pub photo: PathBuf,
    /// How the screen stands while it shows the album (default: the
    /// model's: horizontal for a bar-shaped screen like the 8.8").
    #[arg(long, value_enum)]
    pub orientation: Option<OrientationArg>,
    /// How the photo fills the screen.
    #[arg(long, value_enum, default_value_t = AlbumFit::Cover)]
    pub fit: AlbumFit,
    /// The photo's name in the album (default: made from the photo's file
    /// name; .png is added when missing).
    #[arg(long, value_name = "NAME")]
    pub name: Option<String>,
    /// Replace a photo of the same name.
    #[arg(long)]
    pub yes: bool,
}

/// What `bezel standby` works with besides the screen, built by the
/// composition root.
pub struct StandbyKit<'a> {
    /// Bezel's catalog and local copies, shared with Bezel Studio
    /// (`<data>/bezel/storage`).
    pub archive: &'a mut dyn ArchiveStore,
    /// Where that catalog is, as the output names it.
    pub archive_dir: Option<&'a Path>,
    /// Reads the album's PNG for the upload.
    pub media: &'a mut dyn MediaTranscoder,
    /// Cancels a running upload (the Ctrl+C handler holds a clone).
    pub cancel: &'a CancelToken,
    /// Summaries and warnings (stderr).
    pub log: &'a mut dyn Write,
    /// The time recorded for what is sent now, in seconds since the Unix
    /// epoch.
    pub now: u64,
}

/// Runs a `bezel standby` command and returns what should be printed on
/// stdout; summaries go to `kit.log`, and a summary that cannot be written
/// there stops the command before the screen changes.
pub fn run<B, C>(
    args: &StandbyArgs,
    bus: &B,
    connector: &C,
    kit: &mut StandbyKit<'_>,
) -> anyhow::Result<String>
where
    B: DeviceBus + ?Sized,
    C: ScreenConnector + ?Sized,
{
    match &args.action {
        StandbyAction::Show { screen } => show(bus, connector, screen.as_deref(), kit),
        StandbyAction::Set(set_args) => set(bus, connector, set_args, kit),
        StandbyAction::Album(AlbumArgs {
            action: AlbumAction::Add(add),
        }) => album_add(bus, connector, add, kit),
    }
}

/// Who carries out the choice, said by `show`.
const STUDIO_NOTE: &str = "Bezel Studio carries out the choice when the computer shuts down, \
                           while it runs; `bezel standby set` only records it and stores the \
                           plan B on the screen.";

/// Who carries out the choice, said by `set`.
const STUDIO_DOES_IT: &str = "Bezel Studio carries out the choice when the computer shuts down, \
                              while it runs; this command only records it and stores the plan B \
                              on the screen.";

fn connect<B, C>(bus: &B, connector: &C, target: &Target) -> anyhow::Result<Box<dyn ScreenLink>>
where
    B: DeviceBus + ?Sized,
    C: ScreenConnector + ?Sized,
{
    open_screen(bus, connector, target.screen.as_deref()).context("could not open the screen")
}

/// What the screen is told to do at shutdown, as an instruction.
fn doing(standby: &Standby) -> String {
    match standby {
        Standby::Keep => "leave the screen as it is (nothing is sent)".to_string(),
        Standby::Off(_) => "turn the screen off".to_string(),
        Standby::Video(path) => format!("play {path} in a loop"),
        Standby::Album => {
            "restart the screen into the photo album of its card (sd/image)".to_string()
        }
    }
}

// ---------------------------------------------------------------- show

const READING: &str = "the choice for when the computer shuts down";

/// `bezel standby show`: every connected screen (or the one at `address`),
/// rev C screens with their choice, plan B and choices; the others are not
/// even opened.
fn show<B, C>(
    bus: &B,
    connector: &C,
    address: Option<&str>,
    kit: &mut StandbyKit<'_>,
) -> anyhow::Result<String>
where
    B: DeviceBus + ?Sized,
    C: ScreenConnector + ?Sized,
{
    let screens = discover_screens(bus).context("could not list the screens")?;
    let screens = match address {
        Some(_) => vec![choose_screen(screens, address).context("could not open the screen")?],
        None if screens.is_empty() => {
            return Err(anyhow!(BezelError::ScreenNotFound(
                "no smart screen connected".into()
            )));
        }
        None => screens,
    };
    let mut out = String::new();
    for screen in &screens {
        out.push_str(&screen_overview(connector, screen, kit)?);
        out.push('\n');
    }
    out.push_str(STUDIO_NOTE);
    out.push('\n');
    Ok(out)
}

/// One screen of `show`.
fn screen_overview<C>(
    connector: &C,
    screen: &Screen,
    kit: &mut StandbyKit<'_>,
) -> anyhow::Result<String>
where
    C: ScreenConnector + ?Sized,
{
    let at = screen
        .address()
        .map(|a| format!(" at {}", a.0))
        .unwrap_or_default();
    if screen.family != Family::TuringRevC {
        let name = screen
            .model()
            .map_or_else(|| screen.family.slug().to_string(), |m| m.name.to_string());
        return Ok(format!(
            "{name}{at}: not offered (Turing rev C screens only)\n"
        ));
    }
    let mut link = connector
        .connect(screen)
        .context("could not open the screen")?;
    let model = link.identity().model;
    let key = ScreenKey::new(model.id);
    let overview =
        usecase::show(link.as_mut(), kit.archive, &key).map_err(screen_error(READING))?;
    Ok(overview_text(model.name, &at, &overview))
}

fn overview_text(name: &str, at: &str, overview: &StandbyOverview) -> String {
    let mut out = format!("{name}{at}\n");
    out.push_str(&format!(
        "  when the computer shuts down: {}\n",
        doing(&overview.standby)
    ));
    out.push_str(&format!(
        "  plan B:                       {}\n",
        overview.plan_b
    ));
    out.push_str("  choices:\n");
    for option in overview.options {
        let label = option_label(option.choice, &overview.offer);
        let slug = option.choice.slug();
        match option.unavailable {
            None => out.push_str(&format!("    {slug:<6} {label}\n")),
            Some(reason) => out.push_str(&format!(
                "    {slug:<6} {label}: not now, {}\n",
                reason_text(reason)
            )),
        }
    }
    out
}

fn option_label(choice: Choice, offer: &Offer) -> String {
    match choice {
        Choice::Keep => "leave it as it is".to_string(),
        Choice::Off => "turn it off".to_string(),
        Choice::Video if offer.videos.is_empty() => "loop a stored video".to_string(),
        Choice::Video => format!("loop a stored video ({} on the screen)", offer.videos.len()),
        Choice::Album => "the photo album of the card (sd/image)".to_string(),
    }
}

const fn reason_text(reason: Unavailable) -> &'static str {
    match reason {
        Unavailable::NotConnected => "the screen is not connected",
        Unavailable::Unsupported => "not supported by this screen",
        Unavailable::NoCard => "no memory card",
        Unavailable::NoVideo => "no video stored",
    }
}

// ---------------------------------------------------------------- set

/// Said when `set` stops for `--yes`.
const NOTHING_SENT_OR_RECORDED: &str = "Nothing was sent to the screen and nothing was recorded.";

/// The choice `args` names: `off` takes [`SleepMinutes::SUGGESTED`] without
/// `--sleep`; the rest is checked by the core ([`Standby::from_parts`]).
fn chosen(args: &SetArgs) -> anyhow::Result<Standby> {
    let choice = Choice::from(args.choice);
    let sleep = match (args.sleep, choice) {
        (None, Choice::Off) => Some(SleepMinutes::SUGGESTED.get()),
        (sleep, _) => sleep,
    };
    Standby::from_parts(choice, sleep, args.file.as_deref()).map_err(|e| match e {
        BezelError::InvalidInput(why) => anyhow!(why),
        other => other.into(),
    })
}

/// What `set` is about to do, printed with or without `--yes`: the choice,
/// the plan B it stores on the screen and the brightness stored with it.
fn set_summary(standby: &Standby, brightness: Option<u8>) -> String {
    let plan_b = match standby {
        Standby::Keep => "undone: the start mode of the boot media (`bezel storage boot`), \
                          no sleep timer"
            .to_string(),
        Standby::Off(minutes) => format!(
            "the screen's sleep timer: it turns itself off after {} min without anything from \
             the computer (--sleep chooses, 1-10); the start mode of the boot media stays",
            minutes.get()
        ),
        Standby::Video(_) => "start mode 2, no sleep timer: after a restart or a power cut the \
                              screen plays the first video of sd/video, not necessarily this one"
            .to_string(),
        Standby::Album => "start mode 1, no sleep timer: after a restart or a power cut the \
                           screen shows the album"
            .to_string(),
    };
    let level = match brightness {
        Some(level) => format!("{level}% (--brightness)"),
        None => "the vendor default, about 67% (170 of 255; --brightness chooses)".to_string(),
    };
    format!(
        "When the computer shuts down: {}\n  plan B      {plan_b}\n  brightness  stored with \
         the plan B, the brightness the screen boots with: {level}\n{STUDIO_DOES_IT}\n",
        doing(standby)
    )
}

/// What the screen now does at shutdown, said of it.
fn done(standby: &Standby) -> String {
    match standby {
        Standby::Keep => "left as it is when the computer shuts down".to_string(),
        Standby::Off(_) => "turns off when the computer shuts down".to_string(),
        Standby::Video(path) => format!("loops {path} when the computer shuts down"),
        Standby::Album => {
            "restarts into the photo album of its card when the computer shuts down".to_string()
        }
    }
}

/// `bezel standby set`: the summary, then, with `--yes` only, the plan B
/// on the screen and the choice in the catalog (`app::standby::choose`).
fn set<B, C>(
    bus: &B,
    connector: &C,
    args: &SetArgs,
    kit: &mut StandbyKit<'_>,
) -> anyhow::Result<String>
where
    B: DeviceBus + ?Sized,
    C: ScreenConnector + ?Sized,
{
    let standby = chosen(args)?;
    let brightness = args
        .brightness
        .map(|level| Brightness::new(level).context("brightness is 0-100"))
        .transpose()?;
    summarize_set(&standby, args, &mut Messages::new(&mut *kit.log))?;
    let mut link = connect(bus, connector, &args.target)?;
    let model = link.identity().model;
    let key = ScreenKey::new(model.id);
    // A family without the choice is refused by the core below, keep too.
    if standby == Standby::Keep
        && supports(model)
        && recorded_choice(kit.archive, &key)? == Standby::Keep
    {
        return Ok(format!(
            "{}: already {}; nothing was sent or recorded\n",
            model.name,
            done(&standby)
        ));
    }
    // The core sets the level first, so that the OPTIONS carries it, and
    // records it with the plan B (a family without the choice gets
    // nothing: the core refuses it).
    let plan = usecase::choose(
        link.as_mut(),
        kit.archive,
        &key,
        standby.clone(),
        brightness,
        Confirm::Yes,
    )
    .map_err(choice_error)?;
    let stored = StoredPlanB { plan, brightness };
    Ok(set_result(model.name, &standby, stored, kit.archive_dir))
}

/// Writes what `set` is about to do; without `--yes`, says that nothing
/// was sent or recorded and refuses. Nothing goes further unless the
/// summary reached the user.
fn summarize_set(standby: &Standby, args: &SetArgs, log: &mut Messages<'_>) -> anyhow::Result<()> {
    write!(log, "{}", set_summary(standby, args.brightness));
    if !args.yes {
        writeln!(
            log,
            "{NOTHING_SENT_OR_RECORDED} Add --yes to make it the choice."
        );
        log.check()?;
        anyhow::bail!("changing what the screen does when the computer shuts down needs --yes");
    }
    log.check()
}

/// The choice recorded for `key` (`keep` without a record).
fn recorded_choice(archive: &mut dyn ArchiveStore, key: &ScreenKey) -> anyhow::Result<Standby> {
    Ok(archive
        .load()?
        .screen(key)
        .map(|r| r.standby.clone())
        .unwrap_or_default())
}

/// A refused or failed choice as the user reads it.
fn choice_error(error: BezelError) -> anyhow::Error {
    match error {
        BezelError::InvalidInput(why) => anyhow!("{why}; nothing was sent or recorded"),
        BezelError::Refused(Refusal::NoCard) => anyhow!(
            "refused: the screen has no memory card, and the album is the card's sd/image; \
             nothing was sent or recorded"
        ),
        other => other.into(),
    }
}

/// What `set` did: the choice, the plan B stored with the level chosen with
/// it (as `show` says it, [`StoredPlanB`]) and where it is recorded.
fn set_result(
    name: &str,
    standby: &Standby,
    stored: StoredPlanB,
    archive_dir: Option<&Path>,
) -> String {
    let catalog = archive_dir
        .map(|dir| format!(" ({})", dir.display()))
        .unwrap_or_default();
    format!(
        "{name}: {}\n  plan B stored on the screen: {stored}\n  recorded in Bezel's catalog{catalog}, \
         which Bezel Studio reads to carry out the choice when the computer shuts down\n",
        done(standby)
    )
}

// ---------------------------------------------------------------- album add

const ALBUM: &str = "the photo album";

/// How a screen of `model` stands when the user does not say: horizontal
/// for a bar-shaped panel (the long side at least twice the short one, like
/// the 8.8"), else the panel's native orientation. The studio gives a new
/// theme for the model the same orientation.
pub fn model_orientation(model: &DeviceModel) -> Orientation {
    let panel = model.panel.portrait();
    if u64::from(panel.height) >= 2 * u64::from(panel.width) {
        Orientation::Landscape
    } else {
        model.native_orientation
    }
}

/// The photo's name in the album: `--name` (`.png` added when missing) or
/// one made from the photo's file name; checked by the upload rule before
/// anything is written.
fn album_name(args: &AlbumAddArgs) -> anyhow::Result<FileName> {
    let name = match &args.name {
        Some(name) if name.to_ascii_lowercase().ends_with(".png") => name.clone(),
        Some(name) => format!("{name}.png"),
        None => {
            let host = args.photo.file_name().unwrap_or_default().to_string_lossy();
            return Ok(FileName::suggest(&host, "png"));
        }
    };
    FileName::for_upload(&name).map_err(|e| anyhow!("{name}: {}", Refusal::InvalidName(e)))
}

/// The album's PNG on this computer while it is sent (the upload reads a
/// file), deleted when dropped; Bezel keeps its bytes as the local copy.
struct TemporaryPng(PathBuf);

impl TemporaryPng {
    fn write(name: &FileName, png: &[u8]) -> anyhow::Result<Self> {
        let path = std::env::temp_dir().join(format!("bezel-album-{}-{name}", std::process::id()));
        fs::write(&path, png).with_context(|| format!("cannot write {}", path.display()))?;
        Ok(Self(path))
    }

    fn location(&self) -> MediaLocation {
        MediaLocation(self.0.to_string_lossy().into_owned())
    }
}

impl Drop for TemporaryPng {
    fn drop(&mut self) {
        // Best effort: a file left in the temporary folder harms nothing.
        let _ = fs::remove_file(&self.0);
    }
}

/// How the photo is framed, as the summary says it.
fn framing_text(orientation: Orientation, given: bool, fit: AlbumFit) -> String {
    let name = OrientationArg::from(orientation).cli_name();
    let how = if given {
        "--orientation"
    } else {
        "the model's; --orientation chooses"
    };
    let fit = match fit {
        AlbumFit::Cover => "filled: it covers the screen and what overflows is cut",
        AlbumFit::Contain => "fitted: the whole photo, black around it",
    };
    format!("{name} ({how}), {fit}")
}

/// What `album add` is about to do: the photo, where it goes, how it is
/// framed, what is stored and the photo it replaces.
fn album_summary(
    args: &AlbumAddArgs,
    photo: Size,
    screen: &str,
    orientation: Orientation,
    prepared: &PreparedUpload,
) -> String {
    let mut out = format!(
        "Add {} ({}x{}) to the photo album of {screen}\n  as       {}\n",
        args.photo.display(),
        photo.width,
        photo.height,
        prepared.plan.path
    );
    let framing = framing_text(orientation, args.orientation.is_some(), args.fit);
    let stored = prepared
        .media
        .dimensions
        .map(|d| format!("{}x{} ", d.width, d.height))
        .unwrap_or_default();
    out.push_str(&format!(
        "  framed   {framing}\n  stored   as a {stored}PNG turned for the panel, {}\n",
        size_text(prepared.media.bytes)
    ));
    if let Some(old) = &prepared.plan.replaces {
        let size = old
            .size
            .map_or_else(|| "size unknown".to_string(), size_text);
        out.push_str(&format!("  replaces {} ({size})\n", old.path));
    }
    out
}

/// The photo at `path`, upright; what cannot be read says why.
fn read_photo(path: &Path) -> anyhow::Result<Frame> {
    photo::open(path).map_err(|e| match e {
        BezelError::InvalidInput(why) => anyhow!(why),
        other => other.into(),
    })
}

/// `bezel standby album add`: reads the photo first (a photo that cannot
/// be read never opens the screen), frames it for the screen, checks the
/// upload (queries only), prints what it does, then sends it to `sd/image`
/// through the storage manager.
fn album_add<B, C>(
    bus: &B,
    connector: &C,
    args: &AlbumAddArgs,
    kit: &mut StandbyKit<'_>,
) -> anyhow::Result<String>
where
    B: DeviceBus + ?Sized,
    C: ScreenConnector + ?Sized,
{
    let picture = read_photo(&args.photo)?;
    let name = album_name(args)?;
    let mut link = connect(bus, connector, &args.target)?;
    let model = link.identity().model;
    if !supports(model) {
        return Err(screen_error(ALBUM)(BezelError::Unsupported(
            "only Turing rev C screens show the photo album of their card".into(),
        )));
    }
    let orientation = args
        .orientation
        .map_or_else(|| model_orientation(model), Orientation::from);
    let png = photo::album_png(&picture, model, orientation, args.fit.into())?;
    let file = TemporaryPng::write(&name, &png)?;
    let request = UploadRequest {
        source: file.location(),
        name: name.to_string(),
        location: StorageLocation::new(Medium::Card, MediaKind::Image),
        options: ConvertOptions::default(),
    };
    let prepared =
        prepare_upload(link.as_mut(), kit.media, &request).map_err(screen_error(ALBUM))?;
    let mut log = Messages::new(&mut *kit.log);
    let summary = album_summary(args, picture.size(), model.name, orientation, &prepared);
    write!(log, "{summary}");
    let path = &prepared.plan.path;
    if prepared.plan.replaces.is_some() && !args.yes {
        writeln!(log, "{NOTHING_SENT} Add --yes to replace it.");
        log.check()?;
        anyhow::bail!("replacing {path} needs --yes");
    }
    // Nothing is sent unless the summary reached the user.
    log.check()?;
    let bytes = send_to_album(link.as_mut(), &prepared, args.yes, kit)?;
    Ok(format!(
        "{}: added {path} to the photo album ({})\n",
        model.name,
        size_text(bytes)
    ))
}

/// Sends the prepared photo through the storage manager (the exact bytes
/// kept as the local copy, the entry cataloged); returns the bytes stored.
fn send_to_album(
    link: &mut dyn ScreenLink,
    prepared: &PreparedUpload,
    replace: bool,
    kit: &mut StandbyKit<'_>,
) -> anyhow::Result<u64> {
    let confirm = if replace { Confirm::Yes } else { Confirm::No };
    let mut quiet = |_: Progress| {};
    let mut job = Job::new(kit.cancel, &mut quiet);
    let result = Manager::new(link, &mut *kit.archive)
        .upload(kit.media, prepared, confirm, kit.now, &mut job);
    match result {
        Ok(uploaded) => Ok(uploaded.bytes),
        Err(BezelError::Cancelled { partial }) => Err(cancelled(&prepared.plan.path, partial)),
        Err(e) => Err(screen_error(ALBUM)(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::doubles::{turing_usb_bus, weact_bus};
    use crate::{Cli, Command};
    use bezel_core::domain::archive::Catalog;
    use bezel_core::domain::device::ModelId;
    use bezel_core::domain::standby::PlanB;
    use bezel_core::domain::storage::{RemotePath, StartMode};
    use bezel_devices::fake::{FakeStorage, Kept, StorageCall};
    use bezel_devices::{FakeBus, FakeConnector};
    use bezel_media::FfmpegTranscoder;
    use bezel_media::archive::MemoryArchive;
    use clap::Parser;

    const NOW: u64 = 1_790_500_000;

    fn path(text: &str) -> RemotePath {
        RemotePath::parse(text).unwrap()
    }

    fn key() -> ScreenKey {
        ScreenKey::new(ModelId("turing-8.8"))
    }

    /// An 8.8" with a card and one stored video.
    fn screen_with_card() -> FakeConnector {
        FakeConnector::with_storage(
            FakeStorage::default()
                .with_card(1 << 30)
                .with_file(path("sd/video/loop.mp4"), vec![1; 4096]),
        )
    }

    /// Runs `bezel standby …` on `bus`: stdout (or the error) and stderr.
    fn standby_on(
        args: &[&str],
        bus: &FakeBus,
        connector: &FakeConnector,
        archive: &mut MemoryArchive,
    ) -> (anyhow::Result<String>, String) {
        let cli = Cli::try_parse_from(args).unwrap();
        let Command::Standby(standby) = &cli.command else {
            unreachable!("parsed as standby")
        };
        let cancel = CancelToken::new();
        let mut log = Vec::new();
        let mut media = FfmpegTranscoder::new(None);
        let mut kit = StandbyKit {
            archive,
            archive_dir: None,
            media: &mut media,
            cancel: &cancel,
            log: &mut log,
            now: NOW,
        };
        let out = run(standby, bus, connector, &mut kit);
        (out, String::from_utf8(log).unwrap())
    }

    fn standby(
        args: &[&str],
        connector: &FakeConnector,
        archive: &mut MemoryArchive,
    ) -> (anyhow::Result<String>, String) {
        standby_on(args, &FakeBus::turing_88(), connector, archive)
    }

    fn recorded(archive: &mut MemoryArchive) -> Standby {
        archive
            .load()
            .unwrap()
            .screen(&key())
            .map(|r| r.standby.clone())
            .unwrap_or_default()
    }

    /// A `width` x `height` PNG on disk, its left half red and right half
    /// blue.
    fn photo(name: &str, width: u32, height: u32) -> PathBuf {
        let file =
            std::env::temp_dir().join(format!("bezel-standby-{}-{name}", std::process::id()));
        image::RgbImage::from_fn(width, height, |x, _| {
            image::Rgb(if x < width / 2 {
                [255, 0, 0]
            } else {
                [0, 0, 255]
            })
        })
        .save(&file)
        .unwrap();
        file
    }

    #[test]
    fn set_without_yes_opens_nothing_and_says_so() {
        let connector = screen_with_card();
        let mut archive = MemoryArchive::new();
        let (out, log) = standby(
            &["bezel", "standby", "set", "off", "--sleep", "3"],
            &connector,
            &mut archive,
        );
        assert_eq!(
            out.unwrap_err().to_string(),
            "changing what the screen does when the computer shuts down needs --yes"
        );
        assert!(
            log.starts_with("When the computer shuts down: turn the screen off\n"),
            "{log}"
        );
        assert!(log.contains("after 3 min without anything"), "{log}");
        assert!(log.contains("about 67% (170 of 255"), "{log}");
        assert!(log.contains("this command only records it"), "{log}");
        assert!(
            log.ends_with(
                "Nothing was sent to the screen and nothing was recorded. Add --yes to make it \
                 the choice.\n"
            ),
            "{log}"
        );
        let screen = connector.log();
        assert_eq!(screen.connects, 0, "not even opened");
        assert!(screen.storage.calls.is_empty());
        assert!(screen.kept.is_empty(), "{:?}", screen.kept);
        assert!(screen.brightness.is_empty());
        assert_eq!(archive.load().unwrap(), Catalog::default());
    }

    /// DoD row 5 (critic of iteration 1), D-2026-10-03-power-off-standby-2
    /// (3): with --yes the level of --brightness reaches the screen first,
    /// then the plan B, whose OPTIONS carries that level, so the screen
    /// starts with it; the fake screen keeps both in one ordered log.
    /// Without --brightness the plan B alone goes, at the link's level
    /// (none set: the vendor's default).
    #[test]
    fn set_with_yes_sends_the_brightness_then_the_plan_b() {
        let connector = screen_with_card();
        let mut archive = MemoryArchive::new();
        let (out, _) = standby(
            &[
                "bezel",
                "standby",
                "set",
                "album",
                "--brightness",
                "40",
                "--yes",
            ],
            &connector,
            &mut archive,
        );
        let out = out.unwrap();
        assert!(
            out.contains(
                "plan B stored on the screen: start mode 1, sleep timer off, brightness 40%\n"
            ),
            "{out}"
        );
        let forty = Brightness::new(40).unwrap();
        let album = PlanB::new(StartMode::Image, 0);
        assert_eq!(
            connector.log().kept,
            [Kept::Brightness(forty), Kept::Options(album, Some(forty))]
        );
        assert_eq!(recorded(&mut archive), Standby::Album);

        let connector = screen_with_card();
        let (out, _) = standby(
            &["bezel", "standby", "set", "off", "--sleep", "2", "--yes"],
            &connector,
            &mut archive,
        );
        out.unwrap();
        assert_eq!(
            connector.log().kept,
            [Kept::Options(PlanB::new(StartMode::Default, 2), None)]
        );
    }

    #[test]
    fn set_with_yes_stores_the_plan_b_then_records_the_choice() {
        let connector = screen_with_card();
        let mut archive = MemoryArchive::new();
        let (out, log) = standby(
            &[
                "bezel",
                "standby",
                "set",
                "off",
                "--sleep",
                "3",
                "--brightness",
                "40",
                "--yes",
            ],
            &connector,
            &mut archive,
        );
        assert_eq!(
            out.unwrap(),
            "Turing Smart Screen 8.8\": turns off when the computer shuts down\n  plan B stored \
             on the screen: start mode 0, sleep timer 3 min, brightness 40%\n  recorded in \
             Bezel's catalog, which Bezel Studio reads to carry out the choice when the computer \
             shuts down\n"
        );
        assert!(log.contains("40% (--brightness)"), "{log}");
        assert!(!log.contains("Nothing was sent"), "{log}");
        let screen = connector.log();
        assert_eq!(screen.brightness, vec![Brightness::new(40).unwrap()]);
        assert_eq!(
            screen.storage.calls,
            [StorageCall::Options(PlanB::new(StartMode::Default, 3))]
        );
        assert_eq!(
            recorded(&mut archive),
            Standby::Off(SleepMinutes::new(3).unwrap())
        );

        // off without --sleep takes the suggested 5 minutes.
        let (out, _) = standby(
            &["bezel", "standby", "set", "off", "--yes"],
            &connector,
            &mut archive,
        );
        assert!(out.unwrap().contains("sleep timer 5 min"));

        // video and album, then keep undoes the plan B.
        let (out, _) = standby(
            &[
                "bezel",
                "standby",
                "set",
                "video",
                "--file",
                "sd/video/loop.mp4",
                "--yes",
            ],
            &connector,
            &mut archive,
        );
        assert!(
            out.unwrap()
                .contains("loops sd/video/loop.mp4 when the computer shuts down")
        );
        assert_eq!(
            recorded(&mut archive),
            Standby::Video(path("sd/video/loop.mp4"))
        );
        let (out, _) = standby(
            &["bezel", "standby", "set", "album", "--yes"],
            &connector,
            &mut archive,
        );
        assert!(out.unwrap().contains("start mode 1, sleep timer off"));
        assert_eq!(recorded(&mut archive), Standby::Album);
        let (out, _) = standby(
            &["bezel", "standby", "set", "keep", "--yes"],
            &connector,
            &mut archive,
        );
        assert!(
            out.unwrap()
                .contains("left as it is when the computer shuts down")
        );
        assert_eq!(recorded(&mut archive), Standby::Keep);
        let options = |calls: &[StorageCall]| {
            calls
                .iter()
                .filter(|c| matches!(c, StorageCall::Options(_)))
                .count()
        };
        let before = connector.log();
        assert_eq!(options(&before.storage.calls), 5);
        assert_eq!(
            before.storage.options,
            Some(PlanB::new(StartMode::Default, 0))
        );

        // From keep to keep nothing is sent, not even the brightness.
        let (out, _) = standby(
            &[
                "bezel",
                "standby",
                "set",
                "keep",
                "--brightness",
                "90",
                "--yes",
            ],
            &connector,
            &mut archive,
        );
        assert_eq!(
            out.unwrap(),
            "Turing Smart Screen 8.8\": already left as it is when the computer shuts down; \
             nothing was sent or recorded\n"
        );
        let after = connector.log();
        assert_eq!(after.storage.calls, before.storage.calls);
        assert_eq!(after.brightness, before.brightness);
    }

    #[test]
    fn set_refuses_what_the_screen_cannot_do_before_anything_changes() {
        let mut archive = MemoryArchive::new();
        // The choice is checked before the screen is opened.
        let connector = screen_with_card();
        for (args, why) in [
            (
                &["bezel", "standby", "set", "keep", "--sleep", "3", "--yes"][..],
                "the sleep timer goes only with the choice off",
            ),
            (
                &["bezel", "standby", "set", "video", "--yes"][..],
                "video needs a stored video",
            ),
            (
                &[
                    "bezel",
                    "standby",
                    "set",
                    "video",
                    "--file",
                    "sd/image/a.png",
                    "--yes",
                ][..],
                "is not in a video folder",
            ),
        ] {
            let (out, log) = standby(args, &connector, &mut archive);
            let err = out.unwrap_err().to_string();
            assert!(err.contains(why), "{why:?} in {err}");
            assert!(log.is_empty(), "{log}");
        }
        assert_eq!(connector.log().connects, 0);

        // A video that is not stored, an album without a card: refused,
        // nothing written or recorded.
        let (out, _) = standby(
            &[
                "bezel",
                "standby",
                "set",
                "video",
                "--file",
                "internal/video/gone.mp4",
                "--yes",
            ],
            &connector,
            &mut archive,
        );
        assert_eq!(
            out.unwrap_err().to_string(),
            "internal/video/gone.mp4 is not stored on the screen; nothing was sent or recorded"
        );
        let bare = FakeConnector::with_storage(FakeStorage::default());
        let (out, _) = standby(
            &["bezel", "standby", "set", "album", "--yes"],
            &bare,
            &mut archive,
        );
        assert!(
            out.unwrap_err()
                .to_string()
                .starts_with("refused: the screen has no memory card"),
        );
        for screen in [connector.log(), bare.log()] {
            assert!(
                !screen
                    .storage
                    .calls
                    .iter()
                    .any(StorageCall::changes_the_screen),
                "{:?}",
                screen.storage.calls
            );
        }
        assert_eq!(archive.load().unwrap(), Catalog::default());

        // Another family: unsupported, and no brightness either.
        let usb = FakeConnector::default();
        let (out, _) = standby_on(
            &[
                "bezel",
                "standby",
                "set",
                "off",
                "--brightness",
                "10",
                "--yes",
            ],
            &turing_usb_bus(),
            &usb,
            &mut archive,
        );
        let err = out.unwrap_err().to_string();
        assert!(err.contains("rev C screens only"), "{err}");
        // keep too (review W9 of iteration 1): such a screen keeps no
        // choice, not even "as it is".
        let (out, _) = standby_on(
            &["bezel", "standby", "set", "keep", "--yes"],
            &turing_usb_bus(),
            &usb,
            &mut archive,
        );
        let err = out.unwrap_err().to_string();
        assert!(err.contains("rev C screens only"), "{err}");
        assert!(usb.log().brightness.is_empty());
        assert!(usb.log().storage.calls.is_empty());
        assert_eq!(archive.load().unwrap(), Catalog::default());
    }

    #[test]
    fn show_says_the_choice_the_plan_b_and_what_can_be_chosen() {
        let mut archive = MemoryArchive::new();
        let connector = screen_with_card();
        let (out, log) = standby(&["bezel", "standby", "show"], &connector, &mut archive);
        assert!(log.is_empty());
        assert_eq!(
            out.unwrap(),
            "Turing Smart Screen 8.8\" at /dev/ttyACM1\n  when the computer shuts down: leave \
             the screen as it is (nothing is sent)\n  plan B:                       start \
             mode 0, sleep timer off\n  choices:\n    keep   leave it as it is\n    off    \
             turn it off\n    video  loop a stored video (1 on the screen)\n    album  the \
             photo album of the card (sd/image)\n\nBezel Studio carries out the choice when \
             the computer shuts down, while it runs; `bezel standby set` only records it and \
             stores the plan B on the screen.\n"
        );
        assert!(
            !connector
                .log()
                .storage
                .calls
                .iter()
                .any(StorageCall::changes_the_screen)
        );

        // The recorded choice, with the plan B next to the boot media.
        let mut catalog = archive.load().unwrap();
        let record = catalog.screen_mut(&key());
        record.standby = Standby::Off(SleepMinutes::new(7).unwrap());
        record.boot = Some(path("sd/video/loop.mp4"));
        archive.save(&catalog).unwrap();
        let bare = FakeConnector::with_storage(FakeStorage::default());
        let (out, _) = standby(
            &["bezel", "standby", "show", "-s", "/dev/ttyACM0"],
            &bare,
            &mut archive,
        );
        let out = out.unwrap();
        assert!(
            out.contains("when the computer shuts down: turn the screen off\n"),
            "{out}"
        );
        assert!(out.contains("start mode 2, sleep timer 7 min"), "{out}");
        assert!(
            out.contains("video  loop a stored video: not now, no video stored"),
            "{out}"
        );
        assert!(
            out.contains("album  the photo album of the card (sd/image): not now, no memory card"),
            "{out}"
        );

        // Review W4 and W7 (iteration 1): the plan B is the one the record
        // says was stored last, with the level chosen with it.
        let (out, _) = standby(
            &[
                "bezel",
                "standby",
                "set",
                "album",
                "--brightness",
                "40",
                "--yes",
            ],
            &connector,
            &mut archive,
        );
        out.unwrap();
        let (out, _) = standby(&["bezel", "standby", "show"], &connector, &mut archive);
        assert!(out.unwrap().contains(
            "plan B:                       start mode 1, sleep timer off, brightness 40%\n"
        ));
        // The boot media set afterwards (`bezel storage boot`) wins on the
        // screen: `show` says its plan B, the choice stays.
        let mut catalog = archive.load().unwrap();
        catalog.screen_mut(&key()).stored = Some(StoredPlanB {
            plan: PlanB::new(StartMode::Video, 0),
            brightness: None,
        });
        archive.save(&catalog).unwrap();
        let (out, _) = standby(&["bezel", "standby", "show"], &connector, &mut archive);
        let out = out.unwrap();
        assert!(
            out.contains("restart the screen into the photo album"),
            "{out}"
        );
        assert!(
            out.contains("plan B:                       start mode 2, sleep timer off\n"),
            "{out}"
        );

        // Other families are listed, not opened.
        let both = FakeBus::turing_88().and(turing_usb_bus());
        let (out, _) = standby_on(&["bezel", "standby", "show"], &both, &bare, &mut archive);
        let out = out.unwrap();
        assert!(
            out.contains(
                "Turing 8.8\" V1.x (USB) at usb:3-1: not offered (Turing rev C screens only)"
            ),
            "{out}"
        );
        let (out, _) = standby_on(
            &["bezel", "standby", "show"],
            &weact_bus(),
            &bare,
            &mut archive,
        );
        assert!(out.unwrap().contains("not offered"));
        assert_eq!(bare.log().connects, 2, "only the 8.8\" was opened");

        let (out, _) = standby(
            &["bezel", "standby", "show", "-s", "/dev/nope"],
            &bare,
            &mut archive,
        );
        assert!(format!("{:#}", out.unwrap_err()).contains("/dev/nope"));
        let (out, _) = standby_on(
            &["bezel", "standby", "show"],
            &FakeBus::default(),
            &bare,
            &mut archive,
        );
        assert!(
            out.unwrap_err()
                .to_string()
                .contains("no smart screen connected")
        );
    }

    #[test]
    fn album_add_frames_the_photo_for_the_screen_and_needs_yes_to_replace() {
        let wide = photo("Beach Day.png", 400, 200);
        let file = wide.to_string_lossy().into_owned();
        let name = format!("bezel-standby-{}-beach_day.png", std::process::id());
        let target = format!("sd/image/{name}");
        let connector = screen_with_card();
        let mut archive = MemoryArchive::new();
        let (out, log) = standby(
            &["bezel", "standby", "album", "add", &file],
            &connector,
            &mut archive,
        );
        let out = out.unwrap();
        assert!(
            out.starts_with(&format!(
                "Turing Smart Screen 8.8\": added {target} to the photo album ("
            )),
            "{out}"
        );
        assert!(
            log.contains("(400x200) to the photo album of Turing"),
            "{log}"
        );
        assert!(log.contains(&format!("  as       {target}\n")), "{log}");
        assert!(
            log.contains("  framed   horizontal (the model's; --orientation chooses), filled"),
            "{log}"
        );
        assert!(
            log.contains("  stored   as a 480x1920 PNG turned for the panel"),
            "{log}"
        );
        let stored = connector.log().storage.files[&path(&target)].clone();
        let png = image::load_from_memory(&stored).unwrap();
        assert_eq!((png.width(), png.height()), (480, 1920));
        let catalog = archive.load().unwrap();
        let entry = catalog
            .screen(&key())
            .unwrap()
            .entries
            .last()
            .unwrap()
            .clone();
        assert_eq!(entry.path, path(&target));
        assert_eq!(archive.read(&entry.content).unwrap(), Some(stored));

        // The same name again: refused without --yes, replaced with it.
        let uploads = |c: &FakeConnector| {
            c.log()
                .storage
                .calls
                .iter()
                .filter(|c| matches!(c, StorageCall::Upload(..)))
                .count()
        };
        let (out, log) = standby(
            &[
                "bezel",
                "standby",
                "album",
                "add",
                &file,
                "--orientation",
                "vertical",
                "--fit",
                "contain",
            ],
            &connector,
            &mut archive,
        );
        assert_eq!(
            out.unwrap_err().to_string(),
            format!("replacing {target} needs --yes")
        );
        assert!(
            log.contains("  framed   vertical (--orientation), fitted"),
            "{log}"
        );
        assert!(log.contains(&format!("  replaces {target} (")), "{log}");
        assert!(log.ends_with("Nothing was sent to the screen. Add --yes to replace it.\n"));
        assert_eq!(uploads(&connector), 1);
        let (out, _) = standby(
            &["bezel", "standby", "album", "add", &file, "--yes"],
            &connector,
            &mut archive,
        );
        out.unwrap();
        assert_eq!(uploads(&connector), 2);

        // A name of one's own, .png added.
        let (out, _) = standby(
            &[
                "bezel", "standby", "album", "add", &file, "--name", "Holiday",
            ],
            &connector,
            &mut archive,
        );
        assert!(out.unwrap().contains("added sd/image/holiday.png"));
        let _ = fs::remove_file(wide);
    }

    #[test]
    fn album_add_refuses_before_sending_what_cannot_go() {
        let wide = photo("refused.png", 40, 20);
        let file = wide.to_string_lossy().into_owned();
        let mut archive = MemoryArchive::new();

        // Not a photo, a bad name: the screen is not even opened.
        let connector = screen_with_card();
        let (out, _) = standby(
            &["bezel", "standby", "album", "add", "/no/such/photo.jpg"],
            &connector,
            &mut archive,
        );
        assert!(out.unwrap_err().to_string().contains("/no/such/photo.jpg"));
        let (out, _) = standby(
            &["bezel", "standby", "album", "add", &file, "--name", "../up"],
            &connector,
            &mut archive,
        );
        assert_eq!(
            out.unwrap_err().to_string(),
            "../up.png: invalid file name: '/' is not allowed (use a-z, 0-9, '_', '.' and '-')"
        );
        assert_eq!(connector.log().connects, 0);

        // No card: refused by the upload's check, nothing sent.
        let bare = FakeConnector::with_storage(FakeStorage::default());
        let (out, _) = standby(
            &["bezel", "standby", "album", "add", &file],
            &bare,
            &mut archive,
        );
        assert_eq!(
            out.unwrap_err().to_string(),
            "refused: the screen has no memory card"
        );
        assert!(
            !bare
                .log()
                .storage
                .calls
                .iter()
                .any(StorageCall::changes_the_screen)
        );

        // Another family.
        let usb = FakeConnector::default();
        let (out, _) = standby_on(
            &["bezel", "standby", "album", "add", &file],
            &turing_usb_bus(),
            &usb,
            &mut archive,
        );
        assert_eq!(
            out.unwrap_err().to_string(),
            "this screen does not support the photo album (only Turing rev C screens show the \
             photo album of their card)"
        );
        assert!(usb.log().storage.calls.is_empty());
        assert_eq!(archive.load().unwrap(), Catalog::default());
        let _ = fs::remove_file(wide);
    }

    #[test]
    fn names_orientations_and_what_needs_the_catalog() {
        let args = |line: &[&str]| {
            let cli = Cli::try_parse_from(line).unwrap();
            let Command::Standby(standby) = cli.command else {
                unreachable!("parsed as standby")
            };
            standby
        };
        let add = |line: &[&str]| match args(line).action {
            StandbyAction::Album(AlbumArgs {
                action: AlbumAction::Add(add),
            }) => add,
            other => unreachable!("{other:?}"),
        };
        let named = |line: &[&str]| album_name(&add(line)).unwrap().to_string();
        assert_eq!(
            named(&["bezel", "standby", "album", "add", "/p/My Phone.Photo.JPG"]),
            "my_phone_photo.png"
        );
        assert_eq!(
            named(&[
                "bezel", "standby", "album", "add", "a.jpg", "--name", "Sea.PNG"
            ]),
            "sea.png"
        );
        assert_eq!(
            named(&[
                "bezel", "standby", "album", "add", "a.jpg", "--name", "x.jpg"
            ]),
            "x.jpg.png"
        );
        let long = "a".repeat(300);
        let suggested = named(&["bezel", "standby", "album", "add", &format!("{long}.jpg")]);
        assert_eq!(suggested.len(), FileName::MAX_BYTES);
        assert!(suggested.ends_with(".png"));

        let set_without_yes = args(&["bezel", "standby", "set", "keep"]);
        assert!(!set_without_yes.uses_catalog());
        assert_eq!(set_without_yes.cancel_note(), None);
        assert!(args(&["bezel", "standby", "set", "keep", "--yes"]).uses_catalog());
        assert!(args(&["bezel", "standby", "show"]).uses_catalog());
        let album = args(&["bezel", "standby", "album", "add", "a.jpg"]);
        assert!(album.uses_catalog());
        assert_eq!(album.cancel_note(), Some("cancelling the upload..."));

        for (line, wanted) in [
            (
                &["bezel", "standby", "set", "off", "--sleep", "0"][..],
                false,
            ),
            (
                &["bezel", "standby", "set", "off", "--sleep", "11"][..],
                false,
            ),
            (
                &["bezel", "standby", "set", "off", "--sleep", "10"][..],
                true,
            ),
            (&["bezel", "standby", "set", "sleep"][..], false),
            (
                &["bezel", "standby", "album", "add", "a", "--fit", "fill"][..],
                false,
            ),
        ] {
            assert_eq!(Cli::try_parse_from(line).is_ok(), wanted, "{line:?}");
        }

        let model = |id| bezel_core::domain::catalog::model_by_id(ModelId(id)).unwrap();
        assert_eq!(
            model_orientation(model("turing-8.8")),
            Orientation::Landscape
        );
        let square = model("turing-2.1");
        assert_eq!(model_orientation(square), square.native_orientation);
        assert_eq!(VideoFit::from(AlbumFit::Contain), VideoFit::Contain);
        for choice in [
            ChoiceArg::Keep,
            ChoiceArg::Off,
            ChoiceArg::Video,
            ChoiceArg::Album,
        ] {
            let slug = Choice::from(choice).slug();
            assert_eq!(ChoiceArg::from_str(slug, false), Ok(choice));
        }
        assert_eq!(
            reason_text(Unavailable::NotConnected),
            "the screen is not connected"
        );
        assert_eq!(
            reason_text(Unavailable::Unsupported),
            "not supported by this screen"
        );
    }
}
