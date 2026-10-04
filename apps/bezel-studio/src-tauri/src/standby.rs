//! "When the computer shuts down", in the screen's settings
//! (D-2026-10-03-power-off-standby-2, -4, -6): each rev C screen's choice and
//! the plan B written with it, over the core's `app::standby` use cases, and
//! the card's album (`sd/image`), whose photos are framed for the screen as
//! it stands.
//!
//! Screen access is the storage tab's ([`crate::storage`]): one operation at
//! a time under the claim on the screens, the live screen through its open
//! link (lent by the session), any other awake screen opened for the
//! operation and closed after. A screen asleep is never woken: its options
//! say `notConnected` and a change to it is `unsupported`; nor is a screen
//! of another family opened. In the final state of a shutdown every call
//! that reaches a screen answers `busy` (D-2026-10-03-power-off-standby-3).
//!
//! The choice lives in the catalog the CLI shares (`ScreenRecord.standby`,
//! keyed by the screen's model, [`ScreenKey::new`], as the shutdown reads it
//! in [`crate::power`]), read again at every call: a choice the CLI made
//! while the studio runs shows. Changing it, and adding a photo, takes the
//! user's [`Confirm`], the answer of the UI's dialog that says what is
//! written; with `Confirm::No` nothing reaches any screen and nothing is
//! recorded. Replacing a photo of the same name takes a confirmation of its
//! own ([`PhotoAsked::replace`]), which this side checks against the card's
//! listing, as the storage tab and the CLI do (D-2026-10-03-power-off-
//! standby-4 (3)): without it a name the album has is `notConfirmed` and
//! nothing is sent. The album's photos are listed
//! by the storage manager's `manager_overview`, whose listing
//! `manager_thumbnail` answers for (a photo Bezel sent shows its local
//! copy), and removed by the storage tab's `delete_stored` (a confirmed
//! delete).
//!
//! A photo is decoded on this computer (`bezel_media::photo`: JPEG, PNG, BMP
//! or a GIF's first picture, its EXIF orientation applied), framed by Fill
//! or Fit into the shape the screen has as it stands (the orientation last
//! used with it, `screenOrientations`, else its model's), turned to the
//! panel and sent as a PNG of the panel's size through the core's
//! `Manager::upload`, which keeps the local copy and records it.

#[cfg(test)]
pub(crate) mod tests;

use std::fmt::Display;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use bezel_core::BezelError;
use bezel_core::app::manager::Manager;
use bezel_core::app::standby::{choose, show};
use bezel_core::app::storage::{UploadRequest, info, prepare_upload};
use bezel_core::app::{choose_screen, discover_screens};
use bezel_core::domain::archive::ScreenKey;
use bezel_core::domain::clock::LocalTime;
use bezel_core::domain::device::DeviceModel;
use bezel_core::domain::discovery::{Screen, ScreenState};
use bezel_core::domain::frame::Frame;
use bezel_core::domain::framing::VideoFit;
use bezel_core::domain::geometry::Orientation;
use bezel_core::domain::job::{Job, Progress};
use bezel_core::domain::media::{ConvertOptions, MediaKind};
use bezel_core::domain::screen::Confirm;
use bezel_core::domain::standby::{
    Choice, Offer, Standby, Unavailable, album_frame, supports, unavailable,
};
use bezel_core::domain::storage::{
    Confirmed, FileName, Medium, Operation, Refusal, StorageLocation,
};
use bezel_core::ports::{MediaLocation, MediaTranscoder, ScreenLink};
use bezel_media::photo;

use crate::backend::{Backend, MAX_FILE_BYTES, default_orientation, read_limited};
use crate::clock::unix_seconds;
use crate::dto::{AlbumAddedDto, StandbyDto};
use crate::messages::{ErrorCode, UiError, UiResult};
use crate::studio::Resume;

/// Extensions the photo picker offers: what the album decodes.
pub const PHOTO_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "bmp", "gif"];

/// The album: the card's image folder, which the screen shows one photo at
/// a time in start mode 1 (D-2026-10-03-power-off-standby-4).
pub const ALBUM: StorageLocation = StorageLocation::new(Medium::Card, MediaKind::Image);

/// The extension of the album's photos: PNG, the format measured there.
const ALBUM_EXTENSION: &str = "png";

/// Why a change cannot reach a screen of another family.
const OTHER_FAMILY: &str =
    "only Turing rev C screens keep a choice for when the computer shuts down";

/// Why a change cannot reach a screen asleep, which is never woken for it.
const ASLEEP: &str = "the screen is asleep; wake it first";

/// Why the album cannot be chosen or filled without a card.
const NO_CARD: &str = "the album needs an SD card in the screen";

/// A choice as the window asks for it (`set_standby`'s `choice`,
/// `sleepMinutes` and `file`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Asked {
    /// `keep`, `off`, `video` or `album`.
    pub choice: String,
    /// The sleep timer of `off`, 1 to 10 minutes.
    pub sleep_minutes: Option<u32>,
    /// The video of `video`: `<internal|sd>/video/<name>`.
    pub file: Option<String>,
}

impl Asked {
    /// The choice asked for, or `invalidInput` saying what is wrong with it.
    fn standby(&self) -> UiResult<Standby> {
        let choice = Choice::from_slug(&self.choice)
            .ok_or_else(|| invalid(format!("choice \"{}\"", self.choice)))?;
        // Past what the firmware takes either way: refused below.
        let minutes = self
            .sleep_minutes
            .map(|m| u8::try_from(m).unwrap_or(u8::MAX));
        Ok(Standby::from_parts(choice, minutes, self.file.as_deref())?)
    }
}

/// A photo as the window asks to add it to the album (`album_add`'s
/// `source`, `fit`, `name` and `replace`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoAsked {
    /// The photo on this computer.
    pub source: PathBuf,
    /// `cover` (Fill) or `contain` (Fit).
    pub fit: String,
    /// Its name in the album, a `.png`.
    pub name: String,
    /// The user's answer to replacing the album's photo of that name, which
    /// the dialog asks apart (D-2026-10-03-power-off-standby-4 (3)): without
    /// `Confirm::Yes`, a name the card's listing has is `notConfirmed`
    /// (`replacing sd/image/<name>`) and nothing is sent.
    pub replace: Confirm,
}

/// How a screen can be reached for its choice, as discovery and the session
/// tell it.
enum Reach {
    /// Through its link: the live screen's, or an awake rev C screen opened
    /// for it. The screen as discovery lists it, when it does.
    Link(Option<Screen>),
    /// Not at all: why (a family without the choice, a screen asleep), its
    /// model and the screen as listed.
    Away(Unavailable, &'static DeviceModel, Screen),
}

fn invalid(detail: impl Display) -> UiError {
    UiError::new(ErrorCode::InvalidInput).arg("detail", detail)
}

fn unsupported(detail: &str) -> UiError {
    UiError::new(ErrorCode::Unsupported).arg("detail", detail)
}

/// Why a change cannot reach a screen that is `reason` away.
fn away(reason: Unavailable) -> UiError {
    match reason {
        Unavailable::NotConnected => unsupported(ASLEEP),
        _ => unsupported(OTHER_FAMILY),
    }
}

/// `error` as the window gets it; a choice of the album without a card is
/// `unsupported`, as when adding a photo.
fn without_card(error: BezelError) -> UiError {
    match error {
        BezelError::Refused(Refusal::NoCard) => unsupported(NO_CARD),
        other => other.into(),
    }
}

/// The model of `screen` as discovery knows it: its one model, else its
/// first candidate (all of a screen's candidates share its family).
fn model_of(screen: &Screen) -> UiResult<&'static DeviceModel> {
    screen
        .model()
        .or_else(|| screen.candidates.first().copied())
        .ok_or_else(|| unsupported("the screen's model is not known"))
}

/// How a photo is framed: `cover` (Fill) or `contain` (Fit).
fn fit_of(fit: &str) -> UiResult<VideoFit> {
    VideoFit::from_slug(fit).ok_or_else(|| invalid(format!("fit \"{fit}\"")))
}

/// The name a photo gets in the album as typed: an upload name (lower case,
/// `[a-z0-9_.-]`, no leading dot) ending in `.png`.
fn album_name(name: &str) -> UiResult<FileName> {
    FileName::for_upload(name)
        .ok()
        .filter(|n| n.extension() == Some(ALBUM_EXTENSION))
        .ok_or_else(|| invalid(format!("the name \"{name}\"")))
}

/// The photo in the file at `source`, upright (its EXIF orientation
/// applied); a file too large, unreadable or not a photo says so.
fn read_photo(source: &Path) -> UiResult<Frame> {
    let bytes = read_limited(source, MAX_FILE_BYTES)?;
    photo::decode(&bytes).map_err(|e| match e {
        BezelError::InvalidInput(why) => invalid(format!("{}: {why}", source.display())),
        other => other.into(),
    })
}

/// `native`, a frame turned to the panel of `model`, as the user sees it on
/// the screen standing in `orientation`.
fn as_seen(native: &Frame, model: &DeviceModel, orientation: Orientation) -> Frame {
    let turns = orientation.quarter_turns_to(model.native_orientation);
    native.rotated((4 - turns) % 4)
}

/// `png` as a `data:` URL.
fn data_url(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", STANDARD.encode(png))
}

impl Backend {
    // ------------------------------------------------------------ choice --

    /// The choice of `screen` (named by either of its ports) as the catalog
    /// records it now, the four options and why any is not offered, its
    /// stored videos and card, and how it stands. An awake rev C screen is
    /// asked (queries only); one asleep or of another family is not.
    pub fn standby_overview(&self, screen: &str, time: LocalTime) -> UiResult<StandbyDto> {
        match self.reach(screen)? {
            Reach::Away(reason, model, found) => {
                self.storage.refuse_while_shutting_down()?;
                let standby = self.recorded(model)?;
                let orientation = self.standing(screen, Some(&found), model);
                Ok(StandbyDto::of(
                    &standby,
                    &unavailable(reason),
                    &Offer::default(),
                    orientation,
                ))
            }
            Reach::Link(found) => self.on_choice_screen(screen, Resume::Frames, time, |link| {
                self.overview_on(link, screen, found.as_ref())
            }),
        }
    }

    /// Changes the choice of `screen` to `asked`: writes its plan B on the
    /// screen and records it in the catalog (`app::standby::choose`).
    /// `confirm` is the user's answer to the dialog that said what is
    /// written; with `Confirm::No` nothing reaches the screen nor the
    /// catalog. The screen must be awake (or live) and rev C.
    pub fn set_standby(
        &self,
        screen: &str,
        asked: &Asked,
        confirm: Confirm,
        time: LocalTime,
    ) -> UiResult<StandbyDto> {
        let standby = asked.standby()?;
        Confirmed::require(confirm, &Operation::Standby(standby.clone()))?;
        let found = self.changeable(screen)?;
        self.on_choice_screen(screen, Resume::Frames, time, |link| {
            let key = ScreenKey::new(link.identity().model.id);
            {
                let mut store = self.storage.archive();
                // The level the link has now goes with the plan B: none is
                // chosen here ("the level it has now", the dialog says).
                choose(link, store.as_mut(), &key, standby, None, confirm).map_err(without_card)?;
            }
            self.overview_on(link, screen, found.as_ref())
        })
    }

    /// The overview of the screen behind `link`, asked for as `asked`
    /// (listed as `found`).
    fn overview_on(
        &self,
        link: &mut dyn ScreenLink,
        asked: &str,
        found: Option<&Screen>,
    ) -> UiResult<StandbyDto> {
        let model = link.identity().model;
        let key = ScreenKey::new(model.id);
        let overview = show(link, &mut **self.storage.archive(), &key)?;
        Ok(StandbyDto::of(
            &overview.standby,
            &overview.options,
            &overview.offer,
            self.standing(asked, found, model),
        ))
    }

    /// The choice the catalog records now for a screen of `model` (`keep`
    /// without one).
    fn recorded(&self, model: &DeviceModel) -> UiResult<Standby> {
        let catalog = self.storage.archive().load()?;
        let record = catalog.screen(&ScreenKey::new(model.id));
        Ok(record.map(|r| r.standby.clone()).unwrap_or_default())
    }

    /// How `asked` can be reached: the live screen through its link (by
    /// either of its ports); else as discovery lists it: another family or a
    /// screen asleep not at all, an awake rev C screen opened for it.
    fn reach(&self, asked: &str) -> UiResult<Reach> {
        let live = self.studio().is_live(asked);
        let listed = discover_screens(self.bus.as_ref())
            .and_then(|screens| choose_screen(screens, Some(asked)));
        let found = match listed {
            Ok(found) => found,
            Err(_) if live => return Ok(Reach::Link(None)),
            Err(error) => return Err(error.into()),
        };
        let model = model_of(&found)?;
        if !supports(model) {
            return Ok(Reach::Away(Unavailable::Unsupported, model, found));
        }
        if !live && found.state() != ScreenState::Awake {
            return Ok(Reach::Away(Unavailable::NotConnected, model, found));
        }
        Ok(Reach::Link(Some(found)))
    }

    /// `asked` as discovery lists it, when a change can reach it; else why
    /// not (`unsupported`), before anything is opened.
    fn changeable(&self, asked: &str) -> UiResult<Option<Screen>> {
        match self.reach(asked)? {
            Reach::Link(found) => Ok(found),
            Reach::Away(reason, ..) => Err(away(reason)),
        }
    }

    /// How the screen `asked` (listed as `found`) of `model` stands: the
    /// orientation last used with it, by either of its ports, else the
    /// model's ([`default_orientation`]).
    fn standing(&self, asked: &str, found: Option<&Screen>, model: &DeviceModel) -> Orientation {
        let settings = self.settings.load();
        let ports = found
            .into_iter()
            .flat_map(|screen| [&screen.display, &screen.wake])
            .flatten()
            .map(|endpoint| endpoint.address.0.as_str());
        std::iter::once(asked)
            .chain(ports)
            .find_map(|key| settings.orientation_for(key))
            .unwrap_or_else(|| default_orientation(model))
    }

    /// Runs `work` on `screen` as the one operation on the screens (`busy`
    /// otherwise, and in the final state of a shutdown): through the live
    /// link, given back after with a frame (`resume`), or a link opened for
    /// it. One that may change what the screen plays (`Resume::Video`)
    /// drops the transfer plan waiting for confirmation.
    fn on_choice_screen<T>(
        &self,
        screen: &str,
        resume: Resume,
        time: LocalTime,
        work: impl FnOnce(&mut dyn ScreenLink) -> UiResult<T>,
    ) -> UiResult<T> {
        let _claim = self.storage.claim()?;
        if resume == Resume::Video {
            self.storage.forget_plan();
        }
        self.on_screen(screen, resume, time, work)?
    }

    // ------------------------------------------------------------- album --

    /// The photo at `source` framed by `fit` (`cover` or `contain`) as the
    /// album of `screen` shows it, as the user sees it: in the shape the
    /// screen has as it stands (4:1 lying, 1:4 standing on the 8.8"), as a
    /// PNG `data:` URL. Nothing reaches the screen.
    pub fn album_preview(&self, screen: &str, source: &Path, fit: &str) -> UiResult<String> {
        let found = choose_screen(discover_screens(self.bus.as_ref())?, Some(screen))?;
        let model = model_of(&found)?;
        let picture = read_photo(source)?;
        let fit = fit_of(fit)?;
        let orientation = self.standing(screen, Some(&found), model);
        let native = album_frame(&picture, model, orientation, fit);
        let png = photo::encode_png(&as_seen(&native, model, orientation))?;
        Ok(data_url(&png))
    }

    /// Sends the photo `asked` (its `source` framed by its `fit` like its
    /// preview) to the card's album of `screen` as its `name` (a PNG of the
    /// panel's size): `confirm` is the user's answer to the dialog that
    /// showed it and named it. With `Confirm::No` nothing reaches the
    /// screen; without a card nothing is sent. A name the album has already
    /// is replaced only with [`PhotoAsked::replace`]: else `notConfirmed`,
    /// after the listing only. Recorded in the catalog with its local copy,
    /// like every upload.
    pub fn album_add(
        &self,
        screen: &str,
        asked: &PhotoAsked,
        confirm: Confirm,
        time: LocalTime,
    ) -> UiResult<AlbumAddedDto> {
        let found = self.changeable(screen)?;
        let picture = read_photo(&asked.source)?;
        let fit = fit_of(&asked.fit)?;
        let name = album_name(&asked.name)?;
        if confirm == Confirm::No {
            let detail = format!("adding {name} to the album");
            return Err(UiError::new(ErrorCode::NotConfirmed).arg("detail", detail));
        }
        self.on_choice_screen(screen, Resume::Video, time, |link| {
            let model = link.identity().model;
            if !supports(model) {
                return Err(unsupported(OTHER_FAMILY));
            }
            if info(link)?.card.is_none() {
                return Err(unsupported(NO_CARD));
            }
            let orientation = self.standing(screen, found.as_ref(), model);
            let png = photo::album_png(&picture, model, orientation, fit)?;
            let file = self.write_album_png(&name, &png)?;
            let sent = self.send_to_album(link, &file, &name, asked.replace);
            // A copy left behind is written over by the next photo of the
            // name.
            let _ = std::fs::remove_file(&file);
            sent
        })
    }

    /// Writes `png`, the album's photo `name`, where files wait to be sent.
    fn write_album_png(&self, name: &FileName, png: &[u8]) -> UiResult<PathBuf> {
        let dir = self.storage.scratch().join("album");
        let file = dir.join(name.as_str());
        std::fs::create_dir_all(&dir)
            .and_then(|()| std::fs::write(&file, png))
            .map_err(|e| UiError::file(file.display(), e))?;
        Ok(file)
    }

    /// Sends the PNG `file` to the album as `name` (the claim held): the
    /// storage tab's preflight, which lists the album, then the core's
    /// recorded upload, which the shutdown can cancel and which replaces a
    /// photo of that name only with `replace` (`NotConfirmed` before a byte
    /// is sent otherwise).
    fn send_to_album(
        &self,
        link: &mut dyn ScreenLink,
        file: &Path,
        name: &FileName,
        replace: Confirm,
    ) -> UiResult<AlbumAddedDto> {
        let request = UploadRequest {
            source: MediaLocation(file.display().to_string()),
            name: name.to_string(),
            location: ALBUM,
            options: ConvertOptions::default(),
        };
        let mut store = self.storage.archive();
        let mut media = self.storage.media();
        let media: &mut dyn MediaTranscoder = media.as_mut();
        let prepared = prepare_upload(link, media, &request)?;
        let token = self.storage.start_job();
        let mut quiet = |_: Progress| {};
        let mut job = Job::new(&token, &mut quiet);
        let sent = Manager::new(link, store.as_mut()).upload(
            media,
            &prepared,
            replace,
            unix_seconds(),
            &mut job,
        );
        self.storage.end_job();
        let sent = sent?;
        Ok(AlbumAddedDto {
            path: sent.path.to_string(),
            bytes: sent.bytes,
        })
    }
}
