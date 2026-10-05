//! Read-only loopback fallback for LibreHardwareMonitor 0.9.6.
//! Runs on the existing LHM worker; rendering never waits for HTTP.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use serde::Deserialize;

use crate::lhm::{HINT, Row};

pub(super) struct Reader(ureq::Agent);

impl Reader {
    pub(super) fn new() -> Self {
        Self(
            ureq::Agent::config_builder()
                .proxy(None)
                .max_redirects(0)
                .timeout_global(Some(Duration::from_secs(2)))
                .build()
                .into(),
        )
    }

    pub(super) fn query(&self) -> Result<(Vec<Row>, HashMap<String, String>), String> {
        let failed = |e| format!("{HINT} (local HTTP: {e})");
        let mut response = self
            .0
            .get("http://127.0.0.1:8085/data.json")
            .call()
            .map_err(failed)?;
        let json = response
            .body_mut()
            .with_config()
            .limit(4 * 1024 * 1024)
            .read_to_string()
            .map_err(failed)?;
        parse(&json)
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct Node {
    text: String,
    hardware_id: Option<String>,
    sensor_id: Option<String>,
    #[serde(rename = "Type")]
    sensor_type: String,
    raw_value: Option<String>,
    children: Vec<Node>,
}

pub(super) fn parse(json: &str) -> Result<(Vec<Row>, HashMap<String, String>), String> {
    let root: Node = serde_json::from_str(json)
        .map_err(|e| format!("LibreHardwareMonitor invalid data.json: {e}"))?;
    let mut rows = Vec::new();
    let mut hardware = HashMap::new();
    collect(root, "", &mut rows, &mut hardware);
    // LHM can publish the same GPU sensor twice in the tree. One
    // identifier must produce one catalog entry and one measurement.
    let mut seen = HashSet::new();
    rows.retain(|row| seen.insert(row.identifier.clone()));
    if rows.is_empty() {
        return Err("LibreHardwareMonitor data.json contains no sensors".into());
    }
    Ok((rows, hardware))
}

fn collect(node: Node, parent: &str, rows: &mut Vec<Row>, hardware: &mut HashMap<String, String>) {
    let parent = node.hardware_id.as_deref().unwrap_or(parent);
    if node.hardware_id.is_some() {
        hardware.insert(parent.to_string(), node.text.clone());
    }
    if let Some(identifier) = node.sensor_id {
        rows.push(Row {
            identifier,
            name: node.text,
            sensor_type: node.sensor_type,
            value: number(node.raw_value.as_deref()),
            parent: parent.to_string(),
        });
    }
    for child in node.children {
        collect(child, parent, rows, hardware);
    }
}

// RawValue is still formatted text (e.g. "44,0 °C" on Italian
// Windows), but unlike Value its unit does not change with magnitude.
// Missing or unparseable measurements remain unavailable, never zero.
fn number(raw: Option<&str>) -> f32 {
    raw.and_then(|raw| raw.split_whitespace().next())
        .and_then(|raw| raw.replace(',', ".").parse::<f32>().ok())
        .filter(|n| n.is_finite())
        .unwrap_or(f32::NAN)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::lhm::Mapping;

    #[test]
    fn italian_tree_maps_cpu_and_nested_board_sensors_using_raw_units() {
        let (rows, hardware) = parse(
            r#"{
          "Text":"Sensor","Children":[
            {"Text":"Intel Core Ultra 7 265K","HardwareId":"/intelcpu/0","Children":[
              {"Text":"Temperatures","Children":[
                {"Text":"CPU Package","SensorId":"/intelcpu/0/temperature/22",
                 "Type":"Temperature","Value":"111.2 °F","RawValue":"44,0 °C"}]}]},
            {"HardwareId":"/motherboard","Children":[
              {"Text":"Nuvoton","HardwareId":"/lpc/nct6687dr/0","Children":[
                {"Text":"CPU Fan","SensorId":"/lpc/nct6687dr/0/fan/0",
                 "Type":"Fan","RawValue":"2348 RPM"}]}]},
            {"HardwareId":"/nic/0","Children":[
              {"Text":"Download","SensorId":"/nic/0/throughput/0","Type":"Throughput",
               "Value":"18,6 MB/s","RawValue":"19549180,0 B/s"}]}] }"#,
        )
        .expect("valid tree");
        assert_eq!(rows[0].value, 44.0);
        assert_eq!(rows[1].parent, "/lpc/nct6687dr/0");
        assert_eq!(rows[2].value, 19_549_180.0);
        let mapping = Mapping::discover(Some(&rows), &hardware);
        let readings = mapping.readings(Ok(&rows));
        assert!(
            readings
                .iter()
                .any(|(info, reading)| info.key.as_str() == "cpu.temperature"
                    && *reading == bezel_core::domain::sensor::Reading::Value(44.0))
        );
        assert!(
            readings
                .iter()
                .any(|(info, reading)| info.key.as_str() == "cpu.fan"
                    && *reading == bezel_core::domain::sensor::Reading::Value(2348.0))
        );
    }

    #[test]
    fn repeated_sensor_identifiers_have_one_catalog_entry() {
        let (rows, hardware) = parse(
            r#"{"Children":[
          {"Text":"GPU Bus","SensorId":"/gpu-nvidia/0/load/3","Type":"Load","RawValue":"1,0 %"},
          {"Text":"GPU Bus","SensorId":"/gpu-nvidia/0/load/3","Type":"Load","RawValue":"1,0 %"}
        ]}"#,
        )
        .expect("valid duplicate tree");
        assert_eq!(rows.len(), 1);
        let mapping = Mapping::discover(Some(&rows), &hardware);
        assert_eq!(mapping.catalog().len(), mapping.readings(Ok(&rows)).len());
    }

    #[test]
    fn missing_values_are_unavailable_and_wrong_documents_fail() {
        for raw in [None, Some(""), Some("N/A"), Some("NaN"), Some("Infinity")] {
            assert!(number(raw).is_nan());
        }
        assert_eq!(number(Some("0 RPM")), 0.0);
        assert!(parse("{}").is_err());
        assert!(parse("<html>error</html>").is_err());
    }
}
