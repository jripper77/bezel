# Coming from turing-smart-screen-python

[Português (Brasil)](pt-BR/migrating.md)

Bezel drives the same screens as
[turing-smart-screen-python](https://github.com/mathoudebine/turing-smart-screen-python)
and reads its themes. The two cannot run at the same time.

## 1. Stop it

Two programs on one screen mix their data, so Bezel refuses a port that another
program holds and names it, for example `/dev/ttyACM0 is in use by python3
(PID 4242)`.

- **Started in a terminal:** press Ctrl+C there.
- **Started as a systemd service:** find the service and turn it off, so it does
  not come back at the next boot:

  ```bash
  systemctl list-units --all | grep -i -E 'turing|smart'
  systemctl --user disable --now <its name>.service          # a user service
  sudo systemctl disable --now <its name>.service            # a system service
  ```

- **Windows:** quit it from its tray icon, and remove it from **Task Manager →
  Startup apps** (or from the Task Scheduler, if you added it there).

The vendor app (TURZX / Turing) also holds the screen: quit it from its tray
icon before using Bezel.

## 2. Bring your themes

In the app: **Themes → Import…**, then choose the theme's folder (for example
`res/themes/MyTheme` inside turing-smart-screen-python) or its `theme.yaml`.
The imported theme opens in the editor; click **Save** to keep it in your
library. What could not be converted exactly (a sensor Bezel does not have, a
font it cannot find) is listed after the import.

From a terminal:

```bash
bezel import turing-smart-screen-python/res/themes/MyTheme -o mytheme.bezeltheme
bezel run mytheme.bezeltheme
```

`bezel run` and `bezel render` also take the theme folder directly and convert
it on the fly. Themes of the vendor app (`.turtheme`) import the same way.

## 3. Your settings

| `config.yaml` | In Bezel |
|---|---|
| `COM_PORT` | found automatically; `--screen` picks one when several are connected |
| `REVISION` | detected from the screen |
| `DISPLAY_REVERSE` | **Rotate 180°** in the app; the `-flipped` orientations on the command line |
| `BRIGHTNESS` | **Screen → Settings → Brightness**, or `bezel brightness 40` |
| `THEME` | the theme you run: [Running at login](run-at-login.md) |
| `HW_SENSORS` | Bezel reads the sensors itself; on Windows it uses LibreHardwareMonitor too ([Sensors](sensors.md)) |
| `PING` | **Preferences → Sensors**, or `--ping-host` |

On Linux, if you used to run it with `sudo` or had added your user to the
`dialout` group, Bezel needs neither: see [Let Bezel open the screen](permissions.md).
