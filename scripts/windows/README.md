# Windows light runtime and login setup

Copy these scripts beside `bezel.exe` and `bezel-studio.exe` in the existing installation directory. Save your theme and select the screen in Studio first. The light runtime reads `lastTheme` and `liveScreen` from Studio's existing settings; it does not open WebView or the editor. The `.cmd` launchers allow double-click use; right-click `configure-startup.cmd` and choose **Run as administrator** for the one-time setup.

- `start-light.ps1`: start the saved theme with `--tray`, wait up to roughly 90 seconds for the local sensor server, and retry initial screen connection failures. The native tray menu opens Studio or quits; double-click opens Studio. The light process remains idle while Studio is open, hides its own icon, and resumes with the newly saved theme and screen when Studio exits. Logs and the owned process identity are stored beside the executable. A second invocation does not start another runtime. Startup is skipped if Studio is already open.
- `stop-light.ps1`: stop only this launcher's CLI process, checking executable path and process start time. The display retains its last frame.
- `open-studio.ps1`: compatibility shortcut that simply opens Studio. Opening the executable directly also requests control automatically. The Windows preference **Use Bezel Light when closing Studio** is enabled by default. The close button asks about unsaved changes and hands the saved theme to Light, starting it if needed. Disable the preference to keep live Studio in the tray. Explicit Quit stops both runtimes.
- `configure-sensors.ps1`: one-time elevated setup for the bundled headless sensor reader. It verifies CPU temperatures before disabling the previous Libre GUI task; logs and rollback information are retained in the installation. The new helper exits after 30 seconds without active Bezel clients.
- `configure-startup.ps1`: run manually in an elevated PowerShell window with your normal Windows account. It backs up and removes only the root tasks `TempMonitor_0_3`, `TempMonitor_8`, and `UsbMonitor`, configures the bundled sensor helper when present, and replaces the current user's `Bezel` Run entry with the light launcher. Older installations without the bundled payload retain the external Libre setup. No execution policy changes are used. Confirm the installation paths before running.

The bundled `sensors/` payload replaces the need to open LibreHardwareMonitor or run its web server. Its scheduled task uses the current user and requires an interactive login; Bezel itself remains unelevated. Updating executables does not reconfigure startup. External Libre remains a compatible fallback.

To roll back, restore the original Run value from `startup-backup/bezel-run.txt`, unregister `Bezel-LibreHardwareMonitor`, and import the three saved XML files in Task Scheduler. Do not import legacy tasks unless their installed executable paths are still correct. Login behavior needs a real sign-out/reboot check after setup.

## Embedded sensors

Implemented in `apps/bezel-sensors-helper/Program.cs`. The payload includes only the LHM library and its runtime dependencies, with licenses and source metadata; the CLI and editor read its bounded, fresh local snapshot. See `sensors/README.md` in the installation for details and rollback.
