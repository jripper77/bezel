# Runtime artifacts

Files the reference applications write while running: the vendor app's log and configuration files, its sensor-engine
init file, and the Python app's runtime files. Useful for importing user settings, for reading bug reports, and for
knowing what Bezel must not do. Confidence: **static**; the log statistics come from one long-running installation
and are given only as proportions and medians.

## 1. Vendor `debug.log`

### 1.1 Format

- Location: `debug.log` in the application folder. CRLF line endings, UTF-8.
- Line prefix: `yyyy-MM-dd HH:mm:ss ` (the older build wrote no separator before the message).
- **Never rotated or truncated**: after a year of daily use it reached hundreds of MB and millions of lines, about
  99.4 % of them the one-per-second heartbeat.
- Stack traces are written in the Windows display language.
- About 60 call sites write to it. The leveled overload used by the sensor code does not reach the file in this build,
  so **no sensor values are logged**.
- Never logged: firmware version, COM port name, the panel's VID:PID, power events, configuration read errors.
- The Setting page's "Download Log" zips it as `Log_<yyyyMMdd_HHmmss>.zip`.

### 1.2 Message templates

Placeholders: `<n>`, `<m>` integers; `<path>` a Windows device path; `<SN>` a device serial; `<exception>` an
exception message.

| Template | Meaning |
|---|---|
| `reinite cnt:<n>   ERRCNT:<m>` | 1 Hz heartbeat of the running device loop: `<n>` frames re-sent (almost always 0), `<m>` transport error counter (never reset during a session) |
| `GetAllHidDevice,item=><path>` | on every hot-plug event, one line per **HID device of the PC** (keyboards, mice, dongles), with instance ids; none of them is the panel |
| `GetAllWinUsbDevice,item=><path>` | one line per WinUSB device enumerated (the serial 8.8" panel never appears here) |
| `GetAllDevice.count = <n>` | number of screens found by the enumeration (0 or 1 in practice) |
| `新增设备` | "new device added" by the hot-plug handler |
| `Start!` | a device start cycle began |
| `ready for monitoring` | init succeeded, streaming started |
| ` finally stop Start` | the start loop ended |
| `finally MonitorList.count=<n>` | device list size after the loop |
| `Task Start(int waitCnt) failed: <exception>` + stack, then `Device Error UsbMonitorL <method signature>` | a serial write failed during start (for example the brightness command); start aborted, device list emptied |
| `88inchRenderFrame failed` | render/encode/send of a frame threw |
| `this.Hide()` / `this.Hide();` | window hidden to the tray |
| `MonitorList.Remove=88inch` | device removed or app exit |
| `isAutoStart=False`, `MonitorList.coint=<n>`, `item.SN =<SN>__item.PortName=<port>__MonitorList.COUNT<n>` | older build only |
| `start CheckIniteCode();`, `HWi32_Init(initeCode)`, `IniteSensorIndex()`, `use default GPU`, `Total Memoy Size = <n> MB` | sensor engine start-up |
| `Init wch Exception; ex=<exception>`, `CloseUSB(SN) Exception`, `wch Send error!`, `wch Send error! writer == null \|\| writer.IsDisposed`, `ErrorCode Send Exception=><exception>` | WCH transport errors |
| `Send WchCmd.StopVideo`, `WchCmd.ResetMem` | WCH commands |
| `StartTheme No Video`, `StartTheme With Video=>`, `StartTheme Wch Exception =><exception>`, `<name>:MonitorState stop`, `<name>:IniteDev finally` | WCH theme loop |
| `POSLEN:`, `blockcnt:`, `cnt overflow:` | run-list encoders ([pixel-formats.md](pixel-formats.md)) |

Proportions in the analysed log: about 94 % of `Start!` reached `ready for monitoring`; in about 86 % of all heartbeats
`ERRCNT` was 1 (one reconnect per session, never reset); start failures clustered in periods consistent with a flaky
cable or hub.

## 2. Typical session timeline (serial 8.8" panel)

| t (s) | Log lines | Interpretation |
|---|---|---|
| -2 | `GetAllWinUsbDevice ...`, 40+ `GetAllHidDevice ...`, `GetAllDevice.count = 1` | a PnP event (panel appeared) triggers a full enumeration |
| 0 | `新增设备`, `Start!` | the device object is created; settings are read; themes scanned |
| +4 | `ready for monitoring` | serial init done, theme loaded, streaming (median 3 s, range 1-23 s) |
| +5 .. +15 | `reinite cnt:0   ERRCNT:0` every second | steady streaming |
| +16 | enumeration; `GetAllDevice.count = 0` | **the panel drops off the USB bus** (median 12 s after ready, range 0-224 s; in about 80 % of sessions) |
| +22 | enumeration; `GetAllDevice.count = 1` | it re-enumerates (median 21 s after ready; gap about 9 s) |
| +30 | `ERRCNT` becomes 1 (median 25 s after ready) and stays 1 | one reconnect/re-init |
| ... | one heartbeat per second for hours | streaming |
| end | `this.Hide()`, ` finally stop Start`, `MonitorList.Remove=88inch` | exit or unplug |

The drop costs roughly 10 s of black screen at every launch. Whether the host's wake/reset traffic to the companion
MCU ([devices.md](devices.md) section 5.2) or the firmware causes it is unknown. Bezel keeps its state across
re-enumeration and re-opens the device by identity.

## 3. Configuration files (`AppConfig.data`, `config\config_<device id>.data`)

### 3.1 Container

- An MS-NRBF (`BinaryFormatter`) stream of one `UsbMonitorL.AppConfig` object, assembly `UsbMonitorL,
  Version=3.1.1.1, Culture=neutral, PublicKeyToken=null`, **padded with zeros to the MemoryStream capacity** (2048 bytes
  for typical content; a longer adapter or GPU name pushes it to 4096). Stop at MessageEnd (`0b`).
- The global file `AppConfig.data` is loaded by the app-wide monitor (a fresh object with `BurnPrevent = true` when
  missing). Each device has `config\config_<device id>.data`, where the id is the Windows PnP instance path of the
  device with its separators removed (or its serial when there is no PnP id); it therefore changes when the panel moves
  to another USB port.
- The per-device file uses the same class; its sensor fields are null (sensor choices live only in the global file).
- Parse it with the same whitelisted NRBF reader as themes ([themes-turzx.md](themes-turzx.md) section 1).

### 3.2 Record layout

| Offset | Record |
|---|---|
| 0..16 | SerializedStreamHeader (root id 1) |
| 17..88 | BinaryLibrary id 2, `UsbMonitorL, Version=3.1.1.1, ...` |
| 89..1530 | ClassWithMembersAndTypes `UsbMonitorL.AppConfig`, 42 members (names, BinaryTypeEnum table, additional infos, library id 2) |
| 1531.. | value block, members in declaration order (primitives inline; strings as `06 <id> <LPS>`, `0a` null or `09 <id>` reference) |
| after the values | `List<int>` class metadata (system class: `_items`, `_size`, `_version`), an empty `int[]` (`0f` record), `0b` MessageEnd, zero padding |

Offsets 0..1549 are fixed for this assembly version; from `hmName` on, offsets depend on string lengths. Member names in
the stream are backing fields (`<IsLoopPlay>k__BackingField`). `ThemeList` and `ThemePreviewList` are not serialized
(rebuilt by scanning the theme folders).

### 3.3 Members

| # | Member | NRBF type | Offset | Meaning |
|---|---|---|---|---|
| 0 | `IsLoopPlay` | Boolean | 1531 | playlist repeat |
| 1 | `IsRandomPlay` | Boolean | 1532 | playlist shuffle |
| 2 | `LangCode` | Int32 | 1533 | UI language index (one OEM brand only) |
| 3 | `BurnPrevent` | Boolean | 1537 | swap themes hourly (true in a new global file) |
| 4 | `Brightness` | Byte | 1538 | 1..255, default 170 |
| 5 | `ThemeIndex` | Int32 | 1539 | legacy |
| 6 | `startMode` | Byte | 1543 | device start mode: 0 default, 1 image, 2 video |
| 7 | `HideData` | Boolean | 1544 | hide the data layer |
| 8 | `SleepMode` | Boolean | 1545 | "compatibility startup" |
| 9 | `sleepDelay` | Byte | 1546 | 0 = no sleep, n = minutes (0..10; the checkbox sets 2) |
| 10 | `imgFlip` | Byte | 1547 | album flip 180° |
| 11 | `offLineMode` | Byte | 1548 | disable screen-off |
| 12 | `Fahrenheit` | Boolean | 1549 | |
| 13 | `hmName` | String | 1550 | device display name (filled from the model on first connect, for example `88inch`) |
| 14 | `LikeList` | reference to `List<int>` | variable | shop "liked" theme ids |
| 15 | `AutoStart` | Boolean | variable | Task Scheduler autostart (only the global copy is used) |
| 16 | `NoUpdate` | Boolean | | |
| 17 | `MsgShowOnBoot` | Boolean | | |
| 18 | `LastSelectThemeIndex` | Int32 | | |
| 19 | `LastSelectThemeName` | String | | |
| 20 | `PrevSelectThemeName` | String | | previous theme (burn-in swap) |
| 21 | `customColor` | Int32[] | | colour-dialog custom colours (16) |
| 22 | `shopUserName` | String | | shop login |
| 23 | `shopUserPwd` | String | | shop password **in plain text** |
| 24 | `netcardName` | String | | network adapter for the speed items (localised adapter name) |
| 25 | `CpuTempSensorSubName` | String | | CPU temperature label (default `CPU Package`) |
| 26 | `CpuUsedSubName` | String | | CPU usage label (default `Total CPU Usage`) |
| 27 | `CityName` | String | | weather city |
| 28 | `CityWeatherCode` | String | | weather location id |
| 29 | `rotateLcd` | Int32 | | rotation index |
| 30 | `RTSS` | Boolean | | FPS via RTSS |
| 31 | `cpuModel` | String | | manual CPU name (null = auto) |
| 32 | `gpuModel` | String | | manual GPU name |
| 33 | `ddrModel` | String | | manual RAM description |
| 34 | `delayTime` | Int32 | | autostart delay (s) |
| 35 | `GpuName` | String | | `GPU [#n]: <name>` |
| 36 | `GpuMemName` | String | | GPU memory sensor label |
| 37 | `CpuVSensorSubName` | String | | CPU voltage label (default `VID Average`) |
| 38 | `CPUFANName` | String | | `<device index>.<label>` |
| 39 | `CPUPUMPFANName` | String | | same format |
| 40 | `CaseFAN_1_Name` | String | | same format (identical strings are written once and referenced with `09`) |
| 41 | `CaseFAN_2_Name` | String | | same format |

For import, Bezel reads the display name, brightness, rotation, start mode, sleep delay, flip, offline mode, playlist
and burn-in flags, Fahrenheit and the last theme name; it ignores the shop credentials and maps sensor labels to its own
ids only as suggestions.

## 4. `code.ini`

Two bytes, ASCII `64`, no newline. Read with an integer parse and passed to the sensor engine's init function. The
vendor's quick-start guide says: if the app is stuck on the initialisation screen, change `64` to `0` or `8192`,
save and restart (probably flags selecting sensor groups or drivers: 0x40, 0x2000; unconfirmed). Bezel does not need
it.

## 5. Other vendor runtime files

| Item | Notes |
|---|---|
| `theme\<res>\`, `restore\<res>\`, `video\<res>\`, `visual\`, `theme_temp\`, `fonts\` | [themes-turzx.md](themes-turzx.md) section 8 |
| `fw\` (empty in this install), `temp\`, `DownloadTemp\`, `UploadTemp\`, `ImageCache\` | firmware, caches, shop staging |
| `Driver\` | indirect-display driver package for Desktop mode (INF matching `USB\VID_1A86&PID_AD10`, `AD11&MI_00`, `AD12&MI_00`, `AD13&MI_00`) |
| third-party components | ffmpeg and FFmpeg libraries, MediaInfo, NAudio, HidLibrary, LibUsbDotNet, RJCP.SerialPortStream, Task Scheduler wrapper, a cloud-storage SDK (shop uploads), BouncyCastle, Newtonsoft.Json, DotNetZip, the HWiNFO sensor engine under a generic runtime-library file name |
| help material | an FPS guide (install RTSS, enable "Run at Windows startup"), a quick-start image, TF-card formatting instructions |
| string table | about 540 keys in six languages, embedded |

## 6. Python runtime files

| File | Behaviour |
|---|---|
| `config.yaml` | next to the program; edited in place by the configurator |
| `log.log` | in the **current working directory**; `RotatingFileHandler` 1 MB with `backupCount=0` (truncated, no backups), level DEBUG, format `%(asctime)s [%(levelname)s] %(message)s`, plus the console |
| `screencap.png` | simulated display only: written to the working directory after every update; served with an auto-reloading page on `http://localhost:5678/` (reload every 250 ms) |
| `res/themes/<theme>/preview.png` | rewritten by the theme editor on every reload; regenerated by CI with the simulated display and random sensors |
| `<video>.h264` | created next to an MP4 by the TUR_USB video helpers |

## 7. Privacy notes

What the reference artifacts expose, and what Bezel does instead:

| Artifact | Exposure | Bezel |
|---|---|---|
| vendor `debug.log` | the HID/WinUSB device inventory of the PC with instance ids, on every hot-plug; grows without bound | structured ring buffer; logs only its own devices; bounded size; redaction option |
| vendor `config_<device id>.data` file names | the Windows device instance path | settings keyed by a stable, non-identifying device key |
| vendor `AppConfig.data` | shop password in plain text; localised adapter and GPU names | no online accounts; secrets (if any, for example a weather API key) in the OS keyring |
| vendor `.turtheme` | author's absolute paths, stale sensor history, a preview of the author's live values | stripped on import, never written on export |
| vendor app binary | embedded third-party API credentials | never reused; Bezel uses keyless services or the user's own key |
| Python `config.yaml` | weather API key in plain text | as above |
| Python `log.log` | written to whatever the working directory is | per-user state/log directory |

Bug reports and fixtures committed to the Bezel repository must never contain device serial numbers, PnP instance
ids, user names, local paths or keys.
