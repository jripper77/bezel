# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/). Versions are computed by CI from
the Conventional Commits.

## [Unreleased]

### Added
- Cargo workspace with the hexagonal core (`bezel-core`), the device adapter
  (`bezel-devices`), the `bezel` CLI and the Bezel Studio app.
- `bezel devices`: lists the connected smart screens without writing to them.
- Reverse-engineering specification of every supported protocol and file
  format in `docs/reverse-engineering/`.
- Screen drivers for every protocol family: Turing rev A, XuanFang rev B,
  Turing rev C (with the wake micro-controller), Kipye rev D, WeAct Studio
  Display FS, the Turing/TURZX USB generation (0x1CBE) and WCH panels
  (0x43A8); 44 models in the catalog, the Turing 8.8" validated on hardware.
- `bezel test-pattern`, `bezel brightness`, `bezel show <picture>`,
  `bezel off` and `bezel release`; orientations are `vertical`,
  `horizontal` (or `portrait`, `landscape`) and their flipped forms.
- A screen held by another program is refused with that program's name and
  PID instead of garbling both streams.
- `bezel sensors`: every sensor of the machine (CPU per core, temperatures of
  every hwmon chip, NVIDIA and AMD GPUs, memory, disks, network) as a table,
  JSON or a live `--watch`; a sensor that cannot be read says why instead of
  showing a guess. Windows reads LibreHardwareMonitor when it runs.
- `bezel render <theme> -o out.png` renders one frame of a theme with the
  machine's sensors (or the demo values with `--fake`) to a PNG of the canvas
  size; it previews `.turtheme` and turing-smart-screen-python themes too.
- `bezel run <theme>` shows a theme on the screen with live sensors at the
  theme's refresh rate until Ctrl+C (or SIGTERM), then hands the screen back to
  its standalone mode.
- `bezel import <src> -o <dst>` converts a TURZX `.turtheme` or a
  turing-smart-screen-python theme into a native `.bezeltheme` (or folder) and
  prints what could not be converted exactly.
- Bundled "Midnight" themes for the 8.8" (horizontal and vertical), 5", 3.5"
  (both ways) and 2.1"/2.8" round screens, with the Inter and JetBrains Mono
  fonts (SIL Open Font License 1.1); a theme can be named instead of a path.
- Bezel Studio, the desktop app: a single window with a drag-and-drop theme
  editor (widgets and sensors from the library onto a canvas that shows the
  real renderer's frame; snapping, alignment, layers, undo/redo, pt-BR and
  English, light and dark), vertical or horizontal themes with one click, live
  mode on the screen that keeps running from the tray, start at login, the theme
  library with bundled themes and import of other apps' themes.
- Bezel Studio: a Storage tab in the Screen panel — usage bars for the internal
  flash and the memory card, files per folder, sending by drag-and-drop with a
  progress bar and Cancel, play/stop, and delete or the boot media behind a
  confirmation dialog naming the file; a theme with a video background offers
  "Send to the screen" and then plays over the video the screen loops; for screens
  that cannot play videos, live mode decodes it on the computer, as `bezel run`
  does.
- `bezel storage info|ls|put|rm|play|stop|boot`: the screen's internal flash
  and memory card (sizes, `--json`), sending pictures and videos with a
  progress bar and Ctrl+C to cancel, device-side playback and the boot media.
  Deleting, replacing and changing the boot media print what they will do and
  need `--yes`; without it nothing reaches the screen.
- `bezel storage put` converts a video to the panel's format with an external
  ffmpeg (`--ffmpeg PATH`, else `PATH`): turned for `--orientation`, cropped to
  the panel's shape instead of stretched, optional `--fps`; without ffmpeg,
  clips already in the format still go and the install command is shown. A
  full screen lists what could be deleted and deletes nothing.
- `bezel run` with a video-background theme has the screen loop the stored
  video under the theme, shows the poster with the `bezel storage put` command
  when the video is missing, and decodes it on the computer for screens that
  cannot play videos (`--ffmpeg PATH`).
- Bezel Studio gives a theme a video background: **Add video…** in the Media
  tab, or a video or animated GIF dropped on the tab or on the editing area,
  copies it into the theme with a poster taken by ffmpeg (shown under the
  elements; without ffmpeg the video comes without one), and **Properties**
  shows it with whether the screen stores it. An animated GIF sent to a screen
  is converted to a video at a constant frame rate.
- A cancelled upload reports the incomplete file it left and the command that
  deletes it; an upload whose stored size differs from the file says to delete
  it and send it again.
- A Turing rev C screen that froze (it stopped reading what Bezel sent, or it
  is on the bus but answers nothing) is restarted through its wake chip
  without a USB replug: once, on its own, by the next connection (the next
  command, `bezel run` started again, turning Live on), with
  `bezel restart [-s SCREEN]`, or with **Restart screen…** in the studio's
  Screen panel, which errors meaning a frozen screen also offer. It is back in
  about 10 s; what it played stops, its stored files stay.
- Animated GIF elements move at their own pace, between the theme's
  refreshes: up to 30 frames a second on the screen (only the GIF's rectangle
  is sent; a slow link skips frames instead of lagging) and 15 in the studio's
  preview, while sensors keep the theme's refresh. Full-screen GIFs belong in
  a video background.
- Live mode recovers from a screen that stops taking frames: `bezel run`, the
  service and the studio connect it again after 2, 5 and 10 s (restarting a
  frozen rev C screen on the way, found again under its new port), say so
  meanwhile, and stop with the error after the third attempt.
- `bezel udev-rules` prints the Linux udev rule generated from the device
  catalog and the one-line sudo command that installs it, for AppImage,
  archive and source installs; Bezel never runs it. A refused port points at
  it, and the studio shows the command, ready to copy.
- Turing USB panels in the vendor's desktop mode (1a86:ad10–ad13) are listed by
  `bezel devices` as "desktop mode (not validated on hardware)", and
  `bezel monitor-mode --yes` switches one back to USB monitor mode; without
  `--yes` (or the studio's confirmation) nothing is sent.
- Game FPS (`gpu.fps`), read-only: the RivaTuner Statistics Server shared
  memory on Windows, the newest MangoHud CSV log on Linux (`--mangohud-dir`).
  Nothing measuring a game, or a reading older than 3 s, is unavailable with
  how to turn the source on. Not yet validated with a real game.
- `net.ping`, the round trip to `--ping-host` (default 8.8.8.8), measured on a
  thread of its own so a silent host never delays the other sensors, and only
  while a shown theme or the studio's sensor list uses it (no traffic
  otherwise); fans,
  pump, voltages, network totals and available memory from hwmon/sysfs on
  Linux and LibreHardwareMonitor on Windows; the sensor keys of imported themes
  map to Bezel's.
- Bezel Studio in Portuguese and English throughout, following the system
  language unless one is chosen in Preferences; errors and import warnings are
  translated; the ping target and the MangoHud log folder are Preferences.
- Bezel Studio's Themes tab shows each theme as a thumbnail drawn by the real
  renderer with sample values (a video background shows its poster), kept in
  the app's cache until the theme changes, and names the screen it was made
  for (`8.8″ · 1920×480`). **For this screen** (the default while one is
  connected) / **All** and **Vertical** / **Horizontal** filter the list; the
  choice is remembered.
- Linux packages: the deb and the rpm install the `bezel` command as
  `/usr/bin/bezel`, the `bezel-run@` systemd user service in
  `/usr/lib/systemd/user` (running `/usr/bin/bezel`) and the bundled themes
  where the command finds them, next to the udev rule; installing applies the
  rule to serial, USB and HID devices at once.
- The storage manager on the command line:
  `bezel storage mv|rename|restore|cleanup|catalog|cache`. Bezel keeps a local
  copy of every file it sends (the exact bytes, in `<data>/bezel/storage`:
  `~/.local/share` on Linux, `%APPDATA%` on Windows), since the screens cannot
  send a file back, rename or move it. `bezel storage mv PATH... --to internal|sd`
  and `rename` send that copy again, check the stored size and only then delete
  the source; a failure or Ctrl+C keeps the source and stops the batch.
  `restore internal|sd` sends back the files missing from a formatted or new
  card, after checking the free space and the 25 MiB rev C limit before the
  first byte, and never deletes. `cleanup` lists the vendor app's duplicate
  copies (`x.mp4.mp4`, `x.mp4<digits>.mp4`), interrupted uploads and files no
  theme plays, never the boot media nor a theme's video; `cleanup --dry-run`
  only lists, `--yes` deletes exactly the pre-checked files it printed.
  `catalog associate` gives a file Bezel did not send a local copy from its
  original; `cache clear` removes the copies of deleted files (`--all`: every
  copy), which count toward a 2 GiB limit (`cache --limit`). Each command
  prints the exact list first and changes nothing without `--yes`.
- Bezel Studio: the Storage tab opens full width as the storage manager —
  internal memory and SD card side by side with thumbnails, multiple selection
  with the mouse or the keyboard, sort and filter, **Move to the SD card** (or
  dragging to the other side), copy, rename, restore (also of files deleted
  through Bezel, listed unchecked), the cleanup assistant with its exact list
  to confirm (a file that changed since is not deleted), associating an
  original and the local copies with **Clear cache…**; progress and results
  are announced to screen readers.
  On Turing USB screens, what ends in a delete (move, rename, delete, cleanup)
  is disabled with the reason, and sizes come from the catalog or show as
  unknown.
- User guide in English (`docs/user/`) and Portuguese (`docs/user/pt-BR/`):
  installing on each system, screen permissions and Windows drivers (WinUSB
  with Zadig, LibreHardwareMonitor), the unsigned installers and SmartScreen,
  the first theme, vertical or horizontal use, sensors, game FPS, storage and
  video with the storage manager, ffmpeg, preparing an SD card, running at
  login, coming from turing-smart-screen-python, troubleshooting and the
  supported screens.
- Video background framing in Bezel Studio: **Properties → Framing** turns the
  video (Auto, or 0, 90, 180 or 270°), fills or fits it (with a pad color),
  zooms it (100 to 400%) and places it (Position X and Y, Center, Reset
  framing); **Frame on canvas**, or a double-click on the video, does it on the
  editing area with the mouse (drag, wheel) and the keys (arrows, + and −, 0,
  Esc or Enter), each change or gesture one undo step. The editor's preview
  plays the video background, framed, at up to 15 pictures a second (the
  poster when motion is reduced or without ffmpeg). A theme keeps an optional
  `framing` in its video background (`rotation`, `fit`, `zoom`, `position`,
  `padColor`), left out when it is the default; `bezel run` honours it. A
  re-framed video goes to the screen as a copy of its own, named with `_f` and
  8 hex digits, and Bezel deletes no earlier copy. See
  [Framing the video](docs/user/storage-and-video.md#framing-the-video).
- GIF and sticker search in Bezel Studio, from KLIPY with your own free key:
  the Media tab's **Collection** (next to **This theme**) opens **Search GIFs
  and stickers** — GIFs or stickers, by text or **Trending**, 24 at a time with
  **Load more**, explicit results only while **Show explicit results** is on
  (off at every start). The key stays on this computer (`klipy.json` in the
  app's config folder, readable only by you) and shows only its last 4
  characters; without a key, or before you search, nothing reaches KLIPY. A
  test key allows 100 requests an hour, and the help says how to ask for
  production in KLIPY's Partner Panel. **Add to collection** keeps the largest
  GIF of at most 25 MiB, once per content, in your local collection, where each
  item can be added as an image, used as the background, dragged onto the
  canvas, renamed or deleted (the confirmation names the themes that use it;
  they keep their own copy). A sticker's transparency shows the theme under
  it. See [GIFs and stickers](docs/user/gifs-and-stickers.md).

### Changed
- Turing rev C screens take at most 25 MiB per file: their firmware keeps a
  whole upload in memory and froze past about 28 MiB. A larger file is refused
  before anything is sent, with the limit in MiB; a conversion caps the
  video's bitrate from its length so it fits, and a converted video still over
  the limit is refused with how to make it fit (a shorter clip, `--fps`).
  Turing USB screens keep the vendor's 120 MB.
- A theme file that cannot be read or does not fit its screen fails with
  `theme file: …` instead of a transport error; the studio opens a
  `theme.json` like the command line does.
- A file a Turing USB screen stores without reporting its size counts as
  present, with an unknown size, when listing, playing and in video
  backgrounds.
- `scripts/install-local.sh` points the `bezel-run@` service it installs at the
  `bezel` it installed.
- `bezel storage ls` shows, for the files Bezel sent, their state (stored, or
  pending: an upload that did not finish) and whether Bezel keeps a local copy;
  `put`, `rm` and `boot`, and the studio's sending, deleting and theme videos,
  record what they do in Bezel's catalog.

### Fixed
- A rev C screen that another app just turned off (turing-smart-screen-python
  and the vendor app send TURNOFF on exit) is woken instead of failing with
  "Broken pipe".
- A theme video stored turned for the screen, as in the vendor's horizontal
  themes (Dragon Ball: 480x1920 in a 1920x480 theme on the 8.8"), is turned
  back by the framing's Auto instead of being zoomed past the screen, and the
  `dragon.mp4` the screen already stores is found (Bezel looked for
  `dragon_90.mp4`) and used without sending it again when its size matches;
  framed by default, such a video is sent as it is, without ffmpeg. The
  studio's preview plays a video background instead of showing its poster.
- With a screen live in Bezel Studio, its controls work again. An 8.8" that
  went live through its wake micro-controller's port (from the tray, or when
  the studio resumed after a restart) failed brightness changes with "Device or
  resource busy", hid its SD card and made the Themes tab's **For this screen**
  filter flicker. Now the brightness changes on the live screen, the **Storage**
  tab shows the internal memory and the SD card and manages them, and
  **For this screen** stays enabled and steady.
