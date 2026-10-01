# Troubleshooting

[Português (Brasil)](pt-BR/troubleshooting.md)

Start with `bezel devices`: it lists the connected screens and their state
without sending them anything. Add `-v` to any command to see what Bezel sends
and receives.

## "No smart screen found"

- Check the USB cable (some cables only charge) and try another port.
- Linux: your user may not be allowed to open it yet: see the next item.
- Windows: Turing USB (`1CBE`) and WCH (`43A8`) screens need the WinUSB driver:
  [Let Bezel open the screen](permissions.md#windows).

## "access denied" / "Permission denied" (Linux)

The udev rule is missing. Run `bezel udev-rules`, copy the command it prints,
run it, then unplug and replug the screen. The app shows the same command when
the port is denied. Details: [Let Bezel open the screen](permissions.md#linux).

## "… is in use by …"

Another program holds the screen, for example:

```text
/dev/ttyACM1 is in use by python3 (PID 4242)
```

Stop that program: turing-smart-screen-python, the vendor app, a
`bezel-run@` service, a second `bezel run`, or Bezel's own window or tray with
**Live** on. See [Coming from turing-smart-screen-python](migrating.md#1-stop-it).

## The screen is dark, or listed as "asleep"

Rev C screens sleep when another program turns them off (turing-smart-screen-python
and the vendor app do it when they quit) and after `bezel off`. Anything that
draws wakes them: **Live** in the app, `bezel run`, `bezel show`,
`bezel test-pattern`. Waking takes a few seconds.

## The screen froze / stopped responding

Symptoms: *"the screen stopped responding: it stopped reading what was sent"*,
"timeout talking to …", an upload or the live theme that stalls. A Turing rev C
screen (the 8.8" and the other serial models with a wake chip) can freeze, for
example on an upload over its 25 MiB limit. **No replug is needed**: Bezel
restarts it through its wake chip, and it comes back in about 10 seconds.

- **On its own**: the next command, `bezel run` started again (the
  `bezel-run@` service restarts it), or turning **Live** on again finds the
  frozen screen and restarts it once before connecting. Nothing is restarted
  while the screen answers.
- **Command line**: `bezel restart` (or `bezel restart -s /dev/ttyACM1` for one
  screen). It says that it restarts the screen, waits for it and prints where
  it is back. The terminal also prints this hint after an error that means the
  screen froze.
- **App**: **Screen → Settings → Restart screen…**, after a short confirmation.
  A storage error that means the screen froze has the same button, and so has
  the screen's card when live mode stopped because of it.

A restart stops what the screen shows or plays; the files it stores stay. If
the screen still does not answer after a restart, or it has no wake chip
(Bezel then says it cannot restart it), unplug its USB cable, wait a few
seconds and plug it back in.

## After cancelling an upload

Delete the incomplete file before sending again: the app offers **Delete the
incomplete file**, and the terminal prints the `bezel storage rm … --yes`
command. If the next upload ends with *"the stored size differs; delete it and
send it again"*, delete that file and send it once more. See
[Cancelling an upload](storage-and-video.md#cancelling-an-upload).

## "refused: the file is … MiB and this screen takes files up to 25 MiB each"

Turing rev C screens take at most 25 MiB per file: their firmware keeps the
whole upload in memory and freezes on a larger one. Bezel refuses the file
before sending anything. Send a shorter clip, or let Bezel convert the video at
a lower frame rate (`--fps 24`): a conversion caps the bitrate so the result
fits. See [How large a file can be](storage-and-video.md#how-large-a-file-can-be).

## A video will not send

"needs ffmpeg" or "ffmpeg not found": install ffmpeg with `libx264`
([Installing ffmpeg](ffmpeg.md)). Pictures, and videos already in the screen's
format, go without it.

## "No SD card in the screen"

The card is missing or not FAT32 on an MBR partition table:
[Preparing an SD card](sd-card.md).

## A sensor shows `—`

`bezel sensors` prints why. On Windows, run LibreHardwareMonitor as
administrator. See [Sensors](sensors.md#when-a-value-is-unavailable).

## Game FPS shows `—`

No tool is measuring a game, or the game is paused: [Game FPS](fps.md).

## Windows: "Windows protected your PC"

Bezel's installers are not signed. Click **More info**, then **Run anyway**:
[Install Bezel](install.md#releases-are-not-signed).

## A Turing USB panel shows as "desktop mode"

It is in the vendor's desktop (second monitor) mode:
[Supported screens](devices.md#desktop-mode).

## Reporting a problem

Include the output of `bezel --version` and `bezel -v devices`, and the log of
what failed (`bezel -v …`, or `journalctl --user -u bezel-run@<theme>` for the
service). Remove serial numbers and personal paths before posting.
