# Protocol: Kipye Qiye Smart Display 3.5" (rev D)

Reference: `library/lcd/lcd_comm_rev_d.py` of turing-smart-screen-python at `2b33ab4`.
Confidence: **static** for behaviour, **verified** for every vector in section 9.

## 1. Transport and detection

- USB CDC-ACM with IAD, VID:PID 454d:4e41, iManufacturer `MEANS Technology`, iProduct `MEAN PANEL`;
  bcdUSB 1.10, EP0 64, 100 mA; interrupt IN 0x83 (8 B, bInterval 1); bulk OUT 0x02, IN 0x82 (64).
- Open: 115200 8N1, `rtscts=True`, read timeout 1 s.
- Python auto-detect: VID:PID 454d:4e41 only (`lcd_comm_rev_d.py:54-62`).
- Resolution 320 x 480, portrait native.
- No handshake: `InitializeComm()` is a no-op.

## 2. Packet format

Commands are 4 bytes, except BLOCKWRITE (10 bytes). The opcodes are ASCII letters (`C` = 0x43, `D` = 0x44,
`A` = 0x41, `G` = 0x47, `P` = 0x50). Multi-byte values are big-endian.

## 3. Command table

| Name | Bytes | Meaning | Python use |
|---|---|---|---|
| GETINFO | `47 00 00 00` | unknown answer format | no |
| SETORG | `43 48 00 00` | portrait orientation | yes |
| SET180 | `43 47 00 00` | reverse portrait (180°) | yes |
| SETHF | `43 44 00 00` | portrait, horizontal mirror | no |
| SETVF | `43 46 00 00` | described as "reverse portrait with horizontal mirroring" (probably vertical flip) | no |
| SETBL | `43 43` + BE16 level (0..500) | backlight | yes |
| DISPCOLOR | `43 42` + BE16 RGB565 colour | fill the whole screen | `Clear` (0xFFFF white) |
| BLOCKWRITE | `43 41` + BE16 x0 + BE16 **x1** + BE16 y0 + BE16 y1 | set the drawing window (inclusive; note the x0, x1, y0, y1 order) | yes |
| INTOPICMODE | `44 00 00 00` | start bitmap transmission | yes |
| OUTPICMODE | `41 00 00 00` | end bitmap transmission | yes |
| data packet | `50` + up to 63 bytes | RGB565 **big-endian** pixels, 64-byte packets | yes |

## 4. Acknowledgements

Every command write (not pixel packets) is followed by `reset_input_buffer()`: "we don't process acknowledgements
the screen sends back" (`lcd_comm_rev_d.py:64-68`). The acknowledgement format is unknown.

## 5. Brightness (`lcd_comm_rev_d.py:105-116`)

`level * 5` (0..500) for level 0..100, sent **twice** "because sometimes it is not applied".
`ScreenOff()` = SetBrightness(0); `ScreenOn()` = SetBrightness(25). `Reset()` and `Clear()` = `43 42 ff ff`.

## 6. Orientation (`lcd_comm_rev_d.py:158-166`)

- Reverse orientations are on-device: SET180 for REVERSE_PORTRAIT and REVERSE_LANDSCAPE, SETORG otherwise.
- Portrait versus landscape is software: landscape images are rotated 270 CCW (= 90 CW), and the window becomes

```
x0 = 320 - y - h_orig        y0 = x
x1 = 320 - y - 1             y1 = x + w_orig - 1
```

  where `w_orig`, `h_orig` are the image size before rotation. The literal 320 is `display_width` (portrait width),
  not `get_width()`.

## 7. Bitmap (`lcd_comm_rev_d.py:168-187`)

1. Clip like rev A (a rectangle beyond the screen edge is cut).
2. BLOCKWRITE with the window (section 3 order), INTOPICMODE, N data packets of `50` + 63 bytes (the last one
   shorter), OUTPICMODE.
3. A full 320 x 480 frame (307,200 B of RGB565) is 4876 packets of 64 B plus one packet of 13 B.

## 8. Timing and quirks

- No acknowledgement is processed; input is discarded after each command.
- Each command locks the Python queue separately, so the BLOCKWRITE / INTOPICMODE / data / OUTPICMODE sequence is
  not atomic in Python. Bezel must send it as one sequence.
- Brightness is applied twice by design (unreliable single write).
- Open questions: GETINFO answer, acknowledgement format, exact semantics of SETHF / SETVF.

## 9. Test vectors (verified)

```
SetBrightness(0)   : 43 43 00 00 | 43 43 00 00        (input discarded after each)
SetBrightness(25)  : 43 43 00 7d | 43 43 00 7d
SetBrightness(100) : 43 43 01 f4 | 43 43 01 f4
Clear              : 43 42 ff ff
SetOrientation P / L   : 43 48 00 00
SetOrientation RP / RL : 43 47 00 00

grad (3x2) at (10,20), PORTRAIT and REVERSE_PORTRAIT:
  43 41 00 0a 00 0c 00 14 00 15 | 44 00 00 00 | 50 f800 07e0 001f ffff 0000 11aa | 41 00 00 00
grad (3x2) at (10,20), LANDSCAPE and REVERSE_LANDSCAPE:
  43 41 01 2a 01 2b 00 0a 00 0c | 44 00 00 00 | 50 ffff f800 0000 07e0 11aa 001f | 41 00 00 00

Full 320 x 480 frame (golden file): 4876 x 64-byte packets + 1 x 13-byte packet
```
