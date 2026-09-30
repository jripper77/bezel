# Bezel user guide

[Português (Brasil)](pt-BR/README.md)

Bezel drives the USB system-monitor screens sold as Turing Smart Screen, TURZX,
XuanFang, Kipye, WeAct and their rebrands, on Linux and Windows. It has two
parts:

- **Bezel** (`bezel-studio`), the app: design themes by dragging widgets and
  sensors, show them live on the screen, and manage the screen's pictures and
  videos.
- **`bezel`**, the command line: the same things from a terminal or a service.

## Getting started

1. [Install Bezel](install.md) on Linux or Windows.
2. [Let Bezel open the screen](permissions.md): the Linux udev rule, the
   Windows drivers.
3. [Make your first theme](first-theme.md) and turn on *Live*.
4. [Use the screen vertically or horizontally](vertical-or-horizontal.md).

## Using Bezel

- [Sensors](sensors.md): what Bezel measures, and why a value can show `—`.
- [Game FPS](fps.md): RivaTuner Statistics Server on Windows, MangoHud on Linux.
- [Screen storage and video](storage-and-video.md): pictures and videos kept on
  the screen, what it shows at power-up.
- [Installing ffmpeg](ffmpeg.md), needed to convert videos.
- [Preparing an SD card](sd-card.md) for screens with a card slot.
- [Running at login](run-at-login.md): from the tray, or as a systemd service.
- [Coming from turing-smart-screen-python](migrating.md).

## When something goes wrong

- [Troubleshooting](troubleshooting.md)
- [Supported screens](devices.md)
