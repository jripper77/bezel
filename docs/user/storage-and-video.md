# Screen storage and video

[Português (Brasil)](pt-BR/storage-and-video.md)

Screens with storage (Turing rev C, and the Turing USB generation) keep pictures
and videos in their internal memory and, when they have a slot, on an SD card.
They play them on their own and can show one at power-up, without Bezel running.

The screen has four folders: `internal/image`, `internal/video`, `sd/image` and
`sd/video`. `sd` is the memory card, reached only through the screen. Bezel
never formats it; see [Preparing an SD card](sd-card.md).

## In the app

**Screen → Storage** shows how full the internal memory and the SD card are, and
their files.

- **Send**: drag a file onto the list, or **Send a file…**. Bezel shows what it
  will do (conversion, destination, a file it replaces) and asks you to confirm.
  The progress bar goes through *Converting*, *Sending* and *Checking*.
- **Play** and **Stop playback**: the screen plays a stored video in a loop, or
  shows a stored picture. With **Live** on, the theme covers what the screen
  plays, so these wait until you turn Live off.
- **Delete**: asks for confirmation, naming the file.
- **At start**: **Show at start** picks the file the screen shows when it powers
  up; **Back to the default clock** undoes it. The screen remembers the choice.

## From the command line

```bash
bezel storage info                          # used and free space
bezel storage ls                            # every stored file; or one folder: bezel storage ls sd/video
bezel storage put clip.mp4                  # convert when needed and send, with progress
bezel storage put logo.png sd/image/logo.png
bezel storage play internal/video/clip.mp4  # loop it on the screen (--once: play it once)
bezel storage stop
bezel storage rm internal/video/clip.mp4 --yes
bezel storage boot internal/video/clip.mp4 --brightness 60 --yes   # shown at power-up
bezel storage boot default --yes            # back to the built-in start screen
```

Whatever deletes, replaces or changes the power-up choice (`rm`, `put` over an
existing file, `boot`) first prints what it will do and needs `--yes`; without
it nothing reaches the screen.

## What can be sent

- Pictures: JPEG, PNG, BMP, GIF, sent as they are.
- Videos: converted with ffmpeg to the screen's format (on the 8.8": 480×1920
  H.264 MP4 without sound), turned for the orientation you choose
  (`--orientation`, default: the video's own shape) and cropped to the screen's
  shape, never stretched. `--fps 24` lowers the frame rate. A video already in
  the right format goes as it is. ffmpeg is not included:
  [Installing ffmpeg](ffmpeg.md).
- File names: lower-case letters `a-z`, digits, `_`, `.` and `-`. Up to 120 MB
  per file.
- When a file does not fit, Bezel says how much is free and lists the stored
  files, largest first. It never deletes anything for you.

## Cancelling an upload

You can cancel an upload (**Cancel upload** in the app, Ctrl+C in the terminal;
a second Ctrl+C quits at once). Part of the file may stay on the screen:

1. **Delete the incomplete file.** The app offers **Delete the incomplete file**;
   the terminal prints the command, for example
   `bezel storage rm internal/video/clip.mp4 --yes`. If the app says the screen
   stopped answering, it comes back on the next action: press **Refresh**, then
   delete the incomplete file if it is listed.
2. **Send it again.** If the next upload ends with *"the stored size differs;
   delete it and send it again"*, bytes from the cancelled upload reached it:
   delete that file and send it once more.

If the screen stops answering altogether (the upload stalls, or every command
times out), unplug its USB cable, wait a few seconds and plug it back in.

## What the screen shows at power-up

On rev C screens the power-up choice also stores the brightness the screen
starts with: in the app, the brightness set under **Settings**; on the command
line, `--brightness`; otherwise the vendor's default, about 67%. On the
Turing USB generation, Bezel can send and play files but cannot yet delete them,
play a video once, or change the power-up choice.

## Themes with a video background

A theme can use a video as its background: the screen loops the video and Bezel
draws the theme over it.

- If the video is not on the screen yet, the screen shows the theme's still
  picture and the app offers **Send to the screen**; `bezel run` prints the exact
  `bezel storage put` command.
- Screens that cannot play videos get them decoded on the computer, which needs
  ffmpeg (`bezel run --ffmpeg PATH` if it is not on the `PATH`).
