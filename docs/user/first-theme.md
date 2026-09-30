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
- **Media** tab: **Add image…** to use a picture in the theme, or **Use as
  background**.
- Undo (Ctrl+Z) and redo (Ctrl+Shift+Z) are in the top bar, next to the zoom.

The canvas shows exactly what the screen will get: it is drawn by the same
renderer.

## 3. Show it on the screen

Turn on the **Live** switch in the top bar. The screen follows your edits as you
make them. Click **Save** to keep the theme in your library.

Closing the window keeps the theme running from the tray icon. The tray menu
has **Open Bezel**, **Live on the screen** and **Quit**. To have it come back at
login, see [Running at login](run-at-login.md).

## 4. Screen settings

The **Screen** tab has two parts:

- **Settings**: **Brightness**, **Hand back to the screen's own mode** (its clock
  or stored media) and the start-at-login option.
- **Storage**: pictures and videos kept on the screen
  ([Screen storage and video](storage-and-video.md)).

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
