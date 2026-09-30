# Bezel

**The open studio for USB smart screens.** Bezel drives the little USB system-monitor
screens sold as Turing Smart Screen, TURZX, XuanFang, Kipye, WeAct and their OEM
rebrands — on Linux and Windows, from one app.

> Status: early development. Every protocol family is implemented and the Turing
> 8.8" is validated on real hardware; sensors are measured on Linux and Windows;
> the renderer, headless themes (`bezel run`) and the studio's editor work;
> screen storage and video backgrounds work (`bezel storage` validated on the
> 8.8"; the studio's Storage tab awaits its hardware check); packaging and the
> 1.0 release come next.

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
- **Live**: the switch shows the theme on the screen while you edit; closing the
  window keeps it running from the tray. *Start with the computer* (Screen panel)
  brings it back at login.
- **Themes**: bundled ones for each screen size, your own library, and import of
  the vendor app's `.turtheme` and turing-smart-screen-python themes.
- **Screen panel**: brightness, and the *Storage* tab for the screen's internal
  flash and memory card (send pictures and videos, play them, choose what the
  screen shows at power-up).

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

- **Linux** (systemd user service, installed by `scripts/install-local.sh`):
  ```bash
  systemctl --user enable --now bezel-run@turing-8.8-horizontal
  journalctl --user -u bezel-run@turing-8.8-horizontal -f   # its log
  ```
  For your own theme file, override the command once (`systemctl --user edit bezel-run@mine`):
  ```ini
  [Service]
  ExecStart=
  ExecStart=%h/.local/bin/bezel run %h/themes/mine.bezeltheme
  ```
- **Windows** (a logon task; `bezel.exe` comes in the release's CLI `.zip`, here
  unpacked into `C:\Tools\bezel`):
  ```powershell
  schtasks /Create /SC ONLOGON /TN "Bezel" /TR "C:\Tools\bezel\bezel.exe run turing-8.8-horizontal"
  ```

Stop any other program that drives the screen first (a turing-smart-screen-python
service, the vendor app): `bezel` refuses a port another process holds.

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
bezel storage boot internal/video/clip.mp4 --brightness 60 --yes   # shown after power-up
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
  On the Turing USB generation Bezel cannot yet delete files, play a video once
  or change the boot media; `bezel` says so.
- **Themes with a video background.** `bezel run` has the screen loop the video
  and draws the theme over it. When the screen does not store the video yet, the
  poster shows and `bezel run` prints the exact `bezel storage put` command;
  screens that cannot play videos get them decoded on this computer (ffmpeg,
  `bezel run --ffmpeg PATH`).

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

`bezel devices` lists what is connected and which models match.

## Build from source

```bash
cargo build --release --locked
bash scripts/install-local.sh   # installs into ~/.local (no sudo)
```

Linux needs read/write access to the screen's serial port: install
`packaging/linux/60-bezel.rules` into `/etc/udev/rules.d/` (the deb/rpm packages do it).

## License

GPL-3.0-or-later. Bezel is a clean reimplementation; protocol knowledge comes from the
GPL-3.0 [turing-smart-screen-python](https://github.com/mathoudebine/turing-smart-screen-python)
and from interoperability analysis documented in `docs/reverse-engineering/`.
