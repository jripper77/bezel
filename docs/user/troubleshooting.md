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

- **While live**: `bezel run`, the `bezel-run@` service and **Live** in the app
  connect the screen again by themselves, 2, 5 and 10 seconds after it
  stopped; connecting restarts a frozen screen first, then the theme goes on.
  The app's status bar says *Reconnecting to the screen (attempt 1 of 3)…*,
  then *The screen is back and shows the theme live again.*; `bezel run`
  writes the same in the terminal. After the third attempt live mode stops
  with the error. Turning **Live** off (or Ctrl+C) meanwhile stops at once.
- **On its own**: the next command, or turning **Live** on again, finds the
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

With the bundled Windows reader, Studio's status bar shows **Libre reading**
with a green dot. Yellow means at least one hardware device has no readings;
red means no Libre readings are available. Hover over the status for the
affected device. Optional missing sensors do not turn a healthy device yellow.
**Restart Libre** restarts the configured Bezel sensor reader; the status
updates when readings return. This can recover a Corsair PSU whose USB
readings stopped, even when its computed Total Output still reports `0 W`.
The restart requires the `Bezel-Sensors` task created by `configure-sensors.cmd`.

## Game FPS shows `—`

No tool is measuring a game, or the game is paused: [Game FPS](fps.md).

## Windows: "Windows protected your PC"

Bezel's installers are not signed. Click **More info**, then **Run anyway**:
[Install Bezel](install.md#releases-are-not-signed).

## A Turing USB panel shows as "desktop mode"

It is in the vendor's desktop (second monitor) mode:
[Supported screens](devices.md#desktop-mode).

## The app does not open

Start it from a terminal (`bezel-studio`) and read what it prints. On Windows
the app opens no console: in a Command Prompt, in the folder Bezel is installed
in, run `bezel-studio.exe 2> bezel-studio.txt`, then open `bezel-studio.txt`.

The app's messages in the terminal (`bezel-studio: …`) are fixed sentences from
a closed list: none of them holds the text of the error behind it, nor a file
or folder of yours. This is by design, so that nothing the app prints can carry
a secret, such as your KLIPY key, or a personal path. When the app stops on an
error (it panics), the one line it prints adds where in the source code that
happened: a file of Bezel's or of a library's code, from its crate's folder
on (`tao-0.37.1/src/…`), and a line and a column, never the error's message.
When the app cannot start, or starts without a part, the terminal says which
part failed:

| The terminal says | What failed |
| --- | --- |
| `bezel-studio: the app panicked at tao-…/src/platform_impl/linux/event_loop.rs:…` | Linux: there is no graphical session, so GTK could not start. Start the app inside your desktop session, not over SSH or from a system service. |
| `bezel-studio: the app's folders were not found`, then `bezel-studio: the app panicked at tauri-…/src/app.rs:…` | Where the app keeps its settings, data and cache (on Linux, found from `HOME` and the `XDG_*_HOME` variables). |
| `bezel-studio: the tray icon was not added`, then `bezel-studio: the app panicked at tauri-…/src/app.rs:…` | The tray icon. Linux: the AppIndicator library (`libayatana-appindicator`). |
| `bezel-studio: the app panicked at tauri-…/src/app.rs:…`, with no line before it | The window or the page inside it. Linux: WebKitGTK; with the AppImage, try `--appimage-extract-and-run`. Windows: the WebView2 runtime; run the installer again. |
| `bezel-studio: the app panicked at …`, another place | An error inside the app or a library it uses. Report it, with the whole line. |
| `bezel-studio: the app did not start: a plugin did not start` | One of its plugins: a single window, file dialogs, opening links, starting at login. |
| `bezel-studio: the app did not start` | Another cause. |
| `bezel-studio: refresh loop not started: no thread or memory to spare` | The app opens, but sensors and the live screen do not update: the system is out of threads or memory. Close some programs and open Bezel again. |
| `bezel-studio: refresh loop not started` | The same, for another cause. |
| `bezel-studio: could not restart with the DMA-BUF renderer off: the app's program file is gone` | Linux. At start the app restarts itself once with WebKitGTK's DMA-BUF renderer off (with it on, NVIDIA's driver closes the app); its program file was removed or replaced meanwhile. Open it again. |
| `bezel-studio: could not restart with the DMA-BUF renderer off: running the app's program file was not allowed` | The same restart, refused: the program is on a disk mounted `noexec`, or a security policy stops it. |
| `bezel-studio: could not restart with the DMA-BUF renderer off` | The same restart, for another cause. In these three cases, start it with `WEBKIT_DISABLE_DMABUF_RENDERER=1 bezel-studio`. |

In a place, `…` stands for the library's version, the line and the column,
which change from one Bezel release to the next.

When you report the problem, copy these lines as they are.

## Reporting a problem

Include the output of `bezel --version` and `bezel -v devices`, and the log of
what failed (`bezel -v …`, or `journalctl --user -u bezel-run@<theme>` for the
service). Remove serial numbers and personal paths before posting.
