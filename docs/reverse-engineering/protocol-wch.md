# Protocol: WCH WinUSB panels (VID 0x43A8)

Source: interoperability analysis of the vendor application TURZX V3.07. The Python reference does not support this
family (no VID 0x43A8, no `aa 55 .. bb` framing, no DES-ECB key).
Confidence: **static** for all behaviour. The encrypted packets in section 10 are **static, computed** (DES applied
offline with the documented key; independently recomputed). No capture of a real device exists.

## 1. Devices

| USB id | Vendor-table name | Resolution (W x H) | Portrait flag | Panel type in the 0x40 reply |
|---|---|---|---|---|
| 43a8:0e61 | "3.38 inch" | 180 x 640 | yes | 4 -> `338_Rect` |
| 43a8:0e6d | "4.3 inch" | 480 x 272 | no | none maps to it |
| 43a8:0e64 | "G-GEAR aio" (OEM) | 320 x 320 | no | 6 -> `28_Sq` |
| 43a8:0e5e | (none) | 240 x 320 | yes | 8 -> `24_Rect`, 5 -> `28_Rect` |

- The resolution always comes from this table; the device never reports it.
- After the 0x40 query the vendor app replaces the device's display name with the model string from the reply. For
  an unknown panel type, or when the query fails, the name becomes empty. That is always the case for the 4.3"
  (latent bug). The model string also drives OEM defaults (for example `28_Sq` selects an OEM name and default theme).

## 2. Transport

- USB bulk through WinUSB (LibUsbDotNet on Windows). The device is matched by a case-insensitive substring test of
  `vid_43a8&pid_0e..` in the WinUSB device path; the path's third `#`-separated component (the iSerial, or the Windows
  instance id when the device has none) is kept as the device key and used to re-open it.
- Endpoints: **OUT 0x02** (bulk) and **IN 0x82**. The reader is opened as "interrupt" with a 32-byte buffer; with the
  WinUSB backend the transfer type comes from the descriptor. No SetConfiguration, no ClaimInterface, no control
  transfers, no pipe policies.
- Open: retried up to 4 more times with 250 ms sleeps when the device is not found.
- Linux (Bezel): libusb, claim interface 0 (detach a kernel driver if bound), bulk OUT 0x02; read IN 0x82 with the
  transfer type the descriptor declares.

## 3. Crypto

- Key: `41 5f d9 fa 13 42 58 b7` (8 raw bytes, not ASCII). DES ignores the parity bit of each byte.
- Commands: DES-**ECB** encryption of the 8-byte block `[cmd, arg, 00, 00, 00, 00, 00, 00]`. (The vendor code uses
  PKCS7 padding, which yields 16 bytes for an 8-byte input; only the first 8 are used, so the result is plain
  single-block DES-ECB.)
- Responses: bytes `[2..9]` of each 32-byte response are DES-ECB-decrypted (no padding) in place; bytes 0-1 and
  10-31 are raw.
- Frame data is **not** encrypted.

## 4. Command packet (host to device, 32 bytes)

```
[0]       aa
[1]       55
[2..9]    DES_ECB_Encrypt(key, [cmd, arg, 00, 00, 00, 00, 00, 00])
[10]      01            packet type: command
[11..30]  00
[31]      bb
```

Written as one 32-byte bulk OUT transfer (not through the 4096-byte chunker). Then: sleep 1 ms; discard pending IN
data; read 32 bytes with a 100 ms timeout; decrypt `[2..9]`. The read length and error code are ignored: on a timeout
the buffer is all zeros.

Response layout (**inferred** from the parsers): `[0..1]` probably `aa 55`; `[2..9]` encrypted, plaintext `[0]` is
a status or echo code; presumably `[31] = bb`.

## 5. Command table

The opcode set of the firmware enum is {2, 3, 4, 5, 6, 7, 8, 9, 56, 64, 65, 84, 85, 86, 87, 88, 112}. Two names
survive in log strings: StopVideo and ResetMem.

| Opcode | Name | Arg | Sent when | Response use |
|---|---|---|---|---|
| 0x38 (56) | StopVideo | 0 | right after the device is added; when the monitor loop stops | ignored |
| 0x40 (64) | GetVersion / model query | 0 | on open and on init | parsed (section 6) |
| 0x41 (65) | reply code of 0x40 (plaintext `resp[2]` must be 0x41) | - | never sent | - |
| 0x54 (84) | SetBrightness | 0..255 (UI slider 1..255, default 170); **0 = panel off** | brightness changes; shutdown and system sleep send 0; resume sends the saved value | ignored |
| 0x56 (86) | SetRotation | rotation index 0..3 | after the device configuration is saved | ignored |
| 0x58 (88) | ResetMem | 0 | at theme start; automatically when a frame ACK has status 0x60 | ignored |
| 0x70 (112) | factory/test mode (unnamed) | 0 | only during init when the app runs with its `-test` switch | ignored |
| 2..9, 0x55 (85), 0x57 (87) | unknown (85/87 might be "get brightness"/"get rotation"; unverified) | - | never sent | - |

## 6. Model and firmware query (0x40)

```
resp = command(0x40, 0)                   // resp[2..9] decrypted
if resp is missing or resp[2] != 0x41  -> failure
switch resp[5]: 5 -> "28_Rect", 6 -> "28_Sq", 8 -> "24_Rect", 4 -> "338_Rect", other -> unchanged
firmware = decimal(resp[3]) + "." + decimal(resp[4])
```

Decrypted block at `resp[2..9]`: `[0x41, fwMajor, fwMinor, panelType, ?, ?, ?, ?]`. The vendor app uses the
firmware string only as a success indicator.

## 7. Frame path

### 7.1 Pixel encoding

Raw **BGR888**: GDI+ `Format24bppRgb` locked bits copied as-is, rows **top to bottom**, bytes `B, G, R` per pixel,
stride `(w * 3 + 3) & ~3`. For every WCH resolution `w * 3` is a multiple of 4, so there is no row padding. No
header, no compression. The frame is always the full panel (`W * H * 3` bytes).

### 7.2 Block framing (480 data bytes per 512-byte block)

```
nBlocks = ceil(len / 480)
buf     = nBlocks * 512 zero bytes
for i in 0 .. nBlocks-1:
    block[0]      = aa
    block[1]      = 55
    block[2..9]   = 00                    (NOT encrypted)
    block[10]     = 02                    packet type: data
    block[11..12] = BE16(i + 1)           1-based block sequence
    block[13..30] = 00
    block[31]     = bb
    block[32..511]= data[i*480 ..]        480 bytes; the last block is zero-padded
write buf in 4096-byte transfers (section 7.3)
if the writes succeeded:
    sleep 1 ms; read the 32-byte ACK (100 ms timeout); decrypt [2..9]
    if ACK plaintext resp[2] == 0x60:     device buffer problem
        send ResetMem (0x58); sleep 300 ms
```

The header carries no total length, frame id, rectangle or checksum: the device must infer the frame end from its
fixed resolution. **There are no partial updates on this transport.**

First block header: `aa 55 00 00 00 00 00 00 00 00 02 00 01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 bb`
followed by 480 pixel bytes.

### 7.3 USB write chunking

The block buffer is copied slice by slice into **one reused 4096-byte array**, and the full 4096 bytes are always
written, even for the last slice (if `len % 4096 != 0` the tail would carry stale bytes of the previous slice; for all
four panels `nBlocks` is a multiple of 8, so this never happens). The first failing write aborts the frame.
Each transfer: 2000 ms timeout; on error the device is closed, re-opened by its key, and the transfer retried once.

| Panel | W x H | Bytes per frame (BGR) | 512-byte blocks | Bytes on the wire | 4096-byte writes |
|---|---|---|---|---|---|
| 3.38" (0e61) | 180 x 640 | 345,600 | 720 | 368,640 | 90 |
| 4.3" (0e6d) | 480 x 272 | 391,680 | 816 | 417,792 | 102 |
| 320 x 320 (0e64) | 320 x 320 | 307,200 | 640 | 327,680 | 80 |
| 2.4"/2.8" (0e5e) | 240 x 320 | 230,400 | 480 | 245,760 | 60 |

## 8. Operational flows

- **Open / init**: open, then 0x40; if it fails, sleep 200 ms, reconnect (close and re-open by key), 0x40 again. In
  test mode, send 0x70 after a successful first query.
- **Device added**: StopVideo 0x38 immediately.
- **Theme start**: ResetMem 0x58, sleep 200 ms.
  - Without a background video: render the theme, compose the frame (rotation applied on the PC), send it as a full
    BGR frame, sleep 1000 ms, repeat: **1 frame per second**. With a theme playlist (loop or shuffle) each theme runs
    for 10 iterations (~10 s). Exceptions are logged and the loop continues.
  - With a background video: the PC decodes the video and streams it (section 9).
- **Direct image**: a caller-supplied W x H image is sent as one BGR frame (no scaling).
- **Video file check**: accepted only if the video's frame size equals the panel's W x H.
- **Shutdown**: brightness 0x54 with 0 (panel off). **System sleep**: 0x54 with 0; **resume**: 0x54 with the saved
  value. **Monitor stop**: StopVideo 0x38.
- **Not supported**: storage, TF card, upload, file listing and deletion, play-from-storage, boot image or video,
  firmware update, time sync, reboot. (A PC-side `.avt` frame-cache format, a .NET-serialized frame dictionary,
  exists only as dead code: nothing writes or plays it.)
- **Rotation**: the vendor app both rotates frames on the PC **and** sends 0x56. Whether the firmware applies 0x56
  itself is unknown; verify on hardware before rotating on both sides.

Recommended startup order (as the vendor app): open, 0x40 (check 0x41, read firmware and panel type), 0x54
brightness, 0x56 rotation, 0x58 ResetMem, 200 ms, frames. On stop: 0x38, then 0x54 with 0.

## 9. Video (PC-side decode)

Video is always **streamed from the PC**; there is no on-device storage.

- Decoding with FFmpeg on the PC to BGR24, at a size rounded **up** to multiples of 8 (`ceil(W/8)*8 x ceil(H/8)*8`);
  each frame is used as a W x H top-left crop of that buffer.
- Pacing: `interval = 1000 / fps` ms; after each frame sleep `max(0, interval - elapsed)`, where `elapsed` is taken
  from the millisecond *component* of a TimeSpan rather than the total (a bug that only matters above 1 s per frame).
- Per frame: every `fps` frames the theme overlay is re-rendered (about once per second); the video frame is rotated
  by the rotation setting (90 / 180 / 270), drawn onto a W x H 24-bit canvas with nearest-neighbour interpolation and
  no smoothing, the theme overlay is drawn on top with alpha, and the result is sent as one BGR full frame. On a send
  error: reconnect and resend once.
- A single-theme setup loops the video forever; with a playlist the video plays once so the playlist can advance.
- Throughput: 240 x 320 at 30 fps needs about 7.4 MB/s.

## 10. Test vectors (static, computed)

Command packets (bytes `[11..30]` are `00`):

```
StopVideo     0x38, 0   : aa 55 76 76 23 ac 62 ee 39 19 01 + 20 x 00 + bb
GetVersion    0x40, 0   : aa 55 22 61 7a 4a eb 54 98 89 01 + 20 x 00 + bb
SetBrightness 0x54, 170 : aa 55 ae 57 54 59 1c 6e 51 92 01 + 20 x 00 + bb
SetBrightness 0x54, 0   : aa 55 8b 88 d1 14 03 85 c9 c9 01 + 20 x 00 + bb
SetBrightness 0x54, 255 : aa 55 80 4a 57 c4 66 80 60 b9 01 + 20 x 00 + bb
SetRotation   0x56, 0   : aa 55 8e c3 bc b8 51 cb c7 d9 01 + 20 x 00 + bb
SetRotation   0x56, 1   : aa 55 00 8b 58 04 9e 3b 20 4d 01 + 20 x 00 + bb
SetRotation   0x56, 2   : aa 55 cf f8 c4 44 7a 11 90 21 01 + 20 x 00 + bb
SetRotation   0x56, 3   : aa 55 24 b3 2c 24 d2 18 ef e9 01 + 20 x 00 + bb
ResetMem      0x58, 0   : aa 55 3a 81 75 42 54 4d db 3f 01 + 20 x 00 + bb
TestMode      0x70, 0   : aa 55 91 0b c6 95 c8 43 5c 3b 01 + 20 x 00 + bb
```

Data block framing: see section 7.2 (first block header) and the size table in section 7.3.

## 11. Timing, locking, recovery

- Bulk write timeout 2000 ms per 4096-byte transfer; read timeout 100 ms; 1 ms between write and read; 300 ms after
  an automatic ResetMem; 200 ms after the theme-start ResetMem; 200 ms before the init retry; open retries 4 x 250 ms.
- One lock guards a whole frame (blocks plus ACK read) and each transfer. A command's reply is read **outside** the
  lock, so a brightness change during video streaming can read a frame ACK, or lose its own reply to the frame path's
  input flush. Replies are ignored anyway (except 0x40). Bezel should serialise commands and frames on one queue.
- Recovery: a failed transfer causes close, re-open and one retry; a failed video frame causes a reconnect and one
  resend; a failed init retries 0x40 once after a reconnect; an ACK status 0x60 triggers ResetMem.
- The vendor app's exception paths re-open every 300 ms and re-close every 100 ms without a bound (its retry counter
  is passed unchanged); Bezel must bound these loops.
- No keep-alive or heartbeat exists.

## 12. Open questions

1. `lsusb -v` of a 43a8 device: interface number, the transfer type and wMaxPacketSize of IN 0x82, MS OS 2.0
   descriptors.
2. Meaning of opcodes 2..9, 85, 87, of ACK status 0x60, and of plaintext response bytes beyond `resp[5]`.
3. Whether the firmware accepts a partial final 4096-byte transfer (irrelevant for the known resolutions).
4. Whether 0x56 rotates in firmware.
