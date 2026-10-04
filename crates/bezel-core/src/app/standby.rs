//! What a screen does when the computer shuts down
//! (D-2026-10-03-power-off-standby-2, -3): showing the choice, changing it
//! (which writes the plan B on the screen and records the choice in the
//! catalog shared by the CLI and the studio) and applying it at shutdown.
//!
//! Rev C screens only; any other family is `Unsupported` before anything is
//! sent. Changing the choice needs the screen and `Confirm::Yes`: with
//! `Confirm::No` nothing is sent, loaded or saved. Applying it reads the
//! choice from the catalog itself ([`at_shutdown`] takes no choice from its
//! caller) and sends only what it says (TURNOFF, PLAY_VIDEO in a loop, or
//! OPTIONS and RESTART), and TURNOFF instead when the choice cannot be
//! honoured, so that the screen never stays frozen on the last frame.

use crate::app::storage::{Presence, ensure_stored, presence, storage_of};
use crate::domain::archive::{ScreenKey, ScreenRecord};
use crate::domain::media::MediaKind;
use crate::domain::screen::{Brightness, Confirm};
use crate::domain::standby::{
    Offer, PlanB, RecordedChoice, Standby, StandbyOption, StoredPlanB, Unavailable, supports,
    unavailable,
};
use crate::domain::storage::{
    Confirmed, FileEntry, Medium, Operation, Refusal, RemotePath, Repeat, StartMode,
    StorageLocation,
};
use crate::ports::{ArchiveStore, ScreenLink, ScreenStorage};
use crate::{BezelError, Result};

/// One screen's choice as [`show`] reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StandbyOverview {
    /// The screen's catalog key.
    pub key: ScreenKey,
    /// The recorded choice (`keep` when none was recorded).
    pub standby: Standby,
    /// The plan B on the screen as the record says: the one Bezel stored
    /// last, by the choice or the boot media set after it, with its level
    /// when one was chosen ([`ScreenRecord::plan_b`]).
    pub plan_b: StoredPlanB,
    /// What the screen offers: its card and stored videos (nothing for a
    /// family without the choice).
    pub offer: Offer,
    /// The four options and why any cannot be chosen now.
    pub options: [StandbyOption; 4],
}

/// What [`at_shutdown`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied {
    /// `keep`: nothing was sent.
    Nothing,
    /// `off`: the screen was turned off.
    TurnedOff,
    /// `video`: the stored video loops.
    Video(RemotePath),
    /// `album`: start mode 1 was written (with the level stored with the
    /// last plan B, when one was chosen) and the screen restarts into the
    /// photos of its card; the record says that plan B was stored last.
    Album,
    /// `album` as [`Applied::Album`], the screen restarting into its photos,
    /// but its record could not be updated (the error says why): it still
    /// says the plan B stored before the restart.
    AlbumNotRecorded(BezelError),
    /// The choice could not be honoured (the video is gone, no card): the
    /// screen was turned off instead of staying frozen.
    TurnedOffInstead(Unavailable),
}

/// `Unsupported` unless the screen behind `link` takes the choice (rev C).
fn ensure_supported(link: &dyn ScreenLink) -> Result<()> {
    let model = link.identity().model;
    if supports(model) {
        return Ok(());
    }
    Err(BezelError::Unsupported(format!(
        "what {} does when the computer shuts down cannot be chosen (rev C screens only)",
        model.name
    )))
}

/// The record of `key` (an empty one, `keep` and the default start screen,
/// without one). Only reads the store.
fn record_of(store: &mut dyn ArchiveStore, key: &ScreenKey) -> Result<ScreenRecord> {
    Ok(store.load()?.screen(key).cloned().unwrap_or_default())
}

/// The choice of the screen behind `link` (keyed `key` in the catalog of
/// `store`), its plan B, and the four options with why any cannot be chosen
/// now. Rev C: queries only (storage info, then the listing of
/// `internal/video` and, with a card, `sd/video`, and each video's size).
/// Other families: every option `Unsupported`, nothing asked of the
/// screen.
pub fn show(
    link: &mut dyn ScreenLink,
    store: &mut dyn ArchiveStore,
    key: &ScreenKey,
) -> Result<StandbyOverview> {
    let record = record_of(store, key)?;
    let plan_b = StoredPlanB {
        plan: record.plan_b(),
        brightness: record.start_brightness(),
    };
    let standby = record.standby;
    let (offer, options) = if supports(link.identity().model) {
        let offer = offer(storage_of(link)?)?;
        let options = offer.options();
        (offer, options)
    } else {
        (Offer::default(), unavailable(Unavailable::Unsupported))
    };
    Ok(StandbyOverview {
        key: key.clone(),
        standby,
        plan_b,
        offer,
        options,
    })
}

/// The card and the stored videos (internal first) with their sizes, by
/// queries only; a card's folder is listed only with a card in (listing
/// creates it).
fn offer(storage: &mut dyn ScreenStorage) -> Result<Offer> {
    let info = storage.info()?;
    let mut videos = Vec::new();
    for medium in Medium::ALL {
        if info.capacity(medium).is_none() {
            continue;
        }
        let location = StorageLocation::new(medium, MediaKind::Video);
        for name in storage.list(location)? {
            let path = RemotePath::new(location, name);
            let size = presence(storage, &path)?.size();
            videos.push(FileEntry { path, size });
        }
    }
    Ok(Offer {
        card: info.card.is_some(),
        videos,
    })
}

/// Changes the choice of the screen behind `link` to `standby`: writes its
/// plan B on the screen (OPTIONS whole, [`Standby::plan_b`] next to the
/// recorded boot media), `brightness` first when the user chose one, so
/// that the OPTIONS carries it and the screen starts with it; then records
/// the choice under `key` in the catalog of `store` with the plan B stored
/// ([`ScreenRecord::stored`]), and returns that plan B
/// (D-2026-10-03-power-off-standby-2 (3)).
///
/// `Confirm::No`: nothing sent, loaded or saved. A family without the
/// choice: `Unsupported`, nothing sent. From `keep` to `keep`: nothing sent
/// nor saved, not even the level. Before writing, the screen is asked
/// whether the choice can be honoured: `video` needs its file stored
/// (`InvalidInput` otherwise), `album` a card (`Refused(NoCard)`). The
/// catalog is read again right before it is saved, so another writer's
/// changes stay.
pub fn choose(
    link: &mut dyn ScreenLink,
    store: &mut dyn ArchiveStore,
    key: &ScreenKey,
    standby: Standby,
    brightness: Option<Brightness>,
    confirm: Confirm,
) -> Result<PlanB> {
    let confirmed = Confirmed::require(confirm, &Operation::Standby(standby.clone()))?;
    ensure_supported(link)?;
    let record = record_of(store, key)?;
    let plan = standby.plan_b(record.boot_media().start_mode());
    if record.standby == Standby::Keep && standby == Standby::Keep {
        return Ok(plan);
    }
    honoured(storage_of(link)?, &standby)?;
    if let Some(level) = brightness {
        link.set_brightness(level)?;
    }
    storage_of(link)?.set_options(plan, confirmed)?;
    let mut catalog = store.load()?;
    let record = catalog.screen_mut(key);
    record.standby = standby;
    record.stored = Some(StoredPlanB { plan, brightness });
    store.save(&catalog)?;
    Ok(plan)
}

/// Whether the screen can honour `standby` now (queries only): `video`
/// needs its file stored in a video folder, `album` a card.
fn honoured(storage: &mut dyn ScreenStorage, standby: &Standby) -> Result<()> {
    match standby {
        Standby::Keep | Standby::Off(_) => Ok(()),
        Standby::Video(path) => {
            if path.location.kind != MediaKind::Video {
                return Err(BezelError::InvalidInput(format!("{path} is not a video")));
            }
            ensure_stored(storage, path)
        }
        Standby::Album => match storage.info()?.card {
            Some(_) => Ok(()),
            None => Err(BezelError::Refused(Refusal::NoCard)),
        },
    }
}

/// The choice the catalog of `store` records for `key` (`keep` without a
/// record), as [`at_shutdown`] applies it. Only reads the store.
fn recorded_choice(store: &mut dyn ArchiveStore, key: &ScreenKey) -> Result<RecordedChoice> {
    let catalog = store.load()?;
    Ok(RecordedChoice::of(catalog.screen(key)))
}

/// Applies the choice the catalog of `store` records for the screen behind
/// `link` (keyed `key`; `keep` without a record) as the computer shuts down
/// (D-2026-10-03-power-off-standby-3), sending only what the choice says
/// and waiting for nothing the screen does afterwards:
/// - `keep`: nothing;
/// - `off`: [`ScreenLink::turn_off_now`];
/// - `video`: a size query, then its file loops ([`ScreenStorage::play_video`]
///   with [`Repeat::Loop`]);
/// - `album`: a storage info query, then the level stored with the last
///   plan B when the user chose one ([`StoredPlanB::brightness`]), the
///   plan B of `album` (start mode 1, no timer,
///   [`ScreenStorage::set_options`], which carries that level) and
///   [`ScreenStorage::restart`];
/// - a video that is gone or an album without a card: `turn_off_now`
///   instead ([`Applied::TurnedOffInstead`]).
///
/// The choice is read here, from the catalog, and never comes from the
/// caller's hands: the OPTIONS and RESTART run under the confirmation the
/// user gave when [`choose`] recorded it (D-2026-10-03-power-off-standby-2
/// (5)), the one use case that records a choice, and only after
/// `Confirm::Yes`. What this guarantees ends at the port: the core cannot
/// tell the shared catalog from a store an adapter fills by itself, so
/// that every choice a store holds was confirmed rests on the writers of
/// the catalog and on code review (D-2026-10-03-power-off-standby-6 (3)).
/// Neither the choice as the shutdown takes it nor its reading is public:
///
/// ```compile_fail
/// use bezel_core::domain::standby::RecordedChoice;
/// ```
///
/// ```compile_fail
/// use bezel_core::app::standby::recorded_choice;
/// ```
///
/// The store is only read, but for `album` when the record says another
/// plan B was stored last (a boot media set after the choice): the album's
/// is recorded then, after the restart ([`ScreenRecord::stored`]). The
/// screen restarts all the same, so a catalog that cannot be read again or
/// saved then is [`Applied::AlbumNotRecorded`], not an error.
/// A family without the choice: `Unsupported`, nothing sent.
pub fn at_shutdown(
    link: &mut dyn ScreenLink,
    store: &mut dyn ArchiveStore,
    key: &ScreenKey,
) -> Result<Applied> {
    let choice = recorded_choice(store, key)?;
    let standby = choice.standby();
    if *standby == Standby::Keep {
        return Ok(Applied::Nothing);
    }
    ensure_supported(link)?;
    match standby {
        Standby::Keep => Ok(Applied::Nothing),
        Standby::Off(_) => {
            link.turn_off_now()?;
            Ok(Applied::TurnedOff)
        }
        Standby::Video(path) => loop_video(link, path),
        Standby::Album => restart_into_album(link, &choice, store, key),
    }
}

/// `video` at shutdown: loops `path` when it is stored.
fn loop_video(link: &mut dyn ScreenLink, path: &RemotePath) -> Result<Applied> {
    let storage = storage_of(link)?;
    let stored =
        path.location.kind == MediaKind::Video && presence(storage, path)? != Presence::Absent;
    if !stored {
        return turned_off_instead(link, Unavailable::NoVideo);
    }
    storage.play_video(path, Repeat::Loop)?;
    Ok(Applied::Video(path.clone()))
}

/// `album` at shutdown: start mode 1 at the level the user stored with the
/// plan B (review W4: `--brightness` holds after the restart too), and a
/// restart, with a card; then the record says that plan B was stored last,
/// or the answer says it could not (review W2 of iteration 3: the screen
/// restarted all the same).
fn restart_into_album(
    link: &mut dyn ScreenLink,
    choice: &RecordedChoice,
    store: &mut dyn ArchiveStore,
    key: &ScreenKey,
) -> Result<Applied> {
    if storage_of(link)?.info()?.card.is_none() {
        return turned_off_instead(link, Unavailable::NoCard);
    }
    if let Some(level) = choice.brightness() {
        link.set_brightness(level)?;
    }
    let storage = storage_of(link)?;
    let plan = choice.standby().plan_b(StartMode::Default);
    storage.set_options(plan, Confirmed::recorded(choice))?;
    storage.restart(Confirmed::recorded(choice))?;
    let brightness = choice.brightness();
    match record_stored(store, key, StoredPlanB { plan, brightness }) {
        Ok(()) => Ok(Applied::Album),
        Err(error) => Ok(Applied::AlbumNotRecorded(error)),
    }
}

/// Records `stored` as the plan B last stored on the screen of `key`
/// ([`ScreenRecord::stored`]) unless the record already says it (review
/// W5: the album's restart rewrites a boot media's plan B set after the
/// choice). The catalog is read again right before it is saved.
fn record_stored(store: &mut dyn ArchiveStore, key: &ScreenKey, stored: StoredPlanB) -> Result<()> {
    let mut catalog = store.load()?;
    let record = catalog.screen_mut(key);
    let said = StoredPlanB {
        plan: record.plan_b(),
        brightness: record.start_brightness(),
    };
    if said == stored {
        return Ok(());
    }
    record.stored = Some(stored);
    store.save(&catalog)
}

/// A choice that cannot be honoured: the screen is turned off instead.
fn turned_off_instead(link: &mut dyn ScreenLink, reason: Unavailable) -> Result<Applied> {
    link.turn_off_now()?;
    Ok(Applied::TurnedOffInstead(reason))
}
