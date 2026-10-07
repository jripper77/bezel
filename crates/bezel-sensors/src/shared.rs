//! One physical sensor source shared by independent screen runtimes.
use bezel_core::{
    Result,
    domain::sensor::{SensorInfo, Snapshot, Wanted},
    ports::SensorSource,
};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

struct Hub {
    source: Box<dyn SensorSource>,
    wanted: BTreeMap<u64, Wanted>,
    next: u64,
    latest: Option<(Instant, Snapshot)>,
}
pub struct SharedSensors {
    hub: Arc<Mutex<Hub>>,
    id: u64,
}
impl SharedSensors {
    pub fn new(source: Box<dyn SensorSource>) -> Self {
        Self {
            hub: Arc::new(Mutex::new(Hub {
                source,
                wanted: BTreeMap::new(),
                next: 1,
                latest: None,
            })),
            id: 0,
        }
    }
    pub fn replace(&self, source: Box<dyn SensorSource>) {
        let mut hub = self.hub.lock().unwrap_or_else(PoisonError::into_inner);
        hub.source = source;
        hub.latest = None;
        update_wanted(&mut hub);
    }
}
fn update_wanted(hub: &mut Hub) {
    let all = hub
        .wanted
        .values()
        .fold(Wanted::nothing(), |a, b| a.union(b));
    hub.source.want(&all);
}
impl Clone for SharedSensors {
    fn clone(&self) -> Self {
        let mut hub = self.hub.lock().unwrap_or_else(PoisonError::into_inner);
        let id = hub.next;
        hub.next += 1;
        Self {
            hub: Arc::clone(&self.hub),
            id,
        }
    }
}
impl Drop for SharedSensors {
    fn drop(&mut self) {
        let mut hub = self.hub.lock().unwrap_or_else(PoisonError::into_inner);
        hub.wanted.remove(&self.id);
        update_wanted(&mut hub);
    }
}
impl SensorSource for SharedSensors {
    fn catalog(&mut self) -> Result<Vec<SensorInfo>> {
        self.hub
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .source
            .catalog()
    }
    fn sample(&mut self) -> Result<Snapshot> {
        let mut hub = self.hub.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((at, snapshot)) = &hub.latest
            && at.elapsed() < Duration::from_millis(200)
        {
            return Ok(snapshot.clone());
        }
        let snapshot = hub.source.sample()?;
        hub.latest = Some((Instant::now(), snapshot.clone()));
        Ok(snapshot)
    }
    fn want(&mut self, wanted: &Wanted) {
        let mut hub = self.hub.lock().unwrap_or_else(PoisonError::into_inner);
        hub.wanted.insert(self.id, wanted.clone());
        update_wanted(&mut hub);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Counted(Arc<std::sync::atomic::AtomicUsize>);
    impl SensorSource for Counted {
        fn catalog(&mut self) -> Result<Vec<SensorInfo>> {
            Ok(Vec::new())
        }
        fn sample(&mut self) -> Result<Snapshot> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(Snapshot::default())
        }
    }
    #[test]
    fn screens_share_one_sensor_sample() {
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut first = SharedSensors::new(Box::new(Counted(Arc::clone(&count))));
        let mut second = first.clone();
        first.sample().unwrap();
        second.sample().unwrap();
        assert_eq!(count.load(std::sync::atomic::Ordering::Relaxed), 1);
        assert_eq!(first.hub.lock().unwrap().wanted.len(), 0);
        first.want(&Wanted::All);
        second.want(&Wanted::nothing());
        assert_eq!(first.hub.lock().unwrap().wanted.len(), 2);
        drop(second);
        assert_eq!(first.hub.lock().unwrap().wanted.len(), 1);
    }
}
