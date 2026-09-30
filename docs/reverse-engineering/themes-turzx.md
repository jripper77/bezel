# Themes: the vendor `.turtheme` format (TURZX V3.07)

The theme file format of the vendor application, as Bezel imports it. Confidence: **static** (26 installed themes
plus the app's dashboard theme were decoded offline with an independent NRBF parser; class layouts were read from the
serialized metadata). Rendering algorithms: [rendering.md](rendering.md) section 3.

## 1. Container

- A `.turtheme` is **not** a zip, not encrypted and not compressed. It is a raw **MS-NRBF** stream (.NET
  `BinaryFormatter`) of one `UsbMonitorL.Theme` object graph, **followed by zero padding**.
- The padding exists because the writer serializes into a `MemoryStream` and writes `GetBuffer()` (the stream's
  capacity) instead of its length. Example: the vendor dashboard theme is 125,626 bytes of NRBF followed by 70,626 zero
  bytes (196,252 in total). Readers must stop at the MessageEnd record (`0b`) and ignore the tail. Writers may omit the
  padding.
- The vendor reader is a plain `BinaryFormatter.Deserialize` with no binder and the default "simple" assembly format:
  assembly-version mismatches and **missing members are tolerated** (older files load with defaults). A separate
  binder remaps legacy type names containing `lcd207` (files of an older product line).
- Post-load fix-ups done by the vendor app:
  1. if `GraphList[0]` is a `GraphAnimation`: set `crop.rotate = 0` and `crop.ret = 0`;
  2. if `videoPath != null` and `GraphList[0]` is a `GraphImage`: convert that background image into a
     `GraphAnimation`;
  3. set `themePath` to the path the file was loaded from and `reload = false`;
  4. resolve fonts (section 7).

Related tool: the Python repository's `tools/turing-theme-extractor.py` handles an older vendor theme format (`.data`
files) as an opaque blob: it scans for the PNG signature `89 50 4e 47 0d 0a 1a 0a` and the IEND chunk tail
`49 45 4e 44 ae 42 60 82` and writes each PNG out, recovering no layout, fonts or bindings. Embedded bitmaps in
`.turtheme` files are raw PNG bytes too, so the same carving works on them, but Bezel parses the records instead.

### Security rule

Never deserialize a `.turtheme` (or any NRBF file) with a general-purpose deserializer that instantiates types named by
the file. Parse the NRBF records, accept only the classes listed in section 5 (plus the listed `System.*` helper
types), bound every length and count, reject unknown classes, and treat PNG payloads as untrusted images.

## 2. NRBF record walk-through

From a stock 480 x 1920 theme ("Simple black and white theme"), `UsbMonitorL` assembly version 1.1.0.0:

```
off       bytes / record                                            meaning
0x000000  00 01000000 ffffffff 01000000 00000000                    SerializedStreamHeader: RootId=1, HeaderId=-1, Major=1, Minor=0
0x000011  0c 02000000 42 "UsbMonitorL, Version=1.1.0.0, Culture=neutral, PublicKeyToken=null"
                                                                    BinaryLibrary id 2
0x000059  0c 03000000 51 "System.Drawing, Version=4.0.0.0, Culture=neutral, PublicKeyToken=b03f5f7f11d50a3a"
                                                                    BinaryLibrary id 3
0x0000b0  0c 04000000 4e "WindowsBase, Version=3.0.0.0, Culture=Neutral, PublicKeyToken=31bf3856ad364e35"
                                                                    BinaryLibrary id 4 (ObservableCollection type-forward)
0x000104  05 01000000 11 "UsbMonitorL.Theme" 14000000 <20 member names> <20 BinaryTypeEnum> <additional infos> 02000000
                                                                    ClassWithMembersAndTypes, object 1, 20 members (21 in v3 files: + FrameRate)
0x00044d  05 fbffffff 14 "System.Drawing.Color" (4 members) ... 03000000
                                                                    frontColor value (struct inline, object id -5)
0x000492  0a ...                                                    Color.name = null, then value (Int64), knownColor (Int16), state (Int16) inline
0x00049f  01 faffffff fbffffff                                      ClassWithId: backColor (object -6) reuses the metadata of -5
...       06 <id> LPS "Simple black and white theme"                BinaryObjectString = Theme.name
...       06 <id> LPS "<absolute path on the author's PC>"          Theme.themePath
...       09 <idRef>                                                MemberReference (themePic -> Bitmap defined later)
...       0a 0a 0a 0a                                               videoPath, o_videoPath, videoTargetPath, videoName = null
0x000555  05 ... "System.Drawing.Bitmap" 1 member "Data" type 7 (PrimitiveArray) add 2 (Byte) lib 3
0x000584  05 ... "System.Collections.ObjectModel.ObservableCollection`1[[UsbMonitorL.GraphItem, UsbMonitorL, Version=1.1.0.0, ...]]"
                                                                    members _monitor, "Collection`1+items", lib 4
0x000772  0f 0b000000 f03d0100 02 89504e47...                       ArraySinglePrimitive, object 11, 81,392 x Byte = PNG (themePic)
0x01456c  05 0c000000 ... ObservableCollection`1+SimpleMonitor[...] { _busyCount: Int32 }
0x01462c  04 ... "System.Collections.Generic.List`1[[UsbMonitorL.GraphItem, ...]]" { _items, _size, _version }
                                                                    SystemClassWithMembersAndTypes (mscorlib, no library record)
0x0146f8  07 0e000000 00 01000000 20000000 04 "UsbMonitorL.GraphItem" 02000000
                                                                    BinaryArray (single, rank 1, capacity 32) of class GraphItem
0x014721  09 0f000000 09 10000000 ... (x 30)                        one MemberReference per element (FORWARD references)
0x0147b7  0d 02                                                     ObjectNullMultiple256: 2 unused capacity slots of _items[]
0x0147b9  05 0f000000 16 "UsbMonitorL.GraphImage" ...               the element objects follow (object 15 first)
0x03294a  0b                                                        MessageEnd (207,179 bytes of NRBF); zeros follow up to 0x515b0 (333,232 bytes)
```

The `0f` record at 0x000772 is the theme preview (`themePic`): object id `0b 00 00 00` (11), length LE32
`f0 3d 01 00` (81,392), primitive type `02` (Byte), then the PNG file. The member names in the class records are the
backing-field names, for example `<setColor>k__BackingField`. Offsets checked against the file (static).

## 3. NRBF encoding rules Bezel needs

- Little-endian throughout. `LengthPrefixedString` = 7-bit varint length + UTF-8 bytes.
- Members whose BinaryTypeEnum is Primitive are written **inline without a record header**. Any other member is a full
  record: `06` string, `09` reference, `0a` null, `0d`/`0e` null runs, `01`/`04`/`05` objects, `07`/`0f`/`10`/`11`
  arrays.
- **Forward references are normal**: the item array holds `09` references to objects serialized later. Resolve in
  two passes or lazily.
- Object identity is preserved: `TextAlignment` objects are shared by many elements (220 of 347 alignments in the stock
  themes are references), and the two bitmaps of a `GraphClock` are often the same object.
- Class metadata is emitted once per class; later objects of the same class use `ClassWithId` (`01`).
- `Queue<string>` lives in library `System, Version=4.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089` (its
  library id is arbitrary per file); `List<T>` is a system class with no library record.
- A robust reader keys on **member names**, not positions, and defaults anything missing.

## 4. Schema versions seen

| Files | `UsbMonitorL` version in the stream | Differences |
|---|---|---|
| 20 stock themes (2023) | 1.1.0.0 | no `Theme.FrameRate`; `GraphItem` lacks `useGradient`/`revert`/`fahrenheit`; `FontConfig` lacks `GrColor`/`GrDirection`; `GraphArchBar` lacks `revert`/`round`/`totalAngel`/`useGradient` |
| dashboard theme, one restore copy | 1.1.1.0 | intermediate |
| themes re-saved by V3.07 | 3.1.1.1 | current layout including `FrameRate` |

## 5. Class schemas (current V3.07 layout)

Member names are the NRBF names; auto-property backing fields `<x>k__BackingField` are shortened to `x`. Base-class
members appear in the stream prefixed `GraphItem+` / `GraphImage+` and come **after** the derived class's own members.

### 5.1 `UsbMonitorL.Theme`

| # | Member | Type | Meaning |
|---|---|---|---|
| 0 | `setColor` | bool | legacy global recolour flag; always false |
| 1 | `frontColor` | Color | always Empty |
| 2 | `backColor` | Color | always Empty |
| 3 | `isVisualTheme` | bool | true only for in-app dashboard panels (hides the editor's clock pivot marker) |
| 4 | `isTempTheme` | bool | a working copy living in `theme_temp\` |
| 5 | `isAidaTheme` | bool | never read (legacy AIDA64 import) |
| 6 | `isAidaTransparent` | bool | never read |
| 7 | `aidaLoadMark` | string | never read |
| 8 | `reload` | bool | runtime "force re-send" flag; set after an editor save |
| 9 | `name` | string | theme name (= file name without extension) |
| 10 | `isLanscape` | bool | (sic) true = landscape |
| 11 | `width` | Int32 | canvas width in px (1920 for 8.8" landscape, 480 for portrait; 870 for the dashboard) |
| 12 | `height` | Int32 | canvas height |
| 13 | `themePath` | string | absolute path on the **author's** PC; overwritten on load |
| 14 | `themePic` | Bitmap | full-canvas preview PNG rendered at save time (theme list thumbnail) |
| 15 | `videoPath` | string | absolute PC path of the background video; relocated on install to `<dir>\<fileName>` |
| 16 | `o_videoPath` | string | original (un-rotated) video path |
| 17 | `videoTargetPath` | string | path on the device, for example `/mnt/UDISK/video/<name>.mp4` or `/mnt/SDCARD/video/<name>.mp4` |
| 18 | `videoName` | string | video file name (may carry a `_90`/`_180`/`_270` suffix) |
| 19 | `FrameRate` | Int32 | (v3 only) video fps probed on import; 20 when unknown |
| 20 | `GraphList` | `ObservableCollection<GraphItem>` | z-ordered element list; **index 0 must be the background** (image or video), enforced by the editor at save |

`ObservableCollection<GraphItem>` = `{ _monitor: SimpleMonitor { _busyCount: Int32 }, "Collection`1+items":
List<GraphItem> { _items: GraphItem[] (capacity-sized, null-padded), _size: Int32, _version: Int32 } }`. Use
`_size` elements of `_items`.

### 5.2 `UsbMonitorL.GraphItem` (base of every element; used directly for `Text` and `Data`)

| Member | Type | Meaning |
|---|---|---|
| `AcceptDataList` | List<string> | data names the element may bind to (editor filter only) |
| `hide` | bool | element hidden (not drawn on the device nor in the editor) |
| `useGradient` | bool | unused by the text renderer (text gradients are driven by `FontConfig.GrDirection`) |
| `enabled` | bool | always false, unused |
| `TypeName` | string | **element type discriminator**: `Text`, `Data`, `Image`, `Animation`, `StatuBar`, `ArchBar`, `Clock`, `Chart` |
| `SubTypeName` | string | always null |
| `_DisplayName` | string | layer-list label, localised at creation (for example `数据--Cpu利用率`: type label + `--` + data display name [+ `_` + SubName]) |
| `posX`, `posY` | Int32 | anchor in canvas pixels (text: the string-format anchor point; Clock: needle offset from the pivot) |
| `revert` | bool | invert the value (`Rate := 1 - Rate`) for bars |
| `fahrenheit` | bool | per-element °F for temperatures |
| `m_data` | M_Data | data binding (null for Image and Animation) |
| `fontConfig` | FontConfig | null for non-text types |

Defaults: `Text` -> `AcceptDataList = ["StaticText"]`; `Data` -> the 42 data names of [sensors.md](sensors.md)
section 3; position (200, 100). The editor adds Text/Data at (120, 100), centred.

### 5.3 `UsbMonitorL.M_Data` (binding and last value)

| Member | Type | Meaning |
|---|---|---|
| `content` | string | always null |
| `DataQueue` | Queue<string> | history samples for `Chart` (stale values from the author's PC are saved in the file) |
| `queueLen` | Int32 | history length = chart width / column width |
| `ShowUnit` | bool | append the unit |
| `DataName` | string | **sensor key** ([sensors.md](sensors.md) section 3); `StaticText` for fixed text |
| `Sanma_Eng_Name` | string | English label |
| `SubName` | string | qualifier (section 5.9) |
| `b_DataName` | string | always null |
| `DisplayName` | string | Chinese label (for example `Cpu利用率`) |
| `Rate` | double | **normalised value 0..1** used by bars, arcs and needles |
| `ValueWithUnit` | string | **the string actually drawn** by Text/Data elements (`88%`, `4800M`, `888KB/s`, `12:00`) |
| `_Value` | string | raw value, or the static text content |
| `_IsEnabled` | bool | always true |

### 5.4 `UsbMonitorL.FontConfig`

| Member | Type | Default | Meaning |
|---|---|---|---|
| `isBold` | bool | false | bold style |
| `name` | string | `Microsoft YaHei` | font family, matched against the **zh-CN localised** family name |
| `size` | Int32 | 24 | GDI+ `Font(family, size, style)`: unit = **points** (at 96 dpi, px = size x 96 / 72). Editor range 10..200 |
| `interval` | float | 0 | extra letter spacing in px; non-zero switches to per-glyph drawing |
| `color` | Color | ARGB(255, 128, 128, 128) | text colour |
| `GrColor` | Color | ARGB(255, 128, 128, 128) | second gradient colour |
| `GrDirection` | Int32 | 0 | 0 none, 1 left->right, 2 top->bottom, 3 top-left->bottom-right, 4 top-right->bottom-left; used as `LinearGradientMode(GrDirection - 1)` |
| `alignment` | TextAlignment | Left | `{ displayName: "左"/"中"/"右", index: 0 Left / 1 Center / 2 Right }` |

### 5.5 `System.Drawing.Color` (library System.Drawing)

Members: `name` (String, null in practice), `value` (Int64, ARGB in the low 32 bits), `knownColor` (Int16, .NET
`KnownColor` enum), `state` (Int16).

| `state` | Meaning | Colour to use |
|---|---|---|
| 0 | `Color.Empty` | transparent / "no colour" |
| 1 | known colour | `KnownColor` table lookup of `knownColor` |
| 2 | ARGB valid | `value` |
| 8 | name valid | named colour |

All of 0, 1 and 2 occur in stock themes (for example 138 text colours are `knownColor = 164, state = 1` = White).
Known values seen: 35 Black, 66 DeepPink, 95 LightGray, 140 Purple, 141 Red, 151 SkyBlue, 164 White. Bezel embeds the
public .NET `KnownColor` table.

### 5.6 `System.Drawing.Bitmap`, `System.Drawing.Rectangle`, `UsbMonitorL.TransFormInfo`

- `Bitmap`: single member `Data: byte[]` -> ArraySinglePrimitive(Byte) holding a **PNG file** (all 164 embedded
  bitmaps in the stock themes are 32-bpp ARGB PNGs at 96 dpi).
- `Rectangle`: `x`, `y`, `width`, `height` (Int32).
- `TransFormInfo`: `ret` (Int32; 1 = apply a crop/scale/rotate transform when transcoding the video, 0 = use as is),
  `rotate` (Int32, quarter turns 0..3), `rect` (Rectangle, crop in **source-video pixels**).

### 5.7 Element classes

| `TypeName` (UI label) | Class | Own members (defaults) | Binds to |
|---|---|---|---|
| `Text` (Text) | GraphItem | - | `StaticText` |
| `Data` (Data) | GraphItem | - | 42 data names |
| `Image` (Image) | GraphImage | `step` = 0.02, `zoom_rate` = 1.0, `bitmap` (scaled), `O_bitmap` (original), `ImgName` (source file name only) | nothing |
| `Animation` (video) | GraphAnimation | `step` = 0.02, `zoom_rate` = 1.0, `midLevel` = 56, `bitmap` (first-frame poster, scaled), `O_bitmap`, `S_bitmap` (pristine poster), `videoName`, `crop: TransFormInfo`, `direction` (rotation 0..3), `FilePath`, `potritMode`, `ration` = 1, `SWith`/`SHeight` (source video size) | nothing |
| `StatuBar` (Status bar = progress bar) | GraphStatuBar | `direction` = 0, `trBack`, `useGradient` = false, `fillBack`, `lineWidth` = 1, `width` = 200, `height` = 30, `radius` = 0, `FrontColor` = Purple, `BackColor` = Black, `GradientColor` = DeepPink, `useSubsection` = true | 20 data names |
| `ArchBar` (Curved bar = ring gauge) | GraphArchBar | `useBlock` = true, `revert`, `round`, `trBack`, `fillBack`, `archWidth` = 20, `lineWidth` = 1, `diameter` = 200, `height` = 30 (unused), `startPer` = 0, `totalAngel` = 360, `FrontColor` = Purple, `BackColor` = LightGray, `GradientColor` = DeepPink; base `useGradient` = true | 20 data names |
| `Clock` (Clock = rotating needle) | GraphClock : GraphImage | `centerX` = 240, `centerY` = 240, `o_Y`, `o_X` (unused), `angle` = 0 (start angle), `endAngle` = 360 (sweep), `moveOpoint`; runtime/newer: `lastRate`, `offset`, `revert`, `tmp` | 23 data names including DATE, DAY, TIME |
| `Chart` (Chart = scrolling area graph) | GraphLine | `LineColor` = SkyBlue, `FillColor` = ARGB(40, SkyBlue), `BorderColor` = SkyBlue, `lineWidth` = 1, `rollDirection`, `_width` = 300, `maxValue` = 100, `_height` = 200, `borderWidth` = 1, `columnWidth` = 5, `coefficient` = height / maxValue | 35 data names |

### 5.8 Rendering summary (exact algorithms in [rendering.md](rendering.md) section 3)

- Canvas: a `theme.width x theme.height` bitmap at 96 dpi, high-quality smoothing and bicubic interpolation,
  anti-aliased text. Elements are drawn in `GraphList` order; hidden elements are skipped.
- Text/Data: draw `m_data.ValueWithUnit` at `(posX, posY)`; `posX` is the left edge, centre or right edge per the
  alignment; optional gradient and letter spacing.
- Image: drawn 1:1 at `(posX, posY)` (the scaled `bitmap`).
- Animation: in the editor, the poster at `(posX, posY)`; on the device, the real video. When a background video
  exists the PC render omits item 0 and produces only the overlay.
- StatuBar: fill length `(int)(width * Rate)` in one of four directions, optional segments, rounded corners, gradient.
- ArchBar: arc in a `diameter` square starting at `startPer% * 360 - 90` degrees, sweep `totalAngel * Rate`.
- Clock: needle image rotated about `(centerX, centerY)` by `angle + (Rate - offset) * endAngle`.
- Chart: area graph of the history samples spaced `columnWidth` px.

### 5.9 `SubName` vocabulary

| DataName | SubName values |
|---|---|
| `DRVLOAD` | drive letter `C` .. `Z` |
| `HDDTEMP`, `HDDUSED` | disk index `0` .. `10` |
| `TIME` | `h:m:s`, `h:m`, `m`, `s`, `h_24`, `h_12` (editor list); the renderer also accepts `hm`, `HHmm`, `hh`, `HH`, `mm`, `ss` |
| `DAY` | `Day_cn`, `Num_cn`, `Day_en`, `Num` |
| `DATE` | `Y-M-D`, `Y`, `M`, `M_en`, `M_cn`, `D` (editor list); the renderer also accepts `yyyy-MM-dd`, `yyyy`, `MM`, `dd` |

The two vocabularies come from the editor's sub-item lists and the renderer's format switch respectively; mapping
between them is not fully established (static).

## 6. Resolution keys

The theme folder name is the **resolution key** `<W><H>` of the native panel (for example `4801920` = 480 x 1920).
The app scans `theme\*` sub-folders and maps each name to a device class. Known keys (in match order):

| Key | Label / size |
|---|---|
| `480800` | 5.0" |
| `480480r` | 480 x 480 round (2.1" / 2.8") |
| `480480s` | 480 x 480 square (3.4") |
| `720720` | 720 x 720 (4.0") |
| `7201472`, `7201568` | 6.5" |
| `400400` | 4.0" |
| `240320` | 240 x 320 |
| `4801920` | 480 x 1920 (8.8") |
| `8001280`, `10802320`, `10802224`, `368960`, `4481280`, `640480`, `180640`, `480272`, `7201280`, `800480` | same as the key |
| `320320` | 320 x 320 |
| `240320V` | a second 240 x 320 branch with a vertical directory (unreachable in this build) |

The canvas size is the device resolution in the chosen orientation: landscape uses the device's W x H with
`isLanscape = true`, portrait the swapped size with `isLanscape = false`. On the 8.8": landscape 1920 x 480 (19 stock
themes), portrait 480 x 1920 (7 "Vertical" themes). The composed frame is rotated to the panel's native orientation
according to the theme orientation and the device rotation setting (0..3).

## 7. Fonts

- On load, each element's `fontConfig.name` is looked up first in a private font collection loaded from the app's
  `fonts\` folder at startup, then among installed system fonts, comparing the **zh-CN localised** family name.
- A missing font marks the element `uninstalled`, renders it with Microsoft YaHei, and lists it in a "fonts not found"
  message that tells the user to install fonts from the `fonts\` folder or the vendor site.
- Fonts are **not embedded** in `.turtheme` files (shop packages carry them separately). The vendor's `fonts\` folder
  holds about 120-150 files (commercial and free families); Bezel ships none of them.
- The editor's font list shows only system-installed families.
- Bezel resolves families by all localised names (name table), then falls back to a configurable default and reports
  the substitution.

## 8. Folder layout of the vendor app

| Path | Purpose |
|---|---|
| `theme\<res>\*.turtheme` | installed themes; **every file is fully deserialized at startup** just to get `themePic` for the list (label `"<index>.<name>"`) |
| `restore\<res>\*.turtheme` | factory copies used by "Reset"; the editor saves every theme to both `theme\<res>` and `restore\<res>` |
| `video\<res>\*.mp4` | local background videos (+ rotated `_90`/`_180`/`_270` variants) |
| `themevertical\`, `restorevertical\` | vertical variants, used only by the `240320V` branch |
| `theme_temp\` | timestamped working copies `<name><yyyyMMddhhmmss>.turtheme` with `isTempTheme = true`; also listed as previews |
| `visual\panel_common.turtheme` | 870 x 660 in-app dashboard (other OEM builds use other dashboard theme names) |
| `fonts\` | private fonts loaded at startup |
| `ImageCache\`, `UploadTemp\`, `DownloadTemp\` | theme-shop thumbnails and staging |
| `temp\` | initial folder of the "Load theme" dialog when no device is connected |
| `config\config_<device id>.data`, `AppConfig.data` | settings ([runtime-artifacts.md](runtime-artifacts.md) section 3) |
| `fw\`, `Driver\`, `FFMpeg\`, `ffmpeg.exe`, `code.ini` | firmware, desktop-mode display driver, codecs, sensor-engine init option |

## 9. Theme-shop packages

The vendor's online theme shop distributes **password-protected zip files** containing `<themeName>@<userName>.turtheme`,
the background video, and a `fonts\` folder with the fonts the theme needs; file names inside use the GBK encoding.
Installing one copies the theme to `theme\<res>\` and `restore\<res>\`, videos to `video\<res>\`, and fonts to the app
fonts. **Bezel does not handle shop packages.** It imports plain `.turtheme` files (plus separately supplied videos and
fonts).

## 10. Privacy and import hygiene

- `themePath` and `videoPath` contain absolute paths from the author's PC (user names, drive letters, folder names).
- `M_Data.DataQueue` carries stale sensor samples from the author's PC.
- `themePic` is a rendered preview that may show the author's live sensor values.
- Bezel's importer drops the path fields, clears histories, and regenerates previews; its exporter never writes them.

## 11. Mapping notes for the Bezel importer

- Build the element list from `GraphList` in order; keep `hide`.
- Colours: apply the `state` rules of section 5.5; `Color.Empty` means "not set".
- Font size is in points: convert with `px = size * 96 / 72` before handing it to a pixel-size rasteriser.
- Background: index 0 is an `Image` or an `Animation`; an `Animation` also references a video file that must be
  supplied separately (by `videoName`).
- Bindings: map `DataName` (+ `SubName`) to Bezel sensor ids through the alias table in [sensors.md](sensors.md).
