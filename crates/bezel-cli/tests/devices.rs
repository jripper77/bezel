//! End-to-end tests of the `bezel` binary against the simulated bus.

use assert_cmd::Command;

#[test]
fn devices_json_lists_fake_turing_88() {
    let out = Command::cargo_bin("bezel")
        .expect("binary built")
        .args(["--fake", "devices", "--json"])
        .output()
        .expect("bezel ran");
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    let screens = json.as_array().expect("array");
    assert_eq!(screens.len(), 1);
    assert_eq!(screens[0]["state"], "awake");
    assert_eq!(screens[0]["family"], "turing-rev-c");
    assert_eq!(screens[0]["models"][0]["id"], "turing-8.8");
    assert_eq!(screens[0]["display"]["address"], "/dev/ttyACM1");
    assert_eq!(screens[0]["wake"]["serial"], "CT88INCH");
}

#[test]
fn version_is_printed() {
    let out = Command::cargo_bin("bezel")
        .expect("binary built")
        .arg("--version")
        .output()
        .expect("bezel ran");
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("bezel "));
}
