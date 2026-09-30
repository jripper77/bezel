# Reverse-engineering specification

This directory is the byte-level contract that Bezel's drivers, encoders, theme loaders and renderers
implement. Every encoder in the Rust code base is tested against the hex vectors written down here, so
these documents favour exact numbers over prose. When the code and this spec disagree, one of them has
a bug: fix it, or fix the spec and say why in the commit message.

## Files

| File | Contents |
|---|---|
| [devices.md](devices.md) | The supported-device table (mirrors the catalog), USB descriptors and string markers, detection and disambiguation, vendor-table entries Bezel does not support, rev C MCU/SoC pairing and wake-up |
| [protocol-turing-rev-a.md](protocol-turing-rev-a.md) | Turing 3.5" and UsbPCMonitor 3.5"/5"/7" (6-byte packed commands, RGB565 LE) |
| [protocol-xuanfang-rev-b.md](protocol-xuanfang-rev-b.md) | XuanFang 3.5" rev B and "flagship" (10-byte framed commands, RGB565 BE, backplate LEDs) |
| [protocol-turing-rev-c.md](protocol-turing-rev-c.md) | Turing serial SoC generation, 2.1" to 8.8" (250-byte `ef 69` commands, 249+1 data blocks, BGRA frames, run-list partials, storage, on-device video, firmware, recovery, hardware observations) |
| [protocol-kipye-rev-d.md](protocol-kipye-rev-d.md) | Kipye Qiye 3.5" (4-byte ASCII-like commands, 64-byte pixel packets) |
| [protocol-weact.md](protocol-weact.md) | WeAct Studio Display FS V1 3.5" and 0.96" (LE16 commands terminated by `0a`) |
| [protocol-turing-usb.md](protocol-turing-usb.md) | Turing/TURZX USB models on VID 0x1CBE (DES-CBC command headers, PNG/JPEG frames, storage, H.264 streaming) and the desktop-mode HID companion 1A86:AD11 |
| [protocol-wch.md](protocol-wch.md) | WinUSB panels on VID 0x43A8 (DES-ECB command packets, 480-in-512 byte blocks, BGR888 full frames) |
| [pixel-formats.md](pixel-formats.md) | RGB565 LE/BE, BGR, BGRA, compressed 3-byte BGRA, row runs, run lists with the single-pixel flag, opacity (POSLEN) lists, diff encoders, rotation and native-address formulas |
| [video.md](video.md) | On-device playback versus PC streaming per family, exact ffmpeg command lines, containers, rotation suffixes, device storage paths |
| [themes-python-yaml.md](themes-python-yaml.md) | The YAML theme schema of turing-smart-screen-python, defaults, scheduler semantics, YAML 1.1 quirks, `scale_theme.py` |
| [themes-turzx.md](themes-turzx.md) | The `.turtheme` container (MS-NRBF), record walk-through, class schemas, element types, fonts, folder layout, resolution keys |
| [rendering.md](rendering.md) | Reference rendering semantics Bezel reproduces (Pillow widgets of the Python app, GDI+ elements of the vendor app) |
| [sensors.md](sensors.md) | Unified sensor catalog, how each reference measures each metric, their measurement bugs, and Bezel's Linux/Windows strategy |
| [ui-inventory.md](ui-inventory.md) | Feature inventory of both reference apps, UX critique, and the single-window design Bezel follows |
| [runtime-artifacts.md](runtime-artifacts.md) | Vendor app log templates, a typical session timeline, config file layouts, `code.ini`, privacy notes |

## Method

Two independent sources were analysed.

1. **turing-smart-screen-python** (GPL-3.0, <https://github.com/mathoudebine/turing-smart-screen-python>)
   at commit `2b33ab4`. The code was read statically. Its LCD classes were also executed offline against a
   recording mock (the serial object replaced by a recorder, the USB class never instantiated), which
   produced the "verified" hex vectors in these documents. `path:line` references such as
   `library/lcd/lcd_comm_rev_c.py:183` point into that repository at that commit.
2. **TURZX V3.07**, the vendor's Windows application (8.8" English build). It was analysed for
   interoperability only: its USB transports, file formats, sensor sources, UI features and runtime files.
   Facts are stated as behaviour ("the serial transport writes...", "the frame encoder emits..."). No
   code, no internal symbol names and no line references into that program are reproduced.

The static analysis sent no command to a physical screen. The facts tagged **hardware** come from the project's own
tests on one Turing 8.8" rev C ([protocol-turing-rev-c.md](protocol-turing-rev-c.md) section 19).

The Python repository also ships golden files (`tests/library/lcd/golden/`, lines `write <hex>` / `read <n>` recorded by
`tests/library/lcd/serial_mock.py`). They agree with this spec for revs A, B and D. The rev C golden files are stale
(recorded in January 2025 with 5"-only code) and its rev C tests error out; use the vectors in
[protocol-turing-rev-c.md](protocol-turing-rev-c.md) instead.

## Legal note

These documents exist for interoperability: they let free software drive hardware that the user owns
and read files that the user created or received. They contain protocol and file-format facts only.

- No vendor source code, decompiled code, binaries, fonts, themes, artwork or videos are included.
- Byte sequences reproduced here are protocol constants, keys that the protocols require (for example
  the TUR_USB DES key and the WCH DES key), and test vectors generated from the documented algorithms.
- Credentials that the vendor application embeds for its own online services (weather API, theme shop)
  are deliberately not documented, and Bezel does not use them.
- Product names and trademarks belong to their owners and are used only to identify hardware.

## Notation

| Notation | Meaning |
|---|---|
| `ef 69` | Two bytes, 0xEF then 0x69. Hex is lowercase in byte dumps and space-separated where readable; long vectors may be written without spaces. |
| `0x7B` / `123` | A single value in hex or decimal. |
| `BE16`, `BE24`, `BE32` | Unsigned big-endian integer of 2, 3 or 4 bytes. |
| `LE16`, `LE32` | Unsigned little-endian integer of 2 or 4 bytes. |
| `pad250` / `+pad` | Zero bytes appended up to the next multiple of 250 bytes. A message that is already a multiple of 250 is not padded. |
| `N x 00`, `2c x250` | N repetitions of a byte. |
| `[a..b]` | Inclusive byte-offset range. |
| `\|` in a vector | Boundary between separate `write()` calls (or visually separated fields when the text says so). |
| `R 1024` | A blocking read of 1024 bytes follows. |
| W, H | Width and height of the screen in the **current** orientation. |
| native | The orientation in which the panel scans its frame buffer. |
| PORTRAIT=0, REVERSE_PORTRAIT=1, LANDSCAPE=2, REVERSE_LANDSCAPE=3 | Orientation enum of the Python reference (`library/lcd/lcd_comm.py:40-44`). Device resolutions are written portrait first (width <= height). |
| "grad" | The 3x2 test image used by most vectors. Row 0: red `(255,0,0)`, green `(0,255,0)`, blue `(0,0,255)`. Row 1: white, black, `#123456`. |
| Rotations | "90 CCW" is counter-clockwise, as Pillow's `rotate(90)`. "90 CW" is clockwise. |

## Confidence legend

Each fact, table or vector carries one of these tags (per row, per section, or in the heading).

| Tag | Meaning |
|---|---|
| **verified** | Produced by running the reference code against a recording mock. Byte-exact for that code. |
| **static** | Read from code (Python source or the vendor application) without executing it. Vectors marked "static, computed" were generated offline by applying a statically-read algorithm (for example DES with a documented key). |
| **inferred** | A static conclusion that goes beyond what the code states (for example the meaning of a header field). Treat as a hypothesis. |
| **hardware** | Confirmed on a real screen. So far only the Turing 8.8" rev C observations in [protocol-turing-rev-c.md](protocol-turing-rev-c.md) section 19 and [devices.md](devices.md) section 5.4. |

"verified" means the reference implementation emits these bytes. It does not mean a screen accepts them.
