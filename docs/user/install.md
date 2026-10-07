# Install Bezel

[Português (Brasil)](pt-BR/install.md)

This Windows development fork currently has no published binary releases.
See the [fork's build instructions](../../README.md#build-and-setup-on-windows) to build
its changes; future packages will appear on
[this fork's releases page](https://github.com/jripper77/bezel/releases).
The package instructions below describe the inherited installer formats and
apply when such packages are available. Upstream packages do not include this
fork's modifications.
Each release has these files (`<version>` is the release number, such as `1.0.0`):

| System | File | Contents |
|---|---|---|
| Debian 12+, Ubuntu 22.04+, Mint 21+ | `bezel_<version>_amd64.deb` | app, `bezel` command, udev rule, service, themes |
| Fedora, openSUSE | `bezel-<version>-1.x86_64.rpm` | app, `bezel` command, udev rule, service, themes |
| Any Linux | `bezel_<version>_amd64.AppImage` | the app only; runs without installing |
| Linux, command only | `bezel-x86_64-unknown-linux-gnu.tar.gz` | `bezel` |
| Windows 10 and 11 | `bezel_<version>_x64-setup.exe` or `bezel_<version>_x64_en-US.msi` | the app |
| Windows, command only | `bezel-x86_64-pc-windows-msvc.zip` | `bezel.exe` |

Before installing, stop any other program that drives the screen (the vendor
app, turing-smart-screen-python): only one program can use it at a time. See
[Coming from turing-smart-screen-python](migrating.md).

## Releases are not signed

Bezel's installers carry no code signature (no Authenticode on Windows, no GPG
on Linux). To check that a download is intact, compare its SHA-256 with the one
the releases page shows next to each file:

```bash
sha256sum bezel_<version>_amd64.deb
```

```powershell
Get-FileHash .\bezel_<version>_x64-setup.exe
```

## Linux

**Debian, Ubuntu, Mint:**

```bash
sudo apt install ./bezel_<version>_amd64.deb
```

**Fedora:**

```bash
sudo dnf install ./bezel-<version>-1.x86_64.rpm
```

**openSUSE:** `sudo zypper install ./bezel-<version>-1.x86_64.rpm`

The package installs:

- *Bezel* in the application menu (`bezel-studio`) and the `bezel` command;
- the udev rule that lets you open the screen without root
  (`/usr/lib/udev/rules.d/60-bezel.rules`), applied during the installation;
- the `bezel-run@` systemd user service ([Running at login](run-at-login.md));
- the bundled themes.

If the screen was already plugged in and Bezel still cannot open it, unplug it
and plug it back in.

To remove Bezel: `sudo apt remove bezel` or `sudo dnf remove bezel`.

**AppImage:**

```bash
chmod +x bezel_<version>_amd64.AppImage
./bezel_<version>_amd64.AppImage
```

The AppImage cannot install the udev rule: do it once as described in
[Let Bezel open the screen](permissions.md). If the AppImage does not start,
run it with `--appimage-extract-and-run`.

**Command only:** unpack the archive and put `bezel` on your `PATH`, for example
in `~/.local/bin`:

```bash
tar -xzf bezel-x86_64-unknown-linux-gnu.tar.gz bezel
install -m 755 bezel ~/.local/bin/bezel
bezel --version
```

## Windows

Run `bezel_<version>_x64-setup.exe` (installs for your user) or the `.msi`
(installs for every user of the computer and asks for administrator rights).
Bezel then appears in the Start menu. If Microsoft Edge WebView2 is missing, the
installer downloads it.

Because the installer is not signed, Windows SmartScreen may stop it with
"Windows protected your PC". Click **More info**, check that the file is the one
you downloaded, then **Run anyway**.

For the command line, unpack `bezel-x86_64-pc-windows-msvc.zip` into a folder of
your choice (for example `C:\Tools\bezel`) and run `bezel.exe` from there, or add
that folder to your `PATH`.

Next: [Let Bezel open the screen](permissions.md). Screens of the Turing USB
generation and WCH panels need the WinUSB driver, and CPU temperatures need
LibreHardwareMonitor.

## From source

See the Windows build instructions in the project [README](../../README.md#build-and-setup-on-windows).
