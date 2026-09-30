# UI and feature inventory

What the two reference applications let a user do, where their UX fails, and the single-window design Bezel
follows. Confidence: **static** (read from code, UI definitions and the vendor app's bundled help material).

## 1. turing-smart-screen-python

### 1.1 Components

| Program | Role |
|---|---|
| `main.py` | the system monitor: drives the screen with the configured theme; optional tray icon |
| `configure.py` | Tkinter configuration window (sv-ttk "Sun Valley" theme, fixed 820 x 590) |
| `theme-editor.py` | Tkinter theme **viewer** with auto-reload |
| `simple-program.py` | example of driving a screen from code without a theme |
| `res/themes/scale_theme.py` | theme coordinate scaler ([themes-python-yaml.md](themes-python-yaml.md) section 11) |
| `tools/` | theme preview generator, vendor-theme image extractor, serial port lister, lsusb dumps |

### 1.2 `configure.py` (single window "Display configuration", `configure.py:276-712`)

In order: screen model (Turing / UsbPCMonitor / XuanFang rev B & flagship / Kipye Qiye / WeAct FS V1 / Simulated) ->
screen size (0.96, 2.1/2.8, 2.8 round new rev, 3.5, 4.6, 5, 5.2, 8, 8.8, 8.8/9.2 new rev, 12.3) -> COM port (from
`comports()`, "Automatic detection" first) -> orientation (classic / reverse) -> brightness slider (warning that the
Turing 3.5" gets hot above 50 %) -> theme (filtered by size, sorted case-insensitively; 2.1/2.8 and 8.8/9.2 merged)
with a preview (`preview.png` shrunk to 320 x 480, circular mask for 2.1") and the author link -> hardware monitoring
(Automatic / LHM / Python libraries / fake random / fake static) -> Ethernet and Wi-Fi interface (psutil interface
names; disabled for the fake backends) -> CPU fan (Linux/macOS only, refreshed every 500 ms as `name/label (pct% -
RPM)`, first entry "Auto-detected: ..."; tooltip suggests `sudo sensors-detect`) -> Windows admin warning (disables
"Save and run" when LHM/Automatic and not elevated).

Buttons: "Weather & Ping" (sub-window: ping host, API key, latitude/longitude, units, language, and an OpenWeatherMap
geocoding city search), "Open themes folder", "Edit theme" (spawns the theme editor), "Save settings", "Save and run"
(starts `main.py` detached and closes). The (model, size) -> protocol mapping (`configure.py:100-154`) means **the
user must know the model and size**; the attached device is not detected.

### 1.3 `config.yaml` (next to the program; edited in place)

| Key | Values | Meaning |
|---|---|---|
| `config.COM_PORT` | `AUTO`, `/dev/ttyACM0`, `COM3`, ... | serial port |
| `config.THEME` | folder name under `res/themes` (default `3.5inchTheme2`) | theme; must match the screen size (not verified) |
| `config.HW_SENSORS` | `AUTO`, `PYTHON`, `LHM`, `STUB`, `STATIC` | sensor backend |
| `config.ETH`, `config.WLO` | interface name or `""` | the two network slots (Windows: adapter display names that must equal the LHM hardware name) |
| `config.CPU_FAN` | `AUTO` or `<chip>/<label>` | Linux/macOS only |
| `config.PING` | host (shipped `8.8.8.8`) | ping target |
| `config.WEATHER_API_KEY`, `WEATHER_LATITUDE`, `WEATHER_LONGITUDE`, `WEATHER_UNITS` (`metric`/`imperial`/`standard`), `WEATHER_LANGUAGE` (50 OWM codes) | | weather (key stored in plain text) |
| `display.REVISION` | `A`, `B`, `C`, `D`, `TUR_USB`, `WEACT_A`, `WEACT_B`, `SIMU` | protocol driver |
| `display.BRIGHTNESS` | 0..100 (default 20) | percent |
| `display.DISPLAY_REVERSE` | bool | rotate 180° |
| `display.RESET_ON_STARTUP` | bool (default true) | reset (re-enumeration) at start |

Not configurable: locale (taken from the environment), GPU choice, disk mount point, log level, autostart, several
screens, units.

### 1.4 `theme-editor.py` (383 lines)

Not a graphical editor. It forces the static sensor backend and the simulated display, opens `theme.yaml` in the OS
default text editor, renders every group with `INTERVAL > 0` once with fixed values, and shows the image in an
always-on-top Tk window (background = the theme's LED colour, circular mask for 2.1", automatic 1:2 zoom above 1000 px,
zoom buttons and mouse wheel in steps of 0.2). It polls the file's modification time every 100 ms and re-renders on
save (errors show an error image; details only in the log), and **rewrites the theme's `preview.png` on every
reload**. Click prints X/Y; drag draws a rectangle and prints `Zone: X=,Y=,width=,height=` to copy into the YAML.

Limitations: nothing is written back to the theme; no live editing, undo, property panel, completion or validation;
groups with `INTERVAL: 0` are not rendered even if a widget has `SHOW: true`; sensor values are constant, so line
graphs are degenerate and text-width changes cannot be previewed; `DISPLAY_RGB_LED.split(', ')` breaks on `255,0,0`.

### 1.5 Runtime, tray, autostart, packaging

- Tray (pystray): menu **Configure** (launches the configurator and exits the monitor) and **Exit**. Silently absent
  when unsupported (for example GNOME without the AppIndicator extension). No status, pause or reload.
- Startup: optional reset, handshake, screen on + brightness + LED colour, orientation, static images and texts, then
  metric threads 0.25 s apart.
- Errors: a theme error, an unknown backend, LHM without admin, or an unsupported Python version exit with **code 0**;
  a sensor exception silently kills that metric's thread; no reconnect or hot-plug; if the USB write thread dies,
  producers keep queueing (frozen screen, growing memory).
- Windows sleep/resume turns the screen off/on and redraws static content; **Linux and macOS have no suspend handling**.
- Autostart: **none provided**. A sample systemd unit (`tools/turing-smart-screen-python.service`) is a system service
  with `User=YOUR_USERNAME_HERE`, `WorkingDirectory=/opt/turing-smart-screen-python/`, `Restart=always` and hardening
  (`ProtectSystem=full`, `ProtectHome=read-only`, `PrivateTmp=true`, `NoNewPrivileges=true`); it documents neither the
  serial group nor udev rules for the USB models.
- Windows: sensors need **administrator** (LHM drivers); the installer is per-user without elevation and `main.exe` has
  no UAC manifest. The installer offers to (re)install PawnIO, the signed driver LHM uses. Its only shortcut launches
  the configurator.
- Packaging: PyInstaller one-dir bundle with three executables (monitor, configurator, theme editor); Linux tarball;
  Windows Inno Setup installer and portable zip.

## 2. Vendor app (TURZX V3.07)

### 2.1 Product structure

- One WPF executable serving about 55 OEM brands; the brand, language and feature flags are compiled in (this build:
  TURZX brand, English, theme shop disabled, "design mode" enabled).
- Single instance: a mutex plus a named pipe `TURZX_APP_Pipe`; a second launch sends `SHOW` to raise the running window.
- Must run **as administrator** (sensor engine).
- Command-line switches: `-p`, `-test`, `-auto`, `-testsd`, `-port` (exact effects not all established; `-test`
  enables a factory test mode, `-auto -port` is used by autostart).
- Tray: **Turn on the device**, **Exit the software**, **Exit the software and shut the device**. Closing the window
  hides it to the tray.
- Autostart: a Windows Task Scheduler task named `Monitor Starter` (logon trigger with the configured delay, highest
  run level, arguments `-auto -port`), not a Run registry key.
- Update check: opens the vendor's update web page when a newer version is flagged; can be disabled.
- Reacts to system suspend/resume and to hot-plug (a WMI watcher on `Win32_PnPEntity` every 2 s).

### 2.2 Page map

```
Main window
 +-- left menu: logo, device list, Home | Device | Theme Edit | Setting | Theme Mall (hidden) | About,
 |               "Connected but no device?" help link, version "2025-08-22 V3.0.7"
 +-- right frame:
       Home           preview, theme, background, colours, rotation, brightness, Run/Stop
       Device         on-device storage, files, start mode, sleep, firmware
       Theme Edit     separate editor window (section 2.8)
       Setting        PC-side sensors, weather, startup, units, logs
       Desktop Mode   switch the panel between data mode and Windows-monitor mode
       System Info    live dashboard rendered from a built-in theme
       About          contacts and versions
 dialogs: splash/progress, message box, device rename, image adjust, video adjust, colour picker
```

Other OEM shells rearrange the same controls and add: playlist Repeat/Shuffle, "Hide data", OLED "Burn-in prevention"
(swap the two last used themes every hour), and a Landscape/Portrait/All theme filter.

### 2.3 Home page

| Control | Behaviour |
|---|---|
| Preview with left/right arrows | the current theme rendered with sample values; arrows step through the theme list |
| Themes combo | thumbnails and names of every `.turtheme` in the device's resolution folder; changing it while running hot-swaps the stream |
| Set Background | image (png/jpg/bmp), GIF or MP4 (<= 120 MB) -> image/video adjust dialog -> replaces the theme's background element; videos are transcoded ([video.md](video.md)) |
| Font Color / Back Color | colour dialog with 16 remembered custom colours; recolours the running theme's texts / bar backgrounds |
| Rotation | 0°, 90°, 180°, 270°; switching between landscape and portrait switches the theme list |
| Brightness | slider 1..255 (default 170), sent live |
| Run / Stop | Run connects and starts streaming; while running, theme and rotation controls are disabled ("Please stop device") |
| Theme Edit (DIY Theme) | opens the editor |
| Device, Desktop Mode | open those pages (Desktop Mode only for capable devices) |
| Reset (hidden) | restores the stock copy of a theme |
| ClearBuffer | clears local render and video caches |

Flow: launch -> splash -> load settings -> device found -> "Connecting" -> Home shows the preview -> **the user must
press Run** (unless started with `-auto`) -> streaming -> closing hides to the tray.

### 2.4 Setting page (global)

| Group | Settings |
|---|---|
| Sensors | network adapter, CPU temperature sensor, CPU voltage sensor, CPU usage sensor, GPU, CPU fan, pump, case fan 1, case fan 2 (raw engine labels) |
| Model names | CPU / GPU / RAM model: auto-detected or manual text |
| FPS | "FPS (RTSS)" checkbox, "Detect RTSS" button |
| Weather | city search and selection |
| Startup | Auto Start, Show Startup Tips, Delay Start (s), "Compatibility startup" (re-init after resume; help text: power off completely, power on, tick it) |
| Behaviour | Disable Update, Use Fahrenheit, "Sleep after a 2-minute timeout" (device blanks when the PC sleeps), Sleep after N minutes (hidden list: No Sleep, 1..10) |
| Maintenance | ClearBuffer; "Download Log" (zips `debug.log` to `Log_<yyyyMMdd_HHmmss>.zip`) |
| Language | none in this build; the string table has 6 languages (zh, us, jp, fr, kr, traditional Chinese; about 540 keys) |

One Save button writes the global settings and recreates the autostart task; "some settings require a software
restart".

### 2.5 Device page (per device; serial Linux-SoC family)

| Control | Action |
|---|---|
| Storage info | internal flash and TF card total / used / free, and the ROM string (for example `chs_5inch.dev1_rom1.87`) |
| Refresh Storage, Restart Device | re-query; reboot the panel (optionally reconnect) |
| Update Rom | firmware update (hidden for many brands; no images shipped in this install) |
| StartMode | default / play image / play video, "takes effect after the monitor is powered off" |
| Sleep after minutes | device sleep delay |
| Album flips 180° | flip for gallery playback (not for streaming) |
| Disable Screen Off | offline mode (some brands only) |
| Save | pushes the device settings packet ([protocol-turing-rev-c.md](protocol-turing-rev-c.md) section 6) |
| Path selector | internal Video, internal Image, SD Card Video, SD Card Image |
| File list, Refresh, Upload, Delete, Play Select, Stop | upload jpg/jpeg/png/bmp/gif as images and mp4 (transcoded to `.h264` or MP4); ASCII file names only (letters, digits, `-`, `_`, `.`); <= 120 MB; 4 GB ceiling; internal-storage sync "takes tens of seconds to minutes", TF card is quick; "Play Select" on a video also makes it the next boot animation |
| Boot Logo (hidden) | set the device boot logo |
| hidden factory buttons | MCU reboot test, burn-in test |

TF-card help: if the card is not recognised, delete all partitions, create one primary FAT32 partition spanning the
card, reinsert, power-cycle.

### 2.6 Desktop Mode page

Two states. **Monitor mode** (data streaming): step 1 "Install Driver" (installs an indirect-display driver matching
`USB\VID_1A86&PID_AD10..AD13` with `pnputil`, elevated), step 2 "Switch Mode". **Desktop mode**: the panel is an extra
Windows monitor (extend or mirror); a button switches back. Driver presence is detected through WMI on the `AD11` HID
interface. A Linux equivalent would need a udl/evdi-style path; out of scope for now.

### 2.7 Dialogs

| Dialog | Features |
|---|---|
| Image adjust | 800 x 480 client with a panel mask overlay at 80 %; wheel zoom (slider 0..112), drag to pan, zoom -/+, rotate +-90°, Fill (fit and fill the empty area with a colour), Save/Apply |
| Video adjust | the same on the first frame, plus Fill; refuses blank areas; the crop rectangle drives ffmpeg |
| Splash / progress | brand, tip text, progress bar, optional Cancel |
| Message box | OK, Yes/No (convert video?), rotation choice when converting, "click to update" |
| Rename device | text box, Apply / Cancel; default name = model (for example `88inch`) |
| Theme shop (disabled) | login/register with e-mail verification, browse (All/Landscape/Portrait; default/latest/most liked), install, like, comment, upload (zip of theme + video + fonts), ranking; credentials stored in plain text |

### 2.8 Theme editor (separate 1400 x 900 window)

Layout, left to right:

1. **Palette** of element types + "Add control" (double-click also adds): Animation (video), Text, Data, Status bar,
   Curved bar, Image, and (except one OEM) Clock, Chart.
2. **Canvas setup**: Horizontal/Vertical (swaps W/H and re-fits the video); in design-mode builds an arbitrary canvas
   width/height.
3. **Layer list** bound to the element list: move up / move down (z-order), delete, duplicate (deep copy appended at
   the end), hide checkbox.
4. **Property panel** for the selected type:
   - common: X, Y (validated integers), data source (filtered by the element's accepted data names), sub-item (drive
     letter, disk number, time/date/weekday format);
   - Text/Data: text, font size (box and wheel), font family (installed fonts only), alignment Left/Middle/Right,
     letter spacing, colour (dialog and hex `RRGGBB`), gradient colour and direction (None, LeftToRight, TopToBottom,
     TL->BR, TR->BL), unit, bold, °F;
   - Status bar: length, height, border, corner radius, foreground/background/gradient colours, transparent
     background, fill background, gradient, segmented, invert, direction 0..3 (numbers only);
   - Curved bar: "radius" (actually the diameter), border (arc width), start %, total angle, colours, transparent
     background, blocks, round caps, reverse, invert;
   - Chart: width, height, border width, fill transparency 0..255, maximum value, reverse flow, line/border/fill
     colours;
   - Image / Clock: select image, Fill (stretch), zoom -/+, fill colour; Clock adds start angle, sweep angle, reverse,
     move pivot, pivot X/Y;
   - Video: select video (`*.mp4;*.gif`, some builds also `*.h264;*.264`), fill/cover, zoom slider 11..112 with -/+,
     rotate left/right.
5. **Save / Load**: theme name, Save, Load (file dialog in the resolution folder; any error -> "wrong theme").
6. **Preview** at **1:1 scale without zoom** inside a scroll viewer (a 1920-px theme scrolls horizontally); tip "the
   first control in the list must be the background image or video".

Interaction model and limits:

- Selection happens **only in the layer list**. A mouse-down anywhere on the preview drags the currently selected
  element (no hit-testing); for a Clock the angle is zeroed while dragging. The wheel resizes the selected element
  (font size, zoom).
- Every property change re-renders the whole theme. **No undo/redo, no keyboard nudging, no snapping or guides, no
  multi-select, no grouping, no rulers.**
- Save validates the name, non-emptiness and background-first, computes the video crop (and may transcode/upload the
  video), renders the preview into the theme, writes the theme to both the theme and restore folders, then refreshes
  the main page.
- Only one video per theme; element positions are absolute; font sizes are points; fonts are not embedded.

### 2.9 Vendor troubleshooting advice (bundled quick-start material)

- If the app is stuck on the initialisation screen, change `code.ini` from `64` to `0` or `8192`.
- Requires .NET Framework 4.7.2 and a running Windows Management Instrumentation service (used for USB/COM discovery).
- Close other RGB/lighting software (it grabs the HID/USB devices).
- For devices that only enumerate after a re-plug, check the BIOS "ERP" power option.
- "CPU temperature differs from other software": pick another sensor in Settings (the drop-downs show raw engine
  labels such as `CPU (Tctl/Tdie)` or `CPU Core Voltage (SVI2 TFN)`).
- "Screen stays on after shutdown": set the device sleep to 2 minutes.
- After "Upgrade ROM", restart the screen.

Bezel turns the equivalent checks into inline diagnostics rather than a help image.

### 2.10 Persistent settings

Global `AppConfig.data`: sensor choices, weather city, autostart and delay, compatibility startup, update and startup
message flags, Fahrenheit, RTSS, language index, shop credentials. Per device `config\config_<device id>.data`: display
name, last and previous theme, brightness, rotation, start mode, sleep delay, album flip, offline mode, burn-in
prevention, playlist flags, hide data, custom colours, liked themes. Layouts in
[runtime-artifacts.md](runtime-artifacts.md) section 3. One monitor object per device; each device streams
independently with its own theme list keyed by resolution.

## 3. UX critique

### 3.1 Python app

1. Themes are hand-written YAML (200 to 1500 lines) with absolute pixel coordinates; preview only after saving;
   coordinates are copied from a status line.
2. Boilerplate: the same 12-20 style keys repeat for every widget; no styles, inheritance, groups or presets.
3. No theme/screen validation; a wrong size shows as clipped or black output; missing sensors silently hide widgets.
4. No per-theme fonts; a missing PNG or font crashes a thread or exits.
5. Sensor semantics are hidden (truncation, `MIN_SIZE` padding, fixed unit strings, no number formatting).
6. One disk, two network slots, no per-core, no GPU choice, name heuristics; the user must pick model and size.
7. One `config.yaml` inside the install directory (not writable under Program Files or `/opt`; the sample service
   makes home read-only).
8. Admin required on Windows; minimal tray; Configure and Exit both stop the monitor.
9. Errors exit with code 0 or kill threads silently; one 1 MB log in the working directory.
10. Weather needs a paid-tier OpenWeatherMap subscription; the key is stored in plain text; geocoding over `http://`.

### 3.2 Vendor app

1. "What is on my screen" is changed in three places (Home theme, Theme Edit, Device start mode) across five pages and
   a separate editor window.
2. "Device" (on-panel storage and boot) and "Setting" (PC-side sensors and startup) are easily confused.
3. An explicit **Run** step; settings are locked while running.
4. Many settings need a restart; one global Save also rewrites the scheduled task.
5. Sensor pickers show raw engine labels in nine fixed slots, far from where values are used, with no live value.
6. The editor is a separate form; the background must be the first layer; numeric fields and wheel tips instead of
   direct manipulation.
7. Background/video flow: file dialog -> crop dialog -> transcode -> upload, with constraints (24 fps, exact
   resolution, 120 MB, ASCII names, "no blank space") surfaced late.
8. Per-device settings are keyed by the Windows device instance id, so moving the panel to another USB port loses them.
9. Reconnects are invisible except for message boxes: in most sessions the panel drops off the bus about 12 s after
   streaming starts and returns about 9 s later ([runtime-artifacts.md](runtime-artifacts.md) section 2).
10. Admin rights, a scheduled task, hide-to-tray, a hard-wired brand, and a sensor engine shipped under a misleading
    file name.
11. Privacy: plaintext shop password, embedded third-party API credentials, an unbounded `debug.log` (hundreds of MB
    after a year of use, about 99 % heartbeat lines) that records every HID device of the PC on each hot-plug event.
12. Language compiled in; inconsistent strings.
13. Two models of "startup content" (PC streaming versus stored image/video) with different constraints on different
    pages.

## 4. Bezel design: one window, drag and drop

```
+--------------------------------------------------------------------------------------+
| [device chips: (o) 8.8" front  [+]]   theme name [v]   Live [on/off]   brightness --o | top bar
+----------------+----------------------------------------------+----------------------+
| LIBRARY        | CANVAS (true-scale panel preview, live data)  | INSPECTOR            |
| tabs:          |  drag from the library, drop on the canvas    | selected element:    |
|  Widgets       |  handles: move / resize / rotate, snap guides |  source [sensor v]   |
|  Sensors (live)|  arrow-key nudge, duplicate, z-order          |  font / colour       |
|  Media         |  rulers, safe area, zoom; landscape/portrait  |  unit / decimals     |
|  Themes        |  layer list under the canvas                  |  (nothing selected:  |
|  Device        |                                               |   panel settings)    |
+----------------+----------------------------------------------+----------------------+
| status: connected (USB path) . firmware . fps . last frame ms . sensors ok . [Logs]  |
+--------------------------------------------------------------------------------------+
```

1. **Live by default, no Run button.** Edits reach the panel immediately (debounced 50-100 ms, changed regions only).
   A Pause toggle replaces Stop. Connection is automatic, with a persistent status chip (connecting, streaming,
   reconnecting with the reason).
2. **The Sensors tab is a drag source**: a searchable live list (`CPU Package 54 °C`, `GPU Load 31 %`,
   `Net down 1.2 MB/s`). Dropping a row on the canvas creates a value widget with the right unit and format; dropping
   it on an existing widget rebinds it. The inspector's source picker is the same list. "Auto" choices are presets with
   a badge. This replaces the nine fixed pickers.
3. **Widget shelf** (Text, Value, Bar, Arc, Gauge/needle, Line chart, Image, Animation/GIF/Video, Group) with
   thumbnails. The vendor's eight element types are the initial set, so `.turtheme` imports map 1:1.
4. **Media drop targets**: dropping PNG/JPG/GIF/MP4 on the canvas makes it the background (or asks "background or
   new layer") with an inline crop/zoom overlay (wheel zoom, drag pan, fit/fill/stretch). Constraint problems (fps,
   size, resolution, file names) show before conversion; transcoding runs in the background with progress.
5. **Themes tab**: thumbnails with live previews, click to apply, drop a file to import (`.turtheme`, Python theme
   folders, Bezel's own format), favourites, "revert to stock", and a schedule editor for playlists, shuffle, interval
   and the OLED hourly swap.
6. **Device tab** (per chip) merges the vendor's two pages: brightness, rotation (visual four-way picker), sleep and
   screen-off schedule, boot content (drop a stored image or video on the "Boot" slot), storage bars (internal / TF)
   with a file grid and drag-and-drop upload with progress and clear limits, firmware version and update, TF-card
   check with the FAT32 hint.
7. **Settings drawer**: autostart (systemd user service or XDG autostart on Linux; delay), units (°C/°F, MB/GB,
   KB/s versus Mbit/s), language (from the locale, switchable live), network interface, weather city (autocomplete),
   privacy (log level, redaction), sensor permissions (RAPL, hwmon), import/export of the configuration.
8. **Stable device identity**: key settings by USB serial or a port-independent identity when available, else a
   user-visible name plus VID:PID and port; offer "adopt settings from another device".
9. **Undo/redo everywhere**, autosave, a versioned theme file (manifest + assets) with a documented migration from
   `.turtheme` and Python YAML.
10. **Diagnostics view** instead of an unbounded log: a ring buffer of structured events (attach/detach, reconnect,
    frame drops, sensor failures) with "copy report"; unrelated USB/HID devices are never logged.
11. **Ergonomics**: keyboard nudging, snapping to grid and elements, multi-select align/distribute, numeric fields as
    secondary input, optional wheel adjustment, high-contrast mode, scalable UI.
12. From the Python app's failures: validate themes against the connected screen, report missing assets and sensors
    in the UI, keep configuration in the user's config directory, reconnect on hot-plug and resume, and exit with
    non-zero codes on errors.
