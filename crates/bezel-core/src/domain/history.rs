//! Rolling sample histories for graph elements.

use std::collections::{BTreeMap, VecDeque};

use super::sensor::{SensorKey, Snapshot};

/// The last N values of each tracked sensor, oldest first. Unavailable
/// readings are kept as gaps (`None`) so time stays evenly spaced.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Histories {
    series: BTreeMap<SensorKey, (usize, VecDeque<Option<f64>>)>,
}

impl Histories {
    /// Tracks each `(key, length)`; the longest length wins for a repeated key.
    pub fn new(wanted: &[(SensorKey, usize)]) -> Self {
        let mut series: BTreeMap<SensorKey, (usize, VecDeque<Option<f64>>)> = BTreeMap::new();
        for (key, len) in wanted {
            let entry = series.entry(key.clone()).or_insert((0, VecDeque::new()));
            entry.0 = entry.0.max(*len);
        }
        Self { series }
    }

    /// Appends the snapshot's value of every tracked key.
    pub fn push(&mut self, snapshot: &Snapshot) {
        for (key, (len, values)) in &mut self.series {
            values.push_back(snapshot.get(key).value());
            while values.len() > *len {
                values.pop_front();
            }
        }
    }

    /// Copies the values of keys both histories track (after a theme edit).
    pub fn adopt(&mut self, previous: &Histories) {
        for (key, (len, values)) in &mut self.series {
            if let Some((_, old)) = previous.series.get(key) {
                let skip = old.len().saturating_sub(*len);
                values.extend(old.iter().skip(skip).copied());
            }
        }
    }

    /// The history of `key`, oldest first (empty when untracked).
    pub fn get(&self, key: &SensorKey) -> Vec<Option<f64>> {
        self.series
            .get(key)
            .map(|(_, v)| v.iter().copied().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::sensor::Reading;

    #[test]
    fn keeps_the_last_n_values_with_gaps() {
        let k = SensorKey::new("cpu.usage").expect("key");
        let other = SensorKey::new("gpu.usage").expect("key");
        let mut h = Histories::new(&[(k.clone(), 2), (k.clone(), 3)]);
        for v in [1.0, 2.0, 3.0, 4.0] {
            let mut s = Snapshot::default();
            s.insert(k.clone(), Reading::Value(v));
            h.push(&s);
        }
        h.push(&Snapshot::default());
        assert_eq!(h.get(&k), vec![Some(3.0), Some(4.0), None]);
        assert!(h.get(&other).is_empty());

        let mut shorter = Histories::new(&[(k.clone(), 2), (other.clone(), 5)]);
        shorter.adopt(&h);
        assert_eq!(shorter.get(&k), vec![Some(4.0), None]);
        assert!(shorter.get(&other).is_empty());
    }
}
