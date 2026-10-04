//! What a screen does when the computer shuts down
//! (D-2026-10-03-power-off-standby-2, -3, -4): the user's choice per screen,
//! the plan B written on the screen with it, and the photos of the card's
//! album framed for the panel.
//!
//! Rev C screens only (the family measured). The choice lives in the
//! screen's catalog record ([`super::archive::ScreenRecord::standby`]), shared
//! by the CLI and the studio; changing it writes the plan B, the screen's
//! OPTIONS whole ([`PlanB`]), so that the screen does something sensible on
//! its own when nobody applies the choice at shutdown. The studio applies it
//! at shutdown (`app::standby::at_shutdown`).

use std::fmt;

use super::archive::ScreenRecord;
use super::device::{DeviceModel, Family};
use super::frame::{Frame, RGBA_BYTES};
use super::framing::{ResolvedFraming, VideoFit, frame_picture};
use super::geometry::Orientation;
use super::media::MediaKind;
use super::screen::Brightness;
use super::storage::{BootMedia, FileEntry, RemotePath, StartMode};
use crate::{BezelError, Result};

/// The minutes of the screen's sleep timer that `off` writes: 1 to 10 (the
/// firmware's range; OPTIONS byte 14 counts the time without host traffic).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SleepMinutes(u8);

impl SleepMinutes {
    /// The shortest timer.
    pub const MIN: SleepMinutes = SleepMinutes(1);
    /// The longest timer the firmware takes.
    pub const MAX: SleepMinutes = SleepMinutes(10);
    /// What the interface suggests.
    pub const SUGGESTED: SleepMinutes = SleepMinutes(5);

    /// A timer of `minutes`; `None` outside 1..=10.
    pub const fn new(minutes: u8) -> Option<Self> {
        if minutes >= Self::MIN.0 && minutes <= Self::MAX.0 {
            Some(Self(minutes))
        } else {
            None
        }
    }

    /// The minutes.
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// The four options without their details, in the order they are offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Choice {
    /// Leave the screen as it is (the default: nothing is sent at shutdown).
    Keep,
    /// Turn the screen off.
    Off,
    /// Loop a video stored on the screen.
    Video,
    /// Let the screen show the photos of its card (`sd/image`) one by one.
    Album,
}

impl Choice {
    /// The four options, in the order they are offered.
    pub const ALL: [Choice; 4] = [Choice::Keep, Choice::Off, Choice::Video, Choice::Album];

    /// Stable machine name (`keep`, `off`, `video`, `album`).
    pub const fn slug(self) -> &'static str {
        match self {
            Choice::Keep => "keep",
            Choice::Off => "off",
            Choice::Video => "video",
            Choice::Album => "album",
        }
    }

    /// The option named by `slug`.
    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.slug() == slug)
    }
}

/// What a screen does when the computer shuts down: the choice recorded for
/// it (D-2026-10-03-power-off-standby-2 (1)).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum Standby {
    /// Nothing is sent at shutdown (the default, and what a catalog without
    /// a choice means).
    #[default]
    Keep,
    /// TURNOFF at shutdown; the plan B is the screen's sleep timer.
    Off(SleepMinutes),
    /// The stored video at this path (in `internal/video` or `sd/video`)
    /// loops at shutdown; the plan B is start mode 2 (the firmware then
    /// plays the first video of `sd/video`, not necessarily this one).
    Video(RemotePath),
    /// The screen restarts into start mode 1 at shutdown and shows the
    /// photos of `sd/image` one by one; needs a card.
    Album,
}

impl Standby {
    /// The option this is.
    pub const fn choice(&self) -> Choice {
        match self {
            Standby::Keep => Choice::Keep,
            Standby::Off(_) => Choice::Off,
            Standby::Video(_) => Choice::Video,
            Standby::Album => Choice::Album,
        }
    }

    /// The sleep timer of `off`; `None` for the others.
    pub const fn sleep_minutes(&self) -> Option<SleepMinutes> {
        match self {
            Standby::Off(minutes) => Some(*minutes),
            _ => None,
        }
    }

    /// The video of `video`; `None` for the others.
    pub const fn file(&self) -> Option<&RemotePath> {
        match self {
            Standby::Video(path) => Some(path),
            _ => None,
        }
    }

    /// A choice from its parts as the driving adapters receive them (the
    /// studio's `{choice, sleepMinutes, file}`, the CLI's
    /// `<choice> --sleep N --file <internal|sd>/video/<name>`): `off` needs
    /// the minutes (1 to 10), `video` a path in a video folder, and the
    /// others neither. Anything else is `InvalidInput` saying why.
    pub fn from_parts(
        choice: Choice,
        sleep_minutes: Option<u8>,
        file: Option<&str>,
    ) -> Result<Self> {
        let refuse = |why: &str| Err(BezelError::InvalidInput(why.to_string()));
        if sleep_minutes.is_some() && choice != Choice::Off {
            return refuse("the sleep timer goes only with the choice off");
        }
        if file.is_some() && choice != Choice::Video {
            return refuse("a file goes only with the choice video");
        }
        match choice {
            Choice::Keep => Ok(Standby::Keep),
            Choice::Album => Ok(Standby::Album),
            Choice::Off => match sleep_minutes.and_then(SleepMinutes::new) {
                Some(minutes) => Ok(Standby::Off(minutes)),
                None => refuse("off needs the sleep timer, 1 to 10 minutes"),
            },
            Choice::Video => match file {
                Some(text) => Ok(Standby::Video(video_path(text)?)),
                None => refuse("video needs a stored video, <internal|sd>/video/<name>"),
            },
        }
    }

    /// The plan B this choice writes on a screen whose boot media starts in
    /// `boot` (D-2026-10-03-power-off-standby-2 (3)): `keep` the boot
    /// media's start mode and no timer (it undoes the others), `off` the
    /// boot media's start mode and its timer, `video` start mode 2 and
    /// `album` start mode 1, both without a timer (the timer would put them
    /// to sleep too).
    pub const fn plan_b(&self, boot: StartMode) -> PlanB {
        match self {
            Standby::Keep => PlanB::new(boot, 0),
            Standby::Off(minutes) => PlanB::new(boot, minutes.get()),
            Standby::Video(_) => PlanB::new(StartMode::Video, 0),
            Standby::Album => PlanB::new(StartMode::Image, 0),
        }
    }
}

/// `text` as the path of a video: `<internal|sd>/video/<name>`.
fn video_path(text: &str) -> Result<RemotePath> {
    let path = RemotePath::parse(text)?;
    if path.location.kind != MediaKind::Video {
        return Err(BezelError::InvalidInput(format!(
            "{path} is not in a video folder"
        )));
    }
    Ok(path)
}

/// The choice a screen's catalog record holds, as the shutdown applies it
/// (`app::standby::at_shutdown`, which reads it from the store it is
/// given). Private to the core: no caller hands the shutdown a choice, it
/// is always read from a catalog, where only `app::standby::choose` records
/// one, after the user's `Confirm::Yes`; so its actions run under that
/// confirmation (D-2026-10-03-power-off-standby-2 (5)). That a store holds
/// only such choices is up to the store's writers and to code review: the
/// core cannot tell the shared catalog from a store an adapter fills by
/// itself (D-2026-10-03-power-off-standby-6 (3)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecordedChoice {
    standby: Standby,
    brightness: Option<Brightness>,
}

impl RecordedChoice {
    /// The choice `record` holds (`keep` without a record), and the level
    /// the plan B it last stored starts with, when the user chose one.
    pub(crate) fn of(record: Option<&ScreenRecord>) -> Self {
        Self {
            standby: record.map(|r| r.standby.clone()).unwrap_or_default(),
            brightness: record.and_then(ScreenRecord::start_brightness),
        }
    }

    /// The choice.
    pub(crate) fn standby(&self) -> &Standby {
        &self.standby
    }

    /// The backlight level stored with the screen's last plan B, when the
    /// user chose one ([`StoredPlanB::brightness`]).
    pub(crate) fn brightness(&self) -> Option<Brightness> {
        self.brightness
    }
}

/// What the screen does on its own, written persistently with every choice
/// and boot media (rev C: OPTIONS 0x7D, written whole with the brightness
/// the link last sent and no flip; D-2026-10-03-power-off-standby-2 (3), (4)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlanB {
    /// What the screen shows on its own after power-up.
    pub start_mode: StartMode,
    /// Minutes without host traffic before the screen sleeps: 0 never,
    /// otherwise 1 to [`SleepMinutes::MAX`] (adapters clamp above it).
    pub sleep_minutes: u8,
}

impl PlanB {
    /// A plan B.
    pub const fn new(start_mode: StartMode, sleep_minutes: u8) -> Self {
        Self {
            start_mode,
            sleep_minutes,
        }
    }

    /// What setting `boot` as the boot media writes next to the recorded
    /// `standby`: the boot media's start mode, and the timer of `off`
    /// (D-2026-10-03-power-off-standby-2 (4): neither rewrites the other's
    /// part).
    pub fn with_boot(boot: &BootMedia, standby: &Standby) -> Self {
        let sleep = standby.sleep_minutes().map_or(0, SleepMinutes::get);
        Self::new(boot.start_mode(), sleep)
    }
}

/// The plan B Bezel last stored on a screen (its OPTIONS), as the screen's
/// catalog record keeps it: by the choice (`app::standby::choose`) or by the
/// boot media (`Manager::set_boot_media`), whichever came last, since both
/// write the same OPTIONS (D-2026-10-03-power-off-standby-2 (4)), with the
/// backlight level stored with it when the user chose one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StoredPlanB {
    /// What the screen does on its own.
    pub plan: PlanB,
    /// The level the screen starts with, when the user chose one (the
    /// CLI's `--brightness`, the storage tab's level); the album's restart
    /// at shutdown writes it again. `None`: the level the link had then,
    /// which is not known here.
    pub brightness: Option<Brightness>,
}

impl fmt::Display for StoredPlanB {
    /// The plan B ([`PlanB`]'s text), then `, brightness N%` when one was
    /// chosen.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.plan)?;
        match self.brightness {
            Some(level) => write!(f, ", brightness {}%", level.percent()),
            None => Ok(()),
        }
    }
}

impl fmt::Display for PlanB {
    /// `start mode <0|1|2>, sleep timer <off|N min>`, the numbers of the
    /// OPTIONS packet.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mode = match self.start_mode {
            StartMode::Default => 0,
            StartMode::Image => 1,
            StartMode::Video => 2,
        };
        match self.sleep_minutes {
            0 => write!(f, "start mode {mode}, sleep timer off"),
            minutes => write!(f, "start mode {mode}, sleep timer {minutes} min"),
        }
    }
}

/// Whether a screen of `model` takes the choice: rev C, the family whose
/// start modes, timer and restart were measured (D-2026-10-03-power-off-
/// standby-2 (1)). Elsewhere the choice is not offered.
pub fn supports(model: &DeviceModel) -> bool {
    model.family == Family::TuringRevC
}

/// Why an option cannot be chosen now (or, at shutdown, why a choice could
/// not be honoured).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Unavailable {
    /// The screen is not connected: changing the choice writes on it.
    NotConnected,
    /// The screen's family does not take the choice.
    Unsupported,
    /// No memory card is inserted (the album is `sd/image`).
    NoCard,
    /// No video is stored on the screen (or the chosen one is gone).
    NoVideo,
}

impl Unavailable {
    /// Stable machine code (`notConnected`, `unsupported`, `noCard`,
    /// `noVideo`), what the interfaces translate.
    pub const fn code(self) -> &'static str {
        match self {
            Unavailable::NotConnected => "notConnected",
            Unavailable::Unsupported => "unsupported",
            Unavailable::NoCard => "noCard",
            Unavailable::NoVideo => "noVideo",
        }
    }
}

/// One of the four options as offered for a screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StandbyOption {
    /// The option.
    pub choice: Choice,
    /// Why it cannot be chosen now; `None` when it can.
    pub unavailable: Option<Unavailable>,
}

/// The four options when none can be chosen, all for `reason` (a screen that
/// is not connected, or of a family without the choice).
pub fn unavailable(reason: Unavailable) -> [StandbyOption; 4] {
    Choice::ALL.map(|choice| StandbyOption {
        choice,
        unavailable: Some(reason),
    })
}

/// What a connected rev C screen offers the choice, as read from it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Offer {
    /// Whether a memory card is inserted.
    pub card: bool,
    /// The videos stored on the screen with their sizes, internal first:
    /// what `video` can play.
    pub videos: Vec<FileEntry>,
}

impl Offer {
    /// The four options in [`Choice::ALL`] order: `keep` and `off` always,
    /// `video` with a stored video, `album` with a card.
    pub fn options(&self) -> [StandbyOption; 4] {
        Choice::ALL.map(|choice| {
            let unavailable = match choice {
                Choice::Video if self.videos.is_empty() => Some(Unavailable::NoVideo),
                Choice::Album if !self.card => Some(Unavailable::NoCard),
                _ => None,
            };
            StandbyOption {
                choice,
                unavailable,
            }
        })
    }
}

/// A photo of the album as the screen stores it
/// (D-2026-10-03-power-off-standby-4): `picture` (decoded, its EXIF
/// orientation already applied) framed by `fit` into the screen's shape as
/// the user looks at it in `orientation` (Cover fills and cuts the overflow,
/// Contain leaves black around it; [`frame_picture`]), turned to the panel's
/// native orientation (480x1920 on the 8.8") and made opaque over black (the
/// album has nothing behind it).
pub fn album_frame(
    picture: &Frame,
    model: &DeviceModel,
    orientation: Orientation,
    fit: VideoFit,
) -> Frame {
    let shape = model.panel.portrait().in_orientation(orientation);
    let framing = ResolvedFraming {
        fit,
        ..ResolvedFraming::plain(0)
    };
    let framed = frame_picture(picture, &framing, shape);
    let mut native = framed.rotated(orientation.quarter_turns_to(model.native_orientation));
    over_black(&mut native);
    native
}

/// Composites every pixel of `frame` over opaque black.
fn over_black(frame: &mut Frame) {
    for pixel in frame.as_rgba_mut().as_chunks_mut::<RGBA_BYTES>().0 {
        let alpha = u16::from(pixel[3]);
        if alpha == 255 {
            continue;
        }
        for channel in &mut pixel[..3] {
            let value = (u16::from(*channel) * alpha + 127) / 255;
            *channel = u8::try_from(value).unwrap_or(u8::MAX);
        }
        pixel[3] = 255;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::catalog::model_by_id;
    use crate::domain::device::ModelId;
    use crate::domain::frame::Rgba;
    use crate::domain::geometry::Size;

    fn eight_eight() -> &'static DeviceModel {
        model_by_id(ModelId("turing-8.8")).unwrap()
    }

    const RED: Rgba = Rgba::opaque(255, 0, 0);
    const BLUE: Rgba = Rgba::opaque(0, 0, 255);
    const GREEN: Rgba = Rgba::opaque(0, 255, 0);

    /// A `size` picture whose first half (left when wider, top when taller)
    /// is red and second half blue, with a green pixel at its top-left.
    fn halves(size: Size) -> Frame {
        let mut frame = Frame::filled(size, BLUE);
        let first = if size.width >= size.height {
            crate::domain::frame::Rect::new(0, 0, size.width / 2, size.height)
        } else {
            crate::domain::frame::Rect::new(0, 0, size.width, size.height / 2)
        };
        frame.fill_rect(first, RED);
        frame.fill_rect(crate::domain::frame::Rect::new(0, 0, 1, 1), GREEN);
        frame
    }

    /// The pixel the user sees at `(x, y)` of the screen in `orientation`,
    /// read from the panel-native `frame` (what the driver does in reverse).
    fn seen(frame: &Frame, orientation: Orientation, x: u32, y: u32) -> Rgba {
        let model = eight_eight();
        let turns = orientation.quarter_turns_to(model.native_orientation);
        let back = frame.rotated((4 - turns) % 4);
        back.pixel(x, y).unwrap()
    }

    #[test]
    fn a_horizontal_photo_fills_a_horizontal_screen() {
        // 2:1 on a 4:1 screen: Cover keeps the full width and cuts top and
        // bottom; the left half stays left as the user looks at it.
        let photo = halves(Size::new(400, 200));
        let frame = album_frame(
            &photo,
            eight_eight(),
            Orientation::Landscape,
            VideoFit::Cover,
        );
        assert_eq!(frame.size(), Size::new(480, 1920), "panel-native");
        let at = |x, y| seen(&frame, Orientation::Landscape, x, y);
        assert_eq!(at(10, 240), RED);
        assert_eq!(at(1910, 240), BLUE);
        assert!(
            frame
                .as_rgba()
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[3] == 255)
        );

        // Contain shows the whole photo, black on both sides.
        let frame = album_frame(
            &photo,
            eight_eight(),
            Orientation::ReverseLandscape,
            VideoFit::Contain,
        );
        assert_eq!(frame.size(), Size::new(480, 1920));
        let at = |x, y| seen(&frame, Orientation::ReverseLandscape, x, y);
        assert_eq!(at(10, 240), Rgba::BLACK);
        assert_eq!(at(1910, 240), Rgba::BLACK);
        // 960x480 in the middle: from x 480 to 1440.
        assert_eq!(at(480, 0), GREEN, "the photo's top-left corner");
        assert_eq!(at(700, 240), RED);
        assert_eq!(at(1300, 240), BLUE);
    }

    #[test]
    fn a_vertical_photo_stands_up_on_a_vertical_screen() {
        // A phone photo 1:2 on a 1:4 screen in portrait: Contain fits the
        // width, black above and below; the top half stays on top.
        let photo = halves(Size::new(300, 600));
        for orientation in [Orientation::Portrait, Orientation::ReversePortrait] {
            let frame = album_frame(&photo, eight_eight(), orientation, VideoFit::Contain);
            assert_eq!(frame.size(), Size::new(480, 1920), "{orientation:?}");
            let at = |x, y| seen(&frame, orientation, x, y);
            // 480x960 in the middle: from y 480 to 1440.
            assert_eq!(at(240, 10), Rgba::BLACK);
            assert_eq!(at(240, 1910), Rgba::BLACK);
            assert_eq!(at(0, 480), GREEN, "the photo's top-left corner");
            assert_eq!(at(240, 700), RED);
            assert_eq!(at(240, 1300), BLUE);
        }
        // Cover fills the screen: the middle columns of the photo, whole
        // height.
        let frame = album_frame(
            &photo,
            eight_eight(),
            Orientation::Portrait,
            VideoFit::Cover,
        );
        let at = |x, y| seen(&frame, Orientation::Portrait, x, y);
        assert_eq!((at(0, 0), at(479, 1919)), (RED, BLUE));
        assert_eq!((at(240, 900), at(240, 1000)), (RED, BLUE));
    }

    #[test]
    fn a_transparent_photo_goes_over_black() {
        let photo = Frame::filled(
            Size::new(480, 1920),
            Rgba {
                r: 200,
                g: 100,
                b: 50,
                a: 128,
            },
        );
        let frame = album_frame(
            &photo,
            eight_eight(),
            Orientation::Portrait,
            VideoFit::Cover,
        );
        assert_eq!(frame.pixel(0, 0), Some(Rgba::opaque(100, 50, 25)));
    }

    #[test]
    fn each_choice_writes_its_plan_b() {
        let five = SleepMinutes::new(5).unwrap();
        let video = RemotePath::parse("sd/video/loop.mp4").unwrap();
        let cases = [
            (
                Standby::Keep,
                StartMode::Video,
                PlanB::new(StartMode::Video, 0),
            ),
            (
                Standby::Off(five),
                StartMode::Image,
                PlanB::new(StartMode::Image, 5),
            ),
            (
                Standby::Off(five),
                StartMode::Default,
                PlanB::new(StartMode::Default, 5),
            ),
            (
                Standby::Video(video),
                StartMode::Image,
                PlanB::new(StartMode::Video, 0),
            ),
            (
                Standby::Album,
                StartMode::Video,
                PlanB::new(StartMode::Image, 0),
            ),
        ];
        for (standby, boot, plan) in cases {
            assert_eq!(standby.plan_b(boot), plan, "{standby:?} over {boot:?}");
        }
        // The boot media keeps the timer of off, and only that.
        let clip = BootMedia::File(RemotePath::parse("internal/video/a.mp4").unwrap());
        assert_eq!(
            PlanB::with_boot(&clip, &Standby::Off(five)),
            PlanB::new(StartMode::Video, 5)
        );
        assert_eq!(
            PlanB::with_boot(&BootMedia::Default, &Standby::Album),
            PlanB::new(StartMode::Default, 0)
        );
        assert_eq!(
            PlanB::new(StartMode::Image, 0).to_string(),
            "start mode 1, sleep timer off"
        );
        assert_eq!(
            PlanB::new(StartMode::Default, 7).to_string(),
            "start mode 0, sleep timer 7 min"
        );
        assert_eq!(Standby::default(), Standby::Keep);
    }

    #[test]
    fn the_sleep_timer_is_one_to_ten_minutes() {
        assert_eq!(SleepMinutes::new(0), None);
        assert_eq!(SleepMinutes::new(11), None);
        assert_eq!(SleepMinutes::new(1), Some(SleepMinutes::MIN));
        assert_eq!(SleepMinutes::new(10), Some(SleepMinutes::MAX));
        assert_eq!(SleepMinutes::SUGGESTED.get(), 5);
    }

    #[test]
    fn choices_read_from_their_parts() {
        let parts = |choice, sleep, file| Standby::from_parts(choice, sleep, file);
        assert_eq!(parts(Choice::Keep, None, None), Ok(Standby::Keep));
        assert_eq!(parts(Choice::Album, None, None), Ok(Standby::Album));
        let off = parts(Choice::Off, Some(3), None).unwrap();
        assert_eq!(off.sleep_minutes().map(SleepMinutes::get), Some(3));
        let video = parts(Choice::Video, None, Some("internal/video/a.mp4")).unwrap();
        assert_eq!(
            video.file().map(ToString::to_string).as_deref(),
            Some("internal/video/a.mp4")
        );
        assert_eq!((video.choice(), off.choice()), (Choice::Video, Choice::Off));
        assert_eq!(Standby::Keep.file(), None);
        assert_eq!(Standby::Keep.sleep_minutes(), None);
        for (choice, sleep, file, why) in [
            (Choice::Off, None, None, "1 to 10"),
            (Choice::Off, Some(0), None, "1 to 10"),
            (Choice::Off, Some(11), None, "1 to 10"),
            (Choice::Keep, Some(5), None, "only with the choice off"),
            (
                Choice::Album,
                None,
                Some("sd/video/a.mp4"),
                "only with the choice video",
            ),
            (Choice::Video, None, None, "needs a stored video"),
            (
                Choice::Video,
                None,
                Some("sd/image/a.png"),
                "not in a video folder",
            ),
            (Choice::Video, None, Some("a.mp4"), "expected <internal|sd>"),
        ] {
            let err = parts(choice, sleep, file).unwrap_err();
            assert!(matches!(err, BezelError::InvalidInput(_)), "{err}");
            assert!(err.to_string().contains(why), "{why:?} in {err}");
        }
        for choice in Choice::ALL {
            assert_eq!(Choice::from_slug(choice.slug()), Some(choice));
        }
        assert_eq!(Choice::from_slug("sleep"), None);
    }

    #[test]
    fn options_say_why_they_cannot_be_chosen() {
        let reasons = |options: [StandbyOption; 4]| options.map(|o| o.unavailable);
        let empty = Offer::default();
        assert_eq!(
            reasons(empty.options()),
            [
                None,
                None,
                Some(Unavailable::NoVideo),
                Some(Unavailable::NoCard)
            ]
        );
        let full = Offer {
            card: true,
            videos: vec![FileEntry {
                path: RemotePath::parse("sd/video/a.mp4").unwrap(),
                size: Some(1),
            }],
        };
        assert_eq!(reasons(full.options()), [None; 4]);
        assert_eq!(full.options().map(|o| o.choice), Choice::ALL);
        let away = unavailable(Unavailable::NotConnected);
        assert_eq!(reasons(away), [Some(Unavailable::NotConnected); 4]);
        let codes = [
            Unavailable::NotConnected,
            Unavailable::Unsupported,
            Unavailable::NoCard,
            Unavailable::NoVideo,
        ]
        .map(Unavailable::code);
        assert_eq!(codes, ["notConnected", "unsupported", "noCard", "noVideo"]);
        assert!(supports(eight_eight()));
        let usb = model_by_id(ModelId("turing-usb-8.8")).unwrap();
        assert!(!supports(usb));
    }
}
