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

#[test]
fn fake_sensors_take_the_ping_host_and_mangohud_folder() {
    let out = Command::cargo_bin("bezel")
        .expect("binary built")
        .args(["--fake", "sensors", "--json", "--ping-host", "1.1.1.1"])
        .args(["--mangohud-dir", "/nowhere"])
        .output()
        .expect("bezel ran");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    let sensors = json["sensors"].as_array().expect("sensors array");
    assert!(sensors.iter().any(|s| s["key"] == "gpu.fps"));
    assert!(sensors.iter().any(|s| s["key"] == "net.ping"));
}

/// This machine, read-only: `gpu.fps` comes from the newest MangoHud log of
/// `--mangohud-dir` (the golden Witcher 3 log, written just now) and
/// `net.ping` from `--ping-host`.
#[cfg(target_os = "linux")]
#[test]
fn sensors_read_game_fps_and_ping_from_the_given_sources() {
    let dir = std::env::temp_dir().join(format!("bezel-cli-mangohud-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp folder");
    let log = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../bezel-sensors/src/fps/fixtures/mangohud/witcher3_2026-09-30_21-05-00.csv");
    let text = std::fs::read(log).expect("golden log");
    std::fs::write(dir.join("witcher3_2026-09-30_21-05-00.csv"), text).expect("fresh log");
    let out = Command::cargo_bin("bezel")
        .expect("binary built")
        .args([
            "sensors",
            "--json",
            "--ping-host",
            "127.0.0.1",
            "--mangohud-dir",
        ])
        .arg(&dir)
        .output()
        .expect("bezel ran");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    let sensors = json["sensors"].as_array().expect("sensors array");
    let find = |key: &str| {
        sensors
            .iter()
            .find(|s| s["key"] == key)
            .expect("key in the catalog")
    };
    let fps = find("gpu.fps");
    assert_eq!(fps["value"], 139.874, "{fps}");
    let source = fps["source"].as_str().expect("source");
    assert!(
        source.contains(dir.to_str().expect("UTF-8 temp path")),
        "{source}"
    );
    assert!(source.ends_with("(not validated on hardware)"), "{source}");
    let ping = find("net.ping");
    assert!(
        ping["source"]
            .as_str()
            .expect("source")
            .contains("127.0.0.1")
    );
    assert!(
        ping["value"].is_f64() || ping["unavailable"].is_string(),
        "{ping}"
    );
}
