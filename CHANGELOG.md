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

### Fixed
- A rev C screen that another app just turned off (turing-smart-screen-python
  and the vendor app send TURNOFF on exit) is woken instead of failing with
  "Broken pipe".
