# Devices

Supported and known models, how they identify themselves on USB, and how a host tells them apart.
Resolutions are written portrait first (width x height). Confidence: **static** unless a row says otherwise;
USB descriptor values come from the `tools/lsusb/*.txt` dumps shipped in the Python reference.

## 1. Model table

| Model | Size | Resolution | Native scan orientation | Family (doc) | USB id(s) | Serial / string markers |
|---|---|---|---|---|---|---|
| Turing Smart Screen 3.5" (original) | 3.5" | 320 x 480 | portrait; the device rotates on command | rev A ([doc](protocol-turing-rev-a.md)) | 1a86:5722 | iManufacturer `Turing`, iProduct `UsbMonitor`, iSerial `USB35INCHIPSV2`; no answer to HELLO |
| UsbPCMonitor 3.5" | 3.5" | 320 x 480 | portrait, device rotates | rev A | 1a86:5722 | iManufacturer `2017-2-25`, iProduct `UsbMonitor`, iSerial `USB35INCHIPSV2`; HELLO answer `01 x6` |
| UsbPCMonitor 5" | 5" | 480 x 800 | portrait, device rotates | rev A | 1a86:5722 | as 3.5"; HELLO answer `02 x6` |
| UsbPCMonitor 7" | 7" | 600 x 1024 | portrait, device rotates | rev A | 1a86:5722 (assumed; no dump) | HELLO answer `03 x6`; no theme size exists for it |
| XuanFang 3.5" rev B | 3.5" | 320 x 480 | portrait; device switches portrait/landscape, reverse is software | rev B ([doc](protocol-xuanfang-rev-b.md)) | 1a86:5722 | iManufacturer `江苏沁恒` (WCH), iProduct `XFZX`, iSerial `2017-2-25`; USB 1.10 |
| XuanFang 3.5" "flagship" | 3.5" | 320 x 480 | as rev B | rev B | 1a86:5722 | identical descriptors to rev B; HELLO sub-revision `0x02`/`0x12`; backplate RGB LEDs |
| Turing Smart Screen 2.1" | 2.1" (round) | 480 x 480 | LANDSCAPE of the library enum | rev C ([doc](protocol-turing-rev-c.md)) | asleep: 1a86:ca21; awake: 1d6b:0121 | asleep `Turing`/`UsbMonitor`/`CT21INCH`; awake `Android`/`Android`/`20080411` |
| Turing Smart Screen 2.8" (UART) | 2.8" | 480 x 480 | LANDSCAPE | rev C | not recorded (assumed as 2.1") | not recorded |
| Turing Smart Screen 5" | 5" | 480 x 800 | 800 x 480 buffer (LANDSCAPE) | rev C | asleep: 1a86:5722; awake: 1d6b:0106 | asleep `Turing`/`UsbMonitor`/`USB7INCH`; awake `Android`/`Android`/`20080411` |
| Turing Smart Screen 8.8" (UART, older HW) | 8.8" | 480 x 1920 | 480 x 1920 buffer (REVERSE_PORTRAIT) | rev C | asleep: 1a86:ca21 or 1a86:ca88; awake: 0525:a4a7 | asleep `Turing`/`UsbMonitor`/`CT21INCH` (older MCU firmware, sic) or `CT88INCH` (newer); awake iManufacturer `Linux 5.4.61 with sunxi_usb_udc`, iProduct `Gadget Serial v2.4`, **no iSerial** |
| Kipye Qiye Smart Display 3.5" | 3.5" | 320 x 480 | portrait; device does 180°, landscape is software | rev D ([doc](protocol-kipye-rev-d.md)) | 454d:4e41 | iManufacturer `MEANS Technology`, iProduct `MEAN PANEL` |
| WeAct Studio Display FS V1 3.5" | 3.5" | 320 x 480 | portrait, device rotates | WeAct A ([doc](protocol-weact.md)) | 1a86:fe0c | iSerial starts with `AB` |
| WeAct Studio Display FS V1 0.96" | 0.96" | 80 x 160 | portrait, device rotates | WeAct B | 1a86:fe0c | iSerial starts with `AD` |
| Turing USB 2.8" round (HW rev 1.x) | 2.8" | 480 x 480 | REVERSE_PORTRAIT of the library enum | TUR_USB ([doc](protocol-turing-usb.md)) | 1cbe:0028 | none used |
| Turing USB 4.6" | 4.6" | 320 x 960 | REVERSE_PORTRAIT | TUR_USB | 1cbe:0046 | |
| Turing USB 5.2" | 5.2" | 720 x 1280 | REVERSE_PORTRAIT | TUR_USB | 1cbe:0050 (0x0052 in the Python code before commit `cf0f1db`) | |
| Turing USB 8" | 8" | 800 x 1280 | REVERSE_PORTRAIT | TUR_USB | 1cbe:0080 | |
| Turing USB 8.8" (HW rev 1.x) | 8.8" | 480 x 1920 | REVERSE_PORTRAIT | TUR_USB | 1cbe:0088 | |
| Turing USB 9.2" | 9.2" | 462 x 1920 | REVERSE_PORTRAIT | TUR_USB | 1cbe:0092 | themes are drawn at 480 x 1920 and cropped to 462 |
| Turing USB 12.3" | 12.3" | 720 x 1920 | REVERSE_PORTRAIT | TUR_USB | 1cbe:0123 | |
| WCH 3.38" | 3.38" | 180 x 640 | portrait flag set in the vendor table | WCH ([doc](protocol-wch.md)) | 43a8:0e61 | panel type 4 (`338_Rect`) in the 0x40 reply |
| WCH 4.3" | 4.3" | 480 x 272 | landscape | WCH | 43a8:0e6d | no panel-type code maps to it |
| WCH 320 x 320 ("G-GEAR aio" OEM) | 2.8" square (from the model string `28_Sq`) | 320 x 320 | square | WCH | 43a8:0e64 | panel type 6 (`28_Sq`) |
| WCH 2.4" / 2.8" rectangular | 2.4" / 2.8" (from `24_Rect` / `28_Rect`) | 240 x 320 | portrait flag set | WCH | 43a8:0e5e | panel type 8 (`24_Rect`) or 5 (`28_Rect`) |

Notes:

- The Python theme-size table (`library/display.py:58-84`) maps a size string to portrait pixels:
  `0.96"` 80x160, `2.1"` 480x480, `2.8"` 480x480, `3.5"` 320x480, `4.6"` 320x960, `5"` 480x800, `5.2"` 720x1280,
  `8"` 800x1280, `8.8"` 480x1920, `9.2"` 480x1920 (the real panel is 462 wide, 480 keeps 8.8" themes compatible),
  `12.3"` 720x1920; anything else falls back to 320x480 with a warning.
- The WCH rows exist only in the vendor application; the Python reference does not support VID 0x43A8.
- "Native scan orientation" for rev C and TUR_USB is expressed in the Python library's orientation enum
  (derived from its rotation code); see [pixel-formats.md](pixel-formats.md) section 11.

## 2. USB descriptors (from the Python repo's lsusb dumps)

| Device / state | USB | Class and endpoints | Dump |
|---|---|---|---|
| Turing 3.5" (rev A) | bcdUSB 2.00, EP0 64, bus-powered 500 mA | CDC-ACM without IAD: if0 interrupt IN 0x81 (8 B, bInterval 255); if1 bulk IN 0x82 (64), bulk OUT **0x03** (64) | `tools/lsusb/turing3.5inch.txt` |
| UsbPCMonitor 3.5" / 5" (rev A) | as above | CDC-ACM: interrupt 0x81; bulk IN 0x82, bulk OUT **0x02** | `usbMonitor3.5inch.txt`, `usbMonitor5inch.txt` |
| XuanFang rev B / flagship | **bcdUSB 1.10, EP0 8**, 100 mA | CDC-ACM: interrupt 0x81 (8); bulk OUT 0x02, IN 0x82 (64) | `xuanfangB.txt`, `xuanfangFlagship.txt` |
| Turing 2.1" rev C asleep (MCU) | 2.00, EP0 64 | CDC-ACM: interrupt 0x81; bulk IN 0x82, OUT 0x03 (64) | `turing2.1inchOff.txt` |
| Turing 2.1" rev C awake (SoC) | 2.00 high-speed, self-powered, bcdDevice ff.ff | CDC-ACM **with IAD** ("CDC Serial"): if0 interrupt IN 0x84 (10 B); if1 bulk IN 0x81 (512), bulk OUT 0x01 (512) | `turing2.1inchOn.txt` |
| Turing 5" rev C asleep / awake | as 2.1" | as 2.1" | `turing5inchOff.txt`, `turing5inchOn.txt` |
| Turing 8.8" rev C asleep (1a86:ca21) | 2.00 | as 2.1" asleep | `turing8.8inchOff.txt` |
| Turing 8.8" rev C asleep (1a86:ca88, `CT88INCH`) | no dump | CDC-ACM (known from the Python code, `lcd_comm_rev_c.py:142`) | - |
| Turing 8.8" rev C awake (0525:a4a7) | bcdDevice 5.04, bConfigurationValue 2, "CDC ACM config" | CDC-ACM with IAD: interrupt 0x84 (10); bulk IN 0x81 (512), OUT 0x01 (512) | `turing8.8inchOn.txt` |
| Kipye Qiye 3.5" (rev D) | bcdUSB 1.10, EP0 64, 100 mA | CDC-ACM with IAD: interrupt IN 0x83 (8, bInterval 1); bulk OUT 0x02, IN 0x82 (64) | `kipye_qiye.txt` |
| WeAct FS V1 (both sizes) | no dump | CDC serial port | - |
| TUR_USB (1cbe:*) | no dump | Python uses interface 0, first OUT and first IN endpoint (addresses not hard-coded) | - |
| WCH (43a8:*) | no dump | Vendor app: bulk OUT 0x02, IN 0x82 (opened as interrupt with a 32-byte buffer; transfer type from the descriptor) | - |

All UART families are USB CDC-ACM; the baud rate (115200) is only a SET_LINE_CODING value.

## 3. Detection in the Python reference

`LcdComm.openSerial()` (`library/lcd/lcd_comm.py:106-136`) makes up to 10 attempts one second apart. With
`COM_PORT: AUTO` it re-runs the class's `auto_detect_com_port()` at every attempt (the port can change while the
screen resets), then opens `serial.Serial(port, 115200, timeout=1, rtscts=True)`. After 10 failures it logs an
error and exits with code 0.

Each rule iterates `serial.tools.list_ports.comports()` in OS order and returns the first match. The serial-number
test and the VID:PID test run in the same loop, so whichever matching port comes first wins.

| Family | Match rule | Python reference |
|---|---|---|
| A | iSerial == `USB35INCHIPSV2` **or** VID:PID == 1a86:5722 | `lcd_comm_rev_a.py:67-77` |
| B | iSerial == `2017-2-25` **or** 1a86:5722 | `lcd_comm_rev_b.py:70-80` |
| C | wake step, then the awake-port match (section 5) | `lcd_comm_rev_c.py:138-181` |
| D | 454d:4e41 | `lcd_comm_rev_d.py:54-62` |
| WeAct A | 1a86:fe0c **or** iSerial starts with `AB` | `lcd_comm_weact_a.py:47-58` |
| WeAct B | 1a86:fe0c **or** iSerial starts with `AD` | `lcd_comm_weact_b.py:44-55` |
| TUR_USB | not a serial port: `usb.core.find(idVendor=0x1cbe, idProduct=pid)` for pid in 0x0028, 0x0046, 0x0050, 0x0080, 0x0088, 0x0092, 0x0123; first found wins | `lcd_comm_turing_usb.py:462-497` |

The user selects the protocol (`display.REVISION` in `config.yaml`); detection only finds a port for that protocol.

## 4. Disambiguation

### 4.1 What the Python reference does

- **1a86:5722 is shared** by the Turing 3.5" (A), UsbPCMonitor 3.5"/5" (A), XuanFang rev B and flagship (B) and a
  **sleeping** Turing 5" rev C (`USB7INCH`). Python does not disambiguate: the configured revision decides the
  protocol, and with several such devices attached the first enumerated match wins, even a wrong one, because the
  VID:PID fallback matches them all.
- Within rev A, Turing 3.5" versus UsbPCMonitor 3.5"/5"/7" is resolved by the HELLO answer.
- Within rev B, rev B versus flagship and on/off-only versus ranged brightness are resolved by the HELLO answer.
- Within rev C, 2.1"/2.8" versus 5" versus 8.8" is **not** read from the device (the HELLO string is unreliable:
  2.1" units answer `chs_5inch...`); the size comes from the theme's `DISPLAY_SIZE`.
- WeAct 3.5" and 0.96" share 1a86:fe0c; the iSerial prefix separates them, but the VID:PID test in the same loop
  matches either.
- TUR_USB models are distinguished purely by PID.

### 4.2 Descriptor-based strategy for Bezel (recommendation, static)

The lsusb dumps show that the string descriptors separate every 1a86:5722 variant without sending a byte:

| iManufacturer | iProduct | iSerial | Conclusion |
|---|---|---|---|
| `Turing` | `UsbMonitor` | `USB35INCHIPSV2` | Turing 3.5" rev A |
| `2017-2-25` | `UsbMonitor` | `USB35INCHIPSV2` | UsbPCMonitor rev A; size from HELLO (`01`/`02`/`03`) |
| `江苏沁恒` | `XFZX` | `2017-2-25` | XuanFang rev B or flagship; variant from HELLO |
| `Turing` | `UsbMonitor` | `USB7INCH` | Turing 5" rev C, MCU asleep: run the wake procedure, then use the SoC port |

Then: 1a86:ca21 / 1a86:ca88 (`CT21INCH`, `CT88INCH`) are rev C MCUs; 1d6b:0121 / 1d6b:0106 / iSerial `20080411` /
0525:a4a7 are rev C SoCs; 454d:4e41 is rev D; 1a86:fe0c with `AB`/`AD` is WeAct; VID 0x1CBE is TUR_USB by PID;
VID 0x43A8 is WCH by PID. Never probe a device with another family's handshake to "see what answers": the effect
of foreign commands on these firmwares is unknown. On Windows the string descriptors are available through the
device's PnP properties; iSerial also appears in the instance path of devices that report one.

## 5. Rev C: MCU/SoC pairing and wake-up

A rev C screen contains a small MCU (a CH552-class chip) that enumerates while the screen is asleep, and a Linux SoC
(Allwinner "sunxi" gadget, or an Android gadget on older units) that enumerates as a second CDC-ACM device when
the screen is awake. All protocol traffic goes to the SoC port.

| Device | How it is recognised | Role |
|---|---|---|
| 1a86:ca88, iSerial `CT88INCH` (8.8", newer MCU firmware) | Python: only by the serial string (no VID:PID rule for ca88) | wake target |
| 1a86:ca21, iSerial `CT21INCH` (2.1" and older 8.8") | serial string or VID:PID | wake target |
| 1a86:5722, iSerial `USB7INCH` (5") | serial string | wake target |
| 0525:a4a7 (8.8" SoC, sunxi gadget, no iSerial) | VID:PID | protocol port |
| 1d6b:0121 / 1d6b:0106 (2.1" / 5" SoC, Android gadget), or any port with iSerial `20080411` | VID:PID or serial | protocol port |

### 5.1 Python wake procedure (static; `lcd_comm_rev_c.py:138-181`)

1. For every port whose iSerial is `USB7INCH`, `CT21INCH` or `CT88INCH`, or whose VID:PID is 1a86:ca21, call the
   wake routine.
2. Wake routine: up to `WAKE_RETRIES = 15` iterations. Open `serial.Serial(port, 115200, timeout=1, rtscts=True)`
   and let it close immediately (exceptions ignored). If an awake port is now listed, sleep 1 s and return;
   otherwise sleep 1 s and retry. After 15 failures, log an error. **No byte is written to the MCU port**: only
   open and close (on Linux: SET_LINE_CODING, DTR/RTS raised on open and dropped on close).
3. Return the first awake port: iSerial `20080411`, or 0525:a4a7, 1d6b:0121, 1d6b:0106.

Consequences: each AUTO detection with a known MCU present costs about 1 s plus one open/close of the MCU tty, even
when the SoC is already up. With a fixed `COM_PORT` there is no wake-up at all. After `Reset()` (RESTART) the SoC
gadget disappears and re-enumerates; Python waits up to 15 s for it to vanish and up to 15 s for it to return, then
re-runs detection. The MCU and SoC may be listed at the same time (reported for at least one 8.8" unit behind an
internal hub; the Python dumps were captured one device at a time and neither confirm nor exclude it).

### 5.2 Vendor application wake behaviour (static)

- The vendor app matches the 8.8" by the SoC id 0525:a4a7 (its device table records 480 x 1920 and a companion
  MCU code `CA88`) and drives it through its serial transport. It never matches the `CT..INCH` serial strings.
- It groups COM ports that share a USB parent: it reads each port's location path, drops the last component (two
  components if the last one contains `USBMI`) and keys a table by the parent path. The *sibling* COM port of the
  protocol port is the MCU.
- As a wake pulse it opens the sibling port at 115200 8N1 with DTR and RTS asserted and closes it again.
- In its connect-retry path it writes the six bytes `00 00 00 00 00 c9` to the sibling port, waits 8000 ms, then
  closes it. The bytes have the shape of a rev A 6-byte command with opcode 0xC9 (**inferred**; meaning unknown).
- Its logs show the panel dropping off the bus about 12 s after streaming starts and returning about 9 s later in
  most sessions ([runtime-artifacts.md](runtime-artifacts.md) section 2). Whether the host's wake/reset traffic or
  the firmware causes this is unknown. Bezel must survive re-enumeration and re-open the port by identity.

## 6. Other entries in the vendor device table (partial, static)

| USB id | Vendor-table meaning |
|---|---|
| 0525:a4a7 | 8.8" 480 x 1920, serial transport, desktop-mode capable, companion MCU code `CA88` |
| 1a86:5722 | a 3.5" serial model (table name `35inchO`) |
| 1a86:ad11 | HID "DesktopMode" companion device |
| 1a86:ad10, ad11 (MI_00), ad12 (MI_00), ad13 (MI_00) | matched by the vendor's indirect-display driver used for "Desktop mode" (the panel acting as a Windows monitor) |
| 0x1CBE PIDs | the vendor's USB family, enumerated through WinUSB (same family as TUR_USB) |
| 43a8:0e61 / 0e6d / 0e64 / 0e5e | WCH family (section 1) |

The table holds about 45 entries keyed by `VID_xxxx&PID_xxxx`; the resolution of every device comes from the table,
never from the device. The full table is part of the pending consolidation below.

## TURZX additions

_Pending: consolidated from the TURZX USB/HID analysis._
