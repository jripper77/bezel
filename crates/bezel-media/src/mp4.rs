//! A minimal reader of ISO base media files (MP4, and MOV which shares the
//! box layout): only the boxes that say whether a video is already in a
//! screen's profile — brand, video codec, picture size, pixel format (from
//! the H.264 sequence parameter set), B-frames, frame rate, duration and the
//! presence of audio. Sample data (`mdat`) is skipped, never read.
//!
//! The decision itself (in profile or not) is the core's
//! `UploadProfile::mismatches`; this module only reports what the file is.

use std::fmt;
use std::io::{self, Read, Seek, SeekFrom};
use std::time::Duration;

use bezel_core::domain::geometry::Size;
use bezel_core::domain::media::{
    FrameRate, MediaFormat, MediaInfo, VideoCodec, VideoPixelFormat, VideoTrack,
};

/// Largest `moov` box read into memory (real ones are a few hundred KiB).
const MOOV_LIMIT: u64 = 64 * 1024 * 1024;
/// Largest `ftyp` box read into memory.
const FTYP_LIMIT: u64 = 4096;
/// Box types an ISO base media file (or a QuickTime file without `ftyp`)
/// starts with.
const LEADING_BOXES: [&[u8; 4]; 7] = [
    b"ftyp", b"moov", b"mdat", b"free", b"skip", b"wide", b"pnot",
];
/// Major brands of ISO files that are not MP4 videos: QuickTime movies and
/// the HEIF/AVIF image family.
const NOT_MP4_BRANDS: [&[u8; 4]; 8] = [
    b"qt  ", b"avif", b"avis", b"heic", b"heix", b"hevc", b"mif1", b"msf1",
];
/// H.264 profiles whose SPS carries the chroma format and bit depths.
const HIGH_PROFILES: [u32; 13] = [100, 110, 122, 244, 44, 83, 86, 118, 128, 138, 139, 134, 135];
/// Size of the fixed part of a visual sample entry (ISO 14496-12 12.1.3).
const VISUAL_ENTRY_FIXED: usize = 78;

type FourCc = [u8; 4];

/// Why a file could not be read as an ISO base media file.
#[derive(Debug)]
pub(crate) enum Mp4Error {
    /// The file could not be read.
    Io(io::Error),
    /// The boxes are not laid out as the format says.
    Malformed(&'static str),
}

impl fmt::Display for Mp4Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Mp4Error::Io(e) => write!(f, "{e}"),
            Mp4Error::Malformed(why) => f.write_str(why),
        }
    }
}

impl From<io::Error> for Mp4Error {
    fn from(e: io::Error) -> Self {
        Mp4Error::Io(e)
    }
}

type Parsed<T> = Result<T, Mp4Error>;

/// The payloads of the `ftyp` and `moov` boxes, when present.
type TopLevel = (Option<Vec<u8>>, Option<Vec<u8>>);

/// True when `head` (the first bytes of a file) starts like an ISO base
/// media file.
pub(crate) fn sniff(head: &[u8]) -> bool {
    head.get(4..8)
        .is_some_and(|kind| LEADING_BOXES.iter().any(|t| kind == t.as_slice()))
}

/// What the ISO base media file in `reader` (`bytes` long) is. MP4 files
/// report [`MediaFormat::Mp4`]; QuickTime movies and HEIF/AVIF images
/// report [`MediaFormat::Other`] with whatever their tracks say.
pub(crate) fn probe<R: Read + Seek>(reader: &mut R, bytes: u64) -> Parsed<MediaInfo> {
    let (ftyp, moov) = top_level(reader, bytes)?;
    let moov = moov.ok_or(Mp4Error::Malformed("no moov box"))?;
    let mut video = None;
    let mut has_audio = false;
    for (kind, payload) in children(&moov)? {
        if &kind != b"trak" {
            continue;
        }
        match track(payload)? {
            Track::Video(found) if video.is_none() => video = Some(found),
            Track::Audio => has_audio = true,
            _ => {}
        }
    }
    Ok(MediaInfo {
        format: container(ftyp.as_deref()),
        bytes,
        dimensions: video.as_ref().and_then(|v| v.size),
        video: video.map(|v| v.track),
        has_audio,
    })
}

/// Reads the `ftyp` and `moov` payloads, seeking over everything else.
fn top_level<R: Read + Seek>(reader: &mut R, bytes: u64) -> Parsed<TopLevel> {
    let (mut ftyp, mut moov) = (None, None);
    let mut at = 0u64;
    while bytes.saturating_sub(at) >= 8 {
        reader.seek(SeekFrom::Start(at))?;
        let (kind, header, size) = read_header(reader, bytes - at)?;
        let payload = size - header;
        match &kind {
            b"ftyp" if ftyp.is_none() => {
                ftyp = Some(read_payload(reader, payload.min(FTYP_LIMIT))?)
            }
            b"moov" if moov.is_none() => {
                if size > bytes - at {
                    return Err(Mp4Error::Malformed("the moov box is cut short"));
                }
                if payload > MOOV_LIMIT {
                    return Err(Mp4Error::Malformed("the moov box is too large"));
                }
                moov = Some(read_payload(reader, payload)?);
            }
            _ => {}
        }
        at = at.saturating_add(size);
    }
    Ok((ftyp, moov))
}

/// Reads a box header: its type, header length and total size. A box that
/// claims to run past the end of the file is cut at the end (a partially
/// written `mdat`); `moov` is checked by the caller.
fn read_header<R: Read>(reader: &mut R, remaining: u64) -> Parsed<(FourCc, u64, u64)> {
    let mut head = [0u8; 8];
    reader.read_exact(&mut head)?;
    let size32 = u32::from_be_bytes([head[0], head[1], head[2], head[3]]);
    let kind = [head[4], head[5], head[6], head[7]];
    let (header, size) = match size32 {
        0 => (8, remaining),
        1 => {
            let mut large = [0u8; 8];
            reader.read_exact(&mut large)?;
            (16, u64::from_be_bytes(large))
        }
        n => (8, u64::from(n)),
    };
    if size < header {
        return Err(Mp4Error::Malformed("a box is smaller than its header"));
    }
    Ok((kind, header, size))
}

fn read_payload<R: Read>(reader: &mut R, len: u64) -> Parsed<Vec<u8>> {
    let mut out = Vec::new();
    reader.take(len).read_to_end(&mut out)?;
    Ok(out)
}

/// The boxes inside `data`, in order. Fewer than 8 trailing bytes (the
/// QuickTime terminator) are ignored.
fn children(mut data: &[u8]) -> Parsed<Vec<(FourCc, &[u8])>> {
    let mut out = Vec::new();
    while data.len() >= 8 {
        let size32 = be_u32(data, 0)?;
        let kind = [data[4], data[5], data[6], data[7]];
        let (header, size) = match size32 {
            0 => (8, data.len() as u64),
            1 => (16, be_u64(data, 8)?),
            n => (8, u64::from(n)),
        };
        if size < header as u64 || size > data.len() as u64 {
            return Err(Mp4Error::Malformed("a box does not fit its parent"));
        }
        let size = size as usize;
        out.push((kind, &data[header..size]));
        data = &data[size..];
    }
    Ok(out)
}

fn child<'a>(boxes: &[(FourCc, &'a [u8])], kind: &FourCc) -> Option<&'a [u8]> {
    boxes
        .iter()
        .find(|(k, _)| k == kind)
        .map(|(_, payload)| *payload)
}

fn be_u16(data: &[u8], at: usize) -> Parsed<u16> {
    data.get(at..at + 2)
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
        .ok_or(Mp4Error::Malformed("a box is cut short"))
}

fn be_u32(data: &[u8], at: usize) -> Parsed<u32> {
    data.get(at..at + 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or(Mp4Error::Malformed("a box is cut short"))
}

fn be_u64(data: &[u8], at: usize) -> Parsed<u64> {
    let hi = be_u32(data, at)?;
    let lo = be_u32(data, at + 4)?;
    Ok((u64::from(hi) << 32) | u64::from(lo))
}

/// [`MediaFormat::Mp4`] unless the major brand says QuickTime or a HEIF
/// image; a file without `ftyp` is an old QuickTime movie.
fn container(ftyp: Option<&[u8]>) -> MediaFormat {
    match ftyp.and_then(|f| f.get(0..4)) {
        Some(major) if !NOT_MP4_BRANDS.iter().any(|b| major == b.as_slice()) => MediaFormat::Mp4,
        _ => MediaFormat::Other,
    }
}

enum Track {
    Video(Video),
    Audio,
    Other,
}

struct Video {
    size: Option<Size>,
    track: VideoTrack,
}

fn track(trak: &[u8]) -> Parsed<Track> {
    let boxes = children(trak)?;
    let Some(mdia) = child(&boxes, b"mdia") else {
        return Ok(Track::Other);
    };
    let mdia = children(mdia)?;
    let handler = match child(&mdia, b"hdlr") {
        Some(hdlr) => be_u32(hdlr, 8)?.to_be_bytes(),
        None => return Ok(Track::Other),
    };
    match &handler {
        b"soun" => Ok(Track::Audio),
        b"vide" => video(&boxes, &mdia).map(Track::Video),
        _ => Ok(Track::Other),
    }
}

fn video(trak: &[(FourCc, &[u8])], mdia: &[(FourCc, &[u8])]) -> Parsed<Video> {
    let (timescale, declared) = match child(mdia, b"mdhd") {
        Some(mdhd) => media_header(mdhd)?,
        None => (0, None),
    };
    let stbl = match child(mdia, b"minf") {
        Some(minf) => children(minf)?
            .into_iter()
            .find(|(k, _)| k == b"stbl")
            .map(|(_, p)| children(p))
            .transpose()?
            .unwrap_or_default(),
        None => Vec::new(),
    };
    let entry = child(&stbl, b"stsd")
        .map(sample_entry)
        .transpose()?
        .flatten();
    let timing = child(&stbl, b"stts")
        .map(time_to_sample)
        .transpose()?
        .unwrap_or_default();
    let reordered = child(&stbl, b"ctts").map(reorders).transpose()?;
    let tkhd_size = child(trak, b"tkhd").map(track_size).transpose()?.flatten();
    let size = entry
        .as_ref()
        .map(|e| e.size)
        .filter(|s| s.area() > 0)
        .or(tkhd_size);
    let codec = entry.as_ref().map_or(VideoCodec::Other, |e| e.codec);
    let pixel_format = entry
        .as_ref()
        .and_then(|e| e.sps.as_deref())
        .and_then(sps_pixel_format);
    let samples = timing.samples();
    let ticks = declared.filter(|d| *d > 0).unwrap_or(timing.ticks());
    Ok(Video {
        size,
        track: VideoTrack {
            codec,
            pixel_format: if codec == VideoCodec::H264 {
                pixel_format
            } else {
                None
            },
            b_frames: (samples > 0).then(|| reordered.unwrap_or(false)),
            frame_rate: timing.frame_rate(timescale),
            duration: media_time(ticks, timescale),
        },
    })
}

/// `mdhd`: the media timescale and the declared duration (`None` when the
/// file says "unknown").
fn media_header(mdhd: &[u8]) -> Parsed<(u32, Option<u64>)> {
    if mdhd.first() == Some(&1) {
        let duration = be_u64(mdhd, 24)?;
        Ok((
            be_u32(mdhd, 20)?,
            (duration != u64::MAX).then_some(duration),
        ))
    } else {
        let duration = be_u32(mdhd, 16)?;
        Ok((
            be_u32(mdhd, 12)?,
            (duration != u32::MAX).then_some(u64::from(duration)),
        ))
    }
}

/// `tkhd`: the presentation size (16.16 fixed point).
fn track_size(tkhd: &[u8]) -> Parsed<Option<Size>> {
    let at = if tkhd.first() == Some(&1) { 88 } else { 76 };
    let width = be_u32(tkhd, at)? >> 16;
    let height = be_u32(tkhd, at + 4)? >> 16;
    Ok((width > 0 && height > 0).then_some(Size::new(width, height)))
}

struct SampleEntry {
    codec: VideoCodec,
    size: Size,
    sps: Option<Vec<u8>>,
}

/// The first entry of `stsd`: codec, coded size and, for H.264, the first
/// sequence parameter set of its `avcC`.
fn sample_entry(stsd: &[u8]) -> Parsed<Option<SampleEntry>> {
    let entries = children(stsd.get(8..).unwrap_or_default())?;
    let Some((kind, entry)) = entries.first() else {
        return Ok(None);
    };
    let size = Size::new(u32::from(be_u16(entry, 24)?), u32::from(be_u16(entry, 26)?));
    let codec = match kind {
        b"avc1" | b"avc3" => VideoCodec::H264,
        _ => VideoCodec::Other,
    };
    let sps = match codec {
        VideoCodec::H264 => {
            let inner = children(entry.get(VISUAL_ENTRY_FIXED..).unwrap_or_default())?;
            child(&inner, b"avcC").and_then(first_sps)
        }
        VideoCodec::Other => None,
    };
    Ok(Some(SampleEntry { codec, size, sps }))
}

/// The first SPS of an `AVCDecoderConfigurationRecord`.
fn first_sps(avcc: &[u8]) -> Option<Vec<u8>> {
    let count = avcc.get(5)? & 0x1F;
    if count == 0 {
        return None;
    }
    let len = usize::from(be_u16(avcc, 6).ok()?);
    avcc.get(8..8 + len).map(<[u8]>::to_vec)
}

/// Sample counts and durations from `stts`.
#[derive(Default)]
struct Timing(Vec<(u64, u64)>);

impl Timing {
    fn samples(&self) -> u64 {
        self.0.iter().map(|(count, _)| count).sum()
    }

    fn ticks(&self) -> u64 {
        self.0
            .iter()
            .map(|(count, delta)| count.saturating_mul(*delta))
            .sum()
    }

    /// The rate of the most common sample duration (what players call the
    /// real frame rate; a different last sample does not change it).
    fn frame_rate(&self, timescale: u32) -> Option<FrameRate> {
        let (_, delta) = self
            .0
            .iter()
            .filter(|(count, delta)| *count > 0 && *delta > 0)
            .max_by_key(|(count, _)| *count)?;
        ratio(u64::from(timescale), *delta)
    }
}

fn time_to_sample(stts: &[u8]) -> Parsed<Timing> {
    let count = be_u32(stts, 4)? as usize;
    let mut entries = Vec::with_capacity(count.min(stts.len() / 8));
    for i in 0..count {
        let at = 8 + i * 8;
        entries.push((
            u64::from(be_u32(stts, at)?),
            u64::from(be_u32(stts, at + 4)?),
        ));
    }
    Ok(Timing(entries))
}

/// True when the composition offsets of `ctts` differ between samples: the
/// display order is not the decode order, so the stream has B-frames. A
/// constant offset only delays every frame alike.
fn reorders(ctts: &[u8]) -> Parsed<bool> {
    let count = be_u32(ctts, 4)? as usize;
    let mut first = None;
    for i in 0..count {
        let at = 8 + i * 8;
        if be_u32(ctts, at)? == 0 {
            continue;
        }
        let offset = be_u32(ctts, at + 4)?;
        match first {
            None => first = Some(offset),
            Some(seen) if seen != offset => return Ok(true),
            Some(_) => {}
        }
    }
    Ok(false)
}

/// `num / den` reduced; a ratio whose terms do not fit in 32 bits is
/// approximated in thousandths.
fn ratio(num: u64, den: u64) -> Option<FrameRate> {
    if den == 0 {
        return None;
    }
    let divisor = gcd(num, den);
    let (num, den) = (num / divisor, den / divisor);
    match (u32::try_from(num), u32::try_from(den)) {
        (Ok(num), Ok(den)) => FrameRate::new(num, den),
        _ => {
            let milli = u128::from(num) * 1000 / u128::from(den);
            FrameRate::new(u32::try_from(milli).ok()?, 1000)
        }
    }
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// `ticks` of a `timescale` clock as a duration; `None` when either is 0.
fn media_time(ticks: u64, timescale: u32) -> Option<Duration> {
    if ticks == 0 || timescale == 0 {
        return None;
    }
    let nanos = u128::from(ticks) * 1_000_000_000 / u128::from(timescale);
    Some(Duration::from_nanos(
        u64::try_from(nanos).unwrap_or(u64::MAX),
    ))
}

/// The pixel format an H.264 sequence parameter set (a NAL unit with its
/// header byte) declares: 4:2:0 at 8 bits is yuv420p. Profiles below High
/// are always yuv420p. `None` when the SPS cannot be read.
pub(crate) fn sps_pixel_format(nal: &[u8]) -> Option<VideoPixelFormat> {
    let rbsp = unescape(nal);
    let mut bits = Bits::new(&rbsp);
    if bits.read(8)? & 0x1F != 7 {
        return None;
    }
    let profile = bits.read(8)?;
    bits.read(16)?; // constraint flags and level
    bits.ue()?; // seq_parameter_set_id
    if !HIGH_PROFILES.contains(&profile) {
        return Some(VideoPixelFormat::Yuv420p);
    }
    let chroma_format = bits.ue()?;
    if chroma_format == 3 {
        bits.read(1)?; // separate_colour_plane_flag
    }
    let luma_depth = bits.ue()?;
    let chroma_depth = bits.ue()?;
    Some(
        if chroma_format == 1 && luma_depth == 0 && chroma_depth == 0 {
            VideoPixelFormat::Yuv420p
        } else {
            VideoPixelFormat::Other
        },
    )
}

/// Removes the emulation-prevention bytes (`00 00 03` → `00 00`).
fn unescape(nal: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(nal.len());
    let mut zeros = 0;
    for &byte in nal {
        if zeros >= 2 && byte == 3 {
            zeros = 0;
            continue;
        }
        zeros = if byte == 0 { zeros + 1 } else { 0 };
        out.push(byte);
    }
    out
}

/// A big-endian bit reader with Exp-Golomb codes.
struct Bits<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }

    fn bit(&mut self) -> Option<u32> {
        let byte = self.data.get(self.at / 8)?;
        let bit = (byte >> (7 - self.at % 8)) & 1;
        self.at += 1;
        Some(u32::from(bit))
    }

    fn read(&mut self, count: u32) -> Option<u32> {
        (0..count).try_fold(0u32, |acc, _| Some((acc << 1) | self.bit()?))
    }

    /// An unsigned Exp-Golomb code (`ue(v)`).
    fn ue(&mut self) -> Option<u32> {
        let mut zeros = 0;
        while self.bit()? == 0 {
            zeros += 1;
            if zeros > 31 {
                return None;
            }
        }
        let rest = u64::from(self.read(zeros)?);
        u32::try_from((1u64 << zeros) - 1 + rest).ok()
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    //! Synthetic ISO files, built box by box.

    /// Real SPS written by x264 (High profile, 4:2:0, 8 bits, 48x192).
    pub(crate) const SPS_HIGH_420: &str = "6764000aacd94c66c044000003000400000300c03c489658";
    /// Real SPS (High 4:4:4 Predictive).
    pub(crate) const SPS_HIGH_444: &str = "67f4000a919b298cd808800000030080000018078912cb";
    /// Real SPS (Constrained Baseline).
    pub(crate) const SPS_BASELINE: &str = "6742c00ad90c66c044000003000400000300c03c489920";
    /// Real SPS (High 10, 4:2:0 at 10 bits).
    pub(crate) const SPS_HIGH_10: &str = "676e000aa6cd94c66c0440000003004000000c03c4896580";

    pub(crate) fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect()
    }

    pub(crate) fn boxed(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = u32::try_from(payload.len() + 8)
            .unwrap()
            .to_be_bytes()
            .to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(payload);
        out
    }

    fn full(kind: &[u8; 4], version: u8, payload: &[u8]) -> Vec<u8> {
        let mut body = vec![version, 0, 0, 0];
        body.extend_from_slice(payload);
        boxed(kind, &body)
    }

    fn be(values: &[u32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_be_bytes()).collect()
    }

    /// What a synthetic movie holds.
    #[derive(Clone)]
    pub(crate) struct Movie {
        pub(crate) brand: [u8; 4],
        pub(crate) codec: [u8; 4],
        pub(crate) width: u16,
        pub(crate) height: u16,
        pub(crate) sps: Vec<u8>,
        pub(crate) timescale: u32,
        pub(crate) duration: u32,
        pub(crate) stts: Vec<(u32, u32)>,
        pub(crate) ctts: Option<Vec<(u32, u32)>>,
        pub(crate) audio: bool,
    }

    impl Movie {
        /// 10 s of 480x1920 H.264 (High, 4:2:0) at 24 fps, no audio.
        pub(crate) fn in_rev_c_profile() -> Self {
            Self {
                brand: *b"isom",
                codec: *b"avc1",
                width: 480,
                height: 1920,
                sps: hex(SPS_HIGH_420),
                timescale: 12288,
                duration: 122_880,
                stts: vec![(240, 512)],
                ctts: None,
                audio: false,
            }
        }

        /// The file: `ftyp`, a 16-byte `mdat`, then `moov` (at the end, as
        /// ffmpeg writes it without `+faststart`).
        pub(crate) fn bytes(&self) -> Vec<u8> {
            let mut ftyp = self.brand.to_vec();
            ftyp.extend_from_slice(&[0, 0, 2, 0]);
            ftyp.extend_from_slice(b"isomiso2avc1mp41");
            let mut moov = full(b"mvhd", 0, &[0; 96]);
            moov.extend(self.video_track());
            if self.audio {
                moov.extend(plain_track(b"soun", Vec::new()));
            }
            let mut out = boxed(b"ftyp", &ftyp);
            out.extend(boxed(b"mdat", &[0; 16]));
            out.extend(boxed(b"moov", &moov));
            out
        }

        fn video_track(&self) -> Vec<u8> {
            let mut stbl = full(b"stsd", 0, &[be(&[1]), self.sample_entry()].concat());
            stbl.extend(entries(b"stts", &self.stts));
            if let Some(ctts) = &self.ctts {
                stbl.extend(entries(b"ctts", ctts));
            }
            let mut tkhd = vec![0u8; 76];
            tkhd.extend(be(&[
                u32::from(self.width) << 16,
                u32::from(self.height) << 16,
            ]));
            let mut trak = full(b"tkhd", 0, &tkhd[4..]);
            let mdhd = be(&[0, 0, self.timescale, self.duration, 0]);
            let mut mdia = full(b"mdhd", 0, &mdhd);
            mdia.extend(hdlr(b"vide"));
            mdia.extend(boxed(b"minf", &boxed(b"stbl", &stbl)));
            trak.extend(boxed(b"mdia", &mdia));
            boxed(b"trak", &trak)
        }

        fn sample_entry(&self) -> Vec<u8> {
            let mut entry = vec![0u8; 6];
            entry.extend_from_slice(&1u16.to_be_bytes());
            entry.extend_from_slice(&[0; 16]);
            entry.extend_from_slice(&self.width.to_be_bytes());
            entry.extend_from_slice(&self.height.to_be_bytes());
            entry.extend(be(&[0x0048_0000, 0x0048_0000, 0]));
            entry.extend_from_slice(&1u16.to_be_bytes());
            entry.extend_from_slice(&[0; 32]);
            entry.extend_from_slice(&[0x00, 0x18, 0xFF, 0xFF]);
            let mut avcc = vec![1, self.sps[1], self.sps[2], self.sps[3], 0xFF, 0xE1];
            avcc.extend_from_slice(&u16::try_from(self.sps.len()).unwrap().to_be_bytes());
            avcc.extend_from_slice(&self.sps);
            avcc.extend_from_slice(&[1, 0, 4, 0x68, 0xEB, 0xE3, 0xCB]);
            entry.extend(boxed(b"avcC", &avcc));
            boxed(&self.codec, &entry)
        }
    }

    fn hdlr(kind: &[u8; 4]) -> Vec<u8> {
        let mut body = vec![0u8; 4];
        body.extend_from_slice(kind);
        body.extend_from_slice(&[0; 13]);
        full(b"hdlr", 0, &body)
    }

    pub(crate) fn plain_track(handler: &[u8; 4], stbl: Vec<u8>) -> Vec<u8> {
        let mut mdia = full(b"mdhd", 0, &be(&[0, 0, 48000, 480_000, 0]));
        mdia.extend(hdlr(handler));
        mdia.extend(boxed(b"minf", &boxed(b"stbl", &stbl)));
        boxed(b"trak", &boxed(b"mdia", &mdia))
    }

    fn entries(kind: &[u8; 4], pairs: &[(u32, u32)]) -> Vec<u8> {
        let mut body = be(&[u32::try_from(pairs.len()).unwrap()]);
        for (a, b) in pairs {
            body.extend(be(&[*a, *b]));
        }
        full(kind, 0, &body)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::fixtures::*;
    use super::*;

    fn probed(movie: &Movie) -> MediaInfo {
        let bytes = movie.bytes();
        let len = bytes.len() as u64;
        probe(&mut Cursor::new(bytes), len).unwrap()
    }

    #[test]
    fn an_in_profile_mp4_is_read_from_its_boxes() {
        let movie = Movie::in_rev_c_profile();
        let bytes = movie.bytes();
        assert!(sniff(&bytes));
        let info = probed(&movie);
        assert_eq!(info.format, MediaFormat::Mp4);
        assert_eq!(info.bytes, bytes.len() as u64);
        assert_eq!(info.dimensions, Some(Size::new(480, 1920)));
        assert!(!info.has_audio);
        let track = info.video.unwrap();
        assert_eq!(track.codec, VideoCodec::H264);
        assert_eq!(track.pixel_format, Some(VideoPixelFormat::Yuv420p));
        assert_eq!(track.b_frames, Some(false));
        assert_eq!(track.frame_rate, FrameRate::new(24, 1));
        assert_eq!(track.duration, Some(Duration::from_secs(10)));
    }

    #[test]
    fn audio_b_frames_and_other_codecs_are_reported() {
        let mut movie = Movie::in_rev_c_profile();
        movie.audio = true;
        movie.ctts = Some(vec![(1, 1024), (1, 2560), (1, 0), (1, 512)]);
        let info = probed(&movie);
        assert!(info.has_audio);
        assert_eq!(info.video.unwrap().b_frames, Some(true));
        // A constant composition offset is only a delay.
        movie.ctts = Some(vec![(0, 7), (240, 1024)]);
        assert_eq!(probed(&movie).video.unwrap().b_frames, Some(false));
        movie.codec = *b"hvc1";
        let info = probed(&movie);
        let track = info.video.unwrap();
        assert_eq!(track.codec, VideoCodec::Other);
        assert_eq!(track.pixel_format, None);
        assert_eq!(info.dimensions, Some(Size::new(480, 1920)));
    }

    #[test]
    fn quicktime_and_image_brands_are_not_mp4() {
        let mut movie = Movie::in_rev_c_profile();
        movie.brand = *b"qt  ";
        let info = probed(&movie);
        assert_eq!(info.format, MediaFormat::Other);
        assert!(info.video.is_some(), "the tracks are still read");
        movie.brand = *b"heic";
        assert_eq!(probed(&movie).format, MediaFormat::Other);
        assert_eq!(container(None), MediaFormat::Other);
        assert_eq!(container(Some(b"mp42")), MediaFormat::Mp4);
    }

    #[test]
    fn sps_pixel_formats_follow_profile_chroma_and_depth() {
        let format = |text| sps_pixel_format(&hex(text));
        assert_eq!(format(SPS_HIGH_420), Some(VideoPixelFormat::Yuv420p));
        assert_eq!(format(SPS_BASELINE), Some(VideoPixelFormat::Yuv420p));
        assert_eq!(format(SPS_HIGH_444), Some(VideoPixelFormat::Other));
        assert_eq!(format(SPS_HIGH_10), Some(VideoPixelFormat::Other));
        assert_eq!(sps_pixel_format(&[0x68, 0xEB]), None, "a PPS is not an SPS");
        assert_eq!(sps_pixel_format(&[0x67, 100]), None, "cut short");
        assert_eq!(unescape(&[0, 0, 3, 1, 0, 0, 3]), vec![0, 0, 1, 0, 0]);
        // 32 leading zeros are not an Exp-Golomb code.
        assert_eq!(Bits::new(&[0, 0, 0, 0, 0xFF]).ue(), None);
    }

    #[test]
    fn frame_rate_duration_and_ratio_edges() {
        let timing = Timing(vec![(239, 512), (1, 256)]);
        assert_eq!(timing.frame_rate(12288), FrameRate::new(24, 1));
        assert_eq!(timing.samples(), 240);
        assert_eq!(Timing(vec![(0, 0)]).frame_rate(1000), None);
        assert_eq!(ratio(30000, 1001), FrameRate::new(30000, 1001));
        assert_eq!(
            ratio(5_000_000_001, 100_000_000_000),
            FrameRate::new(50, 1000)
        );
        assert_eq!(ratio(1 << 40, 1), None);
        assert_eq!(ratio(1, 0), None);
        assert_eq!(media_time(0, 1000), None);
        assert_eq!(media_time(1500, 1000), Some(Duration::from_millis(1500)));
        // Without a declared duration the samples add up.
        let mut movie = Movie::in_rev_c_profile();
        movie.duration = 0;
        assert_eq!(
            probed(&movie).video.unwrap().duration,
            Some(Duration::from_secs(10))
        );
        // No samples (a fragmented file): B-frames and rate are unknown.
        movie.stts.clear();
        let track = probed(&movie).video.unwrap();
        assert_eq!(track.b_frames, None);
        assert_eq!(track.frame_rate, None);
        assert_eq!(track.duration, None);
    }

    #[test]
    fn largesize_and_open_ended_boxes_are_followed() {
        let movie = Movie::in_rev_c_profile().bytes();
        // Rewrite the 24-byte mdat (after the 32-byte ftyp) as a 64-bit box.
        let mut large = movie[..32].to_vec();
        large.extend(1u32.to_be_bytes());
        large.extend_from_slice(b"mdat");
        large.extend(24u64.to_be_bytes());
        large.extend_from_slice(&[0; 8]);
        large.extend_from_slice(&movie[56..]);
        let len = large.len() as u64;
        let info = probe(&mut Cursor::new(large), len).unwrap();
        assert_eq!(info.dimensions, Some(Size::new(480, 1920)));
        // A trailing mdat of size 0 runs to the end of the file.
        let mut open = movie.clone();
        open.extend_from_slice(&[0, 0, 0, 0]);
        open.extend_from_slice(b"mdat");
        open.extend_from_slice(&[9; 40]);
        let len = open.len() as u64;
        assert!(probe(&mut Cursor::new(open), len).is_ok());
    }

    #[test]
    fn version_1_headers_64_bit_boxes_and_other_tracks() {
        let mut v1 = vec![1, 0, 0, 0];
        v1.extend([0u8; 16]);
        v1.extend(90_000u32.to_be_bytes());
        v1.extend(180_000u64.to_be_bytes());
        assert_eq!(media_header(&v1).unwrap(), (90_000, Some(180_000)));
        v1[24..32].copy_from_slice(&u64::MAX.to_be_bytes());
        assert_eq!(media_header(&v1).unwrap(), (90_000, None));
        let mut data = 1u32.to_be_bytes().to_vec();
        data.extend_from_slice(b"free");
        data.extend(20u64.to_be_bytes());
        data.extend([7; 4]);
        data.extend(0u32.to_be_bytes());
        data.extend_from_slice(b"skip");
        data.extend([1, 2, 3]);
        let boxes = children(&data).unwrap();
        assert_eq!(boxes.len(), 2);
        assert_eq!(
            (boxes[0].1, boxes[1].1),
            ([7u8; 4].as_slice(), [1u8, 2, 3].as_slice())
        );
        // Subtitles, a track without media and media without a handler.
        let text = plain_track(b"text", Vec::new());
        assert!(matches!(track(&text[8..]).unwrap(), Track::Other));
        assert!(matches!(
            track(&boxed(b"tkhd", &[0; 84])).unwrap(),
            Track::Other
        ));
        assert!(matches!(track(&boxed(b"mdia", &[])).unwrap(), Track::Other));
    }

    #[test]
    fn broken_files_are_errors_never_panics() {
        let movie = Movie::in_rev_c_profile().bytes();
        for cut in 0..movie.len() {
            let _ = probe(&mut Cursor::new(movie[..cut].to_vec()), cut as u64);
        }
        let only_ftyp = &movie[..32];
        let err = probe(&mut Cursor::new(only_ftyp.to_vec()), 32).unwrap_err();
        assert_eq!(err.to_string(), "no moov box");
        let mut tiny = movie[..32].to_vec();
        tiny.extend([0, 0, 0, 4]);
        tiny.extend_from_slice(b"free");
        let len = tiny.len() as u64;
        assert!(matches!(
            probe(&mut Cursor::new(tiny), len),
            Err(Mp4Error::Malformed(_))
        ));
        let mut cut_moov = movie.clone();
        cut_moov.truncate(movie.len() - 10);
        let len = cut_moov.len() as u64;
        assert_eq!(
            probe(&mut Cursor::new(cut_moov), len)
                .unwrap_err()
                .to_string(),
            "the moov box is cut short"
        );
        assert!(children(&[0, 0, 0, 40, b'f', b'r', b'e', b'e']).is_err());
        let io = Mp4Error::from(io::Error::other("disk"));
        assert_eq!(io.to_string(), "disk");
        assert!(!sniff(b"\x89PNG\r\n\x1a\n"));
        assert!(!sniff(b"abc"));
    }
}
