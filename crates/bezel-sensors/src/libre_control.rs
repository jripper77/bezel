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
            "Task start accepted; reader health will be logged separately",
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
        // Stop is asynchronous. Wait until it has exited before starting the
        // IgnoreNew task; an immediate Start would silently do nothing.
        let script = "$ErrorActionPreference='Stop'; try { \
            $task=Get-ScheduledTask -TaskName 'Bezel-Sensors' -TaskPath '\\'; \
            Write-Output ('before: state=' + $task.State); \
            Stop-ScheduledTask -TaskName 'Bezel-Sensors' -TaskPath '\\'; \
            $deadline=[DateTime]::UtcNow.AddSeconds(10); \
            while ((Get-ScheduledTask -TaskName 'Bezel-Sensors' -TaskPath '\\').State -eq 'Running') { \
                if ([DateTime]::UtcNow -gt $deadline) { throw 'Sensor reader did not stop in time' }; \
                Start-Sleep -Milliseconds 200 \
            }; \
            Write-Output 'stop completed'; \
            Start-ScheduledTask -TaskName 'Bezel-Sensors' -TaskPath '\\'; \
            $info=Get-ScheduledTaskInfo -TaskName 'Bezel-Sensors' -TaskPath '\\'; \
            Write-Output ('after: state=' + (Get-ScheduledTask -TaskName 'Bezel-Sensors' -TaskPath '\\').State + '; lastTaskResult=' + $info.LastTaskResult) \
            } catch { [Console]::Error.WriteLine($_.Exception.Message); exit 1 }";
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
