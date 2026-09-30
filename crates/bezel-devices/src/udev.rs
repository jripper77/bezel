//! The Linux udev rule that lets the logged-in user open every device of the
//! catalog without root, generated from the catalog, and the one-line
//! command that installs it (D-2026-09-30-release-polish-3).
//!
//! `packaging/linux/60-bezel.rules` is exactly [`rules`] (a test keeps them
//! identical); deb and rpm install it. `bezel udev-rules` prints it for
//! AppImage, archive and source installs, and the studio shows
//! [`install_command`] when a port is denied. Bezel never runs the command:
//! the app never elevates.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use bezel_core::domain::catalog::{DESKTOP_MODE_IDS, RULES};
use bezel_core::domain::device::{Transport, UsbId};

/// Name of the rule file (60: before `73-seat-late.rules` applies `uaccess`).
pub const FILE_NAME: &str = "60-bezel.rules";

/// Where a manual install puts it (packages use `/usr/lib/udev/rules.d`).
pub const INSTALL_DIR: &str = "/etc/udev/rules.d";

/// The kernel subsystem a device is opened through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Subsystem {
    /// A CDC-ACM serial port (`/dev/ttyACM*`).
    Tty,
    /// A raw USB device (`/dev/bus/usb/...`).
    Usb,
    /// A HID interface (`/dev/hidraw*`).
    Hidraw,
}

impl Subsystem {
    /// The name udev matches with `SUBSYSTEM==`.
    pub const fn name(self) -> &'static str {
        match self {
            Subsystem::Tty => "tty",
            Subsystem::Usb => "usb",
            Subsystem::Hidraw => "hidraw",
        }
    }

    /// The subsystem of a transport.
    pub const fn of(transport: Transport) -> Self {
        match transport {
            Transport::Serial => Subsystem::Tty,
            Transport::UsbBulk => Subsystem::Usb,
            Transport::Hid => Subsystem::Hidraw,
        }
    }
}

/// One line of the rule: `uaccess` for this USB id through this subsystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Grant {
    /// USB vendor/product id.
    pub usb: UsbId,
    /// Subsystem of the device node.
    pub subsystem: Subsystem,
}

impl Grant {
    /// The udev line.
    pub fn line(&self) -> String {
        format!(
            "SUBSYSTEM==\"{}\", ATTRS{{idVendor}}==\"{:04x}\", ATTRS{{idProduct}}==\"{:04x}\", TAG+=\"uaccess\"",
            self.subsystem.name(),
            self.usb.vid,
            self.usb.pid
        )
    }
}

/// Every grant the catalog needs, sorted by USB id: each screen endpoint
/// through its family's transport, and each desktop-mode id through hidraw.
pub fn grants() -> Vec<Grant> {
    let screens = RULES.iter().map(|r| Grant {
        usb: r.usb,
        subsystem: Subsystem::of(r.family.transport()),
    });
    let desktop_mode = DESKTOP_MODE_IDS.iter().map(|usb| Grant {
        usb: *usb,
        subsystem: Subsystem::of(Transport::Hid),
    });
    let unique: BTreeSet<Grant> = screens.chain(desktop_mode).collect();
    unique.into_iter().collect()
}

const HEADER: &str = "\
# Bezel: lets the logged-in user open USB smart screens without root.
# Generated from the supported-device catalog by `bezel udev-rules`
# (crates/bezel-devices/src/udev.rs); a test keeps this file identical.
# Serial screens are CDC-ACM ttys; the Turing USB (1cbe) and WCH (43a8)
# families are raw USB devices; a Turing USB panel in desktop mode
# (1a86:ad10-ad13, not validated on hardware) is reached through hidraw.
";

/// The whole rule file.
pub fn rules() -> String {
    let mut text = String::from(HEADER);
    text.push('\n');
    for grant in grants() {
        let _ = writeln!(text, "{}", grant.line());
    }
    text
}

/// What gives the rule to the install command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleSource<'a> {
    /// A command line that prints it, such as `bezel udev-rules` (quoted by
    /// the caller as the shell must see it).
    Printed(&'a str),
    /// A file that holds it (a path, quoted here).
    File(&'a str),
}

/// The one-line command that installs the rule and applies it to the
/// devices already plugged in. Shown to the user, never run by Bezel.
pub fn install_command(source: RuleSource<'_>) -> String {
    let target = format!("{INSTALL_DIR}/{FILE_NAME}");
    let install = match source {
        RuleSource::Printed(command) => {
            format!("{command} 2>/dev/null | sudo tee {target} >/dev/null")
        }
        RuleSource::File(path) => format!("sudo install -m 644 {} {target}", shell_quote(path)),
    };
    format!("{install} && sudo udevadm control --reload && sudo udevadm trigger")
}

/// `text` as one shell word: unchanged when it only has characters the
/// shell takes literally, otherwise in single quotes.
pub fn shell_quote(text: &str) -> String {
    let plain = !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-./+:@%,".contains(c));
    if plain {
        return text.to_string();
    }
    format!("'{}'", text.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::domain::catalog::known_usb_ids;

    #[test]
    fn every_catalog_id_is_granted_once_through_its_subsystem() {
        let grants = grants();
        let ids: Vec<UsbId> = grants.iter().map(|g| g.usb).collect();
        let mut expected = known_usb_ids();
        expected.extend_from_slice(DESKTOP_MODE_IDS);
        expected.sort();
        assert_eq!(ids, expected, "one line per id, sorted");
        let of = |vid, pid| {
            grants
                .iter()
                .find(|g| g.usb == UsbId::new(vid, pid))
                .map(|g| g.subsystem)
        };
        assert_eq!(of(0x0525, 0xa4a7), Some(Subsystem::Tty));
        assert_eq!(of(0x1cbe, 0x0088), Some(Subsystem::Usb));
        assert_eq!(of(0x43a8, 0x0e5e), Some(Subsystem::Usb));
        assert_eq!(of(0x1a86, 0xad11), Some(Subsystem::Hidraw));
    }

    #[test]
    fn lines_read_like_udev_wants_them() {
        let grant = Grant {
            usb: UsbId::new(0x1a86, 0xad11),
            subsystem: Subsystem::Hidraw,
        };
        assert_eq!(
            grant.line(),
            r#"SUBSYSTEM=="hidraw", ATTRS{idVendor}=="1a86", ATTRS{idProduct}=="ad11", TAG+="uaccess""#
        );
        let text = rules();
        assert!(text.starts_with("# Bezel: lets the logged-in user"));
        assert!(text.ends_with("TAG+=\"uaccess\"\n"));
        assert_eq!(
            text.lines().filter(|l| l.starts_with("SUBSYSTEM")).count(),
            grants().len()
        );
    }

    #[test]
    fn install_commands_are_one_line_and_use_sudo() {
        assert_eq!(
            install_command(RuleSource::Printed("bezel udev-rules")),
            "bezel udev-rules 2>/dev/null | sudo tee /etc/udev/rules.d/60-bezel.rules >/dev/null \
             && sudo udevadm control --reload && sudo udevadm trigger"
        );
        assert_eq!(
            install_command(RuleSource::File("/home/a b/60-bezel.rules")),
            "sudo install -m 644 '/home/a b/60-bezel.rules' /etc/udev/rules.d/60-bezel.rules \
             && sudo udevadm control --reload && sudo udevadm trigger"
        );
        for source in [RuleSource::Printed("x"), RuleSource::File("y")] {
            assert!(!install_command(source).contains('\n'));
        }
    }

    #[test]
    fn shell_quoting_keeps_plain_words() {
        assert_eq!(
            shell_quote("./target/release/bezel"),
            "./target/release/bezel"
        );
        assert_eq!(
            shell_quote("/opt/Bezel Studio/bezel"),
            "'/opt/Bezel Studio/bezel'"
        );
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(shell_quote(""), "''");
        assert_eq!(shell_quote("$HOME"), "'$HOME'");
        assert_eq!(shell_quote("a=b"), "'a=b'", "not an assignment");
    }

    #[test]
    fn subsystems_follow_transports() {
        assert_eq!(Subsystem::of(Transport::Serial).name(), "tty");
        assert_eq!(Subsystem::of(Transport::UsbBulk).name(), "usb");
        assert_eq!(Subsystem::of(Transport::Hid).name(), "hidraw");
    }
}
