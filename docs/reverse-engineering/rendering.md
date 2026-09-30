# Rendering semantics

What Bezel's renderer reproduces so that imported themes look the way their authors saw them. Section 1 covers the
Pillow widgets of turing-smart-screen-python (`library/lcd/lcd_comm.py` at `2b33ab4`); section 3 the GDI+ elements of
the vendor app. Confidence: **static**, except the reference renders in section 1.9 (**verified**).

## 1. Python reference (Pillow)

All high-level drawing lives in `LcdComm` and renders with Pillow into an image that the protocol class then sends with
`DisplayPILImage(image, x, y)`. Reference environment: Pillow 12.3.0, FreeType 2.14.3, RAQM available.

### 1.1 Caches (`lcd_comm.py:747-756`, `89-92`)

- `open_image(path)`: `Image.open` once per path, returns a copy. Images keep their mode (most theme PNGs are RGBA,
  some RGB, a few palette `P`).
- `open_font(path, size)`: `ImageFont.truetype(path, size)` once per (path, size); `size` is a FreeType **pixel** size.
- `text_bbox_cache[(x, y)] = (l, t, r, b)`: never cleared, not even on an orientation change.

### 1.2 `DisplayBitmap(path, x=0, y=0, width=0, height=0)` (`lcd_comm.py:251-259`)

Open (cached); if width and height are both non-zero and differ from the image size, `image.resize((w, h))` (Pillow
default filter: BICUBIC; NEAREST for `1`/`P` images); then `DisplayPILImage(image, x, y, w, h)`.

### 1.3 `DisplayText` (`lcd_comm.py:261-355`)

Signature: `text, x=0, y=0, width=0, height=0, font="./res/fonts/roboto-mono/RobotoMono-Regular.ttf",
font_size=20, font_color=(0,0,0), background_color=(255,255,255), background_image=None, align='left', anchor='la'`.
Theme defaults differ ([themes-python-yaml.md](themes-python-yaml.md) section 7.1).

1. Parse colours. Assert `x <= W`, `y <= H`, `len(text) > 0`, `font_size > 0`.
2. If `width > 0 and height == 0`: `height = font_size`.
3. Canvas: a full-screen `Image.new('RGB', (W, H), background_color)`, or the **whole** `background_image` (assumed
   full-screen, aligned at (0,0) in the current orientation). This is the "transparent text" trick: text is drawn on
   the background picture at absolute screen coordinates and the result is cropped, so it looks transparent without
   reading the screen back.
4. Auto box (`width == 0 or height == 0`): `bbox = draw.textbbox((x, y), text, font, align, anchor)`; floor left/top,
   ceil right/bottom. **Anti-ghosting** (commit `d2dbe88`, PR #1070): union with the bbox previously drawn with the
   same `(x, y)` key, then store the new **un-unioned** bbox. A shorter new value therefore still repaints the area of
   the previous value with the background.
5. Fixed box (`width > 0 and height > 0`): box = `(x, y, x + width, y + height)`; the anchor point moves inside the box:
   horizontal `anchor[0]`: `m` -> `int((l + r) / 2)`, `r` -> `r`, otherwise `l`; vertical `anchor[-1]`: `m` ->
   `int((t + b) / 2)`, `b` -> `b`, otherwise `t` (so `a`, `t`, `s`, `d` all use the top edge). No bbox cache here.
6. `draw.text((x, y), text, font, fill=font_color, align, anchor)`.
7. Clamp the box to the screen (`max(., 0)`, `min(., W or H)`), crop the canvas to it, `DisplayPILImage(crop, left,
   top)`. Text outside a fixed box is clipped.

Multi-line text (`\n`) supports `align` left/center/right/justify, but Pillow raises "anchor not supported for
multiline text" when the vertical anchor is `t` or `b`. Theme text defaults to `lt`, so multi-line theme text only
works with an explicit `a`/`m`/`s`/`d` anchor. Line spacing = `font.getbbox("A")[3] + 4` (Pillow `spacing=4`).

### 1.4 `DisplayProgressBar` (`lcd_comm.py:357-430`)

Signature: `x, y, width, height, min_value=0, max_value=100, value=50, bar_color=(0,0,0), bar_outline=True,
background_color=(255,255,255), background_image=None, reverse_direction=False`.

1. Assert the rectangle is on screen. Clamp `value` to `[min, max]`.
2. Canvas: `Image.new('RGB', (width, height), background_color)` or `background_image.crop((x, y, x+w, y+h))`.
3. Horizontal if `width > height`, else vertical (squares are vertical).
4. `filled = (value - min) / (max - min) * length - 1`, clamped to `>= 0` (float). (The min offset was fixed in
   `17f13b4`, issue #954.)
5. Rectangle `[x1, y1, x2, y2]` (inclusive; floats truncated toward zero by Pillow), starting from `0, 0, w-1, h-1`:
   horizontal: normal `x2 = filled`, reverse `x1 = w - 1 - filled`; vertical: normal `y1 = h - 1 - filled` (fills
   from the bottom), reverse `y2 = filled` (from the top). Drawn with `fill = outline = bar_color`. At `value == min`
   a 1-px line is still drawn.
6. If `bar_outline`: outline `[0, 0, w-1, h-1]` in `bar_color`.
7. `DisplayPILImage(canvas, x, y)`.

### 1.5 `DisplayLineGraph` (`lcd_comm.py:432-527`)

Signature: `x, y, width, height, values, min_value=0, max_value=100, autoscale=False, line_color=(0,0,0),
line_width=2, graph_axis=True, axis_color=(0,0,0), axis_font="./res/fonts/roboto/Roboto-Black.ttf",
axis_font_size=10, background_color=(255,255,255), background_image=None, axis_minmax_format="{:0.0f}"`.

1. Canvas as for progress bars.
2. Autoscale: `trueMin`/`trueMax` over non-NaN values (initialised to `max_value`/`min_value`); if
   `trueMin != max_value and trueMax != min_value`: `min_value = max(trueMin - 5, min_value)`,
   `max_value = min(trueMax + 5, max_value)`.
3. `step = width / len(values)` (the length includes NaNs); `yScale = height / (max - min)`, or 0 when equal.
4. For each **non-NaN** value (clamped): point `(count * step, height - (v - min) * yScale)`; `count` advances only for
   non-NaN values, so NaNs are skipped entirely and the curve starts at x = 0 with the first real sample (histories
   start full of NaN). A value equal to `min` maps to `y = height` (one pixel below the canvas).
5. `draw.line(points, fill=line_color, width=line_width)` (no joints; floats truncated; a single point draws
   nothing).
6. If `graph_axis`: line `[0, h-1, w-1, h-1]`, line `[0, 0, 0, h-1]`, tick `[0, 0, 1, 0]`. Max label
   `axis_minmax_format.format(max_value)` at `(2, -top)` where `(_, top, right, bottom) = font.getbbox(label)`; min
   label at `(w - 1 - right, h - 2 - bottom)` (anchor `la`). Python format-spec semantics apply.
7. `DisplayPILImage(canvas, x, y)`.

### 1.6 `DisplayRadialProgressBar` (`lcd_comm.py:552-744`)

Signature: `xc, yc, radius, bar_width, min_value=0, max_value=100, angle_start=0, angle_end=360, angle_sep=5,
angle_steps=10, clockwise=True, value=50, text=None, with_text=True, font="./res/fonts/roboto/Roboto-Black.ttf",
font_size=20, font_color=(0,0,0), bar_color=(0,0,0), background_color=(255,255,255), background_image=None,
custom_bbox=(0,0,0,0), text_offset=(0,0), bar_background_color=(0,0,0), draw_bar_background=False,
bar_decoration=""`.

Angles follow Pillow: degrees, 0 = 3 o'clock, positive = clockwise on screen; `arc` draws clockwise from start to
end, bbox `[0, 0, d-1, d-1]` with `d = 2 * radius`, `width = bar_width` (thickness grows inward).

1. If `angle_start % 361 == angle_end % 361`: add 0.1 to the start (clockwise) or to the end (counter-clockwise).
2. Asserts: circle on screen; `0 < bar_width <= radius`; start != end (mod 361); `angle_steps` int > 0;
   `angle_sep >= 0`; `angle_sep * angle_steps < 360`. Clamp the value.
3. Canvas: `Image.new('RGB', (d, d), background_color)` or the background image cropped to
   `(xc - r, yc - r, xc + r, yc + r)`.
4. `pct = (value - min) / (max - min)`; `angle_start %= 361`; `angle_end %= 361` (**modulo 361**: 405 -> 44).
5. Clockwise: `ecart = 360 - start + end if end < start else end - start`.
   - track (if `draw_bar_background`): arc `start .. start + ecart` in `bar_background_color`;
   - decoration `Ellipse`: dots at `angle_end` (`bar_background_color`), `angle_start` (`bar_color`) and
     `angle_start + pct * ecart` (`bar_color`) (section 1.7);
   - `angle_sep == 0`: one arc `start .. start + pct * ecart`;
   - segmented: `ac = ecart / angle_steps`, `angleE = start + pct * ecart`, `n = int((angleE - start) / ac)`; arcs
     `start + i*ac .. start + (i+1)*ac - angle_sep` for `i` in `0..n-1`, then `start + n*ac .. angleE` (a zero-length
     arc draws nothing).
6. Counter-clockwise: `ecart = start - end if end < start else 360 - end + start`; track `start - ecart .. start`;
   decorations at end, start and `start - pct * ecart`; solid arc `start - pct * ecart .. start`; segmented:
   `angleS = start - pct * ecart`, `n = int((start - angleS) / ac)`, arcs `start - (i+1)*ac + angle_sep .. start - i*ac`,
   then `angleS .. start - n*ac`.
7. Text if `with_text`: when `text is None`, `text = f"{int(pct * 100 + .5)}%"`; `(l, t, r, b) = font.getbbox(text)`;
   `w = r - l`, `h = b - t`; `draw.text((radius - w/2 + off_x, radius - t - h/2 + off_y), text)` (anchor `la`; the
   left bearing is not subtracted; float positions give Pillow sub-pixel starts). Themes pass `""` when `SHOW_TEXT` is
   false, which draws nothing.
8. If `custom_bbox != (0, 0, 0, 0)`: crop the canvas to it (local coordinates) and display at
   `(xc - r + cb[0], yc - r + cb[1])`; else display at `(xc - r, yc - r)`.

### 1.7 `DrawRadialDecoration(draw, angle, radius, width, color)` (`lcd_comm.py:529-549`)

```
c, s = cos(angle in degrees), sin(angle in degrees)
xf = c * (radius - width/2) + radius
yf = s * (radius - width/2) + radius
xf = floor(xf + 0.5); yf = floor(yf + 0.5)      # the "== 0.5" special case compares a tuple (math.modf) to 0.5
                                                 # and is therefore never taken
ellipse([xf - width/2, yf - width/2, xf + width/2, yf + width/2 - 2], outline=color, fill=color, width=1)
```

(The bottom coordinate is literally `yf - 1 + width/2 - 1`.) The `Rectangle` decoration named by some themes is not
implemented.

### 1.8 Pillow behaviours that matter for pixel parity

- Text is anti-aliased (8-bit coverage, font mode `L`) on RGB/RGBA canvases; on palette (`P`) backgrounds Pillow draws
  1-bit (aliased) text. The fill is blended per channel with the coverage; on RGBA canvases alpha is blended too.
- Layout engine: RAQM (HarfBuzz shaping, kerning, ligatures) when libraqm is available, otherwise BASIC (FreeType
  kerning only). Output differs between installations.
- Anchors: horizontal `l` left / `m` middle / `r` right of the advance box; vertical `a` ascender, `t` top of ink,
  `m` middle, `s` baseline, `b` bottom of ink, `d` descender.
- Rectangle and line coordinates: floats truncated toward zero; rectangle bounds inclusive.
- `arc(start == end)` draws nothing; spans >= 360 draw a full ring.
- `rotate(90/180/270, expand=True)` and `transpose` are lossless; `rotate(90)` is counter-clockwise.
- Widgets never composite onto the current screen: each widget repaints its whole rectangle from the background image
  (or colour), so overlapping widgets erase each other within their rectangles.
- Exact glyph parity also depends on FreeType load flags and hinting and on Pillow's blend rounding (C code outside
  the Python repository). Text parity is therefore "close", not byte-exact: test geometry exactly and glyph pixels
  with tolerances.

### 1.9 Reference renders (verified)

Produced by calling the Python API offline (320 x 480 portrait screen). The rectangle is what `DisplayPILImage`
received. Fonts are files from the Python repo's `res/fonts/`.

| # | Call | Arguments (defaults omitted) | Sent rectangle x, y, w, h | Mode |
|---|---|---|---|---|
| 0 | DisplayText | "Basic text" at (50,85), RobotoMono-Regular | 50, 91, 120, 15 | RGB |
| 1 | DisplayText | "CPU 42%" at (10,10), RobotoMono-Regular 10, red on black, anchor `lt` | 10, 10, 42, 8 | RGB |
| 2 | DisplayText | "100%" at (200,10), Roboto-Bold 24, `lt` | 200, 10, 59, 17 | RGB |
| 3 | DisplayText | then "9%" at (200,10), same style (ghosting union) | 200, 10, 59, 17 | RGB |
| 4 | DisplayText | "mid" in box (20,300,100,40), Roboto-Regular 18, `mm`, white on blue | 20, 300, 100, 40 | RGB |
| 5 | DisplayText | "right" in box (150,300,100,40), Roboto-Regular 18, `rb` | 150, 300, 100, 40 | RGB |
| 6 | DisplayText | "Custom italic multiline text\nright-aligned" at (5,120), Roboto-Italic 20, blue on yellow, align right | 5, 123, 234, 44 | RGB |
| 7 | DisplayText | "Transparent" at (5,180), GeForce-Bold 30, white, background image (3.5" theme background) | 5, 190, 157, 26 | RGBA |
| 8 | DisplayProgressBar | (10,40,140,30) value 40, yellow, outline | 10, 40, 140, 30 | RGB |
| 9 | DisplayProgressBar | (10,80,140,30) value 0, red, no outline | 10, 80, 140, 30 | RGB |
| 10 | DisplayProgressBar | (160,40,140,30) value 40, green, reverse, no outline | 160, 40, 140, 30 | RGB |
| 11 | DisplayProgressBar | (10,120,20,100) value 60, blue | 10, 120, 20, 100 | RGB |
| 12 | DisplayProgressBar | (40,120,20,100) value 60, blue, reverse | 40, 120, 20, 100 | RGB |
| 13 | DisplayProgressBar | (70,120,100,10) min 25, max 95, value 60, black | 70, 120, 100, 10 | RGB |
| 14 | DisplayRadialProgressBar | centre (98,260) r 25, width 4, value 37, sep 0, green | 73, 235, 50, 50 | RGB |
| 15 | DisplayRadialProgressBar | centre (222,260) r 40, width 13, start 405, end 135, 10 steps, sep 5, CCW, value 63, orange, text "63°C" red Roboto-Black 20 | 182, 220, 80, 80 | RGB |
| 16 | DisplayRadialProgressBar | centre (60,400) r 50, width 10, start 135, end 45, sep 0, CW, value 70, red, track (80,80,80), `Ellipse`, Roboto-Black 16 white on black | 10, 350, 100, 100 | RGB |
| 17 | DisplayRadialProgressBar | centre (180,400) r 50, width 10, start 180, end 0, CW, value 50, (0,128,255), custom_bbox (0,0,100,55), text offset (0,-10), Roboto-Black 14 | 130, 350, 100, 55 | RGB |
| 18 | DisplayLineGraph | (100,120,200,80), values `[nan x5, 10, 30, 25, 80, 95, 60, 40, 55, 70, 20]`, red, axis | 100, 120, 200, 80 | RGB |
| 19 | DisplayLineGraph | (100,210,200,60), same values, autoscale, width 1, axis blue, background (240,240,240) | 100, 210, 200, 60 | RGB |

Final `text_bbox_cache`: `(50,85)` -> `[50, 91, 170, 106]`; `(10,10)` -> `[10, 10, 52, 18]`; `(200,10)` ->
`[200, 10, 231, 27]` (the un-unioned bbox of "9%"); `(5,120)` -> `[5, 123, 239, 167]`; `(5,180)` ->
`[5, 190, 162, 216]`.

Bbox check for Roboto Mono Regular 20 px, "Basic text" at (50,85): `la` (50,91,170,106); `lt` (50,85,170,100);
`ls` (50,70,170,85); `ld` (50,64,170,79); `mm` (-10,78,110,93); `rb` (-70,70,50,85).

The images themselves are not committed here. The fonts are files of the Python repository's `res/fonts/` (each
directory carries its own licence); tests that reuse these rectangles must use the same font files.

## 2. Value formatting before rendering

Sensor values are formatted before they reach the widgets: `int()` truncation, `MIN_SIZE` right-alignment and unit
strings. See [themes-python-yaml.md](themes-python-yaml.md) section 8.2 (Python) and [sensors.md](sensors.md)
section 3 (vendor app).

## 3. Vendor app reference (GDI+)

### 3.1 Canvas and draw order

- Canvas: `new Bitmap(theme.width, theme.height)` at 96 dpi with `SmoothingMode = HighQuality`, `InterpolationMode =
  HighQualityBicubic`, `CompositingQuality = AssumeLinear`, `TextRenderingHint = AntiAlias`.
- Elements are drawn in `GraphList` order (painter's algorithm). `hide == true` elements are skipped. The "hide data"
  setting skips everything except the background.
- When the theme has a background video, the PC render **omits item 0** (the Animation) and produces only the overlay;
  the video plays on the device (or is composited under the overlay on the PC for WCH panels).
- In the editor the canvas is first cleared to black (white/transparent in one OEM design build).
- Values: every element reads its binding's `Rate` (0..1) or `ValueWithUnit` string, refreshed on every rendered frame
  (theme `FrameRate`, default 20, or the video fps).

### 3.2 Text and Data

- Font = `Font(family, size, bold ? Bold : Regular)` with `size` in **points**; a missing family falls back to
  Microsoft YaHei.
- Brush: `SolidBrush(color)`; if `GrDirection != 0` (and `interval == 0`): `LinearGradientBrush(rect(pos,
  MeasureString(text)), color, GrColor, GrDirection - 1)`; for centred text that rect is shifted left by width / 2.
- `interval == 0`: `DrawString(text, font, brush, PointF(posX, posY), StringFormat { Alignment = alignment.index })`.
  `posX` is the **left edge, the centre or the right edge** per the alignment (Near/Center/Far). The vertical anchor is
  the top of the GDI+ line box, which includes GDI+ internal leading and padding.
- `interval != 0` (letter spacing): per-character `DrawString`, advancing `MeasureString(c).Width + interval`; a `'.'`
  is pulled left by 10 % of its width; right-aligned text is drawn backwards from `posX`; centred text starts at
  `posX - totalWidth / 2`.
- Drawn string = `m_data.ValueWithUnit` (the literal text for `Text` elements).

### 3.3 StatuBar (progress bar)

- Fill length `L = (int)(width * Rate)`; `revert` -> `Rate = 1 - Rate`.
- `direction`: 0 left->right `(posX, posY, L, height)`; 1 right->left `(posX + width - L, posY, L, height)`;
  2 bottom->top `(posX, posY + width - L, height, L)` (**`width` is the length and `height` the thickness**);
  3 top->bottom `(posX, posY, height, L)`.
- Background: unless `trBack`, a rect `(posX - lineWidth, posY - lineWidth, w + 2*lineWidth, h + 2*lineWidth)` filled
  with `BackColor`; `fillBack` fills the empty track; `lineWidth` is also drawn as an outline with
  `Pen(BackColor, lineWidth * 2)`.
- `useGradient` -> `LinearGradientBrush(rect, FrontColor, GradientColor, mode)`, else `SolidBrush(FrontColor)`.
- `useSubsection` (segmented): segment = `width / 20`, gap = `max(1, width / 100)`; a partially filled last segment is
  clipped.
- `radius > 0`: rounded rectangles; a fill below 8 % is forced to `width * 0.08` so the rounded shape renders. The
  editor rejects `radius > height / 2`.

### 3.4 ArchBar (ring gauge)

- Bounding square `(posX, posY, diameter, diameter)`; pen width = `archWidth` rounded **up to odd**.
- Start angle `startPer * 0.01 * 360 - 90` (GDI+ 0° = 3 o'clock, clockwise; -90 = 12 o'clock); total sweep
  `totalAngel` (0..360).
- Track: arc in `BackColor` over the full sweep unless `trBack`. Value: sweep `totalAngel * Rate` in `FrontColor`.
- `revert` fills from the end of the sweep. `useBlock`: 15° segments with 4° gaps. `round`: round caps (`FillEllipse`
  of diameter `archWidth` at both ends of the track and of the value arc). `round` and `useBlock` are mutually
  exclusive in the editor.
- Editor limits: `archWidth <= diameter`, `startPer` 0..99, `totalAngel` 0..360. `GradientColor`, `fillBack`,
  `lineWidth` and `height` are serialized but unused.

### 3.5 Clock (rotating needle)

- `TranslateTransform(centerX, centerY)`; `RotateTransform(angle + (Rate - offset) * endAngle)` (or `angle - ...`
  when `revert`); `DrawImage(bitmap, posX, posY)`; `ResetTransform`.
- So the needle image is drawn in a coordinate system rotated about the pivot, and **(posX, posY) is the needle
  image's offset from the pivot** (example stock theme: pivot (238, 274..1696), angle 0, sweep 265°, needle at
  (-215, 52)).
- `moveOpoint` ("move origin") sets the pivot to (posX, posY) and resets the position.
- The needle image is scaled by `zoom_rate` like an Image (editor default 0.9).
- The editor marks the pivot with a 4 x 4 red square unless `isVisualTheme`.
- One OEM build treats needle images named `01.png` .. `05.png` / `11.png` .. `16.png` as LED segments shown only when
  `Rate` crosses 1/5 or 1/6 thresholds. Bezel does not need this mode.

### 3.6 Chart (scrolling area graph)

- One point per history sample, spaced `columnWidth` px; `y = posY + height - value * coefficient`, clamped to `posY`.
- `rollDirection` puts the newest sample at the right or at the left.
- Area fill `FillColor` (alpha from the "fill transparency" setting 0..255); line `Pen(LineColor, lineWidth)` if
  `lineWidth > 0`; border `DrawRectangle(BorderColor, borderWidth)` if `> 0`.
- For `RAM`/`RAMVALID` the max value becomes RAMTOTAL; for `RAM_GB`/`RAMVALID_GB` RAMTOTAL / 1024.
- Setting the width recomputes `queueLen = width / columnWidth`.
- The editor shows a canned wave `{0.1, 0.2, 0.2, 0.3, 0.2, 0.1, 0.1, 0.3, 0.6, 0.9, 1, 1, 1, 0.8, 0.6, 0.6, 0.5,
  0.3, 0.3} * maxValue`.

### 3.7 Image

- `DrawImage(bitmap, posX, posY)` at 1:1 (the stored, already scaled bitmap).
- Editor zoom: `zoom_rate +- 0.02` within (0.12, 2.0); `bitmap = resize(O_bitmap, w * zoom, h * zoom)`.
- "Fill" **stretches** `O_bitmap` to exactly the canvas size, ignoring the aspect ratio, and sets the position to
  (0, 0). "Fill colour" replaces the image with a solid canvas-sized bitmap.

### 3.8 Animation (background video)

- Editor: the poster (first frame) at (posX, posY). Device: the real video (see [video.md](video.md)).
- Zoom slider 11..112 with neutral 56: `zoom = 1 + (v - 56) * 0.02 * 3` above 56, `1 - (56 - v) * 0.02` below
  (minimum 0.1). "Fill" = cover (`zoom = max(W / w, H / h)`). Rotate buttons turn the poster and `crop.rotate` by 90°.
- At save the visible window becomes `crop.rect` in source pixels; blank areas are refused.
- Only one Animation per theme, always at index 0.

### 3.9 GDI+ behaviours that matter

- Font sizes are points at 96 dpi (px = pt * 4 / 3). Pillow sizes are pixels. A 24 pt vendor font is a 32 px Bezel
  font.
- `DrawString` at a point positions the layout box, not the glyph ink: the box includes GDI+ padding and the font's
  internal leading, so text sits lower and further right than an ink-anchored draw. Bezel emulates this with the
  font's ascent/line-gap metrics; exact offsets are to be measured (unconfirmed).
- Arc angles: 0° = 3 o'clock, positive = clockwise (same convention as Pillow).
- GDI+ pens are centred on the path; the odd pen-width rounding of ArchBar keeps the ring symmetric.
- Anti-aliased text over a transparent canvas produces partially transparent edge pixels, which matters when the
  overlay is composited over a device-side video (POSLEN keeps pixels with alpha > 15).

## 4. Composition models compared

| Aspect | Python app | Vendor app |
|---|---|---|
| Unit of update | one widget rectangle per update | the whole frame, re-rendered every frame |
| Transparency | emulated: each widget repaints from the full-screen background image | real alpha compositing on the canvas |
| Z-order | update order; overlapping widgets erase each other | `GraphList` order |
| Partial updates on the wire | rectangle bitmaps (rev A/B/C/D, WeAct); full frames (TUR_USB) | diff run lists (serial), full frames (WCH, 0x1CBE) |

Recommended model for Bezel: compose a full frame with real alpha (like the vendor app), then compute changed regions
for protocols that accept partial updates. Python themes still render correctly under this model because their widgets
are opaque over the background.
