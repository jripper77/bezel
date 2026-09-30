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

### Fixed
- A rev C screen that another app just turned off (turing-smart-screen-python
  and the vendor app send TURNOFF on exit) is woken instead of failing with
  "Broken pipe".
