# Running at login

[Português (Brasil)](pt-BR/run-at-login.md)

Choose **one** of the ways below: only one program can drive the screen at a
time, so the app's start at login and the service must not both be on.

## From the app (Linux and Windows)

In **Screen → Settings**, tick **Start with the computer, in the tray, showing
the last theme live**. At login Bezel starts in the tray, without its window,
and shows the last theme you had live.

The tray icon's menu has **Open Bezel**, **Live on the screen** (starts or
stops the live theme) and **Quit**.

## Multiple screens

The screen selector opens a separate document for each screen. Switching
keeps its edits and Undo history, and other screens with Live enabled keep
running. Save a theme and enable Live on each screen you want to run.
Studio remembers each screen's file and restores the live screens at startup.

On Windows, Light runs all configured live screens in one process with a
shared sensor reader. Closing Studio hands all of them back to Light when
that preference is enabled. The launcher passes `--screens-config` with
Studio's settings file; a plain `bezel run` still runs a single theme.

When Windows shuts down or restarts, Light stops drawing and applies each
connected rev C screen's saved shutdown choice (off, keep, video or album).
Screens without those choices, including the 3.5-inch rev A, receive the
screen-off command. Light waits at most 3.5 seconds; a configured hardware
sleep timer remains the fallback if a device does not respond. Quitting Light
or opening Studio still hands the screen back normally.

## As a service, without the app (Linux)

`bezel-run@<theme>` is a systemd *user* service: it runs `bezel run <theme>` while
you are logged in, and starts it again if the screen shows up late.

- deb and rpm packages install it (`/usr/lib/systemd/user/bezel-run@.service`).
- `scripts/install-local.sh` (a build from source) installs it in
  `~/.config/systemd/user`, pointing at the `bezel` it installed.

Turn it on with a built-in theme's name (`turing-8.8-horizontal`,
`turing-8.8-vertical`, `turing-5-horizontal`, `turing-3.5-vertical`,
`turing-3.5-horizontal`, `turing-2.1-round`):

```bash
systemctl --user daemon-reload
systemctl --user enable --now bezel-run@turing-8.8-horizontal
systemctl --user status bezel-run@turing-8.8-horizontal
journalctl --user -u bezel-run@turing-8.8-horizontal -f    # its log
```

Turn it off:

```bash
systemctl --user disable --now bezel-run@turing-8.8-horizontal
```

**Your own theme file:** give the instance any name and override the command
once:

```bash
systemctl --user edit bezel-run@mine
```

In the editor, add (use the path of your theme):

```ini
[Service]
ExecStart=
ExecStart=/usr/bin/bezel run %h/themes/mine.bezeltheme
```

Then `systemctl --user enable --now bezel-run@mine`.

**AppImage or the command-line archive:** these do not install the service.
Create it once with `systemctl --user edit --force --full bezel-run@.service`,
paste the following, and change the `ExecStart` path to where your `bezel` is:

```ini
[Unit]
Description=Bezel theme %i on the smart screen
After=graphical-session.target

[Service]
ExecStart=%h/.local/bin/bezel run %i
Restart=on-failure
RestartSec=5
KillSignal=SIGTERM

[Install]
WantedBy=default.target
```

When the service stops, Bezel hands the screen back to its own mode.

## As a logon task, without the app (Windows)

With `bezel.exe` unpacked in `C:\Tools\bezel`:

```powershell
schtasks /Create /SC ONLOGON /TN "Bezel" /TR "C:\Tools\bezel\bezel.exe run turing-8.8-horizontal"
```

A console window stays open while it runs (closing it stops the theme). To
remove the task: `schtasks /Delete /TN "Bezel" /F`. The app's tray start has no
console window.
