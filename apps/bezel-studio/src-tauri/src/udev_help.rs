//! What fixes a port the system denied (D-2026-09-30-release-polish-3): on
//! Linux, the udev rule of `bezel_devices::udev`, written to the app's cache
//! so the one-line install command can name the file. Bezel never runs the
//! command (the app never elevates); the UI shows it, copyable.

use std::path::PathBuf;

use bezel_devices::udev::{RuleSource, install_command, rules};

/// Where the rule is written for the install command.
#[derive(Debug, Clone)]
pub struct UdevHelp {
    file: PathBuf,
}

impl UdevHelp {
    /// The rule goes to `file` (in the app's cache).
    pub fn new(file: PathBuf) -> Self {
        Self { file }
    }

    /// The command that installs the rule, once the rule is in the file it
    /// names; `None` when the file cannot be written.
    pub fn command(&self) -> Option<String> {
        let rule = rules();
        let written = std::fs::read_to_string(&self.file).is_ok_and(|text| text == rule);
        if !written {
            let write = || -> std::io::Result<()> {
                if let Some(dir) = self.file.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                std::fs::write(&self.file, &rule)
            };
            if let Err(e) = write() {
                tracing::warn!(file = %self.file.display(), "udev rule not written: {e}");
                return None;
            }
        }
        let path = self.file.display().to_string();
        Some(install_command(RuleSource::File(&path)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_devices::udev::FILE_NAME;

    #[test]
    fn the_command_installs_the_rule_it_wrote() {
        let dir = std::env::temp_dir().join(format!("bezel-udev help-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("cache").join(FILE_NAME);
        let help = UdevHelp::new(file.clone());
        let command = help.command().unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), rules());
        let quoted = format!("'{}'", file.display());
        assert!(
            command.starts_with(&format!(
                "sudo install -m 644 {quoted} /etc/udev/rules.d/60-bezel.rules"
            )),
            "{command}"
        );
        assert!(command.ends_with("sudo udevadm trigger"), "{command}");
        // An old rule in the file is written again.
        std::fs::write(&file, "old").unwrap();
        assert_eq!(help.command().unwrap(), command);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), rules());

        // A file that cannot be written: no command to show.
        let blocked = UdevHelp::new(file.join("under-a-file"));
        assert_eq!(blocked.command(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
