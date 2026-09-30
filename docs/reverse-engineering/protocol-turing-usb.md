# Protocol: Turing USB (TUR_USB, VID 0x1CBE)

References: `library/lcd/lcd_comm_turing_usb.py` of turing-smart-screen-python at `2b33ab4` (**Python**) and the
"207" USB transport of TURZX V3.07 (**vendor**). Confidence: **static** for behaviour; the Python DES vectors in
section 9 are **verified**, the vendor (PKCS#7) vectors are **static, computed** (pycryptodome and OpenSSL agree).
No capture of a real device exists yet.

Models: Python knows 7 PIDs, the vendor 18 (section 1.1, [devices.md](devices.md) section 1). The Python README states
"UART and USB protocols supported. Note: no video or storage support for now" (`README.md:22`), although the module
contains video and storage functions that its display class does not call.

## 1. Transport and discovery

- Not a serial port: raw USB bulk transfers. Python uses pyusb/libusb (`requirements.txt`: `pyusb~=1.3.1`,
  `pycryptodome~=3.23.0`); the vendor uses WinUSB through LibUsbDotNet and ships no INF for 0x1CBE, so the firmware
  presumably provides the WinUSB binding itself (MS OS descriptors, **inferred**).
- Endpoints: the vendor hard-codes bulk OUT **0x01** and IN **0x81** and issues no control transfer, SetConfiguration
  or interface claim. Python uses interface 0 of the active configuration, the first OUT and the first IN endpoint,
  calls `set_configuration()` (failure only warned) and on Linux `detach_kernel_driver(0)` if a kernel driver is bound
  (`find_usb_device`, `lcd_comm_turing_usb.py:462-497`; the first PID found in table order wins). Without libusb
  Python prints an error and exits (Windows needs `libusb-1.0.dll`).
- One command = one bulk write of the 512-byte packet plus its optional payload, then one 512-byte read:
  - Python (`write_to_device`, `lcd_comm_turing_usb.py:515-538`): `ep_out.write(data, 2000 ms)`,
    `ep_in.read(512, 2000 ms)` = the response, then up to 5 more `read(512, 100 ms)` until a timeout. Returns the first
    response, or `None` on a write error.
  - Vendor: write (2000 ms), one 512-byte read (2000 ms), then a read flush. A read timeout leaves an all-zero
    response. One lock serialises every command/response pair, so frames, the video stream and UI commands
    interleave only at packet granularity.
- Vendor open: 250 ms, open the writer (3 tries, 250 ms apart), 250 ms, open the reader (5 tries); one re-open after
  250 ms when the device is not found. Its close override is a no-op.
- The Python code calls these "USB HID screens" (`lcd_comm.py:59`) and "(Win)USB devices"
  (`lcd_comm_turing_usb.py:934`); they are vendor-class bulk devices, not HID.

### 1.1 PIDs

| PID | Python model, W x H (`lcd_comm_turing_usb.py:44-51`) | Vendor key, native W x H | Notes |
|---|---|---|---|
| 0x0005 | - | `16inch`, 400 x 400 | |
| 0x0016 | - | `28_Sq`, 400 x 400 | |
| 0x0021 | - | `21inch`, 480 x 480 round | |
| 0x0028 | 2.8" round, 480 x 480 | `28inchR`, 480 x 480 round | |
| 0x0034 | - | `34inch`, 480 x 480 square | |
| 0x0035 | - | `35inch`, 640 x 480 | vendor swaps rotation values 0 and 2 |
| 0x0040 | - | `4inch`, 720 x 720 | |
| 0x0046 | 4.6", 320 x 960 | - | |
| 0x0050 | 5.2", 720 x 1280 (0x0052 before commit `cf0f1db`) | `5inch`, 720 x 1280 | desktop mode (section 10) |
| 0x0068 | - | `68inch`, 1080 x 2320 | 30 fps video when the version's hardware field is 1 |
| 0x0080 | 8", 800 x 1280 | `8inch`, 800 x 1280 | desktop mode |
| 0x0088 | 8.8" V1.x, 480 x 1920 | `88inch`, 480 x 1920 | desktop mode |
| 0x0092 | 9.2", 462 x 1920 | `92inch`, 480 x 1920 drawn, squeezed to **464** wide | real panel width unconfirmed |
| 0x0123 | 12.3", 720 x 1920 | - | |
| 0x0288 | - | `288inch`, 480 x 480 round | |
| 0xA062 | - | `62inch`, 368 x 960 | |
| 0xA065 | - | `65inch`, 720 x 1472 | |
| 0xA068 | - | `68inch`, 1080 x 2224 | |
| 0xB062 | - | `62inch`, 448 x 1280 | |
| 0xB065 | - | `65inch`, 720 x 1568 | |

The vendor table also lists 0x0062 (`62inch`, 368 x 960, a legacy "WinLcd" entry) but routes it to its serial
transport, where it cannot work. The resolution always comes from the table, never from the device.

## 2. Command header and encryption

Plaintext header, 500 bytes (Python `build_command_packet_header`, `lcd_comm_turing_usb.py:435-442`; vendor identical
except the timestamp):

```
[0]        command id
[1]        00
[2]        1a
[3]        6d
[4..7]     LE32 timestamp
[8..499]   command arguments (multi-byte integers BIG-endian unless noted), rest 00
```

Timestamp: Python sends milliseconds since local midnight, `int((time.time() - mktime(today 00:00:00 local)) * 1000)`.
The vendor sends the low 32 bits of "UTC now minus local midnight of the previous day" in milliseconds, i.e.
86,400,000 + ms since local midnight - UTC offset. Both work in practice, so the firmware does not check it
(**inferred**).

Encryption (`encrypt_with_des`, `encrypt_command_packet`, `lcd_comm_turing_usb.py:445-459`):

- DES in **CBC** mode, key = ASCII `slv3tuzx` (`73 6c 76 33 74 75 7a 78`), **IV = key**;
- padding to 504 bytes: Python appends zeros (`00 00 00 00`), the vendor uses PKCS#7 (`04 04 04 04`). CBC confines
  the difference to the last ciphertext block `[496..503]`, so the firmware ignores plaintext bytes 496..503
  (**inferred**; do not put arguments there);
- final 512-byte packet: `[0..503]` ciphertext, `[504..509]` = `00`, `[510]` = `a1`, `[511]` = `1a`;
- bulk payloads (PNG, JPEG, H.264, file data) follow **unencrypted** in the same write.

Responses are not encrypted. `resp[0]` echoes the command id; status 0xC8 (200) is at `resp[8]`, except command 113,
which the vendor checks at `resp[1]`. Python's `_resp_ok` (`lcd_comm_turing_usb.py:67-72`) accepts either:
`resp[1] == 0xC8 or resp[8] == 0xC8`.

## 3. Command catalogue

"Path args" = `[8..11]` BE32 path length, `[12..15]` 0, `[16..]` ASCII path. Class: **Q** query, **D** display
state, **P** persistent setting, **X** destructive or disruptive (section 11).

| ID | Vendor meaning | Python label | Header arguments | Payload | Response use | Vendor use | Class |
|---|---|---|---|---|---|---|---|
| 10 | sync / get version | sync | - | - | `[0]` = 0x0A; `[8..39]` UTF-8 version, NUL-trimmed | connect, reconnect | Q |
| 11 | reboot | restart device | - | - | - | device page; after firmware; after 150 | X |
| 12 | power off | - | - | - | - | never reaches a 0x1CBE panel | - |
| 13 | set rotation | "video preamble" | `[8]` = 0..3 (0 and 2 swapped for PID 0x0035) | - | - | theme start, after saving the settings | P (**inferred**) |
| 14 | brightness | brightness | `[8]` = 0..102 | - | - | slider; 0 on suspend and exit | D |
| 15 | video frame rate | frame rate | `[8]` = fps | - | - | video theme start | D |
| 17 | get H.264 chunk size | same | - | - | BE32 `[8..11]`; 0 -> 202,752 | before streaming | Q |
| 38 | create/open remote file | open remote file | path args | - | ok iff `[8]` = 0xC8 | uploads >= 102,400 B | X |
| 39 | write file chunk | same | `[8..11]` BE32 1,048,576; `[12..15]` BE32 bytes in this chunk; `[16]` = 1 on the last | vendor: always 1,048,576 B | non-null | uploads | X |
| 40 | write a small file in one shot | **delete remote file** | `[8..11]` BE32 path length; `[12..15]` BE32 data length; `[16..]` path | the file | non-null | uploads < 102,400 B, boot image | X |
| 41 | unknown | "video preamble" | `[8]` = 0 | - | - | theme start | D |
| 42 | delete remote file | - | path args | - | - | device page; factory test mode | X |
| 98 | get file size | **play file** | path args | - | LE32 `[8..11]` bytes | existence, upload check | Q |
| 99 | list directory | - | path args | - | 20 responses concatenated (section 6) | device page | Q, may create the directory |
| 100 | storage info | refresh storage | - | - | six LE32 values in KiB (section 6) | device page, video | Q |
| 101 | show JPEG | upload JPEG frame | `[8..11]` BE32 size | JPEG | non-null | frames without video | D |
| 102 | show PNG | upload PNG frame | `[8..11]` BE32 size | PNG | non-null | frames over a video, clear | D |
| 110 | play stored video (loops) | "alternate playback" | path args | - | ok iff `[8]` = 0xC8 | device page | D |
| 111 | stop local playback | "video preamble" | - | - | - | before playing | D |
| 112 | playback busy? | "video preamble" | - | - | busy iff `[8]` != 0 | polled after 111 | Q |
| 113 | show stored image | "image playback" | path args | - | ok iff **`[1]`** = 0xC8 | device page | D |
| 114 | unknown | - | - | - | - | 50 ms after 111 on the device page | D |
| 121 | H.264 stream chunk | play H.264 chunk | `[8..11]` BE32 length; `[12]` = 1 on the chunk that reaches EOF | Annex-B chunk | `[8]` = decoder queue depth | video themes | D |
| 122 | stream queue status | "delay" | - | - | `[8]` = queue depth | flow control | Q |
| 123 | stop stream | stop stream | - | - | - | end of a video theme | D |
| 125 | save device settings | save settings | `[8]` brightness **0..255**, `[9]` startMode, `[10]` 0, `[11]` rotation, `[12]` sleep delay, `[13]` offline mode | - | - | every theme start and exit | P |
| 150 | switch to Desktop mode | - | - | - | - | Desktop Mode page, then 11 | X |
| 246 | speed test (debug) | - | the header is built but never sent | 10 x 10 MiB of raw zeros | - | test window | X |
| 251 | AIO pump/fan control | - | `[8..17]` = `ff 0f a1 00 hi(a) lo(a) b c d` | - | - | test window | X |
| 252 | LED strip data | - | `[12..15]` BE32 payload length | `hi(n) lo(n) m data`, encoded by a library the install does not ship | - | test window | X |

IDs 201 and 253 are in the vendor's list but never sent.

**Conflict between the references**: the vendor uses 40 as a one-shot write and 98 as "get file size"; Python labels
40 "delete" and 98 "play file". One of them is wrong or the firmwares differ. A mislabelled 40 overwrites a file:
never send 40 or 98 until a capture settles it.

## 4. Handshake, version, brightness, rotation, settings

- Sync: Python `InitializeComm()` sends command 10 and ignores the answer (a `delay_sync` helper, sync + 200 ms,
  exists but is unused).
- Vendor version query: up to 2 attempts (10 for one OEM build); each sends command 10 and requires `resp[0]` = 0x0A,
  then reads `[8..39]` as the version string; otherwise it reconnects and waits 200 ms. The string is parsed as
  `<...>_<hw>_<build>` (**inferred** from the parser; no real sample). A 1cbe:0068 with `hw` = 1 gets 30 fps video.
- Vendor reconnect: command 10; if there is no echo, close, 100 ms, re-open (without re-sending 10).
- Brightness: Python `int(level / 100 * 102)`; vendor `(byte)(b / 2.5)` for the 0..255 setting (truncating). Range
  0..102 on the wire.
- Rotation: the vendor sends command 13 at theme start, after saving its settings, and also rotates the frame on the
  PC; Python rotates in software only (`SetOrientation(o)` recreates the host canvas empty at the new size).
- Settings: command 125 stores brightness (raw 0..255, unlike command 14), startMode, rotation, sleep delay and an
  offline mode on the device. The vendor sends it at every theme start and first at exit.
- On/off: Python `ScreenOff()` = `Clear()` + brightness 0, `ScreenOn()` = `SetBrightness(25)`, `Reset()` is a no-op
  (command 11 commented out). Vendor exit: 100 ms, command 125, command 14 with 0.

## 5. Frame path

Every update is a complete encoded image at the panel's native size. There is no partial update, no RGB565 and no raw
frame on this protocol.

### 5.1 Python

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
- **Every widget update re-encodes and sends the entire frame.**
- `Clear()` sends a fixed 3,703-byte PNG (480 x 1920 RGBA, fully transparent, **for every model**) with command 102
  (`clear_image`, `lcd_comm_turing_usb.py:624-645`). Exact bytes:
  - `[0..107]`:
    `89504e470d0a1a0a0000000d49484452000001e000000780080600000016f084f5000000017352474200aece1ce90000000467414d410000b18f0bfc6105000000097048597300000ec300000ec301c76fa86400000e0c49444154785eedc1010d000000c2a0f74f6d0f0714`
  - `[108..3680]`: 3,573 x `00`
  - `[3681..3702]`: `f0664ac80001119d820a0000000049454e44ae426082`

### 5.2 Vendor

```
theme start:  version query (command 10); firmware check (section 6); command 111, then command 112 polls until idle;
              command 13 <rotation>; command 14 <brightness>; command 125 <settings>
              (the start routine shared with rev C, protocol-turing-rev-c.md section 7.2)
              command 41 [8]=0 | command 102 with a fully transparent PNG at the panel's own size ("clear")
              [video theme] section 7
every second: compose the theme at the panel's native size
              [9.2"] stretch the 480-wide canvas to 464
              encode: JPEG quality 95, or PNG (keeps alpha) while a device-side video plays
              larger than 1,048,576 bytes -> the frame is dropped
              command 101 (JPEG) or 102 (PNG)
              failure -> reconnect; more than 3 failures in a row -> the theme stops
```

Transparent PNG pixels show the device-side video underneath. Whether a PNG must match the panel resolution is not
tested (Python's `Clear` always sends 480 x 1920).

## 6. Storage and file commands

| Location | Vendor path | Python |
|---|---|---|
| Internal images / videos | `/usr/data/img/`, `/usr/data/video/` | - |
| TF card images / videos | `/tmp/sdcard/mmcblk0p1/img/`, `/tmp/sdcard/mmcblk0p1/video/` | `<name>.png`, `<name>.h264` there |
| RAM | `/tmp/video/` | - |
| Settings file | `/usr/data/app.cfg` (deleted only in the vendor's factory test mode) | - |
| Boot logo | `/usr/data/boot.jpg`: JPEG quality 95, at most 307,200 bytes, panel size | - |
| Firmware image | `/usr/data/update.app` | - |

The vendor's device page lists only the two TF-card locations for this family.

- Upload, vendor: files under 102,400 bytes go in one command 40; larger ones use command 38 (requires
  `resp[8]` = 0xC8) then command 39 chunks of 1 MiB whose payload is always 1,048,576 bytes (the tail of a short last
  chunk holds stale data; `[12..15]` gives the real length). The result is checked with command 98; names are
  lower-cased.
- Upload, Python (`lcd_comm_turing_usb.py:753-930`): only `.png` and `.mp4` (converted to `.h264` first,
  [video.md](video.md)); command 38 then command 39 chunks of at most 1 MiB with exact lengths. If a response is
  missing or not `_resp_ok`, the chunk is re-sent with the legacy layout (`[8..11]` = chunk length only).
- Storage info, command 100: six **LE32 values in KiB**. `[8..11]` TF total (0 = no card), `[12..15]` TF used,
  `[16..19]` TF free, `[20..23]` internal total, `[24..27]` internal used, `[28..31]` internal free. The vendor uses
  TF total and both free values; the two "used" fields and internal total are **inferred** by analogy. Python decodes
  the first three as total / used / free and prints them as MB/GB.
- List, command 99: the vendor sends it 20 times and decodes the twenty 512-byte responses as one UTF-8 text; the
  names follow `file:` separated by `/`; `nodir-createdone` means the firmware created the empty directory. Whether
  each response carries header bytes is unknown.
- Play: the vendor first stops and waits (command 111, then up to 10 x {command 112; done when `resp[8]` = 0;
  100 ms}), clears with the transparent PNG, then sends 110 (video, loops) or 113 (image). Its device page sends 111,
  50 ms, 114 before playing.
- Firmware (vendor, destructive; checked at every theme start): a file in the app's firmware folder whose name contains `turzx`, the model key as the
  second `_` field and a build number last; if the build is newer than the version's, it is uploaded to
  `/usr/data/update.app` and command 11 reboots the panel. The folder ships empty.

## 7. Video streaming

Python `send_video` (`lcd_comm_turing_usb.py:683-744`), not used by its display class:

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

Vendor, for a theme with a video:

```
transcode to "<source>.h264" with ffmpeg if needed (video.md section 3); probe the frame rate if the theme has none
command 15 [8]=fps
command 17 -> chunk size (0 -> 202,752)
loop: read one chunk; at EOF rewind, and stop unless the theme loops
      command 121 [8..11]=BE32 length, [12]=1 when the chunk reaches EOF, chunk appended
      no response -> reconnect and resend the same chunk (3 attempts)
      30 ms
      resp[8] > 3 -> repeat { 50 ms; command 122 } while resp[8] > 2
end of theme: command 123
```

Meanwhile the 1 Hz loop of section 5.2 sends PNG overlays. A single theme loops forever; in a carousel the theme
advances after the video's length (10 s when unknown).

## 8. Timing, threading, quirks

- Every command waits for a 512-byte response (2 s timeout); Python adds up to 5 x 100 ms flush reads.
- The Python display class has **no update queue and no lock**: sensor threads call `DisplayPILImage` concurrently
  (concurrent `paste` and USB writes). The vendor serialises every command/response pair.
- The 1 MiB transfer limit forces Python's JPEG fallback for large, detailed frames, and makes the vendor drop them.
  Full-frame PNG level 9 on every widget update is slow.
- 9.2": Python crops 480-wide themes to 462; the vendor squeezes them to 464.
- The vendor has no keep-alive; the 1 Hz frames are the only traffic.

## 9. Test vectors

DES-CBC, key = IV = `slv3tuzx`, timestamp fixed to 0x01020304 (so `[4..7]` = `04 03 02 01`). Python (zero padding),
**verified**:

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

Last two ciphertext blocks, same plaintexts. `[488..495]` is common to both paddings; `[496..503]` differs
(**static, computed**; the zero-padding column matches the verified vectors):

| Command | `[488..495]` | `[496..503]` vendor (PKCS#7) | `[496..503]` Python (zeros) |
|---|---|---|---|
| 10 | `fc 10 4a ba c4 ef fc cf` | `78 1a 5f 0c 81 c0 55 98` | `ec 54 35 a2 83 9f c6 a3` |
| 14, `[8]` = 40 | `89 58 dc e7 44 4d d6 ee` | `29 12 16 b3 d7 06 3c c9` | `f5 37 4c fb 24 39 6b 48` |
| 102, `[8..11]` = `00 00 0e 77` | `f7 7f b8 08 75 b0 76 be` | `73 6b 15 83 c8 79 fa e5` | `1f b0 72 fd bc 6b 3d 67` |

Every packet ends with `00 00 00 00 00 00 a1 1a`.

## 10. Desktop mode and the HID companion (1A86:AD11)

The vendor can turn the 8" (0x0080), 8.8" (0x0088) and 5.2" (0x0050) into a Windows monitor:

- **Enter** (disruptive): command 150, then command 11. The panel reboots and re-enumerates as **1a86:ad11**, a
  composite device: interface `MI_00` is a vendor USB-display interface driven by a Windows indirect-display driver,
  and a HID interface serves the two commands below. While in this mode it is no longer a TUR_USB device.
- **HID reports**: 64 bytes, report id 0 (65 bytes with the id on the wire).

| Name | Host -> device | Device -> host |
|---|---|---|
| Model query | `aa 55 33` + 61 x `00` | one report within 1000 ms; its third payload byte (index 2 without the report id, as hidapi returns it) is the model: `80` 8", `88` 8.8", `50` 5.2" (the low byte of the TUR_USB PID). No answer: the vendor assumes 8.8" |
| Back to monitor mode | two reports: `35 66 33 37 35 39 64 66` (ASCII `5f3759df`), then `00 35 66 33 37 35 39 64 66`, each zero-padded to 64 bytes | none |

- The HID device has no brightness command, no frames and no storage; the vendor runs no theme on it.
- The driver's INF matches `USB\VID_1A86&PID_AD10`, `USB\VID_1A86&PID_AD11&MI_00`, `USB\VID_1A86&PID_AD12&MI_00` and
  `USB\VID_1A86&PID_AD13&MI_00`. It is an IddCx (UMDF 2) driver, attestation-signed for Windows 10/11 x64, that
  identifies itself as a generic "USB display for embedded" driver from Ingenic (so the 0x1CBE panels are probably
  Ingenic-based, **inferred**). From its strings: it reads a vendor-specific capability descriptor (minimum and
  maximum size, maximum transfer, supported formats, resolution list) and an EDID through control transfers, and
  sends each frame as two bulk messages encoded as H.264 (FFmpeg with x264 or GPU encoders), MJPEG or an "RLX" format.
  The wire format was not reversed.

Bezel lists a panel in desktop mode (1a86:ad10-ad13, `bezel devices`: `desktop-mode`, "not validated on hardware")
and offers only the switch back to USB monitor mode (`bezel monitor-mode --yes`, the studio behind a confirmation):
the model query `aa 55 33` then the two `5f3759df` reports above, report id 0, 64 bytes. Nothing is sent to the HID
device without that confirmation, listing is read-only, and command 150 (entering desktop mode) is never sent. Built
from these bytes only: no one has run it on such a panel yet (D-2026-09-30-release-polish-8).

## 11. Disruptive commands and Bezel's policy

Decision `D-2026-09-30-device-protocols-2` limits automatic traffic to what the vendor apps send on every start and
stop, and names reboot, persistent settings and rotation, storage writes and firmware as never sent implicitly. Its
text uses the rev C opcodes; the TUR_USB counterparts below are this spec's mapping. They need an explicit command,
and a destructive one needs `Confirm::Yes`:

| Command | Effect | Kind |
|---|---|---|
| 11 reboot | reboots the panel; it re-enumerates | disruptive |
| 150 desktop mode | changes the USB identity until switched back over HID | disruptive |
| 38 / 39 / 40 writes, boot logo | write or overwrite device files | destructive (storage write) |
| 42 delete | removes a device file | destructive |
| firmware (`/usr/data/update.app` + 11) | replaces the firmware | destructive |
| 125 save settings | persistent brightness, start mode, rotation, sleep, offline mode | persistent |
| 13 rotation | rotation setting | persistent (**inferred**) |
| 110, 113 | device-side playback of stored media | explicit only |
| 41, 114, 12 | unknown semantics | not sent implicitly |
| 246, 251, 252 | debug speed test, pump/fan and LED control | out of scope |

Commands that fit the automatic set: 10, 14, 101, 102 (frames and the clear PNG), 111 / 112 / 123 (stop and wait) and
the stream of a video theme the user started (15, 17, 121, 122).

## 12. Open questions

1. The 512-byte response layout beyond the bytes read by the apps; why command 113 reports status at `[1]`.
2. The real version string of command 10 (`..._<hw>_<build>` is inferred).
3. Command 99: headers inside each response and the end-of-list signal.
4. Commands 40 and 98 (section 3 conflict); semantics of 12, 41 and 114; whether 13 persists.
5. The brightness scale that command 125 stores (raw 0..255) versus command 14 (0..102).
6. The timestamp: presumably ignored.
7. Whether arguments beyond plaintext offset 495 survive (the paddings differ there).
8. The HID report descriptor and interface number of 1a86:ad11; why the switch-back sends two reports.
9. The desktop-mode display protocol (capability descriptor, EDID request, bulk framing).
10. The real diagonals of `16inch` (0x0005), `28_Sq` (0x0016) and `288inch` (0x0288), and the true width of the 9.2".
11. The interface class and descriptors of a real 0x1CBE panel (no `lsusb` dump yet), storage units, the maximum
    frame rate.
