//! Demand-driven background polling; sensor samples never wait on media apps.
use crate::provider::Provider;
use bezel_core::domain::{
    playback::{MediaSession, app_name},
    sensor::{SensorInfo, Snapshot, Wanted},
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::DataReader;
use windows_core::{Interface, RuntimeType};
use windows_future::{AsyncStatus, IAsyncOperation};

#[derive(Default)]
struct Cached {
    at: Option<Instant>,
    snapshot: Snapshot,
}
pub(super) struct Activity {
    cache: Arc<Mutex<Cached>>,
    wanted: Arc<AtomicU8>,
}
impl Activity {
    pub fn new() -> Self {
        let cache = Arc::new(Mutex::new(Cached::default()));
        let wanted = Arc::new(AtomicU8::new(0));
        let weak = Arc::downgrade(&cache);
        let request = Arc::clone(&wanted);
        if let Err(error) = std::thread::Builder::new()
            .name("bezel-media-context".into())
            .spawn(move || run(weak, request))
        {
            tracing::debug!(%error,"could not start Windows media context worker");
        }
        Self { cache, wanted }
    }
}
fn run(weak: std::sync::Weak<Mutex<Cached>>, request: Arc<AtomicU8>) {
    // SAFETY: initialize COM/WinRT on this dedicated thread; balance success at exit.
    #[allow(unsafe_code)]
    let initialized = unsafe {
        windows::Win32::System::WinRT::RoInitialize(
            windows::Win32::System::WinRT::RO_INIT_MULTITHREADED,
        )
    }
    .is_ok();
    let mut system = sysinfo::System::new();
    if !initialized {
        tracing::debug!("Windows media context: WinRT initialization failed");
    }
    let mut last_health = None;
    let mut manager = None;
    let mut covers = std::collections::BTreeMap::<String, Vec<u8>>::new();
    while let Some(cache) = weak.upgrade() {
        let need = request.load(Ordering::Relaxed);
        if need != 0 {
            let mut snapshot = Snapshot::default();
            if need & 1 != 0 {
                system.refresh_processes_specifics(
                    sysinfo::ProcessesToUpdate::All,
                    true,
                    sysinfo::ProcessRefreshKind::nothing(),
                );
                snapshot.activity_available = !system.processes().is_empty();
                snapshot.applications = system
                    .processes()
                    .values()
                    .map(|p| app_name(&p.name().to_string_lossy()))
                    .collect();
                // SAFETY: query the foreground window's PID, without dereferencing its handle.
                #[allow(unsafe_code)]
                let pid = unsafe {
                    let mut pid = 0;
                    let window = windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow();
                    windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(
                        window, &mut pid,
                    );
                    pid
                };
                snapshot.foreground = system
                    .process(sysinfo::Pid::from_u32(pid))
                    .map(|p| app_name(&p.name().to_string_lossy()));
            }
            if need & 2 != 0 && initialized {
                let deadline = Instant::now() + Duration::from_secs(2);
                if manager.is_none() {
                    match Manager::RequestAsync().and_then(|op| wait(op, deadline)) {
                        Ok(m) => manager = Some(m),
                        Err(e) => tracing::debug!(error=%e,"Windows media manager unavailable"),
                    }
                }
                if let Some(m) = &manager {
                    match sessions(m, &mut covers, deadline) {
                        Ok(media) => {
                            snapshot.media = media;
                            snapshot.media_available = true;
                        }
                        Err(e) => {
                            tracing::debug!(error=%e,"Windows media session unavailable");
                            manager = None;
                            covers.clear();
                        }
                    }
                }
            }
            let health = (
                snapshot.activity_available,
                snapshot.applications.len(),
                snapshot.foreground.is_some(),
                snapshot.media_available,
                snapshot.media.len(),
            );
            if last_health != Some(health) {
                tracing::debug!(
                    processes = health.1,
                    foreground = health.2,
                    media_available = health.3,
                    sessions = health.4,
                    "Windows application/media context"
                );
                last_health = Some(health);
            }
            if let Ok(mut out) = cache.lock() {
                *out = Cached {
                    at: Some(Instant::now()),
                    snapshot,
                };
            }
        } else {
            manager = None;
            covers.clear();
            if let Ok(mut out) = cache.lock() {
                *out = Cached::default();
            }
        }
        drop(cache);
        std::thread::sleep(Duration::from_millis(500));
    }
    if initialized {
        // SAFETY: balances successful RoInitialize on this same thread.
        #[allow(unsafe_code)]
        unsafe {
            windows::Win32::System::WinRT::RoUninitialize();
        }
    }
}
impl Provider for Activity {
    fn catalog(&self) -> Vec<SensorInfo> {
        vec![]
    }
    fn want(&mut self, wanted: &Wanted) {
        let flag = u8::from(wanted.contains("activity.processes"))
            | (u8::from(wanted.contains("media.sessions")) << 1);
        self.wanted.store(flag, Ordering::Relaxed);
    }
    fn sample(&mut self, now: Instant, out: &mut Snapshot) {
        if let Ok(cache) = self.cache.lock()
            && cache
                .at
                .is_some_and(|at| now.saturating_duration_since(at) < Duration::from_secs(4))
        {
            out.media.clone_from(&cache.snapshot.media);
            out.media_available = cache.snapshot.media_available;
            out.applications.clone_from(&cache.snapshot.applications);
            out.foreground.clone_from(&cache.snapshot.foreground);
            out.activity_available = cache.snapshot.activity_available;
        }
    }
}
fn wait<T: RuntimeType>(op: IAsyncOperation<T>, deadline: Instant) -> windows_core::Result<T> {
    while op.Status()? == AsyncStatus::Started {
        if Instant::now() >= deadline {
            let _ = op.Cancel();
            return Err(windows_core::Error::new(
                windows_core::HRESULT(0x800705B4u32 as i32),
                "media query timed out",
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    op.GetResults()
}
fn sessions(
    manager: &Manager,
    covers: &mut std::collections::BTreeMap<String, Vec<u8>>,
    deadline: Instant,
) -> windows_core::Result<Vec<MediaSession>> {
    let mut out = Vec::new();
    let mut active = std::collections::BTreeSet::new();
    let sessions = manager.GetSessions()?;
    for i in 0..sessions.Size()?.min(8) {
        if Instant::now() >= deadline {
            break;
        }
        let session = sessions.GetAt(i)?;
        let source = session.SourceAppUserModelId()?.to_string();
        let playing = session.GetPlaybackInfo()?.PlaybackStatus()? == Status::Playing;
        let timeline = session.GetTimelineProperties()?;
        let properties = match wait(session.TryGetMediaPropertiesAsync()?, deadline) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let title: String = properties.Title()?.to_string().chars().take(512).collect();
        let artist: String = properties.Artist()?.to_string().chars().take(256).collect();
        let key = format!("{source}\0{title}\0{artist}");
        active.insert(key.clone());
        let cover = covers
            .entry(key)
            .or_insert_with(|| {
                let read = || -> windows_core::Result<Vec<u8>> {
                    let stream = wait(properties.Thumbnail()?.OpenReadAsync()?, deadline)?;
                    let size = stream.Size()?;
                    if size == 0 || size > 512 * 1024 {
                        return Ok(vec![]);
                    }
                    let reader = DataReader::CreateDataReader(&stream)?;
                    let loaded = wait(
                        reader
                            .LoadAsync(size as u32)?
                            .cast::<IAsyncOperation<u32>>()?,
                        deadline,
                    )?;
                    if loaded != size as u32 {
                        return Ok(vec![]);
                    }
                    let mut bytes = vec![0; size as usize];
                    reader.ReadBytes(&mut bytes)?;
                    Ok(bytes)
                };
                let bytes = read().unwrap_or_default();
                let Ok(mut reader) =
                    image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format()
                else {
                    return vec![];
                };
                let mut limits = image::Limits::default();
                limits.max_image_width = Some(2048);
                limits.max_image_height = Some(2048);
                limits.max_alloc = Some(32 * 1024 * 1024);
                reader.limits(limits);
                let Ok(image) = reader.decode() else {
                    return vec![];
                };
                let mut png = std::io::Cursor::new(Vec::new());
                if image
                    .thumbnail(256, 256)
                    .write_to(&mut png, image::ImageFormat::Png)
                    .is_err()
                {
                    return vec![];
                }
                png.into_inner()
            })
            .clone();
        let start = timeline.StartTime()?.Duration as f64 / 10_000_000.;
        out.push(MediaSession {
            source,
            title,
            artist,
            playing,
            position: (timeline.Position()?.Duration as f64 / 10_000_000. - start).max(0.),
            duration: (timeline.EndTime()?.Duration as f64 / 10_000_000. - start).max(0.),
            cover,
        });
    }
    covers.retain(|k, _| active.contains(k));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "reads the real Windows desktop context on request"]
    fn actual_windows_context_sample_is_nonblocking_and_demand_driven() {
        let mut activity = Activity::new();
        activity.want(&Wanted::All);
        let deadline = Instant::now() + Duration::from_secs(5);
        let snapshot = loop {
            let mut out = Snapshot::default();
            let start = Instant::now();
            activity.sample(start, &mut out);
            assert!(
                start.elapsed() < Duration::from_millis(50),
                "cached sample must never wait on media apps"
            );
            if out.activity_available && out.media_available {
                break out;
            }
            assert!(
                Instant::now() < deadline,
                "Windows process context did not arrive"
            );
            std::thread::sleep(Duration::from_millis(100));
        };
        println!(
            "Windows context: {} process names, {} media sessions, foreground available={}",
            snapshot.applications.len(),
            snapshot.media.len(),
            snapshot.foreground.is_some()
        );
        for media in snapshot.media {
            println!(
                "source={} playing={} title_available={} cover_bytes={} duration={}",
                media.source,
                media.playing,
                !media.title.is_empty(),
                media.cover.len(),
                media.duration
            );
        }
        activity.want(&Wanted::nothing());
        std::thread::sleep(Duration::from_millis(700));
        let mut out = Snapshot::default();
        activity.sample(Instant::now(), &mut out);
        assert!(!out.activity_available && out.media.is_empty());
    }
}
