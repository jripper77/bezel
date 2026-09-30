//! Turing/TURZX USB wire format (VID 0x1CBE, the vendor app's "207" family).
//!
//! Spec: `docs/reverse-engineering/protocol-turing-usb.md`. Every command is a
//! 500-byte plaintext header `[cmd] 00 1A 6D [LE32 timestamp] [args…]`,
//! DES-CBC encrypted with key = IV = `slv3tuzx` after PKCS#7 padding (as the
//! vendor app does, D-2026-09-30-device-protocols-4), and sent as a 512-byte
//! packet `ciphertext[0..504] ‖ 00×6 ‖ A1 1A`. A bulk payload (PNG, JPEG, file
//! data, H.264) follows unencrypted in the same USB write. Replies are 512
//! bytes and are not encrypted. Multi-byte header arguments are big-endian;
//! multi-byte reply fields are little-endian.
//!
//! This module is pure: it builds and parses bytes and encodes frames.
//!
//! Storage (spec § 6) ships with golden vectors only (no capture of a real
//! device, `hardware_validated = false`, D-2026-09-30-storage-video-1): the
//! driver uses STORAGE_INFO, LIST_DIR, OPEN_FILE, WRITE_CHUNK, PLAY_VIDEO and
//! SHOW_IMAGE, never [`op::WRITE_FILE`] (40) nor [`op::FILE_SIZE`] (98), whose
//! meanings the references dispute (spec § 3).

use std::fmt;

use bezel_core::domain::storage::{self, Capacity};
use cbc::cipher::{Block, BlockModeEncrypt, KeyIvInit};

/// DES-CBC encryptor of the command headers.
type DesCbc = cbc::Encryptor<des::Des>;

/// Plaintext header length.
pub const HEADER_LEN: usize = 500;
/// Ciphertext length: the header padded to a multiple of the DES block (8).
pub const CIPHERTEXT_LEN: usize = 504;
/// Size of every command packet, and of every reply.
pub const PACKET_LEN: usize = 512;
/// Offset of the first argument byte in the header.
pub const ARGS: usize = 8;
/// Largest argument block a header can carry.
pub const MAX_ARGS: usize = HEADER_LEN - ARGS;
/// Bytes 2 and 3 of every header.
pub const MAGIC: [u8; 2] = [0x1A, 0x6D];
/// The last two bytes of every packet.
pub const TRAILER: [u8; 2] = [0xA1, 0x1A];
/// DES key and IV: ASCII `slv3tuzx`.
pub const KEY: [u8; 8] = *b"slv3tuzx";
/// Status byte of a successful reply (at `[8]`, or `[1]` for some commands).
pub const STATUS_OK: u8 = 0xC8;
/// Largest image (and file chunk) the device accepts in one write: 1 MiB.
pub const MAX_IMAGE: usize = 1024 * 1024;
/// Highest brightness level on the wire.
pub const MAX_BRIGHTNESS: u8 = 102;
/// H.264 chunk size used when the device does not report one.
pub const DEFAULT_CHUNK: u32 = 202_752;
/// Bulk OUT endpoint (vendor app).
pub const EP_OUT: u8 = 0x01;
/// IN endpoint (vendor app).
pub const EP_IN: u8 = 0x81;
/// Offset of the status byte in most replies (spec § 2).
pub const STATUS_AT: usize = 8;
/// Offset of the status byte in a [`op::SHOW_IMAGE`] reply (spec § 2).
pub const IMAGE_STATUS_AT: usize = 1;
/// Bytes per KiB, the unit of a STORAGE_INFO reply.
const KIB: u64 = 1024;

/// Storage roots on the device (spec § 6): `<root>img/` and `<root>video/`.
pub mod root {
    /// Internal storage.
    pub const INTERNAL: &str = "/usr/data/";
    /// The TF card.
    pub const CARD: &str = "/tmp/sdcard/mmcblk0p1/";
}

/// The boot logo the vendor app writes (spec § 6). Golden only: it is
/// written with [`super::op::WRITE_FILE`], which Bezel never sends
/// (D-2026-09-30-storage-video-5).
pub mod boot_logo {
    /// Where the firmware reads it.
    pub const PATH: &str = "/usr/data/boot.jpg";
    /// Largest file the vendor app writes there.
    pub const MAX_BYTES: usize = 307_200;
    /// JPEG quality the vendor app encodes it with (at the panel size).
    pub const QUALITY: u8 = 95;
    /// The first two bytes of every JPEG (SOI marker).
    pub const JPEG_SOI: [u8; 2] = [0xFF, 0xD8];
}

/// Command ids (vendor-app meanings, which differ from the Python labels for
/// 40, 98 and 13; see the spec's section on the vendor app).
pub mod op {
    /// Sync / version query; the reply echoes 10 at `[0]` and carries the
    /// version string at `[8..40]`.
    pub const SYNC: u8 = 10;
    /// Reboot the device (disruptive; never sent implicitly).
    pub const RESTART: u8 = 11;
    /// Device-side rotation, `[8]` = 0..3 (persistent; never sent implicitly).
    pub const SET_ROTATION: u8 = 13;
    /// Backlight, `[8]` = 0..=102.
    pub const SET_BRIGHTNESS: u8 = 14;
    /// Frame rate of a streamed video, `[8]` = fps.
    pub const SET_FRAME_RATE: u8 = 15;
    /// H.264 chunk size query; the reply carries a BE32 at `[8..12]`.
    pub const GET_CHUNK_SIZE: u8 = 17;
    /// Create or open a remote file for a chunked upload (path args).
    pub const OPEN_FILE: u8 = 38;
    /// One chunk of a chunked upload.
    pub const WRITE_CHUNK: u8 = 39;
    /// Write a small file (< 100 KiB) in one shot (vendor app; the Python
    /// reference calls it "delete"). Never sent until a capture settles it.
    pub const WRITE_FILE: u8 = 40;
    /// Sent by the vendor app when a theme starts, `[8]` = 0 (meaning unknown).
    pub const THEME_START: u8 = 41;
    /// Delete a remote file (path args; destructive).
    pub const DELETE_FILE: u8 = 42;
    /// Size of a remote file (path args); LE32 at `[8..12]` (vendor app; the
    /// Python reference calls it "play file"). Never sent until a capture
    /// settles it.
    pub const FILE_SIZE: u8 = 98;
    /// List a remote directory (path args).
    pub const LIST_DIR: u8 = 99;
    /// Storage totals: six LE32 KiB values from `[8]`.
    pub const STORAGE_INFO: u8 = 100;
    /// Show a JPEG, `[8..12]` = BE32 length, the JPEG follows.
    pub const SHOW_JPEG: u8 = 101;
    /// Show a PNG, `[8..12]` = BE32 length, the PNG follows.
    pub const SHOW_PNG: u8 = 102;
    /// Play a stored video, looping (path args).
    pub const PLAY_VIDEO: u8 = 110;
    /// Stop device-side playback.
    pub const STOP_PLAYBACK: u8 = 111;
    /// Is device-side playback busy? (`[8]` != 0).
    pub const PLAYBACK_BUSY: u8 = 112;
    /// Show a stored image (path args; its reply status is at `[1]`).
    pub const SHOW_IMAGE: u8 = 113;
    /// One chunk of a streamed H.264 video.
    pub const STREAM_CHUNK: u8 = 121;
    /// Decoder queue depth of the stream (`[8]`).
    pub const STREAM_STATUS: u8 = 122;
    /// Stop the stream; the vendor app sends it whenever a theme loop ends.
    pub const STOP_STREAM: u8 = 123;
    /// Persistent settings (never sent implicitly).
    pub const SAVE_SETTINGS: u8 = 125;
}

/// A 500-byte plaintext command header.
pub type Header = [u8; HEADER_LEN];

/// Builds a header: `[cmd] 00 1A 6D [LE32 timestamp] [args] 00…`.
/// `timestamp` is milliseconds since local midnight (the device does not
/// appear to check it). `None` when `args` exceeds [`MAX_ARGS`].
pub fn header(cmd: u8, timestamp: u32, args: &[u8]) -> Option<Header> {
    if args.len() > MAX_ARGS {
        return None;
    }
    let mut h = [0u8; HEADER_LEN];
    h[0] = cmd;
    h[2..4].copy_from_slice(&MAGIC);
    h[4..8].copy_from_slice(&timestamp.to_le_bytes());
    h[ARGS..ARGS + args.len()].copy_from_slice(args);
    Some(h)
}

/// A header whose short, constant-size arguments always fit.
fn fixed(cmd: u8, timestamp: u32, args: &[u8]) -> Header {
    header(cmd, timestamp, args).unwrap_or([0; HEADER_LEN])
}

/// An argument-less command.
pub fn simple(cmd: u8, timestamp: u32) -> Header {
    fixed(cmd, timestamp, &[])
}

/// Sync (the handshake).
pub fn sync(timestamp: u32) -> Header {
    simple(op::SYNC, timestamp)
}

/// Reboot (disruptive; only on an explicit request).
pub fn restart(timestamp: u32) -> Header {
    simple(op::RESTART, timestamp)
}

/// Backlight level, clamped to 0..=102.
pub fn set_brightness(timestamp: u32, level: u8) -> Header {
    fixed(op::SET_BRIGHTNESS, timestamp, &[level.min(MAX_BRIGHTNESS)])
}

/// The wire level for a percentage, as the Python reference maps it:
/// `int(percent / 100 * 102)`.
pub fn brightness_level(percent: u8) -> u8 {
    (u16::from(percent.min(100)) * u16::from(MAX_BRIGHTNESS) / 100) as u8
}

/// Device-side rotation in quarter turns (0..=3), as the vendor app sends it
/// after a configuration change (persistent; only on an explicit request).
pub fn set_rotation(timestamp: u32, quarter_turns: u8) -> Header {
    fixed(op::SET_ROTATION, timestamp, &[quarter_turns & 3])
}

/// Frame rate of a streamed video.
pub fn set_frame_rate(timestamp: u32, fps: u8) -> Header {
    fixed(op::SET_FRAME_RATE, timestamp, &[fps])
}

/// Header of a frame: [`op::SHOW_PNG`] or [`op::SHOW_JPEG`] with the BE32 image length.
pub fn show_image_data(timestamp: u32, opcode: u8, len: u32) -> Header {
    fixed(opcode, timestamp, &len.to_be_bytes())
}

/// Arguments naming a remote path: `[BE32 len] 00 00 00 00 [ASCII path]`.
/// `None` for a non-ASCII path or one that does not fit a header.
pub fn path_args(path: &str) -> Option<Vec<u8>> {
    if !path.is_ascii() || path.len() > MAX_ARGS - 8 {
        return None;
    }
    let mut args = Vec::with_capacity(8 + path.len());
    args.extend_from_slice(&(path.len() as u32).to_be_bytes());
    args.extend_from_slice(&[0; 4]);
    args.extend_from_slice(path.as_bytes());
    Some(args)
}

/// A command whose only argument is a remote path ([`op::OPEN_FILE`],
/// [`op::DELETE_FILE`], [`op::FILE_SIZE`], [`op::LIST_DIR`],
/// [`op::PLAY_VIDEO`], [`op::SHOW_IMAGE`]).
pub fn path_command(cmd: u8, timestamp: u32, path: &str) -> Option<Header> {
    header(cmd, timestamp, &path_args(path)?)
}

/// One chunk of a chunked upload: `[8..12]` = BE32 buffer capacity (1 MiB),
/// `[12..16]` = BE32 bytes in this chunk, `[16]` = 1 on the last chunk.
/// The vendor app always sends [`MAX_IMAGE`] payload bytes after it.
pub fn write_chunk(timestamp: u32, chunk_len: u32, last: bool) -> Header {
    let mut args = [0u8; 9];
    args[..4].copy_from_slice(&(MAX_IMAGE as u32).to_be_bytes());
    args[4..8].copy_from_slice(&chunk_len.to_be_bytes());
    args[8] = u8::from(last);
    fixed(op::WRITE_CHUNK, timestamp, &args)
}

/// A small file written in one shot: `[8..12]` = BE32 path length,
/// `[12..16]` = BE32 data length, `[16..]` = ASCII path; the data follows.
pub fn write_file(timestamp: u32, path: &str, data_len: u32) -> Option<Header> {
    let mut args = path_args(path)?;
    args[4..8].copy_from_slice(&data_len.to_be_bytes());
    header(op::WRITE_FILE, timestamp, &args)
}

/// The header the vendor app writes the boot logo with: [`write_file`] to
/// [`boot_logo::PATH`], `jpeg` following. `None` unless `jpeg` starts with
/// the JPEG SOI marker and has at most [`boot_logo::MAX_BYTES`] bytes.
/// Golden only: no driver sends it (D-2026-09-30-storage-video-5).
pub fn boot_logo_header(timestamp: u32, jpeg: &[u8]) -> Option<Header> {
    if !jpeg.starts_with(&boot_logo::JPEG_SOI) || jpeg.len() > boot_logo::MAX_BYTES {
        return None;
    }
    write_file(timestamp, boot_logo::PATH, u32::try_from(jpeg.len()).ok()?)
}

/// One chunk of a streamed H.264 video: `[8..12]` = BE32 length, `[12]` = 1
/// on the chunk that reaches the end of the file.
pub fn stream_chunk(timestamp: u32, len: u32, last: bool) -> Header {
    let mut args = [0u8; 5];
    args[..4].copy_from_slice(&len.to_be_bytes());
    args[4] = u8::from(last);
    fixed(op::STREAM_CHUNK, timestamp, &args)
}

/// The persistent settings of [`op::SAVE_SETTINGS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// Backlight, raw 0..=255 (not the 0..=102 wire scale of [`op::SET_BRIGHTNESS`]).
    pub brightness: u8,
    /// What the screen shows by itself at boot.
    pub start_mode: u8,
    /// Device-side rotation, 0..=3.
    pub rotation: u8,
    /// Sleep delay.
    pub sleep: u8,
    /// Offline mode.
    pub offline_mode: u8,
}

/// SAVE_SETTINGS: `[8]` brightness, `[9]` start mode, `[10]` 0, `[11]` rotation,
/// `[12]` sleep, `[13]` offline mode (persistent; only on an explicit request).
pub fn save_settings(timestamp: u32, s: Settings) -> Header {
    fixed(
        op::SAVE_SETTINGS,
        timestamp,
        &[
            s.brightness,
            s.start_mode,
            0,
            s.rotation & 3,
            s.sleep,
            s.offline_mode,
        ],
    )
}

/// How the 500-byte header is padded to 504 bytes before encryption.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Padding {
    /// `04 04 04 04`, the .NET default the vendor app uses (what Bezel sends).
    Pkcs7,
    /// `00 00 00 00`, the Python reference's form. Only the last ciphertext
    /// block differs, so the firmware ignores plaintext bytes 496..504.
    Zero,
}

/// DES-CBC encrypts a header after `padding`.
pub fn encrypt(header: &Header, padding: Padding) -> [u8; CIPHERTEXT_LEN] {
    let mut buf = [0u8; CIPHERTEXT_LEN];
    buf[..HEADER_LEN].copy_from_slice(header);
    if padding == Padding::Pkcs7 {
        let n = (CIPHERTEXT_LEN - HEADER_LEN) as u8;
        buf[HEADER_LEN..].fill(n);
    }
    let (blocks, _) = Block::<DesCbc>::slice_as_chunks_mut(&mut buf);
    DesCbc::new(&KEY.into(), &KEY.into()).encrypt_blocks(blocks);
    buf
}

/// The 512-byte packet of a header, with the given padding.
pub fn packet_with(header: &Header, padding: Padding) -> [u8; PACKET_LEN] {
    let mut packet = [0u8; PACKET_LEN];
    packet[..CIPHERTEXT_LEN].copy_from_slice(&encrypt(header, padding));
    packet[PACKET_LEN - 2..].copy_from_slice(&TRAILER);
    packet
}

/// The 512-byte packet the vendor app sends for a header (PKCS#7).
pub fn packet(header: &Header) -> [u8; PACKET_LEN] {
    packet_with(header, Padding::Pkcs7)
}

/// One USB write: the packet of `header` followed by the unencrypted `payload`.
pub fn packet_and_payload(header: &Header, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(PACKET_LEN + payload.len());
    out.extend_from_slice(&packet(header));
    out.extend_from_slice(payload);
    out
}

/// The Python reference's success test: `[1]` or `[8]` is 0xC8.
pub fn resp_ok(reply: &[u8]) -> bool {
    reply.get(1) == Some(&STATUS_OK) || reply.get(8) == Some(&STATUS_OK)
}

/// The vendor app's success test for `cmd`: 0xC8 at [`STATUS_AT`], or at
/// [`IMAGE_STATUS_AT`] for [`op::SHOW_IMAGE`] (spec § 2). Used for
/// [`op::OPEN_FILE`], [`op::PLAY_VIDEO`] and [`op::SHOW_IMAGE`].
pub fn accepted(cmd: u8, reply: &[u8]) -> bool {
    let at = if cmd == op::SHOW_IMAGE {
        IMAGE_STATUS_AT
    } else {
        STATUS_AT
    };
    reply.get(at) == Some(&STATUS_OK)
}

/// True when a PLAYBACK_BUSY reply says nothing plays any more (`[8]` = 0,
/// spec § 6). A missing reply is not idle.
pub fn playback_idle(reply: &[u8]) -> bool {
    reply.get(STATUS_AT) == Some(&0)
}

/// The version string of a SYNC reply: `None` unless `[0]` echoes 10;
/// `[8..40]` as UTF-8 with the NUL padding removed (may be empty).
pub fn sync_version(reply: &[u8]) -> Option<String> {
    if reply.first() != Some(&op::SYNC) {
        return None;
    }
    let field = reply.get(ARGS..reply.len().min(ARGS + 32)).unwrap_or(&[]);
    let text = String::from_utf8_lossy(field);
    Some(text.trim_end_matches('\0').trim().to_string())
}

fn le32(reply: &[u8], at: usize) -> Option<u32> {
    let bytes = reply.get(at..at + 4)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// A STORAGE_INFO reply: six LE32 values in KiB (vendor-app layout; the
/// Python reference reads only the first three).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageInfo {
    /// TF card size; 0 when no card is inserted.
    pub card_total_kib: u32,
    /// TF card used.
    pub card_used_kib: u32,
    /// TF card free.
    pub card_free_kib: u32,
    /// Internal storage size.
    pub internal_total_kib: u32,
    /// Internal storage used.
    pub internal_used_kib: u32,
    /// Internal storage free.
    pub internal_free_kib: u32,
}

impl StorageInfo {
    /// Parses `[8..32]`; `None` when the reply is shorter.
    pub fn parse(reply: &[u8]) -> Option<StorageInfo> {
        Some(StorageInfo {
            card_total_kib: le32(reply, 8)?,
            card_used_kib: le32(reply, 12)?,
            card_free_kib: le32(reply, 16)?,
            internal_total_kib: le32(reply, 20)?,
            internal_used_kib: le32(reply, 24)?,
            internal_free_kib: le32(reply, 28)?,
        })
    }

    /// Parses a STORAGE_INFO reply: `None` unless `[0]` echoes
    /// [`op::STORAGE_INFO`] (spec § 2) and the six fields are there.
    pub fn from_reply(reply: &[u8]) -> Option<StorageInfo> {
        if reply.first() != Some(&op::STORAGE_INFO) {
            return None;
        }
        Self::parse(reply)
    }

    /// True when a TF card is present.
    pub fn has_card(&self) -> bool {
        self.card_total_kib != 0
    }

    /// The capacities in bytes. No vendor reserve is known for this family;
    /// the card exists when its total is not 0 (spec § 6).
    pub fn info(&self) -> storage::StorageInfo {
        let capacity = |total: u32, used: u32, free: u32| Capacity {
            total: u64::from(total) * KIB,
            used: u64::from(used) * KIB,
            free: u64::from(free) * KIB,
        };
        let internal = capacity(
            self.internal_total_kib,
            self.internal_used_kib,
            self.internal_free_kib,
        );
        let card = self
            .has_card()
            .then(|| capacity(self.card_total_kib, self.card_used_kib, self.card_free_kib));
        storage::StorageInfo { internal, card }
    }
}

/// The H.264 chunk size of a GET_CHUNK_SIZE reply: BE32 `[8..12]` when it is
/// within 1..=1 MiB, else [`DEFAULT_CHUNK`].
pub fn chunk_size(reply: &[u8]) -> u32 {
    reply
        .get(8..12)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        .filter(|n| (1..=MAX_IMAGE as u32).contains(n))
        .unwrap_or(DEFAULT_CHUNK)
}

/// The size in bytes of a FILE_SIZE reply (LE32 `[8..12]`).
pub fn file_size(reply: &[u8]) -> Option<u32> {
    le32(reply, 8)
}

/// The decoder queue depth of a STREAM_CHUNK or STREAM_STATUS reply (`[8]`).
pub fn queue_depth(reply: &[u8]) -> Option<u8> {
    reply.get(8).copied()
}

/// An encoded frame, ready to follow its header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Image {
    /// PNG with alpha (sent with [`op::SHOW_PNG`]).
    Png(Vec<u8>),
    /// JPEG, alpha dropped (sent with [`op::SHOW_JPEG`]).
    Jpeg(Vec<u8>),
}

impl Image {
    /// The command that carries it.
    pub fn opcode(&self) -> u8 {
        match self {
            Image::Png(_) => op::SHOW_PNG,
            Image::Jpeg(_) => op::SHOW_JPEG,
        }
    }

    /// The encoded bytes.
    pub fn bytes(&self) -> &[u8] {
        match self {
            Image::Png(b) | Image::Jpeg(b) => b,
        }
    }
}

/// Why a frame could not be encoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    /// The pixel buffer does not match `width x height` RGBA, or the size is
    /// out of the encoder's range.
    Size,
    /// The PNG encoder failed.
    Png(String),
    /// The JPEG encoder failed.
    Jpeg(String),
    /// Even the smallest JPEG exceeds the device limit.
    TooLarge {
        /// Smallest encoding produced.
        smallest: usize,
        /// The limit.
        limit: usize,
    },
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EncodeError::Size => write!(f, "frame buffer does not match its size"),
            EncodeError::Png(e) => write!(f, "PNG encoding failed: {e}"),
            EncodeError::Jpeg(e) => write!(f, "JPEG encoding failed: {e}"),
            EncodeError::TooLarge { smallest, limit } => {
                write!(
                    f,
                    "frame does not fit {limit} bytes (smallest JPEG {smallest})"
                )
            }
        }
    }
}

impl std::error::Error for EncodeError {}

fn check_size(rgba: &[u8], width: u32, height: u32) -> Result<(), EncodeError> {
    let expected = u64::from(width) * u64::from(height) * 4;
    if width == 0 || height == 0 || rgba.len() as u64 != expected {
        return Err(EncodeError::Size);
    }
    Ok(())
}

/// Encodes RGBA8 as an RGBA PNG. The reference uses zlib level 9; Bezel uses
/// the png crate's fast deflate, since every frame is re-encoded.
pub fn encode_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, EncodeError> {
    check_size(rgba, width, height)?;
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let png_err = |e: png::EncodingError| EncodeError::Png(e.to_string());
    let mut writer = encoder.write_header().map_err(png_err)?;
    writer.write_image_data(rgba).map_err(png_err)?;
    writer.finish().map_err(png_err)?;
    Ok(out)
}

/// JPEG chroma subsampling (Pillow's `subsampling` 2, 1, 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subsampling {
    /// 4:2:0 (Pillow 2).
    S420,
    /// 4:2:2 (Pillow 1).
    S422,
    /// 4:4:4 (Pillow 0).
    S444,
}

impl Subsampling {
    fn factor(self) -> jpeg_encoder::SamplingFactor {
        match self {
            Subsampling::S420 => jpeg_encoder::SamplingFactor::R_4_2_0,
            Subsampling::S422 => jpeg_encoder::SamplingFactor::R_4_2_2,
            Subsampling::S444 => jpeg_encoder::SamplingFactor::R_4_4_4,
        }
    }
}

/// Quality the JPEG fallback starts at.
pub const JPEG_START_QUALITY: u8 = 90;

/// The Python reference's JPEG search order: subsampling 4:2:0, 4:2:2, 4:4:4;
/// within each, quality from 90 down by 5 while above 10, then by 1 to 1.
pub fn jpeg_attempts() -> impl Iterator<Item = (Subsampling, u8)> {
    [Subsampling::S420, Subsampling::S422, Subsampling::S444]
        .into_iter()
        .flat_map(|s| {
            std::iter::successors(Some(JPEG_START_QUALITY), |&q| match q {
                11.. => Some(q - 5),
                2..=10 => Some(q - 1),
                _ => None,
            })
            .map(move |q| (s, q))
        })
}

/// Encodes RGBA8 as a baseline JPEG (alpha dropped).
pub fn encode_jpeg(
    rgba: &[u8],
    width: u32,
    height: u32,
    quality: u8,
    subsampling: Subsampling,
) -> Result<Vec<u8>, EncodeError> {
    check_size(rgba, width, height)?;
    let w = u16::try_from(width).map_err(|_| EncodeError::Size)?;
    let h = u16::try_from(height).map_err(|_| EncodeError::Size)?;
    let mut out = Vec::new();
    let mut encoder = jpeg_encoder::Encoder::new(&mut out, quality.clamp(1, 100));
    encoder.set_sampling_factor(subsampling.factor());
    encoder
        .encode(rgba, w, h, jpeg_encoder::ColorType::Rgba)
        .map_err(|e| EncodeError::Jpeg(e.to_string()))?;
    Ok(out)
}

/// The first JPEG of [`jpeg_attempts`] that fits `limit` bytes.
pub fn encode_jpeg_under(
    rgba: &[u8],
    width: u32,
    height: u32,
    limit: usize,
) -> Result<Vec<u8>, EncodeError> {
    let mut smallest = usize::MAX;
    for (subsampling, quality) in jpeg_attempts() {
        let jpeg = encode_jpeg(rgba, width, height, quality, subsampling)?;
        if jpeg.len() <= limit {
            return Ok(jpeg);
        }
        smallest = smallest.min(jpeg.len());
    }
    Err(EncodeError::TooLarge { smallest, limit })
}

/// Encodes a native-orientation RGBA8 frame the way the Python reference
/// does: a PNG when it fits `limit` bytes, else the JPEG search.
pub fn encode_frame(
    rgba: &[u8],
    width: u32,
    height: u32,
    limit: usize,
) -> Result<Image, EncodeError> {
    let png = encode_png(rgba, width, height)?;
    if png.len() <= limit {
        return Ok(Image::Png(png));
    }
    encode_jpeg_under(rgba, width, height, limit).map(Image::Jpeg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbc::cipher::BlockModeDecrypt;

    /// The timestamp every reference vector uses (`[4..8]` = `04 03 02 01`).
    const TS: u32 = 0x0102_0304;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn decrypt(ciphertext: &[u8; CIPHERTEXT_LEN]) -> [u8; CIPHERTEXT_LEN] {
        let mut buf = *ciphertext;
        let (blocks, _) = Block::<DesCbc>::slice_as_chunks_mut(&mut buf);
        cbc::Decryptor::<des::Des>::new(&KEY.into(), &KEY.into()).decrypt_blocks(blocks);
        buf
    }

    #[test]
    fn packets_match_the_reference_vectors() {
        // docs: protocol-turing-usb.md § Test vectors (pycryptodome), and the
        // vendor-app analysis (scratchpad tv207.py) for the PKCS#7 tails.
        // (name, header, packet[0..16], PKCS#7 ct[488..504], zero-pad ct[488..504])
        let cases = [
            (
                "sync",
                sync(TS),
                "ccffa6fb53021074fd8ec712b392c6a6",
                "fc104abac4effccf781a5f0c81c05598",
                "fc104abac4effccfec5435a2839fc6a3",
            ),
            (
                "brightness 40",
                set_brightness(TS, 40),
                "d9284351e56cef7f51328753232e04f6",
                "8958dce7444dd6ee291216b3d7063cc9",
                "8958dce7444dd6eef5374cfb24396b48",
            ),
            (
                "png 3703",
                show_image_data(TS, op::SHOW_PNG, 0x0e77),
                "d508e75bad1759cba626c175906a610e",
                "f77fb80875b076be736b1583c879fae5",
                "f77fb80875b076be1fb072fdbc6b3d67",
            ),
        ];
        for (name, h, first16, pkcs7_tail, zero_tail) in cases {
            let vendor = packet(&h);
            let python = packet_with(&h, Padding::Zero);
            assert_eq!(hex(&vendor[..16]), first16, "{name}: first block");
            assert_eq!(hex(&vendor[488..504]), pkcs7_tail, "{name}: PKCS#7 tail");
            assert_eq!(hex(&python[488..504]), zero_tail, "{name}: zero-pad tail");
            // CBC: the two paddings share every ciphertext block but the last.
            assert_eq!(vendor[..496], python[..496], "{name}");
            let differing: Vec<usize> = (0..PACKET_LEN)
                .filter(|&i| vendor[i] != python[i])
                .collect();
            assert!(!differing.is_empty(), "{name}");
            assert!(
                differing.iter().all(|i| (496..504).contains(i)),
                "{name}: only the last block may differ, got {differing:?}"
            );
            // Both end with 6 zero bytes and A1 1A.
            for p in [&vendor, &python] {
                assert_eq!(hex(&p[504..]), "000000000000a11a", "{name}");
            }
        }
        // The Python reference's own tail of the sync packet (spec § 9).
        assert_eq!(
            hex(&packet_with(&sync(TS), Padding::Zero)[496..]),
            "ec5435a2839fc6a3000000000000a11a"
        );
    }

    #[test]
    fn ciphertext_decrypts_to_the_padded_header() {
        let h = show_image_data(TS, op::SHOW_JPEG, 123_456);
        let plain = decrypt(&encrypt(&h, Padding::Pkcs7));
        assert_eq!(plain[..HEADER_LEN], h[..]);
        assert_eq!(plain[HEADER_LEN..], [4, 4, 4, 4]);
        let plain = decrypt(&encrypt(&h, Padding::Zero));
        assert_eq!(plain[HEADER_LEN..], [0, 0, 0, 0]);
    }

    #[test]
    fn header_layout() {
        let h = sync(TS);
        assert_eq!(hex(&h[..8]), "0a001a6d04030201");
        assert!(h[8..].iter().all(|&b| b == 0));
        assert_eq!(set_brightness(0, 40)[8], 40);
        assert_eq!(set_brightness(0, 200)[8], MAX_BRIGHTNESS);
        // Sizes are big-endian: 3,703 = 0x00000e77.
        assert_eq!(
            hex(&show_image_data(0, op::SHOW_PNG, 0x0e77)[..12]),
            "66001a6d0000000000000e77"
        );
        assert!(header(1, 0, &[7; MAX_ARGS]).is_some());
        assert!(header(1, 0, &[7; MAX_ARGS + 1]).is_none());
        assert_eq!(restart(TS)[0], op::RESTART);
        assert_eq!(simple(op::STOP_STREAM, TS)[0], op::STOP_STREAM);
        assert_eq!(
            set_frame_rate(TS, 25)[..9],
            [15, 0, 0x1a, 0x6d, 4, 3, 2, 1, 25]
        );
        assert_eq!(set_rotation(TS, 6)[..9], [13, 0, 0x1a, 0x6d, 4, 3, 2, 1, 2]);
    }

    #[test]
    fn brightness_follows_the_python_mapping() {
        // int(level / 100 * 102)
        assert_eq!(brightness_level(0), 0);
        assert_eq!(brightness_level(1), 1);
        assert_eq!(brightness_level(25), 25);
        assert_eq!(brightness_level(50), 51);
        assert_eq!(brightness_level(99), 100);
        assert_eq!(brightness_level(100), 102);
        assert_eq!(brightness_level(200), 102);
    }

    #[test]
    fn storage_and_video_encoders() {
        let open = path_command(op::OPEN_FILE, TS, "/tmp/sdcard/mmcblk0p1/img/a.png").unwrap();
        assert_eq!(open[0], op::OPEN_FILE);
        assert_eq!(&open[8..16], &[0, 0, 0, 31, 0, 0, 0, 0]);
        assert_eq!(&open[16..47], b"/tmp/sdcard/mmcblk0p1/img/a.png");
        assert_eq!(open[47], 0);
        assert!(path_command(op::DELETE_FILE, TS, "/usr/data/é").is_none());
        assert!(path_command(op::FILE_SIZE, TS, &"a".repeat(MAX_ARGS - 8)).is_some());
        assert!(path_command(op::FILE_SIZE, TS, &"a".repeat(MAX_ARGS - 7)).is_none());

        let chunk = write_chunk(TS, 1000, true);
        assert_eq!(&chunk[8..17], &[0, 0x10, 0, 0, 0, 0, 0x03, 0xe8, 1]);
        assert_eq!(write_chunk(TS, 5, false)[16], 0);

        let small = write_file(TS, "/usr/data/boot.jpg", 0x0102).unwrap();
        assert_eq!(small[0], op::WRITE_FILE);
        assert_eq!(&small[8..16], &[0, 0, 0, 18, 0, 0, 1, 2]);
        assert_eq!(&small[16..34], b"/usr/data/boot.jpg");

        let s = stream_chunk(TS, 202_752, true);
        assert_eq!(
            &s[..13],
            &[121, 0, 0x1a, 0x6d, 4, 3, 2, 1, 0, 3, 0x18, 0, 1]
        );

        let settings = save_settings(
            TS,
            Settings {
                brightness: 170,
                start_mode: 1,
                rotation: 5,
                sleep: 3,
                offline_mode: 1,
            },
        );
        assert_eq!(
            &settings[..14],
            &[125, 0, 0x1a, 0x6d, 4, 3, 2, 1, 170, 1, 0, 1, 3, 1]
        );
    }

    #[test]
    fn storage_packets_match_the_reference_vectors() {
        // Spec § 2/§ 3 layouts, encrypted by OpenSSL's DES-CBC (legacy
        // provider) with PKCS#7 padding; the same script reproduces the
        // verified sync vector of § 9. Golden only: no capture of a real
        // screen exists (D-2026-09-30-storage-video-1).
        // (name, header, plaintext[0..24], packet[0..16], packet[488..504])
        let path = |cmd, p: &str| path_command(cmd, TS, p).unwrap();
        let cases = [
            (
                "storage info",
                simple(op::STORAGE_INFO, TS),
                "64001a6d0403020100000000000000000000000000000000",
                "0d0f4d5aff6928c70b292fa759ddb3f8",
                "9234820321198b12616c6e39017b22b3",
            ),
            (
                "list internal videos",
                path(op::LIST_DIR, "/usr/data/video/"),
                "63001a6d0403020100000010000000002f7573722f646174",
                "d9f21aab0609590355032d30fc356184",
                "7aa7d61e4bca88793935ed98a1c28577",
            ),
            (
                "list card images",
                path(op::LIST_DIR, "/tmp/sdcard/mmcblk0p1/img/"),
                "63001a6d040302010000001a000000002f746d702f736463",
                "d9f21aab06095903ef0cf8697a13ab1f",
                "2c4281f6713f3799840bde8bffba8feb",
            ),
            (
                "open file",
                path(op::OPEN_FILE, "/usr/data/video/clip.h264"),
                "26001a6d0403020100000019000000002f7573722f646174",
                "4f19aecbd9a36eb172dc78001ea0a87b",
                "9e76c6031d4ccb75ab4a87023b3eb1e6",
            ),
            (
                "write chunk, 1 MiB",
                write_chunk(TS, 1_048_576, false),
                "27001a6d0403020100100000001000000000000000000000",
                "fb3921145c3b8cdbf05ef21dd3b5ac8b",
                "aa67e7f03160fd769f0f7c47742d0ce1",
            ),
            (
                "write chunk, last 1000",
                write_chunk(TS, 1000, true),
                "27001a6d0403020100100000000003e80100000000000000",
                "fb3921145c3b8cdb38127b8d533024a0",
                "4ae915f0dbce62222aed0c8eb2021763",
            ),
            (
                "play video",
                path(op::PLAY_VIDEO, "/tmp/sdcard/mmcblk0p1/video/88.h264"),
                "6e001a6d0403020100000023000000002f746d702f736463",
                "13f01a509d49b56e73a80493c57fbae8",
                "8481056d80708510cc2104e431e05a68",
            ),
            (
                "show image",
                path(op::SHOW_IMAGE, "/usr/data/img/logo.png"),
                "71001a6d0403020100000016000000002f7573722f646174",
                "6bd72bf298ebe212975839801b3df0f5",
                "4de793bd6411543a4e91d79c2ea4c07e",
            ),
            (
                "stop playback",
                simple(op::STOP_PLAYBACK, TS),
                "6f001a6d0403020100000000000000000000000000000000",
                "e36104b5f63f6b8c67919b2d95f406ba",
                "86c6a110cb0123c13b05e074e53008e8",
            ),
            (
                "playback busy",
                simple(op::PLAYBACK_BUSY, TS),
                "70001a6d0403020100000000000000000000000000000000",
                "635d93e727c1f6547590dcc63704b147",
                "13be5753f0189233c25ddfe1bac64d1f",
            ),
        ];
        for (name, h, plain24, first16, tail) in cases {
            assert_eq!(hex(&h[..24]), plain24, "{name}: plaintext");
            let p = packet(&h);
            assert_eq!(hex(&p[..16]), first16, "{name}: first blocks");
            assert_eq!(hex(&p[488..504]), tail, "{name}: last blocks");
            assert_eq!(hex(&p[504..]), "000000000000a11a", "{name}: trailer");
        }
        // The paths end right after their bytes: the rest is zero.
        let clip = path(op::OPEN_FILE, "/usr/data/video/clip.h264");
        assert_eq!(&clip[16..41], b"/usr/data/video/clip.h264");
        assert!(clip[41..].iter().all(|&b| b == 0));
    }

    #[test]
    fn boot_logo_is_a_golden_vector_only() {
        // Spec § 6: /usr/data/boot.jpg, JPEG quality 95, at most 307,200
        // bytes, written by the vendor app with command 40 (never sent by
        // Bezel). Same OpenSSL script as the storage vectors.
        let mut jpeg = vec![0u8; boot_logo::MAX_BYTES];
        jpeg[..2].copy_from_slice(&boot_logo::JPEG_SOI);
        let h = boot_logo_header(TS, &jpeg).unwrap();
        assert_eq!(
            hex(&h[..24]),
            "28001a6d04030201000000120004b0002f7573722f646174"
        );
        assert_eq!(&h[16..34], boot_logo::PATH.as_bytes());
        let p = packet(&h);
        assert_eq!(hex(&p[..16]), "f96ad14e18448d82ccaffc5369103b25");
        assert_eq!(hex(&p[488..504]), "8c6137cdb91e48afcd7147df201de9c2");

        jpeg.push(0);
        assert!(boot_logo_header(TS, &jpeg).is_none(), "over 307,200 bytes");
        assert!(boot_logo_header(TS, b"\x89PNG").is_none(), "not a JPEG");
        assert!(boot_logo_header(TS, &[]).is_none());
        // A quality-95 JPEG of a small panel fits.
        let rgba = [40u8, 80, 120, 255].repeat(48 * 48);
        let q95 = encode_jpeg(&rgba, 48, 48, boot_logo::QUALITY, Subsampling::S420).unwrap();
        let h = boot_logo_header(0, &q95).unwrap();
        assert_eq!(&h[12..16], &(q95.len() as u32).to_be_bytes());
    }

    #[test]
    fn storage_replies() {
        let mut r = [0u8; PACKET_LEN];
        for (i, v) in [0u32, 0, 0, 262_144, 65_536, 196_608].iter().enumerate() {
            r[8 + 4 * i..12 + 4 * i].copy_from_slice(&v.to_le_bytes());
        }
        assert_eq!(StorageInfo::from_reply(&r), None, "no echo of 100");
        r[0] = op::STORAGE_INFO;
        let info = StorageInfo::from_reply(&r).unwrap().info();
        assert_eq!(
            info.internal,
            Capacity {
                total: 262_144 * 1024,
                used: 65_536 * 1024,
                free: 196_608 * 1024,
            },
            "KiB to bytes, no reserve"
        );
        assert_eq!(info.card, None, "TF total 0: no card");
        r[8..12].copy_from_slice(&1u32.to_le_bytes());
        r[16..20].copy_from_slice(&1u32.to_le_bytes());
        let card = StorageInfo::from_reply(&r).unwrap().info().card;
        assert_eq!(
            card,
            Some(Capacity {
                total: 1024,
                used: 0,
                free: 1024
            })
        );
        assert_eq!(StorageInfo::from_reply(&r[..31]), None);
        assert_eq!(StorageInfo::from_reply(&[]), None);

        let mut ok = [0u8; 16];
        ok[STATUS_AT] = STATUS_OK;
        assert!(accepted(op::OPEN_FILE, &ok));
        assert!(accepted(op::PLAY_VIDEO, &ok));
        assert!(!accepted(op::SHOW_IMAGE, &ok), "113 reports at [1]");
        let mut image_ok = [0u8; 16];
        image_ok[IMAGE_STATUS_AT] = STATUS_OK;
        assert!(accepted(op::SHOW_IMAGE, &image_ok));
        assert!(!accepted(op::PLAY_VIDEO, &image_ok));
        assert!(!accepted(op::OPEN_FILE, &[]));

        let mut busy = [0u8; 16];
        assert!(playback_idle(&busy));
        busy[STATUS_AT] = 1;
        assert!(!playback_idle(&busy));
        assert!(!playback_idle(&[]), "no reply is not idle");
        assert_eq!(
            (root::INTERNAL, root::CARD),
            ("/usr/data/", "/tmp/sdcard/mmcblk0p1/"),
            "spec § 6"
        );
    }

    #[test]
    fn replies() {
        let mut r = [0u8; PACKET_LEN];
        assert!(!resp_ok(&r));
        r[8] = STATUS_OK;
        assert!(resp_ok(&r));
        assert!(resp_ok(&[0, STATUS_OK]));
        assert!(!resp_ok(&[]));

        assert_eq!(sync_version(&r), None);
        r[0] = op::SYNC;
        r[8..20].copy_from_slice(b"TURZX_1_123\0");
        assert_eq!(sync_version(&r).as_deref(), Some("TURZX_1_123"));
        assert_eq!(sync_version(&[op::SYNC]).as_deref(), Some(""));
        assert_eq!(sync_version(&[]), None);

        let mut s = [0u8; 40];
        for (i, v) in [100u32, 40, 60, 2000, 500, 1500].iter().enumerate() {
            s[8 + 4 * i..12 + 4 * i].copy_from_slice(&v.to_le_bytes());
        }
        let info = StorageInfo::parse(&s).unwrap();
        assert_eq!(info.card_total_kib, 100);
        assert_eq!(info.card_free_kib, 60);
        assert_eq!(info.internal_used_kib, 500);
        assert_eq!(info.internal_free_kib, 1500);
        assert!(info.has_card());
        assert!(StorageInfo::parse(&s[..31]).is_none());

        let mut c = [0u8; 12];
        assert_eq!(chunk_size(&c), DEFAULT_CHUNK);
        c[8..12].copy_from_slice(&65_536u32.to_be_bytes());
        assert_eq!(chunk_size(&c), 65_536);
        c[8..12].copy_from_slice(&(2 * MAX_IMAGE as u32).to_be_bytes());
        assert_eq!(chunk_size(&c), DEFAULT_CHUNK);
        assert_eq!(chunk_size(&[]), DEFAULT_CHUNK);

        assert_eq!(file_size(&s), Some(100));
        assert_eq!(file_size(&s[..10]), None);
        assert_eq!(queue_depth(&s), Some(100));
        assert_eq!(queue_depth(&s[..8]), None);
    }

    #[test]
    fn jpeg_search_order_matches_the_reference() {
        let attempts: Vec<(Subsampling, u8)> = jpeg_attempts().collect();
        assert_eq!(attempts.len(), 3 * 26);
        let first: Vec<u8> = attempts.iter().take(26).map(|a| a.1).collect();
        assert_eq!(
            first,
            vec![
                90, 85, 80, 75, 70, 65, 60, 55, 50, 45, 40, 35, 30, 25, 20, 15, 10, 9, 8, 7, 6, 5,
                4, 3, 2, 1
            ]
        );
        assert_eq!(attempts[0].0, Subsampling::S420);
        assert_eq!(attempts[26], (Subsampling::S422, 90));
        assert_eq!(attempts[77], (Subsampling::S444, 1));
    }

    fn noise(w: u32, h: u32) -> Vec<u8> {
        let mut state = 0x1234_5678u32;
        (0..w * h * 4)
            .map(|i| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                if i % 4 == 3 { 255 } else { state as u8 }
            })
            .collect()
    }

    #[test]
    fn frames_are_png_when_they_fit() {
        let rgba = [10u8, 20, 30, 128].repeat(16 * 8);
        let img = encode_frame(&rgba, 16, 8, MAX_IMAGE).unwrap();
        assert_eq!(img.opcode(), op::SHOW_PNG);
        let bytes = img.bytes();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        // IHDR: 16 x 8, 8-bit RGBA (colour type 6).
        assert_eq!(&bytes[16..26], &[0, 0, 0, 16, 0, 0, 0, 8, 8, 6]);
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes.to_vec()));
        let mut reader = decoder.read_info().unwrap();
        let mut out = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut out).unwrap();
        assert_eq!(out, rgba, "alpha is kept");
    }

    #[test]
    fn large_frames_fall_back_to_jpeg_under_the_limit() {
        let rgba = noise(64, 64);
        let png = encode_png(&rgba, 64, 64).unwrap();
        let limit = 4096;
        assert!(png.len() > limit);
        let img = encode_frame(&rgba, 64, 64, limit).unwrap();
        assert_eq!(img.opcode(), op::SHOW_JPEG);
        assert!(img.bytes().len() <= limit);
        assert_eq!(&img.bytes()[..2], &[0xFF, 0xD8]);
        let q90 = encode_jpeg(&rgba, 64, 64, 90, Subsampling::S420).unwrap();
        assert!(q90.len() > limit, "the search had to lower the quality");
        let err = encode_frame(&rgba, 64, 64, 100).unwrap_err();
        assert!(matches!(err, EncodeError::TooLarge { limit: 100, .. }));
        assert!(err.to_string().contains("100 bytes"));
    }

    #[test]
    fn bad_buffers_are_rejected() {
        assert_eq!(encode_png(&[0; 15], 2, 2), Err(EncodeError::Size));
        assert_eq!(encode_png(&[], 0, 0), Err(EncodeError::Size));
        assert_eq!(
            encode_jpeg(&vec![0; 70_000 * 4], 70_000, 1, 90, Subsampling::S444),
            Err(EncodeError::Size)
        );
        assert!(EncodeError::Size.to_string().contains("size"));
        assert!(EncodeError::Png("x".into()).to_string().contains("PNG"));
        assert!(EncodeError::Jpeg("x".into()).to_string().contains("JPEG"));
        let jpeg = Image::Jpeg(vec![1, 2]);
        assert_eq!(jpeg.bytes(), &[1, 2]);
    }
}
