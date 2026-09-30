# Protocol: Turing rev C (2.1" / 2.8" / 5" / 8.8", UART generation)

Reference: `library/lcd/lcd_comm_rev_c.py` of turing-smart-screen-python at `2b33ab4`.
Confidence: **static** for behaviour, **verified** for the vectors in section 13, **inferred** where stated.

These screens pair a small MCU that enumerates while the screen sleeps with a Linux (sunxi) or Android SoC that
enumerates as a CDC-ACM gadget when awake. All protocol traffic goes to the SoC port.
Detection, the MCU/SoC pairing and the wake procedure are in [devices.md](devices.md) section 5.

## 1. Transport

- Awake SoC: 0525:a4a7 (8.8"), 1d6b:0121 (2.1"), 1d6b:0106 (5"), or any port with iSerial `20080411`.
  CDC-ACM with IAD; bulk IN 0x81 and OUT 0x01 with 512-byte packets (high speed).
- Open: 115200 8N1, `rtscts=True`, read timeout 1 s.
- There are no checksums. Every read result is discarded; reads are used only for pacing.
- The Python constructor takes width and height from the theme (`library/display.py:97-100`) "because of issue with
  Turing rev. C size auto-detection".

## 2. Framing (`lcd_comm_rev_c.py:183-213`)

`_send_command(cmd, payload, padding, bypass_queue, readsize)`:

1. message = command bytes (omitted for SEND_PAYLOAD) + payload;
2. if `len(message) % 250 != 0`, pad to the next multiple of 250 with the padding byte (0x00, or 0x2C for
   START_DISPLAY_BITMAP);
3. write the whole message in **one** `write()` call; if `readsize`, then `read(readsize)` (content ignored).

Commands are queued unless `bypass_queue`. Callers wrap multi-message sequences in the queue mutex.

Header structure (**inferred**, consistent with every command):

```
[0]      opcode
[1..2]   ef 69
[3..6]   BE32 length of the command's payload
[7..9]   00 00 00
[10..]   inline payload
```

For example SET_BRIGHTNESS has length 1 and the level at byte 10; OPTIONS has length 5 with bytes 10..14;
DISPLAY_BITMAP `00 17 70 00` is BE32 1,536,000 = 480 x 800 x 4, the BGRA frame size of the 5". See quirk 14.2 for
the byte that the current Python code overwrites.

A code comment gives the protocol order (`lcd_comm_rev_c.py:43-63`): "READ HELLO ALWAYS IS 23. ALL READS IS 1024".

## 3. Command table (`lcd_comm_rev_c.py:65-95`)

All commands are padded to 250 bytes with 0x00 unless stated.

| Name | Bytes before padding | Extra payload | Read after | Python use |
|---|---|---|---|---|
| HELLO | `01 ef 69 00 00 00 01 00 00 00 c5 d3` | - | 23 | `_hello` |
| OPTIONS | `7d ef 69 00 00 00 05 00 00 00 2d` | `[startmode] [00] [flip] [sleep]` | - | `SetOrientation` |
| RESTART | `84 ef 69 00 00 00 01` | - | - | `Reset` |
| TURNOFF | `83 ef 69 00 00 00 01` | - | - | `ScreenOff` |
| TURNON | `83 ef 69 00 00 00 00` | - | - | **never sent** |
| SET_BRIGHTNESS | `7b ef 69 00 00 00 01 00 00 00` | `[level 0..255]` | - | `SetBrightness` (bypasses the queue) |
| STOP_VIDEO | `79 ef 69 00 00 00 01` | - | - | `ScreenOn`, `ScreenOff` |
| STOP_MEDIA | `96 ef 69 00 00 00 01` | - | 1024 | `ScreenOn`, `ScreenOff` |
| QUERY_STATUS | `cf ef 69 00 00 00 01` | - | 1024 | after every bitmap |
| PRE_UPDATE_BITMAP | `86 ef 69 00 00 00 01` | - | - | full frame |
| START_DISPLAY_BITMAP | `2c` | none; padded with **0x2C**: 250 x `2c` | - | full frame |
| DISPLAY_BITMAP_2INCH | `c8 ef 69 00 0e 10` | `BE16(480 * 480 / 64)` = `0e 10` | - | full frame, 2.1" / 2.8" |
| DISPLAY_BITMAP_5INCH | `c8 ef 69 00 17 70` | `0e 10` | - | full frame, 5" |
| DISPLAY_BITMAP_8INCH | `c8 ef 69 00 38 40` | `0e 10` | - | full frame, 8.8" |
| UPDATE_BITMAP | `cc ef 69 00` | see section 9 | - | partial update header |
| SEND_PAYLOAD | none | raw data | optional | data blocks |

OPTIONS field values: start mode `00` default / `01` image / `02` video; flip `01` FLIP_180 / `00` NO_FLIP; sleep
interval `00` (off) to `0a` (ten). Python only ever sends start mode default, NO_FLIP and sleep off.

## 4. Handshake: HELLO and model (`lcd_comm_rev_c.py:215-254`)

1. Discard pending input; send HELLO (bypassing the queue); `read(23)`; keep only printable ASCII; discard input.
2. While the answer does not start with `chs_`: sleep 1 s and resend. **Python loops forever.**
3. Known answers (from the Python history): 5" `chs_5inch.dev1_rom1.87`; 2.1" `chs_5inch.dev1_rom1.88` (sic: 2.1"
   units report `5inch`); 8.8" `chs_88inch.dev1_rom1.88` and `chs_88inch.dev1_rom1.90` (commits `8c26266`, `d720a80`).
   The vendor app shows the same kind of string as the "ROM" line of its device page.
4. Sub-revision comes from the **configured** size, not the answer: 480x480 -> REV_2INCH (2.1" and 2.8"),
   480x800 -> REV_5INCH, 480x1920 -> REV_8INCH, anything else logs an error.
5. ROM version = `int(answer.split(".")[2])` (87, 88, 90, ...). If unparsable or outside 80..100, 87 is assumed.

Bezel should bound the retry loop, and may parse the model from the string only as a hint.

## 5. Brightness (`lcd_comm_rev_c.py:299-307`)

`int(level / 100 * 255)` for level 0..100, sent as `7b ef 69 00 00 00 01 00 00 00 <L>` + pad250.
255 is brightest. Python writes it immediately from the calling thread (bypassing the queue).

## 6. Orientation and OPTIONS (`lcd_comm_rev_c.py:309-318`)

`SetOrientation(o)` always sends OPTIONS `7d ef 69 00 00 00 05 00 00 00 2d 00 00 00 00` + pad250 (start mode
default, NO_FLIP, sleep off), queued. The FLIP_180 variant for reverse orientations is commented out; it was
enabled and disabled several times (commits `c09d0a5`, `be7bf45`, `6a5d69d`, `d720a80`). **All rotation is done in
software** (section 10).

**Inferred:** the vendor app's device page saves a 5-byte settings payload `[brightness, startMode, 0, imgFlip,
sleepDelay]` with command 125 (= 0x7D, the OPTIONS opcode). Aligned with the Python layout, byte 10 (`2d` in Python,
i.e. 45/255) would be a brightness value, byte 11 the start mode, byte 13 the flip flag and byte 14 the sleep delay.
Unconfirmed on hardware.

## 7. Screen on/off, reset, clear

- `ScreenOff()`: STOP_VIDEO; STOP_MEDIA + `read(1024)`; TURNOFF (`lcd_comm_rev_c.py:287-291`).
- `ScreenOn()`: STOP_VIDEO; STOP_MEDIA + `read(1024)`. TURNON is never sent and the brightness restore is commented
  out (`lcd_comm_rev_c.py:293-297`). How the panel wakes after TURNOFF is not established.
- `Reset()`: RESTART (bypassing the queue), close the port, wait up to 15 s while an awake port is still listed, wait
  up to 15 s while none is listed, then re-open (which re-runs the wake-up detection on AUTO)
  (`lcd_comm_rev_c.py:259-273`).
- `Clear()`: SetOrientation(PORTRAIT), full white frame, restore the previous orientation (`lcd_comm_rev_c.py:275-285`).

## 8. Full-frame update (`lcd_comm_rev_c.py:347-365`, `374-395`)

Taken when `x == 0 and y == 0 and w == W and h == H`. Under the queue mutex:

1. PRE_UPDATE_BITMAP `86 ef 69 00 00 00 01` + pad250.
2. START_DISPLAY_BITMAP: 250 x `2c`.
3. DISPLAY_BITMAP_xINCH + `0e 10` + pad250 (5": `c8 ef 69 00 17 70 0e 10` + 242 x `00`).
4. SEND_PAYLOAD: rotate the image to the native orientation (section 10), convert to **BGRA, 4 bytes per pixel**
   (alpha 0xFF for RGB sources), split into 249-byte chunks joined by a single `00` byte (so every 250-byte block is
   249 data bytes + `00`), pad250 with zeros, one `write()`, then `read(1024)`.
   Sizes: 2.1" 925,500 B; 5" 1,542,250 B; 8.8" 3,701,250 B.
5. QUERY_STATUS + pad250, `read(1024)`.

## 9. Partial update (`lcd_comm_rev_c.py:366-372`, `397-467`)

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

Each row record has the same layout as a run record of the vendor's run-list format
([pixel-formats.md](pixel-formats.md) section 6): linear native index, pixel count, pixels.

History: the size field was 2 bytes until 2023 (commits `ebc32cd`, `5301977`: overflow with big fonts); an earlier
workaround split images over 0xFF00 bytes (`e971733`, removed).

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
| PORTRAIT | rotate 180 | H - y - h | **H - x - w** (suspected bug, quirk 14.3) |

Full frames use the same rotations (2.1"/5": PORTRAIT 90 CCW, REVERSE_PORTRAIT 270, REVERSE_LANDSCAPE 180;
8.8": LANDSCAPE 270, REVERSE_LANDSCAPE 90, PORTRAIT 180).

## 11. Pixel formats

- Full frames: BGRA `[B, G, R, A]`.
- Partial updates: BGR `[B, G, R]` for 2.1"/2.8" and for ROM <= 88; BGRA for 5"/8.8" with ROM > 88.
- The 3-byte "compressed BGRA" helper (`image_to_compressed_BGRA`) exists but is unused: it was used briefly for
  ROM <= 88 (`d439a06`) then replaced by plain BGR (`94ed336`: "because this program does not support transparent
  background"). With plain BGR the low two bits of B and G carry whatever the colour has, which a firmware that
  reads them as alpha would treat as partial transparency ([pixel-formats.md](pixel-formats.md) section 4).

## 12. Timing, flow control, threading

- Every bitmap ends with QUERY_STATUS and a blocking `read(1024)` (1 s timeout); full frames also read 1024 bytes
  after the payload; STOP_MEDIA reads 1024. These reads are the effective pacing; their content is ignored.
- Full-frame and partial sequences are atomic under the Python queue mutex. `SetBrightness` is not: it writes from
  the caller's thread and can interleave with a large payload being written by the consumer thread.
- Per-update overhead of a partial update: 3 x 250-byte blocks, 5 bytes per row, 1 byte per 249 data bytes, and one
  1024-byte read.

## 13. Test vectors (verified)

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
 8.8" P : size 0x1e | 0dee33 0003 563412 000000 ffffff | 0df013 0003 ff0000 00ff00 0000ff | ef69   (quirk 14.3)
 8.8" L : size 0x23 | 00148a 0002 ffffff 0000ff | 00166a ... | 00184a ... | ef69
 8.8" RP: size 0x1e | 00258a 0003 0000ff 00ff00 ff0000 | 00276a ... | ef69
 8.8" RL: size 0x23 | 0df7b4 0002 ff0000 563412 | 0df994 ... | 0dfb74 ... | ef69
```

`...` rows follow the same pattern as the complete rows above them (next address, same width, next pixels).
ROM 90 on 5" and 8.8": identical addresses with 4-byte BGRA pixels, for example 5" L, size 0x24:
`003e8a 0003 0000ffff 00ff00ff ff0000ff | 0041aa 0003 ffffffff 000000ff 563412ff | ef69`.
ROM 90 on 2.1": still BGR. Every partial update is followed by `cfef6900000001+pad, R 1024`.

Chunking: a 100 x 1 landscape row (305 raw bytes) becomes `249 bytes, 00, 56 bytes, ef 69` = 308 bytes, padded to 500.

## 14. Quirks and known bugs

1. HELLO is unreliable (2.1" reports `chs_5inch`) and Python retries it forever until the answer starts with
   `chs_` (`lcd_comm_rev_c.py:224-242`).
2. DISPLAY_BITMAP always carries `0e 10` after the size bytes (`display_width^2 / 64` with `display_width` = 480 for
   every model), so under the inferred header the 4th length byte becomes 0x0E instead of 0x00
   (`lcd_comm_rev_c.py:359-361`; commits `d1826ea`, `d05b275`). The January 2025 code (and the stale golden files)
   sent `c8 ef 69 00 17 70` followed by zeros. Whether firmware reads this byte is unknown.
3. 8.8" PORTRAIT partial updates compute the column with `get_height()` (1920) instead of `get_width()` (480): the
   address gains 1440 = 3 native rows, i.e. the image is shifted by 3 px in user space and may overflow at the bottom
   (`lcd_comm_rev_c.py:409-412`). Unconfirmed on hardware. Recommendation: implement the geometrically consistent
   `W - x - w` and keep the Python value only as a compatibility vector until a hardware check decides.
4. The `00` separator is inserted only when the raw rows exceed 250 bytes: a raw length of exactly 250 is not framed,
   and when the last chunk is exactly 249 bytes, `ef 69` lands where a separator would be
   (`lcd_comm_rev_c.py:463-465`). Whether the firmware requires the framing for short payloads is unknown.
5. FLIP_180 was toggled several times; reverse orientations are software-only now.
6. BGR versus BGRA depends on ROM and model (commits `d720a80`, `964e420`, `d439a06`, `94ed336`, `8a694c0`). Only ROM
   87, 88 and 90 are known.
7. Wake-up requires opening the sleeping MCU port (possibly several times) until the SoC enumerates; the stale MCU
   entry may stay listed (commits `1ba10c2`, `719d348`, `901b3d4`).
8. `SetBrightness` bypasses the queue.
9. The Python rev C unit tests error out (`AttributeError: ... no attribute 'sub_revision'`, because `_hello` is never
   called) and the rev C golden files are stale (recorded 2025-01-04 by `3537e32`: 5"-only code, DISPLAY_BITMAP
   followed by zeros, FLIP_180 for reverse orientations, BGR partials). Use section 13, not those files.

## 15. Open questions

1. Header semantics: the `[op] ef 69 [BE32 len] 00 00 00` structure, HELLO's trailing `c5 d3`, OPTIONS' `2d`, and
   the effect of the extra `0e 10`.
2. The 23-byte HELLO answer beyond the ID string, and the 1024-byte answers to QUERY_STATUS / STOP_MEDIA / full
   frames (frame counter? error status?).
3. How the screen wakes after TURNOFF; meaning of the start modes and sleep interval.
4. Which USB control request actually wakes the MCU (the tty open performs SET_LINE_CODING and DTR/RTS changes).
5. Whether `rtscts` / DTR matter to any firmware.

## 16. Vendor-app observations (partial, static)

Facts about the vendor application's serial transport that surfaced in other parts of the analysis. They are
consistent with the Python protocol but not yet consolidated:

- Brightness slider 1..255 (default 170) is sent live with command 123 (= 0x7B, SET_BRIGHTNESS).
- The device page's Save sends command 125 (= 0x7D) with `[brightness, startMode, 0, imgFlip, sleepDelay]`
  (section 6). Other commands used by stop/play/reset flows (names as in the Python table): 121 (= 0x79, STOP_VIDEO),
  131 (= 0x83, TURNOFF/ON), 132 (= 0x84, RESTART); 129 (= 0x81) carries an unknown one-byte setting (possibly
  orientation; unconfirmed).
- Partial updates use run lists of changed pixels between consecutive frames: raw 4-byte BGRA pixels when the ROM
  version is >= 1.89, compressed 3-byte BGRA pixels below. An opacity run list (POSLEN) is sent with opcode 208
  (0xD0). Formats in [pixel-formats.md](pixel-formats.md) sections 6-8.
- The device page reports internal-flash and TF-card storage (total / used / free) and the ROM string; background
  videos are converted to MP4 and stored on the device ([video.md](video.md)).
- Wake-up and re-enumeration behaviour: [devices.md](devices.md) section 5.2.

## TURZX additions

_Pending: consolidated from the TURZX serial analysis._
