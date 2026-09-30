//! The packaged udev rule must grant exactly the catalog's USB ids, with the
//! subsystem each one is opened through (tty for serial, usb for bulk,
//! hidraw for a panel in desktop mode), and be what `udev::rules` generates.

use bezel_core::domain::catalog::{DESKTOP_MODE_IDS, RULES, known_usb_ids};
use bezel_core::domain::device::{Transport, UsbId};
use bezel_devices::udev;
use std::collections::BTreeMap;

const RULES_FILE: &str = include_str!("../../../packaging/linux/60-bezel.rules");

fn parse(line: &str) -> Option<(String, UsbId)> {
    let field = |key: &str| {
        let start = line.find(key)? + key.len();
        let rest = &line[start..];
        let end = rest.find('"')?;
        Some(rest[..end].to_string())
    };
    let subsystem = field("SUBSYSTEM==\"")?;
    let vid = u16::from_str_radix(&field("ATTRS{idVendor}==\"")?, 16).ok()?;
    let pid = u16::from_str_radix(&field("ATTRS{idProduct}==\"")?, 16).ok()?;
    assert!(line.contains("TAG+=\"uaccess\""), "{line}");
    Some((subsystem, UsbId::new(vid, pid)))
}

#[test]
fn udev_rules_match_the_catalog() {
    let granted: BTreeMap<UsbId, String> = RULES_FILE
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| parse(l).map(|(s, id)| (id, s)).expect("well-formed rule"))
        .collect();
    let mut catalog: Vec<UsbId> = known_usb_ids();
    catalog.extend_from_slice(DESKTOP_MODE_IDS);
    catalog.sort();
    assert_eq!(granted.keys().copied().collect::<Vec<_>>(), catalog);

    for rule in RULES {
        let expected = match rule.family.transport() {
            Transport::Serial => "tty",
            Transport::UsbBulk => "usb",
            Transport::Hid => "hidraw",
        };
        assert_eq!(granted[&rule.usb], expected, "subsystem of {}", rule.usb);
    }
    for id in DESKTOP_MODE_IDS {
        assert_eq!(granted[id], "hidraw", "subsystem of {id}");
    }
}

#[test]
fn packaged_file_is_the_generated_rule() {
    assert_eq!(RULES_FILE, udev::rules());
}
