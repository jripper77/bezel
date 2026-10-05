//! The bundled read-only LHM helper publishes an atomic per-user snapshot.
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::lhm::Row;

const MAX_BYTES: u64 = 4 * 1024 * 1024;
const STALE: Duration = Duration::from_secs(6);

pub(super) struct Reader {
    directory: Option<PathBuf>,
}

impl Reader {
    pub(super) fn new() -> Self {
        let directory = std::env::var_os("LOCALAPPDATA")
            .map(|base| PathBuf::from(base).join("io.github.slipalison.bezel/sensors"));
        let reader = Self { directory };
        reader.request();
        if reader.query().is_err() {
            use std::os::windows::process::CommandExt;
            if let Some(windows) = std::env::var_os("WINDIR") {
                let scheduler = PathBuf::from(windows).join("System32/schtasks.exe");
                // Run only our preconfigured, per-user elevated sensor task.
                // No UAC dialog, shell command text or executable download.
                let _ = std::process::Command::new(scheduler)
                    .args(["/Run", "/TN", "Bezel-Sensors"])
                    .creation_flags(0x0800_0000)
                    .output();
            }
        }
        reader
    }

    fn request(&self) {
        let Some(directory) = &self.directory else {
            return;
        };
        if std::fs::create_dir_all(directory).is_ok()
            && let Ok(file) = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .open(directory.join("request"))
        {
            let _ = file.set_modified(SystemTime::now());
        }
    }

    pub(super) fn query(&self) -> Result<(Vec<Row>, HashMap<String, String>), String> {
        self.request();
        let directory = self
            .directory
            .as_ref()
            .ok_or("sensor helper data folder unavailable")?;
        read_snapshot(&directory.join("hardware.json"), SystemTime::now())
    }
}

fn read_snapshot(
    path: &Path,
    now: SystemTime,
) -> Result<(Vec<Row>, HashMap<String, String>), String> {
    let file = File::open(path).map_err(|_| "bundled sensor helper is not running")?;
    let metadata = file
        .metadata()
        .map_err(|_| "sensor snapshot metadata unavailable")?;
    let modified = metadata
        .modified()
        .map_err(|_| "sensor snapshot timestamp unavailable")?;
    if now.duration_since(modified).unwrap_or(Duration::ZERO) > STALE {
        return Err("bundled sensor helper stopped updating".into());
    }
    let mut json = String::new();
    file.take(MAX_BYTES + 1)
        .read_to_string(&mut json)
        .map_err(|_| "could not read sensor snapshot")?;
    if json.len() as u64 > MAX_BYTES {
        return Err("sensor snapshot is too large".into());
    }
    super::lhm_http::parse(&json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_snapshot_preserves_identifiers_and_missing_values_and_expires() {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "bezel-sensors-{}-{unique}.json",
            std::process::id()
        ));
        std::fs::write(&path, r#"{"Provider":"Bezel LibreHardwareMonitorLib","Children":[
          {"Text":"CPU","HardwareId":"/intelcpu/0","Children":[
            {"Text":"CPU Package","SensorId":"/intelcpu/0/temperature/0","Type":"Temperature","RawValue":"42.5"},
            {"Text":"Missing","SensorId":"/intelcpu/0/power/0","Type":"Power","RawValue":null}]}]}"#).unwrap();
        let (rows, hardware) = read_snapshot(&path, SystemTime::now()).unwrap();
        assert_eq!(hardware["/intelcpu/0"], "CPU");
        assert_eq!(rows[0].identifier, "/intelcpu/0/temperature/0");
        assert_eq!(rows[0].value, 42.5);
        assert!(rows[1].value.is_nan());
        assert!(read_snapshot(&path, SystemTime::now() + Duration::from_secs(7)).is_err());
        std::fs::write(&path, vec![b' '; MAX_BYTES as usize + 1]).unwrap();
        assert!(
            read_snapshot(&path, SystemTime::now())
                .unwrap_err()
                .contains("too large")
        );
        std::fs::remove_file(path).unwrap();
    }
}
