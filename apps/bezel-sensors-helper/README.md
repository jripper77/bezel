# Bezel headless hardware sensors (Windows x64)

This small GPL-3.0-or-later helper embeds the unmodified **LibreHardwareMonitorLib 0.9.6** library under MPL-2.0. It uses the Windows .NET Framework 4.7.2 runtime, without the Libre GUI, an SDK, a web server or network requests. Only the helper needs administrator rights; the CLI and Studio remain unelevated.

`scripts/windows/build-sensors.ps1` compiles with the Windows Framework compiler and copies only the library's transitive DLL dependencies. It is pinned to the installed 0.9.6 release. The helper's source is `Program.cs`; the complete patched Bezel source archive is distributed alongside the binaries. `libraries.json` records exact DLL versions, source commits (where present in product metadata) and SHA-256 hashes. The packaged NuGet metadata in `licenses/` identifies dependency authors, repositories, source commits and licenses. Microsoft libraries are MIT, HidSharp includes its original license, and BlackSharp/DiskInfoToolkit/RAMSPDToolkit are MPL-2.0.

Original library source: [LibreHardwareMonitor v0.9.6](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/tree/v0.9.6), commit `3d331e3370efb858411f19511373eff65a218701`. Dependencies' original source URLs are recorded in their `.nuspec` files. The original MPL license and Libre third-party notices, including embedded PawnIO modules, are included. Hardware-access drivers already used by Libre must be present; this setup does not install drivers.

## Runtime

Run the installation's `configure-sensors.cmd` as administrator once with the normal Windows account. It registers the per-user `Bezel-Sensors` task with an interactive login and highest privileges. The task is also started on demand by Bezel's sensor worker. After the new helper produces CPU temperatures, setup backs up and disables `Bezel-LibreHardwareMonitor` and stops only the GUI executable referenced by that task. Existing Libre remains installed and can be used again after stopping the new task.

The helper atomically replaces `%LOCALAPPDATA%\io.github.slipalison.bezel\sensors\hardware.json` roughly once per second. Bezel reads it on its sensor worker, with a 4 MiB limit and a six-second freshness limit; rendering does not wait for hardware access. The existing sensor IDs, tree, aliases, types and raw units are preserved. Missing values stay unavailable. External WMI/loopback Libre remains a fallback when the bundled snapshot is unavailable.

Bezel updates the local `request` heartbeat while its sensor provider is alive. The helper grants 30 seconds after hardware initialization for clients to connect, even when a heartbeat from a previous login exists. After that it stops after 30 seconds without a client. The login launcher updates the heartbeat before waking the task and while waiting for sensors; both CLI and Studio can also wake the scheduled task when needed. It is single-instance for the current user. No window or tray icon is added by the helper. Errors go to a bounded local `helper.log` beside the snapshot.

Only `/.../name` aliases are imported from the old Libre settings. The helper reads hardware and sensor values and never calls the fan-control API; persisted control modes and software values are ignored. CAM can continue managing cooling and lighting.

## Diagnostics and rollback

Diagnostics are kept beside `hardware.json`: `helper.log` records hardware update times,
exceptions with stack traces, missing sensor IDs, and Corsair values on a health change
or every 30 seconds. `reader.log` records the source used by Bezel (embedded, WMI or
HTTP), fallback errors, Corsair readings, and each restart request and scheduler result.
Reader entries use `atUnixMillis` (UTC milliseconds since the Unix epoch) and the
process ID; helper entries use UTC timestamps and the process ID. Each log rotates
at 1 MiB and retains one previous file with a `.1` suffix. A successful scheduler
start is logged separately from the reader's actual health: it does not guarantee
that the PSU resumed reading.

The helper accepts `--once`, `--output PATH`, and `--names PATH`. `--self-test` checks invariant number serialization and rejects restoring control modes. The production executable requires elevation through its manifest; the build can be checked with an unelevated console compilation of the same source before configuring the privileged task.

For rollback, stop/disable `Bezel-Sensors` and restore/enable `Bezel-LibreHardwareMonitor` from `startup-backup/Bezel-LibreHardwareMonitor.xml`. Its original executable and configuration are left installed. Bezel will fall back to the original Libre interface once the last embedded snapshot expires.

After standby, Bezel's recovery waits for a newly published snapshot rather
than just a Task Scheduler acknowledgement. Early exits are retried, and
Studio/Light serialize concurrent recovery requests. The helper acquires its
single-instance mutex even when an earlier owner abandoned it; an existing
mutex name alone does not indicate a running reader. Studio and CLI also check
for a missing or stalled embedded snapshot in their polling worker: allow
startup/recovery 90 seconds, tolerate samples up to 60 seconds old before
requesting recovery, and keep at most one restart worker per sensor reader.
These recovery thresholds are separate from the six-second freshness limit
used to accept readings. Missing data remains unavailable during recovery.
