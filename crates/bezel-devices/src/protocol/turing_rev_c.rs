//! Turing rev C wire format (2.1"/2.8"/5"/8.8" UART generation).
//!
//! Spec: `docs/reverse-engineering/protocol-turing-rev-c.md`. Every command is
//! one 250-byte packet `[op] EF 69 [len u32 BE] [flag] 00 00 [data ≤ 240] 00…`;
//! bulk data follows as 250-byte blocks of 249 payload bytes plus one zero.
//! Replies are ASCII text.
//!
//! This module is pure: it only builds and parses bytes.

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

/// The run list sent when nothing changed: one dummy single-pixel record, as
/// the vendor app does every tick (the firmware tolerates the short record).
pub const NO_CHANGE: [u8; 6] = [0x80, 0, 0, 0, 0, 0];

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
