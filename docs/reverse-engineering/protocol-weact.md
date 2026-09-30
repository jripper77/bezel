# Protocol: WeAct Studio Display FS V1 (3.5" and 0.96")

Reference: `library/lcd/lcd_comm_weact_a.py` (3.5", 320 x 480) and `library/lcd/lcd_comm_weact_b.py` (0.96",
80 x 160) of turing-smart-screen-python at `2b33ab4` (imported from WeActStudio.SystemMonitor).
Confidence: **static** for behaviour, **verified** for every vector in section 10.

## 1. Transport and detection

- USB CDC serial port, VID:PID 1a86:fe0c for both sizes. iSerial starts with `AB` (3.5") or `AD` (0.96").
- Open: 115200 8N1, `rtscts=True`, read timeout 1 s.
- Python auto-detect: 1a86:fe0c **or** iSerial prefix (`lcd_comm_weact_a.py:47-58`, `lcd_comm_weact_b.py:44-55`).
  Because the VID:PID test matches both sizes, AUTO can pick the wrong one when both are attached; use the prefix.
- Both sizes speak the same protocol; the 0.96" lacks the temperature/humidity sensor commands.

## 2. Packet format

All multi-byte values are little-endian. Every command ends with `CMD_END = 0x0A`. Read commands are
`opcode | 0x80`.

## 3. Command table

| Opcode | Name | Layout | Python use |
|---|---|---|---|
| 0x02 | SET_ORIENTATION | `02 <orientation 0..3> 0a` (library enum: 0 P, 1 RP, 2 L, 3 RL) | yes, on-device rotation |
| 0x03 | SET_BRIGHTNESS | `03 <level 0..255> LE16(1000) 0a` (1000 = milliseconds, presumably fade time) | yes |
| 0x04 | FULL (fill rectangle) | `04 LE16 x0=0 LE16 y0=0 [(W-1) & 0xff] [(W >> 8) & 0xff] [(H-1) & 0xff] [(H >> 8) & 0xff] LE16 RGB565 0a` (12 B) | `Clear` (black) |
| 0x05 | SET_BITMAP | `05 LE16 x0 LE16 y0 LE16 x1 LE16 y1 0a` (10 B, inclusive), then raw RGB565 **LE** | `DisplayPILImage` |
| 0x15 | SET_BITMAP_WITH_FASTLZ | as 0x05, then per chunk `LE16 raw_len, LE16 comp_len, FastLZ data` | commented out |
| 0x06 | ENABLE_HUMITURE_REPORT (3.5" only) | `06 LE16 period_ms 0a` (0 = off, else 500..65535) | `ScreenOff` (0) |
| 0x07 | FREE | `07 0a` | `ScreenOff` |
| 0x42 | SYSTEM_VERSION | read form `c2 0a` -> 19-byte answer; version = bytes [1..8] as ASCII | `InitializeComm` |
| 0x81 | WHO_AM_I | - | no |

Device-to-host humidity report (3.5", unused `HandleSensorReport`, `lcd_comm_weact_a.py:197-211`): frame
`86 <4 bytes> 0a`, unpacked as `<Hh`: temperature = u16 / 100, humidity = i16 / 100.

## 4. Handshake (`lcd_comm_weact_a.py:90-117`)

`readall()` (drains input until the 1 s timeout), send `c2 0a`, `read(19)`, discard pending input, log
`answer[1:9]`. FastLZ capability detection is commented out (firmware "V1.0.0.0" = no FastLZ).

## 5. Brightness

`int(level / 100 * 255)`. The value is remembered; `ScreenOn()` re-sends the remembered value. After `ScreenOff()`
that value is 0 (quirk); the Python app calls `SetBrightness(config)` afterwards anyway.

## 6. Orientation

On-device: `02 <o> 0a`. Bitmaps are sent in current-orientation coordinates and pixel order.

## 7. Bitmap (`lcd_comm_weact_a.py:213-276`)

1. Assert that the rectangle fits (`x + w <= W`, `y + h <= H`); there is no clipping, an assertion fails instead.
2. SET_BITMAP header, then RGB565 LE data in chunks of `W * 4` bytes, all under the queue mutex (atomic).
3. No acknowledgement.

## 8. Screen on/off, reset, clear

- `ScreenOff()` 3.5": brightness 0, humidity report off (`06 00 00 0a`), FREE. 0.96": brightness 0, FREE.
- `Clear()` = FULL with black. `Reset()` is a no-op.

## 9. Quirks and open questions

- FULL encodes the high bytes as `W >> 8` / `H >> 8` instead of `(W-1) >> 8` / `(H-1) >> 8`. Wrong only when W or H
  is a multiple of 256; no current size is affected.
- `ScreenOn()` after `ScreenOff()` restores brightness 0.
- Unknown: the full 19-byte version answer layout, FREE semantics, the 1000 ms brightness field, FastLZ chunk
  semantics, the sign convention of the humidity report.

## 10. Test vectors (verified)

```
InitializeComm          : readall(), c2 0a, R 19
SetBrightness(0)        : 03 00 e8 03 0a
SetBrightness(25)       : 03 3f e8 03 0a
SetBrightness(100)      : 03 ff e8 03 0a
Full(red), 320x480 P    : 04 00 00 00 00 3f 01 df 01 00 f8 0a
Clear, 320x480 P        : 04 00 00 00 00 3f 01 df 01 00 00 0a
Full(red), 480x320 L    : 04 00 00 00 00 df 01 3f 01 00 f8 0a
SetOrientation P/L/RP/RL: 02 00 0a / 02 02 0a / 02 01 0a / 02 03 0a
SetSensorReportTime(1000): 06 e8 03 0a
Free                    : 07 0a
ScreenOff (3.5")        : 03 00 e8 03 0a | 06 00 00 0a | 07 0a
grad (3x2) at (10,20)   : 05 0a 00 14 00 0c 00 15 00 0a | 00f8 e007 1f00 ffff 0000 aa11
0.96" Full(blue), 80x160: 04 00 00 00 00 4f 00 9f 00 1f 00 0a
```
