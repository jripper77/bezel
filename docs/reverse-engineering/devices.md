# Devices

Supported and known models, how they identify themselves on USB, and how a host tells them apart.
Resolutions are written portrait first (width x height). Sources: the Python reference (theme sizes, and the
`tools/lsusb/*.txt` descriptor dumps it ships), the device table of TURZX V3.07 (the vendor app), and the project's
tests on one Turing 8.8" rev C. Confidence per row.

## 1. Supported devices

This table mirrors Bezel's catalog (`crates/bezel-core/src/domain/catalog.rs`: 44 models, 50 USB rules). A change to
one must be made to the other.

- **Native orientation**: the catalog's `native_orientation`, named after the Python orientation enum. Frames are
  rotated from the theme orientation to it before encoding ([pixel-formats.md](pixel-formats.md) section 11). "device
  rotates" marks families that also have an orientation command.
- **Family**: [rev A](protocol-turing-rev-a.md), [rev B](protocol-xuanfang-rev-b.md),
  [rev C](protocol-turing-rev-c.md), [rev D](protocol-kipye-rev-d.md), [WeAct](protocol-weact.md),
  [TUR_USB](protocol-turing-usb.md), [WCH](protocol-wch.md).
- **VID:PID**: display endpoint / wake endpoint (rev C MCU, never written to). A condition in brackets is the iSerial
  rule of the catalog. Wake ids marked (inf.) are **inferred** from the vendor table's CH552 codes (section 5.3).
- **Sources** in the notes: Py = Python reference, V = vendor table (key in backticks), HW = hardware.

| Model (catalog id) | Size | Resolution | Native orientation | Family | VID:PID display / wake | Notes | Confidence |
|---|---|---|---|---|---|---|---|
| WeAct Studio Display FS 0.96" (`weact-fs-0.96`) | 0.96" | 80 x 160 | PORTRAIT, device rotates | WeAct (B) | 1a86:fe0c [iSerial `AD...`] / - | Py; the catalog stores 0.9" (tenths) | static |
| Turing 1.6" Square USB (`turing-usb-1.6`) | 1.6" | 400 x 400 | REVERSE_PORTRAIT | TUR_USB | 1cbe:0005 / - | V `16inch`; the diagonal comes from the key only (the vendor's theme shop labels this resolution "4.0") | static; size inferred |
| Turing Smart Screen 2.1" (`turing-2.1`) | 2.1" round | 480 x 480 | LANDSCAPE | rev C | 1d6b:0121 / 1a86:ca21 | Py, V `21/28inch` (CH552 code CA21); HELLO answers `chs_5inch...` | static |
| Turing 2.1" Round USB (`turing-usb-2.1-round`) | 2.1" round | 480 x 480 | REVERSE_PORTRAIT | TUR_USB | 1cbe:0021 / - | V `21inch` | static |
| Turing Smart Screen 2.4" (`turing-2.4`) | 2.4" | 240 x 320 | PORTRAIT | rev C | 1d6b:0124, 125f:7903 / 1a86:ca24 (inf.) | V `24inch` (CA24); 125f:7903 is an OEM variant that V3.07 never discovers (no theme resolution key) | static; wake inferred |
| 2.4"/2.8" WCH (`wch-2.4-2.8-rect`) | 2.4" / 2.8" | 240 x 320 | PORTRAIT | WCH | 43a8:0e5e / - | V; panel type 8 (`24_Rect`) or 5 (`28_Rect`) in the 0x40 reply; catalog diagonal 2.8" | static |
| Turing Smart Screen 2.8" (`turing-2.8`) | 2.8" round | 480 x 480 | LANDSCAPE | rev C | 1d6b:0121 / 1a86:ca21 | Py (no dump: ids assumed as the 2.1"), V `21/28inch` | static; USB ids inferred |
| Turing Smart Screen 2.8" Square (`turing-2.8-square`) | 2.8" square | 320 x 320 | LANDSCAPE | rev C | 1d6b:0127 / 1a86:ca27 (inf.) | V `28inch` (CA27) | static; wake inferred |
| Turing 2.8" Round USB (`turing-usb-2.8-round`) | 2.8" round | 480 x 480 | REVERSE_PORTRAIT | TUR_USB | 1cbe:0028 / - | Py (HW rev 1.x), V `28inchR` | static |
| Turing 2.8" Square USB (`turing-usb-2.8-square`) | 2.8" square | 400 x 400 | REVERSE_PORTRAIT | TUR_USB | 1cbe:0016 / - | V `28_Sq`; diagonal from the key | static; size inferred |
| 2.8" Square / G-GEAR aio WCH (`wch-2.8-square`) | 2.8" square | 320 x 320 | PORTRAIT | WCH | 43a8:0e64 / - | V ("G-GEAR aio" OEM); panel type 6 (`28_Sq`) | static |
| Turing 2.88" Round USB (`turing-usb-2.88-round`) | 2.88" round | 480 x 480 | REVERSE_PORTRAIT | TUR_USB | 1cbe:0288 / - | V `288inch`; the catalog stores 2.9" (tenths) | static; size inferred |
| 3.38" Bar WCH (`wch-3.38`) | 3.38" | 180 x 640 | PORTRAIT | WCH | 43a8:0e61 / - | V; panel type 4 (`338_Rect`); catalog diagonal 3.4" | static |
| Turing Smart Screen 3.4" Square (`turing-3.4`) | 3.4" square | 480 x 480 | LANDSCAPE | rev C | 1d6b:0134 / 1a86:ca34 (inf.) | V `34inch` (CA34) | static; wake inferred |
| Turing 3.4" Square USB (`turing-usb-3.4`) | 3.4" square | 480 x 480 | REVERSE_PORTRAIT | TUR_USB | 1cbe:0034 / - | V `34inch` | static |
| Turing Smart Screen 3.5" (`turing-3.5`) | 3.5" | 320 x 480 | PORTRAIT, device rotates | rev A | 1a86:5722 [other iSerial] / - | Py; `Turing` / `UsbMonitor` / `USB35INCHIPSV2`; no HELLO answer | static |
| UsbPCMonitor 3.5" (`usbpcmonitor-3.5`) | 3.5" | 320 x 480 | PORTRAIT, device rotates | rev A | 1a86:5722 [other iSerial] / - | Py; `2017-2-25` / `UsbMonitor` / `USB35INCHIPSV2`; HELLO answer `01 x6` | static |
| XuanFang 3.5" (`xuanfang-3.5`) | 3.5" | 320 x 480 | PORTRAIT; device portrait/landscape, reverse in software | rev B | 1a86:5722 [iSerial `2017-2-25`] / - | Py; `江苏沁恒` / `XFZX` / `2017-2-25`; USB 1.10 | static |
| XuanFang 3.5" Flagship (`xuanfang-3.5-flagship`) | 3.5" | 320 x 480 | PORTRAIT; as rev B | rev B | 1a86:5722 [iSerial `2017-2-25`] / - | Py; same descriptors; HELLO sub-revision `0x02`/`0x12`; backplate RGB LEDs | static |
| Kipye Qiye Smart Display 3.5" (`kipye-qiye-3.5`) | 3.5" | 320 x 480 | PORTRAIT; device does 180°, landscape in software | rev D | 454d:4e41 / - | Py; `MEANS Technology` / `MEAN PANEL` | static |
| WeAct Studio Display FS 3.5" (`weact-fs-3.5`) | 3.5" | 320 x 480 | PORTRAIT, device rotates | WeAct (A) | 1a86:fe0c (any other iSerial, `AB...` in practice) / - | Py | static |
| Turing 3.5" USB (`turing-usb-3.5`) | 3.5" | 480 x 640 | LANDSCAPE | TUR_USB | 1cbe:0035 / - | V `35inch` (640 x 480 native); the vendor swaps rotation values 0 and 2 | static |
| Turing Smart Screen 4" Square (`turing-4`) | 4" square | 720 x 720 | LANDSCAPE | rev C | 1d6b:a040 / 1a86:ca40 (inf.) | V `4inch` (CA40); vendor "large screen" | static; wake inferred |
| Turing 4" Square USB (`turing-usb-4`) | 4" square | 720 x 720 | REVERSE_PORTRAIT | TUR_USB | 1cbe:0040 / - | V `4inch` | static |
| 4.3" WCH (`wch-4.3`) | 4.3" | 272 x 480 | LANDSCAPE | WCH | 43a8:0e6d / - | V (480 x 272 native); no panel-type code maps to it | static |
| Turing 4.6" USB (`turing-usb-4.6`) | 4.6" | 320 x 960 | REVERSE_PORTRAIT | TUR_USB | 1cbe:0046 / - | Py only (not in V3.07) | static |
| UsbPCMonitor 5" (`usbpcmonitor-5`) | 5" | 480 x 800 | PORTRAIT, device rotates | rev A | 1a86:5722 [other iSerial] / - | Py; as UsbPCMonitor 3.5"; HELLO answer `02 x6` | static |
| Turing Smart Screen 5" (`turing-5`) | 5" | 480 x 800 | LANDSCAPE (800 x 480 buffer) | rev C | 1d6b:0106 / 1a86:5722 [iSerial `USB7INCH`], 1a86:ca50 (inf.) | Py, V `5inch` (CA50); asleep `Turing` / `UsbMonitor` / `USB7INCH`, awake `Android` / `Android` / `20080411`; HELLO `chs_5inch.dev1_rom1.87` | static; ca50 inferred |
| Turing 5.2" USB (`turing-usb-5.2`) | 5.2" | 720 x 1280 | REVERSE_PORTRAIT | TUR_USB | 1cbe:0050 / - | Py (0x0052 before commit `cf0f1db`), V `5inch`; desktop-mode capable | static |
| Turing 6.2" USB (`turing-usb-6.2`) | 6.2" bar | 368 x 960 | REVERSE_PORTRAIT | TUR_USB | 1cbe:a062 / - | V `62inch` | static |
| Turing 6.2" V2 USB (`turing-usb-6.2-v2`) | 6.2" bar | 448 x 1280 | REVERSE_PORTRAIT | TUR_USB | 1cbe:b062 / - | V `62inch` | static |
| Turing Smart Screen 6.5" (`turing-6.5`) | 6.5" bar | 720 x 1568 | PORTRAIT | rev C | 1d6b:a065 / 1a86:ca65 (inf.) | V `65inch` (CA65); large screen | static; wake inferred |
| Turing 6.5" USB (`turing-usb-6.5`) | 6.5" bar | 720 x 1472 | REVERSE_PORTRAIT | TUR_USB | 1cbe:a065 / - | V `65inch` | static |
| Turing 6.5" B USB (`turing-usb-6.5-b`) | 6.5" bar | 720 x 1568 | REVERSE_PORTRAIT | TUR_USB | 1cbe:b065 / - | V `65inch` (OEM variant) | static |
| Turing Smart Screen 6.8" (`turing-6.8`) | 6.8" bar | 1080 x 2320 | PORTRAIT | rev C | 1d6b:a068 / 1a86:ca68 (inf.) | V `68inch` (CA68); large screen; no per-tick QUERY_STATUS | static; wake inferred |
| Turing 6.8" USB (`turing-usb-6.8`) | 6.8" bar | 1080 x 2320 | REVERSE_PORTRAIT | TUR_USB | 1cbe:0068 / - | V `68inch`; 30 fps video when the version's hardware field is 1 | static |
| Turing 6.8" B USB (`turing-usb-6.8-b`) | 6.8" bar | 1080 x 2224 | REVERSE_PORTRAIT | TUR_USB | 1cbe:a068 / - | V `68inch` (OEM variant) | static |
| UsbPCMonitor 7" (`usbpcmonitor-7`) | 7" | 600 x 1024 | PORTRAIT, device rotates | rev A | 1a86:5722 [other iSerial] / - | Py; HELLO answer `03 x6`; no dump, no Python theme size | static; USB id inferred |
| Turing Smart Screen 8" (`turing-8`) | 8" | 800 x 1280 | PORTRAIT | rev C | 1d6b:a080 / - | V `8inch`; large screen; no CH552 code, hence no wake rule | static |
| Turing 8" USB (`turing-usb-8`) | 8" | 800 x 1280 | REVERSE_PORTRAIT | TUR_USB | 1cbe:0080 / - | Py, V `8inch`; desktop-mode capable | static |
| Turing Smart Screen 8.8" (`turing-8.8`) | 8.8" bar | 480 x 1920 | REVERSE_PORTRAIT | rev C | 0525:a4a7 / 1a86:ca88, 1a86:ca21 | Py, V `88inch` (CA88), HW. SoC `Linux 5.4.61 with sunxi_usb_udc` / `Gadget Serial v2.4`, no iSerial; MCU iSerial `CT88INCH` (newer) or `CT21INCH` on 1a86:ca21 (older); HELLO `chs_88inch.dev1_rom1.90` | static; hardware (0525:a4a7 + 1a86:ca88, HELLO) |
| Turing 8.8" V1.x USB (`turing-usb-8.8`) | 8.8" bar | 480 x 1920 | REVERSE_PORTRAIT | TUR_USB | 1cbe:0088 / - | Py (HW rev 1.x), V `88inch`; desktop-mode capable | static |
| Turing 9.2" USB (`turing-usb-9.2`) | 9.2" bar | 462 x 1920 | REVERSE_PORTRAIT | TUR_USB | 1cbe:0092 / - | Py crops 480-wide themes to 462; V `92inch` squeezes them to 464; real width unconfirmed | static |
| Turing 12.3" USB (`turing-usb-12.3`) | 12.3" bar | 720 x 1920 | REVERSE_PORTRAIT | TUR_USB | 1cbe:0123 / - | Py only (not in V3.07) | static |

### 1.1 Notes

- The Python theme-size table (`library/display.py:58-84`) maps a size string to portrait pixels:
  `0.96"` 80x160, `2.1"` 480x480, `2.8"` 480x480, `3.5"` 320x480, `4.6"` 320x960, `5"` 480x800, `5.2"` 720x1280,
  `8"` 800x1280, `8.8"` 480x1920, `9.2"` 480x1920 (the real panel is 462 wide, 480 keeps 8.8" themes compatible),
  `12.3"` 720x1920; anything else falls back to 320x480 with a warning.
- The vendor keys theme folders by resolution (`4801920`, `480480r` round, `480480s` square, ...;
  [themes-turzx.md](themes-turzx.md)) and accepts a device only when a folder for its key exists. The 8.8" build
  ships only `4801920`, so out of the box it drives only the 480 x 1920 models.
- The resolution always comes from a table (Bezel's catalog, the Python theme, the vendor table), never from the
  device. The only device-reported identities are the WCH panel type and the HID model byte of desktop mode
  ([protocol-turing-usb.md](protocol-turing-usb.md) section 10).
- The vendor's "large screen" class (rev C behaviour, [protocol-turing-rev-c.md](protocol-turing-rev-c.md)) covers
  the 4", 6.5", 6.8", 8" and 8.8".

### 1.2 Vendor-table entries Bezel does not support

| USB id | Vendor entry | Why |
|---|---|---|
| 0525:a4a7 | 11.3" 480 x 1920 (`113inch`, CH552 code CA11) | not in the vendor's active list; the id resolves to the 8.8" |
| 1cbe:0062 | 6.2" 368 x 960 "WinLcd" | routed to the vendor's serial transport without a port; cannot work in V3.07 |
| 1a86:5722 | 3.5" 480 x 320 `35inchO` on the rev C transport | never discovered in V3.07 (no theme key for 480 x 320); Bezel treats 1a86:5722 as rev A / rev B / sleeping 5" (section 4) |
| 7304:125f, 125f:7304 | 3.5" 640 x 480 and 320 x 240 (CA35) | not in the vendor's active list |
| 1d6b:0321 | virtual 400x600, 850x600, 870x660, 730x650, 800x1400 | not in the vendor's active list |
| 1a86:ad11 (INF also ad10, ad12, ad13) | a TUR_USB panel in desktop mode (Windows indirect display + HID) | display protocol not reversed ([protocol-turing-usb.md](protocol-turing-usb.md) section 10) |

## 2. USB descriptors

| Device / state | USB | Class and endpoints | Strings (iManufacturer / iProduct / iSerial) | Dump |
|---|---|---|---|---|
| Turing 3.5" (rev A) | bcdUSB 2.00, EP0 64, bus-powered 500 mA | CDC-ACM without IAD: if0 interrupt IN 0x81 (8 B, bInterval 255); if1 bulk IN 0x82 (64), bulk OUT **0x03** (64) | `Turing` / `UsbMonitor` / `USB35INCHIPSV2` | `tools/lsusb/turing3.5inch.txt` |
| UsbPCMonitor 3.5" / 5" (rev A) | as above | CDC-ACM: interrupt 0x81; bulk IN 0x82, bulk OUT **0x02** | `2017-2-25` / `UsbMonitor` / `USB35INCHIPSV2` | `usbMonitor3.5inch.txt`, `usbMonitor5inch.txt` |
| XuanFang rev B / flagship | **bcdUSB 1.10, EP0 8**, 100 mA | CDC-ACM: interrupt 0x81 (8); bulk OUT 0x02, IN 0x82 (64) | `江苏沁恒` (WCH) / `XFZX` / `2017-2-25` | `xuanfangB.txt`, `xuanfangFlagship.txt` |
| Turing 2.1" rev C asleep (MCU) | 2.00, EP0 64 | CDC-ACM: interrupt 0x81; bulk IN 0x82, OUT 0x03 (64) | `Turing` / `UsbMonitor` / `CT21INCH` | `turing2.1inchOff.txt` |
| Turing 2.1" rev C awake (SoC) | 2.00 high-speed, self-powered, bcdDevice ff.ff | CDC-ACM **with IAD** ("CDC Serial"): if0 interrupt IN 0x84 (10 B); if1 bulk IN 0x81 (512), bulk OUT 0x01 (512) | `Android` / `Android` / `20080411` | `turing2.1inchOn.txt` |
| Turing 5" rev C asleep / awake | as 2.1" | as 2.1" | asleep `Turing` / `UsbMonitor` / `USB7INCH`; awake as 2.1" | `turing5inchOff.txt`, `turing5inchOn.txt` |
| Turing 8.8" rev C asleep (1a86:ca21) | 2.00 | as 2.1" asleep | `Turing` / `UsbMonitor` / `CT21INCH` (older MCU firmware, sic) | `turing8.8inchOff.txt` |
| Turing 8.8" rev C MCU (1a86:ca88) | no dump | CDC-ACM (Python opens it as a tty, `lcd_comm_rev_c.py:142`) | iSerial `CT88INCH` | - |
| Turing 8.8" rev C awake (0525:a4a7) | bcdDevice 5.04, bConfigurationValue 2, "CDC ACM config" | CDC-ACM with IAD: interrupt 0x84 (10); bulk IN 0x81 (512), OUT 0x01 (512) | `Linux 5.4.61 with sunxi_usb_udc` / `Gadget Serial v2.4` / **none** | `turing8.8inchOn.txt` |
| Other rev C SoCs (1d6b:0124, 0127, 0134, a040, a065, a068, a080; 125f:7903) | no dump | CDC serial (the vendor opens them as COM ports) | - | - |
| Kipye Qiye 3.5" (rev D) | bcdUSB 1.10, EP0 64, 100 mA | CDC-ACM with IAD: interrupt IN 0x83 (8, bInterval 1); bulk OUT 0x02, IN 0x82 (64) | `MEANS Technology` / `MEAN PANEL` / - | `kipye_qiye.txt` |
| WeAct FS V1 (both sizes) | no dump | CDC serial port | iSerial starts with `AB` (3.5") or `AD` (0.96") | - |
| TUR_USB (1cbe:*) | no dump | vendor class; the vendor hard-codes bulk OUT 0x01 and IN 0x81, Python takes the first OUT and IN of interface 0 | none used | - |
| WCH (43a8:*) | no dump | vendor app: bulk OUT 0x02, IN 0x82 (opened as interrupt with a 32-byte buffer; transfer type from the descriptor) | none used | - |
| Desktop mode (1a86:ad11) | no dump | composite: `MI_00` vendor display interface, one HID interface | - | - |

All UART families are USB CDC-ACM; the baud rate (115200) is only a SET_LINE_CODING value.

Hardware: the tested 8.8" lists the SoC (0525:a4a7) and the MCU (1a86:ca88, `CT88INCH`) at the same time.

## 3. Detection

### 3.1 Python reference

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

### 3.2 Vendor app (static)

- Three enumerations are merged: Windows serial ports (rev C family), WinUSB devices (TUR_USB and WCH) and HID
  devices (desktop mode). A table entry matches when its `VID_xxxx&PID_yyyy` is a substring of the device's PnP id or
  path (case-sensitive for serial ports, case-insensitive otherwise); the first entry in table order wins.
- Hot-plug: PnP creation and deletion events, polled every 2 s. Only events of the composite parent trigger a rescan
  (children with `&MI_xx` do not match).
- A device's settings file is keyed by its PnP instance path, which for the 8.8" SoC (no iSerial) depends on the
  USB port ([runtime-artifacts.md](runtime-artifacts.md) section 3).
- The CH552 codes and the `CT..INCH` strings are never used for matching.

## 4. Disambiguation

### 4.1 What the references do

- **1a86:5722 is shared** by the Turing 3.5" (A), UsbPCMonitor 3.5"/5" (A), XuanFang rev B and flagship (B) and a
  **sleeping** Turing 5" rev C (`USB7INCH`). Python does not disambiguate: the configured revision decides the
  protocol, and with several such devices attached the first enumerated match wins, even a wrong one, because the
  VID:PID fallback matches them all. The vendor table maps 1a86:5722 to a 3.5" on its rev C transport, an entry
  V3.07 never activates.
- Within rev A, Turing 3.5" versus UsbPCMonitor 3.5"/5"/7" is resolved by the HELLO answer.
- Within rev B, rev B versus flagship and on/off-only versus ranged brightness are resolved by the HELLO answer.
- Within rev C, Python does **not** read the model from the device (the HELLO string is unreliable: 2.1" units answer
  `chs_5inch...`); the size comes from the theme's `DISPLAY_SIZE`. The vendor identifies the model by the SoC's
  VID:PID and requires the HELLO answer to contain that model's key (or `chs_5inch.dev1`).
- 0525:a4a7 is shared by the 8.8" and the vendor's inactive 11.3" entry; only the HELLO key separates them.
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

Then: 1a86:ca21 / 1a86:ca88 (`CT21INCH`, `CT88INCH`) and the inferred 1a86:caXX ids are rev C MCUs; 0525:a4a7,
1d6b:0106, 0121, 0124, 0127, 0134, a040, a065, a068, a080, 125f:7903 and iSerial `20080411` are rev C SoCs; 454d:4e41 is rev D; 1a86:fe0c with `AB`/`AD` is WeAct;
VID 0x1CBE is TUR_USB by PID; VID 0x43A8 is WCH by PID. Never probe a device with another family's handshake to "see
what answers": the effect of foreign commands on these firmwares is unknown. On Windows the string descriptors are
available through the device's PnP properties; iSerial also appears in the instance path of devices that report one.

## 5. Rev C: MCU/SoC pairing and wake-up

A rev C screen contains a small MCU (CH552 class) that enumerates while the screen is asleep, and a Linux SoC
(Allwinner "sunxi" gadget, or an Android gadget on older units) that enumerates as a second CDC-ACM device when
the screen is awake. All protocol traffic goes to the SoC port.

| Device | How it is recognised | Role |
|---|---|---|
| 1a86:ca88, iSerial `CT88INCH` (8.8", newer MCU firmware) | Python: serial string only (no VID:PID rule for ca88); Bezel: VID:PID | wake target |
| 1a86:ca21, iSerial `CT21INCH` (2.1" and older 8.8") | serial string or VID:PID | wake target |
| 1a86:5722, iSerial `USB7INCH` (5") | serial string | wake target |
| 1a86:ca24, ca27, ca34, ca40, ca50, ca65, ca68 | Bezel: VID:PID (**inferred**, section 5.3; never observed) | wake target |
| 0525:a4a7 (8.8" SoC, sunxi gadget, no iSerial) | VID:PID | protocol port |
| 1d6b:0121 / 1d6b:0106 (2.1" / 5" SoC, Android gadget), or any port with iSerial `20080411` | VID:PID or serial | protocol port |
| 1d6b:0124, 0127, 0134, a040, a065, a068, a080; 125f:7903 | VID:PID (vendor table) | protocol port |

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
re-runs detection.

### 5.2 Vendor app (static)

- It finds the MCU as the **sibling** COM port of the SoC port: it reads each COM port's location path, drops the
  last component (two when the last one contains `USBMI`, a composite-interface node), keys a table by the resulting
  parent path, and pairs the first two COM ports under the same parent. It opens the sibling at 115200 8N1 with DTR
  and RTS asserted.
- Byte-less wake pulse (open, flush, close): only in its factory test mode (a command-line switch), after a
  successful HELLO. Never in normal operation.
- MCU command: in its reconnect ladder ([protocol-turing-rev-c.md](protocol-turing-rev-c.md) section 15), at attempt
  6 and every tenth attempt (10, 20, ...), outside test mode, it writes the six bytes `00 00 00 00 00 c9` to the
  sibling port, waits 8000 ms and closes it. The shape is a rev A 6-byte command with opcode 0xC9; the 8 s wait
  suggests it resets or power-cycles the SoC (**inferred**; disruptive). The vendor's code carries an unused list of
  MCU command values: 10, 11, 13, 14, 15, 40, 101, 201, 253.
- The same ladder restarts the SoC's USB device node at attempts 2 and 4.
- Its logs show the panel dropping off the bus about 12 s after streaming starts and returning about 9 s later in
  most sessions ([runtime-artifacts.md](runtime-artifacts.md) section 2). Whether the host's reset traffic or the
  firmware causes this is unknown.

### 5.3 CH552 codes and inferred MCU ids

The vendor table stores a CH552 code per serial model and never reads it: CA21 (2.1"/2.8" round), CA24 (2.4"),
CA27 (2.8" square; also copied onto the `35inchO` entry), CA34 (3.4"), CA40 (4"), CA50 (5"), CA65 (6.5"), CA68 (6.8"),
CA88 (8.8"), CA11 (11.3"), CA35 (inactive 3.5"); the 8" has none. CA88 matches the MCU of the tested 8.8"
(1a86:ca88, **hardware**) and CA21 the dumped 2.1" MCU (1a86:ca21). Bezel's catalog therefore treats 1a86:caXX as the
wake endpoint for CA24, CA27, CA34, CA40, CA50, CA65 and CA68 (**inferred**, wake-only, never written to).
Counter-examples to keep in mind: the dumped sleeping 5" enumerates as 1a86:5722 `USB7INCH`, not 1a86:ca50, and an
older 8.8" MCU enumerates as 1a86:ca21 `CT21INCH`, not 1a86:ca88.

### 5.4 Hardware observations and Bezel's policy

- **Hardware** (one 8.8"): SoC and MCU are listed at the same time. When another program that was driving the screen
  stopped, the SoC gadget re-enumerated with a new USB device number within about 2 s.
- Bezel must survive re-enumeration: it keeps its state and re-opens the SoC by identity (VID:PID; the 8.8" SoC has
  no iSerial).
- Bezel never writes to the MCU implicitly: the 0xC9 command and the other disruptive steps are explicit actions
  (decision `D-2026-09-30-device-protocols-2`, [protocol-turing-rev-c.md](protocol-turing-rev-c.md) section 16).
