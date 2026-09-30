# Protocol: Turing rev A (Turing 3.5", UsbPCMonitor 3.5" / 5" / 7")

Reference: `library/lcd/lcd_comm_rev_a.py` of turing-smart-screen-python at `2b33ab4`.
Confidence: **static** for behaviour, **verified** for every vector in section 11.

## 1. Transport and detection

- USB CDC-ACM, VID:PID 1a86:5722 (shared with rev B and a sleeping rev C 5"; see [devices.md](devices.md) section 4).
- Open: 115200 baud, 8N1, `rtscts=True`, read timeout 1 s, blocking writes (pyserial asserts DTR on open).
- Python auto-detect: iSerial `USB35INCHIPSV2` or VID:PID 1a86:5722 (`lcd_comm_rev_a.py:67-77`).
- No checksums, no acknowledgements, nothing is read except the HELLO answer.
- Native resolution 320 x 480 portrait by default; 480 x 800 or 600 x 1024 after HELLO (section 4).

## 2. Command packet (6 bytes)

`SendCommand(cmd, x, y, ex, ey)` (`lcd_comm_rev_a.py:79-94`) packs four 10-bit fields MSB first, then the opcode:

```
byte0 = x >> 2
byte1 = ((x & 3) << 6)  + (y >> 4)
byte2 = ((y & 15) << 4) + (ex >> 6)
byte3 = ((ex & 63) << 2) + (ey >> 8)
byte4 = ey & 255
byte5 = cmd
```

That is the 40-bit big-endian bit string `x[9:0] y[9:0] ex[9:0] ey[9:0]` followed by the opcode. `ex`, `ey` are
**inclusive** end coordinates. Each field is 0..1023: 600 x 1024 fits, nothing larger can be addressed.

## 3. Command table (`lcd_comm_rev_a.py:32-47`)

| Opcode | Dec | Name | Arguments | Used by Python |
|---|---|---|---|---|
| 0x65 | 101 | RESET | 0,0,0,0 | `Reset()` |
| 0x66 | 102 | CLEAR (to white) | 0,0,0,0 | `Clear()` |
| 0x67 | 103 | TO_BLACK ("NOT TESTED") | - | no |
| 0x6C | 108 | SCREEN_OFF | 0,0,0,0 | `ScreenOff()` |
| 0x6D | 109 | SCREEN_ON | 0,0,0,0 | `ScreenOn()` |
| 0x6E | 110 | SET_BRIGHTNESS | x = level 0..255, **0 = brightest** | `SetBrightness()` |
| 0x79 | 121 | SET_ORIENTATION | 16-byte variant (section 6) | `SetOrientation()` |
| 0xC5 | 197 | DISPLAY_BITMAP | x0, y0, x1, y1 (inclusive) | `DisplayPILImage()` |
| 0x28 | 40 | LCD_28 ("?") | unknown | no |
| 0x29 | 41 | LCD_29 ("?") | unknown | no |
| 0x45 | 69 | HELLO | special: six `45` bytes | `InitializeComm()` |
| 0x7A | 122 | SET_MIRROR | unknown | no |
| 0xC3 | 195 | DISPLAY_PIXELS ("list of non-contiguous pixels") | format unknown | no |

## 4. Handshake: HELLO and sub-revision (`lcd_comm_rev_a.py:96-121`)

Send `45 45 45 45 45 45` directly (not queued), `read(6)` with the 1 s timeout, then discard pending input.

| 6-byte answer | Sub-revision | Resolution |
|---|---|---|
| `01 01 01 01 01 01` | USBMONITOR_3_5 | 320 x 480 |
| `02 02 02 02 02 02` | USBMONITOR_5 | 480 x 800 |
| `03 03 03 03 03 03` | USBMONITOR_7 | 600 x 1024 |
| anything else, or nothing | TURING_3_5 (the official Turing 3.5" does not answer HELLO) | 320 x 480 |

How the Turing 3.5" treats the six HELLO bytes is unknown (they are not a valid 6-byte command for it: opcode 0x45).

## 5. Brightness (`lcd_comm_rev_a.py:146-154`)

`level_absolute = int(255 - (level / 100) * 255)` for `level` in 0..100, sent in the `x` field of opcode 0x6E.
100 % -> 0, 50 % -> 127, 25 % -> 191, 1 % -> 252, 0 % -> 255. The Python config warns that rev A screens get hot at
high brightness (`config.yaml:68`); `configure.py:700` warns above 50 %.

## 6. Orientation: on-device, 16-byte packet (`lcd_comm_rev_a.py:156-176`)

```
[0..4]   00 00 00 00 00          packed x = y = ex = ey = 0
[5]      79                      opcode 121
[6]      orientation + 100       64 portrait, 65 reverse portrait, 66 landscape, 67 reverse landscape
[7..8]   BE16 width              width in the NEW orientation
[9..10]  BE16 height
[11..15] 00 00 00 00 00          padding to 16 bytes
```

The packet was 11 bytes before commit `44ecbdc` ("Fix rotation for newer models of Turing 3.5""): newer units need
16. Python writes it directly: it **bypasses the queue and the write-retry logic**.

After this command the device rotates by itself: bitmaps are always sent in current-orientation coordinates and pixel
order. The Python golden files show identical pixel payloads for landscape and reverse landscape.

## 7. Bitmap and partial update (`lcd_comm_rev_a.py:178-219`)

1. `w`, `h` default to the image size. Clip to the screen (`x + w > W` -> `w = W - x`; same for `y`) and crop the
   image to `(0, 0, w, h)`.
2. DISPLAY_BITMAP header with `x0 = x`, `y0 = y`, `x1 = x + w - 1`, `y1 = y + h - 1` (queued).
3. Pixel data: RGB565 **little-endian**, row-major, top-left first ([pixel-formats.md](pixel-formats.md) section 1),
   written in chunks of `W * 8` bytes (four display rows: 2560 B in portrait 320, 3840 B in landscape 480). Each
   chunk is a separate queue item / `write()`; the last one is shorter. No trailer, no acknowledgement.

There is no "full versus partial" distinction: every widget update is a DISPLAY_BITMAP of its bounding box.

## 8. Reset, Clear, Screen on/off

- `Reset()` (`lcd_comm_rev_a.py:126-133`): RESET immediately (bypasses the queue), close the port, **sleep 5 s**,
  re-open (the port name may change).
- `Clear()` (`lcd_comm_rev_a.py:135-138`): SetOrientation(PORTRAIT) (a code comment calls this a bug workaround:
  orientation must be portrait before clearing), CLEAR (queued), then SetOrientation(PORTRAIT) again. The previous
  orientation is **not** restored, and because SetOrientation bypasses the queue while CLEAR is queued, the intended
  order is not guaranteed.
- `ScreenOff()` / `ScreenOn()`: opcodes 0x6C / 0x6D with zero coordinates.
- No backplate LEDs.

## 9. Timing, flow control, threading

- No acknowledgement is ever read; the host paces nothing. Only the OS serial buffer and USB flow control apply.
- Python queues work items on one consumer thread. The DISPLAY_BITMAP header and its data chunks are enqueued under
  two separate mutex acquisitions (commit `6506e0c`, a self-deadlock fix), so another thread's command can slip in
  between header and data. Bezel must send header and data as one uninterrupted sequence.
- On macOS Python flushes after every write (bitmap corruption otherwise, issue #7).
- Startup order side effect in Python: ScreenOn and SetBrightness are queued while SetOrientation bypasses the queue,
  so the orientation packet reaches the device first.
- `RESET_ON_STARTUP` (default true) makes the screen disconnect and re-enumerate; on rev A this can change the port
  name (`config.yaml:76-80`).

## 10. Quirks and known issues

| Item | Evidence |
|---|---|
| Clear needs portrait first and leaves the screen in portrait | `lcd_comm_rev_a.py:135-138` |
| Newer 3.5" units need the 16-byte orientation packet | commit `44ecbdc` |
| Header and data are not atomic in the Python queue | commit `6506e0c` |
| 10-bit coordinate fields; Python raises for values >= 1024 | section 2 |
| UsbPCMonitor 7" (600 x 1024) is detected but no theme size exists | `lcd_comm_rev_a.py:112-115`, `display.py:58-84` |
| Unknown payload formats: opcodes 0x28, 0x29, 0x67, 0x7A, 0xC3 | section 3 |

## 11. Test vectors (verified)

```
SetBrightness(0)   : 3f c0 00 00 00 6e
SetBrightness(1)   : 3f 00 00 00 00 6e
SetBrightness(25)  : 2f c0 00 00 00 6e
SetBrightness(50)  : 1f c0 00 00 00 6e
SetBrightness(100) : 00 00 00 00 00 6e
ScreenOff          : 00 00 00 00 00 6c
ScreenOn           : 00 00 00 00 00 6d
RESET              : 00 00 00 00 00 65
CLEAR              : 00 00 00 00 00 66
HELLO              : 45 45 45 45 45 45   then R 6

SetOrientation, 320 x 480 panel:
  PORTRAIT          : 00000000007964014001e00000000000
  LANDSCAPE         : 0000000000796601e001400000000000
  REVERSE_PORTRAIT  : 00000000007965014001e00000000000
  REVERSE_LANDSCAPE : 0000000000796701e001400000000000
SetOrientation PORTRAIT, 600 x 1024 panel:
                      00000000007964025804000000000000

grad (3x2) at (10,20), portrait:
  02 81 40 30 15 c5 | 00f8 e007 1f00 ffff 0000 aa11
600 x 1024 panel, 3x2 all-red image at (597,1022):
  95 7f e9 5f ff c5 | 00f8 00f8 00f8 00f8 00f8 00f8
Full 320 x 480 frame, portrait (golden file):
  16-byte orientation packet, 00 00 04 fd df c5, then 120 writes of 2560 bytes
```

Golden file: `tests/library/lcd/golden/rev_a_set_brightness.txt` holds the default-brightness vector.
