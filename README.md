# Bezel

**The open studio for USB smart screens.** Bezel drives the little USB system-monitor
screens sold as Turing Smart Screen, TURZX, XuanFang, Kipye, WeAct and their OEM
rebrands — on Linux and Windows, from one app.

> Status: early development. Screen discovery works; drawing, sensors, the theme
> editor and SD-card/video support land phase by phase (see the roadmap below).

## Quick start

```bash
bezel devices          # list connected screens (read-only)
bezel devices --json   # the same, as JSON
```

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
