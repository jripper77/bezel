//! Driver for Turing rev C screens (2.1"/2.8"/5"/8.8" UART generation):
//! frames, and the stored files and device-side playback of spec § 13.
//!
//! Nothing storage-related is sent implicitly (spec § 16): the storage
//! commands, OPTIONS 0x7D and playback go out only from the
//! [`ScreenStorage`] methods, which the core's use cases call. 0x81 and 0x82
//! are never sent. RESTART 0x84 goes out only from
//! [`ScreenStorage::restart`], whose [`Confirmed`] proof only an explicit
//! user choice gives (D-2026-09-30-device-protocols-2): the `album` choice
//! applied when the computer shuts down (D-2026-10-03-power-off-standby-3).
//! On small screens the vendor sends 0x82 and re-initialises before
//! PLAY_VIDEO; Bezel does not (disruptive, and no small screen has been
//! validated).
//!
//! What a screen does when the computer shuts down goes out within the
//! shutdown's deadline and waits for nothing afterwards
//! (D-2026-10-03-power-off-standby-3): [`ScreenLink::turn_off_now`] is
//! TURNOFF 0x83 alone, [`ScreenStorage::restart`] RESTART 0x84 alone, and a
//! play stops what plays with one STOP_MEDIA (not the 20 polls of a theme
//! start) before PLAY_VIDEO; nothing follows any of them, 0x87 included.
//!
//! A live link whose frame stops changing still sends frame traffic: when
//! nothing went out for [`KEEPALIVE_AFTER`], an unchanged frame becomes the
//! smallest partial update, pixel 0 as it is, then QUERY_STATUS
//! (D-2026-10-03-power-off-standby-5), so that the screen's own sleep timer
//! (OPTIONS byte 14) never puts a live screen to sleep.
//!
//! A cancelled upload has no abort in the protocol: after the UPLOAD_FILE
//! header the firmware takes every byte as file data until it has the
//! declared length (spec § 19). Bezel sends nothing more of that length
//! after a cancel: completing it with filler hung the 8.8" until a USB
//! replug (D-2026-09-30-release-polish-10). The recovery is HELLO (with its
//! resync blocks), then GET_FILE_SIZE measures the partial file the user is
//! offered to delete. Bytes the firmware's writer still queued may land in
//! the next upload, whose size check (the core's) catches them.

use std::time::{Duration, Instant};

use bezel_core::domain::device::DeviceModel;
use bezel_core::domain::frame::{Frame, RGBA_BYTES};
use bezel_core::domain::geometry::Orientation;
use bezel_core::domain::job::Job;
use bezel_core::domain::media::MediaKind;
use bezel_core::domain::screen::{Brightness, ScreenIdentity};
use bezel_core::domain::standby::PlanB;
use bezel_core::domain::storage::{
    Confirmed, FileName, Medium, RemotePath, Repeat, StartMode, StorageInfo, StorageLocation,
};
use bezel_core::ports::{ScreenLink, ScreenStorage, TransferStats};
use bezel_core::{BezelError, Result};

use crate::driver::{
    Monotonic, Pause, Sent, SteadyClock, StorageRoots, check_frame, io_err, parse_listing,
    send_in_chunks, upload_size,
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
/// STOP_MEDIA polls while waiting for `media_stop` (the vendor's theme
/// start: at connect, before an upload and on an explicit stop).
const STOP_MEDIA_POLLS: usize = 20;
/// STOP_MEDIA polls before PLAY_VIDEO and PLAY_IMAGE: one. The 8.8" answers
/// the first (a play while another video played took 0.55 s end to end,
/// measured on 2026-10-03), the vendor restarts a stalled video with
/// STOP_VIDEO and PLAY_VIDEO alone (spec § 12.1), and the play at shutdown
/// must leave within the deadline: at most [`STOP_VIDEO_SETTLE`] +
/// [`REPLY_TIMEOUT`] before PLAY_VIDEO goes out, not the 20 polls (about
/// 28 s) of a firmware that never answers (D-2026-10-03-power-off-standby-3).
const PLAY_STOP_MEDIA_POLLS: usize = 1;
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
/// How long a live link may send nothing before an unchanged frame becomes
/// the keepalive (D-2026-10-03-power-off-standby-5): half the shortest sleep
/// timer the firmware takes (1 minute).
pub const KEEPALIVE_AFTER: Duration = Duration::from_secs(30);

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

/// A connected rev C screen. `C` tells the time of the keepalive.
pub struct TuringRevC<W: Wire, P: Pause, C: Monotonic = SteadyClock> {
    wire: W,
    pause: P,
    clock: C,
    /// When the last write went out.
    sent_at: Instant,
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
    transfer: TransferStats,
}

impl<W: Wire, P: Pause + Clone> TuringRevC<W, P> {
    /// Handshakes over `wire` and prepares the screen for streaming.
    /// `candidates` are the models discovery allowed; the HELLO answer picks one.
    pub fn connect(wire: W, pause: &P, candidates: &[&'static DeviceModel]) -> Result<Self> {
        Self::connect_with_clock(wire, pause, SteadyClock, candidates)
    }
}

impl<W: Wire, P: Pause + Clone, C: Monotonic> TuringRevC<W, P, C> {
    /// [`TuringRevC::connect`] with the clock of the keepalive.
    pub fn connect_with_clock(
        mut wire: W,
        pause: &P,
        clock: C,
        candidates: &[&'static DeviceModel],
    ) -> Result<Self> {
        let hello = handshake(&mut wire, pause)?;
        tracing::debug!(reply = %hello.raw, rom = hello.rom, "HELLO");
        let model = pick_model(&hello, candidates).ok_or_else(|| {
            BezelError::Transport(format!("unexpected screen model: {}", hello.raw))
        })?;
        let sent_at = clock.now();
        let mut screen = Self {
            wire,
            pause: pause.clone(),
            clock,
            sent_at,
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
            transfer: TransferStats::default(),
            options: Options {
                brightness: DEFAULT_STORED_BRIGHTNESS,
                start_mode: proto::StartMode::Default,
                flip: false,
                sleep_minutes: 0,
            },
        };
        screen.stop_media(STOP_MEDIA_POLLS)?;
        screen.enter_streaming()?;
        Ok(screen)
    }
}

impl<W: Wire, P: Pause, C: Monotonic> TuringRevC<W, P, C> {
    /// The wire, for tests and diagnostics.
    pub fn wire(&self) -> &W {
        &self.wire
    }

    fn send(&mut self, bytes: &[u8]) -> Result<()> {
        let started = Instant::now();
        let result = self.wire.send(bytes).map_err(io_err);
        self.transfer.write_ms += started.elapsed().as_secs_f64() * 1000.0;
        result?;
        self.transfer.bytes += bytes.len() as u64;
        self.sent_at = self.clock.now();
        Ok(())
    }

    fn enter_streaming(&mut self) -> Result<()> {
        self.send(&proto::simple(op::PRE_UPDATE_BITMAP))?;
        self.streaming = true;
        Ok(())
    }

    /// STOP_VIDEO, then STOP_MEDIA until the device says `media_stop`, at
    /// most `polls` times.
    fn stop_media(&mut self, polls: usize) -> Result<()> {
        self.send(&proto::simple(op::STOP_VIDEO))?;
        self.pause.pause(STOP_VIDEO_SETTLE);
        for poll in 1..=polls {
            self.send(&proto::simple(op::STOP_MEDIA))?;
            let answer = self
                .wire
                .receive(REPLY_MAX, REPLY_TIMEOUT)
                .map_err(io_err)?;
            if String::from_utf8_lossy(&answer).contains(reply::MEDIA_STOPPED) {
                return Ok(());
            }
            if poll < polls {
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

    /// Stops device-side playback (before uploads and on request), polling
    /// STOP_MEDIA at most `polls` times.
    fn stop_playback(&mut self, polls: usize) -> Result<()> {
        self.media_changed();
        self.stop_media(polls)
    }

    fn native(&self, frame: &Frame) -> Result<Vec<u8>> {
        let model = self.identity.model;
        check_frame(model, self.orientation, frame)?;
        let turns = self.orientation.quarter_turns_to(model.native_orientation);
        Ok(rgba_to_bgra(frame.rotated(turns).as_rgba()))
    }

    fn full_frame(&mut self, bgra: Vec<u8>) -> Result<()> {
        self.transfer.kind = "full";
        if !self.streaming {
            self.enter_streaming()?;
        }
        self.send(&proto::start_display_block())?;
        self.send(&proto::full_frame_header(bgra.len() as u32))?;
        self.send(&proto::blocks(&bgra))?;
        // The device may say something after a frame; drain it so it does not
        // pollute the next reply.
        let reply_started = Instant::now();
        let after = self
            .wire
            .receive(REPLY_MAX, FRAME_REPLY_WAIT)
            .map_err(io_err);
        self.transfer.reply_ms += reply_started.elapsed().as_secs_f64() * 1000.0;
        let after = after?;
        tracing::debug!(bytes = bgra.len(), reply = %printable(&after), "full frame");
        self.last = Some(bgra);
        self.seq = 0;
        Ok(())
    }

    /// A partial update carrying `list`, then QUERY_STATUS: a full frame
    /// next when the device asks for one.
    fn partial(&mut self, mut list: Vec<u8>, bgra: Vec<u8>) -> Result<()> {
        self.transfer.kind = "partial";
        list.extend_from_slice(&proto::MAGIC);
        self.send(&proto::partial_header(list.len() as u32, self.seq))?;
        self.send(&proto::blocks(&list))?;
        self.seq = self.seq.wrapping_add(1);
        self.last = Some(bgra);
        if self.needs_full_frame()? {
            self.last = None;
        }
        Ok(())
    }

    /// An unchanged frame: nothing, unless nothing went out for
    /// [`KEEPALIVE_AFTER`]; then pixel 0 as it is
    /// ([`proto::keepalive_run`]) and QUERY_STATUS, as after every partial
    /// (D-2026-10-03-power-off-standby-5).
    fn keep_awake(&mut self, bgra: Vec<u8>) -> Result<()> {
        let quiet = self.clock.now().saturating_duration_since(self.sent_at);
        if quiet < KEEPALIVE_AFTER {
            return Ok(());
        }
        tracing::debug!(?quiet, "keepalive");
        let list = proto::keepalive_run(&bgra, self.format);
        self.partial(list, bgra)
    }

    /// QUERY_STATUS round-trip; `true` when the device asks for a full frame.
    fn needs_full_frame(&mut self) -> Result<bool> {
        self.send(&proto::simple(op::QUERY_STATUS))?;
        let reply_started = Instant::now();
        let answer = self
            .wire
            .receive_status(REPLY_MAX, REPLY_TIMEOUT)
            .map_err(io_err);
        self.transfer.reply_ms += reply_started.elapsed().as_secs_f64() * 1000.0;
        let answer = answer?;
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
    /// recovers the link like a cancel between blocks.
    fn await_received(&mut self, path: &RemotePath, job: &Job<'_>) -> Result<()> {
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
                    return Err(self.recover_after_cancel(path));
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

    /// After an interrupted upload (spec § 19). Nothing more of the data
    /// phase goes out: filler that completed the declared length hung the
    /// 8.8" until a USB replug (D-2026-09-30-release-polish-10). HELLO (with
    /// its resync blocks) brings the link back and GET_FILE_SIZE measures the
    /// partial file, for the user to delete. When no HELLO is answered the
    /// link is left for the next command, which wakes the screen. Bytes the
    /// firmware's writer still queued may land in the next upload; its size
    /// check catches them. Never deletes.
    fn recover_after_cancel(&mut self, path: &RemotePath) -> BezelError {
        self.media_changed();
        if let Err(e) = handshake(&mut self.wire, &self.pause) {
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

    fn device_path(&self, path: &RemotePath) -> Result<String> {
        self.roots().path(path)
    }
}

impl<W: Wire, P: Pause, C: Monotonic> ScreenLink for TuringRevC<W, P, C> {
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
        self.transfer = TransferStats {
            kind: "unchanged",
            ..TransferStats::default()
        };
        let started = Instant::now();
        let bgra = self.native(frame)?;
        self.transfer.conversion_ms = started.elapsed().as_secs_f64() * 1000.0;
        let Some(last) = self.last.as_ref() else {
            return self.full_frame(bgra);
        };
        let started = Instant::now();
        let diff = proto::diff_runs(last, &bgra, self.format);
        self.transfer.diff_ms = started.elapsed().as_secs_f64() * 1000.0;
        match diff {
            None => self.full_frame(bgra),
            Some(list) if list.is_empty() => self.keep_awake(bgra),
            Some(list) => self.partial(list, bgra),
        }
    }

    fn last_transfer(&self) -> Option<TransferStats> {
        Some(self.transfer)
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

    /// TURNOFF 0x83 alone: nothing is read and nothing waited for (the SoC
    /// leaves the bus about 3 s later, spec § 19); the `off` choice when the
    /// computer shuts down (D-2026-10-03-power-off-standby-3).
    fn turn_off_now(&mut self) -> Result<()> {
        self.last = None;
        tracing::info!("TURNOFF");
        self.send(&proto::simple(op::TURN_OFF))
    }

    fn release(&mut self) -> Result<()> {
        self.last = None;
        self.send(&proto::simple(op::END_UPDATE_BITMAP))
    }

    fn storage(&mut self) -> Option<&mut dyn ScreenStorage> {
        Some(self)
    }
}

impl<W: Wire, P: Pause, C: Monotonic> ScreenStorage for TuringRevC<W, P, C> {
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
    /// A cancel ends the data phase where it is, without filler, then
    /// recovers the link with HELLO and measures the partial file.
    fn upload(&mut self, path: &RemotePath, data: &[u8], job: &mut Job<'_>) -> Result<()> {
        let size = upload_size(data)?;
        let target = self.device_path(path)?;
        let header = proto::upload_file(&target, size).ok_or_else(|| too_long(&target))?;
        job.checkpoint()?;
        self.stop_playback(STOP_MEDIA_POLLS)?;
        let folder = self.roots().folder(path.location);
        self.list_folder(&folder)?;
        job.checkpoint()?;
        let what = format!("UPLOAD_FILE {target}");
        self.request(&header, CREATE, &what, has(reply::CREATED))?;
        tracing::info!(%target, size, "upload");
        let (wire, clock, sent_at) = (&mut self.wire, &self.clock, &mut self.sent_at);
        let sent = send_in_chunks(data, UPLOAD_CHUNK, job, |chunk| {
            wire.send(&proto::blocks(chunk)).map_err(io_err)?;
            *sent_at = clock.now();
            Ok(())
        })?;
        match sent {
            Sent::All => self.await_received(path, job),
            Sent::Cancelled { accepted } => {
                tracing::info!(%target, accepted, "upload cancelled");
                Err(self.recover_after_cancel(path))
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
        self.stop_playback(PLAY_STOP_MEDIA_POLLS)?;
        let what = format!("PLAY_VIDEO {target}");
        self.request(&packet, PLAY_VIDEO, &what, has(reply::VIDEO_PLAYING))
    }

    fn play_image(&mut self, path: &RemotePath) -> Result<()> {
        let target = self.device_path(path)?;
        let packet = path_packet(op::PLAY_IMAGE, &target)?;
        self.stop_playback(PLAY_STOP_MEDIA_POLLS)?;
        let what = format!("PLAY_IMAGE {target}");
        self.request(&packet, PLAY_IMAGE, &what, has(reply::IMAGE_SHOWN))
    }

    fn stop(&mut self) -> Result<()> {
        self.stop_playback(STOP_MEDIA_POLLS)
    }

    /// OPTIONS 0x7D written whole: the last brightness this link sent (the
    /// vendor default before any), `plan`'s start mode, no flip and
    /// `plan`'s sleep timer.
    fn set_options(&mut self, plan: PlanB, _confirmed: Confirmed) -> Result<()> {
        self.options.start_mode = match plan.start_mode {
            StartMode::Default => proto::StartMode::Default,
            StartMode::Image => proto::StartMode::Image,
            StartMode::Video => proto::StartMode::Video,
        };
        self.options.flip = false;
        self.options.sleep_minutes = plan.sleep_minutes;
        tracing::info!(options = ?self.options, "OPTIONS");
        self.send(&proto::set_options(self.options))
    }

    /// RESTART 0x84 alone, waiting for nothing: the SoC restarts into its
    /// start mode and the link is gone (the `album` choice when the computer
    /// shuts down, D-2026-10-03-power-off-standby-3).
    fn restart(&mut self, _confirmed: Confirmed) -> Result<()> {
        self.media_changed();
        tracing::info!("RESTART");
        self.send(&proto::simple(op::RESTART))
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
    use bezel_core::app::standby::{self, Applied};
    use bezel_core::app::storage::{self, PreparedUpload};
    use bezel_core::domain::archive::{Catalog, ContentId, ScreenKey};
    use bezel_core::domain::catalog::model_by_id;
    use bezel_core::domain::device::ModelId;
    use bezel_core::domain::frame::{Rect, Rgba};
    use bezel_core::domain::job::{CancelToken, Progress};
    use bezel_core::domain::media::{
        MediaFormat, MediaInfo, MediaTools, StreamSpec, TranscodeTarget,
    };
    use bezel_core::domain::screen::Confirm;
    use bezel_core::domain::standby::{
        PlanB as Plan, SleepMinutes, Standby, StoredPlanB, Unavailable,
    };
    use bezel_core::domain::storage::{BootMedia, Capacity, Operation, UploadAction, UploadPlan};
    use bezel_core::ports::{ArchiveStore, MediaLocation, MediaTranscoder, VideoFrames};

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
        let stats = s.last_transfer().unwrap();
        assert_eq!(stats.kind, "full");
        assert_eq!(
            stats.bytes,
            sent[n - 3..].iter().map(|b| b.len() as u64).sum::<u64>()
        );
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
        assert_eq!(s.last_transfer().unwrap().bytes, 0);
        assert_eq!(s.last_transfer().unwrap().kind, "unchanged");

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
        let stats = s.last_transfer().unwrap();
        assert_eq!(stats.kind, "partial");
        assert_eq!(
            stats.bytes,
            sent.iter().map(|b| b.len() as u64).sum::<u64>()
        );
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

        // Cancelled after the first write: nothing more of the data phase
        // goes out, no filler (D-2026-09-30-release-polish-10); HELLO puts
        // the link back and GET_FILE_SIZE measures the partial. Nothing is
        // deleted.
        let mut s = connected();
        let before = s.wire().sent.len();
        let arrived = chunk.to_string();
        script(
            &mut s,
            &[
                "media_stop",
                "nodir-createdone",
                "create_success",
                ROM_190,
                &arrived,
            ],
        );
        let (result, progress) = run_upload(&mut s, &clip, &data, Some(1));
        assert_eq!(
            result,
            Err(BezelError::Cancelled {
                partial: Some(chunk)
            })
        );
        assert_eq!(
            progress,
            [(0, total), (chunk, total)],
            "the counters stop at the cancel"
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
        assert_eq!(sent[4], proto::blocks(&data[..UPLOAD_CHUNK]));
        assert_eq!(sent[5], proto::hello(), "HELLO right after the data");
        assert_eq!(
            sent.iter().filter(|w| !is_command(w)).count(),
            1,
            "one data write, no filler"
        );
        assert!(!s.streaming && s.last.is_none(), "the next frame is full");

        // Cancelled before the first write, the header accepted: not one
        // data byte follows. A screen that then finds no file leaves no
        // partial to offer for deletion.
        let mut s = connected();
        let before = s.wire().sent.len();
        script(
            &mut s,
            &[
                "media_stop",
                "nodir-createdone",
                "create_success",
                ROM_190,
                "0",
            ],
        );
        let (result, _) = run_upload(&mut s, &clip, &data, Some(0));
        assert_eq!(result, Err(BezelError::Cancelled { partial: None }));
        let sent = since(&s, before);
        assert!(sent.iter().all(|w| is_command(w)), "no data write");
        assert_eq!(
            commands(sent)[3..],
            [op::UPLOAD_FILE, op::HELLO, op::FILE_SIZE]
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

        // No HELLO is answered after the cancel: its tries with their
        // resync blocks and nothing else, then the error that says the next
        // command reconnects and which path to check for a partial file.
        let pauses = Pauses::default();
        let mut s = connected_with(&pauses, ROM_190, "turing-8.8");
        pauses.take();
        let before = s.wire().sent.len();
        script(
            &mut s,
            &["media_stop", "nodir-createdone", "create_success"],
        );
        let file = test_file(UPLOAD_CHUNK + 1);
        let (result, _) = run_upload(&mut s, &clip, &file, Some(1));
        assert_eq!(
            result,
            Err(BezelError::Timeout(
                "the screen after a cancelled upload; the next command reconnects it, \
                 then check sd/video/clip.mp4 for a partial file"
                    .into()
            ))
        );
        let sent = since(&s, before);
        assert_eq!(sent[4], proto::blocks(&file[..UPLOAD_CHUNK]));
        let round = [
            proto::hello().to_vec(),
            proto::start_display_block().to_vec(),
        ];
        let rounds: Vec<&[Vec<u8>]> = sent[5..].chunks(2).collect();
        assert_eq!(rounds, vec![&round[..]; HELLO_TRIES]);
        let waits = pauses.take();
        assert_eq!(waits[0], STOP_VIDEO_SETTLE);
        assert_eq!(waits[1..], [HELLO_RETRY_PAUSE; HELLO_TRIES]);

        // HELLO answered but GET_FILE_SIZE not: that error, as for any query.
        let mut s = connected();
        script(
            &mut s,
            &["media_stop", "nodir-createdone", "create_success", ROM_190],
        );
        let (result, _) = run_upload(&mut s, &clip, &file, Some(1));
        let err = result.unwrap_err();
        assert!(matches!(err, BezelError::Timeout(_)), "{err}");
        assert!(
            err.to_string()
                .contains("GET_FILE_SIZE /mnt/SDCARD/video/clip.mp4"),
            "{err}"
        );
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

    /// Wire bytes of the data phase of a `size`-byte file: 250-byte blocks
    /// of 249 file bytes.
    fn wire_len(size: usize) -> usize {
        size.div_ceil(proto::BLOCK_PAYLOAD) * BLOCK
    }

    /// What the modelled firmware does when the host waits for an answer in
    /// the middle of a data phase (after a HELLO it took as file data).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Idle {
        /// Keeps waiting for the declared length: no HELLO is answered on
        /// that link, the next connection wakes the screen (the 8.8"'s
        /// first cancelled run, spec § 19).
        Waits,
        /// Leaves the data phase: the next HELLO is answered, but what its
        /// writer still queued goes into the next file (its later run).
        Leaves,
    }

    /// File bytes the modelled writer lags behind the wire in a data phase.
    const WRITER_QUEUE: usize = 10_000;

    /// Device paths of the files these tests upload to the card.
    const CLIP: &str = "/mnt/SDCARD/video/clip.mp4";
    const LOGO: &str = "/mnt/SDCARD/img/logo.png";

    /// GET_STORAGE_INFO answers of the 8.8" with and without its card
    /// (spec § 13.2, § 19).
    const CARD_REPORT: &str = "7340032-1048576-6291456-31260672-2048-31258624";
    const NO_CARD_REPORT: &str = "7340032-1048576-6291456-0-0-0";

    /// What a data phase carries.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Phase {
        /// A file (UPLOAD_FILE).
        Upload,
        /// A full frame (DISPLAY_BITMAP): native BGRA.
        Full,
        /// A partial update (UPDATE_BITMAP): a run list and `EF 69`.
        Partial,
    }

    /// A data phase the modelled firmware receives.
    struct Receiving {
        phase: Phase,
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
    /// bytes go into the next file opened. DELETE_FILE removes a file. Not
    /// modelled: the hang that completing a cancelled data phase with filler
    /// caused (D-2026-09-30-release-polish-10); the tests check that the
    /// driver sends none.
    ///
    /// It also shows frames (full frames and raw BGRA partials into
    /// [`Self::screen`], `full_png_sucess`, QUERY_STATUS answered
    /// `needReSend:0`), reports its storage (with or without a card), plays
    /// a stored video (`play_video_success`), keeps OPTIONS, and takes
    /// TURNOFF and RESTART (spec § 6.2, § 9.2, § 13.5, § 19).
    struct Firmware {
        idle: Idle,
        /// A memory card is inserted.
        card: bool,
        /// STOP_MEDIA is never answered (older firmware, spec § 7.2).
        silent_stop: bool,
        /// What the panel shows, native BGRA.
        screen: Vec<u8>,
        /// The five OPTIONS bytes last written.
        options: Option<[u8; 5]>,
        /// The stored video playing, and its loop flag.
        playing: Option<(String, bool)>,
        /// TURNOFF arrived.
        off: bool,
        /// RESTART arrived.
        restarted: bool,
        /// Every read: the writes before it and how long it may wait.
        reads: Vec<(usize, Duration)>,
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
                card: false,
                silent_stop: false,
                screen: Vec::new(),
                options: None,
                playing: None,
                off: false,
                restarted: false,
                reads: Vec::new(),
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
                op::STOP_VIDEO => {
                    self.playing = None;
                    return;
                }
                op::STOP_MEDIA if self.silent_stop => return,
                op::STOP_MEDIA => {
                    self.playing = None;
                    reply::MEDIA_STOPPED.to_string()
                }
                op::STORAGE_INFO if self.card => CARD_REPORT.to_string(),
                op::STORAGE_INFO => NO_CARD_REPORT.to_string(),
                op::LIST_DIR => self.listing(&named()),
                op::FILE_SIZE => self.file(&named()).map_or(0, <[u8]>::len).to_string(),
                op::UPLOAD_FILE => {
                    let size = u32::from_le_bytes(packet[10 + n..14 + n].try_into().unwrap());
                    self.receive_data(Phase::Upload, named(), size as usize);
                    reply::CREATED.to_string()
                }
                op::DISPLAY_BITMAP => return self.receive_data(Phase::Full, String::new(), n),
                op::UPDATE_BITMAP => return self.receive_data(Phase::Partial, String::new(), n),
                op::QUERY_STATUS => "needReSend:0|renderCnt:0|theme:".to_string(),
                op::PLAY_VIDEO => {
                    let target = named();
                    if self.file(&target).is_none() {
                        return;
                    }
                    self.playing = Some((target, packet[7] == 1));
                    reply::VIDEO_PLAYING.to_string()
                }
                op::SET_OPTIONS => {
                    self.options = Some(packet[10..15].try_into().unwrap());
                    return;
                }
                op::TURN_OFF => {
                    self.off = true;
                    return;
                }
                op::RESTART => {
                    self.restarted = true;
                    return;
                }
                op::DELETE_FILE => {
                    let target = named();
                    self.files.retain(|(p, _)| *p != target);
                    return;
                }
                _ => return,
            };
            self.replies.push_back(answer.into_bytes());
        }

        /// A data phase of `size` bytes follows, as 250-byte blocks.
        fn receive_data(&mut self, phase: Phase, path: String, size: usize) {
            self.receiving = Some(Receiving {
                phase,
                path,
                size,
                owed: wire_len(size),
                payload: Vec::new(),
                block: Vec::new(),
            });
        }

        /// A partial's run list (raw BGRA records, spec § 9.2) drawn on
        /// [`Self::screen`].
        fn draw(&mut self, list: &[u8]) {
            let mut runs = list.strip_suffix(&proto::MAGIC).expect("ends with EF 69");
            while !runs.is_empty() {
                let idx = u32::from_be_bytes([0, runs[0], runs[1], runs[2]]) as usize;
                let (start, count, head) = if idx & 0x80_0000 == 0 {
                    (idx, usize::from(u16::from_be_bytes([runs[3], runs[4]])), 5)
                } else {
                    (idx & 0x7F_FFFF, 1, 3)
                };
                let pixels = &runs[head..head + count * 4];
                self.screen[start * 4..(start + count) * 4].copy_from_slice(pixels);
                runs = &runs[head + count * 4..];
            }
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

        /// The declared length arrived: a frame is shown; a file is
        /// written whole (after what the writer still held) and closed.
        fn close(&mut self) {
            let Some(data) = self.receiving.take() else {
                return;
            };
            let payload = &data.payload[..data.size];
            match data.phase {
                Phase::Full => {
                    self.screen = payload.to_vec();
                    self.replies.push_back(b"full_png_sucess".to_vec());
                }
                Phase::Partial => self.draw(payload),
                Phase::Upload => {
                    let mut content = std::mem::take(&mut self.queued);
                    content.extend_from_slice(payload);
                    self.store(data.path, content);
                    self.replies.push_back(reply::RECEIVED.into());
                }
            }
        }

        /// The data phase of an upload ends short of its declared length:
        /// the file keeps what the writer wrote, the rest waits for the next
        /// file.
        fn leave(&mut self) {
            let upload = self.receiving.take_if(|r| r.phase == Phase::Upload);
            if let Some(upload) = upload {
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
        fn receive(&mut self, max: usize, timeout: Duration) -> io::Result<Vec<u8>> {
            self.reads.push((self.sent.len(), timeout));
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
    fn a_cancel_sends_no_filler_and_the_next_upload_fails_its_size_check() {
        let data = test_file(UPLOAD_CHUNK * 3 + 1000);
        let clip = path("sd/video/clip.mp4");
        // The 7,444-byte PNG uploaded after the cancel on the 8.8".
        let png = test_file(7_444);
        let image = path("sd/image/logo.png");
        // With a firmware that answers HELLO after a cancel and one that
        // does not.
        for idle in [Idle::Leaves, Idle::Waits] {
            // Cancelled after the first write.
            let mut s = TuringRevC::connect(Firmware::new(idle), &NoPause, &[m88()]).unwrap();
            let (result, _) = run_upload(&mut s, &clip, &data, Some(1));
            let fw = s.wire();
            assert_eq!(
                data_phase_bytes(&fw.sent, op::HELLO),
                wire_len(UPLOAD_CHUNK) as u64,
                "{idle:?}: the data, then HELLO: no filler"
            );
            let mut s = match idle {
                Idle::Leaves => {
                    // The HELLO after the resync block is answered and the
                    // partial measured: shorter than what the screen
                    // accepted, the rest queued for its writer.
                    let partial = fw.file(CLIP).unwrap().len();
                    assert!(partial < UPLOAD_CHUNK, "{partial}");
                    assert_eq!(
                        result,
                        Err(BezelError::Cancelled {
                            partial: Some(partial as u64)
                        })
                    );
                    assert_eq!(
                        fw.commands[fw.commands.len() - 3..],
                        [op::UPLOAD_FILE, op::HELLO, op::FILE_SIZE]
                    );
                    s
                }
                Idle::Waits => {
                    // Every HELLO was file data: the reconnect error, then
                    // the next connection wakes the screen.
                    assert!(matches!(result, Err(BezelError::Timeout(_))), "{result:?}");
                    assert_eq!(fw.commands.last(), Some(&op::UPLOAD_FILE));
                    let mut fw = s.wire;
                    fw.wake();
                    TuringRevC::connect(fw, &NoPause, &[m88()]).unwrap()
                }
            };
            let partial = s.size(&clip).unwrap();
            assert!(
                partial.is_some_and(|n| n < data.len() as u64),
                "{idle:?}: a partial to delete, {partial:?}"
            );
            assert_eq!(s.wire().queued.len(), WRITER_QUEUE, "{idle:?}");

            // The next upload takes the queued bytes: the core's size check
            // fails and says what to do. Nothing is deleted for the user.
            let stored = WRITER_QUEUE + png.len();
            assert_eq!(
                upload_as_the_core_does(&mut s, &image, &png),
                Err(BezelError::SizeMismatch {
                    path: image.clone(),
                    sent: png.len() as u64,
                    stored: stored as u64,
                }),
                "{idle:?}"
            );
            let fw = s.wire();
            assert_eq!(fw.file(LOGO).unwrap()[WRITER_QUEUE..], png);
            let destructive = [op::DELETE_FILE, op::RESTART, 0x82, op::SET_OPTIONS];
            assert!(
                fw.commands.iter().all(|o| !destructive.contains(o)),
                "{idle:?}"
            );

            // Deleted and sent again, as the error says: exact.
            storage::delete(&mut s, &image, Confirm::Yes).unwrap();
            assert_eq!(
                upload_as_the_core_does(&mut s, &image, &png),
                Ok(png.len() as u64),
                "{idle:?}"
            );
            assert_eq!(s.wire().file(LOGO), Some(png.as_slice()), "{idle:?}");
        }
    }

    #[test]
    fn a_cancel_while_the_screen_writes_keeps_the_file_whole() {
        // The whole file went out: the firmware closes it, HELLO is answered
        // at once and the next upload is exact.
        let small = test_file(1000);
        let clip = path("sd/video/clip.mp4");
        let png = test_file(700);
        let image = path("sd/image/logo.png");
        for idle in [Idle::Leaves, Idle::Waits] {
            let mut s = TuringRevC::connect(Firmware::new(idle), &NoPause, &[m88()]).unwrap();
            let (result, _) = run_upload(&mut s, &clip, &small, Some(1000));
            assert_eq!(
                result,
                Err(BezelError::Cancelled {
                    partial: Some(1000)
                }),
                "{idle:?}"
            );
            let fw = s.wire();
            assert_eq!(
                fw.commands[fw.commands.len() - 3..],
                [op::UPLOAD_FILE, op::HELLO, op::FILE_SIZE]
            );
            assert_eq!(fw.file(CLIP), Some(small.as_slice()));
            assert!(fw.queued.is_empty());
            assert_eq!(
                upload_as_the_core_does(&mut s, &image, &png),
                Ok(png.len() as u64),
                "{idle:?}"
            );
        }
    }

    /// The host side of the core's upload use case for one prepared file:
    /// its bytes.
    struct HostFile(Vec<u8>);

    impl MediaTranscoder for HostFile {
        fn tools(&mut self) -> MediaTools {
            MediaTools::Missing {
                install_hints: Vec::new(),
            }
        }

        fn probe(&mut self, _: &MediaLocation) -> Result<MediaInfo> {
            unreachable!("the upload comes prepared")
        }

        fn transcode(
            &mut self,
            _: &MediaLocation,
            _: &TranscodeTarget,
            _: &mut Job<'_>,
        ) -> Result<MediaLocation> {
            unreachable!("the file is sent as it is")
        }

        fn load(&mut self, _: &MediaLocation) -> Result<Vec<u8>> {
            Ok(self.0.clone())
        }

        fn stream(&mut self, _: &MediaLocation, _: StreamSpec) -> Result<Box<dyn VideoFrames>> {
            unreachable!("nothing is streamed")
        }
    }

    /// Uploads `file` to `target` through the core's use case, as the CLI
    /// and the studio do: sent, then its stored size checked. The bytes
    /// stored, or the use case's error.
    fn upload_as_the_core_does(
        link: &mut dyn ScreenLink,
        target: &RemotePath,
        file: &[u8],
    ) -> Result<u64> {
        let bytes = file.len() as u64;
        let prepared = PreparedUpload {
            source: MediaLocation("logo.png".into()),
            media: MediaInfo {
                format: MediaFormat::Png,
                bytes,
                dimensions: None,
                video: None,
                has_audio: false,
            },
            plan: UploadPlan {
                path: target.clone(),
                action: UploadAction::AsIs { bytes },
                replaces: None,
            },
        };
        let token = CancelToken::new();
        let mut sink = |_: Progress| {};
        let mut job = Job::new(&token, &mut sink);
        let mut host = HostFile(file.to_vec());
        storage::upload(link, &mut host, &prepared, Confirm::No, &mut job).map(|u| u.bytes)
    }

    /// How a [`BulkWire`] takes its writes longer than one packet (the data
    /// writes), in order; later ones are taken.
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

    /// Uploads `data` to the card's videos of a [`Firmware`] that answers
    /// HELLO after a cancel, behind a [`BulkWire`] following `plan` (only
    /// the wire cancels).
    fn bulk_upload(plan: &[Bulk], data: &[u8]) -> (Result<()>, BulkScreen) {
        let token = CancelToken::new();
        let wire = BulkWire {
            inner: Firmware::new(Idle::Leaves),
            token: token.clone(),
            plan: plan.to_vec(),
            bulk: 0,
        };
        let mut s = TuringRevC::connect(wire, &NoPause, &[m88()]).unwrap();
        let mut sink = |_: Progress| {};
        let mut job = Job::new(&token, &mut sink);
        let result = s.upload(&path("sd/video/clip.mp4"), data, &mut job);
        (result, s)
    }

    #[test]
    fn a_write_the_cancel_cut_short_is_followed_by_hello_only() {
        let data = test_file(UPLOAD_CHUNK * 3 + 1000);
        // The cancel's signal cuts the second write after or before its
        // bytes left: HELLO comes next either way, then the size query.
        for (step, kept) in [
            (Bulk::CutAfterSending, 2 * UPLOAD_CHUNK),
            (Bulk::CutBeforeSending, UPLOAD_CHUNK),
        ] {
            let (result, s) = bulk_upload(&[Bulk::Taken, step], &data);
            let fw = &s.wire().inner;
            assert_eq!(
                data_phase_bytes(&fw.sent, op::HELLO),
                wire_len(kept) as u64,
                "{step:?}"
            );
            let partial = fw.file(CLIP).map(|f| f.len() as u64);
            assert!(
                partial.is_some_and(|n| n < kept as u64),
                "{step:?}: {partial:?}"
            );
            assert_eq!(result, Err(BezelError::Cancelled { partial }), "{step:?}");
        }

        // The screen is unplugged mid-upload, nothing cancelled: the
        // write's error, and no recovery is tried.
        let (result, s) = bulk_upload(&[Bulk::Taken, Bulk::Unplugged], &data);
        assert_eq!(result, Err(BezelError::Transport("unplugged".into())));
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
        let before = s.wire().sent.len();
        s.set_options(PlanB::new(StartMode::Video, 0), confirmed())
            .unwrap();
        // § 17.2: brightness 170 (none sent yet: the vendor default), video,
        // no flip, no sleep.
        let expected = proto::set_options(Options {
            brightness: 170,
            start_mode: proto::StartMode::Video,
            flip: false,
            sleep_minutes: 0,
        });
        assert_eq!(since(&s, before), [expected.to_vec()], "one packet");
        s.set_brightness(Brightness::new(25).unwrap()).unwrap();
        s.set_options(PlanB::new(StartMode::Image, 0), confirmed())
            .unwrap();
        assert_eq!(
            &s.wire().sent.last().unwrap()[..15],
            &[0x7D, 0xEF, 0x69, 0, 0, 0, 5, 0, 0, 0, 64, 1, 0, 0, 0]
        );
        // The plan B's timer is written whole, then undone.
        s.set_options(PlanB::new(StartMode::Default, 5), confirmed())
            .unwrap();
        assert_eq!(&s.wire().sent.last().unwrap()[10..15], &[64, 0, 0, 0, 5]);
        s.set_options(PlanB::new(StartMode::Default, 0), confirmed())
            .unwrap();
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

    // What a screen does when the computer shuts down, through the core's
    // use cases on the firmware simulator (D-2026-10-03-power-off-standby-2,
    // -3, -5): the exact packets, and nothing else.

    /// A clock the tests move by hand.
    #[derive(Clone)]
    struct Hands(Arc<Mutex<Instant>>);

    impl Hands {
        fn new() -> Self {
            Self(Arc::new(Mutex::new(Instant::now())))
        }

        fn advance(&self, by: Duration) {
            *self.0.lock().unwrap_or_else(PoisonError::into_inner) += by;
        }
    }

    impl Monotonic for Hands {
        fn now(&self) -> Instant {
            *self.0.lock().unwrap_or_else(PoisonError::into_inner)
        }
    }

    type Simulated<P = NoPause> = TuringRevC<Firmware, P, Hands>;

    /// Device paths of the videos stored on the simulated screens.
    const LOOP: &str = "/mnt/SDCARD/video/loop.mp4";
    const INTRO: &str = "/mnt/UDISK/video/intro.mp4";

    /// The commands no standby path sends (D-2026-10-03-power-off-standby-2
    /// (5)): delete, upload, rotation, 0x82, leaving the stream (0x87) and
    /// listings (which create folders).
    const NEVER: [u8; 6] = [
        op::DELETE_FILE,
        op::UPLOAD_FILE,
        op::SET_ROTATION,
        0x82,
        op::END_UPDATE_BITMAP,
        op::LIST_DIR,
    ];

    /// A live 8.8" on the simulator (a card when `card`, two videos
    /// stored): brightness 25 % (64) sent and a first frame shown.
    fn live_screen<P: Pause + Clone>(pause: &P, card: bool) -> (Simulated<P>, Hands) {
        let mut fw = Firmware::new(Idle::Waits);
        fw.card = card;
        fw.store(LOOP.into(), test_file(1000));
        fw.store(INTRO.into(), test_file(500));
        let hands = Hands::new();
        let mut s = TuringRevC::connect_with_clock(fw, pause, hands.clone(), &[m88()]).unwrap();
        s.set_brightness(Brightness::new(25).unwrap()).unwrap();
        let frame = Frame::filled(m88().panel, Rgba::opaque(10, 20, 30));
        s.present(&frame).unwrap();
        (s, hands)
    }

    /// The writes `s` made after its first `from`.
    fn writes<P: Pause>(s: &Simulated<P>, from: usize) -> &[Vec<u8>] {
        &s.wire().sent[from..]
    }

    fn packet(bytes: [u8; BLOCK]) -> Vec<u8> {
        bytes.to_vec()
    }

    /// OPTIONS as a plan B writes it: the link's brightness (64), `mode`,
    /// 0, no flip, `sleep`.
    fn options(mode: proto::StartMode, sleep: u8) -> Vec<u8> {
        packet(proto::set_options(Options {
            brightness: 64,
            start_mode: mode,
            flip: false,
            sleep_minutes: sleep,
        }))
    }

    fn size_query(target: &str) -> Vec<u8> {
        packet(proto::path_command(op::FILE_SIZE, target).unwrap())
    }

    /// The catalog in memory, for the core's `choose`.
    #[derive(Default)]
    struct Store(Catalog);

    impl ArchiveStore for Store {
        fn load(&mut self) -> Result<Catalog> {
            Ok(self.0.clone())
        }

        fn save(&mut self, catalog: &Catalog) -> Result<()> {
            self.0 = catalog.clone();
            Ok(())
        }

        fn keep(&mut self, _: &[u8]) -> Result<ContentId> {
            unreachable!("choosing keeps no copy")
        }

        fn read(&mut self, _: &ContentId) -> Result<Option<Vec<u8>>> {
            unreachable!("choosing reads no copy")
        }

        fn discard(&mut self, _: &ContentId) -> Result<()> {
            unreachable!("choosing discards no copy")
        }
    }

    /// The shutdown of `link` with `standby` recorded for the 8.8" in the
    /// catalog, where the core reads it.
    fn at_shutdown_with(link: &mut dyn ScreenLink, standby: Standby) -> Result<Applied> {
        let key = ScreenKey::new(ModelId("turing-8.8"));
        let mut store = Store::default();
        store.0.screen_mut(&key).standby = standby;
        standby::at_shutdown(link, &mut store, &key)
    }

    #[test]
    fn standby_plan_b_writes_options_whole() {
        let (mut s, _) = live_screen(&NoPause, true);
        let key = ScreenKey::new(ModelId("turing-8.8"));
        let mut store = Store::default();
        let five = SleepMinutes::new(5).unwrap();
        // Each choice: the queries that check it can be honoured, then one
        // OPTIONS with its five fields (brightness, mode, 0, flip, timer).
        let cases = [
            (
                Standby::Off(five),
                vec![options(proto::StartMode::Default, 5)],
                [64, 0, 0, 0, 5],
            ),
            (
                Standby::Video(path("sd/video/loop.mp4")),
                vec![size_query(LOOP), options(proto::StartMode::Video, 0)],
                [64, 2, 0, 0, 0],
            ),
            (
                Standby::Album,
                vec![
                    packet(proto::storage_info()),
                    options(proto::StartMode::Image, 0),
                ],
                [64, 1, 0, 0, 0],
            ),
            // keep undoes: the boot media's mode (none recorded), no timer.
            (
                Standby::Keep,
                vec![options(proto::StartMode::Default, 0)],
                [64, 0, 0, 0, 0],
            ),
        ];
        for (choice, packets, stored) in cases {
            let from = s.wire().sent.len();
            standby::choose(&mut s, &mut store, &key, choice.clone(), None, Confirm::Yes).unwrap();
            assert_eq!(writes(&s, from), packets, "{choice:?}");
            assert_eq!(s.wire().options, Some(stored), "{choice:?}");
            let recorded = store.0.screen(&key).map(|r| r.standby.clone());
            assert_eq!(recorded, Some(choice));
        }

        // From keep to keep, and without the user's yes: nothing at all.
        let from = s.wire().sent.len();
        standby::choose(&mut s, &mut store, &key, Standby::Keep, None, Confirm::Yes).unwrap();
        let refused = standby::choose(&mut s, &mut store, &key, Standby::Album, None, Confirm::No);
        assert!(
            matches!(refused, Err(BezelError::NotConfirmed(_))),
            "{refused:?}"
        );
        assert!(writes(&s, from).is_empty());

        // off keeps the recorded boot media's start mode next to its timer
        // (D-2026-10-03-power-off-standby-2 (4)).
        store.0.screen_mut(&key).boot = Some(path("internal/video/intro.mp4"));
        let from = s.wire().sent.len();
        let ten = Standby::Off(SleepMinutes::MAX);
        let plan = standby::choose(&mut s, &mut store, &key, ten, None, Confirm::Yes).unwrap();
        assert_eq!(plan, PlanB::new(StartMode::Video, 10));
        assert_eq!(writes(&s, from), [options(proto::StartMode::Video, 10)]);
        assert!(s.wire().commands.iter().all(|o| !NEVER.contains(o)));
    }

    #[test]
    fn standby_off_is_turnoff_alone_without_waiting() {
        let (mut s, _) = live_screen(&NoPause, true);
        let from = s.wire().sent.len();
        let reads = s.wire().reads.len();
        let off = Standby::Off(SleepMinutes::SUGGESTED);
        assert_eq!(at_shutdown_with(&mut s, off), Ok(Applied::TurnedOff));
        assert_eq!(writes(&s, from), [packet(proto::simple(op::TURN_OFF))]);
        assert_eq!(
            s.wire().reads.len(),
            reads,
            "nothing read: no wait for the SoC to leave"
        );
        assert!(s.wire().off);
    }

    #[test]
    fn standby_video_loops_the_chosen_file_and_nothing_follows() {
        for (choice, target) in [
            ("sd/video/loop.mp4", LOOP),
            ("internal/video/intro.mp4", INTRO),
        ] {
            let (mut s, _) = live_screen(&NoPause, true);
            let from = s.wire().sent.len();
            let video = Standby::Video(path(choice));
            let applied = at_shutdown_with(&mut s, video).unwrap();
            assert_eq!(applied, Applied::Video(path(choice)));
            assert_eq!(
                writes(&s, from),
                [
                    size_query(target),
                    packet(proto::simple(op::STOP_VIDEO)),
                    packet(proto::simple(op::STOP_MEDIA)),
                    packet(proto::play_video(target, Repeat::Loop).unwrap()),
                ],
                "{choice}"
            );
            let fw = s.wire();
            assert_eq!(fw.playing, Some((target.to_string(), true)), "looping");
            // PLAY_VIDEO is the last command: no 0x87 that would stop the
            // video once the computer is off.
            assert_eq!(fw.commands.last(), Some(&op::PLAY_VIDEO));
            assert!(fw.commands.iter().all(|o| !NEVER.contains(o)));
        }
    }

    #[test]
    fn standby_album_writes_start_mode_1_then_restarts() {
        let (mut s, _) = live_screen(&NoPause, true);
        let from = s.wire().sent.len();
        assert_eq!(at_shutdown_with(&mut s, Standby::Album), Ok(Applied::Album));
        assert_eq!(
            writes(&s, from),
            [
                packet(proto::storage_info()),
                options(proto::StartMode::Image, 0),
                packet(proto::simple(op::RESTART)),
            ]
        );
        let fw = s.wire();
        assert_eq!(fw.options, Some([64, 1, 0, 0, 0]), "OPTIONS whole");
        assert!(fw.restarted);
        let all = fw.sent.len();
        assert!(
            fw.reads.iter().all(|(written, _)| *written < all),
            "nothing waited for after RESTART"
        );
        assert!(fw.commands.iter().all(|o| !NEVER.contains(o)));
    }

    /// Review W4 (iteration 1): a level stored with the plan B
    /// (`--brightness 40`) is set again before the album's OPTIONS, which
    /// then carries it (102 of 255), not the live link's 25 % (64).
    #[test]
    fn standby_album_restarts_at_the_level_stored_with_the_plan_b() {
        let (mut s, _) = live_screen(&NoPause, true);
        let key = ScreenKey::new(ModelId("turing-8.8"));
        let mut store = Store::default();
        let record = store.0.screen_mut(&key);
        record.standby = Standby::Album;
        record.stored = Some(StoredPlanB {
            plan: Plan::new(StartMode::Image, 0),
            brightness: Brightness::new(40),
        });
        let from = s.wire().sent.len();
        assert_eq!(
            standby::at_shutdown(&mut s, &mut store, &key),
            Ok(Applied::Album)
        );
        let at_forty = packet(proto::set_options(Options {
            brightness: 102,
            start_mode: proto::StartMode::Image,
            flip: false,
            sleep_minutes: 0,
        }));
        assert_eq!(
            writes(&s, from),
            [
                packet(proto::storage_info()),
                packet(proto::set_brightness(102)),
                at_forty,
                packet(proto::simple(op::RESTART)),
            ]
        );
        let fw = s.wire();
        assert_eq!(fw.options, Some([102, 1, 0, 0, 0]));
        assert!(fw.restarted);
        assert!(fw.commands.iter().all(|o| !NEVER.contains(o)));
    }

    #[test]
    fn standby_impossible_choices_turn_the_screen_off() {
        // The chosen video is gone: its size query, then TURNOFF.
        let (mut s, _) = live_screen(&NoPause, true);
        let from = s.wire().sent.len();
        let gone = Standby::Video(path("sd/video/gone.mp4"));
        assert_eq!(
            at_shutdown_with(&mut s, gone),
            Ok(Applied::TurnedOffInstead(Unavailable::NoVideo))
        );
        assert_eq!(
            writes(&s, from),
            [
                size_query("/mnt/SDCARD/video/gone.mp4"),
                packet(proto::simple(op::TURN_OFF)),
            ]
        );
        assert_eq!((s.wire().playing.as_ref(), s.wire().off), (None, true));

        // The album without a card: the storage info, then TURNOFF; no
        // OPTIONS and no RESTART.
        let (mut s, _) = live_screen(&NoPause, false);
        let from = s.wire().sent.len();
        assert_eq!(
            at_shutdown_with(&mut s, Standby::Album),
            Ok(Applied::TurnedOffInstead(Unavailable::NoCard))
        );
        assert_eq!(
            writes(&s, from),
            [
                packet(proto::storage_info()),
                packet(proto::simple(op::TURN_OFF)),
            ]
        );
        let fw = s.wire();
        assert_eq!((fw.options, fw.restarted, fw.off), (None, false, true));

        // keep: nothing at all.
        let from = s.wire().sent.len();
        assert_eq!(
            at_shutdown_with(&mut s, Standby::Keep),
            Ok(Applied::Nothing)
        );
        assert!(writes(&s, from).is_empty());
    }

    #[test]
    fn standby_keepalive_after_30_s_without_traffic() {
        let (mut s, hands) = live_screen(&NoPause, false);
        let frame = Frame::filled(m88().panel, Rgba::opaque(10, 20, 30));
        let shown = s.wire().screen.clone();
        assert_eq!(&shown[..4], &[30, 20, 10, 255], "native BGRA");
        let just_short = KEEPALIVE_AFTER - Duration::from_millis(1);

        // Before 30 s without traffic an unchanged frame sends nothing.
        let from = s.wire().sent.len();
        hands.advance(just_short);
        s.present(&frame).unwrap();
        assert!(writes(&s, from).is_empty());

        // At 30 s: one single-pixel run, pixel 0 as it is, then QUERY_STATUS.
        hands.advance(Duration::from_millis(1));
        s.present(&frame).unwrap();
        let list = [0x80, 0, 0, 30, 20, 10, 255, 0xEF, 0x69];
        assert_eq!(
            writes(&s, from),
            [
                packet(proto::partial_header(9, 0)),
                proto::blocks(&list),
                packet(proto::simple(op::QUERY_STATUS)),
            ]
        );
        assert_eq!(s.wire().screen, shown, "the image did not change");

        // The keepalive is traffic: 30 s more before the next one.
        let from = s.wire().sent.len();
        hands.advance(just_short);
        s.present(&frame).unwrap();
        assert!(writes(&s, from).is_empty());

        // So is a frame that changes.
        let mut next = frame.clone();
        next.fill_rect(Rect::new(0, 0, 2, 1), Rgba::WHITE);
        s.present(&next).unwrap();
        hands.advance(just_short);
        let from = s.wire().sent.len();
        s.present(&next).unwrap();
        assert!(writes(&s, from).is_empty());
        hands.advance(Duration::from_millis(1));
        s.present(&next).unwrap();
        let sent = writes(&s, from);
        assert_eq!(
            sent[0],
            packet(proto::partial_header(9, 2)),
            "third partial"
        );
        assert_eq!(commands(sent), [op::UPDATE_BITMAP, op::QUERY_STATUS]);
        assert_eq!(
            Some(&s.wire().screen),
            s.last.as_ref(),
            "the screen shows the last frame sent"
        );
    }

    #[test]
    fn standby_a_play_waits_for_one_stop_media_at_most() {
        // A firmware that never answers STOP_MEDIA (spec § 7.2): the play at
        // shutdown still leaves after one poll, not the 20 of a theme start
        // (about 28 s), well within the shutdown's deadline.
        let pauses = Pauses::default();
        let (mut s, _) = live_screen(&pauses, true);
        s.wire.silent_stop = true;
        pauses.take();
        let from = s.wire().sent.len();
        let reads = s.wire().reads.len();
        let video = path("sd/video/loop.mp4");
        assert_eq!(
            at_shutdown_with(&mut s, Standby::Video(video.clone())),
            Ok(Applied::Video(video))
        );
        assert_eq!(
            commands(writes(&s, from)),
            [
                op::FILE_SIZE,
                op::STOP_VIDEO,
                op::STOP_MEDIA,
                op::PLAY_VIDEO
            ]
        );
        let paused = pauses.take();
        assert_eq!(paused, [STOP_VIDEO_SETTLE]);
        // The reads between the size's answer and PLAY_VIDEO: one poll.
        let fw = s.wire();
        let (size_sent, play_sent) = (from + 1, fw.sent.len());
        let waited: Vec<Duration> = fw.reads[reads..]
            .iter()
            .filter(|(written, _)| (size_sent + 1..play_sent).contains(written))
            .map(|(_, timeout)| *timeout)
            .collect();
        assert_eq!(waited, [REPLY_TIMEOUT]);
        let worst: Duration = paused.iter().chain(&waited).sum();
        assert!(worst < Duration::from_secs(2), "{worst:?}");
        assert_eq!(fw.playing, Some((LOOP.to_string(), true)));
    }
}
