# Use the screen vertically or horizontally

[Português (Brasil)](pt-BR/vertical-or-horizontal.md)

Most screens can stand up (vertical, taller than wide) or lie down (horizontal,
wider than tall). A theme stores the orientation it was made for.

## In the app

- The **Vertical** and **Horizontal** buttons in the top bar turn the theme. The
  layout follows: a vertical stack of gauges becomes a horizontal row.
- **Rotate 180°**, next to them, is for a screen mounted the other way round
  (the cable comes out of the other side).
- **Themes → New vertical / New horizontal** start an empty theme in that
  orientation. The built-in themes for the 8.8" and the 3.5" come in both.

With **Live** on, the screen follows at once.

## From the command line

Draw the test pattern to see which way is up:

```bash
bezel test-pattern --orientation horizontal --seconds 5
```

The corners are marked red (top left), green (top right), white (bottom right)
and blue (bottom left). If they come out upside down, use the flipped
orientation.

`--orientation` takes `vertical`, `horizontal`, `vertical-flipped` and
`horizontal-flipped` (or `portrait`, `landscape`, `reverse-portrait`,
`reverse-landscape`):

```bash
bezel show wallpaper.png                                # horizontal for a wide picture, vertical otherwise
bezel show poster.jpg --orientation vertical --fit contain
```

`bezel run` uses the orientation stored in the theme: open the theme in the app,
turn it, and save.
