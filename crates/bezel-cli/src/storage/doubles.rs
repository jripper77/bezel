//! A media transcoder that keeps its files in memory, for the CLI's
//! tests (the real one runs ffmpeg).

use std::collections::BTreeMap;
use std::time::Duration;

use bezel_core::domain::device::{Transport, UsbId};
use bezel_core::domain::discovery::{DeviceAddress, Endpoint};
use bezel_core::domain::frame::{Frame, Rgba};
use bezel_core::domain::geometry::Size;
use bezel_core::domain::job::{Job, JobPhase, Progress};
use bezel_core::domain::media::{
    FrameRate, MediaFormat, MediaInfo, MediaTools, StreamSpec, TranscodeTarget, VideoCodec,
    VideoPixelFormat, VideoTrack,
};
use bezel_core::ports::{MediaLocation, MediaTranscoder, VideoFrames};
use bezel_core::{BezelError, Result};
use bezel_devices::FakeBus;

/// A bus with a WeAct 0.96" (80x160): no storage, no device playback.
pub(crate) fn weact_bus() -> FakeBus {
    FakeBus::new(vec![Endpoint {
        address: DeviceAddress("/dev/ttyACM0".into()),
        transport: Transport::Serial,
        usb: UsbId::new(0x1a86, 0xfe0c),
        serial_number: Some("AD0001".into()),
        manufacturer: None,
        product: None,
        location: None,
    }])
}

/// A bus with a Turing USB 8.8" (TUR_USB): it cannot delete files through
/// Bezel.
pub(crate) fn turing_usb_bus() -> FakeBus {
    FakeBus::new(vec![Endpoint {
        address: DeviceAddress("usb:3-1".into()),
        transport: Transport::UsbBulk,
        usb: UsbId::new(0x1cbe, 0x0088),
        serial_number: None,
        manufacturer: None,
        product: None,
        location: None,
    }])
}

/// The colour of every picture a stub stream decodes.
pub(crate) const STREAMED: Rgba = Rgba::opaque(200, 10, 10);

/// An H.264 yuv420p MP4 of `size`, 10 s long.
pub(crate) fn video(size: Size, bytes: u64, audio: bool) -> MediaInfo {
    MediaInfo {
        format: MediaFormat::Mp4,
        bytes,
        dimensions: Some(size),
        video: Some(VideoTrack {
            codec: VideoCodec::H264,
            pixel_format: Some(VideoPixelFormat::Yuv420p),
            b_frames: Some(false),
            frame_rate: FrameRate::new(24, 1),
            duration: Some(Duration::from_secs(10)),
        }),
        has_audio: audio,
    }
}

/// A still picture.
pub(crate) fn picture(format: MediaFormat, bytes: u64) -> MediaInfo {
    MediaInfo {
        format,
        bytes,
        dimensions: Some(Size::new(64, 64)),
        video: None,
        has_audio: false,
    }
}

/// Local files held in memory; conversions produce an MP4 of the target
/// size, streams a solid [`STREAMED`] picture.
pub(crate) struct StubMedia {
    pub(crate) tools: MediaTools,
    pub(crate) files: BTreeMap<String, MediaInfo>,
    pub(crate) targets: Vec<TranscodeTarget>,
    pub(crate) streamed: Vec<(MediaLocation, StreamSpec)>,
    /// Bytes of every conversion's output.
    pub(crate) output_bytes: u64,
}

impl StubMedia {
    pub(crate) fn ready() -> Self {
        Self::with_tools(MediaTools::Ready {
            version: "stub".into(),
        })
    }

    pub(crate) fn missing() -> Self {
        Self::with_tools(MediaTools::Missing {
            install_hints: vec!["sudo dnf install ffmpeg".into()],
        })
    }

    fn with_tools(tools: MediaTools) -> Self {
        Self {
            tools,
            files: BTreeMap::new(),
            targets: Vec::new(),
            streamed: Vec::new(),
            output_bytes: 300_000,
        }
    }

    pub(crate) fn with(mut self, location: &str, info: MediaInfo) -> Self {
        self.files.insert(location.to_string(), info);
        self
    }
}

struct Solid(Frame);

impl VideoFrames for Solid {
    fn frame_at(&mut self, _: Duration) -> Result<&Frame> {
        Ok(&self.0)
    }
}

impl MediaTranscoder for StubMedia {
    fn tools(&mut self) -> MediaTools {
        self.tools.clone()
    }

    fn probe(&mut self, source: &MediaLocation) -> Result<MediaInfo> {
        self.files
            .get(&source.0)
            .cloned()
            .ok_or_else(|| BezelError::InvalidInput(format!("no file {}", source.0)))
    }

    fn transcode(
        &mut self,
        source: &MediaLocation,
        target: &TranscodeTarget,
        job: &mut Job<'_>,
    ) -> Result<MediaLocation> {
        self.targets.push(*target);
        job.report(Progress::new(JobPhase::Convert, 0, 10_000));
        job.checkpoint()?;
        job.report(Progress::new(JobPhase::Convert, 10_000, 10_000));
        let output = format!("{}.converted.mp4", source.0);
        self.files
            .insert(output.clone(), video(target.size, self.output_bytes, false));
        Ok(MediaLocation(output))
    }

    fn load(&mut self, source: &MediaLocation) -> Result<Vec<u8>> {
        let info = self.probe(source)?;
        Ok(vec![0x42; usize::try_from(info.bytes).unwrap_or(0)])
    }

    fn stream(&mut self, source: &MediaLocation, spec: StreamSpec) -> Result<Box<dyn VideoFrames>> {
        self.streamed.push((source.clone(), spec));
        Ok(Box::new(Solid(Frame::filled(spec.size, STREAMED))))
    }
}
