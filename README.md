# Bezel Evo

**A Windows-focused evolution of Bezel for USB smart screens.**

Bezel Evo combines a visual theme editor with a Light background runtime,
local hardware monitoring and support for multiple displays. This repository
contains the source and documentation for this fork.

Current development release: **0.1.17**. Object properties follow a consistent
order: name/type, position and size, content/data, appearance, membership,
playback/animation and layer/object actions. Sections are separated by horizontal lines and can be collapsed; they retain
their state while editing; irrelevant sections are omitted. Sensor bindings,
clock/weather configuration and image assets are separate from visual styling.

Right-click an object in the canvas or Layers for cut/copy/paste, grouping,
ordering, visibility and locking actions. Arrow keys move the selection (Shift
moves by 10 px). Layers accept drops into groups, card faces and the root level.
Shapes support shared or individual corner radii; media covers have radius and
text-gap controls. Each card trigger can return to an explicit face after its
configured delay. Deselect the card and its objects to test automatic playback.

In **Themes**, **Save as .bezeltheme...** opens a file dialog to save the current
layout and its available assets wherever you choose (also Ctrl+Shift+S).
The top **Save** button saves to the current location or the internal library.
A background video referenced only in screen storage stays a screen reference;
it is not downloaded from the display when saving a theme.

Development happens on [`windows-improvements`](https://github.com/jripper77/bezel/tree/windows-improvements).
See [FORK.md](FORK.md) for implementation details, validation and limitations.

## What this fork adds

- **Light runtime with a native tray icon:** runs saved themes without keeping
  the editor's WebView open. Open Studio from the tray; closing Studio returns
  control to Light. Unsaved changes are handled before closing.
- **Multiple screens:** separate themes, orientations and live selections;
  Light drives the saved active screens together and shares sensor sampling. Studio delivers frames on an independent
  worker per display, with one outstanding frame per connection and completion
  notifications to keep slow screens from pausing faster ones. Rendering and
  sensor state remain shared.
- **Integrated Windows sensors:** a headless LibreHardwareMonitorLib 0.9.6
  helper reads CPU temperature, fans, power and other available sensors. Only
  the configured helper requires administrator rights. Studio provides reader
  health, restart and diagnostic logs. The helper does not control fans or LEDs.
- **Windows serial fixes:** bounded writes, transfer diagnostics and retries
  for transient port access errors, tested on Turing 8.8-inch Rev C ROM 1.90.
  Protocol framing and command bytes are preserved.
- **Offline vector icons:** searchable Tabler and hardware collections, with
  color, outline, opacity and shadow editing, plus Italian search aliases.
- **Customizable dates and clocks:** language, presets, custom formats and
  casing per object, including Italian dates and day names.
- **Ordinary object groups:** group selected objects without creating a card or
  animation, including nested groups and groups within card faces. Move/resize
  a group together and edit individual members through Layers.
- **Object copy and paste:** duplicate single objects or selections while
  preserving properties, with independent IDs and Undo support.
- **Card widgets:** shared base and named faces, manual face selection, face
  duplication and ordering, grouping existing objects, movement, resizing and
  clipboard support. Face changes support fade, slide and flip transitions,
  configurable duration/direction and optional base movement. Rotate faces on a timer
  or select a face while an app runs, is closed, is in foreground or plays media.
  Rules have priorities and a return delay; rotation resumes after the override.
- **Weather widgets:** city search, units, language, icon size and spacing, with
  outline, filled, colored and shaded 3D-style vector icons authored in this fork.
- **Windows media player:** display title, artist, cover and timeline from Windows
  media sessions. Filter an app (e.g. Spotify) or automatically prefer a playing
  session; hide stopped sessions or configure the empty label. App-provided
  metadata and timing vary; this widget does not send playback commands.
- **Image and shape transparency:** shared linear/radial controls with center,
  radius and start/end opacity. Images retain their existing fit/framing.
- **Screen sleep timer:** discoverable under Screen settings; default 5 minutes,
  per-display persistence and capability checks. Turing Rev C firmware supports
  1 to 10 minutes after PC shutdown; active Live updates keep the display awake.
  Choose Keep in the shutdown options to disable the Off fallback.
- **Ring gradients:** start/end colors and an adjustable transition along the
  full arc, plus a checkbox to preview the ring at 100%.
- **Stored-video backgrounds:** select a video in Screen Storage as the
  background, with looping and shape windows. Shape windows support linear/radial opacity.
- **Local video previews:** a player in Media and Screen Storage with playback,
  pause and seeking. Storage uses an associated local copy and offers
  original-file association when missing. Previewing does not alter the theme.
- **Login and shutdown integration:** launch Light and the helper at login and
  apply saved shutdown actions through the active screen connection.
- **Recovery after long pauses:** Light detects a tray-loop pause of at least
  ten seconds, stops the old render workers, restarts the configured sensor
  helper off the UI thread, verifies a fresh sensor snapshot and reopens the
  saved displays. Studio and Light also retry a missing or stalled bundled
  reader from their sensor worker. The wake MCU and
  display port of one screen no longer create duplicate workers.

## Measuring Live animation performance

In Studio Preferences, enable **Debug** to write app, sensor and device diagnostics
into `debug.log` beside `settings.json` (the full path appears in Preferences).
**Show readings on screen**, nested under Debug, adds a temporary overlay to the
physical Live display. Choose any of the four corners with **Screen corner**. Turning Debug off stops file logging and hides the overlay;
the theme file is unchanged. Logs keep one previous 5 MiB segment and exclude
network-client targets.

Preview and Live reuse recent identical scene frames within one animation interval
(excluding video backgrounds). Debug logs record reuse, preview/editor-lock times
and refresh-loop preparation times to help distinguish rendering from transfer costs.

`SEND FPS` measures successful host transfer completion during animation bursts,
not the panel's internal scanout. `GAP` is the interval between completions;
`RENDER` measures theme rendering and `PRESENT` the complete driver operation.
`handoff_ms` in the logs measures the delay from a prepared frame to its driver
starting; stopped/replaced connections reject completions from older generations.
For Rev C, `C` is rotation/BGRA conversion, `D` difference encoding, `W` serial
write/drain, `A` reply wait, and `KB` the framed bytes sent. `S` shows sensor
sampling time / configured sampling interval in milliseconds. Other drivers show
unavailable transfer phases as `N/A`. FPS requires at least two transfers and
resets after a pause over 300 ms. The overlay uses the previous measurements and
updates at most four times per second; `animating` in the logs identifies active card transitions; it adds some changed pixels, so compare
with logging enabled and the overlay disabled for the final timing capture.

## Build and setup on Windows

**No binary releases have been published for this fork yet.** Future packages
will appear on [Bezel Evo's releases page](https://github.com/jripper77/bezel/releases).

Build with Rust 1.98 or newer, Microsoft C++ build tools and Edge WebView2:

```powershell
git clone --branch windows-improvements https://github.com/jripper77/bezel.git
cd bezel
cargo build --release --locked -p bezel -p bezel-studio
```

Place `target/release/bezel.exe` and `target/release/bezel-studio.exe` together
in your installation folder, with the scripts from [scripts/windows](scripts/windows/).
Open Studio, save a theme and select the screens to run.

The integrated sensor helper has a separate build using the Windows .NET
Framework compiler and LibreHardwareMonitorLib. Follow the
[helper build and setup instructions](apps/bezel-sensors-helper/README.md).
Cargo alone does not bundle the helper or its dependencies.

Run `configure-startup.cmd` as administrator with your normal Windows account
for login setup. Only the hardware reader runs elevated. See the
[Windows launcher guide](scripts/windows/README.md) for startup, logs and rollback.
If you move the installation folder, update the startup configuration.
If Windows changes a display's COM port, select its current port in Studio.

## Documentation

- [Fork changes and validation](FORK.md)
- [Windows Light runtime and startup](scripts/windows/README.md)
- [Integrated sensor helper](apps/bezel-sensors-helper/README.md)
- [Storage, video and local previews](docs/user/storage-and-video.md)
- [Troubleshooting](docs/user/troubleshooting.md)
- [Source structure and dependencies](Cargo.toml)

## Tested hardware and current limits

Testing focuses on Windows with a Turing Smart Screen 8.8-inch Rev C ROM 1.90
and a Turing 3.5-inch display. Other drivers and Linux support are inherited
from Bezel; these additions have not been physically verified on every model
or on Linux.

Local video previews require a copy on the PC, are limited to 64 MiB and depend
on WebView codecs. The current protocol cannot retrieve device-only videos;
those backgrounds show a placeholder in the editor. Partial alpha blending
over stored video on ROM 1.90 still needs hardware confirmation.

The corrected login configuration has been tested by launching Light and the
helper; the next Windows login must confirm the latest path and port fixes.

## Credits and licenses

Bezel Evo is a derivative of **Bezel**, created by
**[Alison Amorim (@slipalison)](https://github.com/slipalison)**.
The original code, editor, protocol research, bundled themes and inherited
documentation remain credited to their authors. Thank you to Alison for making
that work available. The rename identifies this fork; inherited components
retain their original authorship and copyright notices.

- Bezel and this fork: [GPL-3.0-or-later](LICENSE).
- Tabler Icons: [MIT](apps/bezel-studio/src/assets/tabler/LICENSE).
- Material Design Icons: [Apache 2.0](apps/bezel-studio/src/assets/mdi/LICENSE), with its [notice](apps/bezel-studio/src/assets/mdi/NOTICE).
- LibreHardwareMonitorLib: MPL-2.0; dependency licenses and source information
  are documented in the [helper README](apps/bezel-sensors-helper/README.md).
- Weather: Open-Meteo and GeoNames, with attribution and service terms in the
  weather inspector and [fork documentation](FORK.md).

Protocol knowledge also draws on turing-smart-screen-python and the inherited
research in [docs/reverse-engineering](docs/reverse-engineering/).


## Next development work

The [roadmap](TODO.md) tracks face containers in Layers, timed/software/media card
triggers and player widgets, linear/radial image transparency, radial shape
transparency, per-screen sleep timing (default five minutes), additional colored
and 3D-style weather icons, editor/property organization, calculated variables and
PC-idle screensavers. These items are planned; availability and licensing of
external event sources and icon collections still need evaluation.
