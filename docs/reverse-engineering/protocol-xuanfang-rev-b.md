# Protocol: XuanFang rev B (3.5" rev B and "flagship")

Reference: `library/lcd/lcd_comm_rev_b.py` of turing-smart-screen-python at `2b33ab4`.
Confidence: **static** for behaviour, **verified** for every vector in section 10.

## 1. Transport and detection

- USB CDC-ACM, VID:PID 1a86:5722 (shared with rev A and a sleeping rev C 5"). Descriptors: iManufacturer `江苏沁恒`
  (WCH), iProduct `XFZX`, iSerial `2017-2-25`, **bcdUSB 1.10, EP0 8 bytes**, 100 mA; bulk OUT 0x02, IN 0x82 (64).
- Open: 115200 8N1, `rtscts=True`, read timeout 1 s.
- Python auto-detect: iSerial `2017-2-25` or VID:PID 1a86:5722 (`lcd_comm_rev_b.py:70-80`).
- Resolution 320 x 480 portrait.

## 2. Packet format (10 bytes)

```
[0]     cmd
[1..8]  payload, zero-padded to 8 bytes
[9]     cmd (repeated)
```

No checksum (`lcd_comm_rev_b.py:82-107`).

## 3. Command table

| Opcode | Name | Payload | Used by Python |
|---|---|---|---|
| 0xCA | HELLO | `48 45 4c 4c 4f` ("HELLO") + 3 x 00 | `InitializeComm()` |
| 0xCB | SET_ORIENTATION | [0] = 0 portrait / 1 landscape | `SetOrientation()` |
| 0xCC | DISPLAY_BITMAP | BE16 x0, BE16 y0, BE16 x1, BE16 y1 (inclusive) | `DisplayPILImage()` |
| 0xCD | SET_LIGHTING (backplate RGB LEDs) | [0] = R, [1] = G, [2] = B | flagship only |
| 0xCE | SET_BRIGHTNESS | [0] = level | `SetBrightness()` |

## 4. Handshake: HELLO (`lcd_comm_rev_b.py:109-139`)

Send `ca 48 45 4c 4c 4f 00 00 00 ca` directly, `read(10)`, discard pending input. Expected answer:

```
ca 48 45 4c 4c 4f 0a <sub> <?> ca
```

Framing or echo mismatches only log warnings; an empty answer raises an assertion. `answer[6] == 0x0A` and
`answer[7]` selects the sub-revision:

| answer[7] | Sub-revision | Meaning |
|---|---|---|
| 0x01 | A01 | rev B, brightness on/off only |
| 0x02 | A02 | flagship, brightness on/off only |
| 0x11 | A11 | rev B, brightness 0..255 |
| 0x12 | A12 | flagship, brightness 0..255 |

Default before HELLO: A01 (`lcd_comm_rev_b.py:59`). Flagship = A02/A12 (`is_flagship`, line 64); ranged brightness =
A11/A12 (`is_brightness_range`, line 67). The original author's flagship answered `... 0a 12 00` (commit `3f0b334`).
The meaning of `answer[8]` is unknown (0x00 observed).

## 5. Brightness (`lcd_comm_rev_b.py:168-180`)

- A11 / A12: `int(level / 100 * 255)`; 255 is brightest.
- A01 / A02: `1 if level == 0 else 0` (1 = off, 0 = full brightness), with an info log.

## 6. Backplate LEDs (`lcd_comm_rev_b.py:182-187`)

Flagship only: `cd R G B 00 00 00 00 00 cd`. Python sends the theme's `DISPLAY_RGB_LED` at start and `(0, 0, 0)`
when stopping.

## 7. Orientation (`lcd_comm_rev_b.py:189-196`, `229-235`)

- PORTRAIT and REVERSE_PORTRAIT send `cb 00 ...`; LANDSCAPE and REVERSE_LANDSCAPE send `cb 01 ...`.
- The reverse variants are **software**: the image is rotated 180° and the rectangle mirrored, with `W`, `H` the
  current-orientation size:

```
x0 = W - x - w      y0 = H - y - h
x1 = W - x - 1      y1 = H - y - 1
```

## 8. Bitmap (`lcd_comm_rev_b.py:198-256`)

1. `w`, `h` default to the image size; if the image is larger than the screen, clamp to the screen size (the x/y
   offset is not considered).
2. DISPLAY_BITMAP header (queued).
3. Serialise: crop to `(0, 0, w, h)`, rotate 180° for reverse orientations, RGB565 **big-endian**
   ([pixel-formats.md](pixel-formats.md) section 1).
4. Data in chunks of `W * 8` bytes under the queue mutex.
5. **Cooldown**: after each bitmap Python queues `time.sleep(0.05)` (or sleeps inline without a queue). The comment
   says this reduces corrupted bitmaps "because we are not listening to events coming from the display"
   (commit `953daec`).

History: the 2022 code packed pixels as "`0bgggBBBBBRRRRRGGG` little-endian", which is simply RGB565 big-endian
(commit `dc2fc56`).

## 9. Reset, Clear, Screen on/off, timing, quirks

- No native reset, clear or on/off commands.
- `Reset()` = `Clear()`. `Clear()` = SetOrientation(PORTRAIT), full-screen white bitmap, restore the previous
  orientation (`lcd_comm_rev_b.py:144-158`).
- `ScreenOff()` = `SetBrightness(0)`; `ScreenOn()` = `SetBrightness()` (default 25) (lines 160-166).
- Flow control: no acknowledgement is read; the 50 ms per-bitmap cooldown is the only pacing.
- Quirk: as on rev A, the bitmap header and its data are enqueued under separate mutex acquisitions in Python; Bezel
  must send them atomically.
- Quirk: A01/A02 units can only be switched fully on or off.

## 10. Test vectors (verified)

```
HELLO                        : ca 48 45 4c 4c 4f 00 00 00 ca   then R 10
A01 SetBrightness(0)         : ce 01 00 00 00 00 00 00 00 ce
A01 SetBrightness(25)        : ce 00 00 00 00 00 00 00 00 ce
A01 SetBrightness(100)       : ce 00 00 00 00 00 00 00 00 ce
A12 SetBrightness(0)         : ce 00 00 00 00 00 00 00 00 ce
A12 SetBrightness(25)        : ce 3f 00 00 00 00 00 00 00 ce
A12 SetBrightness(100)       : ce ff 00 00 00 00 00 00 00 ce
A12 SetBackplateLedColor(0x11,0x22,0x33)
                             : cd 11 22 33 00 00 00 00 00 cd
SetOrientation P / RP        : cb 00 00 00 00 00 00 00 00 cb
SetOrientation L / RL        : cb 01 00 00 00 00 00 00 00 cb

grad (3x2) at (10,20):
  PORTRAIT and LANDSCAPE : cc 00 0a 00 14 00 0c 00 15 cc | f800 07e0 001f ffff 0000 11aa
  REVERSE_PORTRAIT       : cc 01 33 01 ca 01 35 01 cb cc | 11aa 0000 ffff 001f 07e0 f800
  REVERSE_LANDSCAPE      : cc 01 d3 01 2a 01 d5 01 2b cc | 11aa 0000 ffff 001f 07e0 f800
  (each followed by a 50 ms pause)

Full 320 x 480 frame, portrait (golden file):
  cb 00 .. cb, cc 00 00 00 00 01 3f 01 df cc, then 120 writes of 2560 bytes
```
