//! WCH-based panels wire format (VID 0x43A8).
//!
//! Spec: `docs/reverse-engineering/protocol-wch.md`. A command is one 32-byte
//! packet `AA 55 [DES-ECB(key, [cmd, arg, 00×6])] 01 00×20 BB`; its 32-byte
//! reply carries DES-ECB-encrypted bytes at `[2..10]`. A frame is raw BGR888,
//! top row first, cut into 480-byte payloads that each follow a 32-byte
//! header `AA 55 00×8 02 [BE16 sequence, 1-based] 00×18 BB` (512-byte blocks,
//! not encrypted), written in 4096-byte USB transfers. There is no partial
//! update.
//!
//! This module is pure: it only builds and parses bytes.

use std::ops::Range;

use ecb::cipher::{Block, BlockModeDecrypt, BlockModeEncrypt, KeyInit};

type DesEcbEnc = ecb::Encryptor<des::Des>;
type DesEcbDec = ecb::Decryptor<des::Des>;

/// DES key of the command channel (raw bytes, not ASCII).
pub const KEY: [u8; 8] = [0x41, 0x5F, 0xD9, 0xFA, 0x13, 0x42, 0x58, 0xB7];
/// Size of a command packet and of a reply.
pub const PACKET_LEN: usize = 32;
/// First two bytes of every packet and block.
pub const SYNC: [u8; 2] = [0xAA, 0x55];
/// Last byte of every packet and block header.
pub const END: u8 = 0xBB;
/// Byte 10 of a command packet.
pub const TYPE_COMMAND: u8 = 0x01;
/// Byte 10 of a frame block header.
pub const TYPE_DATA: u8 = 0x02;
/// Bytes of a packet or reply that are DES-encrypted.
pub const CIPHER: Range<usize> = 2..10;
/// Size of a frame block.
pub const BLOCK_LEN: usize = 512;
/// Size of a frame block header.
pub const BLOCK_HEADER: usize = 32;
/// Pixel bytes per frame block.
pub const BLOCK_PAYLOAD: usize = BLOCK_LEN - BLOCK_HEADER;
/// Size of every frame USB transfer.
pub const USB_CHUNK: usize = 4096;
/// Bulk OUT endpoint (vendor app).
pub const EP_OUT: u8 = 0x02;
/// IN endpoint (vendor app).
pub const EP_IN: u8 = 0x82;

/// Command opcodes (the firmware enum is {2..9, 56, 64, 65, 84..88, 112};
/// only these are sent by the vendor app).
pub mod op {
    /// StopVideo: sent when the device is added and when the host stops.
    pub const STOP_VIDEO: u8 = 0x38;
    /// Firmware and panel-type query; answered with [`VERSION_REPLY`].
    pub const GET_VERSION: u8 = 0x40;
    /// Plaintext `[2]` of a successful [`GET_VERSION`] reply.
    pub const VERSION_REPLY: u8 = 0x41;
    /// Backlight, 0..=255 (0 turns the panel off).
    pub const SET_BRIGHTNESS: u8 = 0x54;
    /// Device-side rotation 0..=3 (whether the firmware applies it is unknown).
    pub const SET_ROTATION: u8 = 0x56;
    /// Resets the frame buffer: at theme start and after an ACK of [`super::ACK_RESET`].
    pub const RESET_MEM: u8 = 0x58;
    /// Factory/test mode (only with the vendor app's `-test` switch).
    pub const TEST_MODE: u8 = 0x70;
}

/// Plaintext `[2]` of a frame ACK that asks for [`op::RESET_MEM`].
pub const ACK_RESET: u8 = 0x60;

/// DES-ECB encryption of one 8-byte block with [`KEY`].
pub fn encrypt_block(plain: [u8; 8]) -> [u8; 8] {
    let mut block = Block::<DesEcbEnc>::from(plain);
    DesEcbEnc::new(&KEY.into()).encrypt_block(&mut block);
    block.into()
}

/// DES-ECB decryption of one 8-byte block with [`KEY`].
pub fn decrypt_block(cipher: [u8; 8]) -> [u8; 8] {
    let mut block = Block::<DesEcbDec>::from(cipher);
    DesEcbDec::new(&KEY.into()).decrypt_block(&mut block);
    block.into()
}

/// A command packet: `AA 55 [E(cmd, arg, 0×6)] 01 00×20 BB`.
pub fn command(cmd: u8, arg: u8) -> [u8; PACKET_LEN] {
    let mut p = [0u8; PACKET_LEN];
    p[..2].copy_from_slice(&SYNC);
    p[CIPHER].copy_from_slice(&encrypt_block([cmd, arg, 0, 0, 0, 0, 0, 0]));
    p[10] = TYPE_COMMAND;
    p[PACKET_LEN - 1] = END;
    p
}

/// A reply as the vendor app reads it: up to 32 bytes over a zeroed buffer,
/// with `[2..10]` decrypted in place (bytes 0-1 and 10-31 stay raw).
pub fn decrypt_reply(reply: &[u8]) -> [u8; PACKET_LEN] {
    let mut r = [0u8; PACKET_LEN];
    let n = reply.len().min(PACKET_LEN);
    r[..n].copy_from_slice(&reply[..n]);
    let mut cipher = [0u8; 8];
    cipher.copy_from_slice(&r[CIPHER]);
    r[CIPHER].copy_from_slice(&decrypt_block(cipher));
    r
}

/// Panel types the [`op::GET_VERSION`] reply names at plaintext `[5]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    /// 3.38" bar, 180 x 640 (`338_Rect`).
    Rect338,
    /// 2.8" rectangle, 240 x 320 (`28_Rect`).
    Rect28,
    /// 2.8" square, 320 x 320 (`28_Sq`).
    Square28,
    /// 2.4" rectangle, 240 x 320 (`24_Rect`).
    Rect24,
}

impl Panel {
    /// The panel of a type code; `None` for codes the vendor app does not map
    /// (the 4.3" has none).
    pub fn from_code(code: u8) -> Option<Panel> {
        match code {
            4 => Some(Panel::Rect338),
            5 => Some(Panel::Rect28),
            6 => Some(Panel::Square28),
            8 => Some(Panel::Rect24),
            _ => None,
        }
    }

    /// The vendor app's model string.
    pub fn name(self) -> &'static str {
        match self {
            Panel::Rect338 => "338_Rect",
            Panel::Rect28 => "28_Rect",
            Panel::Square28 => "28_Sq",
            Panel::Rect24 => "24_Rect",
        }
    }
}

/// A decrypted [`op::GET_VERSION`] reply: `[0x41, major, minor, panel, …]` at `[2..]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    /// Firmware major version.
    pub major: u8,
    /// Firmware minor version.
    pub minor: u8,
    /// Panel type code.
    pub panel_code: u8,
}

impl Version {
    /// Parses a decrypted reply; `None` unless `[2]` is 0x41.
    pub fn parse(reply: &[u8; PACKET_LEN]) -> Option<Version> {
        (reply[2] == op::VERSION_REPLY).then_some(Version {
            major: reply[3],
            minor: reply[4],
            panel_code: reply[5],
        })
    }

    /// The panel it names, if the code is known.
    pub fn panel(&self) -> Option<Panel> {
        Panel::from_code(self.panel_code)
    }

    /// Firmware as the vendor app formats it, `major.minor` in decimal.
    pub fn firmware(&self) -> String {
        format!("{}.{}", self.major, self.minor)
    }
}

/// Bytes per BGR888 row: `(w * 3 + 3) & !3` (GDI+ 24-bit stride).
pub const fn stride(width: usize) -> usize {
    (width * 3 + 3) & !3
}

/// RGBA8 rows (top first) to BGR888 with the GDI+ stride; alpha is dropped.
/// `None` when `width` is 0 or the buffer is not whole rows.
pub fn bgr888(rgba: &[u8], width: usize) -> Option<Vec<u8>> {
    let row = width.checked_mul(4)?;
    if width == 0 || !rgba.len().is_multiple_of(row) {
        return None;
    }
    let stride = stride(width);
    let mut out = Vec::with_capacity(rgba.len() / row * stride);
    for line in rgba.chunks_exact(row) {
        for px in line.as_chunks::<4>().0 {
            out.extend_from_slice(&[px[2], px[1], px[0]]);
        }
        out.resize(out.len() + stride - width * 3, 0);
    }
    Some(out)
}

/// The 32-byte header of frame block `seq` (1-based).
pub fn block_header(seq: u16) -> [u8; BLOCK_HEADER] {
    let mut h = [0u8; BLOCK_HEADER];
    h[..2].copy_from_slice(&SYNC);
    h[10] = TYPE_DATA;
    h[11..13].copy_from_slice(&seq.to_be_bytes());
    h[BLOCK_HEADER - 1] = END;
    h
}

/// Frames `data` as 512-byte blocks of header + 480 bytes, the last block
/// zero-padded. Empty input gives no block.
pub fn blocks(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len().div_ceil(BLOCK_PAYLOAD) * BLOCK_LEN);
    for (i, chunk) in data.chunks(BLOCK_PAYLOAD).enumerate() {
        // The vendor app writes (i + 1) as two bytes, so it wraps at 65,536.
        out.extend_from_slice(&block_header((i + 1) as u16));
        out.extend_from_slice(chunk);
        out.resize(out.len() + BLOCK_PAYLOAD - chunk.len(), 0);
    }
    out
}

/// The USB transfers of a block buffer: 4096 bytes each. The vendor app
/// always writes a full 4096 bytes; a short tail (which no known panel
/// produces) is zero-padded here instead of carrying stale bytes.
pub fn usb_writes(buf: &[u8]) -> impl Iterator<Item = [u8; USB_CHUNK]> + '_ {
    buf.chunks(USB_CHUNK).map(|chunk| {
        let mut w = [0u8; USB_CHUNK];
        w[..chunk.len()].copy_from_slice(chunk);
        w
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn packets_match_the_reference_vectors() {
        // docs: protocol-wch.md § 10; the ciphertexts were recomputed with
        // pycryptodome (DES.MODE_ECB, key 41 5f d9 fa 13 42 58 b7).
        let tail = "01".to_string() + &"00".repeat(20) + "bb";
        let cases = [
            (op::STOP_VIDEO, 0, "767623ac62ee3919"),
            (op::GET_VERSION, 0, "22617a4aeb549889"),
            (op::SET_BRIGHTNESS, 170, "ae5754591c6e5192"),
            (op::SET_BRIGHTNESS, 0, "8b88d1140385c9c9"),
            (op::SET_BRIGHTNESS, 255, "804a57c4668060b9"),
            (op::SET_ROTATION, 0, "8ec3bcb851cbc7d9"),
            (op::SET_ROTATION, 1, "008b58049e3b204d"),
            (op::SET_ROTATION, 2, "cff8c4447a119021"),
            (op::SET_ROTATION, 3, "24b32c24d218efe9"),
            (op::RESET_MEM, 0, "3a817542544ddb3f"),
            (op::TEST_MODE, 0, "910bc695c8435c3b"),
        ];
        for (cmd, arg, cipher) in cases {
            let p = command(cmd, arg);
            assert_eq!(hex(&p), format!("aa55{cipher}{tail}"), "{cmd:#04x} {arg}");
        }

        // Replies (pycryptodome): E(plaintext) at [2..10], decrypted in place.
        let replies = [
            ([0x41, 1, 2, 8, 0, 0, 0, 0], "fb19fc859925c8ed"),
            ([0x41, 3, 14, 4, 0, 0, 0, 0], "5f1e6355fc671b3d"),
            ([0x60, 0, 0, 0, 0, 0, 0, 0], "4a54915bdc1c9b3b"),
        ];
        for (plain, cipher) in replies {
            assert_eq!(hex(&encrypt_block(plain)), cipher);
            let mut raw = [0u8; PACKET_LEN];
            raw[..2].copy_from_slice(&SYNC);
            raw[CIPHER].copy_from_slice(&encrypt_block(plain));
            raw[31] = END;
            let r = decrypt_reply(&raw);
            assert_eq!(r[CIPHER], plain);
            assert_eq!(
                (r[0], r[1], r[31]),
                (0xAA, 0x55, 0xBB),
                "raw bytes stay raw"
            );
        }
        // A read that timed out leaves zeros, which "decrypt" to this.
        assert_eq!(hex(&decrypt_reply(&[])[CIPHER]), "579540825369ffee");
        // The vendor's PKCS#7 second block, which it discards.
        assert_eq!(hex(&encrypt_block([8; 8])), "d79721aacd7e2fcd");

        // First frame block header, as printed in spec § 7.2.
        let spec = "aa 55 00 00 00 00 00 00 00 00 02 00 01 00 00 00 00 00 00 00 00 00 00 00 00 \
                    00 00 00 00 00 00 bb";
        assert_eq!(hex(&block_header(1)), spec.replace(' ', ""));
    }

    #[test]
    fn version_replies() {
        let mut r = [0u8; PACKET_LEN];
        r[2..6].copy_from_slice(&[0x41, 1, 12, 8]);
        let v = Version::parse(&r).unwrap();
        assert_eq!(v.firmware(), "1.12");
        assert_eq!(v.panel(), Some(Panel::Rect24));
        assert_eq!(v.panel().map(Panel::name), Some("24_Rect"));
        r[2] = 0x57;
        assert!(Version::parse(&r).is_none());
        assert!(Version::parse(&decrypt_reply(&[])).is_none());
        let names: Vec<Option<&str>> = (0..10)
            .map(|c| Panel::from_code(c).map(Panel::name))
            .collect();
        assert_eq!(
            names,
            vec![
                None,
                None,
                None,
                None,
                Some("338_Rect"),
                Some("28_Rect"),
                Some("28_Sq"),
                None,
                Some("24_Rect"),
                None
            ]
        );
        assert_eq!(
            decrypt_block(encrypt_block([1, 2, 3, 4, 5, 6, 7, 8])),
            [1, 2, 3, 4, 5, 6, 7, 8]
        );
    }

    #[test]
    fn bgr_rows_follow_the_gdi_stride() {
        assert_eq!(stride(240), 720);
        assert_eq!(stride(1), 4);
        assert_eq!(stride(2), 8);
        assert_eq!(stride(5), 16);
        let rgba = [1, 2, 3, 255, 4, 5, 6, 0];
        assert_eq!(bgr888(&rgba, 2).unwrap(), vec![3, 2, 1, 6, 5, 4, 0, 0]);
        assert_eq!(bgr888(&rgba, 1).unwrap(), vec![3, 2, 1, 0, 6, 5, 4, 0]);
        assert!(bgr888(&rgba, 0).is_none());
        assert!(bgr888(&rgba[..7], 2).is_none());
        assert!(bgr888(&[], 3).unwrap().is_empty());
    }

    #[test]
    fn blocks_carry_480_bytes_behind_a_sequenced_header() {
        assert!(blocks(&[]).is_empty());
        let data: Vec<u8> = (0..1000u32).map(|i| i as u8).collect();
        let b = blocks(&data);
        assert_eq!(b.len(), 3 * BLOCK_LEN);
        assert_eq!(&b[..BLOCK_HEADER], &block_header(1));
        assert_eq!(&b[BLOCK_HEADER..BLOCK_LEN], &data[..480]);
        assert_eq!(&b[BLOCK_LEN + 11..BLOCK_LEN + 13], &[0, 2]);
        assert_eq!(&b[2 * BLOCK_LEN + 11..2 * BLOCK_LEN + 13], &[0, 3]);
        assert_eq!(b[2 * BLOCK_LEN + BLOCK_HEADER], data[960]);
        assert!(
            b[2 * BLOCK_LEN + BLOCK_HEADER + 40..]
                .iter()
                .all(|&x| x == 0)
        );
        assert_eq!(&block_header(0x0102)[11..13], &[1, 2]);
    }

    #[test]
    fn frame_sizes_match_the_spec_table() {
        // (W, H, BGR bytes, 512-byte blocks, bytes on the wire, 4096-byte writes)
        let table = [
            (180, 640, 345_600, 720, 368_640, 90),
            (480, 272, 391_680, 816, 417_792, 102),
            (320, 320, 307_200, 640, 327_680, 80),
            (240, 320, 230_400, 480, 245_760, 60),
        ];
        for (w, h, bytes, n_blocks, wire, writes) in table {
            let bgr = bgr888(&vec![0u8; w * h * 4], w).unwrap();
            assert_eq!(bgr.len(), bytes);
            let b = blocks(&bgr);
            assert_eq!(b.len() / BLOCK_LEN, n_blocks);
            assert_eq!(b.len(), wire);
            assert!(b.len().is_multiple_of(USB_CHUNK), "no partial transfer");
            let chunks: Vec<[u8; USB_CHUNK]> = usb_writes(&b).collect();
            assert_eq!(chunks.len(), writes);
            assert_eq!(chunks.concat(), b);
        }
    }

    #[test]
    fn a_short_tail_transfer_is_zero_padded() {
        let buf = vec![7u8; USB_CHUNK + 10];
        let chunks: Vec<[u8; USB_CHUNK]> = usb_writes(&buf).collect();
        assert_eq!(chunks.len(), 2);
        assert!(chunks[1][..10].iter().all(|&b| b == 7));
        assert!(chunks[1][10..].iter().all(|&b| b == 0));
    }
}
