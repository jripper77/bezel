# Game FPS

[Português (Brasil)](pt-BR/fps.md)

The `gpu.fps` sensor (*Game frame rate*) shows the frame rate of the game you are
playing. Bezel does not hook into games: it reads what a frame-rate tool you
already run publishes. Drag the sensor onto a theme like any other.

> The FPS readers follow the published formats of these tools, but have not yet
> been checked against a running game (not validated on hardware). If the value
> looks wrong, please report it.

## Windows: RivaTuner Statistics Server

1. Install RivaTuner Statistics Server (RTSS). It comes with MSI Afterburner, or
   on its own.
2. Start RTSS and leave it running (it can sit in the tray).
3. Start the game. Bezel shows the frame rate of the game RTSS measured last.

While RTSS is not running the sensor is unavailable and says so ("RivaTuner
Statistics Server is not running: start it ...").

## Linux: MangoHud logs

Bezel reads the newest CSV log that MangoHud writes while it logs a game.

1. Run the game with MangoHud: `mangohud ./game`, or in Steam set the launch
   options to `mangohud %command%`.
2. Start logging in the game with **Shift_L+F2** (MangoHud's default key), or
   make it start by itself: add `autostart_log=1` to
   `~/.config/MangoHud/MangoHud.conf`.

Bezel looks for the logs in MangoHud's `output_folder` (set in `MangoHud.conf`),
or in your home folder when it is not set. If you keep them somewhere else,
point Bezel at that folder: **Preferences → Sensors → MangoHud log folder** in
the app, or `--mangohud-dir` on the command line:

```bash
bezel run turing-8.8-horizontal --mangohud-dir ~/mangohud-logs
```

MangoHud writes a new file for each logging session; delete old ones from time
to time.

## What you see

- A number while the game runs. A 0 reported by the tool is shown as 0.
- `—` when no tool is measuring a game, or when the last value is older than
  3 seconds (game paused, minimized or closed). Bezel never keeps showing an old
  value.
