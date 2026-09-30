# Installing ffmpeg

[Português (Brasil)](pt-BR/ffmpeg.md)

Bezel uses [ffmpeg](https://ffmpeg.org) to convert videos to the screen's
format, and to play a theme's video background on screens that cannot play
videos themselves. ffmpeg is not included with Bezel. It must have the H.264
encoder `libx264`.

Without ffmpeg, pictures and videos already in the screen's format are still
sent; Bezel tells you when a video needs converting.

## Linux

**Fedora.** Fedora's own `ffmpeg-free` has no `libx264`. Enable RPM Fusion, then
install the full ffmpeg (it replaces `ffmpeg-free`):

```bash
sudo dnf install https://mirrors.rpmfusion.org/free/fedora/rpmfusion-free-release-$(rpm -E %fedora).noarch.rpm
sudo dnf install ffmpeg --allowerasing
```

**Debian, Ubuntu, Mint:**

```bash
sudo apt install ffmpeg
```

**Arch Linux:** `sudo pacman -S ffmpeg`

## Windows

```powershell
winget install --id Gyan.FFmpeg -e
```

Then close and reopen Bezel (and any terminal), so they see the new `PATH`.

## Check it

```bash
ffmpeg -hide_banner -encoders | grep libx264
```

On Windows: `ffmpeg -hide_banner -encoders | Select-String libx264`. A line with
`libx264` means it is ready.

## An ffmpeg that is not on the PATH

- In the app: **Screen → Storage → Locate ffmpeg…** (it appears when Bezel
  cannot find one).
- On the command line: `--ffmpeg PATH`, the program or its folder:

```bash
bezel storage put clip.mp4 --ffmpeg /opt/ffmpeg/bin/ffmpeg
```
