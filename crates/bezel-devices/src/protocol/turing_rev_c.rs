//! Turing rev C wire format (2.1"/2.8"/5"/8.8" UART generation).
//!
//! Spec: `docs/reverse-engineering/protocol-turing-rev-c.md`. Every command is
//! one 250-byte packet `[op] EF 69 [len u32 BE] [flag] 00 00 [data ≤ 240] 00…`;
//! bulk data follows as 250-byte blocks of 249 payload bytes plus one zero.
//! Replies are ASCII text.
//!
//! This module is pure: it only builds and parses bytes.

use bezel_core::domain::device::ModelId;
use bezel_core::domain::storage::{Capacity, Repeat, StorageInfo};

/// Size of every command packet and data block.
pub const BLOCK: usize = 250;
/// Payload bytes carried by each data block (the 250th byte is zero).
pub const BLOCK_PAYLOAD: usize = 249;
/// Largest inline payload a command packet can carry (offset 10 to 249).
pub const MAX_INLINE: usize = BLOCK - 10;
/// The two magic bytes after every opcode.
pub const MAGIC: [u8; 2] = [0xEF, 0x69];
/// Longest run of pixels one run record may describe.
pub const MAX_RUN: usize = 65_000;

/// Opcodes of the serial family.
pub mod op {
    /// Handshake; the device answers `chs_<model>.dev1_rom<version>`.
    pub const HELLO: u8 = 0x01;
    /// Storage info: `flashTotal-flashUsed-flashFree-sdTotal-sdUsed-sdFree` in KiB.
    pub const STORAGE_INFO: u8 = 0x64;
    /// List a directory (creates it when missing).
    pub const LIST_DIR: u8 = 0x65;
    /// Delete a file (destructive).
    pub const DELETE_FILE: u8 = 0x66;
    /// File size in bytes (`0` when absent).
    pub const FILE_SIZE: u8 = 0x6E;
    /// Create a file and stream its content.
    pub const UPLOAD_FILE: u8 = 0x6F;
    /// Play a video stored on the device (flag byte = loop).
    pub const PLAY_VIDEO: u8 = 0x78;
    /// Stop the device-side video.
    pub const STOP_VIDEO: u8 = 0x79;
    /// Backlight, raw 0..=255.
    pub const SET_BRIGHTNESS: u8 = 0x7B;
    /// Persistent options: brightness, start mode, 0, flip, sleep minutes.
    pub const SET_OPTIONS: u8 = 0x7D;
    /// Device-side rotation, 0..=3 quarter turns.
    pub const SET_ROTATION: u8 = 0x81;
    /// Screen off.
    pub const TURN_OFF: u8 = 0x83;
    /// Reboot the device (disruptive).
    pub const RESTART: u8 = 0x84;
    /// Enter streaming mode, sent once before the first full frame.
    pub const PRE_UPDATE_BITMAP: u8 = 0x86;
    /// Leave streaming mode, sent when the host stops driving the screen.
    pub const END_UPDATE_BITMAP: u8 = 0x87;
    /// Show an image stored on the device.
    pub const PLAY_IMAGE: u8 = 0x8C;
    /// Stop any device-side media; replies `media_stop` once stopped.
    pub const STOP_MEDIA: u8 = 0x96;
    /// Full frame, followed by the BGRA bytes.
    pub const DISPLAY_BITMAP: u8 = 0xC8;
    /// Partial update, followed by the run list.
    pub const UPDATE_BITMAP: u8 = 0xCC;
    /// Status: `needReSend:<0|1>|renderCnt:<n>|theme:<s>`.
    pub const QUERY_STATUS: u8 = 0xCF;
}

/// The two bytes HELLO carries (their meaning is unknown; both references send them).
pub const HELLO_PAYLOAD: [u8; 2] = [0xC5, 0xD3];

/// The MCU command that restarts the SoC (spec § 15, § 16, § 17.2): written
/// as is to the MCU port, not to the SoC, neither padded nor framed. The
/// vendor writes it in its reconnect ladder and holds the port 8 s. The SoC
/// leaves the bus at once and comes back about 10 s later, also from a hung
/// firmware (hardware, 8.8"). Disruptive, not destructive
/// (D-2026-09-30-release-polish-13).
pub const MCU_RESTART: [u8; 6] = [0x00, 0x00, 0x00, 0x00, 0x00, 0xC9];

/// Text the device answers with, matched by content (spec § 3, § 13).
pub mod reply {
    /// STOP_MEDIA: playback stopped.
    pub const MEDIA_STOPPED: &str = "media_stop";
    /// UPLOAD_FILE header accepted: the data phase may follow.
    pub const CREATED: &str = "create_success";
    /// UPLOAD_FILE data phase received and written.
    pub const RECEIVED: &str = "file_rev_done";
    /// PLAY_VIDEO started.
    pub const VIDEO_PLAYING: &str = "play_video_success";
    /// PLAY_IMAGE shown.
    pub const IMAGE_SHOWN: &str = "play_img_ok";
}

/// Storage roots (spec § 13.1). The media folders inside them (`img/`,
/// `video/`) are common to every family with storage.
pub mod root {
    /// Internal flash of the vendor's large screens.
    pub const LARGE_INTERNAL: &str = "/mnt/UDISK/";
    /// Internal flash of the vendor's small screens.
    pub const SMALL_INTERNAL: &str = "/root/";
    /// The TF card, on every model.
    pub const CARD: &str = "/mnt/SDCARD/";
}

/// Models in the vendor's large class (spec, introduction): storage under
/// `/mnt/UDISK`, 15 upload-completion rounds, no 0x82 before video play.
pub const LARGE_MODELS: [&str; 5] = [
    "turing-4",
    "turing-6.5",
    "turing-6.8",
    "turing-8",
    "turing-8.8",
];

/// The vendor's two classes of rev C screens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenClass {
    /// 4", 6.5", 6.8", 8", 8.8".
    Large,
    /// 2.1"/2.8" round, 2.4", 2.8" square, 3.4", 5".
    Small,
}

impl ScreenClass {
    /// The class of a rev C model.
    pub fn of(model: &ModelId) -> Self {
        if LARGE_MODELS.contains(&model.0) {
            ScreenClass::Large
        } else {
            ScreenClass::Small
        }
    }

    /// Root of the internal flash's media folders.
    pub const fn internal_root(self) -> &'static str {
        match self {
            ScreenClass::Large => root::LARGE_INTERNAL,
            ScreenClass::Small => root::SMALL_INTERNAL,
        }
    }
}

/// Builds a command packet. `len` is the opcode-specific length field
/// (payload length, 1 for argument-less commands, or the size of a following
/// data phase); `flag` is byte 7. `None` when `data` exceeds [`MAX_INLINE`].
pub fn command(opcode: u8, len: u32, flag: u8, data: &[u8]) -> Option<[u8; BLOCK]> {
    if data.len() > MAX_INLINE {
        return None;
    }
    let mut packet = [0u8; BLOCK];
    packet[0] = opcode;
    packet[1..3].copy_from_slice(&MAGIC);
    packet[3..7].copy_from_slice(&len.to_be_bytes());
    packet[7] = flag;
    packet[10..10 + data.len()].copy_from_slice(data);
    Some(packet)
}

fn fixed(opcode: u8, len: u32, data: &[u8]) -> [u8; BLOCK] {
    // Only called with constant payloads far below MAX_INLINE.
    command(opcode, len, 0, data).unwrap_or([0; BLOCK])
}

/// An argument-less command (`len = 1`).
pub fn simple(opcode: u8) -> [u8; BLOCK] {
    fixed(opcode, 1, &[])
}

/// HELLO.
pub fn hello() -> [u8; BLOCK] {
    fixed(op::HELLO, 1, &HELLO_PAYLOAD)
}

/// Backlight level, raw 0..=255.
pub fn set_brightness(level: u8) -> [u8; BLOCK] {
    fixed(op::SET_BRIGHTNESS, 1, &[level])
}

/// Device-side rotation in quarter turns (0..=3).
pub fn set_rotation(quarter_turns: u8) -> [u8; BLOCK] {
    fixed(op::SET_ROTATION, 1, &[quarter_turns & 3])
}

/// What the screen shows by itself after boot or when the host stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartMode {
    /// Built-in clock/logo.
    Default = 0,
    /// A stored image.
    Image = 1,
    /// A stored video.
    Video = 2,
}

/// The persistent options packet (0x7D).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Backlight, raw 0..=255.
    pub brightness: u8,
    /// Standalone content.
    pub start_mode: StartMode,
    /// Flip device-side media 180°.
    pub flip: bool,
    /// Minutes until the screen sleeps without host traffic (0 = never, max 10).
    pub sleep_minutes: u8,
}

/// SET_OPTIONS.
pub fn set_options(o: Options) -> [u8; BLOCK] {
    fixed(
        op::SET_OPTIONS,
        5,
        &[
            o.brightness,
            o.start_mode as u8,
            0,
            u8::from(o.flip),
            o.sleep_minutes.min(10),
        ],
    )
}

/// Longest device path UPLOAD_FILE carries: the inline payload minus the
/// LE32 file size after the path (spec § 3).
pub const MAX_UPLOAD_PATH: usize = MAX_INLINE - 4;

/// GET_STORAGE_INFO.
pub fn storage_info() -> [u8; BLOCK] {
    simple(op::STORAGE_INFO)
}

/// A command whose inline payload is a device path and whose length field
/// is the path's byte count: LIST_DIR, DELETE_FILE, GET_FILE_SIZE,
/// PLAY_IMAGE. `None` when the path exceeds [`MAX_INLINE`].
pub fn path_command(opcode: u8, path: &str) -> Option<[u8; BLOCK]> {
    let len = u32::try_from(path.len()).ok()?;
    command(opcode, len, 0, path.as_bytes())
}

/// PLAY_VIDEO: the loop flag rides in byte 7. `None` when the path exceeds
/// [`MAX_INLINE`].
pub fn play_video(path: &str, repeat: Repeat) -> Option<[u8; BLOCK]> {
    let len = u32::try_from(path.len()).ok()?;
    let flag = match repeat {
        Repeat::Once => 0,
        Repeat::Loop => 1,
    };
    command(op::PLAY_VIDEO, len, flag, path.as_bytes())
}

/// UPLOAD_FILE header: BE32 path length in the length field, then the path
/// and the file size as **LE32** (spec § 13.4). The data phase follows as
/// [`blocks`]. `None` when the path exceeds [`MAX_UPLOAD_PATH`].
pub fn upload_file(path: &str, size: u32) -> Option<[u8; BLOCK]> {
    if path.len() > MAX_UPLOAD_PATH {
        return None;
    }
    let mut data = Vec::with_capacity(path.len() + 4);
    data.extend_from_slice(path.as_bytes());
    data.extend_from_slice(&size.to_le_bytes());
    command(op::UPLOAD_FILE, u32::try_from(path.len()).ok()?, 0, &data)
}

/// The 250 × `0x2C` block sent before a full frame (and to resync after a failed HELLO).
pub const fn start_display_block() -> [u8; BLOCK] {
    [0x2C; BLOCK]
}

/// Header of a full frame of `len` BGRA bytes.
pub fn full_frame_header(len: u32) -> [u8; BLOCK] {
    fixed(op::DISPLAY_BITMAP, len, &[])
}

/// Header of a partial update carrying a run list of `list_len` bytes.
/// `seq` counts partial updates since the last full frame.
pub fn partial_header(list_len: u32, seq: u32) -> [u8; BLOCK] {
    let mut data = [0u8; 8];
    data[..4].copy_from_slice(&seq.to_be_bytes());
    fixed(op::UPDATE_BITMAP, list_len, &data)
}

/// Frames a data phase: 249 payload bytes + one zero per 250-byte block, the
/// last block zero-padded. Empty input produces no block.
pub fn blocks(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len().div_ceil(BLOCK_PAYLOAD) * BLOCK);
    for chunk in data.chunks(BLOCK_PAYLOAD) {
        out.extend_from_slice(chunk);
        out.resize(out.len() + BLOCK - chunk.len(), 0);
    }
    out
}

/// Pixel encoding of partial updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// 4 bytes per pixel: B, G, R, A (firmware ROM ≥ 1.89 on large panels).
    Bgra,
    /// 3 bytes per pixel: 6-bit B and G carrying 2 alpha bits each, 8-bit R.
    CompressedBgra,
}

impl PixelFormat {
    /// Bytes per encoded pixel.
    pub const fn bytes(self) -> usize {
        match self {
            PixelFormat::Bgra => 4,
            PixelFormat::CompressedBgra => 3,
        }
    }

    /// Appends one pixel given as B, G, R, A.
    fn push(self, out: &mut Vec<u8>, px: &[u8]) {
        match self {
            PixelFormat::Bgra => out.extend_from_slice(&px[..4]),
            PixelFormat::CompressedBgra => {
                let a4 = px[3] >> 4;
                out.push((px[0] & 0xFC) | (a4 >> 2));
                out.push((px[1] & 0xFC) | (a4 & 0x03));
                out.push(px[2]);
            }
        }
    }
}

/// Encodes the pixels that changed between two native BGRA buffers as a run
/// list: `[idx u24 BE][count u16 BE][pixels]` per run, or
/// `[idx | 0x800000 u24 BE][pixel]` for a single pixel. `idx` is the linear
/// pixel index in the native framebuffer.
///
/// Returns `None` when the buffers differ in length, when an index does not fit
/// 23 bits, or when the list would not be smaller than the frame (the caller
/// then sends a full frame instead). An unchanged frame yields an empty list.
pub fn diff_runs(previous: &[u8], current: &[u8], format: PixelFormat) -> Option<Vec<u8>> {
    if previous.len() != current.len() || !current.len().is_multiple_of(4) {
        return None;
    }
    let pixels = current.len() / 4;
    if pixels > 0x80_0000 {
        return None;
    }
    let changed = |i: usize| previous[i * 4..i * 4 + 4] != current[i * 4..i * 4 + 4];
    let mut out = Vec::new();
    let mut i = 0;
    while i < pixels {
        if !changed(i) {
            i += 1;
            continue;
        }
        let start = i;
        while i < pixels && i - start < MAX_RUN && changed(i) {
            i += 1;
        }
        push_run(&mut out, start, &current[start * 4..i * 4], format);
        if out.len() >= current.len() {
            return None;
        }
    }
    Some(out)
}

fn push_run(out: &mut Vec<u8>, start: usize, pixels: &[u8], format: PixelFormat) {
    let count = pixels.len() / 4;
    let idx = start as u32;
    if count == 1 {
        out.extend_from_slice(&(idx | 0x80_0000).to_be_bytes()[1..]);
    } else {
        out.extend_from_slice(&idx.to_be_bytes()[1..]);
        out.extend_from_slice(&(count as u16).to_be_bytes());
    }
    for px in pixels.as_chunks::<4>().0 {
        format.push(out, px);
    }
}

/// A parsed HELLO answer, e.g. `chs_88inch.dev1_rom1.90`.
#[derive(Debug, Clone, PartialEq)]
pub struct Hello {
    /// The printable answer.
    pub raw: String,
    /// Model token between `chs_` and the first dot (`88inch`, `5inch`).
    pub model: String,
    /// ROM version, e.g. 1.9 for `rom1.90`.
    pub rom: f32,
}

impl Hello {
    /// Parses a reply; `None` unless it contains a `chs_` answer.
    pub fn parse(reply: &[u8]) -> Option<Hello> {
        let text: String = reply
            .iter()
            .filter(|b| b.is_ascii_graphic() || **b == b' ')
            .map(|&b| char::from(b))
            .collect();
        let start = text.find("chs_")?;
        let raw = text[start..].to_string();
        let model = raw["chs_".len()..].split('.').next()?.to_string();
        let rom = raw.split("rom").nth(1).map_or(0.0, rom_version);
        Some(Hello { raw, model, rom })
    }

    /// Whether partial updates carry 4-byte BGRA pixels (ROM ≥ 1.89) or the
    /// 3-byte compressed form.
    pub fn partial_format(&self) -> PixelFormat {
        if self.rom >= 1.89 {
            PixelFormat::Bgra
        } else {
            PixelFormat::CompressedBgra
        }
    }
}

/// The vendor's ROM parser: every digit after `rom` adds `d / 10^k`.
fn rom_version(s: &str) -> f32 {
    s.chars()
        .filter_map(|c| c.to_digit(10))
        .enumerate()
        .map(|(k, d)| d as f32 / 10f32.powi(k as i32))
        .sum()
}

/// A parsed QUERY_STATUS reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// The device asks for a full frame.
    pub need_resend: bool,
    /// Frames the device rendered (advances while it plays a video).
    pub render_count: Option<u64>,
    /// Theme name the device reports, if any.
    pub theme: Option<String>,
}

impl Status {
    /// Parses `needReSend:0|renderCnt:123|theme:x`; `None` without a `|`.
    pub fn parse(reply: &[u8]) -> Option<Status> {
        let text = String::from_utf8_lossy(reply).replace('\0', "");
        if !text.contains('|') {
            return None;
        }
        let mut parts = text.split('|');
        let first = parts.next().unwrap_or_default();
        let render_count = parts
            .next()
            .and_then(|p| p.trim().strip_prefix("renderCnt:"))
            .and_then(|n| n.trim().parse().ok());
        let theme = parts
            .next()
            .and_then(|p| p.trim().strip_prefix("theme:"))
            .map(|t| t.trim().to_string());
        Some(Status {
            need_resend: first.contains("needReSend:1"),
            render_count,
            theme,
        })
    }
}

/// Flash the vendor keeps out of the reported total and free (spec § 13.2).
pub const FLASH_RESERVE_KIB: u64 = 512;
/// A card counts as inserted only when its total exceeds this (spec § 13.2).
pub const CARD_PRESENT_ABOVE_KIB: u64 = 1024;
/// Bytes per KiB, the unit of GET_STORAGE_INFO.
const KIB: u64 = 1024;

/// A reply as text: NUL bytes removed (the vendor's parser does the same),
/// surrounding whitespace trimmed.
fn text(reply: &[u8]) -> String {
    String::from_utf8_lossy(reply)
        .replace('\0', "")
        .trim()
        .to_string()
}

/// A GET_STORAGE_INFO reply `a-b-c-d-e-f`, fields in KiB as the device sends
/// them: flash total, used, free, then TF card total, used, free.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageReport {
    /// Flash total, reserve included.
    pub flash_total: u64,
    /// Flash in use.
    pub flash_used: u64,
    /// Flash free, reserve included.
    pub flash_free: u64,
    /// Card total; 0 without a card.
    pub card_total: u64,
    /// Card in use.
    pub card_used: u64,
    /// Card free.
    pub card_free: u64,
}

impl StorageReport {
    /// Parses a reply; `None` unless it is six decimal fields joined by `-`.
    pub fn parse(reply: &[u8]) -> Option<Self> {
        let text = text(reply);
        let fields: Vec<u64> = text
            .split('-')
            .map(|f| f.trim().parse().ok())
            .collect::<Option<_>>()?;
        let [
            flash_total,
            flash_used,
            flash_free,
            card_total,
            card_used,
            card_free,
        ] = fields.try_into().ok()?;
        Some(Self {
            flash_total,
            flash_used,
            flash_free,
            card_total,
            card_used,
            card_free,
        })
    }

    /// The capacities in bytes, as the vendor reads them: the flash reserve
    /// is taken off total and free, and the card exists only when its total
    /// exceeds [`CARD_PRESENT_ABOVE_KIB`].
    pub fn info(&self) -> StorageInfo {
        let bytes = |kib: u64| kib.saturating_mul(KIB);
        let internal = Capacity {
            total: bytes(self.flash_total.saturating_sub(FLASH_RESERVE_KIB)),
            used: bytes(self.flash_used),
            free: bytes(self.flash_free.saturating_sub(FLASH_RESERVE_KIB)),
        };
        let card = (self.card_total > CARD_PRESENT_ABOVE_KIB).then(|| Capacity {
            total: bytes(self.card_total),
            used: bytes(self.card_used),
            free: bytes(self.card_free),
        });
        StorageInfo { internal, card }
    }
}

/// A GET_FILE_SIZE reply: the decimal size in bytes (`0` for an absent
/// file). `None` unless the whole reply is a number.
pub fn file_size(reply: &[u8]) -> Option<u64> {
    text(reply).parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// `hex` of a 250-byte packet without its zero padding.
    fn head(packet: &[u8; BLOCK]) -> String {
        let end = packet.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
        hex(&packet[..end])
    }

    #[test]
    fn command_packets_match_the_reference_vectors() {
        // docs: protocol-turing-rev-c.md § Test vectors (verified against the Python reference).
        assert_eq!(head(&hello()), "01ef6900000001000000c5d3");
        assert_eq!(head(&set_brightness(0x3f)), "7bef69000000010000003f");
        assert_eq!(head(&simple(op::STOP_VIDEO)), "79ef6900000001");
        assert_eq!(head(&simple(op::STOP_MEDIA)), "96ef6900000001");
        assert_eq!(head(&simple(op::TURN_OFF)), "83ef6900000001");
        assert_eq!(head(&simple(op::RESTART)), "84ef6900000001");
        assert_eq!(head(&simple(op::QUERY_STATUS)), "cfef6900000001");
        assert_eq!(head(&simple(op::PRE_UPDATE_BITMAP)), "86ef6900000001");
        assert_eq!(head(&set_rotation(2)), "81ef690000000100000002");
        assert_eq!(set_rotation(6)[10], 2);
        assert!(start_display_block().iter().all(|&b| b == 0x2C));
        // docs: § 17.2, "MCU command": six bytes, not padded.
        assert_eq!(hex(&MCU_RESTART), "0000000000c9");
    }

    /// Asserts the whole 250-byte packet: `head_hex`, then `zeros` zero bytes.
    fn assert_packet(packet: Option<[u8; BLOCK]>, head_hex: &str, zeros: usize) {
        let packet = packet.expect("the packet fits");
        let expected = format!("{}{}", head_hex.replace(' ', ""), "00".repeat(zeros));
        assert_eq!(hex(&packet), expected);
    }

    #[test]
    fn storage_packets_match_the_reference_vectors() {
        // docs: protocol-turing-rev-c.md § 17.2 (vendor, static, computed).
        assert_packet(Some(storage_info()), "64ef6900000001", 243);
        assert_packet(
            path_command(op::LIST_DIR, "/mnt/UDISK/video/"),
            "65 ef 69 00 00 00 11 00 00 00 2f 6d 6e 74 2f 55 44 49 53 4b 2f 76 69 64 65 6f 2f",
            223,
        );
        assert_packet(
            path_command(op::FILE_SIZE, "/mnt/SDCARD/video/"),
            "6e ef 69 00 00 00 12 00 00 00 2f 6d 6e 74 2f 53 44 43 41 52 44 2f 76 69 64 65 6f 2f",
            222,
        );
        assert_packet(
            play_video("/mnt/SDCARD/video/88.mp4", Repeat::Loop),
            "78 ef 69 00 00 00 18 01 00 00 2f 6d 6e 74 2f 53 44 43 41 52 44 2f 76 69 64 65 6f 2f \
             38 38 2e 6d 70 34",
            216,
        );
        assert_packet(
            upload_file("/mnt/UDISK/video/88.mp4", 12_345_678),
            "6f ef 69 00 00 00 17 00 00 00 2f 6d 6e 74 2f 55 44 49 53 4b 2f 76 69 64 65 6f 2f \
             38 38 2e 6d 70 34 4e 61 bc 00",
            213,
        );
        assert_packet(
            Some(set_options(Options {
                brightness: 170,
                start_mode: StartMode::Video,
                flip: false,
                sleep_minutes: 0,
            })),
            "7d ef 69 00 00 00 05 00 00 00 aa 02 00 00 00",
            235,
        );
        // Same layout, from the command table (§ 3): no loop flag, path at byte 10.
        let image = "/mnt/SDCARD/img/logo.png";
        assert_packet(
            path_command(op::PLAY_IMAGE, image),
            &format!("8cef6900000018000000{}", hex(image.as_bytes())),
            250 - 10 - image.len(),
        );
        let stored = "/mnt/UDISK/img/logo.png";
        assert_packet(
            path_command(op::DELETE_FILE, stored),
            &format!("66ef6900000017000000{}", hex(stored.as_bytes())),
            250 - 10 - stored.len(),
        );
        assert_eq!(play_video("/a.mp4", Repeat::Once).map(|p| p[7]), Some(0));

        // Paths too long for one packet are refused, never truncated.
        let longest = format!("/{}", "a".repeat(MAX_UPLOAD_PATH - 1));
        assert!(upload_file(&longest, 1).is_some());
        assert!(upload_file(&format!("{longest}a"), 1).is_none());
        let too_long = "a".repeat(MAX_INLINE + 1);
        assert!(path_command(op::LIST_DIR, &too_long).is_none());
        assert!(play_video(&too_long, Repeat::Loop).is_none());
    }

    #[test]
    fn storage_info_subtracts_the_reserved_flash_and_detects_the_card() {
        let kib = |n: u64| n * 1024;
        let report =
            StorageReport::parse(b"7340032-1048576-6291456-31260672-2048-31258624\0\0").unwrap();
        assert_eq!(report.flash_total, 7_340_032);
        let info = report.info();
        assert_eq!(
            info.internal,
            Capacity {
                total: kib(7_340_032 - 512),
                used: kib(1_048_576),
                free: kib(6_291_456 - 512),
            }
        );
        assert_eq!(
            info.card,
            Some(Capacity {
                total: kib(31_260_672),
                used: kib(2048),
                free: kib(31_258_624),
            })
        );

        // A card counts only above 1024 KiB; TF total 0 means none.
        for (reply, card) in [
            (&b"7340032-0-7340032-0-0-0"[..], false),
            (b"7340032-0-7340032-1024-0-1024", false),
            (b" 7340032-0-7340032-1025-0-1025\r\n", true),
        ] {
            let info = StorageReport::parse(reply).unwrap().info();
            assert_eq!(
                info.card.is_some(),
                card,
                "{}",
                String::from_utf8_lossy(reply)
            );
        }
        // The reserve never makes a capacity negative.
        let tiny = StorageReport::parse(b"100-0-100-0-0-0").unwrap().info();
        assert_eq!((tiny.internal.total, tiny.internal.free), (0, 0));

        for bad in [
            &b""[..],
            b"1-2-3",
            b"a-b-c-d-e-f",
            b"1-2-3-4-5-6-7",
            b"media_stop",
        ] {
            assert!(StorageReport::parse(bad).is_none(), "{bad:?}");
        }
        assert_eq!(file_size(b"12345678\0"), Some(12_345_678));
        assert_eq!(file_size(b"0"), Some(0));
        assert_eq!(file_size(b"file_rev_done"), None);
        assert_eq!(file_size(b""), None);
    }

    #[test]
    fn screen_classes_pick_the_internal_root() {
        let class = |id: &'static str| ScreenClass::of(&ModelId(id));
        assert_eq!(class("turing-8.8"), ScreenClass::Large);
        assert_eq!(class("turing-4").internal_root(), "/mnt/UDISK/");
        assert_eq!(class("turing-5"), ScreenClass::Small);
        assert_eq!(class("turing-2.1").internal_root(), "/root/");
    }

    #[test]
    fn options_packet_layout() {
        let p = set_options(Options {
            brightness: 0x2d,
            start_mode: StartMode::Default,
            flip: false,
            sleep_minutes: 0,
        });
        // The Python reference's OPTIONS is exactly this (its 0x2d is the brightness field).
        assert_eq!(head(&p), "7def69000000050000002d");
        let p = set_options(Options {
            brightness: 170,
            start_mode: StartMode::Video,
            flip: true,
            sleep_minutes: 42,
        });
        assert_eq!(
            &p[..15],
            &[0x7d, 0xef, 0x69, 0, 0, 0, 5, 0, 0, 0, 170, 2, 0, 1, 10]
        );
        assert_eq!(StartMode::Image as u8, 1);
    }

    #[test]
    fn full_frame_header_of_the_88_inch() {
        // 480 x 1920 x 4 = 3,686,400 = 0x00384000; the vendor sends zeros after it.
        assert_eq!(head(&full_frame_header(480 * 1920 * 4)), "c8ef69003840");
        assert_eq!(
            &full_frame_header(0x0038_4000)[3..10],
            &[0, 0x38, 0x40, 0, 0, 0, 0]
        );
    }

    #[test]
    fn partial_header_carries_size_and_sequence() {
        let p = partial_header(0x1e, 1);
        assert_eq!(
            &p[..18],
            &[
                0xcc, 0xef, 0x69, 0, 0, 0, 0x1e, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0
            ]
        );
    }

    #[test]
    fn inline_payload_limit() {
        assert!(command(op::LIST_DIR, 240, 0, &[b'a'; 240]).is_some());
        assert!(command(op::LIST_DIR, 241, 0, &[b'a'; 241]).is_none());
        let play = command(op::PLAY_VIDEO, 5, 1, b"/a.mp").unwrap();
        assert_eq!(play[7], 1, "loop flag lives in byte 7");
    }

    #[test]
    fn blocks_frame_every_249_bytes() {
        assert!(blocks(&[]).is_empty());
        let b = blocks(&[7u8; 249]);
        assert_eq!(b.len(), 250);
        assert_eq!(b[249], 0);
        let b = blocks(&[7u8; 250]);
        assert_eq!(b.len(), 500);
        assert_eq!((b[249], b[250], b[251]), (0, 7, 0));
        // The 8.8" frame: 3,686,400 bytes -> 14,805 blocks = 3,701,250 bytes on the wire.
        assert_eq!(blocks(&vec![1u8; 3_686_400]).len(), 3_701_250);
    }

    #[test]
    fn diff_runs_encode_runs_and_single_pixels() {
        let w = 4;
        let prev = vec![0u8; w * 2 * 4];
        let mut cur = prev.clone();
        // pixel 1 alone, pixels 4..=6 as a run
        cur[4..8].copy_from_slice(&[1, 2, 3, 255]);
        for i in 4..7 {
            cur[i * 4..i * 4 + 4].copy_from_slice(&[9, 8, 7, 255]);
        }
        let list = diff_runs(&prev, &cur, PixelFormat::Bgra).unwrap();
        assert_eq!(
            hex(&list),
            "800001010203ff".to_string() + "0000040003" + "090807ff090807ff090807ff"
        );
        let c = diff_runs(&prev, &cur, PixelFormat::CompressedBgra).unwrap();
        // B=1 -> (1&0xfc)|3 = 3 ; G=2 -> (2&0xfc)|3 = 3 ; R=3
        assert_eq!(&c[..6], &[0x80, 0, 1, 3, 3, 3]);
        assert!(
            diff_runs(&prev, &prev, PixelFormat::Bgra)
                .unwrap()
                .is_empty()
        );
        assert!(diff_runs(&prev, &cur[..8], PixelFormat::Bgra).is_none());
    }

    #[test]
    fn diff_runs_split_long_runs_and_give_up_when_larger_than_a_frame() {
        let prev = vec![0u8; (MAX_RUN + 10) * 4];
        let mut cur = prev.clone();
        for px in cur.as_chunks_mut::<4>().0 {
            *px = [1, 1, 1, 255];
        }
        // Everything changed: the list is bigger than the frame -> full frame instead.
        assert!(diff_runs(&prev, &cur, PixelFormat::Bgra).is_none());
        let list = diff_runs(&prev, &cur, PixelFormat::CompressedBgra).unwrap();
        assert_eq!(
            &list[..5],
            &[0, 0, 0, 0xfd, 0xe8],
            "first run capped at 65,000"
        );
        let second = 5 + MAX_RUN * 3;
        assert_eq!(&list[second..second + 5], &[0x00, 0xfd, 0xe8, 0x00, 0x0a]);
    }

    #[test]
    fn hello_answers() {
        let h = Hello::parse(b"chs_88inch.dev1_rom1.90\0\0").unwrap();
        assert_eq!(h.model, "88inch");
        assert!((h.rom - 1.9).abs() < 1e-6);
        assert_eq!(h.partial_format(), PixelFormat::Bgra);
        let h = Hello::parse(b"\x01chs_5inch.dev1_rom1.87").unwrap();
        assert_eq!(h.model, "5inch");
        assert_eq!(h.partial_format(), PixelFormat::CompressedBgra);
        assert!(Hello::parse(b"garbage").is_none());
        assert_eq!(Hello::parse(b"chs_x").unwrap().rom, 0.0);
    }

    #[test]
    fn status_answers() {
        let s = Status::parse(b"needReSend:0|renderCnt:12345|theme:AMD\0").unwrap();
        assert!(!s.need_resend);
        assert_eq!(s.render_count, Some(12345));
        assert_eq!(s.theme.as_deref(), Some("AMD"));
        let s = Status::parse(b"needReSend:1|renderCnt:x").unwrap();
        assert!(s.need_resend);
        assert_eq!(s.render_count, None);
        assert!(Status::parse(b"").is_none());
    }
}
