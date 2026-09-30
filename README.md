# Bezel

**The open studio for USB smart screens.** Bezel drives the little USB system-monitor
screens sold as Turing Smart Screen, TURZX, XuanFang, Kipye, WeAct and their OEM
rebrands — on Linux and Windows, from one app.

> Status: early development. Every protocol family is implemented and the Turing
> 8.8" is validated on real hardware; sensors are measured on Linux and Windows;
> the renderer and the theme editor are landing, then SD-card/video support.

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
