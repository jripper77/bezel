//! The packaged udev rule must grant exactly the catalog's USB ids, with the
//! subsystem each family is opened through (tty for serial, usb for bulk).

use bezel_core::domain::catalog::{RULES, known_usb_ids};
use bezel_core::domain::device::{Transport, UsbId};
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
    let catalog: Vec<UsbId> = known_usb_ids();
    assert_eq!(granted.keys().copied().collect::<Vec<_>>(), catalog);

    for rule in RULES {
        let expected = match rule.family.transport() {
            Transport::Serial => "tty",
            Transport::UsbBulk => "usb",
        };
        assert_eq!(granted[&rule.usb], expected, "subsystem of {}", rule.usb);
    }
}
