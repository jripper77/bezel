# Sensors

[Português (Brasil)](pt-BR/sensors.md)

Bezel reads the computer's sensors itself: processor, graphics card, memory,
disks, network, motherboard and fans. In the app they are in the **Sensors** tab,
grouped the same way; drag one onto the canvas to show it.

From a terminal:

```bash
bezel sensors                    # every sensor, with its key
bezel sensors --watch 1          # refresh every second, until Ctrl+C
bezel sensors --json             # for scripts
```

## When a value is unavailable

Bezel never makes up a number. A sensor it cannot read is *unavailable*: the
theme shows `—`, and `bezel sensors` prints the reason next to it, for example:

```text
  CPU package power           —  cpu.power  (the RAPL energy counter is readable by root only ...)
  Game frame rate             —  gpu.fps  (no MangoHud log in ...: run the game with MangoHud and start logging ...)
```

Common reasons:

| Sensor | Why it is unavailable | What to do |
|---|---|---|
| CPU temperature, fans, power (Windows) | LibreHardwareMonitor is not running | see [Windows](#windows) below |
| CPU power (Linux) | the kernel lets only root read the RAPL counter | leave it off the theme, or grant read access to the file the reason names |
| Fans, voltages (Linux) | the motherboard's sensor chip has no driver loaded (`nct6775`, `it87`) | load the driver for your board (`sudo sensors-detect` from lm-sensors helps) |
| Game FPS | no overlay tool is measuring a game | see [Game FPS](fps.md) |
| Ping | no answer from the ping target yet | check the network or change the target (below) |
| Output volume | not supported yet | — |

A sensor that is simply not there (no second GPU, no swap) is unavailable too.

## Linux

Bezel reads the kernel directly: `/proc` for CPU, memory and network, `hwmon`
for temperatures and fans, NVIDIA's driver (NVML) for NVIDIA cards, `amdgpu` for
AMD cards. Nothing to install.

## Windows

CPU usage, memory, disks and network work out of the box, and NVIDIA cards
through their driver. Temperatures, fans, voltages and power need
[LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor):

1. Download it and run `LibreHardwareMonitor.exe` **as administrator** (it
   needs that to read the hardware).
2. Keep it running. Its **Options → Run On Windows Startup** starts it with
   Windows.

Bezel reads what LibreHardwareMonitor publishes (its WMI provider); it does not
install drivers of its own.

## Ping

`net.ping` measures the round trip to `8.8.8.8` by default. Change the target in
**Preferences → Sensors** in the app, or with `--ping-host` on the command line
(`bezel run --ping-host 1.1.1.1 ...`).
