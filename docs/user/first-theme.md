# Your first theme

[Português (Brasil)](pt-BR/first-theme.md)

Open **Bezel** from the application menu (or run `bezel-studio`). The top bar
shows the connected screen, or "No screen connected".

## 1. Start from a theme

In the library on the left, open the **Themes** tab:

- click a built-in theme to open it (there is one for each screen size), or
- **New vertical** / **New horizontal** for an empty theme of your screen's size.

## 2. Edit it

- **Widgets** tab: drag a widget (text, value, clock, image, shape, bar, ring,
  needle, graph) onto the canvas, or click it to add it in the middle.
- **Sensors** tab: drag a sensor onto the canvas to show its value. Use the
  search box to find one.
- Click an element to change it in the **Properties** panel on the right: sensor,
  font, colors, size, and how often it refreshes.
- **Layers** tab: order, hide and lock elements.
- **Media** tab: **Add image…** to use a picture in the theme, **Add video…**
  for a video background ([A video in the background](#a-video-in-the-background)),
  and **Use as background**.
- Undo (Ctrl+Z) and redo (Ctrl+Shift+Z) are in the top bar, next to the zoom.

The canvas shows exactly what the screen will get: it is drawn by the same
renderer.

## 3. Show it on the screen

Turn on the **Live** switch in the top bar. The screen follows your edits as you
make them. Click **Save** to keep the theme in your library.

If the screen stops taking frames while live (a cable glitch, a frozen
screen), Bezel connects it again by itself and goes on; see
[The screen froze](troubleshooting.md#the-screen-froze--stopped-responding).

Closing the window keeps the theme running from the tray icon. The tray menu
has **Open Bezel**, **Live on the screen** and **Quit**. To have it come back at
login, see [Running at login](run-at-login.md).

## 4. Screen settings

The **Screen** tab has two parts:

- **Settings**: **Brightness**, **Hand back to the screen's own mode** (its clock
  or stored media) and the start-at-login option.
- **Storage**: pictures and videos kept on the screen
  ([Screen storage and video](storage-and-video.md)).

## A video in the background

1. In the **Media** tab, click **Add video…** and pick an MP4, MOV, M4V, MKV,
   WebM or AVI file, or an animated GIF. You can also drop the file on the
   Media tab, or straight on the editing area: there it becomes the background
   at once.
2. Bezel copies the video into the theme and takes a still picture from it, the
   *poster*: one second in, or the first picture of a short clip or a GIF,
   cropped to the theme's shape. The editor shows the poster under your
   elements. Taking it needs ffmpeg ([Installing ffmpeg](ffmpeg.md)); without
   it the video is still added, without a poster, and **Properties** says how
   to install ffmpeg. The Media tab shows the video's length and size.
3. Click **Use as background** on the video. **Properties** shows the video and
   its poster, with **Replace video…**, **Use image…** and **Use a solid
   color**. Undo (Ctrl+Z) brings the previous background back.

What the screen does with it:

- Screens that play videos themselves (Turing rev C, such as the 8.8", and the
  Turing USB generation) loop a copy stored on them, and Bezel draws the theme
  over it. With **Live** on, **Properties** says whether the screen already
  stores it; if not, **Open Storage** leads to **Send to the screen**, which
  converts the video for the screen (an animated GIF becomes an MP4) and sends
  it.
- Turing rev C screens take at most **25 MiB per file**. The conversion lowers
  the bitrate from the video's length so that it fits, so a long clip loses
  some quality; if it still does not fit, Bezel says so before sending
  anything ([How large a file can be](storage-and-video.md#how-large-a-file-can-be)).
- Screens that cannot play videos get the video decoded on the computer while
  **Live** is on, which also needs ffmpeg.

Saving the theme keeps the video and its poster inside the `.bezeltheme` file.

## Animated GIFs

An animated GIF added as an **image** element moves at its own pace, also
between the theme's refreshes: up to 30 frames a second on the screen, up to
15 in the editor. Sensors keep the theme's refresh. Only the GIF's rectangle
goes to the screen, so a small GIF (a logo, an icon, a 240x240 animation)
moves smoothly. A GIF as large as the screen changes only as fast as the USB
link carries whole frames (about 2 a second on the 8.8"): Bezel then skips
frames to keep time. For a full-screen or background animation, use the GIF
as a video background ([A video in the background](#a-video-in-the-background)):
the screen plays it itself. A hidden GIF element does not animate.

## Themes from other apps

**Themes → Import…** converts a theme of the vendor app (`.turtheme`) or of
turing-smart-screen-python (its theme folder or `theme.yaml`). Whatever could
not be converted exactly is listed after the import.

## The same from the command line

```bash
bezel run turing-8.8-horizontal                  # a built-in theme, live, until Ctrl+C
bezel run ~/themes/mine.bezeltheme               # a theme file
bezel render turing-8.8-vertical -o preview.png  # one frame to a picture, no screen needed
bezel import AMD.turtheme -o amd.bezeltheme      # convert another app's theme
```

A theme is a `.bezeltheme` file: a zip with `theme.json` and an `assets/`
folder.

**No screen yet?** `BEZEL_FAKE=1 bezel-studio` opens Bezel with a simulated 8.8"
screen and demo sensors.
