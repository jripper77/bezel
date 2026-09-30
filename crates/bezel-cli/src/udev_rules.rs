//! `bezel udev-rules`: prints the udev rule generated from the catalog
//! (stdout) and the one-line sudo command that installs it (stderr), for
//! AppImage, archive and source installs (D-2026-09-30-release-polish-3).
//! Bezel never runs the command; both come from `bezel_devices::udev`,
//! which the studio shares.

use std::io::Write;

use bezel_core::BezelError;
use bezel_devices::udev::{self, RuleSource};

use crate::messages::Messages;

/// Returns the rule for stdout and writes the install command on `log`.
/// `program` is how the user started Bezel (`bezel`, `./bezel`), so that
/// the command runs the same binary.
pub fn run(program: &str, log: &mut dyn Write) -> anyhow::Result<String> {
    let printer = format!("{} udev-rules", udev::shell_quote(program));
    let mut log = Messages::new(log);
    writeln!(
        log,
        "To install this rule, run (it needs root; Bezel never runs it itself):"
    );
    writeln!(
        log,
        "{}",
        udev::install_command(RuleSource::Printed(&printer))
    );
    log.check()?;
    Ok(udev::rules())
}

/// What to add to an error that says a device was denied: on Linux the
/// udev rule is the fix, and `bezel udev-rules` gives it.
pub fn access_hint(error: &anyhow::Error) -> Option<&'static str> {
    let denied = error
        .chain()
        .any(|cause| matches!(cause.downcast_ref(), Some(BezelError::AccessDenied { .. })));
    denied.then_some(
        "bezel: hint: `bezel udev-rules` prints the udev rule that lets your user open the \
         screen, and the one-line sudo command that installs it",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;
    use bezel_core::domain::catalog::{DESKTOP_MODE_IDS, RULES};
    use bezel_core::domain::device::{Transport, UsbId};

    const PACKAGED: &str = include_str!("../../../packaging/linux/60-bezel.rules");

    /// The line that grants `usb` through `subsystem`, written by hand.
    fn line(subsystem: &str, usb: UsbId) -> String {
        format!(
            "SUBSYSTEM==\"{subsystem}\", ATTRS{{idVendor}}==\"{:04x}\", \
             ATTRS{{idProduct}}==\"{:04x}\", TAG+=\"uaccess\"",
            usb.vid, usb.pid
        )
    }

    #[test]
    fn printed_rule_matches_packaged_file_and_catalog() {
        let mut log = Vec::new();
        let printed = run("bezel", &mut log).unwrap();
        assert_eq!(printed, PACKAGED, "packaging/linux/60-bezel.rules is stale");

        let lines: Vec<&str> = printed.lines().filter(|l| !l.starts_with('#')).collect();
        let mut expected: Vec<String> = RULES
            .iter()
            .map(|r| {
                let subsystem = match r.family.transport() {
                    Transport::Serial => "tty",
                    Transport::UsbBulk => "usb",
                    Transport::Hid => "hidraw",
                };
                line(subsystem, r.usb)
            })
            .chain(DESKTOP_MODE_IDS.iter().map(|id| line("hidraw", *id)))
            .collect();
        expected.sort();
        expected.dedup();
        let mut granted: Vec<String> = lines
            .iter()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect();
        granted.sort();
        assert_eq!(granted, expected);

        let log = String::from_utf8(log).unwrap();
        let commands: Vec<&str> = log.lines().filter(|l| l.contains("sudo")).collect();
        assert_eq!(
            commands,
            [
                "bezel udev-rules 2>/dev/null | sudo tee /etc/udev/rules.d/60-bezel.rules \
                 >/dev/null && sudo udevadm control --reload && sudo udevadm trigger"
            ]
        );
        assert!(log.contains("Bezel never runs it itself"), "{log}");
    }

    #[test]
    fn the_command_runs_the_same_program() {
        let mut log = Vec::new();
        run("/opt/My Tools/bezel", &mut log).unwrap();
        let log = String::from_utf8(log).unwrap();
        assert!(
            log.contains("'/opt/My Tools/bezel' udev-rules 2>/dev/null | sudo tee"),
            "{log}"
        );
        let mut closed = crate::messages::tests::Closing::after(0);
        assert!(
            run("bezel", &mut closed).is_err(),
            "an unwritable stderr fails"
        );
    }

    #[test]
    fn denied_devices_point_at_udev_rules() {
        let denied = BezelError::AccessDenied {
            address: "/dev/ttyACM1".into(),
            reason: "Permission denied".into(),
        };
        let wrapped = Err::<(), _>(denied)
            .context("could not open the screen")
            .unwrap_err();
        assert!(
            access_hint(&wrapped)
                .unwrap()
                .contains("`bezel udev-rules`")
        );
        let other = anyhow::Error::new(BezelError::Timeout("x".into()));
        assert!(access_hint(&other).is_none());
        assert!(access_hint(&anyhow::anyhow!("plain")).is_none());
    }
}
