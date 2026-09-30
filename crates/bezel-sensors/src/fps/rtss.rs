//! Game frame rates from RivaTuner Statistics Server (RTSS), Windows.
//!
//! RTSS publishes what it measures in the named shared memory
//! `RTSSSharedMemoryV2`, laid out as `RTSS_SHARED_MEMORY` of the RTSS SDK's
//! `RTSSSharedMemory.h` (docs/reverse-engineering/sensors.md § 8). The
//! parser here is safe code over a copy of those bytes, tested on every OS
//! with the golden fixtures of `fixtures/rtss/`; only [`Rtss`] (Windows)
//! maps the real memory, and it only reads it.

use super::MAX_AGE;

/// `dwSignature` of an initialised memory: `'RTSS'` as a DWORD, stored as
/// the bytes `SSTR`.
const SIGNATURE: u32 = u32::from_be_bytes(*b"RTSS");
/// `dwSignature` of a memory RTSS is tearing down.
const CLOSING: u32 = 0xDEAD;
/// `dwVersion` (`major << 16 | minor`) of the first layout with the
/// application array.
const FIRST_V2: u32 = 0x0002_0000;

/// How far a period's end may be ahead of the clock read after the copy
/// (RTSS writing in between) and still count as now.
const AHEAD_MS: u32 = 1000;

/// Byte offsets of the header's DWORDs (little-endian).
mod header {
    pub(super) const SIGNATURE: usize = 0x00;
    pub(super) const VERSION: usize = 0x04;
    pub(super) const APP_ENTRY_SIZE: usize = 0x08;
    pub(super) const APP_ARR_OFFSET: usize = 0x0c;
    pub(super) const APP_ARR_SIZE: usize = 0x10;
}

/// Byte offsets in one `RTSS_SHARED_MEMORY_APP_ENTRY`.
mod entry {
    pub(super) const PROCESS_ID: usize = 0;
    pub(super) const NAME: usize = 4;
    /// `char szName[MAX_PATH]`.
    pub(super) const NAME_LEN: usize = 260;
    pub(super) const TIME0: usize = 268;
    pub(super) const TIME1: usize = 272;
    pub(super) const FRAMES: usize = 276;
    pub(super) const FRAME_TIME: usize = 280;
    /// Bytes up to `dwFrameTime` included: the least any v2 entry has.
    pub(super) const MIN_SIZE: usize = 284;
}

/// Why there is no reading while RTSS is absent or closing.
pub(crate) const NOT_RUNNING: &str = "RivaTuner Statistics Server is not running: start it \
     (it comes with MSI Afterburner) to read game FPS";

/// Why there is no reading while RTSS runs without a game.
const NO_GAME: &str = "no game is running under RivaTuner Statistics Server: \
     start one while RTSS runs (it measures the 3D programs it hooks)";

/// An application RTSS hooks and measures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct App {
    /// The executable's file name, from `szName` (a full path).
    pub(crate) name: String,
    /// Start and end of the last measurement period, milliseconds on
    /// RTSS's clock (`GetTickCount`).
    time0: u32,
    time1: u32,
    /// Frames presented in that period.
    frames: u32,
    /// Duration of the last frame, microseconds.
    frame_time_us: u32,
}

impl App {
    /// Milliseconds between the end of its last period and `now`, across
    /// the clock's 49.7-day wrap. A period that ends at most
    /// [`AHEAD_MS`] after `now` (RTSS wrote it after the clock was read) is
    /// 0 ms old; one further ahead is from before the wrap, very old.
    fn age_ms(&self, now: u32) -> u32 {
        let age = now.wrapping_sub(self.time1);
        if age > u32::MAX - AHEAD_MS { 0 } else { age }
    }

    /// Frames per second over its last period (the SDK's
    /// `1000 * dwFrames / (dwTime1 - dwTime0)`), else from its last frame
    /// time. `None` before RTSS measured anything.
    fn frame_rate(&self) -> Option<f64> {
        let period = self.time1.wrapping_sub(self.time0);
        if period > 0 && period <= u32::MAX / 2 {
            return Some(1000.0 * f64::from(self.frames) / f64::from(period));
        }
        (self.frame_time_us > 0).then(|| 1_000_000.0 / f64::from(self.frame_time_us))
    }
}

/// The little-endian DWORD at `at`, if `memory` holds it.
fn dword(memory: &[u8], at: usize) -> Option<u32> {
    let bytes = memory.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

/// A header DWORD as an offset, size or count.
fn header_usize(memory: &[u8], at: usize) -> Result<usize, String> {
    dword(memory, at)
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(truncated)
}

fn truncated() -> String {
    "RivaTuner Statistics Server's shared memory is smaller than its header says".to_string()
}

/// Checks the signature and version, and returns the application array's
/// `(offset, entry size, entries)`.
fn app_array(memory: &[u8]) -> Result<(usize, usize, usize), String> {
    match dword(memory, header::SIGNATURE) {
        Some(SIGNATURE) => {}
        Some(CLOSING) | None => return Err(NOT_RUNNING.to_string()),
        Some(_) => {
            return Err(
                "RivaTuner Statistics Server has not initialised its shared memory yet".to_string(),
            );
        }
    }
    let version = dword(memory, header::VERSION).ok_or_else(truncated)?;
    if version < FIRST_V2 {
        return Err(format!(
            "RivaTuner Statistics Server's shared memory v{}.{} is too old: \
             update RivaTuner (v2.0 or newer is read)",
            version >> 16,
            version & 0xffff
        ));
    }
    let size = header_usize(memory, header::APP_ENTRY_SIZE)?;
    if size < entry::MIN_SIZE {
        return Err(truncated());
    }
    Ok((
        header_usize(memory, header::APP_ARR_OFFSET)?,
        size,
        header_usize(memory, header::APP_ARR_SIZE)?,
    ))
}

/// Every application RTSS measures: slots with a process id whose first
/// period started (the SDK requires a non-zero `dwTime0`). The layout comes
/// from the header, as the SDK asks, not from a fixed version.
pub(crate) fn apps(memory: &[u8]) -> Result<Vec<App>, String> {
    let (offset, size, count) = app_array(memory)?;
    let mut apps = Vec::new();
    for slot in 0..count {
        let bytes = slot
            .checked_mul(size)
            .and_then(|start| start.checked_add(offset))
            .and_then(|base| memory.get(base..base.checked_add(size)?))
            .ok_or_else(truncated)?;
        apps.extend(app(bytes));
    }
    Ok(apps)
}

/// The application in one entry, if the slot is used and measured.
fn app(bytes: &[u8]) -> Option<App> {
    let process_id = dword(bytes, entry::PROCESS_ID)?;
    let time0 = dword(bytes, entry::TIME0)?;
    if process_id == 0 || time0 == 0 {
        return None;
    }
    let raw = bytes.get(entry::NAME..entry::NAME + entry::NAME_LEN)?;
    let raw = raw.split(|b| *b == 0).next().unwrap_or_default();
    let path = String::from_utf8_lossy(raw);
    let name = path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or_default()
        .to_string();
    Some(App {
        name,
        time0,
        time1: dword(bytes, entry::TIME1)?,
        frames: dword(bytes, entry::FRAMES)?,
        frame_time_us: dword(bytes, entry::FRAME_TIME)?,
    })
}

/// The frame rate of the game RTSS measured last, `now` being RTSS's
/// clock when `memory` was copied. Older than [`MAX_AGE`]: unavailable.
pub(crate) fn frame_rate(memory: &[u8], now: u32) -> Result<f64, String> {
    let apps = apps(memory)?;
    let newest = apps
        .iter()
        .min_by_key(|app| app.age_ms(now))
        .ok_or_else(|| NO_GAME.to_string())?;
    if u128::from(newest.age_ms(now)) > MAX_AGE.as_millis() {
        return Err(format!(
            "{} rendered no frame in the last {} s (minimized, paused or closing); \
             FPS shows again when it renders",
            newest.name,
            MAX_AGE.as_secs()
        ));
    }
    newest
        .frame_rate()
        .ok_or_else(|| format!("RivaTuner has not measured {} yet", newest.name))
}

/// `gpu.fps` from this machine's RTSS.
#[cfg(windows)]
pub(crate) struct Rtss;

#[cfg(windows)]
impl super::FrameRateSource for Rtss {
    fn describe(&self) -> String {
        "RivaTuner Statistics Server shared memory".to_string()
    }

    fn read(&mut self) -> Result<f64, String> {
        let memory = shared::copy().ok_or_else(|| NOT_RUNNING.to_string())?;
        // Read after the copy, so no period in it ends later than `now`.
        frame_rate(&memory, shared::tick_count())
    }
}

/// The Win32 calls that open RTSS's memory read-only, copy it and let it
/// go, and the clock its times use. The only `unsafe` in Bezel.
#[cfg(windows)]
mod shared {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::Memory::{
        FILE_MAP_READ, MEM_COMMIT, MEMORY_BASIC_INFORMATION, MEMORY_MAPPED_VIEW_ADDRESS,
        MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, VirtualQuery,
    };
    use windows_sys::Win32::System::SystemInformation::GetTickCount;

    /// The mapping RTSS creates.
    const NAME: &str = "RTSSSharedMemoryV2";

    /// The most copied: the v2 memory with 256 slots of the newest layout
    /// is about 1.3 MB; a bigger region is not RTSS's.
    const MAX_COPY: usize = 16 << 20;

    /// An open mapping and its read-only view, both released on drop.
    struct View {
        mapping: HANDLE,
        view: MEMORY_MAPPED_VIEW_ADDRESS,
    }

    impl View {
        /// `None` when RTSS publishes no memory (it is not running).
        #[allow(
            unsafe_code,
            reason = "Win32 FFI: RTSS publishes frame rates only in a named file mapping"
        )]
        fn open() -> Option<Self> {
            let name: Vec<u16> = NAME.encode_utf16().chain([0]).collect();
            // SAFETY: `name` is a NUL-terminated UTF-16 string that outlives
            // the call; FILE_MAP_READ asks for read access only and the handle
            // is not inherited.
            let mapping = unsafe { OpenFileMappingW(FILE_MAP_READ, 0, name.as_ptr()) };
            if mapping.is_null() {
                return None;
            }
            // SAFETY: `mapping` is the handle just opened with FILE_MAP_READ;
            // offset 0 and length 0 map the whole section, read-only.
            let view = unsafe { MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, 0) };
            // Built before the check so that drop closes `mapping` either way.
            let view = View { mapping, view };
            (!view.view.Value.is_null()).then_some(view)
        }

        /// The view's bytes, copied out.
        #[allow(
            unsafe_code,
            reason = "Win32 FFI: VirtualQuery sizes the view, whose bytes are copied by raw pointer"
        )]
        fn copy(&self) -> Option<Vec<u8>> {
            let mut info = MEMORY_BASIC_INFORMATION::default();
            let info_len = size_of::<MEMORY_BASIC_INFORMATION>();
            // SAFETY: `self.view.Value` is the base of a live view and `info`
            // is a writable MEMORY_BASIC_INFORMATION of `info_len` bytes.
            let written = unsafe { VirtualQuery(self.view.Value, &mut info, info_len) };
            if written == 0 || info.State != MEM_COMMIT {
                return None;
            }
            let len = info.RegionSize.min(MAX_COPY);
            let mut bytes = vec![0u8; len];
            // SAFETY: VirtualQuery reports `RegionSize` committed bytes from
            // the view's base, mapped readable (FILE_MAP_READ) until `self`
            // drops; `bytes` is our own buffer of `len` bytes. RTSS may write
            // meanwhile: every field is a plain integer or text, so a torn
            // value is at worst one wrong reading, and no Rust reference to
            // the shared bytes is ever made.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    self.view.Value.cast::<u8>().cast_const(),
                    bytes.as_mut_ptr(),
                    len,
                );
            }
            Some(bytes)
        }
    }

    impl Drop for View {
        #[allow(
            unsafe_code,
            reason = "Win32 FFI: releases the view and the handle `open` took"
        )]
        fn drop(&mut self) {
            if !self.view.Value.is_null() {
                // SAFETY: `view` came from MapViewOfFile and is unmapped once,
                // here, after every copy of it was taken.
                unsafe { UnmapViewOfFile(self.view) };
            }
            // SAFETY: `mapping` came from OpenFileMappingW and is closed once, here.
            unsafe { CloseHandle(self.mapping) };
        }
    }

    /// A copy of RTSS's memory; `None` when RTSS is not running.
    pub(super) fn copy() -> Option<Vec<u8>> {
        View::open()?.copy()
    }

    /// RTSS's clock: milliseconds since boot, wrapping every 49.7 days.
    #[allow(
        unsafe_code,
        reason = "Win32 FFI: GetTickCount is the clock of RTSS's measurement periods"
    )]
    pub(super) fn tick_count() -> u32 {
        // SAFETY: GetTickCount takes no argument, cannot fail and touches no
        // memory of ours.
        unsafe { GetTickCount() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fps::hex_fixture;

    fn fixture(name: &str) -> Vec<u8> {
        let text = match name {
            "two_games" => include_str!("fixtures/rtss/two_games.hex"),
            "zero_fps" => include_str!("fixtures/rtss/zero_fps.hex"),
            "closing" => include_str!("fixtures/rtss/closing.hex"),
            _ => unreachable!("no fixture {name}"),
        };
        hex_fixture(text)
    }

    /// Overwrites the DWORD at `at`.
    fn set(memory: &mut [u8], at: usize, value: u32) {
        memory[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    /// Byte offset of the witcher3.exe entry (slot 1) in `two_games`.
    const WITCHER: usize = 0x1024 + 600;

    #[test]
    fn the_signature_is_the_bytes_sstr() {
        assert_eq!(SIGNATURE, 0x5254_5353);
        assert_eq!(&fixture("two_games")[..4], b"SSTR");
    }

    #[test]
    fn measured_apps_are_listed_with_their_file_name() {
        let apps = apps(&fixture("two_games")).unwrap();
        let listed: Vec<&str> = apps.iter().map(|a| a.name.as_str()).collect();
        // Slot 0 is free and firefox.exe (slot 3) was never measured.
        assert_eq!(listed, ["witcher3.exe", "Hades.exe"]);
    }

    #[test]
    fn the_game_measured_last_gives_the_frame_rate() {
        let memory = fixture("two_games");
        assert_eq!(frame_rate(&memory, 101_500), Ok(144.0));
        // Hades closed its last period ten seconds earlier: not chosen.
        assert_eq!(frame_rate(&memory, 101_000), Ok(144.0));
    }

    #[test]
    fn a_game_silent_for_3_s_is_unavailable_never_its_last_value() {
        let memory = fixture("two_games");
        assert_eq!(frame_rate(&memory, 104_000), Ok(144.0));
        let why = frame_rate(&memory, 104_001).unwrap_err();
        assert!(
            why.starts_with("witcher3.exe rendered no frame in the last 3 s"),
            "{why}"
        );
    }

    #[test]
    fn a_live_zero_is_zero() {
        assert_eq!(frame_rate(&fixture("zero_fps"), 201_200), Ok(0.0));
    }

    #[test]
    fn absent_or_closing_rtss_says_how_to_start_it() {
        for memory in [fixture("closing"), Vec::new()] {
            let why = frame_rate(&memory, 0).unwrap_err();
            assert_eq!(why, NOT_RUNNING);
            assert!(why.contains("start it"), "{why}");
        }
        let mut uninitialised = fixture("two_games");
        set(&mut uninitialised, header::SIGNATURE, 0);
        assert!(
            frame_rate(&uninitialised, 0)
                .unwrap_err()
                .contains("not initialised")
        );
    }

    #[test]
    fn rtss_without_a_game_says_so() {
        let mut memory = fixture("two_games");
        set(&mut memory, WITCHER + entry::PROCESS_ID, 0);
        set(&mut memory, WITCHER + 600 + entry::PROCESS_ID, 0);
        let why = frame_rate(&memory, 101_500).unwrap_err();
        assert_eq!(why, NO_GAME);
    }

    #[test]
    fn version_1_memory_is_refused() {
        let mut memory = fixture("two_games");
        set(&mut memory, header::VERSION, 0x0001_0003);
        let why = frame_rate(&memory, 101_500).unwrap_err();
        assert!(why.contains("v1.3 is too old"), "{why}");
    }

    #[test]
    fn a_header_that_overstates_the_memory_is_refused() {
        let memory = fixture("two_games");
        let cut = &memory[..memory.len() - 1];
        assert_eq!(frame_rate(cut, 101_500), Err(truncated()));
        let mut tiny_entries = memory.clone();
        set(&mut tiny_entries, header::APP_ENTRY_SIZE, 100);
        assert_eq!(frame_rate(&tiny_entries, 101_500), Err(truncated()));
        let mut huge = memory;
        set(&mut huge, header::APP_ARR_SIZE, u32::MAX);
        assert_eq!(frame_rate(&huge, 101_500), Err(truncated()));
    }

    #[test]
    fn entries_are_found_where_the_header_says() {
        // A newer layout: a longer header and bigger entries.
        let mut memory = vec![0u8; 0x100 + 2 * 1000];
        memory[..4].copy_from_slice(b"SSTR");
        set(&mut memory, header::VERSION, 0x0002_0015);
        set(&mut memory, header::APP_ENTRY_SIZE, 1000);
        set(&mut memory, header::APP_ARR_OFFSET, 0x100);
        set(&mut memory, header::APP_ARR_SIZE, 2);
        let slot = 0x100 + 1000;
        set(&mut memory, slot + entry::PROCESS_ID, 12);
        memory[slot + entry::NAME..slot + entry::NAME + 5].copy_from_slice(b"a.exe");
        set(&mut memory, slot + entry::TIME0, 5_000);
        set(&mut memory, slot + entry::TIME1, 5_500);
        set(&mut memory, slot + entry::FRAMES, 30);
        assert_eq!(frame_rate(&memory, 5_600), Ok(60.0));
    }

    #[test]
    fn times_survive_the_clock_wrap() {
        let mut memory = fixture("two_games");
        set(&mut memory, WITCHER + entry::TIME0, u32::MAX - 499);
        set(&mut memory, WITCHER + entry::TIME1, 500);
        assert_eq!(frame_rate(&memory, 700), Ok(144.0));
        // Written just after the clock was read: 0 ms old, not 49 days.
        assert_eq!(frame_rate(&memory, 490), Ok(144.0));
    }

    #[test]
    fn the_frame_time_is_the_fallback() {
        let mut memory = fixture("two_games");
        set(&mut memory, WITCHER + entry::TIME0, 101_000);
        let fps = frame_rate(&memory, 101_500).unwrap();
        assert!((fps - 1_000_000.0 / 6944.0).abs() < 1e-9, "{fps}");
        set(&mut memory, WITCHER + entry::FRAME_TIME, 0);
        assert_eq!(
            frame_rate(&memory, 101_500),
            Err("RivaTuner has not measured witcher3.exe yet".to_string())
        );
    }
}
