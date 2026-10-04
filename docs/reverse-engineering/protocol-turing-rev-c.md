# Protocol: Turing rev C (serial SoC generation)

References: `library/lcd/lcd_comm_rev_c.py` of turing-smart-screen-python at `2b33ab4` (**Python**), the serial
transport of TURZX V3.07 (**vendor**), and the project's own tests on one Turing 8.8" (section 19).
Confidence: **static** unless tagged; **verified** for the Python vectors in section 17.1; **hardware** in section 19;
**inferred** where stated.

A rev C screen pairs a small MCU that enumerates while the screen sleeps with a Linux (sunxi) or Android SoC that
enumerates as a CDC-ACM gadget when awake. All protocol traffic goes to the SoC port. Models, USB ids, the MCU/SoC
pairing and the wake procedure are in [devices.md](devices.md) sections 1 and 5.

Python drives the 2.1"/2.8" round, 5" and 8.8". The vendor drives every serial SoC model of its table with this
transport (2.1"/2.8" round, 2.4", 2.8" square, 3.4", 4", 5", 6.5", 6.8", 8", 8.8") and splits them in two classes:

| Vendor class | Models | Behaviour that depends on it |
|---|---|---|
| large | 4", 6.5", 6.8", 8", 8.8" (its large-screen list also names a 6.2" and an 11.3") | storage under `/mnt/UDISK`; raw BGRA partials when ROM >= 1.89; no POSLEN overlay mode; no 0x82 before video play; 15 upload-completion rounds; ROTATION 0x81 sent at theme start |
| small | 2.1"/2.8" round, 2.4", 2.8" square, 3.4", 5" | storage under `/root`; compressed 3-byte partials; POSLEN overlay mode with a device-side video; 0x82 and a re-init before video play |

Per-model switches: the 6.8" skips the per-tick QUERY_STATUS; the 5" never deletes videos to make room (section 13.6).

## 1. Transport

- Awake SoC (display endpoint): 0525:a4a7 (8.8"), 1d6b:0121 (2.1"/2.8"), 1d6b:0106 (5"), or any port with iSerial
  `20080411` (Python). The vendor table adds 1d6b:0124 and 125f:7903 (2.4"), 1d6b:0127 (2.8" square), 1d6b:0134 (3.4"),
  1d6b:a040 (4"), 1d6b:a065 (6.5"), 1d6b:a068 (6.8") and 1d6b:a080 (8").
- CDC-ACM with IAD; bulk IN 0x81 and OUT 0x01 with 512-byte packets (high speed) on the dumped 2.1", 5" and 8.8".
  The other models have no dump (same layout **inferred**).
- Line settings: Python opens 115200 8N1 with `rtscts=True` and a 1 s read timeout. The vendor opens 115200 8N1 with
  DTR and RTS asserted and no flow control, write timeout 5 s. On a CDC-ACM gadget these are only SET_LINE_CODING
  and SET_CONTROL_LINE_STATE values; both work.
- No checksums. Every reply is ASCII text without terminator or length. Python discards replies (its reads only pace
  the stream). The vendor parses them from a single read of up to 1024 bytes (10,240 for LIST_DIR) with NUL bytes
  removed; its timed-out reads are not cancelled and swallow the next reply.
- The model and the resolution never come from the device. Python takes them from the theme ("because of issue with
  Turing rev. C size auto-detection", `library/display.py:97-100`); the vendor takes them from its VID:PID table and
  then checks the HELLO answer (section 4).

Bezel reads with a real timeout, accumulates until the line is quiet, and drains stale input before each request.

## 2. Framing

### 2.1 Command packet (static; supersedes the earlier inferred layout)

```
[0]        opcode
[1..2]     ef 69
[3..6]     BE32 length: inline payload length, 1 for commands without argument,
           or the byte size of the data phase that follows (frames, uploads)
[7]        flag byte: 00, except the loop flag of PLAY_VIDEO (0x78)
[8..9]     00 00
[10..249]  inline payload (at most 240 bytes), then 00 up to 250 bytes
```

The vendor writes every command as exactly 250 bytes in one `write()`. Python's `[opcode] ef 69 [BE32 length] 00 00 00
[payload]` is the same structure with byte 7 always 0; for example SET_BRIGHTNESS has length 1 and the level at byte 10,
OPTIONS has length 5 with bytes 10..14.

### 2.2 Data phase

`len` bytes are cut into 249-byte pieces, each followed by one `00` byte; the last block is zero-padded to 250 bytes.
That is `ceil(len / 249)` blocks of 250 bytes (`249k` bytes become `250k`). The device drops the separators using the
length from the header. The vendor issues one `write()` per block, Python one `write()` for everything; the bytes are
the same except for Python quirk 18.4.

### 2.3 START_DISPLAY_BITMAP block

250 x `2c`. Not a header. Sent before every full frame by both apps, and by the vendor between failed HELLO attempts.

### 2.4 Python `_send_command` (`lcd_comm_rev_c.py:183-213`)

1. message = command bytes (omitted for SEND_PAYLOAD) + payload;
2. if `len(message) % 250 != 0`, pad to the next multiple of 250 with the padding byte (0x00, or 0x2C for
   START_DISPLAY_BITMAP);
3. write the whole message in **one** `write()` call; if `readsize`, then `read(readsize)` (content ignored).

Commands are queued unless `bypass_queue`. A code comment gives the protocol order (`lcd_comm_rev_c.py:43-63`):
"READ HELLO ALWAYS IS 23. ALL READS IS 1024".

### 2.5 Atomicity

Python runs full-frame and partial sequences under its queue mutex, except `SetBrightness`, which writes from the
caller's thread. The vendor locks each write separately, so a UI command (brightness, for example) can land between
a header and its data phase. Bezel sends header, data phase and the expected reply as one transaction.

## 3. Command table

Class: **Q** query, **D** display state (reversible), **P** persistent device setting, **X** destructive or
disruptive (section 16). `<path>` is ASCII at byte 10 and `n` its length in bytes 3..6, so paths are at most 240
bytes (236 for 0x6F). Every packet is zero-padded to 250 bytes.

| Op | Name | Packet before padding | Reply (vendor wait) | Python | Vendor use | Class |
|---|---|---|---|---|---|---|
| 0x01 | HELLO | `01 ef 69 00 00 00 01 00 00 00 c5 d3` | device id, e.g. `chs_88inch.dev1_rom1.90` (1 s) | yes, `R 23` | every connect | Q |
| 0x2C | START_DISPLAY_BITMAP | 250 x `2c` | none | yes | before full frames; between failed HELLOs | D |
| 0x64 | GET_STORAGE_INFO | `64 ef 69 00 00 00 01` | `a-b-c-d-e-f` (1 s, 3 tries), section 13.2 | no | storage checks, video start | Q |
| 0x65 | LIST_DIR | `65 ef 69 BE32(n) 00 00 00 <path>` | `...file:<f1>/<f2>/...` or `nodir-createdone` (1 s, 3 tries) | no | device page, cleanup, before uploads | Q, **creates a missing directory** |
| 0x66 | DELETE_FILE | `66 ef 69 BE32(n) 00 00 00 <path>` | none | no | device page; automatic cleanup; factory test mode | X |
| 0x6E | GET_FILE_SIZE | `6e ef 69 BE32(n) 00 00 00 <path>` | decimal bytes, `0` = absent (2 s, 3 tries) | no | existence checks, upload verification | Q |
| 0x6F | UPLOAD_FILE | `6f ef 69 BE32(n) 00 00 00 <path> LE32(size)` + data phase | `create_success` (3 s), then `file_rev_done` | no | uploads, firmware | X |
| 0x78 | PLAY_VIDEO | `78 ef 69 BE32(n) <loop> 00 00 <path>` | `play_video_success` (6 s) | no | theme video, device page | D |
| 0x79 | STOP_VIDEO | `79 ef 69 00 00 00 01` | none | yes | theme start, exit, before uploads | D |
| 0x7B | SET_BRIGHTNESS | `7b ef 69 00 00 00 01 00 00 00 <0..255>` | none | yes | slider, full-frame ritual, suspend and exit (0) | D |
| 0x7D | OPTIONS | `7d ef 69 00 00 00 05 00 00 00 <bright> <mode> 00 <flip> <sleep>` | none | constant, section 6.1 | every theme start, Save, exit | P |
| 0x81 | ROTATION | `81 ef 69 00 00 00 01 00 00 00 <0..3>` | none | no | theme start, large screens | P (**inferred**) |
| 0x82 | (restart device app?) | `82 ef 69 00 00 00 01` | none; the vendor waits 2 s and re-inits | no | before video play, small screens | X |
| 0x83 | TURNOFF | `83 ef 69 00 00 00 01` | none | yes | exit, unless "sleep mode" is set | D |
| 0x84 | RESTART | `84 ef 69 00 00 00 01` | none; the SoC reboots and re-enumerates into its start mode (section 19) | yes, `Reset` | device page; after a firmware upload | X |
| 0x86 | PRE_UPDATE_BITMAP | `86 ef 69 00 00 00 01` | none | before every full frame | once per theme start | D |
| 0x87 | (end of PC stream?) | `87 ef 69 00 00 00 01` | none; after a device video it freezes the video's frame (section 19) | no | every theme stop | D |
| 0x8C | PLAY_IMAGE | `8c ef 69 BE32(n) 00 00 00 <path>` | `play_img_ok` (3 s) | no | device page | D |
| 0x96 | STOP_MEDIA | `96 ef 69 00 00 00 01` | contains `media_stop` once playback stopped (1 s) | yes, `R 1024`, ignored | polled after STOP_VIDEO | D |
| 0xC8 | DISPLAY_BITMAP | `c8 ef 69 BE32(W*H*4) 00 00 00` + data phase | one read, logged; hardware: `full_png_sucess` | yes, bytes 6..7 differ (quirk 18.2) | full frames | D |
| 0xCA | DISPLAY_BITMAP_WITH_ALPHA_LIST | `ca ef 69 BE32(W*H*4) 00 00 00` + data phase, then 0xD0 | none | no | full frames, small screens with a device-side video | D |
| 0xCC | UPDATE_BITMAP | `cc ef 69 BE32(n) 00 00 00 BE32(seq) BE32(p)` + data phase | none | yes, section 9.1 | partial updates | D |
| 0xCF | QUERY_STATUS | `cf ef 69 00 00 00 01` | `needReSend:<0/1>\|renderCnt:<n>\|theme:<s>` (1 s) | after every bitmap, `R 1024`, ignored | every tick, before the partial | Q |
| 0xD0 | ALPHA_POSLEN_LIST | `d0 ef 69 BE32(m) 00 00 00` + data phase | none | no | after 0xCA | D |

Values in the vendor's opcode list that it never sends: 0x2D, 0x67, 0x7A, 0x7C, 0x7E-0x80, 0x85, 0x97, 0x98, 0xC9,
0xCD, 0xCE. The Python constant `2d` inside OPTIONS is the brightness field, not opcode 0x2D.

### 3.1 Python constants (`lcd_comm_rev_c.py:65-95`)

Only the entries whose bytes differ from the table above:

| Name | Bytes before padding | Extra payload | Python use |
|---|---|---|---|
| OPTIONS | `7d ef 69 00 00 00 05 00 00 00 2d` | `[startmode] [00] [flip] [sleep]` | `SetOrientation` |
| TURNON | `83 ef 69 00 00 00 00` | - | **never sent** (neither app sends it) |
| DISPLAY_BITMAP_2INCH | `c8 ef 69 00 0e 10` | `BE16(480 * 480 / 64)` = `0e 10` | full frame, 2.1" / 2.8" |
| DISPLAY_BITMAP_5INCH | `c8 ef 69 00 17 70` | `0e 10` | full frame, 5" |
| DISPLAY_BITMAP_8INCH | `c8 ef 69 00 38 40` | `0e 10` | full frame, 8.8" |
| UPDATE_BITMAP | `cc ef 69 00` | section 9.1 | partial update header |
| SEND_PAYLOAD | none | raw data | data blocks |

OPTIONS field values in Python: start mode `00` default / `01` image / `02` video; flip `01` FLIP_180 / `00` NO_FLIP;
sleep interval `00` (off) to `0a` (ten). Python only ever sends start mode default, NO_FLIP and sleep off.

## 4. Handshake: HELLO and model

### 4.1 Python (`lcd_comm_rev_c.py:215-254`)

1. Discard pending input; send HELLO (bypassing the queue); `read(23)`; keep only printable ASCII; discard input.
2. While the answer does not start with `chs_`: sleep 1 s and resend. **Python loops forever.**
3. Sub-revision comes from the **configured** size, not the answer: 480x480 -> REV_2INCH (2.1" and 2.8"),
   480x800 -> REV_5INCH, 480x1920 -> REV_8INCH, anything else logs an error.
4. ROM version = `int(answer.split(".")[2])` (87, 88, 90, ...). If unparsable or outside 80..100, 87 is assumed.

### 4.2 Vendor

```
repeat up to N times (N = 1 at init and inside the reconnect loop, 3 before restarting a stalled video):
    send HELLO; one read of <= 1024 bytes within 1 s; remove NULs
    accept if the answer contains the model key of the matched table entry ("88inch", "34inch", ...)
           or "chs_5inch.dev1"
    ROM = digits after "rom" read as d0.d1d2...        ("1.90" -> 1.9, "1.88" -> 1.88, "1.87" -> 1.87)
    on failure, when N > 1: 250 x 2c, then 1000 ms
```

- ROM threshold: vendor `>= 1.89`, Python `> 88`. Same split for every known version.
- An answer containing `68inch.dev2` selects a second 6.8" layout (not analysed).
- 0525:a4a7 is also the id of an 11.3" entry that the vendor keeps out of its active list; only the HELLO key could
  tell them apart (**inferred**).

Known answers: 5" `chs_5inch.dev1_rom1.87`; 2.1" `chs_5inch.dev1_rom1.88` (sic: 2.1" units report `5inch`); 8.8"
`chs_88inch.dev1_rom1.88` and `chs_88inch.dev1_rom1.90` (Python history, commits `8c26266`, `d720a80`; `rom1.90` also
on hardware, section 19). The vendor shows the string as the "ROM" line of its device page.

Bezel bounds the retries, identifies the model by VID:PID and uses the HELLO string as a confirmation and for the ROM.

## 5. Brightness

- Python (`lcd_comm_rev_c.py:299-307`): `int(level / 100 * 255)` for level 0..100, sent as
  `7b ef 69 00 00 00 01 00 00 00 <L>` + pad250, immediately from the calling thread (bypassing the queue).
- Vendor: the 0..255 setting as is (default 170). 0 darkens the panel on suspend and at exit. The value is also sent
  inside every full-frame ritual (section 8.2).

255 is brightest.

## 6. Orientation, OPTIONS and ROTATION

### 6.1 Python (`lcd_comm_rev_c.py:309-318`)

`SetOrientation(o)` always sends OPTIONS `7d ef 69 00 00 00 05 00 00 00 2d 00 00 00 00` + pad250 (start mode
default, NO_FLIP, sleep off), queued. The FLIP_180 variant for reverse orientations is commented out; it was
enabled and disabled several times (commits `c09d0a5`, `be7bf45`, `6a5d69d`, `d720a80`). **All rotation is done in
software** (section 10).

### 6.2 OPTIONS 0x7D (vendor)

```
7d ef 69 00 00 00 05 00 00 00 | brightness | startMode | 00 | imgFlip | sleepDelay
```

| Byte | Field | Values |
|---|---|---|
| 10 | brightness | stored brightness 0..255. Python's constant `2d` therefore stores 45 |
| 11 | startMode | 0 default (built-in clock/logo), 1 image, 2 video: what the device shows on its own once the SoC has booted. Applied only when the SoC (re)starts (power-up, 0x84, the MCU restart), not when the host leaves; the vendor UI says it takes effect after a power cycle. **Hardware** (section 19): 1 is the firmware's carousel album (the vendor's zh label 轮播相册), every image of `/mnt/SDCARD/img/` in turn every ~3–5 s, with no interval setting; 2 plays the first entry of `/mnt/SDCARD/video/` |
| 12 | reserved | `00` |
| 13 | imgFlip | 0/1, "flip 180°" for device-side playback. The vendor also sets 1 when its rotation setting is non-zero on large screens. It turns by 180° only, so album pictures for a screen used the other way must be stored already turned to the native buffer (480 x 1920 on the 8.8") |
| 14 | sleepDelay | 0 never, 1..10 minutes: the firmware's own screen-sleep timer. **Hardware** (section 19): it counts the time since the last host traffic and, when it fires, the SoC powers down as with TURNOFF; it also fires during standalone album or video playback |

Sent at every theme start (then 10 ms), by the device page's Save, and first in the exit sequence. Two OEM builds
force startMode 2. The vendor's `offLineMode` setting never reaches rev C: only the config packet of its "207 LCD"
class carries it.

The firmware, not the host, picks the stored file a start mode shows (**hardware**, section 19): start mode 2 boots
the first entry of `/mnt/SDCARD/video/`, and playing another video with 0x78 first (then waiting 75 s) does not change
it; start mode 1 cycles every image of the card. There is no "set boot media" command on this transport: the vendor
app (decompiled) never names a boot file. OPTIONS is the only packet behind its start-mode setting, and its "Play
Select" sends 0x78 with flag 0.

Bezel writes OPTIONS whole from what it recorded for the screen (the start mode, flip 0, the sleep timer, and as
brightness the level the user chose with it, sent as SET_BRIGHTNESS just before, else the link's last level), and only on
an explicit user action: the boot media (`bezel storage boot`, **Show at start**) and the plan B of the shutdown choice
(section 16). Neither rewrites the other's fields on its own; the last action wins, and the catalog records which plan B
was stored last, with its chosen level (`D-2026-10-03-power-off-standby-2` (3), (4)).

### 6.3 ROTATION 0x81 (vendor)

`81 ef 69 00 00 00 01 00 00 00 <r>`, `r` = 0..3 for 0°/90°/180°/270° as chosen in the UI. Sent at theme start on
large screens, right after the settings are saved. The vendor rotates the frame on the PC as well (section 10), so
streamed pixels are always native; whether 0x81 and imgFlip affect streamed frames or only stored media is unknown.

Bezel: OPTIONS and ROTATION persist on the device and are sent only on an explicit user action (section 16).
Rotation is done in software.

## 7. Session sequences

### 7.1 Python

- `ScreenOff()`: STOP_VIDEO; STOP_MEDIA + `read(1024)`; TURNOFF (`lcd_comm_rev_c.py:287-291`).
- `ScreenOn()`: STOP_VIDEO; STOP_MEDIA + `read(1024)`. TURNON is never sent and the brightness restore is commented
  out (`lcd_comm_rev_c.py:293-297`). How the panel wakes after TURNOFF is not established.
- `Reset()`: RESTART (bypassing the queue), close the port, wait up to 15 s while an awake port is still listed, wait
  up to 15 s while none is listed, then re-open (which re-runs the wake-up detection on AUTO)
  (`lcd_comm_rev_c.py:259-273`).
- `Clear()`: SetOrientation(PORTRAIT), full white frame, restore the previous orientation (`lcd_comm_rev_c.py:275-285`).

### 7.2 Vendor theme start and refresh loop

```
1  HELLO; on failure the reconnect ladder (section 15)                    fail -> give up
2  firmware check (section 14)                                            update -> the device reboots, stop
3  STOP_VIDEO; 200 ms; up to 20 x { STOP_MEDIA; read <= 1 s, done when it contains "media_stop"; 400 ms }
4  [large screens] ROTATION <r>; 50 ms
5  SET_BRIGHTNESS <b>; 50 ms
6  OPTIONS; 10 ms
7  [theme with a video] start the device-side video (section 13.5)         fail -> abort
8  10 ms; PRE_UPDATE_BITMAP; 100 ms
9  full frame (section 8.2)
10 every second: [not 6.8"] QUERY_STATUS, parsed (section 12.1)
                [video theme] renderCnt did not advance -> restart the video
                partial update against the last frame sent (section 9.2)
                sleep 1000 ms minus the time spent
```

The vendor's serial refresh rate is therefore a fixed 1 Hz. An anti-burn-in option restarts the theme every hour.

| Event | Vendor traffic |
|---|---|
| Theme stop or switch | `87 ef 69 00 00 00 01` (semantics unknown; probably "leave PC-stream mode"; after a device video it freezes the video's frame, section 19) |
| App exit, Windows shutdown | 100 ms; OPTIONS; SET_BRIGHTNESS 0; TURNOFF (unless "sleep mode" is set); STOP_VIDEO; close |
| System suspend | SET_BRIGHTNESS 0 |
| Resume | SET_BRIGHTNESS `<b>`; re-init with up to 40 reconnect attempts |
| Hot-plug | [devices.md](devices.md) section 3.2 |

## 8. Full-frame update

### 8.1 Python (`lcd_comm_rev_c.py:347-365`, `374-395`)

Taken when `x == 0 and y == 0 and w == W and h == H`. Under the queue mutex:

1. PRE_UPDATE_BITMAP `86 ef 69 00 00 00 01` + pad250.
2. START_DISPLAY_BITMAP: 250 x `2c`.
3. DISPLAY_BITMAP_xINCH + `0e 10` + pad250 (5": `c8 ef 69 00 17 70 0e 10` + 242 x `00`).
4. SEND_PAYLOAD: rotate the image to the native orientation (section 10), convert to **BGRA, 4 bytes per pixel**
   (alpha 0xFF for RGB sources), split into 249-byte chunks joined by a single `00` byte (so every 250-byte block is
   249 data bytes + `00`), pad250 with zeros, one `write()`, then `read(1024)`.
   Sizes: 2.1" 925,500 B; 5" 1,542,250 B; 8.8" 3,701,250 B.
5. QUERY_STATUS + pad250, `read(1024)`.

### 8.2 Vendor

```
2c x250 | 10 ms | 7b ef 69 00 00 00 01 00 00 00 <b> +pad | 100 ms
| c8 ef 69 BE32(W*H*4) 00 00 00 +pad | data phase | 100 ms
| c8 ef 69 BE32(W*H*4) 00 00 00 +pad | data phase (the same frame again) | 10 ms | one read <= 1 s, logged
```

- PRE_UPDATE_BITMAP comes once per theme start (section 7.2 step 8), not before every full frame.
- The length is the BGRA byte count and bytes 7..9 are `00 00 00`.
- Pixels: BGRA, native orientation, top-down, row-major. Alpha matters: transparent pixels let a device-side video
  show through (section 13.5).
- The partial-update sequence number restarts at 0.
- Full frames are sent at theme start, after QUERY_STATUS reports `needReSend:1`, after a reconnect, and when a diff
  cannot be encoded. Small screens with a device-side video use 0xCA and a POSLEN list instead (section 13.5).

### 8.3 Full-frame sizes (static, computed)

| Model | Native buffer (columns x rows) | BGRA bytes | Header before padding | Wire bytes |
|---|---|---|---|---|
| 2.4" | 240 x 320 | 307,200 | `c8 ef 69 00 04 b0 00 00 00 00` | 308,500 |
| 2.8" square | 320 x 320 | 409,600 | `c8 ef 69 00 06 40 00 00 00 00` | 411,250 |
| 2.1" / 2.8" round, 3.4" | 480 x 480 | 921,600 | `c8 ef 69 00 0e 10 00 00 00 00` | 925,500 |
| 4" | 720 x 720 | 2,073,600 | `c8 ef 69 00 1f a4 00 00 00 00` | 2,082,000 |
| 5" | 800 x 480 | 1,536,000 | `c8 ef 69 00 17 70 00 00 00 00` | 1,542,250 |
| 6.5" | 720 x 1568 | 4,515,840 | `c8 ef 69 00 44 e8 00 00 00 00` | 4,534,000 |
| 6.8" | 1080 x 2320 | 10,022,400 | `c8 ef 69 00 98 ee 00 00 00 00` | 10,062,750 |
| 8" | 800 x 1280 | 4,096,000 | `c8 ef 69 00 3e 80 00 00 00 00` | 4,112,500 |
| 8.8" | 480 x 1920 | 3,686,400 | `c8 ef 69 00 38 40 00 00 00 00` | 3,701,250 |

## 9. Partial update

### 9.1 Python (`lcd_comm_rev_c.py:366-372`, `397-467`)

Any other rectangle. Under the queue mutex:

1. SEND_PAYLOAD with a 14-byte header + pad250:

   ```
   cc ef 69 00 | BE24(size) | 00 00 00 | BE32(count)
   ```

   `size = len(raw_rows) + 2` (the code comment: "+2 for the ef69 added later"). `count` is a class-wide counter
   `Count.Start`: 0 for the first partial update of the process, incremented after each one, shared by all
   instances, never reset.
2. SEND_PAYLOAD with the rows:
   - rotate the image to native orientation and compute the native start `(row0, col0)` (section 10);
   - pixel format: **BGRA (4 B)** if the model is not REV_2INCH **and** ROM > 88, otherwise **BGR (3 B)**;
   - for each image row `r`: `BE24(address) BE16(image_width) pixels`, with
     `address = (row0 + r) * rowlen + col0`;
   - if the concatenation is **longer than 250 bytes**, split it into 249-byte chunks joined by `00`;
   - append `ef 69`; pad250 with zeros.
3. QUERY_STATUS + pad250, `read(1024)`.

History: the size field was 2 bytes until 2023 (commits `ebc32cd`, `5301977`: overflow with big fonts); an earlier
workaround split images over 0xFF00 bytes (`e971733`, removed).

### 9.2 Vendor

```
header: cc ef 69 BE32(n) 00 00 00 BE32(seq) BE32(p) + pad250
data:   run list [+ POSLEN list] + ef 69, as a data phase (section 2.2)
n = run-list bytes + p + 2;   p = POSLEN bytes, 0 when no POSLEN list is appended
```

- Run list: diff of the last frame sent and the new frame over the whole native BGRA buffer, as linear-index runs
  that may span rows, single-pixel records, runs <= 65,000 pixels ([pixel-formats.md](pixel-formats.md) sections 6
  and 7). Pixels are 4-byte BGRA on large screens with ROM >= 1.89 and 3-byte compressed BGRA otherwise
  ([pixel-formats.md](pixel-formats.md) section 4).
- `seq` is 0 for the first partial after a full frame and grows by 1 per partial.
- A POSLEN list is appended only on small screens with a device-side video (section 13.5).
- No change: the vendor still sends a partial every tick, with the data `80 00 00 00 00 00 ef 69` (`n` = 8). In raw
  BGRA mode that dummy is one byte short of a valid single-pixel record; the firmware evidently tolerates it
  (**inferred**).
- Overflow (a run longer than 65,000 pixels, about 135 rows of the 8.8", or a diff larger than the frame): the
  vendor's encoder fails and its frame loop treats the failure as a transport error, runs the reconnect ladder, then
  sends a full frame. Bezel sends a full frame directly.

Python's header is the same layout: `00 | BE24(size)` is `BE32(n)`, `BE32(count)` sits at bytes 10..13 like `seq`,
and bytes 14..17 are zero padding, i.e. `p` = 0. A Python row record is a run record, so Python's partials are a
subset of the vendor format. Differences: Python never resets its counter, emits one run per image row, uses plain BGR
(not compressed BGRA) as its 3-byte format, and uses BGRA on the 5" with ROM > 88 where the vendor sends compressed
BGRA (small screen).

## 10. Native geometry and rotation (`lcd_comm_rev_c.py:374-430`)

| Model | Native buffer (columns x rows) | Library orientation that is native | `rowlen` in addresses |
|---|---|---|---|
| 2.1" / 2.8" | 480 x 480 | LANDSCAPE | `display_height` = 480 |
| 5" | 800 x 480 | LANDSCAPE | `display_height` = 800 |
| 8.8" | 480 x 1920 | REVERSE_PORTRAIT | `display_width` = 480 |

`W`, `H` are `get_width()`, `get_height()` of the current orientation; `w`, `h` are the image size **after** the
rotation. Rotations are Pillow `rotate(angle, expand=True)`: counter-clockwise.

2.1" / 2.8" / 5":

| Orientation | Image transform | row0 | col0 |
|---|---|---|---|
| LANDSCAPE (native) | none | y | x |
| PORTRAIT | rotate 90 CCW | W - x - h | y |
| REVERSE_PORTRAIT | rotate 270 CCW (= 90 CW) | x | H - y - w |
| REVERSE_LANDSCAPE | rotate 180 | H - y - h | W - x - w |

8.8":

| Orientation | Image transform | row0 | col0 |
|---|---|---|---|
| REVERSE_PORTRAIT (native) | none | y | x |
| LANDSCAPE | rotate 270 CCW (= 90 CW) | x | H - y - w |
| REVERSE_LANDSCAPE | rotate 90 CCW | W - x - h | y |
| PORTRAIT | rotate 180 | H - y - h | **H - x - w** (suspected bug, quirk 18.3) |

Full frames use the same rotations (2.1"/5": PORTRAIT 90 CCW, REVERSE_PORTRAIT 270, REVERSE_LANDSCAPE 180;
8.8": LANDSCAPE 270, REVERSE_LANDSCAPE 90, PORTRAIT 180).

Vendor: on its portrait panels (2.4", 6.5", 6.8", 8", 8.8") a landscape theme is turned 90° clockwise into the native
buffer and rotation setting 2 adds 180°; other panels are sent as composed. On the 8.8" this is Python's LANDSCAPE
transform. Details in [pixel-formats.md](pixel-formats.md) section 11.

## 11. Pixel formats

| Frame | Python | Vendor |
|---|---|---|
| Full (0xC8) | BGRA `[B, G, R, A]`, alpha 0xFF for RGB sources | BGRA with the composed alpha |
| Partial (0xCC) | BGR on 2.1"/2.8" and on ROM <= 88; BGRA on 5"/8.8" with ROM > 88 | raw BGRA on large screens with ROM >= 1.89; compressed 3-byte BGRA otherwise |

The Python helper `image_to_compressed_BGRA` exists but is unused: it was used briefly for ROM <= 88 (`d439a06`) then
replaced by plain BGR (`94ed336`: "because this program does not support transparent background"). With plain BGR
the low two bits of B and G carry whatever the colour has; the vendor reads those bits as alpha on the 3-byte format,
so a firmware doing the same treats such pixels as partially transparent ([pixel-formats.md](pixel-formats.md)
section 4). Opaque 3-byte pixels need a4 = 15.

## 12. QUERY_STATUS, pacing and threading

### 12.1 QUERY_STATUS reply

`needReSend:<0|1>|renderCnt:<n>|theme:<s>`; the vendor parser treats the third field as optional. It reads:

- no `|` in the answer: an empty answer; more than 3 in a row fail the frame (reconnect);
- `needReSend:1`: send a full frame on the next tick;
- `renderCnt`: device-side counter of rendered video frames. With a video theme, no progress between two ticks means
  the device video stalled: STOP_VIDEO, 80 ms, play the video again, then a full frame;
- `theme:` stored, unused.

Python sends QUERY_STATUS after every bitmap and ignores the answer. Observed answers: section 19.

### 12.2 Pacing

Python: every bitmap ends with QUERY_STATUS and a blocking `read(1024)` (1 s timeout); full frames also read 1024
bytes after the payload; STOP_MEDIA reads 1024. These reads are the effective pacing. Per partial: 3 x 250-byte
blocks, 5 bytes per row, 1 byte per 249 data bytes and one 1024-byte read.

Vendor:

| Step | Delay |
|---|---|
| Frame loop | 1000 ms minus the time spent (1 Hz) |
| HELLO reply | <= 1000 ms; between failed attempts 250 x `2c` + 1000 ms |
| Before the first full frame | 10 ms, 0x86, 100 ms |
| Full-frame ritual | `2c` block, 10 ms, 0x7B, 100 ms, 0xC8, 100 ms, 0xC8, 10 ms, read <= 1 s |
| Theme start | 50 ms after ROTATION, 50 ms after SET_BRIGHTNESS, 10 ms after OPTIONS |
| Stop media | 200 ms after 0x79; 0x96 polled every ~1.4 s (1 s read + 400 ms), <= 20 polls |
| Restart a stalled video | 80 ms after 0x79 |
| Play video, small screens | 0x82, 2000 ms, re-init |
| Upload | CREATE reply <= 3 s; completion section 13.4 |
| Reconnect | section 15 |

### 12.3 Threading

Python: sequences are atomic under the queue mutex, except `SetBrightness`. Vendor: one start task per device runs
render, diff, send and pacing; UI pages and power events call the device from other threads and interleave at write
granularity (section 2.5); replies are matched to requests by timing only.

## 13. Device storage and on-device video (vendor, static)

### 13.1 Paths

| Root | Large screens | Small screens |
|---|---|---|
| Internal images | `/mnt/UDISK/img/` | `/root/img/` |
| Internal videos | `/mnt/UDISK/video/` | `/root/video/` |
| TF card images | `/mnt/SDCARD/img/` | `/mnt/SDCARD/img/` |
| TF card videos | `/mnt/SDCARD/video/` | `/mnt/SDCARD/video/` |
| RAM (lost at reboot) | `/tmp/video/` | `/tmp/video/` |

Start mode 1 cycles the TF card's `img/` and start mode 2 boots the first entry of its `video/` (section 6.2).

Firmware image: `/update.app` (section 14). The vendor's factory test mode (a command-line switch) deletes `/app_cfg`
after init and `/usr/data/app.cfg` at theme start; `/usr/data/...` are otherwise TUR_USB paths. The serial transport
never reads or writes a device configuration file: settings travel with 0x7B, 0x7D and 0x81.

### 13.2 GET_STORAGE_INFO 0x64

Reply `a-b-c-d-e-f`: six decimal fields in KiB, flash total, flash used, flash free, TF total, TF used, TF free. The
vendor subtracts 512 KiB (reserved) from flash total and flash free. TF total 0 means no card; a card counts as
present when TF total > 1024 KiB. Up to 3 tries of 1 s.

### 13.3 File operations

| Operation | Command | Reply | Notes |
|---|---|---|---|
| List | 0x65 `<dir>` | `...file:<f1>/<f2>/...` (text before `file:` unknown) or `nodir-createdone` | **creates the directory** when missing; empty names are skipped |
| Size, exists | 0x6E `<path>` | decimal bytes, `0` absent | the vendor parses int32: files >= 2 GiB read as 0 |
| Delete | 0x66 `<path>` | none | destructive |
| Upload | 0x6F | section 13.4 | destructive: overwrites, fills flash |
| Download, rename, mkdir, format | none | - | TF formatting is a PC-side procedure: one primary FAT32 partition, MBR |

### 13.4 Upload

```
free-space check (section 13.6); refuse files over 4 GiB
STOP_VIDEO; 200 ms; poll STOP_MEDIA until "media_stop"
LIST_DIR <target directory>                                     (creates it)
6f ef 69 BE32(len(path)) 00 00 00 <path> LE32(file size) +pad
read <= 3 s; must contain "create_success"
data phase: the whole file in 249+1 blocks
wait for "file_rev_done": one read of up to 10 s (TF video directory) or 240 s (any other root), repeated up to
    15 times on large screens (once on small), 200 ms apart
GET_FILE_SIZE <path> must equal the file size
```

- Mixed endianness: BE32 path length in bytes 3..6, **LE32** file size after the path. The device drops the block
  padding using that size.
- **Size limit (hardware, 8.8" ROM 1.90):** the firmware keeps the whole upload in memory before storing it. It
  stopped reading at exactly 29,577,216 bytes of a 42 MB upload in three runs (to the card, at 7 MiB/s and at
  0.94 MiB/s alike), left that partial file and stayed hung until the MCU restarted it (section 15); 24.0 MiB passed at
  6 MiB/s and the vendor app's largest stored files are 24.6 MiB. Below the limit the data phase runs at the link's
  speed (5.1 MiB in 0.63 s, 11.8 MiB in 1.63 s) and `file_rev_done` follows within 0.2 s. Bezel caps rev C uploads at
  25 MiB (D-2026-09-30-release-polish-12).
- The vendor lower-cases names and accepts only `[A-Za-z0-9_.-]`; its device page uploads jpg/jpeg/bmp/png/gif/mp4/
  h264 and transcodes an MP4 of another resolution first ([video.md](video.md) section 3).
- The data phase has no abort: after the header the firmware takes the next *declared-size* bytes as file content,
  whatever they are (section 19). A HELLO answered after a cancel does not mean the data phase ended cleanly: the bytes
  still queued for the firmware's writer then go into the next file. Completing the declared length is no way out
  either: on the 8.8" a cancelled upload completed with filler hung the firmware until a USB replug (section 19).
  **Bezel's cancel recovery** (not vendor; D-2026-09-30-release-polish-10): after a cancel it sends nothing more of the
  data phase (no filler), then HELLO (with its resync blocks) and GET_FILE_SIZE, and reports the partial file for the
  user to delete; nothing is deleted. A cancel while waiting for `file_rev_done` (the whole file sent) takes the same
  path. When no HELLO is answered it reports that the next command reconnects the screen (the wake) and names the path
  to check for a partial file. Stray bytes that reach the next upload make its GET_FILE_SIZE differ from the file size;
  that upload then fails, saying to delete the file and send it again.

### 13.5 Device-side video and the overlay

The PC never streams video over serial: a theme video is uploaded once and played in a loop by the device, and the PC
keeps sending the theme overlay at 1 Hz.

1. GET_STORAGE_INFO; 100 ms.
2. Look for `<videoName>` in the RAM, internal and (with a card) TF video directories with GET_FILE_SIZE.
3. If absent, upload it (section 13.4) to the TF card when a card is present, else to internal flash. The file must
   already have the panel's exact native resolution.
4. PLAY_VIDEO with loop = 1: [small screens] 0x82, 2000 ms, re-init (HELLO, up to 10 attempts); GET_FILE_SIZE (result
   unused); `78 ef 69 BE32(n) 01 00 00 <path>`; read <= 6 s for `play_video_success`; 2 attempts. A looping video
   keeps playing after the host closes the port (section 19).

Overlay:

- Large screens: 0xC8 / 0xCC with per-pixel alpha. A = 0 shows the video, A = 255 (a4 = 15 in the 3-byte form) hides it.
- Small screens: 0xCA full frame, then `d0 ef 69 BE32(m) 00 00 00` with the POSLEN list of pixels whose alpha is > 15
  followed by `ef 69` (`m` counts the 2 trailer bytes; nothing is sent for an empty list). Partials append the POSLEN
  list and set `p` (section 9.2). POSLEN: [pixel-formats.md](pixel-formats.md) section 8.

PLAY_IMAGE (device page only, jpg/jpeg/bmp/png): stop media and wait; `8c ef 69 BE32(n) 00 00 00 <path>`; read <= 3 s
for `play_img_ok`. Stall watchdog: section 12.1.

### 13.6 Free-space check and automatic cleanup

```
RAM root: always fits
internal: fits when bytes < flash free * 1024; otherwise a small screen fails here
TF card:  fits when bytes < TF free * 1024
does not fit and the model is not the 5":
    delete every file in /mnt/SDCARD/video/, then (large screens) every file in /mnt/UDISK/video/
    1000 ms; check once more without cleanup
```

Deletion lists the directory and sends DELETE for each name. The vendor's "clear cache" setting runs the same deletion
on demand (STOP_VIDEO, STOP_MEDIA, delete all, 2000 ms). **Destructive: Bezel never deletes device files on its own.**

## 14. Firmware update (vendor, static; destructive)

- Runs at every theme start, right after HELLO, and from the device page.
- Candidates are files in the app's firmware folder whose name contains the model key and ends with `_<version>`
  (for example `88inch_1.91`, no extension). The device version is the number after `rom` in the HELLO answer.
- A newer file is uploaded (section 13.4) to the absolute path `/update.app`, then RESTART 0x84; the firmware
  applies it while booting. No checksum or signature on the host side and no download: the folder ships empty.

Bezel does not implement firmware updates.

## 15. Connection recovery (vendor, static)

```
for attempt in 0 .. max-1:
    stop if the device is being stopped
    close and reopen the port; one HELLO                                 ok -> done
    1500 ms
    attempt 6, 10, 20, 30, ...: MCU command 00 00 00 00 00 c9 on the sibling port, then 8000 ms
    attempt 2 and 4: close the port; restart the USB device node of the SoC (Windows device restart); 4000 ms
    on an exception: 1000 ms
```

- `max` is the start's retry count (40 after resume), or 8 when a frame fails. With 8 the USB restart runs twice and
  the MCU command once. After a failed reconnect the frame sender waits 2000 ms and tries again, 3 rounds at most.
- One OEM build retries forever.
- Linux equivalents of the USB restart: `USBDEVFS_RESET`, or writing 0 then 1 to the device's sysfs `authorized`.
- The MCU and its sibling-port pairing: [devices.md](devices.md) section 5.2.

Every step after the first reopen resets or re-enumerates the screen (section 16).

## 16. Disruptive commands and Bezel's policy

Decision `D-2026-09-30-device-protocols-2` limits Bezel's automatic traffic to what the vendor apps send on every
start and stop: HELLO, STOP_VIDEO / STOP_MEDIA, brightness, PRE/END update (0x86 / 0x87), frames, QUERY_STATUS and
screen off. Everything below is **never sent implicitly**: it needs an explicit command, and a destructive one needs
`Confirm::Yes`. One exception, validated on the 8.8" (`D-2026-09-30-release-polish-13`): the MCU restart is also sent
automatically, once per connection attempt, to a SoC that is on the bus but answers no HELLO or stopped reading; it is
refused while another program holds the SoC's port.

The shutdown choice (`D-2026-10-03-power-off-standby-2` (5) and `-3`) adds no new exception: on its paths, OPTIONS,
RESTART 0x84 and PLAY_VIDEO of a stored file come only from the user's own choice of what a screen does when the
computer shuts down. OPTIONS (written whole: the plan B) goes when the user confirms that choice; at shutdown go the
packets that carry it out, the one case where they leave without a click at that moment:

| Choice | At shutdown | Plan B (OPTIONS) |
|---|---|---|
| `keep` (default) | nothing | start mode of the boot media, sleep 0 (nothing when it already was `keep`) |
| `off` | TURNOFF 0x83 only, without waiting for the SoC to leave | start mode of the boot media, sleep 1..10 min |
| `video` | GET_FILE_SIZE 0x6E of the chosen file, STOP_VIDEO 0x79 and at most one STOP_MEDIA 0x96, then PLAY_VIDEO 0x78, loop; no 0x87 after it | start mode 2, sleep 0 |
| `album` | GET_STORAGE_INFO 0x64 (a card?), SET_BRIGHTNESS 0x7B when a level was stored with the last plan B, OPTIONS (start mode 1, sleep 0, that level), then RESTART 0x84, without waiting | start mode 1, sleep 0 |

Besides the queries, the SET_BRIGHTNESS of `album` is the one packet at shutdown that `D-2026-10-03-power-off-standby-3`
(1) does not list (it names OPTIONS start mode 1 and RESTART), and it adds no exception. OPTIONS stores as its byte 10
the level the link last set (section 6.2), and the link that carries out the choice at shutdown is the live one, at the
level the studio set, or one opened for it, which set none. Without 0x7B first, the album would start after the restart
at that level or at the vendor's default (170), not at the one chosen with the plan B (`bezel standby set album
--brightness`), which `D-2026-10-03-power-off-standby-2` (3) has OPTIONS carry. It goes only when the record holds a
chosen level; otherwise OPTIONS carries the link's level and no 0x7B is sent. 0x7B is the brightness Bezel sends at
every theme start, within the automatic set above: neither a storage command nor a disruptive one, and persistent only
through the OPTIONS that follows it. That OPTIONS (start mode 1) is what the catalog then records as the plan B stored
last, also when a boot media set after the choice had stored another.

A choice that cannot be carried out (the file or the card is gone) gets TURNOFF instead of a frozen frame. The sleep
timer goes only with `off`, because it also powers down a standalone album or video (section 19). No other persistent,
storage or disruptive command (DELETE, UPLOAD, 0x82, 0x81, the MCU restart) leaves these paths, and Bezel never
deletes a device file on its own. While a rev C screen is live, a minimal partial update (one pixel with the value
already shown) and the usual QUERY_STATUS after 30 s without traffic keep the sleep timer from firing
(`D-2026-10-03-power-off-standby-5`): frame traffic, within the automatic set above.

| Command | Effect | Kind |
|---|---|---|
| 0x66 DELETE_FILE | removes a stored file | destructive |
| 0x6F UPLOAD_FILE | writes or overwrites a file; fills flash | destructive (storage write) |
| automatic video cleanup (section 13.6) | deletes every stored video | destructive; Bezel never runs it |
| firmware (`/update.app` + 0x84) | replaces the firmware | destructive |
| 0x84 RESTART | reboots the SoC, one packet: the gadget leaves the bus in about 3 s and returns about 13 s later in its start mode (**hardware**, 8.8") | disruptive |
| 0x82 | restarts something on the device; the vendor waits 2 s and re-inits | disruptive |
| MCU `00 00 00 00 00 c9` | restarts the SoC, with the MCU port held 8 s: it leaves the bus at once and returns about 10 s later (**hardware**, 8.8"), also when hung | disruptive |
| USB device reset (section 15) | re-enumerates the screen | disruptive |
| 0x7D OPTIONS | boot mode, flip, sleep timer and stored brightness; Bezel writes it whole (section 6.2) | persistent |
| 0x81 ROTATION | rotation setting | persistent (**inferred**) |
| 0x65 LIST_DIR on a missing directory | creates it | storage write |
| 0x78 / 0x8C | device-side playback; a looping video outlives the host's connection; neither picks the boot media (section 6.2) | explicit only |

## 17. Test vectors

### 17.1 Python (verified)

Each line is one write; `+pad` means zeros up to 250 bytes; `R n` is a read.

```
SetBrightness(25)   : 7bef69000000010000003f+pad
SetBrightness(100)  : 7bef6900000001000000ff+pad
HELLO               : 01ef6900000001000000c5d3+pad, R 23
ScreenOff           : 79ef6900000001+pad | 96ef6900000001+pad, R 1024 | 83ef6900000001+pad
ScreenOn            : 79ef6900000001+pad | 96ef6900000001+pad, R 1024
RESTART             : 84ef6900000001+pad
SetOrientation(any) : 7def69000000050000002d00000000+pad

Full frame 5"   : 86ef6900000001+pad | 2c x250 | c8ef690017700e10+pad | 1,542,250 B of BGRA blocks, R 1024 | cfef6900000001+pad, R 1024
Full frame 2.1" : ... | c8ef69000e100e10+pad | 925,500 B ...
Full frame 8.8" : ... | c8ef690038400e10+pad | 3,701,250 B ...
```

Partial update of "grad" at (10,20), 5", ROM 87 (BGR), LANDSCAPE, count = 1, exact writes:

```
W 250: cc ef 69 00 00 00 1e 00 00 00 00 00 00 01 + 236 x 00
W 250: 00 3e 8a 00 03 00 00 ff 00 ff 00 ff 00 00 00 41 aa 00 03 ff ff ff 00 00 00 56 34 12 ef 69 + 220 x 00
W 250: cf ef 69 00 00 00 01 + 243 x 00
R 1024
```

(0x003E8A = 20 * 800 + 10, 0x0041AA = 21 * 800 + 10.)

Row payloads for "grad" at (10,20), ROM 87 (BGR). Header = `cc ef 69 00 | BE24 size | 00 00 00 | BE32 count`:

```
 2.1" P : size 0x23 | 036bb4 0002 ff0000 563412 | 036d94 0002 00ff00 000000 | 036f74 0002 0000ff ffffff | ef69
 2.1" L : size 0x1e | 00258a 0003 0000ff 00ff00 ff0000 | 00276a 0003 ffffff 000000 563412 | ef69
 2.1" RP: size 0x23 | 00148a 0002 ffffff 0000ff | 00166a 0002 000000 00ff00 | 00184a 0002 563412 ff0000 | ef69
 2.1" RL: size 0x1e | 035c93 0003 563412 000000 ffffff | 035e73 0003 ff0000 00ff00 0000ff | ef69
 5"   P : size 0x23 | 05b374 0002 ff0000 563412 | 05b694 ... | 05b9b4 ... | ef69
 5"   L : size 0x1e | 003e8a 0003 0000ff 00ff00 ff0000 | 0041aa 0003 ffffff 000000 563412 | ef69
 5"   RP: size 0x23 | 00224a 0002 ffffff 0000ff | 00256a ... | 00288a ... | ef69
 5"   RL: size 0x1e | 059a53 0003 563412 000000 ffffff | 059d73 ... | ef69
 8.8" P : size 0x1e | 0dee33 0003 563412 000000 ffffff | 0df013 0003 ff0000 00ff00 0000ff | ef69   (quirk 18.3)
 8.8" L : size 0x23 | 00148a 0002 ffffff 0000ff | 00166a ... | 00184a ... | ef69
 8.8" RP: size 0x1e | 00258a 0003 0000ff 00ff00 ff0000 | 00276a ... | ef69
 8.8" RL: size 0x23 | 0df7b4 0002 ff0000 563412 | 0df994 ... | 0dfb74 ... | ef69
```

`...` rows follow the same pattern as the complete rows above them (next address, same width, next pixels).
ROM 90 on 5" and 8.8": identical addresses with 4-byte BGRA pixels, for example 5" L, size 0x24:
`003e8a 0003 0000ffff 00ff00ff ff0000ff | 0041aa 0003 ffffffff 000000ff 563412ff | ef69`.
ROM 90 on 2.1": still BGR. Every partial update is followed by `cfef6900000001+pad, R 1024`.

Chunking: a 100 x 1 landscape row (305 raw bytes) becomes `249 bytes, 00, 56 bytes, ef 69` = 308 bytes, padded to 500.

### 17.2 Vendor (static, computed)

```
Full frame 8.8", brightness 170:
  2c x250 | 7bef6900000001000000aa+pad | c8ef6900384000000000+pad | 3,701,250 B
  | c8ef6900384000000000+pad | 3,701,250 B | R <= 1024
Partial, no change, seq 5:
  W 250: cc ef 69 00 00 00 08 00 00 00 00 00 00 05 00 00 00 00 + 232 x 00
  W 250: 80 00 00 00 00 00 ef 69 + 242 x 00
OPTIONS (brightness 170, start mode video, no flip, no sleep):
  7d ef 69 00 00 00 05 00 00 00 aa 02 00 00 00 + 235 x 00
ROTATION 90°:
  81 ef 69 00 00 00 01 00 00 00 01 + 239 x 00
GET_STORAGE_INFO : 64ef6900000001+pad, reply "a-b-c-d-e-f"
PRE / END update : 86ef6900000001+pad | 87ef6900000001+pad
LIST_DIR "/mnt/UDISK/video/":
  65 ef 69 00 00 00 11 00 00 00 2f 6d 6e 74 2f 55 44 49 53 4b 2f 76 69 64 65 6f 2f + 223 x 00
GET_FILE_SIZE "/mnt/SDCARD/video/":
  6e ef 69 00 00 00 12 00 00 00 2f 6d 6e 74 2f 53 44 43 41 52 44 2f 76 69 64 65 6f 2f + 222 x 00
PLAY_VIDEO "/mnt/SDCARD/video/88.mp4", loop:
  78 ef 69 00 00 00 18 01 00 00 2f 6d 6e 74 2f 53 44 43 41 52 44 2f 76 69 64 65 6f 2f 38 38 2e 6d 70 34 + 216 x 00
UPLOAD_FILE "/mnt/UDISK/video/88.mp4", 12,345,678 bytes (destructive):
  6f ef 69 00 00 00 17 00 00 00 2f 6d 6e 74 2f 55 44 49 53 4b 2f 76 69 64 65 6f 2f 38 38 2e 6d 70 34 4e 61 bc 00 + 213 x 00
MCU command (disruptive; written to the MCU port, not the SoC; not padded):
  00 00 00 00 00 c9
```

## 18. Quirks and known bugs

1. HELLO is unreliable (2.1" reports `chs_5inch`) and Python retries it forever until the answer starts with
   `chs_` (`lcd_comm_rev_c.py:224-242`).
2. Python's DISPLAY_BITMAP always carries `0e 10` after the size bytes (`display_width^2 / 64` with `display_width` =
   480 for every model), so the low length byte becomes 0x0E and the flag byte 0x10 where the vendor sends `00 00`
   (`lcd_comm_rev_c.py:359-361`; commits `d1826ea`, `d05b275`). The January 2025 code (and the stale golden files)
   sent `c8 ef 69 00 17 70` followed by zeros. Whether the firmware reads these bytes is unknown; Bezel sends the
   vendor form.
3. 8.8" PORTRAIT partial updates compute the column with `get_height()` (1920) instead of `get_width()` (480): the
   address gains 1440 = 3 native rows, i.e. the image is shifted by 3 px in user space and may overflow at the bottom
   (`lcd_comm_rev_c.py:409-412`). Unconfirmed on hardware. Recommendation: implement the geometrically consistent
   `W - x - w` and keep the Python value only as a compatibility vector until a hardware check decides.
4. Python inserts the `00` separator only when the raw rows exceed 250 bytes: a raw length of exactly 250 is not
   framed, and when the last chunk is exactly 249 bytes, `ef 69` lands where a separator would be
   (`lcd_comm_rev_c.py:463-465`). The vendor always frames. Whether the firmware requires the framing for short
   payloads is unknown.
5. FLIP_180 was toggled several times in Python; reverse orientations are software-only now.
6. BGR versus BGRA depends on ROM and model (commits `d720a80`, `964e420`, `d439a06`, `94ed336`, `8a694c0`). Only ROM
   87, 88 and 90 are known.
7. Wake-up requires opening the sleeping MCU port (possibly several times) until the SoC enumerates; the stale MCU
   entry may stay listed (commits `1ba10c2`, `719d348`, `901b3d4`).
8. `SetBrightness` bypasses the Python queue; the vendor does not serialise header and data phase (section 2.5).
9. The Python rev C unit tests error out (`AttributeError: ... no attribute 'sub_revision'`, because `_hello` is never
   called) and the rev C golden files are stale (recorded 2025-01-04 by `3537e32`: 5"-only code, DISPLAY_BITMAP
   followed by zeros, FLIP_180 for reverse orientations, BGR partials). Use section 17.1, not those files.
10. The vendor's timed-out reads stay pending and consume the next reply; it never discards input. Match replies by
    content.
11. The vendor's no-change dummy and overflow path (section 9.2).
12. The vendor's directory cleanup splits the LIST_DIR answer on `/`; an empty trailing name would make it send
    DELETE for the directory path itself.

## 19. Hardware observations

Confidence **hardware**: one Turing 8.8" rev C (SoC 0525:a4a7 and MCU 1a86:ca88 `CT88INCH` both enumerated), Linux
host, measured by the project. The rows from "Video after the host" to "Host shut down", and the last sentence of
TURNOFF, were measured on ROM 1.90 on 2026-10-03 (`D-2026-10-03-power-off-standby-1`).

| Observation | Detail |
|---|---|
| HELLO | answer `chs_88inch.dev1_rom1.90`: ROM 1.9, so raw BGRA partials (sections 4, 11) |
| Full frame | after a full 480 x 1920 BGRA frame the device replies `full_png_sucess` (sic) |
| Full-frame time | about 220 ms from the HELLO answer to that reply (about 17 MB/s for 3,701,250 wire bytes, **inferred** from the timing) |
| Partial updates | sent back to back, each followed by QUERY_STATUS, they are answered `needReSend:0\|renderCnt:0` roughly every 2.5 ms |
| Re-enumeration | when another program that was driving the screen stopped, the SoC gadget re-enumerated (new USB device number) within about 2 s |
| TURNOFF | 0x83 powers the SoC down: its gadget leaves the bus about 3 s later and only the MCU stays; turing-smart-screen-python sends it on exit. A wake right after takes about 17 s, after a few seconds of sleep about 11 s. The panel goes fully dark, backlight included, and stays off (observed for over 4 minutes); nothing requires the host to wait for the SoC to leave |
| Storage info | 0x64 on the 8.8": flash 65.9 MiB after the 512 KiB reserve; a 29.7 GiB FAT32 card reported in the TF fields |
| Uploads | PNG and MP4 to `/mnt/UDISK/{img,video}` and `/mnt/SDCARD/{img,video}` accepted and verified with GET_FILE_SIZE; `create_success` and `file_rev_done` as in section 13.4 |
| Playback | PLAY_VIDEO (loop) and PLAY_IMAGE answered; after playback a full frame (PRE_UPDATE_BITMAP + frame) is accepted (`full_png_sucess`); that the overlay's alpha shows the video through is still to be confirmed by eye |
| Cancelled upload | the data phase has no abort. With HELLO sent right after a cancel: in one run no HELLO was answered on that link and the next connection woke the screen (~10 s); the first upload afterwards received about 191 KB of stray bytes (caught by the size check). In a later run (a 41,573,338-byte upload to `/mnt/SDCARD/video` cancelled after 13,641,216 bytes, ROM 1.90) the first HELLO went unanswered and the one after the resync block was answered, yet GET_FILE_SIZE reported 12.6 MiB (less than the bytes accepted) and the next upload, a 7,444-byte PNG to `/mnt/SDCARD/img`, was stored as 390,157 bytes: about 382 KB of the cancelled data, still queued for the firmware's writer, went into that file. The card also counted the partial's size as used, even after it was deleted, until the screen rebooted. Completing the declared length hung the firmware: a 41.6 MB upload to the card cancelled after 16.6 MB, then right away the remaining 25 MB as filler blocks (`2c` x 249 + `00`); the firmware stopped reading after about 30.8 MB in all (data and filler), answered nothing and did not recover in 20 minutes; only a USB replug brought it back. A cancel without filler leaves a screen that reconnects on the next command and stray bytes that reach only the next upload, where the size check catches them, so Bezel sends no filler (section 13.4, D-2026-09-30-release-polish-10). |
| Host drain | writes of 64 KB to a card file can take longer than a 10 ms serial timeout to drain; a signal during the drain must not fail the write |
| Upload size | the firmware stops reading at exactly 29,577,216 bytes of an upload, at any rate, and hangs (section 13.4) |
| MCU restart | `00 00 00 00 00 c9` on the MCU port, held 8 s: the SoC left the bus within 0.2–1.4 s and returned under a new device number about 10 s later, answering HELLO; the same from the hung state above (three times), so no USB replug is needed |
| Video after the host | PLAY_VIDEO 0x78 with loop = 1 keeps playing after the host closes the port, and it replaces the frozen last frame of the PC stream: connecting, HELLO and PLAY_VIDEO took about 0.55 s in all |
| 0x87 after a video | 0x87 sent while a device video plays freezes the video's current frame; the screen does not switch to its start-mode content |
| Start mode 1 | after OPTIONS start mode 1 and a SoC restart, the firmware's carousel album (the vendor's zh label 轮播相册) shows every image of `/mnt/SDCARD/img/` in turn, every ~3–5 s, with no interval setting. imgFlip turns by 180° only, so pictures for a landscape screen must be stored already turned to the native 480 x 1920 |
| Start mode 2 | after OPTIONS start mode 2 and a SoC restart, the firmware plays the first entry of `/mnt/SDCARD/video/`. Playing another video with 0x78 first and waiting 75 s did not change it: the start mode is applied only when the SoC starts, and the file is the firmware's choice |
| RESTART 0x84 | one packet: the SoC left the bus in about 3 s and returned about 13 s later, already showing its start mode (the album above). The MCU restart, by contrast, needs its port held 8 s |
| Sleep timer | OPTIONS byte 14 counts idle time since the last host traffic. With a 1-minute timer, 150 s of streamed frames kept the screen awake; about 60–75 s after the stream stopped the SoC left the bus, as with TURNOFF. It also fires while the screen plays the album or a video on its own |
| Host shut down | a board that keeps USB powered in S5 leaves the screen frozen on the last frame after shutdown when the host sends nothing |

Consequences: full frames have a text reply of their own; the partial round trip is far below the vendor's 1 Hz tick;
a host must expect the gadget to come back under a new device number and re-open it by identity
([devices.md](devices.md) section 5.4), to wake a screen another program turned off, and not to cancel an upload
lightly: the firmware has no abort for the data phase. What a screen shows once its host is gone follows from the
last packets: a looping device video keeps playing, TURNOFF leaves it dark, a start mode shows only after a SoC
restart (0x84), the sleep timer turns it off after the idle time, and with nothing sent the last frame stays frozen
for as long as the USB is powered.

## 20. Open questions

1. HELLO's trailing `c5 d3`; the full HELLO answer beyond the id (Python reads 23 bytes, the vendor up to 1024).
2. Whether bytes 7..9 of the 0xC8 header must be zero (Python sends `0e 10` in bytes 6..7), and whether the firmware
   needs the vendor's full-frame ritual (brightness, frame sent twice) or Python's 0x86 / `2c` / 0xC8 suffices.
3. Semantics of 0x82, of 0x87 beyond freezing a device video's frame, and of the vendor's unused MCU command values
   (10, 11, 13, 14, 15, 40, 101, 201, 253); 0xC9 restarts the SoC (section 19).
4. Whether 0x81 and imgFlip affect streamed frames or only stored media; whether 0x81 persists; whether the 0x78
   loop flag survives a SoC restart (it survives the host closing the port, section 19).
5. How the screen wakes after TURNOFF (no host sends TURNON).
6. Whether the no-change dummy (6 bytes + `ef 69`) is valid in raw BGRA mode, where a single-pixel record is 7 bytes.
7. The text before `file:` in LIST_DIR answers, and whether long listings span several USB packets.
8. Whether 0x96 answers `media_stop` when nothing plays.
9. Storage units (KiB per the vendor's arithmetic).
10. Which USB control request actually wakes the MCU; whether `rtscts` / DTR matter to any firmware.
11. Endpoint layout of the models without a descriptor dump.
12. What start modes 1 and 2 show without a memory card (the internal `/mnt/UDISK/` folders?), and whether the
    "first entry" of `/mnt/SDCARD/video/` follows the directory order, the name or the age.
13. Which host packets reset the sleep timer: it was measured with streamed frames only.
