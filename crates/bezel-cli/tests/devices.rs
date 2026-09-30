//! End-to-end tests of the `bezel` binary against the simulated bus.
#![allow(clippy::expect_used)] // helpers of a failing test panic

use assert_cmd::Command;

fn bezel(args: &[&str]) -> std::process::Output {
    Command::cargo_bin("bezel")
        .expect("binary built")
        .args(args)
        .output()
        .expect("bezel ran")
}

#[test]
fn devices_json_lists_fake_turing_88() {
    let out = bezel(&["--fake", "devices", "--json"]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    let screens = json.as_array().expect("array");
    assert_eq!(screens.len(), 2);
    assert_eq!(screens[0]["state"], "awake");
    assert_eq!(screens[0]["family"], "turing-rev-c");
    assert_eq!(screens[0]["models"][0]["id"], "turing-8.8");
    assert_eq!(screens[0]["display"]["address"], "/dev/ttyACM1");
    assert_eq!(screens[0]["wake"]["serial"], "CT88INCH");
    assert_eq!(screens[1]["state"], "desktop-mode");
    assert_eq!(screens[1]["hid"]["usb"], "1a86:ad11");
    assert_eq!(screens[1]["hardware_validated"], false);
}

#[test]
fn hid_desktop_requires_confirm_in_the_binary() {
    let out = bezel(&["--fake", "monitor-mode"]);
    assert!(!out.status.success(), "no --yes, no switch");
    assert!(out.stdout.is_empty());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Nothing was sent to the panel"), "{err}");
    assert!(
        err.contains("bezel: switching back to USB monitor mode needs --yes"),
        "{err}"
    );

    let out = bezel(&["--fake", "monitor-mode", "--yes"]);
    assert!(out.status.success());
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(
        said.starts_with("Sent the switch back to USB monitor mode to hid:/dev/hidraw7"),
        "{said}"
    );
}

#[test]
fn udev_rules_prints_the_rule_and_only_shows_the_command() {
    let out = bezel(&["udev-rules"]);
    assert!(out.status.success());
    let rule = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        rule,
        include_str!("../../../packaging/linux/60-bezel.rules")
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(" udev-rules 2>/dev/null | sudo tee /etc/udev/rules.d/60-bezel.rules"),
        "{err}"
    );
}

#[test]
fn version_is_printed() {
    let out = bezel(&["--version"]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("bezel "));
}
