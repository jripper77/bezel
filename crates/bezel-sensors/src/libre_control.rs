//! Restart only Bezel's preconfigured per-user sensor task, without elevating Studio.

/// Restart the bundled Libre reader. The task must already be configured.
pub fn restart_libre_reader() -> std::io::Result<()> {
    #[cfg(windows)]
    crate::sensor_log::log("restart.requested", "Restart Bezel-Sensors scheduled task");
    let result = restart();
    #[cfg(windows)]
    match &result {
        Ok(()) => crate::sensor_log::log(
            "restart.scheduled",
            "Reader published a fresh snapshot after restart",
        ),
        Err(error) => crate::sensor_log::log("restart.failed", &error.to_string()),
    }
    result
}

fn restart() -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let windows = std::env::var_os("WINDIR")
            .ok_or_else(|| std::io::Error::other("Windows directory unavailable"))?;
        let shell = std::path::PathBuf::from(windows)
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        // Serialized across Studio and Light; task acceptance alone is not
        // success. Wait for a newly published snapshot and retry early exits.
        let script = include_str!("windows/restart-reader.ps1");
        let output = std::process::Command::new(shell)
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .creation_flags(0x0800_0000)
            .output()?;
        crate::sensor_log::log(
            "restart.scheduler",
            &format!(
                "exit={}; stdout={}; stderr={}",
                output.status,
                String::from_utf8_lossy(&output.stdout).trim(),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        );
        if !output.status.success() {
            return Err(std::io::Error::other(format!(
                "Could not restart Bezel-Sensors: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    Err(std::io::Error::other(
        "The Libre reader is available on Windows only",
    ))
}
