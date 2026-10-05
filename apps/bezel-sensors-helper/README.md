# Bezel headless hardware sensors (Windows x64)

This small GPL-3.0-or-later helper embeds the unmodified **LibreHardwareMonitorLib 0.9.6** library under MPL-2.0. It uses the Windows .NET Framework 4.7.2 runtime, without the Libre GUI, an SDK, a web server or network requests. Only the helper needs administrator rights; the CLI and Studio remain unelevated.

`scripts/windows/build-sensors.ps1` compiles with the Windows Framework compiler and copies only the library's transitive DLL dependencies. It is pinned to the installed 0.9.6 release. The helper's source is `Program.cs`; the complete patched Bezel source archive is distributed alongside the binaries. `libraries.json` records exact DLL versions, source commits (where present in product metadata) and SHA-256 hashes. The packaged NuGet metadata in `licenses/` identifies dependency authors, repositories, source commits and licenses. Microsoft libraries are MIT, HidSharp includes its original license, and BlackSharp/DiskInfoToolkit/RAMSPDToolkit are MPL-2.0.

Original library source: [LibreHardwareMonitor v0.9.6](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/tree/v0.9.6), commit `3d331e3370efb858411f19511373eff65a218701`. Dependencies' original source URLs are recorded in their `.nuspec` files. The original MPL license and Libre third-party notices, including embedded PawnIO modules, are included. Hardware-access drivers already used by Libre must be present; this setup does not install drivers.

## Runtime

Run the installation's `configure-sensors.cmd` as administrator once with the normal Windows account. It registers the per-user `Bezel-Sensors` task with an interactive login and highest privileges. The task is also started on demand by Bezel's sensor worker. After the new helper produces CPU temperatures, setup backs up and disables `Bezel-LibreHardwareMonitor` and stops only the GUI executable referenced by that task. Existing Libre remains installed and can be used again after stopping the new task.

The helper atomically replaces `%LOCALAPPDATA%\io.github.slipalison.bezel\sensors\hardware.json` roughly once per second. Bezel reads it on its sensor worker, with a 4 MiB limit and a six-second freshness limit; rendering does not wait for hardware access. The existing sensor IDs, tree, aliases, types and raw units are preserved. Missing values stay unavailable. External WMI/loopback Libre remains a fallback when the bundled snapshot is unavailable.

Bezel updates the local `request` heartbeat while its sensor provider is alive. The helper stops after 30 seconds without a client, and both CLI and Studio can wake the scheduled task when needed. It is single-instance for the current user. No window or tray icon is added by the helper. Errors go to a bounded local `helper.log` beside the snapshot.

Only `/.../name` aliases are imported from the old Libre settings. The helper reads hardware and sensor values and never calls the fan-control API; persisted control modes and software values are ignored. CAM can continue managing cooling and lighting.

## Diagnostics and rollback

The helper accepts `--once`, `--output PATH`, and `--names PATH`. `--self-test` checks invariant number serialization and rejects restoring control modes. The production executable requires elevation through its manifest; the build can be checked with an unelevated console compilation of the same source before configuring the privileged task.

For rollback, stop/disable `Bezel-Sensors` and restore/enable `Bezel-LibreHardwareMonitor` from `startup-backup/Bezel-LibreHardwareMonitor.xml`. Its original executable and configuration are left installed. Bezel will fall back to the original Libre interface once the last embedded snapshot expires.
