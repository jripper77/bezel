//! Shutdown actions run through the worker's existing link, never by reopening
//! a screen while it is rendering. Normal exits still use `release`.
use bezel_core::app::standby::at_shutdown;
use bezel_core::domain::archive::ScreenKey;
use bezel_core::domain::standby::supports;
use bezel_core::ports::{ArchiveStore, ScreenLink};
use bezel_core::{BezelError, Result};
use bezel_media::archive::{DiskArchive, storage_dir};

pub(crate) fn finish(link: &mut dyn ScreenLink) -> Result<()> {
    if !supports(link.identity().model) {
        return link.turn_off_now();
    }
    let data = crate::theme::data_home(|name| std::env::var_os(name))
        .ok_or_else(|| BezelError::Transport("cannot find Bezel's shutdown catalog".into()))?;
    let mut archive = DiskArchive::open(storage_dir(&data))?;
    apply(link, &mut archive)
}

fn apply(link: &mut dyn ScreenLink, archive: &mut dyn ArchiveStore) -> Result<()> {
    if supports(link.identity().model) {
        let key = ScreenKey::new(link.identity().model.id);
        at_shutdown(link, archive, &key)?;
        Ok(())
    } else {
        link.turn_off_now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::app::choose_screen;
    use bezel_core::app::open_screen;
    use bezel_core::domain::archive::Catalog;
    use bezel_core::domain::device::{Transport, UsbId};
    use bezel_core::domain::discovery::{DeviceAddress, Endpoint};
    use bezel_core::domain::standby::{SleepMinutes, Standby};
    use bezel_core::domain::storage::{RemotePath, Repeat};
    use bezel_core::ports::ScreenConnector;
    use bezel_devices::fake::{FakeStorage, StorageCall};
    use bezel_devices::{FakeBus, FakeConnector};
    use bezel_media::archive::MemoryArchive;

    #[test]
    fn shutdown_turns_off_rev_a_without_releasing_it() {
        let bus = FakeBus::new(vec![Endpoint {
            address: DeviceAddress("COM3".into()),
            transport: Transport::Serial,
            usb: UsbId::new(0x1a86, 0x5722),
            serial_number: None,
            manufacturer: None,
            product: None,
            location: None,
        }]);
        let connector = FakeConnector::default();
        let mut screen =
            choose_screen(bezel_core::app::discover_screens(&bus).unwrap(), None).unwrap();
        screen.candidates.retain(|model| model.id.0 == "turing-3.5");
        let mut link = connector.connect(&screen).unwrap();
        apply(link.as_mut(), &mut MemoryArchive::new()).unwrap();
        assert_eq!(connector.log().storage.calls, vec![StorageCall::TurnOffNow]);
        assert_eq!(connector.log().releases, 0);
    }

    #[test]
    fn shutdown_honours_off_keep_and_video_in_the_saved_catalog() {
        let video = RemotePath::parse("internal/video/intro.mp4").unwrap();
        for choice in [
            Standby::Off(SleepMinutes::SUGGESTED),
            Standby::Keep,
            Standby::Video(video.clone()),
        ] {
            let connector = FakeConnector::with_storage(
                FakeStorage::default().with_file(video.clone(), vec![1; 10]),
            );
            let mut link = open_screen(&FakeBus::turing_88(), &connector, None).unwrap();
            let mut catalog = Catalog::default();
            catalog
                .screen_mut(&ScreenKey::new(link.identity().model.id))
                .standby = choice.clone();
            apply(link.as_mut(), &mut MemoryArchive::with_catalog(catalog)).unwrap();
            let log = connector.log();
            assert_eq!(log.releases, 0, "release would undo the shutdown choice");
            match choice {
                Standby::Off(_) => assert_eq!(log.storage.calls, vec![StorageCall::TurnOffNow]),
                Standby::Keep => assert!(log.storage.calls.is_empty()),
                Standby::Video(_) => assert!(log.storage.calls.iter().any(|call| matches!(call, StorageCall::PlayVideo(path, Repeat::Loop) if *path == video))),
                _ => unreachable!(),
            }
        }
    }
}
