# Bezel

**The open studio for USB smart screens.** Bezel drives the little USB system-monitor
screens sold as Turing Smart Screen, TURZX, XuanFang, Kipye, WeAct and their OEM
rebrands — on Linux and Windows, from one app.

Every protocol family is implemented and the Turing 8.8" is validated on real
hardware; the other screens follow the protocols of the vendor app and of
turing-smart-screen-python. Releases ship deb, rpm and AppImage packages for
Linux, MSI and setup installers for Windows, and the `bezel` command line for
both.

This repository is a personal Windows-focused fork of
[slipalison/bezel](https://github.com/slipalison/bezel). The release links below
refer to the original project; changes specific to this fork are described in
[FORK.md](FORK.md).

Thanks to Alison Amorim ([@slipalison](https://github.com/slipalison)) for
creating Bezel and sharing its code, protocol research and editor. This fork
builds on that work; the original project remains upstream.

## Install and documentation

Download from the [releases page](https://github.com/slipalison/bezel/releases)
and follow the **[user guide](docs/user/README.md)** (em português:
**[guia do usuário](docs/user/pt-BR/README.md)**): installing on each system,
screen permissions and Windows drivers, the first theme, sensors, game FPS,
storage and video, GIFs and stickers, running at login, what the screen does
when the computer shuts down, and troubleshooting.

The installers are not signed: Windows SmartScreen may ask you to confirm
(*More info → Run anyway*); see [Install Bezel](docs/user/install.md).

## Windows Light runtime and sensors

This fork adds a lightweight background runtime and integrated hardware sensors:

- **Multiple screens.** Studio remembers a theme and orientation for each
  screen. Light can run the saved active screens together with
  `--screens-config`, sharing the sensor worker across displays.
- **Libre status and diagnostics.** Studio shows the reader's health and
  offers a restart of the configured helper. Bounded helper and reader logs
  record unavailable sensors and fallback errors. A heartbeat from a previous
  login no longer causes the helper to exit immediately at startup.

- **Close Studio to Light.** In Preferences, **Use Bezel Light when closing
  Studio** is enabled by default. When a theme is live, the window's X handles
  unsaved changes, starts or resumes Light with the saved theme, and exits
  Studio to free the editor's memory. Turn the switch off to keep live Studio
  in the tray. Cancelling the unsaved-changes dialog keeps Studio open; if
  Light cannot start, Studio stays open and shows the error. **Quit** stops
  both runtimes.
- **Open Studio from Light.** Light runs without a WebView and has a native
  tray menu with **Open Studio** and **Quit**; double-click also opens Studio.
  Opening Studio directly pauses Light and transfers control of the screen.
  Keep `bezel.exe` and `bezel-studio.exe` in the same installation folder.
- **Integrated Windows sensors.** The bundled headless helper uses
  LibreHardwareMonitorLib 0.9.6 for CPU temperatures, power and fan readings,
  without opening the Libre GUI or its web server. Run
  `configure-sensors.cmd` as administrator once with your normal Windows
  account; only the helper needs elevation. Studio and Light remain
  unelevated. The helper starts on demand and stops after 30 seconds without
  active clients. External Libre WMI or its local web server remains a
  fallback. See the [sensor helper documentation](apps/bezel-sensors-helper/README.md).
- **Start Light at login.** Save a theme and select its live screen in Studio,
  then use `start-light.cmd` beside the executables. For login setup, run
  `configure-startup.cmd` as administrator after checking the installation
  paths. Setup also configures the sensor helper, backs up the existing Bezel
  startup entry, and removes the three legacy Turing startup tasks named in
  the [Windows setup guide](scripts/windows/README.md). If you move or rename
  the installation folder, rerun setup to update the login entry and helper
  task. Windows may also assign a different COM port after reconnecting a
  display; select its current port in Studio. Login testing exposed both
  stale paths and changed ports; these were corrected on the test PC. The
  next login still needs to confirm the updated configuration.

- **Local video previews.** **Media → This theme → Preview** and
  **Screen → Storage → Preview** open a player with play, pause, seek and loop
  controls. Storage previews use the associated local copy; missing copies
  can be linked through **Associate original file…**. Previewing does not
  change the theme or start playback on the screen. Files up to 64 MiB are
  supported; codecs depend on WebView. Videos stored only on the device
  cannot be retrieved through the current protocol.

Other additions include offline Tabler and hardware icons, Italian icon search,
configurable dates and clocks, object copy/paste, weather widgets, ring gradients,
stored screen videos as backgrounds, and shaped windows. See
[the fork's changes and validation](FORK.md) for details.

## Bezel Studio

`bezel-studio` (from the packages, or `scripts/install-local.sh`) is the app: one
window to design themes and drive the screen.

- **Design by dragging**: widgets (text, value, clock, image, shape, bar, ring,
  needle, graph) and sensors from the library onto the canvas; move, resize with
  snapping guides, align, layers, undo/redo. The canvas shows exactly what the
  screen gets (the same renderer).
- **Vertical or horizontal**: one click in the top bar turns the theme (the
  layout follows: a vertical stack becomes a horizontal row), and *Rotate 180°*
  when the cable comes out the other side.
- **Live**: the switch shows the theme on the screen while you edit. On Windows,
  closing the window hands the saved theme to Light by default; disable
  **Use Bezel Light when closing Studio** to keep Studio in the tray. On Linux,
  live Studio stays in the tray. *Start with the computer* (Screen panel)
  brings Studio back at login; Windows Light login setup is described above.
- **Themes**: bundled ones for each screen size, your own library, and import of
  the vendor app's `.turtheme` and turing-smart-screen-python themes.
- **Video backgrounds**: the canvas plays the video (up to 15 pictures a
  second), and *Framing* turns it, fills or fits, zooms and places it, also by
  dragging on the canvas. A video already turned for the panel, like the vendor
  app's Dragon Ball theme, stands upright on its own
  ([Framing the video](docs/user/storage-and-video.md#framing-the-video)).
- **GIFs and stickers**: search KLIPY with your own free key and keep what you
  like in a local collection, ready to add as an image, use as the background
  or drag onto the canvas; a sticker's transparency shows the theme under it
  ([GIFs and stickers](docs/user/gifs-and-stickers.md)).
- **Screen panel**: brightness, and the *Storage* tab for the screen's internal
  flash and memory card (send pictures and videos, play them, choose what the
  screen shows at power-up), which opens full width as the storage manager:
  both sides with thumbnails, move by dragging, rename, restore a card, the
  cleanup assistant.
- **When the computer shuts down** (Screen panel, Turing rev C screens): leave
  the screen as it is, turn it off, loop a video stored on it, or show the
  card's photo album instead of a frozen last frame; `bezel standby` makes the
  same choice from a terminal
  ([When the computer shuts down](docs/user/power-off.md)).

`BEZEL_FAKE=1 bezel-studio` opens it with a simulated 8.8" and demo sensors.

## Quick start

```bash
bezel devices                             # list connected screens (read-only)
bezel test-pattern --orientation horizontal --seconds 5
bezel show wallpaper.png                  # horizontal for a wide picture, vertical otherwise
bezel show poster.jpg --orientation vertical --fit contain
bezel brightness 40
bezel off                                 # the next command wakes the screen
bezel release                             # back to the screen's own clock/media
bezel restart                             # a rev C screen that stopped responding, no replug
bezel sensors                             # every sensor of this machine
bezel sensors --watch 1 --json            # live, one JSON document per second
```

Use the screen standing up or lying down: `test-pattern` and `show` take
`--orientation vertical|horizontal` (or `vertical-flipped`, `horizontal-flipped`
when the cable comes out the other side), and a theme carries its own orientation.

## Themes

```bash
bezel run turing-8.8-horizontal           # a bundled theme with live sensors; Ctrl+C hands the screen back
bezel run ~/themes/mine.bezeltheme --screen /dev/ttyACM1
bezel render turing-8.8-vertical -o preview.png   # one frame to a PNG of the canvas size
bezel import AMD.turtheme -o amd.bezeltheme       # a TURZX or turing-smart-screen-python theme, converted
```

A theme is a `.bezeltheme` file (a zip of `theme.json` and `assets/`) or the same
layout as a folder. `render` and `run` also take the vendor app's `.turtheme`
files and turing-smart-screen-python theme folders (or their `theme.yaml`),
converted on the fly; `import` saves the conversion and lists what could not be
carried over.

Bundled themes (`themes/`, the "Midnight" set): `turing-8.8-horizontal` (1920x480),
`turing-8.8-vertical` (480x1920), `turing-5-horizontal` (800x480),
`turing-3.5-vertical` (320x480), `turing-3.5-horizontal` (480x320) and
`turing-2.1-round` (480x480, also the 2.8" round). They are looked up in
`$BEZEL_THEMES_DIR`, then `<bezel>/../share/bezel/themes`, then
`~/.local/share/bezel/themes`, and use the bundled Inter and JetBrains Mono
fonts (`themes/fonts/`, SIL Open Font License 1.1).

### Run a theme at login without a window

The studio does this from its tray ("Start with the computer" in the Screen
panel). Without the studio:

- **Linux** (systemd user service: the deb and rpm packages install it in
  `/usr/lib/systemd/user`, `scripts/install-local.sh` in `~/.config/systemd/user`):
  ```bash
  systemctl --user enable --now bezel-run@turing-8.8-horizontal
  journalctl --user -u bezel-run@turing-8.8-horizontal -f   # its log
  ```
  For your own theme file, override the command once (`systemctl --user edit bezel-run@mine`;
  `%h/.local/bin/bezel` instead of `/usr/bin/bezel` for a from-source install):
  ```ini
  [Service]
  ExecStart=
  ExecStart=/usr/bin/bezel run %h/themes/mine.bezeltheme
  ```
- **Windows** (a logon task; `bezel.exe` comes in the release's CLI `.zip`, here
  unpacked into `C:\Tools\bezel`):
  ```powershell
  schtasks /Create /SC ONLOGON /TN "Bezel" /TR "C:\Tools\bezel\bezel.exe run turing-8.8-horizontal"
  ```

Stop any other program that drives the screen first (a turing-smart-screen-python
service, the vendor app): `bezel` refuses a port another process holds. More in
[Running at login](docs/user/run-at-login.md).

## Screen storage and video

Screens with storage (Turing rev C, the Turing USB generation) keep pictures and
videos in four folders — `internal/image`, `internal/video`, `sd/image` and
`sd/video` (`sd` is the memory card, reached only through the screen; Bezel never
formats it) — and play them on their own.

```bash
bezel storage info                        # flash and card: used and free (--json)
bezel storage ls                          # every stored file; or one folder: bezel storage ls sd/video
bezel storage put clip.mp4                # converted for the panel when needed, with progress
bezel storage put logo.png sd/image/logo.png
bezel storage play internal/video/clip.mp4   # the screen loops it itself (--once: plays it once)
bezel storage stop
bezel storage rm internal/video/clip.mp4 --yes
bezel storage boot internal/video/clip.mp4 --brightness 60 --yes   # start with a video: the first of sd/video
bezel storage boot default --yes          # back to the built-in start screen
```

- **Confirmation.** Whatever deletes, replaces or persists (`rm`, `put` over a
  stored file, `boot`) first prints what it is about to do, and needs `--yes`;
  without it nothing is sent to the screen and the exit code is 1.
- **Sending.** Pictures (JPEG, PNG, BMP, GIF) go as they are. A video is converted
  with ffmpeg to the panel's native format (480x1920 H.264 MP4 without audio on
  the 8.8"): turned for `--orientation` (default: the clip's own shape), cropped
  to the panel's shape (never stretched), `--fps 24` optional. A clip already in
  that format goes as it is. ffmpeg is not bundled: `--ffmpeg PATH`, else the one
  on `PATH`; without it only clips already in the format go, and `put` says how
  to install it. File names are lower-case `a-z 0-9 _ . -`, files up to 120 MB.
- **Progress and Ctrl+C.** `put` draws its progress (convert, upload, verify) on
  stderr; Ctrl+C cancels it and says how to delete what was left
  (`bezel storage rm ... --yes`); a second Ctrl+C quits at once.
- **Full screen.** Nothing is ever deleted for you: the refusal lists the stored
  files, largest first, to choose from.
- **Boot media.** Rev C screens store the boot choice together with the
  brightness they boot with: `--brightness`, else the vendor default (about 67%).
  The choice is a start setting and the screen picks the file (measured on the
  8.8"): a picture makes it show every picture of `sd/image` in turn, a video
  makes it play the first video of `sd/video`, not necessarily the one named.
  On the Turing USB generation Bezel cannot yet delete files, play a video once
  or change the boot media; `bezel` says so.
- **Storage manager.** Bezel keeps a local copy of every file it sends
  (`<data>/bezel/storage`): `bezel storage mv` and `rename` send it again and
  delete the source only after the copy is verified, `restore` refills a
  formatted or new card, `cleanup --dry-run` lists the vendor app's duplicates
  and interrupted uploads, and `cache clear` frees the copies
  ([Managing the files](docs/user/storage-and-video.md#managing-the-files)).
- **Themes with a video background.** `bezel run` has the screen loop the video
  as the theme frames it and draws the theme over it. When the screen does not
  store the video yet, the poster shows and `bezel run` says how to send it: the
  exact `bezel storage put` command, or, for a video framed in Bezel Studio
  (turned, fitted, zoomed or moved), the studio's *Send to the screen*, since
  the command line cannot frame a video. A video already in the panel's format
  goes as it is, without ffmpeg. Screens that cannot play videos get them
  decoded on this computer (ffmpeg, `bezel run --ffmpeg PATH`).

## Supported screens

| Family | Examples | Link |
|---|---|---|
| Turing rev A | Turing Smart Screen 3.5", UsbPCMonitor 3.5"/5"/7" | serial |
| XuanFang rev B | XuanFang 3.5" (and Flagship) | serial |
| Turing rev C | Turing 2.1"–8.8" (the 8.8" is hardware-validated) | serial + wake MCU |
| Kipye rev D | Kipye Qiye 3.5" | serial |
| WeAct | WeAct Studio Display FS 3.5", 0.96" | serial |
| Turing USB (0x1CBE) | TURZX 1.6"–12.3" USB generation | USB bulk |
| WCH (0x43A8) | WCH-based 2.4"–4.3" panels | USB bulk |

`bezel devices` lists what is connected and which models match. A Turing USB
panel in the vendor's desktop mode is listed as "desktop mode (not validated on
hardware)", and `bezel monitor-mode --yes` switches it back to USB monitor mode
([Supported screens](docs/user/devices.md)).

## Build from source

```bash
cargo build --release --locked
bash scripts/install-local.sh   # installs into ~/.local (no sudo)
```

On Linux your user needs access to the screen: the deb and rpm packages install
the udev rule; for a build from source, `bezel udev-rules` prints it and the
one-line sudo command that installs it (Bezel never runs it itself). See
[Let Bezel open the screen](docs/user/permissions.md).

## License

GPL-3.0-or-later. Bezel is a clean reimplementation; protocol knowledge comes from the
GPL-3.0 [turing-smart-screen-python](https://github.com/mathoudebine/turing-smart-screen-python)
and from interoperability analysis documented in `docs/reverse-engineering/`.
