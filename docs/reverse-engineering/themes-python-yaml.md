# Themes: turing-smart-screen-python YAML format

The complete theme format of turing-smart-screen-python at `2b33ab4`, as Bezel imports it. Rendering of each widget
is in [rendering.md](rendering.md); sensor sources in [sensors.md](sensors.md). Confidence: **static** (read from
`library/config.py`, `library/stats.py`, `library/display.py`, `library/scheduler.py` and the 73 shipped themes).

## 1. Package and loading

- A theme is a directory `res/themes/<Name>/` containing `theme.yaml`, its images (PNG, referenced relative to the theme
  directory) and an optional `preview.png`. It is selected by `config.yaml: config.THEME` = the directory name
  (`library/config.py:44-55`).
- Parsed with PyYAML `safe_load` (**YAML 1.1**).
- `res/themes/default.yaml` (every `SHOW: False`, every `INTERVAL: 0`) is loaded first, and every key missing from the
  theme is filled from it recursively (`copy_default`, `library/config.py:41-47`). A theme therefore lists only what
  it shows.
- A missing or invalid theme logs an error and exits with code 0 (`library/config.py:57-70`).
- Fonts are **not** per theme: `FONT` is a path relative to the global `res/fonts/` (`library/stats.py:114`,
  `library/display.py:174`).
- No schema validation exists anywhere: unknown keys are silently ignored.

## 2. YAML 1.1 quirks a YAML 1.2 loader must handle

| Quirk | Handling |
|---|---|
| Booleans are written `True`, `False`, `TRUE`, `FALSE`, `true`, `false` (and YAML 1.1 also allows `yes/no/on/off`). `TRUE`/`FALSE` are booleans in PyYAML but plain strings in YAML 1.2 parsers. | lenient boolean deserializer |
| Colours are unquoted `255, 255, 255`: a **string**. | parse `"r, g, b"` (spaces optional) |
| `#ff0000` unquoted is a YAML comment (value null). | must be quoted in themes; treat null as "missing" |
| `DISPLAY_SIZE: 3.5"` is a plain scalar ending with a double quote; `0.96"`, `2.1"`, `8.8"` are strings, never numbers. | string |
| `DISPLAY_RGB_LED: 255, 0, 0` is a string. | colour |
| Floats appear in `MIN_VALUE`/`MAX_VALUE` (for example `5.3`). | f64 |
| Lists: `CUSTOM_BBOX: [0, 0, 80, 160]`, `TEXT_OFFSET: [-30, 0]`. | fixed arrays |
| Inline `# comments` after values are everywhere; text containing `:` must be quoted (`TEXT: "Used:"`). | standard |
| `FORMAT: "HH:mm:ss zzz"`, `"MM/dd/yyyy"`, `yyyy.MM.dd` are CLDR (Babel) patterns, not strftime. | CLDR formatter |
| Keys present in shipped themes that the engine never reads: `ROTATION` (0.96" WeAct themes), `UNIT_ML`, `MOUNT_POINT` (only `/` is ever used). | warn, accept |

Colour parsing (`library/lcd/color.py:20-47`): a 3-element list/tuple (cast to int), a `"r, g, b"` string, or any
Pillow colour string (`#rgb`, `#rrggbb`, names such as `red`, `rgb(...)`, `hsl(...)`; an RGBA result is truncated to
RGB).

## 3. Top-level keys

| Key | Type | Notes |
|---|---|---|
| `author` | string | default `unknown`; configure.py renders a GitHub link when it starts with `@`. Present in 70 of 73 themes. |
| `display` | map | section 4 |
| `static_images` | ordered map | section 5 |
| `static_text` | ordered map | section 6 (54 themes) |
| `STATS` | map | section 8 |

## 4. `display`

| Key | Type | Default | Meaning |
|---|---|---|---|
| `DISPLAY_SIZE` | string | `3.5"` | Logical portrait resolution: `0.96"` 80x160; `2.1"` 480x480 (round); `2.8"` 480x480; `3.5"` 320x480; `4.6"` 320x960; `5"` 480x800; `5.2"` 720x1280; `8"` 800x1280; `8.8"` 480x1920; `9.2"` 480x1920 (real panel 462 wide, kept 8.8"-compatible); `12.3"` 720x1920. Unknown -> warning, 3.5" (`library/display.py:58-84`). 26 shipped themes omit it. Never checked against the connected screen (`config.check_theme_compatible` is dead code, `library/config.py:72-80`). `2.1"` previews get a circular mask. |
| `DISPLAY_ORIENTATION` | `portrait` \| `landscape` | required (KeyError if missing) | Base orientation. `config.yaml display.DISPLAY_REVERSE: true` turns it into the reverse variant (`library/display.py:41-55`). Landscape swaps W and H (3.5" landscape = 480 x 320). Shipped: 45 portrait, 28 landscape. |
| `DISPLAY_RGB_LED` | colour | `255, 255, 255` | Backplate LED colour, used only by XuanFang flagship (rev B): sent at start, `0,0,0` at stop. |

## 5. `static_images`

Map of name -> block, drawn once at startup in file order (back to front) (`library/display.py:152-162`):

| Key | Type | Default | Meaning |
|---|---|---|---|
| `PATH` | string | required | image path relative to the theme directory |
| `X`, `Y` | int | 0 | top-left |
| `WIDTH`, `HEIGHT` | int | 0 | if both non-zero and different from the image size, the image is resized (Pillow default filter) |

Almost every theme has a single `BACKGROUND` covering the screen. Static images and texts are redrawn after a Windows
resume from sleep (`main.py:165-167`).

## 6. `static_text`

Map of name -> block, drawn once in order. Keys: `TEXT` (required, non-empty), `X`, `Y`, `WIDTH` (0), `HEIGHT` (0),
`FONT` (`roboto-mono/RobotoMono-Regular.ttf`), `FONT_SIZE` (10), `FONT_COLOR` (`0, 0, 0`), `BACKGROUND_COLOR`
(`255, 255, 255`), `BACKGROUND_IMAGE`, `ALIGN` (`left`), `ANCHOR` (`lt`). Same text engine as dynamic text.
Shipped `ANCHOR` usage: `lt` 61, `rt` 20, `mm` 18, `mt` 12, `rm` 9, `lm` 6, `mb` 3. `ALIGN`: left 25, right 22,
center 5 (it only matters for multi-line text).

## 7. Widget blocks

Every displayed value is a leaf block named `TEXT`, `GRAPH`, `RADIAL` or `LINE_GRAPH`, plus a few TEXT blocks placed
directly under a metric (`PERCENT_TEXT`, `USED`, `FREE`, `TOTAL`). A block is drawn only when `SHOW` is true.
Coordinates are absolute pixels in the current-orientation screen space, origin top-left. There is no layout system,
no z-order other than update order, and no compositing: transparency is emulated with `BACKGROUND_IMAGE`
([rendering.md](rendering.md) section 1.3).

### 7.1 TEXT (`library/stats.py:94-121`)

| Key | Type | Default | Semantics |
|---|---|---|---|
| `SHOW` | bool | false | draw or not |
| `SHOW_UNIT` | bool | true | append the metric's unit (`%`, `°C`, ` GHz`, ` M`, ` G`, ` FPS`, `ms`) |
| `MIN_SIZE` | int | per metric (section 8.2) | right-align the value in this many characters (space padded) |
| `X`, `Y` | int | 0 | anchor point (may be negative) |
| `WIDTH`, `HEIGHT` | int | 0 | both > 0: fixed box (text clipped, anchor moved inside the box); otherwise auto box. `WIDTH > 0` with `HEIGHT = 0` -> height = font size |
| `FONT` | path under `res/fonts/` | `roboto-mono/RobotoMono-Regular.ttf` | |
| `FONT_SIZE` | int (px) | 10 | FreeType pixel size |
| `FONT_COLOR` | colour | `0, 0, 0` | |
| `BACKGROUND_COLOR` | colour | **`255, 255, 255`** | solid box background (white when neither background key is set) |
| `BACKGROUND_IMAGE` | theme-relative path | none | wins over the colour; must be a full-screen image aligned at (0,0) |
| `ALIGN` | `left` \| `center` \| `right` | `left` | multi-line only |
| `ANCHOR` | Pillow two-letter anchor | `lt` | horizontal `l`/`m`/`r`, vertical `a`/`t`/`m`/`s`/`b`/`d` |
| `FORMAT` | string | `medium` | `DATE.DAY` and `DATE.HOUR` only: Babel `short`/`medium`/`long`/`full` or a CLDR pattern |

### 7.2 GRAPH: progress bar (`library/stats.py:142-159`)

`SHOW`, `X`, `Y`, `WIDTH` (0), `HEIGHT` (0), `MIN_VALUE` (0), `MAX_VALUE` (100), `BAR_COLOR` (`0, 0, 0`),
`BAR_OUTLINE` (false), `BACKGROUND_COLOR` (`255, 255, 255`), `BACKGROUND_IMAGE`, `REVERSE_DIRECTION` (false).
Horizontal if `WIDTH > HEIGHT`, else vertical (fills bottom-up). The value is passed as `int()`. MIN/MAX may be floats.

### 7.3 RADIAL: radial progress bar (`library/stats.py:162-201`)

| Key | Type | Default | Semantics |
|---|---|---|---|
| `SHOW` | bool | false | |
| `X`, `Y` | int | 0 | **centre** |
| `RADIUS` | int | 1 | outer radius |
| `WIDTH` | int | 1 | ring thickness (<= radius) |
| `MIN_VALUE`, `MAX_VALUE` | number | 0, 100 | |
| `ANGLE_START`, `ANGLE_END` | int (deg) | 0, 360 | 0 = 3 o'clock, positive = clockwise on screen; reduced **modulo 361** |
| `ANGLE_STEPS` | int | 1 | number of segments over the sweep |
| `ANGLE_SEP` | int (deg) | 0 | gap per segment; 0 = solid arc |
| `CLOCKWISE` | bool | **false** | |
| `BAR_COLOR` | colour | `0, 0, 0` | |
| `BAR_BACKGROUND_COLOR` | colour | `0, 0, 0` | unfilled track colour |
| `DRAW_BAR_BACKGROUND` | bool | false | draw the track |
| `BAR_DECORATION` | string | `""` | `Ellipse` = round caps and knob. `Rectangle` is used by 10 shipped themes but **not implemented** (no-op). |
| `SHOW_TEXT` | bool | false | inner text; when false the text is `""` (nothing drawn) |
| `SHOW_UNIT` | bool | true | |
| `FONT`, `FONT_SIZE`, `FONT_COLOR` | | `roboto-mono/RobotoMono-Regular.ttf`, 10, `0, 0, 0` | inner text style (the library default font, used only by direct API calls, is `roboto/Roboto-Black.ttf`) |
| `BACKGROUND_COLOR` | colour | **`0, 0, 0`** | |
| `BACKGROUND_IMAGE` | path | none | |
| `CUSTOM_BBOX` | `[x0, y0, x1, y1]` | `[0, 0, 0, 0]` | crop of the 2R x 2R canvas in local coordinates (half/quarter gauges that do not erase neighbours) |
| `TEXT_OFFSET` | `[dx, dy]` | `[0, 0]` | inner text shift from the centre |

The inner text is the value formatted as in TEXT (right-aligned to the metric's `MIN_SIZE`, + unit), or the custom
sensor's string. The library defaults of
`DisplayRadialProgressBar` (clockwise, 10 steps, 5° separation, white background) differ from these theme defaults.

### 7.4 LINE_GRAPH (`library/stats.py:222-245`)

`SHOW`, `X`, `Y`, `WIDTH` (1), `HEIGHT` (1), `MIN_VALUE` (0), `MAX_VALUE` (100), `HISTORY_SIZE` (10), `AUTOSCALE`
(false), `LINE_COLOR` (`0, 0, 0`), `LINE_WIDTH` (2), `AXIS` (false), `AXIS_COLOR` (= `LINE_COLOR`), `AXIS_FONT`
(`roboto/Roboto-Black.ttf`), `AXIS_FONT_SIZE` (10), `BACKGROUND_COLOR` (`0, 0, 0`), `BACKGROUND_IMAGE`.

- `HISTORY_SIZE` also creates and sizes the metric's history buffer, and is read even when `SHOW` is false.
- Sample spacing = the metric's `INTERVAL`; x resolution = `WIDTH / HISTORY_SIZE`.
- `axis_minmax_format` (`{:0.0f}`) is not exposed to themes.

## 8. `STATS`

### 8.1 Key tree (everything the engine reads)

`INTERVAL` is in seconds (integers in all shipped themes). CPU has one interval per metric; every other group has one
interval for the whole group. `INTERVAL` 0 or absent = the group's thread is not started (metric disabled).

```
STATS:
  CPU:
    PERCENTAGE:   {INTERVAL, TEXT, GRAPH, RADIAL, LINE_GRAPH}
    FREQUENCY:    {INTERVAL, TEXT, GRAPH, RADIAL (broken), LINE_GRAPH}
    LOAD:         {INTERVAL, ONE: {TEXT}, FIVE: {TEXT}, FIFTEEN: {TEXT}}
    TEMPERATURE:  {INTERVAL, TEXT, GRAPH, RADIAL, LINE_GRAPH}
    FAN_SPEED:    {INTERVAL, TEXT, GRAPH, RADIAL, LINE_GRAPH}
  GPU:
    INTERVAL
    PERCENTAGE:     {GRAPH, RADIAL, TEXT, LINE_GRAPH}
    MEMORY_PERCENT: {GRAPH, RADIAL, TEXT, LINE_GRAPH}
    MEMORY_USED:    {TEXT}                            # MiB
    MEMORY_TOTAL:   {TEXT}                            # MiB
    MEMORY:         {GRAPH, RADIAL, TEXT}             # deprecated alias: GRAPH/RADIAL = % used, TEXT = used MiB
    TEMPERATURE:    {TEXT, GRAPH, RADIAL, LINE_GRAPH}
    FPS:            {TEXT, GRAPH, RADIAL, LINE_GRAPH}
    FAN_SPEED:      {TEXT, GRAPH, RADIAL, LINE_GRAPH}
    FREQUENCY:      {TEXT, GRAPH, RADIAL (broken), LINE_GRAPH}
  MEMORY:
    INTERVAL
    SWAP:    {GRAPH, RADIAL, LINE_GRAPH}              # no text widgets for swap
    VIRTUAL: {GRAPH, RADIAL, LINE_GRAPH, PERCENT_TEXT, USED, FREE, TOTAL}   # last four are TEXT blocks
  DISK:
    INTERVAL
    USED:  {GRAPH, RADIAL, LINE_GRAPH, TEXT, PERCENT_TEXT}                  # PERCENT_TEXT is a TEXT block
    TOTAL: {TEXT}
    FREE:  {TEXT}
  NET:
    INTERVAL
    WLO | ETH:
      UPLOAD:   {TEXT, LINE_GRAPH}
      UPLOADED: {TEXT}
      DOWNLOAD: {TEXT, LINE_GRAPH}
      DOWNLOADED: {TEXT}
  DATE:
    INTERVAL
    DAY:  {TEXT (+FORMAT)}
    HOUR: {TEXT (+FORMAT)}
  UPTIME:
    INTERVAL
    SECONDS:   {TEXT}
    FORMATTED: {TEXT}
  WEATHER:
    INTERVAL                                          # effective minimum 300 s
    TEMPERATURE | TEMPERATURE_FELT | UPDATE_TIME | HUMIDITY | WEATHER_DESCRIPTION: {TEXT}
  PING:
    INTERVAL
    TEXT, GRAPH, RADIAL, LINE_GRAPH
  CUSTOM:
    INTERVAL
    <ClassNameInSensorsCustom>: {TEXT, GRAPH, RADIAL, LINE_GRAPH}   # any number of classes
```

`NET.WLO` and `NET.ETH` are two fixed slots bound to the interface names in `config.yaml` (`WLO`, `ETH`).
`CUSTOM.<Class>` names a Python class in `library/sensors/sensors_custom.py`; Bezel maps these to its own custom-source
mechanism and reports unknown classes.

### 8.2 Value formatting per metric (`library/stats.py`)

Values are truncated with `int()` for percent, temperature, FPS, MB and GB (text, radial and bar). Line graphs receive
the un-truncated floats. Text = `f"{value:>{MIN_SIZE}}"` then the unit if `SHOW_UNIT`.

| Metric | Text format | Default `MIN_SIZE` |
|---|---|---|
| CPU/GPU percentage, memory/disk percent, fan % | `int` + `%` | 3 |
| CPU load average (ONE/FIVE/FIFTEEN) | `int(loadavg)` + `%` (the raw run-queue length, not a percentage) | 3 |
| Temperatures | `int` + `°C` (U+00B0) | 3 |
| CPU/GPU frequency | `f"{MHz/1000:.2f}"` + ` GHz` (GRAPH gets `int(GHz)`) | 4 |
| GPU FPS | `int` + ` FPS` | 4 |
| GPU memory used/total, RAM used/free/total | `int(MiB)` + ` M` | 5 |
| Disk used/total/free | `int(bytes / 1e9)` + ` G` (decimal) | 5 |
| Ping | `int(ms)` + `ms` | 6 |
| Net totals (UPLOADED/DOWNLOADED) | psutil `bytes2human` (1024 base), for example `10.3G`; no unit suffix | 6 |
| Net rates (UPLOAD/DOWNLOAD) | `bytes2human(rate, "%(value).1f %(symbol)s/s")`, for example `1.5 M/s` | 10 |
| Date (DAY/HOUR) | Babel `format_date` / `format_time` with `FORMAT` | 0 |
| Uptime SECONDS / FORMATTED | raw int seconds / `str(timedelta)` (`49 days, 17:00:36`, English only) | 0 |
| Weather | temperature `:.1f` + `°C`/`°F`/`°K`; felt temperature in parentheses; humidity `:.0f%`; description capitalised; update time `@HH:MM` | 0 |
| Custom | `as_string()`, else `str(as_numeric())` | 0 |

### 8.3 INTERVAL and the scheduler (`library/scheduler.py:53-81`)

- One thread per group (per metric for CPU), each a `sched.scheduler` loop. The periodic wrapper schedules the next
  run `INTERVAL` seconds ahead **before** invoking the action: fixed rate, first run immediate. An action longer than
  `INTERVAL` (CPU% blocks for `INTERVAL`, ping up to 4 s) makes the next run fire immediately after it.
- `INTERVAL` is also the CPU% measurement window, the divisor of the network rate on the Python backend, and the line
  graph sample spacing (history length x interval = visible time span).
- All widgets of a group refresh together (GPU's metrics are one tick).
- Threads start 0.25 s apart in the order CPU percentage, frequency, load, temperature, fan, GPU (only when a GPU is
  detected), memory, disk, net, date, uptime, custom, weather, ping (`main.py:241-255`).
- Weather ignores `INTERVAL: 0`: its period is `max(300, INTERVAL)` and the HTTP call is skipped unless a weather TEXT
  has `SHOW: true`.
- Typical shipped values: PERCENTAGE 1; FREQUENCY/LOAD/TEMPERATURE/FAN 5; GPU 1; MEMORY 5; DISK 10; NET 1; DATE 1;
  UPTIME 1; PING 10; WEATHER 300; CUSTOM 3.

### 8.4 History and unsupported sensors

- Each metric keeps a history list of `LINE_GRAPH.HISTORY_SIZE` samples (default 10), pre-filled with NaN; each sample
  is appended and the oldest dropped. History is recorded even when the line graph is hidden, always at `INTERVAL`
  spacing (`library/stats.py:248-255`).
- If a value is NaN (or FPS < 0) and one of the metric's widgets has `SHOW: true`, a warning is logged **once** and
  `SHOW` is set to false in memory for those widgets: they are never drawn again and the background shows through.
  Bezel shows an explicit "unavailable" state instead ([sensors.md](sensors.md) section 1).

## 9. Worked example

A 3.5" landscape theme (480 x 320). CPU% every second as text, a segmented 270° gauge and a 60-sample line graph;
CPU temperature every 5 s; GPU group every second with a bar, text and used VRAM; RAM percent and used every 5 s;
disk used every 10 s; ethernet download rate in a fixed centred box; date and time with CLDR patterns; ping every
10 s; one custom sensor class. Everything else falls back to `default.yaml`. Validated to parse with PyYAML; every key
is one the engine reads.

```yaml
---
author: "@example"

display:
  DISPLAY_SIZE: 3.5"
  DISPLAY_ORIENTATION: landscape        # 480 x 320 logical pixels
  DISPLAY_RGB_LED: 0, 120, 255

static_images:
  BACKGROUND:
    PATH: background.png                # 480x320 PNG in the theme folder
    X: 0
    Y: 0
    WIDTH: 480
    HEIGHT: 320

static_text:
  CPU_LABEL:
    TEXT: "CPU"
    X: 20
    Y: 18
    FONT: roboto/Roboto-Bold.ttf
    FONT_SIZE: 18
    FONT_COLOR: 150, 150, 150
    BACKGROUND_IMAGE: background.png    # transparent look
  DISK_LABEL:
    TEXT: "Used:"
    X: 250
    Y: 260
    FONT: roboto-mono/RobotoMono-Bold.ttf
    FONT_SIZE: 14
    FONT_COLOR: 200, 200, 200
    BACKGROUND_COLOR: 20, 20, 20        # solid box instead

STATS:
  CPU:
    PERCENTAGE:
      INTERVAL: 1
      TEXT:
        SHOW: True
        SHOW_UNIT: True
        X: 90
        Y: 10
        FONT: jetbrains-mono/JetBrainsMono-Bold.ttf
        FONT_SIZE: 28
        FONT_COLOR: 255, 255, 255
        BACKGROUND_IMAGE: background.png
      RADIAL:
        SHOW: True
        X: 70
        Y: 110
        RADIUS: 50
        WIDTH: 12
        MIN_VALUE: 0
        MAX_VALUE: 100
        ANGLE_START: 135
        ANGLE_END: 45                   # 270 degree gauge
        ANGLE_STEPS: 10
        ANGLE_SEP: 3                    # 10 segments, 3 degrees gap
        CLOCKWISE: True
        BAR_COLOR: 0, 200, 255
        BAR_BACKGROUND_COLOR: 0, 40, 60
        DRAW_BAR_BACKGROUND: True
        BAR_DECORATION: Ellipse
        SHOW_TEXT: True
        SHOW_UNIT: True
        FONT: roboto/Roboto-Bold.ttf
        FONT_SIZE: 18
        FONT_COLOR: 255, 255, 255
        BACKGROUND_IMAGE: background.png
      LINE_GRAPH:
        SHOW: True
        X: 150
        Y: 60
        WIDTH: 140
        HEIGHT: 50
        MIN_VALUE: 0
        MAX_VALUE: 100
        HISTORY_SIZE: 60                # 60 samples x 1 s = last minute
        AUTOSCALE: False
        LINE_COLOR: 0, 200, 255
        LINE_WIDTH: 2
        AXIS: True
        AXIS_COLOR: 120, 120, 120
        AXIS_FONT: roboto/Roboto-Black.ttf
        AXIS_FONT_SIZE: 10
        BACKGROUND_IMAGE: background.png
    TEMPERATURE:
      INTERVAL: 5
      TEXT:
        SHOW: True
        X: 300
        Y: 10
        FONT: jetbrains-mono/JetBrainsMono-Bold.ttf
        FONT_SIZE: 28
        FONT_COLOR: 255, 180, 0
        BACKGROUND_IMAGE: background.png
  GPU:
    INTERVAL: 1
    PERCENTAGE:
      GRAPH:
        SHOW: True
        X: 20
        Y: 200
        WIDTH: 200
        HEIGHT: 14
        MIN_VALUE: 0
        MAX_VALUE: 100
        BAR_COLOR: 120, 255, 0
        BAR_OUTLINE: True
        BACKGROUND_IMAGE: background.png
      TEXT:
        SHOW: True
        X: 230
        Y: 195
        FONT: roboto-mono/RobotoMono-Bold.ttf
        FONT_SIZE: 18
        FONT_COLOR: 120, 255, 0
        BACKGROUND_IMAGE: background.png
    MEMORY_USED:
      TEXT:
        SHOW: True
        X: 20
        Y: 220
        FONT: roboto-mono/RobotoMono-Regular.ttf
        FONT_SIZE: 14
        FONT_COLOR: 200, 200, 200
        BACKGROUND_IMAGE: background.png
  MEMORY:
    INTERVAL: 5
    VIRTUAL:
      PERCENT_TEXT:                     # direct TEXT block
        SHOW: True
        X: 20
        Y: 250
        FONT: roboto-mono/RobotoMono-Bold.ttf
        FONT_SIZE: 18
        FONT_COLOR: 255, 255, 255
        BACKGROUND_IMAGE: background.png
      USED:                             # direct TEXT block
        SHOW: True
        X: 90
        Y: 252
        FONT: roboto-mono/RobotoMono-Regular.ttf
        FONT_SIZE: 14
        FONT_COLOR: 200, 200, 200
        BACKGROUND_IMAGE: background.png
  DISK:
    INTERVAL: 10
    USED:
      TEXT:
        SHOW: True
        X: 300
        Y: 260
        FONT: roboto-mono/RobotoMono-Bold.ttf
        FONT_SIZE: 14
        FONT_COLOR: 200, 200, 200
        BACKGROUND_COLOR: 20, 20, 20
  NET:
    INTERVAL: 1
    ETH:
      DOWNLOAD:
        TEXT:
          SHOW: True
          X: 330
          Y: 110
          WIDTH: 140                    # fixed box, centred
          HEIGHT: 20
          FONT: jetbrains-mono/JetBrainsMono-Bold.ttf
          FONT_SIZE: 16
          FONT_COLOR: 255, 255, 255
          BACKGROUND_IMAGE: background.png
          ANCHOR: mt
          ALIGN: center
  DATE:
    INTERVAL: 1
    DAY:
      TEXT:
        SHOW: True
        FORMAT: "EEE d MMM"             # CLDR pattern
        X: 470
        Y: 300
        FONT: roboto/Roboto-Bold.ttf
        FONT_SIZE: 16
        FONT_COLOR: 200, 200, 200
        BACKGROUND_IMAGE: background.png
        ANCHOR: rb
    HOUR:
      TEXT:
        SHOW: True
        FORMAT: short
        X: 470
        Y: 280
        FONT: roboto/Roboto-Bold.ttf
        FONT_SIZE: 30
        FONT_COLOR: 255, 255, 255
        BACKGROUND_IMAGE: background.png
        ANCHOR: rb
  PING:
    INTERVAL: 10
    TEXT:
      SHOW: True
      SHOW_UNIT: True
      X: 400
      Y: 60
      FONT: roboto-mono/RobotoMono-Regular.ttf
      FONT_SIZE: 14
      FONT_COLOR: 180, 180, 180
      BACKGROUND_IMAGE: background.png
  CUSTOM:
    INTERVAL: 3
    ExampleCustomNumericData:           # class name in library/sensors/sensors_custom.py
      TEXT:
        SHOW: True
        X: 330
        Y: 150
        FONT: roboto-mono/RobotoMono-Bold.ttf
        FONT_SIZE: 24
        FONT_COLOR: 61, 184, 225
        BACKGROUND_IMAGE: background.png
```

Without `BACKGROUND_IMAGE` on a dynamic widget its box is painted white (text, bars) or black (radial, line graph).

## 10. Shipped-theme statistics (for compatibility testing)

- 73 themes. Sizes: 3.5" 14 (+26 without `DISPLAY_SIZE`, 21 of them portrait), 5" 13, 8.8" 6, 2.1" 5, 0.96" 4, 8" 3,
  5.2" 1, 4.6" 1.
- Most used widgets: CPU.PERCENTAGE TEXT/GRAPH/RADIAL, CPU.TEMPERATURE.TEXT, GPU.PERCENTAGE.*,
  MEMORY.VIRTUAL.PERCENT_TEXT/GRAPH, DATE.DAY/HOUR, DISK.USED.PERCENT_TEXT, NET.ETH.*.TEXT. Rare: GPU.FAN_SPEED.RADIAL,
  CPU.FAN_SPEED.*, GPU.FPS, PING.*, UPTIME.FORMATTED, WEATHER.* (6 themes), CUSTOM (1 theme).
- Fonts are referenced from `res/fonts/`: families `roboto`, `roboto-mono`, `jetbrains-mono`, `generale-mono`,
  `geforce`, `digital` (7-segment), `racespace`, `fusion-pixel`, `misaki`, `Cubic_11`, `BoutiqueBitmap9x9`,
  `GlowSansSC-Compressed`, `SourceHanSansCN`. Most used: GeneraleMonoA (415 blocks), JetBrainsMono-Bold (224),
  Roboto-Bold (104), RobotoMono-Regular (82). Each directory carries its own licence file that applies on
  redistribution.

## 11. `res/themes/scale_theme.py`

Command line: `python scale_theme.py theme.yaml output.yaml --from 480x800 --to 800x1280` (sizes match
`(\d+)[xX×](\d+)`). It rewrites the text line by line with a regex, preserving comments and formatting, and only
matches lines of the form `^\s*KEY\s*:\s*<number>` (`scale_theme.py:56-89`).

Factors: `fx = dst_w / src_w`, `fy = dst_h / src_h`, `favg = (fx + fy) / 2`; results are `round()`ed to int (a float
stays a float when the source had a decimal point).

| Key | Factor |
|---|---|
| `X`, `RADIUS`, `WIDTH` | fx |
| `Y`, `HEIGHT` | fy |
| `FONT_SIZE`, `AXIS_FONT_SIZE` | favg |
| everything else | unchanged |

Limitations: `DISPLAY_SIZE` is not changed; background and static images are not resampled (their `WIDTH`/`HEIGHT`
are scaled but the files are not); `WIDTH` is scaled by fx for every block, including the radial ring thickness;
`LINE_WIDTH`, `TEXT_OFFSET`, `CUSTOM_BBOX` (lists are not matched), `ANGLE_*`, `MIN/MAX_VALUE` and `HISTORY_SIZE` are
not scaled; a single `RADIUS` cannot follow non-uniform scaling; static-image `X`/`Y` are scaled. Bezel scales in the
data model with per-property policies instead.

## 12. Typing summary for the Bezel importer

`Color = "r, g, b" | [u8; 3] | Pillow colour string`; lenient `Bool`; `i32` coordinates (may be negative); `f64` for
`MIN_VALUE`/`MAX_VALUE`; `Anchor = [lmr][atmsbd]` (not every combination is valid for multi-line text); image paths
theme-relative; font paths relative to the font root. Unknown keys: warn and keep.
