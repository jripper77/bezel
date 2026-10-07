//! Bounded local diagnostics, independent of the application's tracing setup.
use std::io::Write;
use std::sync::Mutex;
use std::time::SystemTime;

static WRITER: Mutex<()> = Mutex::new(());

pub(crate) fn log(event: &str, details: &str) {
    let Ok(_guard) = WRITER.try_lock() else {
        return;
    };
    let Some(base) = std::env::var_os("LOCALAPPDATA") else {
        return;
    };
    let directory = std::path::PathBuf::from(base).join("io.github.slipalison.bezel/sensors");
    let _ = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(&directory)?;
        let path = directory.join("reader.log");
        if std::fs::metadata(&path).is_ok_and(|m| m.len() > 1024 * 1024) {
            let previous = directory.join("reader.log.1");
            let _ = std::fs::remove_file(&previous);
            std::fs::rename(&path, previous)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        let entry = serde_json::json!({
            "atUnixMillis": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_millis(),
            "pid": std::process::id(), "event": event, "details": details,
        });
        writeln!(file, "{entry}")
    })();
}
