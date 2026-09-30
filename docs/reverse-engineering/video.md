# Video

How each family shows moving pictures, what the reference apps produce with ffmpeg, and where files live on the
devices. Confidence: **static** throughout (no capture or hardware test yet).

## 1. On-device playback versus PC streaming

| Family | Python reference | Vendor app (TURZX V3.07) |
|---|---|---|
| rev A, rev B, rev D, WeAct | no video | not covered by the analysed parts (pending) |
| rev C (serial, Linux SoC) | no video | **on-device playback**: the background video is transcoded to MP4 (H.264), stored on the device (internal flash or TF card) and played by the device; the PC sends only the theme overlay plus an opacity list (POSLEN, [pixel-formats.md](pixel-formats.md) section 8). A device "start mode" can boot straight into a stored video or image. |
| TUR_USB (0x1CBE) | **streaming to the device decoder**: MP4 -> Annex-B `.h264`, sent in chunks with command 121 ([protocol-turing-usb.md](protocol-turing-usb.md) section 7); also upload of `.h264` / `.png` files and play commands 98 / 110 / 113 | Annex-B `.h264` with no B-frames (section 3) |
| WCH (0x43A8) | not supported | **PC-side decode**: FFmpeg decodes on the PC, every video frame is composited with the theme overlay and sent as a full raw BGR frame at the source frame rate ([protocol-wch.md](protocol-wch.md) section 9) |

The vendor app never live-encodes the desktop or the rendered theme to H.264: an in-process x264 encoder
(YUV420P, 2.4 Mbit/s, zerolatency) is present but never instantiated. Live content is always sent as images.

## 2. Python: MP4 to Annex-B (`lcd_comm_turing_usb.py:657-681`, `400-432`)

Output: `<input stem>.h264` next to the input; skipped when that file already exists.

With ffmpeg on `PATH` (argument vector, no shell):

```
ffmpeg -y -i <input.mp4> -c:v copy -bsf:v h264_mp4toannexb -an -f h264 <output.h264>
```

Without ffmpeg, a built-in MP4 parser: pick the H.264 track, read SPS/PPS from `avcC`, write
`00 00 00 01 SPS...` `00 00 00 01 PPS...` first, then for each sample convert the length-prefixed NAL units
(`nal_len_size` 1..4 bytes, big-endian) to `00 00 00 01`-prefixed units, **repeating SPS/PPS before every sync sample**.
No re-encoding: resolution, frame rate and profile of the source are kept, so the source must already match the panel.

## 3. Vendor app: ffmpeg command lines

The vendor app runs a bundled `ffmpeg.exe` with a single argument string. Strings below are exact, including the
double spaces (they are harmless). `<src>`, `<dst>` are quoted paths; `W:H` is the panel resolution in its native
orientation; `<rot>` and `<crop>` are filter prefixes (section 4).

MP4 (H.264) — used for serial (Linux SoC) panels, which store and play the file, and for WCH panels, whose app decodes
it on the PC:

```
-i "<src>"  -vf <rot><crop>scale=W:H,setsar=1:1 -c:v "libx264" -crf 20  -an  -f mp4 "<dst>" -y
```

MP4 with fixed 24 fps (when requested by the caller):

```
-i "<src>"  -vf <rot><crop>scale=W:H,setsar=1:1 -c:v "libx264" -crf 20  -r 24 -an  -pix_fmt yuv420p -f mp4 "<dst>" -y
```

Raw Annex-B `.h264` for the 0x1CBE family, variant A (`[ -r 30]` is present only when the device reports a
capability flag):

```
-i "<src>"  -vf <rot><crop>scale=W:H,setsar=1:1,eq=brightness=-0.1:contrast=0.9:saturation=1  -c:v "libx264" -x264opts bframes=0 -crf 20 [ -r 30] -an  -pix_fmt yuv420p -f h264 "<dst>" -y
```

Variant B (selected by a flag):

```
-i "<src>"  -vf <rot><crop>scale=W:H,setsar=1:1,eq=brightness=-0.1:contrast=0.9:saturation=1  -c:v "libx264" -x264opts bframes=0   -threads 0 -preset faster  -crf 20 [ -r 30]  -an  -f  h264 "<dst>" -y
```

Quick MP4 -> `.h264` next to the source, for the 0x1CBE family:

```
-i "<src>" -vf "eq=brightness=-0.1:contrast=0.9:saturation=1"  -vcodec libx264 -x264opts bframes=0 [ -r 30] -threads 4 -preset ultrafast  "<src>.h264"
```

Summary: serial panels get **MP4**, H.264, audio stripped, exact panel resolution in native orientation, yuv420p and
24 fps when requested. 0x1CBE panels get **raw Annex-B H.264** with **no B-frames**, slightly darkened
(`eq=brightness=-0.1:contrast=0.9`). Progress is parsed from ffmpeg's stderr lines containing `Duration` and `time=`;
the user can cancel.

Bezel builds ffmpeg arguments as a vector (never a shell string) and keeps these filter chains as the defaults.

## 4. Rotation, crop and naming

The crop/rotate step comes from the video adjust dialog (a transform record with `rotate` 0..3, a crop rectangle in
source-video pixels, and a flag that says whether to apply them):

| rotate | `-vf` prefix |
|---|---|
| 0 | (none) |
| 1 | `transpose=1,` (90° clockwise) |
| 2 | `transpose=1,transpose=1,` (180°) |
| 3 | `transpose=2,` (90° counter-clockwise) |

`crop=W:H:X:Y,` follows the rotation prefix when the crop flag is set; then `scale=<panelW>:<panelH>`.

- Rotated copies are named with a `_90`, `_180` or `_270` suffix. A theme records the file name (which may carry the
  suffix), the original un-rotated path, the local path and the device path.
- The editor refuses a crop that leaves blank areas ("Screen has to be filled, no blank space").
- The theme's `FrameRate` (theme format v3) is probed on import; 20 when unknown.
- A helper considers a file suitable for 24 fps when the Windows shell `FrameRate` property contains "24" or parses to
  at most 25 fps. Other import constraints surfaced in the UI: exact resolution (`ErrResolution:WxH` for WCH),
  file names limited to ASCII letters, digits, `-`, `_` and `.`, at most 120 MB per upload, a 4 GB software ceiling.

## 5. Storage paths known so far

| Where | Path | Source |
|---|---|---|
| TUR_USB device, images | `/tmp/sdcard/mmcblk0p1/img/<name>.png` | Python `upload_file` |
| TUR_USB device, videos | `/tmp/sdcard/mmcblk0p1/video/<name>.h264` | Python `upload_file` |
| serial (Linux SoC) device, internal flash | `/mnt/UDISK/video/<name>.mp4` | `videoTargetPath` values in vendor themes |
| serial device, TF card | `/mnt/SDCARD/video/<name>.mp4` | `videoTargetPath` values in vendor themes |
| serial device, settings file | `/usr/data/app.cfg` (the vendor app deletes it in one flow; purpose unknown) | vendor app |
| vendor device page | four locations: internal Video, internal Image, SD Card Video, SD Card Image (internal ones hidden for the 0x1CBE family) | vendor UI |
| PC, vendor app | `video\<res>\` (background videos per resolution key, plus rotated variants) | vendor folder layout ([themes-turzx.md](themes-turzx.md) section 8) |

TF cards must hold a single primary FAT32 partition spanning the card (vendor help text); Bezel can check this and
explain it inline.

## 6. Guidance for Bezel

- Prefer the family's native path: store-and-play on rev C, stream H.264 on TUR_USB, PC decode on WCH.
- Transcode with the vendor filter chains by default so that colour and timing match what users know, and expose the
  darkening (`eq=`) as an option rather than hard-coding it.
- Validate resolution, frame rate, size and file-name constraints **before** transcoding and uploading, and report them
  inline.
- H.264 Annex-B for 0x1CBE must not contain B-frames (vendor default `-x264opts bframes=0`); a stream copy from an MP4
  that has B-frames is untested.
