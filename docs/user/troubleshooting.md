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

## The screen stops answering

Symptoms: "timeout talking to …", "the screen did not wake up", an upload that
stalls. Unplug the screen's USB cable, wait a few seconds, and plug it back in.
Then try again.

## After cancelling an upload

Delete the incomplete file before sending again: the app offers **Delete the
incomplete file**, and the terminal prints the `bezel storage rm … --yes`
command. If the next upload ends with *"the stored size differs; delete it and
send it again"*, delete that file and send it once more. See
[Cancelling an upload](storage-and-video.md#cancelling-an-upload).

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
