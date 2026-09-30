//! End-to-end tests of `bezel sensors` against the demo sensor source.

use assert_cmd::Command;

#[test]
fn sensors_json_lists_fake_readings() {
    let out = Command::cargo_bin("bezel")
        .expect("binary built")
        .args(["--fake", "sensors", "--json", "--timing"])
        .output()
        .expect("bezel ran");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert!(json["sampleMillis"].is_u64(), "{json}");
    let sensors = json["sensors"].as_array().expect("sensors array");
    let find = |key: &str| {
        sensors
            .iter()
            .find(|s| s["key"] == key)
            .expect("key in the demo catalog")
    };
    let usage = find("cpu.usage");
    assert_eq!(usage["category"], "cpu");
    assert_eq!(usage["label"], "CPU usage");
    assert_eq!(usage["quantity"], "percent");
    assert_eq!(usage["source"], "demo");
    // The second sample, not the warming-up one.
    assert_eq!(usage["value"], 12.5);
    assert_eq!(find("gpu.name")["text"], "Demo GPU");
    assert_eq!(
        find("memory.total")["value"],
        64.0 * 1024.0 * 1024.0 * 1024.0
    );
    let power = find("cpu.power");
    assert!(power.get("value").is_none());
    assert!(
        power["unavailable"]
            .as_str()
            .expect("reason")
            .contains("RAPL")
    );
}

#[test]
fn sensors_watch_needs_a_sane_interval() {
    let out = Command::cargo_bin("bezel")
        .expect("binary built")
        .args(["--fake", "sensors", "--watch", "0.01"])
        .output()
        .expect("bezel ran");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("at least 0.25"));
}
