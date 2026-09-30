# Pixel formats, run lists and rotation

Every byte layout a Bezel encoder produces, with the model-specific rotation and address formulas.
Python references point to `library/lcd/serialize.py` and the LCD classes at `2b33ab4`.

## 1. RGB565 (rev A, rev B, rev D, WeAct)

`image_to_RGB565(image, endianness)` (`serialize.py:13-40`), **verified**:

```
v = ((R >> 3) << 11) | ((G >> 2) << 5) | (B >> 3)
```

- Emitted as `<u2` (little-endian: rev A, WeAct) or `>u2` (big-endian: rev B, rev D). Row-major, top row first.
- Images not in RGB/RGBA mode are converted to RGB first. For RGBA the **alpha is ignored** (no compositing: the raw
  RGB of transparent pixels is sent).

| Colour | Value | LE bytes | BE bytes |
|---|---|---|---|
| red `(255,0,0)` | 0xF800 | `00 f8` | `f8 00` |
| green `(0,255,0)` | 0x07E0 | `e0 07` | `07 e0` |
| blue `(0,0,255)` | 0x001F | `1f 00` | `00 1f` |
| white | 0xFFFF | `ff ff` | `ff ff` |
| black | 0x0000 | `00 00` | `00 00` |
| `#123456` | 0x11AA | `aa 11` | `11 aa` |

## 2. BGR, 3 bytes per pixel

`image_to_BGR` (`serialize.py:43-50`), **verified**: `[B, G, R]`, alpha dropped. Used by Python's rev C partial updates
on 2.1"/2.8" and on ROM <= 88 (the vendor app sends compressed BGRA there instead, section 4).

The WCH family also uses BGR888 ([protocol-wch.md](protocol-wch.md) section 7.1): GDI+ 24-bit locked bits, rows top
to bottom, stride `(w * 3 + 3) & ~3` (no padding at the WCH resolutions). **static**.

## 3. BGRA, 4 bytes per pixel

`image_to_BGRA` (`serialize.py:53-59`), **verified**: converts to RGBA if needed (alpha 255 for RGB sources), bytes
`[B, G, R, A]`. Used by rev C full frames and by Python's rev C partial updates on 5"/8.8" with ROM > 88. The vendor
app uses it for rev C full frames (0xC8 / 0xCA) and for partial run lists on its "large screens" (4", 6.5", 6.8", 8",
8.8") with ROM >= 1.89 ([protocol-turing-rev-c.md](protocol-turing-rev-c.md) section 11).

The vendor application extracts frames with GDI+ `Format32bppArgb` locked bits: in memory `B, G, R, A`,
non-premultiplied, top-down, row-major; stride assumed to be `width * 4`. **static**.

## 4. Compressed BGRA, 3 bytes per pixel

Used by the vendor app's serial run-list encoders whenever a 3-byte pixel is emitted: rev C partial updates on every
small screen (2.1"/2.8", 2.4", 2.8" square, 3.4", 5") and on large screens with ROM < 1.89 (**static**):

```
a4    = A >> 4                      (0..15; some encoders force a4 = 15)
byte0 = (B & 0xFC) | (a4 >> 2)      6-bit blue  + alpha bits 3..2
byte1 = (G & 0xFC) | (a4 & 0x03)    6-bit green + alpha bits 1..0
byte2 = R                           8-bit red
```

Worked examples (static, computed) for B=0x12, G=0x34, R=0x56:

| A | a4 | bytes |
|---|---|---|
| 0xFF | 15 | `13 37 56` |
| 0x80 | 8 | `12 34 56` |
| 0x00 | 0 | `10 34 56` |

The Python helper `image_to_compressed_BGRA` (`serialize.py:63-74`, unused) writes `(G & 0xFC) | (a4 & 2)` for byte1,
probably meant `a4 & 3`; for A = 0xFF it gives `13 36 56`. Bezel follows the vendor formula.

Consequence for plain BGR on rev C: the vendor app sends every 3-byte partial pixel in this form, so rev C firmware
reads the low two bits of B and G as alpha (static; not yet confirmed on hardware), and a plain BGR pixel is partially
transparent unless those bits are set. An encoder that wants guaranteed-opaque 3-byte output must OR `0x03` into
byte0 and byte1 (a4 = 15).

## 5. Rev C row runs (Python partial update)

**verified** ([protocol-turing-rev-c.md](protocol-turing-rev-c.md) section 9):

```
per image row r:   BE24(address) BE16(image_width) pixel bytes (BGR or BGRA)
address = (row0 + r) * rowlen + col0          (native linear pixel index)
concatenation > 250 bytes -> 249-byte chunks joined by 00
then ef 69, pad250
header block: cc ef 69 00 | BE24(len(rows) + 2) | 00 00 00 | BE32(count)
```

## 6. Run list with the single-pixel flag (vendor serial encoders)

**static**. Positions are linear pixel indices `idx = y * width + x` in the **native** panel orientation (for example
480 wide x 1920 high on the 8.8"):

```
Run record (count >= 2; in POSLEN lists any count):
  +0  BE24  idx              bit 23 = 0
  +3  BE16  count            pixels in the run, <= 65000
  +5  count x pixel          3-byte compressed BGRA or 4-byte BGRA, per encoder

Single-pixel record (count == 1):
  +0  BE24  idx | 0x800000   bit 23 set (byte0 |= 0x80)
  +3  1 x pixel              no count field
```

- A run that closes with count 1 is converted in place: byte0 gets `| 0x80`, the pixel bytes move left by 2 over the
  count field, and the write cursor goes back by 2.
- Largest index 2^23 - 1 = 8,388,607 (enough for 1080 x 2320 = 2,505,600). Largest run 65,000 pixels; on overflow the
  encoder logs `cnt overflow:` and returns nothing. The vendor's caller then fails, reconnects and sends a full frame;
  Bezel sends a full frame directly.
- The Python rev C row record (section 5) is the same layout: a run per image row.

## 7. Diff encoders (vendor serial transport)

**static**. Both compare the previous and the current frame, pixel by pixel (all four BGRA bytes), and emit changed
pixels as run records:

```
for idx in 0 .. pixel_count-1:
    if cur[idx] != prev[idx]:
        if not in_run: write BE24 idx; reserve 2 count bytes; in_run = true
        append pixel; run_len += 1
    else if in_run:
        if run_len == 1: convert to a single-pixel record (section 6)
        else:            write BE16 run_len into the reserved count bytes
        if run_len > 65000: fail ("cnt overflow")
        in_run = false; run_len = 0
at the end: if in_run: write BE16 run_len (NO single-pixel conversion, even for 1)
```

| Variant | Pixel | Selection in the vendor app |
|---|---|---|
| raw | 4 bytes `B G R A` | large screen (4", 6.5", 6.8", 8", 8.8") and ROM version >= 1.89 |
| compressed | 3 bytes (section 4) with `a4 = A >> 4` | every other rev C case (small screens, or ROM < 1.89) |

The output buffer is sized to the frame length; a diff larger than the frame fails like an overflow (section 6).

Worked example (static, computed). Frame 4 x 2 (8 pixels), previous frame all `00 00 00 00`; current frame changed at
idx 1 = `01 02 03 ff`, idx 2 = `04 05 06 ff`, idx 5 = `07 08 09 ff`, idx 7 = `0a 0b 0c 80` (B G R A):

```
raw (4-byte):
  00 00 01 00 02 01 02 03 ff 04 05 06 ff      run idx 1, count 2
  80 00 05 07 08 09 ff                         single pixel idx 5
  00 00 07 00 01 0a 0b 0c 80                   run idx 7, count 1 (open at the end: not converted)
compressed (3-byte):
  00 00 01 00 02 03 03 03 07 07 06
  80 00 05 07 0b 09
  00 00 07 00 01 0a 08 0c
```

## 8. POSLEN opacity list (vendor serial transport)

**static**. Used only on small rev C screens while the device plays a video under the theme: sent with opcode 208
(0xD0), followed by `ef 69`, after a 0xCA full frame, and appended to 0xCC partials
([protocol-turing-rev-c.md](protocol-turing-rev-c.md) section 13.5). Large screens use per-pixel alpha instead. It lists
the runs of pixels whose alpha is **> 15**; the rest shows the device-side video. Records carry no pixel data:

```
for idx in 0 .. w*h-1:
    if alpha[idx] > 15 and run_len < 65000:
        if not in_run: write BE24 idx at the cursor (do not advance); in_run = true
        run_len += 1
        if run_len >= 65000: the next pixel's alpha is forced to 0 (ends the run; mutates the input)
    else if in_run:
        write BE16 run_len at cursor+3; cursor += 5; in_run = false; run_len = 0
at the end: if in_run: write BE16 run_len; cursor += 10     (5 extra zero bytes)
length = cursor
```

No single-pixel conversion. Worked example (static, computed), alpha per idx = `[0, 255, 255, 0, 0, 255, 0, 255]`:

```
00 00 01 00 02 | 00 00 05 00 01 | 00 00 07 00 01 | 00 00 00 00 00
```

## 9. Other encoders present but unused by any transport (static)

Listed so nobody mistakes them for protocol requirements: a "semi-transparent only" encoder (6-byte records for
pixels with 0 < alpha <= 250); a block encoder (1600-pixel blocks in fixed 4800-byte slots with `00 BE16(len)`
headers; alpha > 10 kept, alpha > 200 -> a4 = 15) and a slot-diff of two block buffers; a merger of two record
streams; a full-frame run encoder (alpha > 10 kept, alpha >= 150 -> a4 = 15); YUV (BT.601) 32 x 32 tile layouts.
None is called. A helper also classifies an image as "opaque" when fewer than 40,000 of its pixels have alpha 0.

## 10. Container formats

| Family | Frame container |
|---|---|
| TUR_USB (0x1CBE) | Python: PNG (RGBA, zlib level 9) with command 102, JPEG fallback with command 101 when the PNG exceeds 1 MiB. Vendor: JPEG quality 95 (command 101), or PNG with alpha (command 102) while a device-side video plays; frames over 1 MiB are dropped ([protocol-turing-usb.md](protocol-turing-usb.md) section 5) |
| WCH (0x43A8) | raw BGR888 in 480-in-512 byte blocks |
| rev C | raw BGRA full frames in 249+1 byte blocks; row runs / run lists for partial updates; POSLEN lists on small screens with a device-side video |
| rev A, B, D, WeAct | raw RGB565 |

## 11. Rotation and native-address formulas

`W`, `H` = current-orientation size; `(x, y)` = rectangle origin; `w`, `h` = image size after any rotation unless
stated; Pillow `rotate(n)` is counter-clockwise.

| Family / model | Portrait/landscape | Reverse (180°) | What is sent |
|---|---|---|---|
| rev A | device: opcode 0x79, value `orientation + 100`, with the new W/H | device | current-orientation pixels |
| rev B | device: `cb 00` / `cb 01` | software: rotate 180, rectangle `x0 = W - x - w`, `y0 = H - y - h`, `x1 = W - x - 1`, `y1 = H - y - 1` | |
| rev C | software, native address (table below); the vendor also sends ROTATION 0x81 on large screens (effect unknown) | software (FLIP_180 disabled) | native-orientation pixels |
| rev D | software: landscape = rotate 270 CCW (90 CW); window `x0 = 320 - y - h_orig`, `x1 = 320 - y - 1`, `y0 = x`, `y1 = x + w_orig - 1` | device: `43 47 00 00` | |
| WeAct | device: `02 <0..3> 0a` | device | current-orientation pixels |
| TUR_USB | software, whole-frame transpose; native = REVERSE_PORTRAIT: PORTRAIT `ROTATE_180`, LANDSCAPE `ROTATE_270` (90 CW), REVERSE_LANDSCAPE `ROTATE_90` (90 CCW); the vendor also sends command 13 | software | native full frame |
| WCH (vendor app) | software (rotation setting 0..3 applied to the composed frame) **and** command 0x56 | same | full frame at the table's W x H |

Rev C native addresses (**verified** against the Python code):

| Model (native buffer, rowlen) | Orientation | Transform | row0 | col0 |
|---|---|---|---|---|
| 2.1"/2.8" (480 x 480, 480), 5" (800 x 480, 800) | LANDSCAPE | none | y | x |
| | PORTRAIT | rotate 90 CCW | W - x - h | y |
| | REVERSE_PORTRAIT | rotate 270 CCW | x | H - y - w |
| | REVERSE_LANDSCAPE | rotate 180 | H - y - h | W - x - w |
| 8.8" (480 x 1920, 480) | REVERSE_PORTRAIT | none | y | x |
| | LANDSCAPE | rotate 270 CCW | x | H - y - w |
| | REVERSE_LANDSCAPE | rotate 90 CCW | W - x - h | y |
| | PORTRAIT | rotate 180 | H - y - h | H - x - w (Python; suspected bug, geometric value W - x - w) |

`address(r) = (row0 + r) * rowlen + col0` for image row `r`.

Vendor app, rev C (**static**): the composed theme is rotated on the PC so the pixels sent are always in native
orientation. On its portrait panels (2.4", 6.5", 6.8", 8", 8.8") a landscape theme (width > height) is turned 90°
clockwise into the native buffer, and rotation setting 2 adds 180°; the effect of settings 1 and 3 on the PC-side
image was not established. Other panels are sent as composed. On the 8.8" the 90° clockwise turn equals Python's
LANDSCAPE transform above. The rotation setting is also sent to the device (0x81), whose effect on streamed frames is
unknown.
