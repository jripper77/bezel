# Protocol: Turing USB (TUR_USB, VID 0x1CBE)

Reference: `library/lcd/lcd_comm_turing_usb.py` of turing-smart-screen-python at `2b33ab4` (the "Python view").
Confidence: **static** for behaviour; the DES vectors in section 9 are **verified** (pycryptodome, recomputed
independently). No capture of a real device exists yet.

Models: 2.8" round, 4.6", 5.2", 8", 8.8" HW rev 1.x, 9.2", 12.3" ([devices.md](devices.md)). The Python README
states "UART and USB protocols supported. Note: no video or storage support for now" (`README.md:22`), although the
module contains video and storage functions that its display class does not call.

## 1. Transport and discovery

- Not a serial port: raw USB via pyusb/libusb (`requirements.txt`: `pyusb~=1.3.1`, `pycryptodome~=3.23.0`).
- VID 0x1CBE; PID -> portrait resolution (`lcd_comm_turing_usb.py:44-51`):

| PID | Model | W x H |
|---|---|---|
| 0x0028 | 2.8" round | 480 x 480 |
| 0x0046 | 4.6" | 320 x 960 |
| 0x0050 | 5.2" (0x0052 before commit `cf0f1db`) | 720 x 1280 |
| 0x0080 | 8" | 800 x 1280 |
| 0x0088 | 8.8" V1.x | 480 x 1920 |
| 0x0092 | 9.2" | 462 x 1920 |
| 0x0123 | 12.3" | 720 x 1920 |

- `find_usb_device()` (`lcd_comm_turing_usb.py:462-497`): the first PID found in that order wins;
  `set_configuration()` (default configuration; failure only warned); on Linux `detach_kernel_driver(0)` if a kernel
  driver is bound to interface 0. Without libusb the program prints an error and exits (Windows needs
  `libusb-1.0.dll`).
- `write_to_device(dev, data, timeout=2000)` (`lcd_comm_turing_usb.py:515-538`): interface 0 of the active
  configuration; the first OUT and the first IN endpoint (addresses and transfer types come from the descriptor).
  One `ep_out.write(data, 2000 ms)`, then `ep_in.read(512, 2000 ms)` = the response, then a flush: up to 5 more
  `read(512, 100 ms)` until a timeout. Returns the first 512-byte response, or `None` on a write error.
- The Python code calls these "USB HID screens" (`lcd_comm.py:59`) and "(Win)USB devices"
  (`lcd_comm_turing_usb.py:934`); the actual interface class is not visible in the code. The vendor app enumerates
  them through WinUSB.

## 2. Command header and encryption

Plaintext header, 500 bytes (`build_command_packet_header`, `lcd_comm_turing_usb.py:435-442`):

```
[0]        command id
[1]        00
[2]        1a
[3]        6d
[4..7]     LE32 timestamp = milliseconds since local midnight
           int((time.time() - mktime(today 00:00:00 local)) * 1000)
[8..499]   command arguments (multi-byte sizes are BIG-endian), rest 00
```

Encryption (`encrypt_with_des`, `encrypt_command_packet`, `lcd_comm_turing_usb.py:445-459`):

- DES in **CBC** mode, key = ASCII `slv3tuzx` (`73 6c 76 33 74 75 7a 78`), **IV = key**;
- the 500-byte plaintext is zero-padded to a multiple of 8 (504 bytes); ciphertext 504 bytes;
- final 512-byte packet: `[0..503]` ciphertext, `[504..509]` = `00`, `[510]` = `a1` (161), `[511]` = `1a` (26).
- Bulk payloads (PNG, JPEG, H.264, file data) are appended **unencrypted** after the 512 bytes, in the same write.
- Responses are not decrypted: fields are read raw (for example `resp[8]`). Success test `_resp_ok`
  (`lcd_comm_turing_usb.py:67-72`): `resp[1] == 0xC8 or resp[8] == 0xC8`.
- Whether the device validates the timestamp is unknown.

## 3. Command catalogue

| ID | Name in the Python code | Header arguments | Payload after 512 B | Response use | Used by the display class |
|---|---|---|---|---|---|
| 10 | sync | - | - | ignored | `InitializeComm` |
| 11 | restart device | - | - | - | no (commented out in `Reset`) |
| 13 | (video preamble, unknown) | - | - | - | no |
| 14 | brightness | [8] = 0..102 | - | - | `SetBrightness` |
| 15 | frame rate | [8] = fps | - | - | no |
| 17 | get H.264 chunk size | - | - | `resp[8..11]` BE32, accepted if 1..1 MiB (default 202,752) | no |
| 38 | open remote file | [8..11] BE32 path length, [12..15] 0, [16..] ASCII path | - | truthiness | no |
| 39 | write file chunk | [8..11] BE32 capacity (1 MiB), [12..15] BE32 chunk length, [16] = 1 on the last chunk | chunk | `_resp_ok`; on failure retried with the legacy layout [8..11] = chunk length only | no |
| 40 | delete remote file | as 38 | - | - | no |
| 41 | (video preamble, unknown) | - | - | - | no |
| 98 | play file | as 38 | - | - | no |
| 100 | refresh storage | - | - | [8..11] total, [12..15] used, [16..19] free, **LE32**, printed as MB/GB (units unconfirmed) | no |
| 101 | upload JPEG frame | [8..11] BE32 size | JPEG | - | `DisplayPILImage` fallback |
| 102 | upload PNG frame | [8..11] BE32 size | PNG | - | `DisplayPILImage`, `Clear` |
| 110 | "alternate playback" | as 38 | - | - | no |
| 111, 112 | (video preamble, unknown) | - | - | - | no |
| 113 | "image playback" | as 38 | - | - | no |
| 121 | play H.264 chunk | [8..11] BE32 size, [12] = 1 on the last chunk | Annex-B H.264 | - | no |
| 122 | stream status ("delay") | - | - | [8] = queue depth | no |
| 123 | stop stream | - | - | - | no |
| 125 | save settings | [8] brightness, [9] startup mode, [10] reserved, [11] rotation, [12] sleep timeout, [13] offline mode | - | - | no |

## 4. Handshake, brightness, orientation, on/off

- `InitializeComm()`: command 10 (sync). A `delay_sync` helper (sync + 200 ms) exists but is unused.
- `SetBrightness(level)`: `int(level / 100 * 102)` -> command 14 with [8] = value (range 0..102).
- `SetOrientation(o)`: no device command. It recreates the host canvas (section 5) empty at the new size.
- `ScreenOff()` = `Clear()` + brightness 0; `ScreenOn()` = `SetBrightness(25)`. No real on/off command is known.
- `Reset()`: no-op (restart command 11 is disabled).

## 5. Frame path (the only path the Python app uses)

- The class keeps `current_state`, an RGBA canvas of the current-orientation size initialised to transparent black
  `(0, 0, 0, 0)` (`lcd_comm_turing_usb.py:942, 972`).
- `DisplayPILImage(image, x, y)` (`lcd_comm_turing_usb.py:974-1003`): clamp the image to the screen, then
  `current_state.paste(image, (x, y))` without a mask (RGBA images keep their alpha, RGB images become alpha 255),
  then rotate the **whole canvas** to the native orientation and send it:

| Library orientation | Transform before sending |
|---|---|
| REVERSE_PORTRAIT | none (native) |
| PORTRAIT | `transpose(ROTATE_180)` |
| LANDSCAPE | `transpose(ROTATE_270)` (90° clockwise) |
| REVERSE_LANDSCAPE | `transpose(ROTATE_90)` (90° counter-clockwise) |

- `send_pil_image_auto` (`lcd_comm_turing_usb.py:116-124`): encode PNG (RGBA, `compress_level=9`). If it is at most
  1 MiB (`MAX_CHUNK_BYTES = 1024 * 1024`; the comment says "1024MB") send command 102. Otherwise JPEG (converted to
  RGB, alpha dropped): quality starts at 90; subsampling tried in the order 2 (4:2:0), 1, 0; within each, quality
  decreases by 5 while above 10, then by 1; the first encoding of at most 1 MiB is sent with command 101. If none
  fits, a RuntimeError is raised.
- **Every widget update re-encodes and sends the entire frame.** There is no partial update on this protocol.
- `Clear()` sends a fixed 3,703-byte PNG (480 x 1920 RGBA, fully transparent, **for every model**) with command 102
  (`clear_image`, `lcd_comm_turing_usb.py:624-645`). Exact bytes:
  - `[0..107]`:
    `89504e470d0a1a0a0000000d49484452000001e000000780080600000016f084f5000000017352474200aece1ce90000000467414d410000b18f0bfc6105000000097048597300000ec300000ec301c76fa86400000e0c49444154785eedc1010d000000c2a0f74f6d0f0714`
  - `[108..3680]`: 3,573 x `00`
  - `[3681..3702]`: `f0664ac80001119d820a0000000049454e44ae426082`
- How the device treats PNG alpha (transparent canvas regions), and whether a PNG must match the panel resolution
  (Clear always sends 480 x 1920), is unknown.

## 6. Storage and file commands (`lcd_comm_turing_usb.py:753-930`)

- On-device paths used by `upload_file`: `/tmp/sdcard/mmcblk0p1/img/<name>.png` and
  `/tmp/sdcard/mmcblk0p1/video/<name>.h264`. Only `.png` and `.mp4` are accepted; MP4 is converted to Annex-B
  `.h264` first ([video.md](video.md)).
- Upload: command 38 (open remote file, ASCII path), then command 39 chunks of at most 1 MiB, each written together
  with its chunk data; `[16] = 1` marks the last chunk. If a response is missing or not `_resp_ok`, the same chunk is
  re-sent with the legacy header (`[8..11]` = chunk length only). Which layout current firmware expects is unknown.
- Delete: command 40. Play: 98 (play), 110 ("alternate playback"), 113 ("image playback"), all with the path layout.
- Storage query: command 100 (section 3).

## 7. Video streaming (`send_video`, `lcd_comm_turing_usb.py:683-744`)

1. MP4 -> Annex-B `.h264` next to the source (skipped if the file exists): ffmpeg if available, otherwise a built-in
   MP4 parser that writes SPS/PPS first and repeats them before every sync sample ([video.md](video.md)).
2. Preamble, each as a separate command: 111, 112, 13, brightness 14 with value 32, 41, Clear (102 with the
   transparent PNG), frame rate 15 with 25.
3. Command 17: chunk size = `resp[8..11]` BE32 if 1..1 MiB, else 202,752.
4. For each chunk of the file: command 121 with `[8..11]` = chunk length and `[12] = 1` on the last chunk, followed
   by the chunk data in the same write. Flow control:
   - if the write returned no response: `delay(2)`;
   - otherwise send command 122 once; if its `resp[8] > 3`: `delay(2)`;
   - `delay(t)`: sleep 50 ms, send 122, and repeat while `resp[8] > t`.
5. Loop the file if requested; finally command 123 (stop stream).

## 8. Timing, threading, quirks

- Every command waits for a 512-byte IN response (2 s timeout) plus up to 5 x 100 ms flush reads.
- The Python display class has **no update queue and no lock**: sensor threads call `DisplayPILImage` concurrently
  (concurrent `paste` and USB writes).
- The 1 MiB transfer limit forces the JPEG fallback for large, detailed frames. Full-frame PNG level 9 on every
  widget update is slow.
- 9.2": the PID table (462 x 1920) overrides the theme size, so 480-wide themes are cropped to 462.
- Open: the interface class and endpoint layout (no descriptor dump), whether responses are encrypted, the meaning of
  commands 13, 41, 111 and 112, storage units, the maximum frame rate.

## 9. Test vectors (verified)

DES-CBC, key = IV = `slv3tuzx`, timestamp fixed to 0x01020304 (so `[4..7]` = `04 03 02 01`):

```
cmd 10 (sync)
  plaintext[0..7]    : 0a 00 1a 6d 04 03 02 01, then 492 x 00 (+4 x 00 padding)
  packet[0..15]      : cc ff a6 fb 53 02 10 74 fd 8e c7 12 b3 92 c6 a6
  packet[496..511]   : ec 54 35 a2 83 9f c6 a3 00 00 00 00 00 00 a1 1a

cmd 14 (brightness), [8] = 40 (0x28)
  packet[0..15]      : d9 28 43 51 e5 6c ef 7f 51 32 87 53 23 2e 04 f6

cmd 102 (PNG), [8..11] = BE32 0x00000e77 (3,703 = the Clear PNG)
  packet[0..15]      : d5 08 e7 5b ad 17 59 cb a6 26 c1 75 90 6a 61 0e
```

## 10. Vendor-app observations (partial, static)

- The vendor app's device table flags the 0x1CBE devices as its "207" family and enumerates them through WinUSB
  (LibUsbDotNet), with the same enumerator as the WCH family.
- For background videos it produces raw Annex-B `.h264` with no B-frames, slightly darkened, optionally at 30 fps
  ([video.md](video.md) section 3).
- Its device page hides the internal-storage paths for this family; only the SD-card image and video paths remain.

## TURZX additions

_Pending: consolidated from the TURZX USB/HID analysis._
