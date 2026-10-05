//! Fixed-endpoint Open-Meteo client and nonblocking weather provider.
use crate::provider::{Provider, put};
use bezel_core::domain::sensor::{Reading, SensorInfo, Snapshot, Wanted};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

type Answer = std::result::Result<(f64, u16), String>;
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(true)
        .proxy(None)
        .max_redirects(0)
        .timeout_global(Some(Duration::from_secs(8)))
        .build()
        .into()
}
fn json(
    mut response: ureq::http::Response<ureq::Body>,
) -> std::result::Result<serde_json::Value, String> {
    let bytes = response
        .body_mut()
        .with_config()
        .limit(1_048_576)
        .read_to_vec()
        .map_err(|_| "weather response unavailable".to_string())?;
    serde_json::from_slice(&bytes).map_err(|_| "invalid weather response".to_string())
}
fn fetch(latitude: f64, longitude: f64) -> Answer {
    let response = agent()
        .get("https://api.open-meteo.com/v1/forecast")
        .query("latitude", latitude.to_string())
        .query("longitude", longitude.to_string())
        .query("current", "temperature_2m,weather_code")
        .call()
        .map_err(|_| "weather connection failed".to_string())?;
    parse_current(&json(response)?)
}
fn parse_current(data: &serde_json::Value) -> Answer {
    let current = &data["current"];
    let t = current["temperature_2m"]
        .as_f64()
        .filter(|v| v.is_finite() && (-150.0..=100.0).contains(v));
    let c = current["weather_code"].as_u64().filter(|v| *v <= 99);
    match (t, c) {
        (Some(t), Some(c)) => Ok((t, c as u16)),
        _ => Err("weather data unavailable".into()),
    }
}

/// A city search result, with the exact coordinates to save in the theme.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct City {
    /// Location name.
    pub name: String,
    /// Country.
    pub country: String,
    /// Region.
    pub region: String,
    /// Latitude.
    pub latitude: f64,
    /// Longitude.
    pub longitude: f64,
}
/// Searches only Open-Meteo's geocoder, when the user presses Search.
pub fn search_cities(query: &str) -> std::result::Result<Vec<City>, String> {
    let query = query.trim();
    if query.chars().count() < 2 || query.chars().count() > 100 {
        return Err("enter 2 to 100 characters".into());
    }
    let response = agent()
        .get("https://geocoding-api.open-meteo.com/v1/search")
        .query("name", query)
        .query("count", "8")
        .query("language", "it")
        .query("format", "json")
        .call()
        .map_err(|_| "city search connection failed".to_string())?;
    let data = json(response)?;
    Ok(data["results"]
        .as_array()
        .into_iter()
        .flatten()
        .take(8)
        .filter_map(|v| {
            let name = v["name"].as_str()?.to_string();
            let latitude = v["latitude"].as_f64()?;
            let longitude = v["longitude"].as_f64()?;
            bezel_core::domain::weather::valid(&name, latitude, longitude).then(|| City {
                name,
                country: v["country"].as_str().unwrap_or("").into(),
                region: v["admin1"].as_str().unwrap_or("").into(),
                latitude,
                longitude,
            })
        })
        .collect())
}
struct Cached {
    answer: Answer,
    next: Instant,
    running: bool,
}
/// Fetches asynchronously; samples read memory and never wait on HTTP.
pub(crate) struct WeatherProvider {
    fetch: Arc<dyn Fn(f64, f64) -> Answer + Send + Sync>,
    wanted: BTreeSet<String>,
    cache: Arc<Mutex<BTreeMap<String, Cached>>>,
}
impl Default for WeatherProvider {
    fn default() -> Self {
        Self {
            wanted: BTreeSet::new(),
            cache: Arc::default(),
            fetch: Arc::new(fetch),
        }
    }
}
fn location(key: &str) -> Option<(String, f64, f64)> {
    let base = key.strip_suffix(".temperature")?.strip_prefix("weather.")?;
    let (lat, lon) = base.split_once(':')?;
    let latitude = lat.parse().ok()?;
    let longitude = lon.parse().ok()?;
    bezel_core::domain::weather::valid("location", latitude, longitude)
        .then(|| (base.to_string(), latitude, longitude))
}
impl Provider for WeatherProvider {
    fn catalog(&self) -> Vec<SensorInfo> {
        Vec::new()
    }
    fn want(&mut self, wanted: &Wanted) {
        self.wanted = match wanted {
            Wanted::Keys(keys) => keys
                .iter()
                .filter_map(|k| location(k.as_str()).map(|l| l.0))
                .take(8)
                .collect(),
            Wanted::All => BTreeSet::new(),
        };
        self.cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|k, v| self.wanted.contains(k) || v.running);
    }
    fn sample(&mut self, now: Instant, out: &mut Snapshot) {
        for base in &self.wanted {
            let key = format!("weather.{base}.temperature");
            let Some((_, lat, lon)) = location(&key) else {
                continue;
            };
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            let running = cache.values().filter(|v| v.running).count();
            let item = cache.entry(base.clone()).or_insert_with(|| Cached {
                answer: Err("loading weather".into()),
                next: now,
                running: false,
            });
            if !item.running && now >= item.next && running < 8 {
                item.running = true;
                let shared = Arc::clone(&self.cache);
                let base = base.clone();
                let fetch = Arc::clone(&self.fetch);
                std::thread::spawn(move || {
                    let answer = fetch(lat, lon);
                    let delay = if answer.is_ok() { 600 } else { 60 };
                    shared.lock().unwrap_or_else(|e| e.into_inner()).insert(
                        base,
                        Cached {
                            answer,
                            next: Instant::now() + Duration::from_secs(delay),
                            running: false,
                        },
                    );
                });
            }
            let (temperature, code) = match &item.answer {
                Ok((t, c)) => (Reading::Value(*t), Reading::Value(f64::from(*c))),
                Err(e) => (
                    Reading::Unavailable(e.clone()),
                    Reading::Unavailable(e.clone()),
                ),
            };
            put(out, &key, temperature);
            put(out, &format!("weather.{base}.code"), code);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_remote_data_and_location_keys() {
        assert_eq!(
            parse_current(&serde_json::json!({"current":{"temperature_2m":21.5,"weather_code":3}}))
                .unwrap(),
            (21.5, 3)
        );
        assert!(
            parse_current(&serde_json::json!({"current":{"temperature_2m":null,"weather_code":3}}))
                .is_err()
        );
        assert!(location("weather.NaN:12.temperature").is_none());
        assert!(location("weather.41.9:12.5.temperature").is_some());
    }
    #[test]
    fn duplicate_locations_share_a_request_and_samples_never_wait() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&calls);
        let (done, finished) = std::sync::mpsc::channel();
        let mut provider = WeatherProvider {
            fetch: Arc::new(move |_, _| {
                count.fetch_add(1, Ordering::SeqCst);
                done.send(()).unwrap();
                Ok((19.0, 3))
            }),
            ..WeatherProvider::default()
        };
        let key =
            bezel_core::domain::sensor::SensorKey::new("weather.41.900000:12.500000.temperature")
                .unwrap();
        provider.want(&Wanted::Keys([key.clone()].into_iter().collect()));
        let mut snapshot = Snapshot::default();
        provider.sample(Instant::now(), &mut snapshot);
        finished.recv_timeout(Duration::from_secs(2)).unwrap();
        // Synchronise with completion without depending on an arbitrary sleep.
        let deadline = Instant::now() + Duration::from_secs(2);
        while provider
            .cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .any(|v| v.running)
        {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        provider.sample(Instant::now(), &mut snapshot);
        assert_eq!(snapshot.get(&key), Reading::Value(19.0));
        provider.sample(Instant::now(), &mut snapshot);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        provider.want(&Wanted::nothing());
        assert!(
            provider
                .cache
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_empty()
        );
    }
    #[test]
    #[ignore = "explicit live service verification"]
    fn live_open_meteo_search_and_current() {
        let cities = search_cities("Roma").unwrap();
        assert!(!cities.is_empty());
        let city = &cities[0];
        let (temperature, code) = fetch(city.latitude, city.longitude).unwrap();
        assert!(temperature.is_finite());
        assert!(code <= 99);
    }
    #[test]
    fn hidden_and_catalog_requests_do_not_fetch_weather() {
        let mut provider = WeatherProvider::default();
        let mut snapshot = Snapshot::default();
        provider.want(&Wanted::All);
        provider.sample(Instant::now(), &mut snapshot);
        assert!(snapshot.is_empty());
        assert!(
            provider
                .cache
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_empty()
        );
    }
}
