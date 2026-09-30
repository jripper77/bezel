//! Driver for Turing rev C screens (2.1"/2.8"/5"/8.8" UART generation):
//! frames, and the stored files and device-side playback of spec § 13.
//!
//! Nothing storage-related is sent implicitly (spec § 16): the storage
//! commands, OPTIONS 0x7D and playback go out only from the
//! [`ScreenStorage`] methods, which the core's use cases call. 0x81, 0x82
//! and 0x84 are never sent. On small screens the vendor sends 0x82 and
//! re-initialises before PLAY_VIDEO; Bezel does not (disruptive, and no
//! small screen has been validated).
//!
//! A cancelled upload has no abort in the protocol: after the UPLOAD_FILE
//! header the firmware takes every byte as file data until it has the
//! declared length, and only that length ends the data phase cleanly (spec
//! § 19). A HELLO it answers after a cancel proves nothing: on the 8.8" the
//! bytes still queued for its writer then went into the next file. So the
//! recovery always sends the rest of the declared length as filler blocks
//! first, then HELLO, then measures the partial file (declared size: the data
//! sent, then filler) that the user is offered to delete.

use std::time::Duration;

use bezel_core::domain::device::DeviceModel;
use bezel_core::domain::frame::{Frame, RGBA_BYTES};
use bezel_core::domain::geometry::Orientation;
use bezel_core::domain::job::{Job, JobPhase, Progress};
use bezel_core::domain::media::MediaKind;
use bezel_core::domain::screen::{Brightness, ScreenIdentity};
use bezel_core::domain::storage::{
    Confirmed, FileName, Medium, RemotePath, Repeat, StartMode, StorageInfo, StorageLocation,
};
use bezel_core::ports::{ScreenLink, ScreenStorage};
use bezel_core::{BezelError, Result};

use crate::driver::{
    Pause, Sent, StorageRoots, check_frame, io_err, parse_listing, send_in_chunks, upload_size,
};
use crate::protocol::turing_rev_c::{
    self as proto, BLOCK, Hello, Options, PixelFormat, ScreenClass, Status, StorageReport, op,
    reply, root,
};
use crate::wire::Wire;

/// How long the device may take to answer HELLO, QUERY_STATUS, STOP_MEDIA,
/// GET_STORAGE_INFO and LIST_DIR.
const REPLY_TIMEOUT: Duration = Duration::from_millis(1000);
/// Longest reply read at once (the spec's "R 1024").
const REPLY_MAX: usize = 1024;
/// Longest LIST_DIR reply (the vendor reads up to 10,240 bytes).
const LIST_REPLY_MAX: usize = 10_240;
/// How long the reply a full frame may get (`full_png_sucess`) is waited for.
const FRAME_REPLY_WAIT: Duration = Duration::from_millis(50);
/// HELLO attempts before giving up.
const HELLO_TRIES: usize = 3;
/// Pause between failed HELLO attempts (after a resync block).
const HELLO_RETRY_PAUSE: Duration = Duration::from_millis(1000);
/// Pause after STOP_VIDEO before the first STOP_MEDIA (spec § 7.2 step 3).
const STOP_VIDEO_SETTLE: Duration = Duration::from_millis(200);
/// STOP_MEDIA polls while waiting for `media_stop`.
const STOP_MEDIA_POLLS: usize = 20;
/// Pause between two STOP_MEDIA polls.
const STOP_MEDIA_POLL_PAUSE: Duration = Duration::from_millis(400);
/// Sends of GET_STORAGE_INFO, LIST_DIR and GET_FILE_SIZE before giving up.
const QUERY_TRIES: usize = 3;
/// How long GET_FILE_SIZE may take to answer.
const FILE_SIZE_TIMEOUT: Duration = Duration::from_secs(2);
/// How long UPLOAD_FILE may take to answer `create_success`.
const CREATE_TIMEOUT: Duration = Duration::from_secs(3);
/// UPLOAD_FILE headers sent per upload (the vendor never resends one).
const CREATE_TRIES: usize = 1;
/// Protocol blocks per write of an upload's data phase. The vendor writes
/// one 250-byte block per call; grouping them keeps the bytes on the wire
/// identical and the number of writes low. Progress and cancellation are
/// per write.
const UPLOAD_CHUNK_BLOCKS: usize = 256;
/// File bytes per write: a whole number of blocks, so the writes together
/// are exactly the framing of the whole file.
const UPLOAD_CHUNK: usize = UPLOAD_CHUNK_BLOCKS * proto::BLOCK_PAYLOAD;
/// One wait for `file_rev_done` after an upload into the card's videos.
const RECEIVED_TIMEOUT_CARD_VIDEO: Duration = Duration::from_secs(10);
/// One wait for `file_rev_done` after an upload anywhere else.
const RECEIVED_TIMEOUT: Duration = Duration::from_secs(240);
/// Waits for `file_rev_done` on large screens.
const RECEIVED_ROUNDS_LARGE: usize = 15;
/// Waits for `file_rev_done` on small screens.
const RECEIVED_ROUNDS_SMALL: usize = 1;
/// Pause between two waits for `file_rev_done`.
const RECEIVED_ROUND_PAUSE: Duration = Duration::from_millis(200);
/// Reads within one wait for `file_rev_done`: the cancel token is checked
/// between them.
const RECEIVED_POLL: Duration = Duration::from_secs(1);
/// How long PLAY_VIDEO may take to answer `play_video_success`.
const PLAY_VIDEO_TIMEOUT: Duration = Duration::from_secs(6);
/// PLAY_VIDEO sends before giving up.
const PLAY_VIDEO_TRIES: usize = 2;
/// How long PLAY_IMAGE may take to answer `play_img_ok`.
const PLAY_IMAGE_TIMEOUT: Duration = Duration::from_secs(3);
/// PLAY_IMAGE sends before giving up.
const PLAY_IMAGE_TRIES: usize = 1;
/// Brightness written into OPTIONS when this link has sent none (the
/// vendor's default setting, spec § 5).
const DEFAULT_STORED_BRIGHTNESS: u8 = 170;
/// After TURNOFF: reads that wait for the SoC to leave the bus (at most
/// `OFF_POLLS` × `OFF_POLL`; a read error means it is gone).
const OFF_POLLS: usize = 16;
/// One of those reads.
const OFF_POLL: Duration = Duration::from_millis(250);
/// After the filler that completes a cancelled upload's data phase: reads
/// of [`RECEIVED_POLL`] that wait for `file_rev_done` (the firmware wrote
/// the rest and closed the file) before HELLO is asked.
const FILLED_POLLS: usize = 10;

/// How long a request waits for its reply, how many times it is sent and
/// how many reply bytes are read.
#[derive(Debug, Clone, Copy)]
struct Wait {
    timeout: Duration,
    tries: usize,
    max: usize,
}

const QUERY: Wait = Wait {
    timeout: REPLY_TIMEOUT,
    tries: QUERY_TRIES,
    max: REPLY_MAX,
};
const LISTING: Wait = Wait {
    timeout: REPLY_TIMEOUT,
    tries: QUERY_TRIES,
    max: LIST_REPLY_MAX,
};
const SIZE: Wait = Wait {
    timeout: FILE_SIZE_TIMEOUT,
    tries: QUERY_TRIES,
    max: REPLY_MAX,
};
const CREATE: Wait = Wait {
    timeout: CREATE_TIMEOUT,
    tries: CREATE_TRIES,
    max: REPLY_MAX,
};
const PLAY_VIDEO: Wait = Wait {
    timeout: PLAY_VIDEO_TIMEOUT,
    tries: PLAY_VIDEO_TRIES,
    max: REPLY_MAX,
};
const PLAY_IMAGE: Wait = Wait {
    timeout: PLAY_IMAGE_TIMEOUT,
    tries: PLAY_IMAGE_TRIES,
    max: REPLY_MAX,
};

/// The card's video folder, whose uploads get the short completion wait.
const CARD_VIDEO: StorageLocation = StorageLocation::new(Medium::Card, MediaKind::Video);

/// How far an upload's data phase got on the wire. After the header the
/// firmware takes every byte as file data until it has the declared length
/// and has no abort (spec § 19): whatever follows a cancel, a HELLO
/// included, lands in the file until then, and only that length closes the
/// file cleanly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DataPhase {
    /// Bytes of the file the UPLOAD_FILE header declared.
    size: u64,
    /// Wire bytes the UPLOAD_FILE header declared.
    declared: u64,
    /// Wire bytes sent since the header. A write the cancel cut short counts
    /// whole, so the bytes still owed are never overestimated and the
    /// filler never passes the declared length.
    sent: u64,
}

impl DataPhase {
    /// The data phase of a `size`-byte file, nothing sent yet.
    fn of(size: u32) -> Self {
        let size = u64::from(size);
        Self {
            size,
            declared: proto::data_phase_len(size),
            sent: 0,
        }
    }

    /// `bytes` more went out.
    fn after(self, bytes: u64) -> Self {
        Self {
            sent: self.sent.saturating_add(bytes),
            ..self
        }
    }

    /// Every declared byte went out.
    fn finished(self) -> Self {
        Self {
            sent: self.declared,
            ..self
        }
    }

    /// Whole blocks the firmware still waits for: 0 once the declared length
    /// went out (every write is whole blocks, so nothing is rounded away).
    fn owed_blocks(self) -> u64 {
        self.declared.saturating_sub(self.sent) / BLOCK as u64
    }

    /// Upload progress at this point: bytes of the file the screen got (the
    /// data, then any filler), at most the file size.
    fn progress(self) -> Progress {
        let blocks = self.sent / BLOCK as u64;
        let done = (blocks * proto::BLOCK_PAYLOAD as u64).min(self.size);
        Progress::new(JobPhase::Upload, done, self.size)
    }
}

/// A connected rev C screen.
pub struct TuringRevC<W: Wire, P: Pause> {
    wire: W,
    pause: P,
    identity: ScreenIdentity,
    format: PixelFormat,
    class: ScreenClass,
    orientation: Orientation,
    last: Option<Vec<u8>>,
    seq: u32,
    /// PRE_UPDATE_BITMAP went out since device-side media last changed.
    streaming: bool,
    /// The OPTIONS fields to send: the last brightness this link sent, and
    /// the start mode, flip and sleep delay of its last OPTIONS.
    options: Options,
}

impl<W: Wire, P: Pause + Clone> TuringRevC<W, P> {
    /// Handshakes over `wire` and prepares the screen for streaming.
    /// `candidates` are the models discovery allowed; the HELLO answer picks one.
    pub fn connect(mut wire: W, pause: &P, candidates: &[&'static DeviceModel]) -> Result<Self> {
        let hello = handshake(&mut wire, pause)?;
        tracing::debug!(reply = %hello.raw, rom = hello.rom, "HELLO");
        let model = pick_model(&hello, candidates).ok_or_else(|| {
            BezelError::Transport(format!("unexpected screen model: {}", hello.raw))
        })?;
        let mut screen = Self {
            wire,
            pause: pause.clone(),
            format: hello.partial_format(),
            class: ScreenClass::of(&model.id),
            identity: ScreenIdentity {
                model,
                firmware: Some(hello.raw),
            },
            orientation: Orientation::Portrait,
            last: None,
            seq: 0,
            streaming: false,
            options: Options {
                brightness: DEFAULT_STORED_BRIGHTNESS,
                start_mode: proto::StartMode::Default,
                flip: false,
                sleep_minutes: 0,
            },
        };
        screen.stop_media()?;
        screen.enter_streaming()?;
        Ok(screen)
    }
}

impl<W: Wire, P: Pause> TuringRevC<W, P> {
    /// The wire, for tests and diagnostics.
    pub fn wire(&self) -> &W {
        &self.wire
    }

    fn send(&mut self, bytes: &[u8]) -> Result<()> {
        self.wire.send(bytes).map_err(io_err)
    }

    fn enter_streaming(&mut self) -> Result<()> {
        self.send(&proto::simple(op::PRE_UPDATE_BITMAP))?;
        self.streaming = true;
        Ok(())
    }

    /// STOP_VIDEO, then STOP_MEDIA until the device says `media_stop`.
    fn stop_media(&mut self) -> Result<()> {
        self.send(&proto::simple(op::STOP_VIDEO))?;
        self.pause.pause(STOP_VIDEO_SETTLE);
        for poll in 1..=STOP_MEDIA_POLLS {
            self.send(&proto::simple(op::STOP_MEDIA))?;
            let answer = self
                .wire
                .receive(REPLY_MAX, REPLY_TIMEOUT)
                .map_err(io_err)?;
            if String::from_utf8_lossy(&answer).contains(reply::MEDIA_STOPPED) {
                return Ok(());
            }
            if poll < STOP_MEDIA_POLLS {
                self.pause.pause(STOP_MEDIA_POLL_PAUSE);
            }
        }
        // Older firmware never answers; streaming still works.
        tracing::debug!("no media_stop answer; continuing");
        Ok(())
    }

    /// Device-side media is about to change: the next frame is a full one,
    /// preceded by PRE_UPDATE_BITMAP as at a theme start (spec § 7.2).
    fn media_changed(&mut self) {
        self.last = None;
        self.streaming = false;
    }

    /// Stops device-side playback (before uploads, plays and on request).
    fn stop_playback(&mut self) -> Result<()> {
        self.media_changed();
        self.stop_media()
    }

    fn native(&self, frame: &Frame) -> Result<Vec<u8>> {
        let model = self.identity.model;
        check_frame(model, self.orientation, frame)?;
        let turns = self.orientation.quarter_turns_to(model.native_orientation);
        Ok(rgba_to_bgra(frame.rotated(turns).as_rgba()))
    }

    fn full_frame(&mut self, bgra: Vec<u8>) -> Result<()> {
        if !self.streaming {
            self.enter_streaming()?;
        }
        self.send(&proto::start_display_block())?;
        self.send(&proto::full_frame_header(bgra.len() as u32))?;
        self.send(&proto::blocks(&bgra))?;
        // The device may say something after a frame; drain it so it does not
        // pollute the next reply.
        let after = self
            .wire
            .receive(REPLY_MAX, FRAME_REPLY_WAIT)
            .map_err(io_err)?;
        tracing::debug!(bytes = bgra.len(), reply = %printable(&after), "full frame");
        self.last = Some(bgra);
        self.seq = 0;
        Ok(())
    }

    fn partial(&mut self, mut list: Vec<u8>, bgra: Vec<u8>) -> Result<()> {
        list.extend_from_slice(&proto::MAGIC);
        self.send(&proto::partial_header(list.len() as u32, self.seq))?;
        self.send(&proto::blocks(&list))?;
        self.seq = self.seq.wrapping_add(1);
        self.last = Some(bgra);
        Ok(())
    }

    /// QUERY_STATUS round-trip; `true` when the device asks for a full frame.
    fn needs_full_frame(&mut self) -> Result<bool> {
        self.send(&proto::simple(op::QUERY_STATUS))?;
        let answer = self
            .wire
            .receive(REPLY_MAX, REPLY_TIMEOUT)
            .map_err(io_err)?;
        let status = Status::parse(&answer);
        tracing::debug!(reply = %printable(&answer), seq = self.seq, "QUERY_STATUS");
        Ok(status.is_some_and(|s| s.need_resend))
    }

    /// Sends `packet` (stale input dropped first) until a reply parses, at
    /// most `wait.tries` times. `what` names the request in errors.
    fn request<T>(
        &mut self,
        packet: &[u8],
        wait: Wait,
        what: &str,
        parse: impl Fn(&[u8]) -> Option<T>,
    ) -> Result<T> {
        for attempt in 1..=wait.tries {
            self.wire.discard_input().map_err(io_err)?;
            self.send(packet)?;
            let answer = self.wire.receive(wait.max, wait.timeout).map_err(io_err)?;
            if let Some(value) = parse(&answer) {
                return Ok(value);
            }
            tracing::debug!(attempt, what, reply = %printable(&answer), "unexpected reply");
        }
        Err(BezelError::Timeout(format!(
            "the screen: no valid answer to {what}"
        )))
    }

    fn roots(&self) -> StorageRoots {
        StorageRoots {
            internal: self.class.internal_root(),
            card: root::CARD,
        }
    }

    fn list_folder(&mut self, folder: &str) -> Result<Vec<FileName>> {
        let packet = path_packet(op::LIST_DIR, folder)?;
        self.request(
            &packet,
            LISTING,
            &format!("LIST_DIR {folder}"),
            parse_listing,
        )
    }

    fn file_size(&mut self, target: &str) -> Result<Option<u64>> {
        let packet = path_packet(op::FILE_SIZE, target)?;
        let what = format!("GET_FILE_SIZE {target}");
        let bytes = self.request(&packet, SIZE, &what, proto::file_size)?;
        Ok((bytes > 0).then_some(bytes))
    }

    /// Waits for `file_rev_done` as the vendor does (spec § 13.4); without
    /// it the use case's size check decides. A cancel during the wait
    /// recovers the link like a cancel between blocks (`phase` is finished:
    /// no filler is owed).
    fn await_received(
        &mut self,
        path: &RemotePath,
        job: &mut Job<'_>,
        phase: DataPhase,
    ) -> Result<()> {
        let wait = if path.location == CARD_VIDEO {
            RECEIVED_TIMEOUT_CARD_VIDEO
        } else {
            RECEIVED_TIMEOUT
        };
        let polls = (wait.as_millis() / RECEIVED_POLL.as_millis()).max(1);
        let rounds = match self.class {
            ScreenClass::Large => RECEIVED_ROUNDS_LARGE,
            ScreenClass::Small => RECEIVED_ROUNDS_SMALL,
        };
        for round in 1..=rounds {
            for _ in 0..polls {
                if job.is_cancelled() {
                    return Err(self.recover_after_cancel(path, phase, job));
                }
                let answer = self
                    .wire
                    .receive(REPLY_MAX, RECEIVED_POLL)
                    .map_err(io_err)?;
                if String::from_utf8_lossy(&answer).contains(reply::RECEIVED) {
                    return Ok(());
                }
            }
            if round < rounds {
                self.pause.pause(RECEIVED_ROUND_PAUSE);
            }
        }
        tracing::warn!(%path, "no file_rev_done after the upload; the size check decides");
        Ok(())
    }

    /// After an interrupted upload (spec § 19). The firmware has no abort
    /// and only the declared length ends its data phase cleanly, even when
    /// it would answer a HELLO: the rest of `phase` goes out as filler
    /// first ([`Self::complete_data_phase`]), then HELLO (with its resync
    /// blocks) brings the link back and GET_FILE_SIZE measures what is
    /// left, the declared size once filler went out. Never deletes.
    fn recover_after_cancel(
        &mut self,
        path: &RemotePath,
        phase: DataPhase,
        job: &mut Job<'_>,
    ) -> BezelError {
        self.media_changed();
        let back = self
            .complete_data_phase(phase, job)
            .and_then(|()| handshake(&mut self.wire, &self.pause));
        if let Err(e) = back {
            tracing::warn!(error = %e, %path, "the screen did not come back after a cancelled upload");
            return BezelError::Timeout(format!(
                "the screen after a cancelled upload; the next command reconnects it, then check {path} for a partial file"
            ));
        }
        match self.device_path(path).and_then(|t| self.file_size(&t)) {
            Ok(partial) => BezelError::Cancelled { partial },
            Err(e) => e,
        }
    }

    /// Completes `phase` when blocks of it are still owed: the filler, then
    /// up to [`FILLED_POLLS`] reads for `file_rev_done`. Nothing when the
    /// declared length went out (a cancel during the completion wait).
    fn complete_data_phase(&mut self, phase: DataPhase, job: &mut Job<'_>) -> Result<()> {
        if phase.owed_blocks() == 0 {
            return Ok(());
        }
        self.send_filler(phase, job)?;
        self.await_closed()
    }

    /// Sends the blocks `phase` still owes as filler (`2c` × 249 + `00`
    /// each, the data-phase framing) in writes of [`UPLOAD_CHUNK_BLOCKS`].
    ///
    /// The trade-off: the filler lands in the partial file (reported through
    /// GET_FILE_SIZE with the declared size, for the user to delete; never
    /// deleted here), and sending it takes about as long as the rest of the
    /// upload would have, bounded by the declared length. Every write
    /// reports Upload progress from the cancel point towards the file size:
    /// the counters stay bytes of the file the screen got, which the filler
    /// completes, and the CLI and the studio already say the upload is being
    /// cancelled, so a long filler reads as a cancel at work, not a hang. It
    /// polls no token (the job is already cancelled): a write that fails
    /// (screen unplugged, a write stalled for 10 s) stops it, and the CLI's
    /// second Ctrl+C quits at once; either leaves the screen as a cancel
    /// without filler would (the next connection wakes it; the next upload
    /// may take stray bytes, which its size check catches).
    fn send_filler(&mut self, mut phase: DataPhase, job: &mut Job<'_>) -> Result<()> {
        let blocks = phase.owed_blocks();
        tracing::info!(
            blocks,
            bytes = blocks * BLOCK as u64,
            "cancelled upload: completing its declared length with filler"
        );
        let chunk = proto::filler_blocks(UPLOAD_CHUNK_BLOCKS);
        while phase.owed_blocks() > 0 {
            let n = phase.owed_blocks().min(UPLOAD_CHUNK_BLOCKS as u64) as usize;
            let write = &chunk[..n * BLOCK];
            self.send(write)?;
            phase = phase.after(write.len() as u64);
            job.report(phase.progress());
        }
        Ok(())
    }

    /// Waits (at most [`FILLED_POLLS`] reads) for `file_rev_done`: the
    /// firmware wrote the completed file and closed it. Without it, the
    /// HELLO that follows decides.
    fn await_closed(&mut self) -> Result<()> {
        for _ in 0..FILLED_POLLS {
            let answer = self
                .wire
                .receive(REPLY_MAX, RECEIVED_POLL)
                .map_err(io_err)?;
            if String::from_utf8_lossy(&answer).contains(reply::RECEIVED) {
                return Ok(());
            }
        }
        tracing::debug!("no file_rev_done after the filler");
        Ok(())
    }

    fn device_path(&self, path: &RemotePath) -> Result<String> {
        self.roots().path(path)
    }
}

impl<W: Wire, P: Pause> ScreenLink for TuringRevC<W, P> {
    fn identity(&self) -> &ScreenIdentity {
        &self.identity
    }

    fn set_brightness(&mut self, brightness: Brightness) -> Result<()> {
        let level = brightness.scaled(255) as u8;
        self.send(&proto::set_brightness(level))?;
        self.options.brightness = level;
        Ok(())
    }

    fn set_orientation(&mut self, orientation: Orientation) -> Result<()> {
        if orientation != self.orientation {
            self.orientation = orientation;
            self.last = None;
        }
        Ok(())
    }

    /// Frames keep their alpha per pixel in both pixel formats: over a video
    /// the screen plays, A = 0 shows the video (spec § 13.5).
    fn present(&mut self, frame: &Frame) -> Result<()> {
        let bgra = self.native(frame)?;
        let Some(last) = self.last.as_ref() else {
            return self.full_frame(bgra);
        };
        match proto::diff_runs(last, &bgra, self.format) {
            None => self.full_frame(bgra),
            Some(list) if list.is_empty() => Ok(()),
            Some(list) => {
                self.partial(list, bgra)?;
                if self.needs_full_frame()? {
                    self.last = None;
                }
                Ok(())
            }
        }
    }

    fn screen_off(&mut self) -> Result<()> {
        self.last = None;
        self.send(&proto::simple(op::TURN_OFF))?;
        // The SoC shuts down and leaves the bus; return once it is gone so
        // the next command wakes it instead of racing its shutdown.
        for _ in 0..OFF_POLLS {
            if self.wire.receive(1, OFF_POLL).is_err() {
                break;
            }
        }
        Ok(())
    }

    fn release(&mut self) -> Result<()> {
        self.last = None;
        self.send(&proto::simple(op::END_UPDATE_BITMAP))
    }

    fn storage(&mut self) -> Option<&mut dyn ScreenStorage> {
        Some(self)
    }
}

impl<W: Wire, P: Pause> ScreenStorage for TuringRevC<W, P> {
    fn info(&mut self) -> Result<StorageInfo> {
        let packet = proto::storage_info();
        let report = self.request(&packet, QUERY, "GET_STORAGE_INFO", StorageReport::parse)?;
        tracing::debug!(?report, "GET_STORAGE_INFO");
        Ok(report.info())
    }

    fn list(&mut self, location: StorageLocation) -> Result<Vec<FileName>> {
        let folder = self.roots().folder(location);
        self.list_folder(&folder)
    }

    fn size(&mut self, path: &RemotePath) -> Result<Option<u64>> {
        let target = self.device_path(path)?;
        self.file_size(&target)
    }

    /// Spec § 13.4: STOP_VIDEO, STOP_MEDIA, LIST_DIR of the folder (creates
    /// it), UPLOAD_FILE until `create_success`, the data phase, then the
    /// wait for `file_rev_done`. The use case verifies with GET_FILE_SIZE.
    fn upload(&mut self, path: &RemotePath, data: &[u8], job: &mut Job<'_>) -> Result<()> {
        let size = upload_size(data)?;
        let target = self.device_path(path)?;
        let header = proto::upload_file(&target, size).ok_or_else(|| too_long(&target))?;
        job.checkpoint()?;
        self.stop_playback()?;
        let folder = self.roots().folder(path.location);
        self.list_folder(&folder)?;
        job.checkpoint()?;
        let what = format!("UPLOAD_FILE {target}");
        self.request(&header, CREATE, &what, has(reply::CREATED))?;
        tracing::info!(%target, size, "upload");
        let phase = DataPhase::of(size);
        let wire = &mut self.wire;
        // Wire bytes of a write that failed once the job was cancelled: the
        // signal that cancelled it may have cut it after they left.
        let mut cut = 0;
        let sent = send_in_chunks(data, UPLOAD_CHUNK, job, |chunk| {
            let framed = proto::blocks(chunk);
            wire.send(&framed).map_err(|e| {
                cut = framed.len() as u64;
                io_err(e)
            })
        })?;
        match sent {
            Sent::All => self.await_received(path, job, phase.finished()),
            Sent::Cancelled { accepted } => {
                tracing::info!(%target, accepted, "upload cancelled");
                let phase = phase.after(proto::data_phase_len(accepted) + cut);
                Err(self.recover_after_cancel(path, phase, job))
            }
        }
    }

    fn delete(&mut self, path: &RemotePath, _confirmed: Confirmed) -> Result<()> {
        let target = self.device_path(path)?;
        let packet = path_packet(op::DELETE_FILE, &target)?;
        tracing::info!(%target, "DELETE_FILE");
        self.send(&packet)
    }

    fn play_video(&mut self, path: &RemotePath, repeat: Repeat) -> Result<()> {
        let target = self.device_path(path)?;
        let packet = proto::play_video(&target, repeat).ok_or_else(|| too_long(&target))?;
        self.stop_playback()?;
        let what = format!("PLAY_VIDEO {target}");
        self.request(&packet, PLAY_VIDEO, &what, has(reply::VIDEO_PLAYING))
    }

    fn play_image(&mut self, path: &RemotePath) -> Result<()> {
        let target = self.device_path(path)?;
        let packet = path_packet(op::PLAY_IMAGE, &target)?;
        self.stop_playback()?;
        let what = format!("PLAY_IMAGE {target}");
        self.request(&packet, PLAY_IMAGE, &what, has(reply::IMAGE_SHOWN))
    }

    fn stop(&mut self) -> Result<()> {
        self.stop_playback()
    }

    /// OPTIONS 0x7D with the last brightness this link sent (the vendor
    /// default before any), its flip and sleep delay: only the start mode
    /// changes.
    fn set_start_mode(&mut self, mode: StartMode, _confirmed: Confirmed) -> Result<()> {
        self.options.start_mode = match mode {
            StartMode::Default => proto::StartMode::Default,
            StartMode::Image => proto::StartMode::Image,
            StartMode::Video => proto::StartMode::Video,
        };
        tracing::info!(options = ?self.options, "OPTIONS");
        self.send(&proto::set_options(self.options))
    }
}

/// A reply check: the text contains `needle`.
fn has(needle: &'static str) -> impl Fn(&[u8]) -> Option<()> {
    move |answer| {
        String::from_utf8_lossy(answer)
            .contains(needle)
            .then_some(())
    }
}

/// A packet naming `target`, or `InvalidInput` when the path is too long.
fn path_packet(opcode: u8, target: &str) -> Result<[u8; BLOCK]> {
    proto::path_command(opcode, target).ok_or_else(|| too_long(target))
}

fn too_long(target: &str) -> BezelError {
    BezelError::InvalidInput(format!("{target}: too long for one command packet"))
}

/// A reply for logs: printable ASCII only.
fn printable(answer: &[u8]) -> String {
    answer
        .iter()
        .filter(|b| b.is_ascii_graphic() || **b == b' ')
        .map(|&b| char::from(b))
        .collect()
}

fn handshake<W: Wire, P: Pause>(wire: &mut W, pause: &P) -> Result<Hello> {
    wire.discard_input().map_err(io_err)?;
    for attempt in 0..HELLO_TRIES {
        wire.send(&proto::hello()).map_err(io_err)?;
        let answer = wire.receive(REPLY_MAX, REPLY_TIMEOUT).map_err(io_err)?;
        if let Some(hello) = Hello::parse(&answer) {
            return Ok(hello);
        }
        tracing::debug!(attempt, "no HELLO answer; resyncing");
        wire.send(&proto::start_display_block()).map_err(io_err)?;
        pause.pause(HELLO_RETRY_PAUSE);
    }
    Err(BezelError::Timeout(
        "the screen did not answer HELLO".into(),
    ))
}

/// The model the HELLO answer names, among the discovery candidates.
/// Only 8.8" answers are trusted to name the size (2.1" units answer `5inch`).
fn pick_model(hello: &Hello, candidates: &[&'static DeviceModel]) -> Option<&'static DeviceModel> {
    if let [only] = candidates {
        return Some(only);
    }
    let wanted = match hello.model.as_str() {
        "88inch" => "turing-8.8",
        "5inch" => "turing-5",
        _ => return None,
    };
    candidates.iter().copied().find(|m| m.id.0 == wanted)
}

/// RGBA8 → BGRA8, the rev C pixel order.
pub fn rgba_to_bgra(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.as_chunks::<RGBA_BYTES>().0 {
        out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::io;
    use std::sync::{Arc, Mutex, PoisonError};

    use super::*;
    use crate::driver::RealTime;
    use crate::wire::ScriptedWire;
    use bezel_core::domain::catalog::model_by_id;
    use bezel_core::domain::device::ModelId;
    use bezel_core::domain::frame::{Rect, Rgba};
    use bezel_core::domain::job::CancelToken;
    use bezel_core::domain::screen::Confirm;
    use bezel_core::domain::storage::{BootMedia, Capacity, Operation};

    #[derive(Clone)]
    struct NoPause;
    impl Pause for NoPause {
        fn pause(&self, _d: Duration) {}
    }

    /// Records every pause.
    #[derive(Clone, Default)]
    struct Pauses(Arc<Mutex<Vec<Duration>>>);
    impl Pause for Pauses {
        fn pause(&self, d: Duration) {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(d);
        }
    }
    impl Pauses {
        fn take(&self) -> Vec<Duration> {
            std::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner))
        }
    }

    type Screen<P = NoPause> = TuringRevC<ScriptedWire, P>;

    const ROM_190: &str = "chs_88inch.dev1_rom1.90";

    fn m88() -> &'static DeviceModel {
        model_by_id(ModelId("turing-8.8")).unwrap()
    }

    fn connected_with<P: Pause + Clone>(pause: &P, hello: &str, model: &'static str) -> Screen<P> {
        let wire = ScriptedWire::with_replies([hello.as_bytes().to_vec(), b"media_stop".to_vec()]);
        let model = model_by_id(ModelId(model)).unwrap();
        TuringRevC::connect(wire, pause, &[model]).unwrap()
    }

    fn connected() -> Screen {
        connected_with(&NoPause, ROM_190, "turing-8.8")
    }

    fn path(text: &str) -> RemotePath {
        RemotePath::parse(text).unwrap()
    }

    fn script<P: Pause>(s: &mut Screen<P>, replies: &[&str]) {
        for r in replies {
            s.wire.reply(r.as_bytes());
        }
    }

    /// A command packet: 250 bytes with the magic after the opcode (data
    /// blocks of these tests never look like one).
    fn is_command(packet: &[u8]) -> bool {
        packet.len() == BLOCK && packet[1..3] == proto::MAGIC
    }

    /// Opcodes of the commands in `sent`, data blocks left out.
    fn commands(sent: &[Vec<u8>]) -> Vec<u8> {
        sent.iter()
            .filter(|p| is_command(p))
            .map(|p| p[0])
            .collect()
    }

    /// What was sent after the first `from` writes.
    fn since<P: Pause>(s: &Screen<P>, from: usize) -> &[Vec<u8>] {
        &s.wire().sent[from..]
    }

    fn confirmed() -> Confirmed {
        Confirmed::require(Confirm::Yes, &Operation::Boot(BootMedia::Default)).unwrap()
    }

    /// Runs an upload whose job cancels once `cancel_at` bytes were
    /// reported; returns the result and the `(done, total)` reports.
    fn run_upload<W: Wire, P: Pause>(
        s: &mut TuringRevC<W, P>,
        target: &RemotePath,
        data: &[u8],
        cancel_at: Option<u64>,
    ) -> (Result<()>, Vec<(u64, u64)>) {
        let token = CancelToken::new();
        let remote = token.clone();
        let mut seen = Vec::new();
        let mut sink = |p: Progress| {
            seen.push((p.done, p.total));
            if cancel_at.is_some_and(|at| p.done >= at) {
                remote.cancel();
            }
        };
        let mut job = Job::new(&token, &mut sink);
        let result = s.upload(target, data, &mut job);
        (result, seen)
    }

    fn test_file(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn connect_handshakes_stops_media_and_enters_streaming() {
        let s = connected();
        let opcodes: Vec<u8> = s.wire().sent.iter().map(|p| p[0]).collect();
        assert_eq!(
            opcodes,
            vec![
                op::HELLO,
                op::STOP_VIDEO,
                op::STOP_MEDIA,
                op::PRE_UPDATE_BITMAP
            ]
        );
        assert_eq!(s.identity().firmware.as_deref(), Some(ROM_190));
        assert_eq!(s.format, PixelFormat::Bgra);
        assert_eq!(s.class, ScreenClass::Large);
        assert_eq!(s.wire().discards, 1);
    }

    #[test]
    fn hello_is_retried_with_a_resync_block_then_times_out() {
        let wire = ScriptedWire::default();
        let err = TuringRevC::connect(wire, &NoPause, &[m88()]).err().unwrap();
        assert!(matches!(err, BezelError::Timeout(_)));
        let mut wire = ScriptedWire::with_replies([vec![], b"chs_88inch.dev1_rom1.88".to_vec()]);
        let _ = handshake(&mut wire, &NoPause).unwrap();
        let opcodes: Vec<u8> = wire.sent.iter().map(|p| p[0]).collect();
        assert_eq!(opcodes, vec![op::HELLO, 0x2C, op::HELLO]);
    }

    #[test]
    fn first_frame_is_full_and_native_bgra() {
        let mut s = connected();
        s.set_orientation(Orientation::Portrait).unwrap();
        let mut frame = Frame::filled(m88().panel, Rgba::BLACK);
        frame.fill_rect(Rect::new(0, 0, 1, 1), Rgba::opaque(255, 0, 0));
        s.present(&frame).unwrap();
        let sent = &s.wire().sent;
        let n = sent.len();
        assert!(sent[n - 3].iter().all(|&b| b == 0x2C));
        assert_eq!(
            &sent[n - 2][..7],
            &[0xC8, 0xEF, 0x69, 0x00, 0x38, 0x40, 0x00]
        );
        let data = &sent[n - 1];
        assert_eq!(data.len(), 3_701_250);
        // Portrait on a reverse-portrait panel: rotated 180°, so the red
        // top-left pixel is the last native pixel (BGRA 00 00 FF FF).
        let bgra = s.last.as_ref().unwrap();
        assert_eq!(&bgra[bgra.len() - 4..], &[0, 0, 255, 255]);
    }

    #[test]
    fn later_frames_are_partial_then_status_is_queried() {
        let mut s = connected();
        let base = Frame::filled(m88().panel, Rgba::BLACK);
        s.present(&base).unwrap();
        let before = s.wire().sent.len();
        // Unchanged frame: nothing is sent.
        s.present(&base).unwrap();
        assert_eq!(s.wire().sent.len(), before);

        let mut next = base.clone();
        next.fill_rect(Rect::new(10, 10, 3, 1), Rgba::WHITE);
        s.wire.reply(b"needReSend:0|renderCnt:1|theme:");
        s.present(&next).unwrap();
        let sent = &s.wire().sent[before..];
        assert_eq!(sent[0][0], op::UPDATE_BITMAP);
        assert_eq!(sent[2][0], op::QUERY_STATUS);
        let list_len =
            u32::from_be_bytes([sent[0][3], sent[0][4], sent[0][5], sent[0][6]]) as usize;
        assert_eq!(list_len, 5 + 3 * 4 + 2, "one run of 3 BGRA pixels + EF 69");
        assert_eq!(&sent[1][list_len - 2..list_len], &[0xEF, 0x69]);
        assert_eq!(s.seq, 1);
    }

    #[test]
    fn need_resend_forces_the_next_frame_full() {
        let mut s = connected();
        let base = Frame::filled(m88().panel, Rgba::BLACK);
        s.present(&base).unwrap();
        let mut next = base.clone();
        next.fill_rect(Rect::new(0, 0, 1, 1), Rgba::WHITE);
        s.wire.reply(b"needReSend:1|renderCnt:1");
        s.present(&next).unwrap();
        assert!(s.last.is_none());
        s.present(&next).unwrap();
        assert_eq!(s.wire().sent.last().unwrap().len(), 3_701_250);
    }

    #[test]
    fn frames_keep_per_pixel_alpha_in_both_formats() {
        let clear = |r, g, b| Rgba { r, g, b, a: 0 };
        // ROM 1.90: BGRA everywhere, A = 0 kept (the video shows through).
        let mut s = connected();
        s.set_orientation(Orientation::ReversePortrait).unwrap();
        let mut frame = Frame::filled(m88().panel, Rgba::BLACK);
        frame.fill_rect(Rect::new(0, 0, 1, 1), clear(10, 20, 30));
        s.present(&frame).unwrap();
        assert_eq!(
            &s.wire().sent.last().unwrap()[..8],
            &[30, 20, 10, 0, 0, 0, 0, 255]
        );
        frame.fill_rect(Rect::new(5, 0, 1, 1), clear(1, 2, 3));
        s.wire.reply(b"needReSend:0|renderCnt:1");
        let before = s.wire().sent.len();
        s.present(&frame).unwrap();
        assert_eq!(&since(&s, before)[1][..7], &[0x80, 0, 5, 3, 2, 1, 0]);

        // ROM 1.88: the 3-byte form carries alpha in the low bits of B and G.
        let mut s = connected_with(&NoPause, "chs_88inch.dev1_rom1.88", "turing-8.8");
        assert_eq!(s.format, PixelFormat::CompressedBgra);
        s.set_orientation(Orientation::ReversePortrait).unwrap();
        let mut frame = Frame::filled(m88().panel, Rgba::BLACK);
        s.present(&frame).unwrap();
        frame.fill_rect(Rect::new(5, 0, 1, 1), clear(0x10, 0xFF, 0xFF));
        frame.fill_rect(Rect::new(7, 0, 1, 1), Rgba::WHITE);
        s.wire.reply(b"needReSend:0|renderCnt:1");
        let before = s.wire().sent.len();
        s.present(&frame).unwrap();
        assert_eq!(
            &since(&s, before)[1][..12],
            &[0x80, 0, 5, 0xFC, 0xFC, 0x10, 0x80, 0, 7, 0xFF, 0xFF, 0xFF]
        );
    }

    #[test]
    fn nothing_storage_related_is_sent_implicitly() {
        let mut s = connected();
        s.set_orientation(Orientation::Landscape).unwrap();
        let panel = m88().panel.in_orientation(Orientation::Landscape);
        let base = Frame::filled(panel, Rgba::BLACK);
        s.present(&base).unwrap();
        let mut next = base.clone();
        next.fill_rect(Rect::new(0, 0, 4, 4), Rgba::WHITE);
        s.wire.reply(b"needReSend:1|renderCnt:1");
        s.present(&next).unwrap();
        s.present(&base).unwrap();
        s.set_brightness(Brightness::MAX).unwrap();
        s.screen_off().unwrap();
        s.release().unwrap();
        let forbidden = [
            op::STORAGE_INFO,
            op::LIST_DIR,
            op::DELETE_FILE,
            op::FILE_SIZE,
            op::UPLOAD_FILE,
            op::PLAY_VIDEO,
            op::SET_OPTIONS,
            op::SET_ROTATION,
            0x82,
            op::RESTART,
            op::PLAY_IMAGE,
        ];
        let sent = commands(&s.wire().sent);
        assert!(sent.len() > 6, "{sent:02x?}");
        assert!(sent.iter().all(|o| !forbidden.contains(o)), "{sent:02x?}");
    }

    #[test]
    fn storage_queries_map_folders_and_parse_replies() {
        let mut s = connected();
        assert!(s.storage().is_some());
        let before = s.wire().sent.len();
        let discards = s.wire().discards;
        script(&mut s, &["garbage", "7340032-1048576-6291456-0-0-0\0"]);
        let info = s.info().unwrap();
        assert_eq!(
            info.internal,
            Capacity {
                total: (7_340_032 - 512) * 1024,
                used: 1_048_576 * 1024,
                free: (6_291_456 - 512) * 1024,
            }
        );
        assert_eq!(info.card, None);
        assert_eq!(
            commands(since(&s, before)),
            [op::STORAGE_INFO, op::STORAGE_INFO],
            "a bad reply is asked again"
        );
        assert_eq!(s.wire().discards, discards + 2, "stale input dropped first");

        let before = s.wire().sent.len();
        script(&mut s, &["file:88.mp4/logo.png/", "nodir-createdone"]);
        let internal_video = path("internal/video/x").location;
        let names: Vec<String> = s
            .list(internal_video)
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(names, ["88.mp4", "logo.png"]);
        assert!(s.list(path("sd/image/x").location).unwrap().is_empty());
        let sent = since(&s, before);
        assert_eq!(
            sent[0],
            proto::path_command(op::LIST_DIR, "/mnt/UDISK/video/").unwrap()
        );
        assert_eq!(
            sent[1],
            proto::path_command(op::LIST_DIR, "/mnt/SDCARD/img/").unwrap()
        );

        let before = s.wire().sent.len();
        script(&mut s, &["12345", "0"]);
        let clip = path("internal/video/88.mp4");
        assert_eq!(s.size(&clip).unwrap(), Some(12_345));
        assert_eq!(s.size(&clip).unwrap(), None, "0 means absent");
        assert_eq!(
            since(&s, before)[0],
            proto::path_command(op::FILE_SIZE, "/mnt/UDISK/video/88.mp4").unwrap()
        );
        let before = s.wire().sent.len();
        let err = s.size(&clip).unwrap_err();
        assert!(matches!(err, BezelError::Timeout(_)), "{err}");
        assert_eq!(commands(since(&s, before)), [op::FILE_SIZE; QUERY_TRIES]);
        assert!(
            err.to_string()
                .contains("GET_FILE_SIZE /mnt/UDISK/video/88.mp4")
        );
    }

    #[test]
    fn upload_reports_progress_and_can_be_cancelled() {
        // A whole upload follows spec § 13.4 and reports after every write.
        let mut s = connected();
        let data = test_file(UPLOAD_CHUNK * 2 + 1000);
        let total = data.len() as u64;
        let clip = path("internal/video/clip.mp4");
        let before = s.wire().sent.len();
        let size = total.to_string();
        script(
            &mut s,
            &[
                "media_stop",
                "file:old.mp4/",
                "create_success",
                "file_rev_done",
                &size,
            ],
        );
        let (result, progress) = run_upload(&mut s, &clip, &data, None);
        result.unwrap();
        assert_eq!(s.size(&clip).unwrap(), Some(total), "the use case's check");
        let sent = since(&s, before);
        assert_eq!(
            commands(sent),
            [
                op::STOP_VIDEO,
                op::STOP_MEDIA,
                op::LIST_DIR,
                op::UPLOAD_FILE,
                op::FILE_SIZE
            ]
        );
        assert_eq!(
            sent[2],
            proto::path_command(op::LIST_DIR, "/mnt/UDISK/video/").unwrap()
        );
        let header = proto::upload_file("/mnt/UDISK/video/clip.mp4", total as u32).unwrap();
        assert_eq!(sent[3], header);
        let writes = &sent[4..7];
        assert!(writes.iter().all(|w| !is_command(w)));
        assert_eq!(
            writes.concat(),
            proto::blocks(&data),
            "the same bytes, 3 writes"
        );
        assert!(is_command(&sent[7]));
        let chunk = UPLOAD_CHUNK as u64;
        assert_eq!(
            progress,
            [
                (0, total),
                (chunk, total),
                (2 * chunk, total),
                (total, total)
            ]
        );

        // Cancelled after the first write: no more data; the rest of the
        // declared length goes out as filler (reported as progress), then
        // HELLO puts the link back and GET_FILE_SIZE measures the partial.
        // Nothing is deleted.
        let mut s = connected();
        let before = s.wire().sent.len();
        script(
            &mut s,
            &[
                "media_stop",
                "nodir-createdone",
                "create_success",
                "file_rev_done",
                ROM_190,
                &size,
            ],
        );
        let (result, progress) = run_upload(&mut s, &clip, &data, Some(1));
        assert_eq!(
            result,
            Err(BezelError::Cancelled {
                partial: Some(total)
            })
        );
        assert_eq!(
            progress,
            [
                (0, total),
                (chunk, total),
                (2 * chunk, total),
                (total, total)
            ],
            "the filler moves the counters from the cancel point to the file size"
        );
        let sent = since(&s, before);
        assert_eq!(
            commands(sent),
            [
                op::STOP_VIDEO,
                op::STOP_MEDIA,
                op::LIST_DIR,
                op::UPLOAD_FILE,
                op::HELLO,
                op::FILE_SIZE
            ]
        );
        let filler: Vec<usize> = sent.iter().filter(|w| is_filler(w)).map(Vec::len).collect();
        assert_eq!(filler, [UPLOAD_CHUNK_BLOCKS * BLOCK, 5 * BLOCK]);
        assert_eq!(sent.iter().filter(|w| !is_command(w)).count(), 3);
        assert!(!s.streaming && s.last.is_none(), "the next frame is full");

        // Cancelled before the first write, the header accepted: the whole
        // declared length is filler. A screen that then finds no file
        // leaves no partial to offer for deletion.
        let mut s = connected();
        let before = s.wire().sent.len();
        script(
            &mut s,
            &[
                "media_stop",
                "nodir-createdone",
                "create_success",
                "file_rev_done",
                ROM_190,
                "0",
            ],
        );
        let (result, _) = run_upload(&mut s, &clip, &data, Some(0));
        assert_eq!(result, Err(BezelError::Cancelled { partial: None }));
        let sent = since(&s, before);
        assert!(sent.iter().all(|w| is_command(w) || is_filler(w)));
        assert_eq!(
            data_phase_bytes(sent, op::HELLO),
            proto::data_phase_len(total)
        );

        // Cancelled before the header: nothing is sent at all.
        let mut s = connected();
        let before = s.wire().sent.len();
        let token = CancelToken::new();
        token.cancel();
        let mut sink = |_: Progress| {};
        let mut job = Job::new(&token, &mut sink);
        let result = s.upload(&clip, &data, &mut job);
        assert_eq!(result, Err(BezelError::Cancelled { partial: None }));
        assert!(since(&s, before).is_empty());
    }

    #[test]
    fn upload_failures_and_the_completion_wait() {
        let clip = path("sd/video/clip.mp4");
        let data = test_file(1000);

        // The device refuses the header: no data follows.
        let mut s = connected();
        let before = s.wire().sent.len();
        script(&mut s, &["media_stop", "nodir-createdone", "no space"]);
        let (result, _) = run_upload(&mut s, &clip, &data, None);
        assert!(matches!(result, Err(BezelError::Timeout(_))), "{result:?}");
        assert!(since(&s, before).iter().all(|w| is_command(w)));
        assert_eq!(
            since(&s, before)[3],
            proto::upload_file("/mnt/SDCARD/video/clip.mp4", 1000).unwrap()
        );
        let (empty, _) = run_upload(&mut s, &clip, &[], None);
        assert!(matches!(empty, Err(BezelError::InvalidInput(_))));

        // No `file_rev_done`: 15 waits 200 ms apart, then the size check decides.
        let pauses = Pauses::default();
        let mut s = connected_with(&pauses, ROM_190, "turing-8.8");
        pauses.take();
        script(
            &mut s,
            &["media_stop", "nodir-createdone", "create_success"],
        );
        let (result, _) = run_upload(&mut s, &clip, &data, None);
        assert_eq!(result, Ok(()));
        let waits = pauses.take();
        assert_eq!(waits[0], STOP_VIDEO_SETTLE);
        assert_eq!(
            &waits[1..],
            [RECEIVED_ROUND_PAUSE; RECEIVED_ROUNDS_LARGE - 1]
        );

        // Cancelled while the device writes: recovered the same way.
        let mut s = connected();
        let size = data.len().to_string();
        script(
            &mut s,
            &[
                "media_stop",
                "nodir-createdone",
                "create_success",
                ROM_190,
                &size,
            ],
        );
        let (result, _) = run_upload(&mut s, &clip, &data, Some(1000));
        assert_eq!(
            result,
            Err(BezelError::Cancelled {
                partial: Some(1000)
            })
        );

        // The screen answers neither `file_rev_done` after the filler (one
        // block owed) nor HELLO: reconnect, then check for the partial.
        let pauses = Pauses::default();
        let mut s = connected_with(&pauses, ROM_190, "turing-8.8");
        pauses.take();
        let before = s.wire().sent.len();
        script(
            &mut s,
            &["media_stop", "nodir-createdone", "create_success"],
        );
        let (result, _) = run_upload(&mut s, &clip, &test_file(UPLOAD_CHUNK + 1), Some(1));
        let err = result.unwrap_err();
        assert!(matches!(err, BezelError::Timeout(_)), "{err}");
        assert!(err.to_string().contains("sd/video/clip.mp4"), "{err}");
        let sent = since(&s, before);
        let first_hello = sent
            .iter()
            .position(|w| is_command(w) && w[0] == op::HELLO)
            .unwrap();
        assert_eq!(sent[first_hello - 1], proto::filler_blocks(1));
        let hellos = commands(sent)
            .into_iter()
            .filter(|o| *o == op::HELLO)
            .count();
        assert_eq!(hellos, HELLO_TRIES);
        let waits = pauses.take();
        assert_eq!(waits[0], STOP_VIDEO_SETTLE);
        assert_eq!(
            waits[1..],
            [HELLO_RETRY_PAUSE; HELLO_TRIES],
            "the wait after the filler reads, it does not pause"
        );
    }

    /// A write of filler blocks: whole blocks of 249 × [`proto::FILLER`] and
    /// a zero (a resync block ends in `2c`, a command carries the magic).
    fn is_filler(write: &[u8]) -> bool {
        !write.is_empty()
            && write.len().is_multiple_of(BLOCK)
            && write.chunks(BLOCK).all(|b| {
                b[..proto::BLOCK_PAYLOAD]
                    .iter()
                    .all(|&x| x == proto::FILLER)
                    && b[proto::BLOCK_PAYLOAD] == 0
            })
    }

    /// Wire bytes of `writes` from the first UPLOAD_FILE header (excluded)
    /// up to the first command whose opcode is `until`.
    fn data_phase_bytes(writes: &[Vec<u8>], until: u8) -> u64 {
        writes
            .iter()
            .skip_while(|w| !(is_command(w) && w[0] == op::UPLOAD_FILE))
            .skip(1)
            .take_while(|w| !(is_command(w) && w[0] == until))
            .map(|w| w.len() as u64)
            .sum()
    }

    #[test]
    fn a_cancel_completes_the_declared_length_before_hello() {
        // Cancelled after the first write of a 3-write file. Whether or not
        // the firmware would answer a HELLO now, only the declared length
        // closes its file cleanly (spec § 19): filler first, then HELLO.
        let mut s = connected();
        let before = s.wire().sent.len();
        let data = test_file(UPLOAD_CHUNK * 3 + 1000);
        let size = data.len() as u64;
        let declared = proto::data_phase_len(size);
        let clip = path("sd/video/clip.mp4");
        let stored = size.to_string();
        script(
            &mut s,
            &[
                "media_stop",
                "nodir-createdone",
                "create_success",
                "file_rev_done",
                ROM_190,
                &stored,
            ],
        );
        let (result, progress) = run_upload(&mut s, &clip, &data, Some(1));
        // GET_FILE_SIZE finds the whole declared size: the partial file, its
        // tail filler, for the user to delete (nothing deleted here).
        assert_eq!(
            result,
            Err(BezelError::Cancelled {
                partial: Some(size)
            })
        );
        let chunk = UPLOAD_CHUNK as u64;
        assert_eq!(
            progress,
            [
                (0, size),
                (chunk, size),
                (2 * chunk, size),
                (3 * chunk, size),
                (size, size)
            ],
            "every filler write moves the counters on from the cancel point"
        );
        let sent = since(&s, before);
        assert_eq!(
            commands(sent),
            [
                op::STOP_VIDEO,
                op::STOP_MEDIA,
                op::LIST_DIR,
                op::UPLOAD_FILE,
                op::HELLO,
                op::FILE_SIZE
            ],
            "no HELLO before the filler, one answered after it"
        );
        // After the header: the data write, filler up to exactly the
        // declared length, then HELLO.
        let header = sent
            .iter()
            .position(|w| is_command(w) && w[0] == op::UPLOAD_FILE)
            .unwrap();
        let whole = UPLOAD_CHUNK_BLOCKS * BLOCK;
        let lens: Vec<usize> = sent[header + 1..header + 5].iter().map(Vec::len).collect();
        assert_eq!(lens, [whole, whole, whole, 5 * BLOCK]);
        assert!(sent[header + 2..header + 5].iter().all(|w| is_filler(w)));
        assert_eq!(sent[header + 5], proto::hello());
        assert_eq!(data_phase_bytes(sent, op::HELLO), declared);

        // No `file_rev_done` within the wait after the filler: HELLO decides.
        let mut s = connected();
        let mut replies = vec!["media_stop", "nodir-createdone", "create_success"];
        replies.extend([""; FILLED_POLLS]);
        replies.extend([ROM_190, stored.as_str()]);
        script(&mut s, &replies);
        let (result, _) = run_upload(&mut s, &clip, &data, Some(1));
        assert_eq!(
            result,
            Err(BezelError::Cancelled {
                partial: Some(size)
            })
        );

        // HELLO answered but GET_FILE_SIZE not: that error, as for any query.
        let mut s = connected();
        script(
            &mut s,
            &[
                "media_stop",
                "nodir-createdone",
                "create_success",
                "file_rev_done",
                ROM_190,
            ],
        );
        let (result, _) = run_upload(&mut s, &clip, &data, Some(1));
        let err = result.unwrap_err();
        assert!(matches!(err, BezelError::Timeout(_)), "{err}");
        assert!(
            err.to_string()
                .contains("GET_FILE_SIZE /mnt/SDCARD/video/clip.mp4"),
            "{err}"
        );
    }

    /// What the modelled firmware does when the host waits for an answer in
    /// the middle of a data phase (after a HELLO it took as file data).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Idle {
        /// Keeps waiting for the declared length: no HELLO is answered on
        /// that link (the 8.8" before T-7.3).
        Waits,
        /// Leaves the data phase: the next HELLO is answered, but what its
        /// writer still queued goes into the next file (the 8.8" after T-7.3).
        Leaves,
    }

    /// File bytes the modelled writer lags behind the wire in a data phase.
    const WRITER_QUEUE: usize = 10_000;

    /// Device paths of the files these tests upload to the card.
    const CLIP: &str = "/mnt/SDCARD/video/clip.mp4";
    const LOGO: &str = "/mnt/SDCARD/img/logo.png";

    /// An upload the modelled firmware receives.
    struct Receiving {
        path: String,
        size: usize,
        /// Wire bytes still to come.
        owed: usize,
        /// Payload of the blocks received (separators dropped).
        payload: Vec<u8>,
        /// The block being received.
        block: Vec<u8>,
    }

    impl Receiving {
        fn take(&mut self, bytes: &[u8]) {
            self.owed -= bytes.len();
            for &b in bytes {
                self.block.push(b);
                if self.block.len() == BLOCK {
                    self.payload
                        .extend_from_slice(&self.block[..proto::BLOCK_PAYLOAD]);
                    self.block.clear();
                }
            }
        }
    }

    /// The rev C firmware as the 8.8" (ROM 1.90) showed it, spec § 19:
    /// 250-byte command packets (blocks without the magic are ignored);
    /// after an UPLOAD_FILE header every byte is file data until the
    /// declared length, which alone closes the file cleanly
    /// (`file_rev_done`). Its writer lags [`WRITER_QUEUE`] bytes behind:
    /// when a data phase ends any other way ([`Idle::Leaves`], or the wake of
    /// the next connection), the file keeps what was written and the queued
    /// bytes go into the next file opened.
    struct Firmware {
        idle: Idle,
        /// Every write, in order.
        sent: Vec<Vec<u8>>,
        /// Opcodes taken as commands.
        commands: Vec<u8>,
        /// The command packet being received.
        packet: Vec<u8>,
        receiving: Option<Receiving>,
        /// What the writer holds for the next file opened.
        queued: Vec<u8>,
        files: Vec<(String, Vec<u8>)>,
        replies: VecDeque<Vec<u8>>,
    }

    impl Firmware {
        fn new(idle: Idle) -> Self {
            Self {
                idle,
                sent: Vec::new(),
                commands: Vec::new(),
                packet: Vec::new(),
                receiving: None,
                queued: Vec::new(),
                files: Vec::new(),
                replies: VecDeque::new(),
            }
        }

        fn file(&self, path: &str) -> Option<&[u8]> {
            self.files
                .iter()
                .find(|(p, _)| p == path)
                .map(|(_, content)| content.as_slice())
        }

        /// The next connection wakes a screen that answers nothing: its data
        /// phase ends, the queued bytes stay for the next file (spec § 19).
        fn wake(&mut self) {
            self.leave();
            self.packet.clear();
        }

        fn feed(&mut self, mut bytes: &[u8]) {
            while !bytes.is_empty() {
                if let Some(upload) = &mut self.receiving {
                    let (data, rest) = bytes.split_at(upload.owed.min(bytes.len()));
                    upload.take(data);
                    bytes = rest;
                    if upload.owed == 0 {
                        self.close();
                    }
                    continue;
                }
                let room = BLOCK - self.packet.len();
                let (part, rest) = bytes.split_at(room.min(bytes.len()));
                self.packet.extend_from_slice(part);
                bytes = rest;
                if self.packet.len() == BLOCK {
                    let packet = std::mem::take(&mut self.packet);
                    self.command(&packet);
                }
            }
        }

        fn command(&mut self, packet: &[u8]) {
            if packet[1..3] != proto::MAGIC {
                return;
            }
            self.commands.push(packet[0]);
            let n = u32::from_be_bytes(packet[3..7].try_into().unwrap()) as usize;
            let named = || String::from_utf8_lossy(&packet[10..10 + n]).into_owned();
            let answer = match packet[0] {
                op::HELLO => ROM_190.to_string(),
                op::STOP_MEDIA => reply::MEDIA_STOPPED.to_string(),
                op::LIST_DIR => self.listing(&named()),
                op::FILE_SIZE => self.file(&named()).map_or(0, <[u8]>::len).to_string(),
                op::UPLOAD_FILE => {
                    let size = u32::from_le_bytes(packet[10 + n..14 + n].try_into().unwrap());
                    self.receiving = Some(Receiving {
                        path: named(),
                        size: size as usize,
                        owed: proto::data_phase_len(u64::from(size)) as usize,
                        payload: Vec::new(),
                        block: Vec::new(),
                    });
                    reply::CREATED.to_string()
                }
                _ => return,
            };
            self.replies.push_back(answer.into_bytes());
        }

        fn listing(&self, folder: &str) -> String {
            let names: Vec<&str> = self
                .files
                .iter()
                .filter_map(|(p, _)| p.strip_prefix(folder))
                .collect();
            if names.is_empty() {
                return "nodir-createdone".into();
            }
            format!("file:{}/", names.join("/"))
        }

        /// The declared length arrived: the writer writes it all (after
        /// what it still held) and the file is closed.
        fn close(&mut self) {
            if let Some(upload) = self.receiving.take() {
                let mut content = std::mem::take(&mut self.queued);
                content.extend_from_slice(&upload.payload[..upload.size]);
                self.store(upload.path, content);
                self.replies.push_back(reply::RECEIVED.into());
            }
        }

        /// The data phase ends short of its declared length: the file keeps
        /// what the writer wrote, the rest waits for the next file.
        fn leave(&mut self) {
            if let Some(upload) = self.receiving.take() {
                let written = upload.payload.len().saturating_sub(WRITER_QUEUE);
                let mut content = std::mem::take(&mut self.queued);
                content.extend_from_slice(&upload.payload[..written]);
                self.queued = upload.payload[written..].to_vec();
                self.store(upload.path, content);
            }
        }

        fn store(&mut self, path: String, content: Vec<u8>) {
            self.files.retain(|(p, _)| *p != path);
            self.files.push((path, content));
        }
    }

    impl Wire for Firmware {
        fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
            self.sent.push(bytes.to_vec());
            self.feed(bytes);
            Ok(())
        }

        /// A pending answer, else silence: a host that waits in the middle
        /// of a data phase is what [`Idle`] is about.
        fn receive(&mut self, max: usize, _timeout: Duration) -> io::Result<Vec<u8>> {
            if let Some(mut answer) = self.replies.pop_front() {
                answer.truncate(max);
                return Ok(answer);
            }
            if self.idle == Idle::Leaves {
                self.leave();
            }
            Ok(Vec::new())
        }

        fn discard_input(&mut self) -> io::Result<()> {
            self.replies.clear();
            Ok(())
        }
    }

    #[test]
    fn the_firmware_model_reproduces_the_cancelled_upload_evidence() {
        let data = test_file(UPLOAD_CHUNK * 3 + 1000);
        // The 7,444-byte PNG uploaded after the cancel on the 8.8".
        let png = test_file(7_444);
        let image = path("sd/image/logo.png");
        for idle in [Idle::Leaves, Idle::Waits] {
            // A cancel recovered the way T-7.3 did: part of the data phase,
            // then HELLO first.
            let mut fw = Firmware::new(idle);
            fw.send(&proto::upload_file(CLIP, data.len() as u32).unwrap())
                .unwrap();
            fw.send(&proto::blocks(&data[..UPLOAD_CHUNK])).unwrap();
            let hello = handshake(&mut fw, &NoPause);
            if idle == Idle::Leaves {
                // After T-7.3: the first HELLO was file data, the one after
                // the resync block is answered...
                assert!(hello.is_ok(), "{hello:?}");
                assert_eq!(fw.commands, [op::UPLOAD_FILE, op::HELLO]);
            } else {
                // Before T-7.3: no HELLO is answered; the next connection
                // wakes the screen.
                assert!(matches!(hello, Err(BezelError::Timeout(_))), "{hello:?}");
                assert_eq!(fw.commands, [op::UPLOAD_FILE]);
                fw.wake();
            }
            // ...the partial is shorter than what the screen accepted...
            let partial = fw.file(CLIP).unwrap().len();
            assert!(partial < UPLOAD_CHUNK, "{idle:?}: {partial}");
            assert_eq!(fw.queued.len(), WRITER_QUEUE);
            // ...and the next upload takes the queued bytes: its size check
            // fails.
            let mut s = TuringRevC::connect(fw, &NoPause, &[m88()]).unwrap();
            let (result, _) = run_upload(&mut s, &image, &png, None);
            assert_eq!(result, Ok(()));
            assert_eq!(
                s.size(&image).unwrap(),
                Some((WRITER_QUEUE + png.len()) as u64),
                "{idle:?}"
            );
            assert_eq!(s.wire().file(LOGO).unwrap()[WRITER_QUEUE..], png);
        }
    }

    #[test]
    fn after_a_cancel_the_next_upload_is_exact() {
        let data = test_file(UPLOAD_CHUNK * 3 + 1000);
        let size = data.len() as u64;
        let clip = path("sd/video/clip.mp4");
        let png = test_file(7_444);
        let image = path("sd/image/logo.png");
        // Cancelled before the first write, after it, and while the screen
        // writes the whole file: `kept` bytes of the data reach the file.
        let cancels = [(0, 0), (1, UPLOAD_CHUNK), (size, data.len())];
        // With a firmware that would answer HELLO after a cancel and one
        // that would not.
        for idle in [Idle::Leaves, Idle::Waits] {
            for (cancel_at, kept) in cancels {
                let case = format!("{idle:?}, cancelled at {cancel_at}");
                let mut s = TuringRevC::connect(Firmware::new(idle), &NoPause, &[m88()]).unwrap();
                let (result, progress) = run_upload(&mut s, &clip, &data, Some(cancel_at));
                assert_eq!(
                    result,
                    Err(BezelError::Cancelled {
                        partial: Some(size)
                    }),
                    "{case}"
                );
                assert_eq!(progress.last(), Some(&(size, size)), "{case}");
                let fw = s.wire();
                let partial = fw.file(CLIP).unwrap();
                assert_eq!(partial[..kept], data[..kept], "{case}");
                assert!(
                    partial[kept..].iter().all(|&b| b == proto::FILLER),
                    "{case}"
                );
                assert!(fw.receiving.is_none() && fw.queued.is_empty(), "{case}");
                // The recovery's one HELLO was a command, answered at once.
                assert_eq!(
                    fw.commands[fw.commands.len() - 3..],
                    [op::UPLOAD_FILE, op::HELLO, op::FILE_SIZE],
                    "{case}"
                );
                let hellos = fw.sent.iter().filter(|w| w.as_slice() == proto::hello());
                assert_eq!(
                    hellos.count(),
                    2,
                    "{case}: the connect's and the recovery's"
                );

                let (result, _) = run_upload(&mut s, &image, &png, None);
                assert_eq!(result, Ok(()), "{case}");
                assert_eq!(s.size(&image).unwrap(), Some(png.len() as u64), "{case}");
                assert_eq!(s.wire().file(LOGO), Some(png.as_slice()), "{case}");
                let destructive = [op::DELETE_FILE, op::RESTART, 0x82, op::SET_OPTIONS];
                assert!(
                    s.wire().commands.iter().all(|o| !destructive.contains(o)),
                    "{case}"
                );
            }
        }
    }

    /// How a [`BulkWire`] takes its writes longer than one packet (data and
    /// filler), in order; later ones are taken.
    #[derive(Debug, Clone, Copy)]
    enum Bulk {
        Taken,
        /// The cancel's signal cuts the write short after its bytes left.
        CutAfterSending,
        /// The cancel's signal cuts the write short before any byte left.
        CutBeforeSending,
        /// The screen is gone.
        Unplugged,
    }

    /// A wire whose long writes follow a plan (see [`Bulk`]) on their way
    /// to `inner`.
    struct BulkWire<W> {
        inner: W,
        token: CancelToken,
        plan: Vec<Bulk>,
        bulk: usize,
    }

    impl<W: Wire> Wire for BulkWire<W> {
        fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
            if bytes.len() <= BLOCK {
                return self.inner.send(bytes);
            }
            let step = self.plan.get(self.bulk).copied().unwrap_or(Bulk::Taken);
            self.bulk += 1;
            let cut = || {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "timeout for retrying flush reached",
                )
            };
            match step {
                Bulk::Taken => self.inner.send(bytes),
                Bulk::CutAfterSending => {
                    self.inner.send(bytes)?;
                    self.token.cancel();
                    Err(cut())
                }
                Bulk::CutBeforeSending => {
                    self.token.cancel();
                    Err(cut())
                }
                Bulk::Unplugged => Err(io::Error::new(io::ErrorKind::BrokenPipe, "unplugged")),
            }
        }

        fn receive(&mut self, max: usize, timeout: Duration) -> io::Result<Vec<u8>> {
            self.inner.receive(max, timeout)
        }

        fn discard_input(&mut self) -> io::Result<()> {
            self.inner.discard_input()
        }
    }

    type BulkScreen = TuringRevC<BulkWire<Firmware>, NoPause>;

    /// Uploads `data` to the card's videos of a [`Firmware`] behind a
    /// [`BulkWire`] following `plan`, cancelled once `cancel_at` bytes were
    /// reported (or by the wire).
    fn bulk_upload(
        idle: Idle,
        plan: &[Bulk],
        cancel_at: Option<u64>,
        data: &[u8],
    ) -> (Result<()>, BulkScreen) {
        let token = CancelToken::new();
        let wire = BulkWire {
            inner: Firmware::new(idle),
            token: token.clone(),
            plan: plan.to_vec(),
            bulk: 0,
        };
        let mut s = TuringRevC::connect(wire, &NoPause, &[m88()]).unwrap();
        let remote = token.clone();
        let mut sink = |p: Progress| {
            if cancel_at.is_some_and(|at| p.done >= at) {
                remote.cancel();
            }
        };
        let mut job = Job::new(&token, &mut sink);
        let result = s.upload(&path("sd/video/clip.mp4"), data, &mut job);
        (result, s)
    }

    #[test]
    fn a_write_the_cancel_cut_counts_whole_so_the_filler_stays_within_the_declared_length() {
        let data = test_file(UPLOAD_CHUNK * 3 + 1000);
        let size = data.len() as u64;
        let declared = proto::data_phase_len(size);
        let kept = 2 * UPLOAD_CHUNK;

        // The cut write did reach the screen: the filler completes the file
        // exactly, HELLO is a command again and the next upload is exact.
        for idle in [Idle::Leaves, Idle::Waits] {
            let plan = [Bulk::Taken, Bulk::CutAfterSending];
            let (result, mut s) = bulk_upload(idle, &plan, None, &data);
            assert_eq!(
                result,
                Err(BezelError::Cancelled {
                    partial: Some(size)
                }),
                "{idle:?}"
            );
            let fw = &s.wire().inner;
            assert_eq!(data_phase_bytes(&fw.sent, op::HELLO), declared);
            let partial = fw.file(CLIP).unwrap();
            assert_eq!(partial[..kept], data[..kept]);
            assert!(partial[kept..].iter().all(|&b| b == proto::FILLER));
            let next = test_file(700);
            let (result, _) = run_upload(&mut s, &path("sd/video/next.mp4"), &next, None);
            assert_eq!(result, Ok(()));
            assert_eq!(
                s.wire().inner.file("/mnt/SDCARD/video/next.mp4"),
                Some(next.as_slice())
            );
        }

        // It did not: the filler comes one write short and the firmware
        // still waits, so it swallows the recovery's HELLOs as a cancel did
        // before the filler (reconnect). The count never passes the
        // declared length.
        let plan = [Bulk::Taken, Bulk::CutBeforeSending];
        let (result, s) = bulk_upload(Idle::Waits, &plan, None, &data);
        let err = result.unwrap_err();
        assert!(matches!(err, BezelError::Timeout(_)), "{err}");
        assert!(err.to_string().contains("sd/video/clip.mp4"), "{err}");
        let fw = &s.wire().inner;
        let after_header = data_phase_bytes(&fw.sent, 0xFF);
        assert!(after_header <= declared, "{after_header} > {declared}");
        assert!(fw.receiving.is_some(), "still in the data phase");
        assert_eq!(fw.commands.last(), Some(&op::UPLOAD_FILE));
        assert_eq!(
            commands(&fw.sent)
                .iter()
                .filter(|o| **o == op::HELLO)
                .count(),
            1 + HELLO_TRIES,
            "the connect's, then one round after the filler"
        );
    }

    #[test]
    fn the_filler_stops_at_a_failed_write_and_is_never_sent_for_a_finished_data_phase() {
        let data = test_file(UPLOAD_CHUNK * 3 + 1000);
        // The screen is unplugged as the filler starts: the reconnect
        // error; nothing, HELLO included, follows the failed write.
        let plan = [Bulk::Taken, Bulk::Unplugged];
        let (result, s) = bulk_upload(Idle::Waits, &plan, Some(1), &data);
        let err = result.unwrap_err();
        assert!(matches!(err, BezelError::Timeout(_)), "{err}");
        let fw = &s.wire().inner;
        assert_eq!(
            fw.sent.last(),
            Some(&proto::blocks(&data[..UPLOAD_CHUNK])),
            "the last write that left is the data"
        );
        assert_eq!(
            commands(&fw.sent)
                .iter()
                .filter(|o| **o == op::HELLO)
                .count(),
            1,
            "the connect's only"
        );

        // Cancelled while the screen writes a file it got whole: nothing is
        // owed, so no filler, and HELLO is answered at once.
        let small = test_file(1000);
        for idle in [Idle::Leaves, Idle::Waits] {
            let (result, s) = bulk_upload(idle, &[], Some(1000), &small);
            assert_eq!(
                result,
                Err(BezelError::Cancelled {
                    partial: Some(1000)
                }),
                "{idle:?}"
            );
            let fw = &s.wire().inner;
            assert!(!fw.sent.iter().any(|w| is_filler(w)));
            assert_eq!(
                fw.commands[fw.commands.len() - 3..],
                [op::UPLOAD_FILE, op::HELLO, op::FILE_SIZE]
            );
            assert_eq!(fw.file(CLIP), Some(small.as_slice()));
        }
    }

    #[test]
    fn playback_stops_media_first_and_waits_for_the_device() {
        let pauses = Pauses::default();
        let mut s = connected_with(&pauses, ROM_190, "turing-8.8");
        let base = Frame::filled(m88().panel, Rgba::BLACK);
        s.present(&base).unwrap();
        pauses.take();

        let video = path("sd/video/88.mp4");
        let before = s.wire().sent.len();
        script(&mut s, &["media_stop", "play_video_success"]);
        s.play_video(&video, Repeat::Loop).unwrap();
        let sent = since(&s, before);
        assert_eq!(
            commands(sent),
            [op::STOP_VIDEO, op::STOP_MEDIA, op::PLAY_VIDEO]
        );
        assert_eq!(
            sent[2],
            proto::play_video("/mnt/SDCARD/video/88.mp4", Repeat::Loop).unwrap()
        );
        assert_eq!(pauses.take(), [STOP_VIDEO_SETTLE]);

        // The next frame goes out full, after PRE_UPDATE_BITMAP (vendor order).
        let before = s.wire().sent.len();
        s.present(&base).unwrap();
        assert_eq!(
            commands(since(&s, before)),
            [op::PRE_UPDATE_BITMAP, op::DISPLAY_BITMAP]
        );

        // Not confirmed: sent once more, then an error.
        let before = s.wire().sent.len();
        script(&mut s, &["media_stop", "", "play_video_success"]);
        s.play_video(&video, Repeat::Once).unwrap();
        let plays = commands(since(&s, before))
            .into_iter()
            .filter(|o| *o == op::PLAY_VIDEO)
            .count();
        assert_eq!(plays, PLAY_VIDEO_TRIES);
        script(&mut s, &["media_stop"]);
        let err = s.play_video(&video, Repeat::Loop).unwrap_err();
        assert!(
            err.to_string()
                .contains("PLAY_VIDEO /mnt/SDCARD/video/88.mp4"),
            "{err}"
        );

        let image = path("internal/image/logo.png");
        let before = s.wire().sent.len();
        script(&mut s, &["media_stop", "play_img_ok"]);
        s.play_image(&image).unwrap();
        assert_eq!(
            since(&s, before)[2],
            proto::path_command(op::PLAY_IMAGE, "/mnt/UDISK/img/logo.png").unwrap()
        );
        let before = s.wire().sent.len();
        s.present(&base).unwrap();
        assert_eq!(
            commands(since(&s, before)),
            [op::PRE_UPDATE_BITMAP, op::DISPLAY_BITMAP],
            "a full frame after an image starts too"
        );
        script(&mut s, &["media_stop"]);
        assert!(matches!(s.play_image(&image), Err(BezelError::Timeout(_))));

        // Stop polls STOP_MEDIA until `media_stop`, 400 ms apart.
        pauses.take();
        let before = s.wire().sent.len();
        script(&mut s, &["", "busy", "media_stop"]);
        s.stop().unwrap();
        assert_eq!(
            commands(since(&s, before)),
            [
                op::STOP_VIDEO,
                op::STOP_MEDIA,
                op::STOP_MEDIA,
                op::STOP_MEDIA
            ]
        );
        assert_eq!(
            pauses.take(),
            [
                STOP_VIDEO_SETTLE,
                STOP_MEDIA_POLL_PAUSE,
                STOP_MEDIA_POLL_PAUSE
            ]
        );

        // Delete: one packet, no reply awaited.
        let before = s.wire().sent.len();
        s.delete(&image, confirmed()).unwrap();
        assert_eq!(
            since(&s, before),
            [
                proto::path_command(op::DELETE_FILE, "/mnt/UDISK/img/logo.png")
                    .unwrap()
                    .to_vec()
            ]
        );
    }

    #[test]
    fn boot_rewrites_options_keeping_the_last_brightness() {
        let mut s = connected();
        s.set_start_mode(StartMode::Video, confirmed()).unwrap();
        // § 17.2: brightness 170 (none sent yet: the vendor default), video,
        // no flip, no sleep.
        let expected = proto::set_options(Options {
            brightness: 170,
            start_mode: proto::StartMode::Video,
            flip: false,
            sleep_minutes: 0,
        });
        assert_eq!(s.wire().sent.last().unwrap(), &expected.to_vec());
        s.set_brightness(Brightness::new(25).unwrap()).unwrap();
        s.set_start_mode(StartMode::Image, confirmed()).unwrap();
        assert_eq!(
            &s.wire().sent.last().unwrap()[..15],
            &[0x7D, 0xEF, 0x69, 0, 0, 0, 5, 0, 0, 0, 64, 1, 0, 0, 0]
        );
        s.set_start_mode(StartMode::Default, confirmed()).unwrap();
        assert_eq!(&s.wire().sent.last().unwrap()[10..15], &[64, 0, 0, 0, 0]);
    }

    #[test]
    fn small_screens_store_under_root_and_wait_once() {
        let pauses = Pauses::default();
        let mut s = connected_with(&pauses, "chs_5inch.dev1_rom1.87", "turing-5");
        assert_eq!(s.class, ScreenClass::Small);
        let before = s.wire().sent.len();
        script(&mut s, &["file:"]);
        assert!(
            s.list(path("internal/image/x").location)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            since(&s, before)[0],
            proto::path_command(op::LIST_DIR, "/root/img/").unwrap()
        );
        pauses.take();
        script(&mut s, &["media_stop", "file:", "create_success"]);
        let (result, _) = run_upload(&mut s, &path("internal/video/a.mp4"), &[1, 2, 3], None);
        assert_eq!(result, Ok(()));
        assert_eq!(pauses.take(), [STOP_VIDEO_SETTLE], "one completion wait");
    }

    #[test]
    fn wrong_size_and_controls() {
        let mut s = connected();
        let small = Frame::filled(bezel_core::domain::geometry::Size::new(10, 10), Rgba::BLACK);
        assert!(s.present(&small).is_err());
        s.set_orientation(Orientation::Landscape).unwrap();
        let wide = Frame::filled(
            m88().panel.in_orientation(Orientation::Landscape),
            Rgba::BLACK,
        );
        s.present(&wide).unwrap();
        s.set_brightness(Brightness::new(25).unwrap()).unwrap();
        s.screen_off().unwrap();
        s.release().unwrap();
        let sent = &s.wire().sent;
        let n = sent.len();
        assert_eq!(
            &sent[n - 3][..11],
            &[0x7B, 0xEF, 0x69, 0, 0, 0, 1, 0, 0, 0, 64]
        );
        assert_eq!(sent[n - 2][0], op::TURN_OFF);
        assert_eq!(sent[n - 1][0], op::END_UPDATE_BITMAP);
    }

    #[test]
    fn model_choice() {
        let two: Vec<&'static DeviceModel> = ["turing-2.1", "turing-2.8"]
            .iter()
            .map(|id| model_by_id(ModelId(id)).unwrap())
            .collect();
        let hello = Hello::parse(b"chs_5inch.dev1_rom1.88").unwrap();
        assert!(pick_model(&hello, &two).is_none());
        let five = model_by_id(ModelId("turing-5")).unwrap();
        assert_eq!(
            pick_model(&hello, &[five, two[0]]).map(|m| m.id.0),
            Some("turing-5")
        );
        let odd = Hello::parse(b"chs_99inch.dev1_rom1.0").unwrap();
        assert!(pick_model(&odd, &two).is_none());
        assert_eq!(rgba_to_bgra(&[1, 2, 3, 4]), vec![3, 2, 1, 4]);
        assert_eq!(printable(b"ok\0\x01!"), "ok!");
        RealTime.pause(Duration::ZERO);
    }
}
