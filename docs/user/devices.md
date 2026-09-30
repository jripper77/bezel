# Supported screens

[Português (Brasil)](pt-BR/devices.md)

`bezel devices` lists the screens connected to this computer and the model each
one matches, without sending them anything.

| Family | Examples | Connection |
|---|---|---|
| Turing rev A | Turing Smart Screen 3.5", UsbPCMonitor 3.5" and 5" | serial |
| XuanFang rev B | XuanFang 3.5" (and Flagship) | serial |
| Turing rev C | Turing Smart Screen 2.1" to 8.8" | serial |
| Kipye rev D | Kipye Qiye 3.5" | serial |
| WeAct | WeAct Studio Display FS 3.5" and 0.96" | serial |
| Turing USB | TURZX / Turing USB generation, 1.6" to 12.3" (USB id `1CBE`) | USB |
| WCH | WCH-based 2.4" to 4.3" panels (USB id `43A8`) | USB |

The Turing 8.8" is tested on real hardware; the others follow the published
protocols of the vendor app and of turing-smart-screen-python. The full list of
models and USB ids is in [devices.md](../reverse-engineering/devices.md).

Storage (pictures and videos on the screen) exists on Turing rev C and the
Turing USB generation: [Screen storage and video](storage-and-video.md).

## Desktop mode

Some Turing USB panels (USB ids `1A86:AD10` to `1A86:AD13`) have a *desktop mode*,
set by the vendor app, in which Windows uses them as a second monitor. In that
mode they are not a smart screen, and Bezel cannot draw on them.

Bezel lists them as **desktop mode (not validated on hardware)** and can switch
them back to USB monitor mode. This switch follows the vendor protocol but has
not been tried on a real panel yet.

```bash
bezel monitor-mode          # says what it would do; sends nothing
bezel monitor-mode --yes    # asks the panel its model, then switches it
```

The panel restarts as a USB screen, and `bezel devices` then lists it normally.
In the app, the switch is behind a confirmation dialog. On Linux, the panel is
reached through `hidraw`, which the udev rule covers
([Let Bezel open the screen](permissions.md)).
