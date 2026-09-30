//! Drivers: each family's handshake and frame pipeline over a [`crate::wire::Wire`].

use std::time::Duration;

use bezel_core::domain::device::DeviceModel;
use bezel_core::domain::frame::Frame;
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::job::{Job, JobPhase, Progress};
use bezel_core::domain::media::MediaKind;
use bezel_core::domain::storage::{
    DEVICE_SIZE_LIMIT, FileName, MAX_PATH_BYTES, Medium, RemotePath, StorageLocation,
};
use bezel_core::{BezelError, Result};

pub mod kipye_rev_d;
pub mod turing_rev_a;
pub mod turing_rev_c;
pub mod turing_usb;
pub mod wch;
pub mod weact;
pub mod xuanfang_rev_b;

/// Pauses between protocol steps. The fake used in tests does not sleep.
pub trait Pause: Send {
    /// Waits `d`.
    fn pause(&self, d: Duration);
}

/// Real time.
#[derive(Debug, Clone, Copy, Default)]
pub struct RealTime;

impl Pause for RealTime {
    fn pause(&self, d: Duration) {
        std::thread::sleep(d);
    }
}

/// A transport failure, as the domain names it.
pub(crate) fn io_err(e: std::io::Error) -> BezelError {
    BezelError::Transport(e.to_string())
}

/// Frames must be the size the screen shows in the current orientation.
pub(crate) fn check_frame_size(frame: &Frame, expected: Size) -> Result<()> {
    if frame.size() == expected {
        return Ok(());
    }
    Err(BezelError::InvalidInput(format!(
        "frame is {}x{}, the screen expects {}x{} in this orientation",
        frame.size().width,
        frame.size().height,
        expected.width,
        expected.height
    )))
}

/// [`check_frame_size`] for `model`'s panel in `orientation`.
pub(crate) fn check_frame(
    model: &DeviceModel,
    orientation: Orientation,
    frame: &Frame,
) -> Result<()> {
    check_frame_size(frame, model.panel.in_orientation(orientation))
}

// Storage helpers shared by the families with storage (rev C, TUR_USB).

/// Image folder inside a storage root (rev C § 13.1, TUR_USB § 6).
const IMAGE_FOLDER: &str = "img/";
/// Video folder inside a storage root (rev C § 13.1, TUR_USB § 6).
const VIDEO_FOLDER: &str = "video/";
/// Start of the file names in a listing reply (`...file:<f1>/<f2>/...`).
const LISTING_PREFIX: &str = "file:";
/// Listing reply of a folder the firmware just created (it was missing).
const FOLDER_CREATED: &str = "nodir-createdone";

/// Where a family keeps its media folders on the device: `<root>img/` and
/// `<root>video/` on the internal flash and on the card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StorageRoots {
    /// Root on the internal flash, with its trailing `/`.
    pub(crate) internal: &'static str,
    /// Root on the memory card, with its trailing `/`.
    pub(crate) card: &'static str,
}

impl StorageRoots {
    /// Device path of `location`'s folder, with its trailing `/`.
    pub(crate) fn folder(&self, location: StorageLocation) -> String {
        let root = match location.medium {
            Medium::Internal => self.internal,
            Medium::Card => self.card,
        };
        let folder = match location.kind {
            MediaKind::Image => IMAGE_FOLDER,
            MediaKind::Video => VIDEO_FOLDER,
        };
        format!("{root}{folder}")
    }

    /// Device path of a stored file; `InvalidInput` when it is longer than
    /// any storage command can carry.
    pub(crate) fn path(&self, path: &RemotePath) -> Result<String> {
        let full = format!("{}{}", self.folder(path.location), path.name);
        if full.len() > MAX_PATH_BYTES {
            return Err(BezelError::InvalidInput(format!(
                "{path}: the device path has {} bytes (at most {MAX_PATH_BYTES})",
                full.len()
            )));
        }
        Ok(full)
    }
}

/// The names in a listing reply (`...file:<f1>/<f2>/...`, or
/// `nodir-createdone` for a folder the firmware just created). Empty names
/// and names that are not valid [`FileName`]s are skipped; NUL bytes and
/// control characters around names are dropped. `None` when the reply is
/// neither form (the caller asks again).
pub(crate) fn parse_listing(reply: &[u8]) -> Option<Vec<FileName>> {
    let text = String::from_utf8_lossy(reply).replace('\0', "");
    let Some((_, names)) = text.split_once(LISTING_PREFIX) else {
        return text.contains(FOLDER_CREATED).then(Vec::new);
    };
    let names = names
        .split('/')
        .map(|name| name.trim_matches(|c: char| c.is_ascii_control()))
        .filter(|name| !name.is_empty())
        .filter_map(|name| FileName::parse(name).ok())
        .collect();
    Some(names)
}

/// Checks that `data` can be uploaded: not empty, and below the size the
/// screens parse as a signed 32-bit number. Returns its size.
pub(crate) fn upload_size(data: &[u8]) -> Result<u32> {
    let size = u32::try_from(data.len())
        .ok()
        .filter(|&n| n > 0 && u64::from(n) < DEVICE_SIZE_LIMIT);
    size.ok_or_else(|| {
        BezelError::InvalidInput(format!(
            "a {}-byte file cannot be stored (1 byte to {} bytes)",
            data.len(),
            DEVICE_SIZE_LIMIT - 1
        ))
    })
}

/// How a data phase sent by [`send_in_chunks`] ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sent {
    /// Every byte went out.
    All,
    /// The job was cancelled between two chunks.
    Cancelled {
        /// Bytes of the data sent before the cancel.
        accepted: u64,
    },
}

/// Sends `data` through `send` in chunks of `chunk_len` bytes (the last one
/// shorter), for an upload's data phase. Reports [`JobPhase::Upload`]
/// progress (bytes sent / `data.len()`) before the first chunk and after
/// every chunk, and checks the job's cancel token before every chunk.
pub(crate) fn send_in_chunks(
    data: &[u8],
    chunk_len: usize,
    job: &mut Job<'_>,
    mut send: impl FnMut(&[u8]) -> Result<()>,
) -> Result<Sent> {
    let total = data.len() as u64;
    let mut accepted = 0;
    job.report(Progress::new(JobPhase::Upload, accepted, total));
    for chunk in data.chunks(chunk_len.max(1)) {
        if job.is_cancelled() {
            return Ok(Sent::Cancelled { accepted });
        }
        send(chunk)?;
        accepted += chunk.len() as u64;
        job.report(Progress::new(JobPhase::Upload, accepted, total));
    }
    Ok(Sent::All)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::domain::frame::Rgba;
    use bezel_core::domain::job::CancelToken;

    const ROOTS: StorageRoots = StorageRoots {
        internal: "/mnt/UDISK/",
        card: "/mnt/SDCARD/",
    };

    #[test]
    fn storage_roots_map_the_four_folders() {
        let folders: Vec<String> = StorageLocation::ALL
            .iter()
            .map(|l| ROOTS.folder(*l))
            .collect();
        assert_eq!(
            folders,
            [
                "/mnt/UDISK/img/",
                "/mnt/UDISK/video/",
                "/mnt/SDCARD/img/",
                "/mnt/SDCARD/video/"
            ]
        );
        let clip = RemotePath::parse("sd/video/88.mp4").unwrap();
        assert_eq!(ROOTS.path(&clip).unwrap(), "/mnt/SDCARD/video/88.mp4");
        // A longest upload name fits under the longest root of every family.
        let longest = StorageRoots {
            internal: "/usr/data/",
            card: "/tmp/sdcard/mmcblk0p1/",
        };
        let name = "a".repeat(FileName::MAX_BYTES);
        let path = RemotePath::parse(&format!("sd/video/{name}")).unwrap();
        assert_eq!(longest.path(&path).unwrap().len(), MAX_PATH_BYTES);
        // A listed name may be longer than an upload name; it is refused, not cut.
        let wide = StorageRoots {
            internal: "/",
            card: "/a-root-longer-than-any-family-has/",
        };
        assert!(matches!(wide.path(&path), Err(BezelError::InvalidInput(_))));
    }

    #[test]
    fn listings_parse_names_and_created_folders() {
        let names = |reply: &[u8]| {
            parse_listing(reply).map(|n| n.iter().map(ToString::to_string).collect::<Vec<_>>())
        };
        assert_eq!(
            names(b"xx file:88.mp4/clip.mp4//\0\0"),
            Some(vec!["88.mp4".into(), "clip.mp4".into()])
        );
        assert_eq!(names(b"file:"), Some(vec![]));
        assert_eq!(names(b"nodir-createdone"), Some(vec![]));
        assert_eq!(
            names(b"file:ok.png/bad\\name.png/\r\nlast.gif\n"),
            Some(vec!["ok.png".into(), "last.gif".into()]),
            "invalid names are skipped, control characters dropped"
        );
        assert_eq!(names(b""), None);
        assert_eq!(names(b"media_stop"), None);
    }

    #[test]
    fn upload_sizes_fit_the_devices_int32() {
        assert_eq!(upload_size(&[1, 2, 3]), Ok(3));
        assert!(matches!(upload_size(&[]), Err(BezelError::InvalidInput(_))));
    }

    #[test]
    fn chunks_report_progress_and_stop_when_cancelled() {
        let token = CancelToken::new();
        let remote = token.clone();
        let mut seen = Vec::new();
        let mut sink = |p: Progress| {
            seen.push((p.done, p.total));
            if p.done >= 4 {
                remote.cancel();
            }
        };
        let mut job = Job::new(&token, &mut sink);
        let mut sent = Vec::new();
        let outcome = send_in_chunks(&[1, 2, 3, 4, 5, 6, 7, 8, 9], 4, &mut job, |c| {
            sent.push(c.to_vec());
            Ok(())
        });
        assert_eq!(outcome, Ok(Sent::Cancelled { accepted: 4 }));
        assert_eq!(sent, [vec![1, 2, 3, 4]]);
        assert_eq!(seen, [(0, 9), (4, 9)]);

        let token = CancelToken::new();
        let mut sink = |_: Progress| {};
        let mut job = Job::new(&token, &mut sink);
        let mut count = 0;
        let outcome = send_in_chunks(&[0; 9], 4, &mut job, |_| {
            count += 1;
            Ok(())
        });
        assert_eq!((outcome, count), (Ok(Sent::All), 3));
        let failed = send_in_chunks(&[0; 9], 4, &mut job, |_| {
            Err(BezelError::Transport("unplugged".into()))
        });
        assert!(matches!(failed, Err(BezelError::Transport(_))));
    }

    #[test]
    fn frame_size_errors_are_invalid_input() {
        let frame = Frame::filled(Size::new(2, 3), Rgba::BLACK);
        assert!(check_frame_size(&frame, Size::new(2, 3)).is_ok());
        let err = check_frame_size(&frame, Size::new(3, 2)).unwrap_err();
        assert!(matches!(err, BezelError::InvalidInput(_)), "{err}");
        assert!(err.to_string().contains("2x3"), "{err}");
        RealTime.pause(Duration::ZERO);
        assert!(matches!(
            io_err(std::io::Error::other("x")),
            BezelError::Transport(_)
        ));
    }
}
