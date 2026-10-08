//! Explicitly enabled, bounded diagnostics. Network targets are never recorded.
use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};

static ENABLED: AtomicBool = AtomicBool::new(false);
static OVERLAY: AtomicBool = AtomicBool::new(false);
static CORNER: AtomicU8 = AtomicU8::new(2);
pub fn corner() -> crate::performance::Corner {
    crate::performance::Corner::from_index(CORNER.load(Ordering::Relaxed))
}
static OUTPUT: Mutex<Option<Output>> = Mutex::new(None);
static INSTALLED: OnceLock<()> = OnceLock::new();
const LIMIT: u64 = 5 * 1024 * 1024;
struct Output {
    file: File,
    path: PathBuf,
    bytes: u64,
}

impl Output {
    fn open(path: &Path) -> io::Result<Self> {
        // Append-only handles on Windows cannot be truncated for rotation.
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(path)?;
        let bytes = file.seek(SeekFrom::End(0))?;
        Ok(Self {
            file,
            path: path.to_owned(),
            bytes,
        })
    }
    fn write_record(&mut self, bytes: &[u8]) -> io::Result<()> {
        if self.bytes + bytes.len() as u64 > LIMIT {
            let previous = self.path.with_extension("previous.log");
            std::fs::copy(&self.path, &previous)?;
            self.file.set_len(0)?;
            self.file.seek(SeekFrom::Start(0))?;
            self.bytes = 0;
        }
        self.file.write_all(bytes)?;
        self.bytes += bytes.len() as u64;
        Ok(())
    }
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}
pub fn overlay() -> bool {
    enabled() && OVERLAY.load(Ordering::Relaxed)
}

fn install() {
    INSTALLED.get_or_init(|| {
        let _ = tracing::subscriber::set_global_default(DebugSubscriber);
    });
}

pub fn configure(
    path: &Path,
    on: bool,
    show: bool,
    corner: crate::performance::Corner,
) -> io::Result<()> {
    install();
    let mut output = OUTPUT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if on && output.is_none() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        *output = Some(Output::open(path)?);
    }
    ENABLED.store(on, Ordering::Relaxed);
    OVERLAY.store(show, Ordering::Relaxed);
    CORNER.store(corner as u8, Ordering::Relaxed);
    if !on {
        *output = None;
    }
    drop(output);
    tracing::info!(
        overlay = show,
        version = crate::EVO_VERSION,
        "debug enabled"
    );
    Ok(())
}

fn allowed(metadata: &Metadata<'_>) -> bool {
    enabled()
        && metadata.level() <= &tracing::Level::DEBUG
        && metadata.target().starts_with("bezel")
        && !metadata.target().starts_with("bezel_klipy")
}

struct Fields(serde_json::Map<String, serde_json::Value>);
impl tracing::field::Visit for Fields {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0
            .insert(field.name().into(), format!("{value:?}").into());
    }
    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.0.insert(field.name().into(), serde_json::json!(value));
    }
    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.0.insert(field.name().into(), value.into());
    }
    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.0.insert(field.name().into(), value.into());
    }
}

struct DebugSubscriber;
impl Subscriber for DebugSubscriber {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        allowed(metadata)
    }
    fn register_callsite(&self, _: &'static Metadata<'static>) -> tracing::subscriber::Interest {
        tracing::subscriber::Interest::sometimes()
    }
    fn new_span(&self, _: &Attributes<'_>) -> Id {
        static IDS: AtomicU64 = AtomicU64::new(1);
        Id::from_u64(IDS.fetch_add(1, Ordering::Relaxed))
    }
    fn record(&self, _: &Id, _: &Record<'_>) {}
    fn record_follows_from(&self, _: &Id, _: &Id) {}
    fn enter(&self, _: &Id) {}
    fn exit(&self, _: &Id) {}
    fn event(&self, event: &Event<'_>) {
        if !allowed(event.metadata()) {
            return;
        }
        let mut fields = Fields(serde_json::Map::new());
        event.record(&mut fields);
        let record = serde_json::json!({
            "time": chrono::Utc::now().to_rfc3339(),
            "level": event.metadata().level().as_str(),
            "target": event.metadata().target(), "fields": fields.0
        });
        let Ok(mut bytes) = serde_json::to_vec(&record) else {
            return;
        };
        bytes.push(b'\n');
        let mut guard = OUTPUT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(output) = guard.as_mut() else {
            return;
        };
        let _ = output.write_record(&bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotation_continues_writing_after_the_limit_on_windows() {
        let path = std::env::temp_dir().join(format!("bezel-rotation-{}.log", std::process::id()));
        let previous = path.with_extension("previous.log");
        let _ = std::fs::remove_file(&path);
        let mut output = Output::open(&path).unwrap();
        output.write_record(&vec![b'x'; LIMIT as usize]).unwrap();
        output.write_record(b"after rotation\n").unwrap();
        output.write_record(b"still recording\n").unwrap();
        drop(output);
        assert_eq!(std::fs::metadata(&previous).unwrap().len(), LIMIT);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "after rotation\nstill recording\n"
        );
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(previous).unwrap();
    }
    #[test]
    fn debug_logs_are_opt_in_and_exclude_network_targets() {
        let path = std::env::temp_dir().join(format!("bezel-debug-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&path);
        configure(&path, false, true, crate::performance::Corner::default()).unwrap();
        assert!(!enabled());
        assert!(!overlay());
        assert!(!path.exists());
        configure(&path, true, false, crate::performance::Corner::default()).unwrap();
        tracing::debug!(target: "bezel_devices::test", bytes = 123u64, "serial diagnostic");
        tracing::warn!(target: "bezel_klipy::test", "network secret must be excluded");
        configure(&path, false, false, crate::performance::Corner::default()).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("serial diagnostic"));
        assert!(!text.contains("network secret"));
        std::fs::remove_file(path).unwrap();
    }
}
