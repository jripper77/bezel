//! LibreHardwareMonitor over WMI. A `WMIConnection` belongs to the COM
//! apartment of the thread that opened it (it is not `Send`), so one worker
//! thread owns it, re-queries every second and publishes the rows, falling
//! back to the loopback web server in releases without WMI; a sample
//! only copies the latest rows and never waits on WMI. When LHM is not
//! running the worker keeps retrying, so starting LHM later brings the CPU
//! temperature and power back without restarting Bezel.

use std::collections::HashMap;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bezel_core::domain::sensor::{SensorInfo, Snapshot};
use serde::Deserialize;
use wmi::WMIConnection;

use crate::lhm::{HINT, Mapping, Row};
use crate::provider::Provider;

const NAMESPACE: &str = "ROOT\\LibreHardwareMonitor";
const REFRESH: Duration = Duration::from_secs(1);
const STALE: Duration = Duration::from_secs(5);
const FIRST_WAIT: Duration = Duration::from_secs(3);

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WmiSensor {
    identifier: String,
    name: String,
    sensor_type: String,
    value: Option<f32>,
    parent: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WmiHardware {
    identifier: String,
    name: String,
}

/// The worker's latest answer.
struct Latest {
    rows: Result<Vec<Row>, String>,
    hardware: HashMap<String, String>,
    at: Instant,
}

type Shared = Arc<Mutex<Option<Latest>>>;

fn query(conn: &WMIConnection) -> Result<(Vec<Row>, HashMap<String, String>), String> {
    let failed = |e: wmi::WMIError| format!("LibreHardwareMonitor WMI query failed: {e}");
    let sensors: Vec<WmiSensor> = conn
        .raw_query("SELECT Identifier, Name, SensorType, Value, Parent FROM Sensor")
        .map_err(failed)?;
    let hardware: Vec<WmiHardware> = conn
        .raw_query("SELECT Identifier, Name FROM Hardware")
        .map_err(failed)?;
    let rows = sensors
        .into_iter()
        .map(|s| Row {
            identifier: s.identifier,
            name: s.name,
            sensor_type: s.sensor_type,
            value: s.value.unwrap_or(f32::NAN),
            parent: s.parent,
        })
        .collect();
    let names = hardware
        .into_iter()
        .map(|h| (h.identifier, h.name))
        .collect();
    Ok((rows, names))
}

fn worker(latest: Shared, ready: mpsc::Sender<()>, stop: mpsc::Receiver<()>) {
    let mut conn: Option<WMIConnection> = None;
    let http = super::lhm_http::Reader::new();
    let embedded = super::embedded::Reader::new();
    let mut last_status = String::new();
    let mut logged_at = Instant::now();
    crate::sensor_log::log("reader.started", "Libre polling worker started");
    loop {
        let mut source = "embedded";
        let mut failures = Vec::new();
        let answer = embedded.query().or_else(|error| {
            failures.push(format!("embedded: {error}"));
            source = "wmi";
            if conn.is_none() {
                conn = WMIConnection::with_namespace_path(NAMESPACE)
                    .map_err(|e| {
                        failures.push(format!("WMI namespace: {e}"));
                    })
                    .ok();
            }
            let answer = match &conn {
                Some(c) => query(c),
                None => Err(HINT.to_string()),
            };
            // External Libre remains compatible when the bundled helper is unavailable.
            answer.or_else(|error| {
                failures.push(format!("WMI: {error}"));
                source = "http";
                conn = None;
                http.query()
            })
        });
        let status = match &answer {
            Ok((rows, _)) => {
                let valid = rows.iter().filter(|r| r.value.is_finite()).count();
                let corsair_missing = rows
                    .iter()
                    .filter(|r| r.identifier.starts_with("/psu/corsair/") && !r.value.is_finite())
                    .count();
                format!(
                    "source={source}; valid={valid}/{}; corsairMissing={corsair_missing}; fallback={}",
                    rows.len(),
                    failures.join(" | ")
                )
            }
            Err(error) => format!("unavailable: {error}; {}", failures.join(" | ")),
        };
        if status != last_status || logged_at.elapsed() >= Duration::from_secs(30) {
            let corsair = answer
                .as_ref()
                .ok()
                .map(|(rows, _)| {
                    rows.iter()
                        .filter(|r| r.identifier.starts_with("/psu/corsair/"))
                        .map(|r| {
                            format!(
                                "{}={}",
                                r.identifier,
                                if r.value.is_finite() {
                                    r.value.to_string()
                                } else {
                                    "missing".into()
                                }
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            crate::sensor_log::log("reader.health", &format!("{status}; corsair=[{corsair}]"));
            last_status = status;
            logged_at = Instant::now();
        }
        let (rows, hardware) = match answer {
            Ok((rows, hardware)) => (Ok(rows), hardware),
            Err(why) => {
                conn = None;
                (Err(why), HashMap::new())
            }
        };
        if let Ok(mut slot) = latest.lock() {
            *slot = Some(Latest {
                rows,
                hardware,
                at: Instant::now(),
            });
        }
        let _ = ready.send(());
        if !matches!(stop.recv_timeout(REFRESH), Err(RecvTimeoutError::Timeout)) {
            crate::sensor_log::log("reader.stopped", "Libre polling worker stopped");
            return;
        }
    }
}

/// The LibreHardwareMonitor provider.
pub(crate) struct Lhm {
    latest: Shared,
    mapping: Mapping,
    /// Dropping it stops the worker.
    _stop: mpsc::Sender<()>,
}

impl Lhm {
    /// Starts the worker and builds the catalog from its first answer.
    pub(crate) fn new() -> Self {
        let latest: Shared = Arc::new(Mutex::new(None));
        let (ready_tx, ready_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel();
        let shared = Arc::clone(&latest);
        let started = std::thread::Builder::new()
            .name("bezel-lhm".into())
            .spawn(move || worker(shared, ready_tx, stop_rx));
        if let Err(e) = started {
            tracing::warn!("cannot start the LibreHardwareMonitor reader: {e}");
        }
        let _ = ready_rx.recv_timeout(FIRST_WAIT);
        let mapping = match latest.lock().as_deref() {
            Ok(Some(first)) => Mapping::discover(first.rows.as_deref().ok(), &first.hardware),
            _ => Mapping::discover(None, &HashMap::new()),
        };
        Self {
            latest,
            mapping,
            _stop: stop_tx,
        }
    }
}

impl Provider for Lhm {
    fn catalog(&self) -> Vec<SensorInfo> {
        self.mapping.catalog()
    }

    fn sample(&mut self, now: Instant, out: &mut Snapshot) {
        let guard = self.latest.lock();
        if let Ok(Some(latest)) = guard.as_deref()
            && now.saturating_duration_since(latest.at) <= STALE
            && let Ok(rows) = &latest.rows
        {
            self.mapping.refresh(rows, &latest.hardware);
        }
        let readings = match guard.as_deref() {
            Ok(Some(latest)) if now.saturating_duration_since(latest.at) > STALE => self
                .mapping
                .readings(Err("LibreHardwareMonitor stopped answering")),
            Ok(Some(latest)) => self
                .mapping
                .readings(latest.rows.as_deref().map_err(String::as_str)),
            _ => self.mapping.readings(Err(HINT)),
        };
        for (info, reading) in readings {
            out.insert(info.key, reading);
        }
    }
}
