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

Light checks for a gap of at least ten seconds in its tray event loop. After
such a gap (including sleep), it stops the old render workers, restarts the
configured sensor task on a background thread and reopens the saved screens.
This is a pause watchdog, not a native suspend notification; shorter pauses
continue through the existing serial reconnect path. Recovery also waits while
Studio owns the display. The display endpoint and wake MCU of a Rev C screen
count as one physical screen when starting multiple workers.

Implemented in `apps/bezel-sensors-helper/Program.cs`. The payload includes only the LHM library and its runtime dependencies, with licenses and source metadata; the CLI and editor read its bounded, fresh local snapshot. See `sensors/README.md` in the installation for details and rollback.

Sensor recovery after standby now waits for a new embedded snapshot, with a
60-second health deadline and bounded retries for tasks that exit immediately.
Studio and Light serialize recovery through a per-user mutex. The helper uses
mutex ownership instead of mutex-name existence and can acquire an abandoned
instance lock after task termination. A sensor-worker watchdog also recovers a
missing/stalled embedded reader in Studio and CLI without blocking the UI.
Startup grace/cooldown is 90 seconds; a snapshot must be at least 60 seconds old
before that watchdog restarts it. Accepted readings still expire after six
seconds. The task remains the existing elevated Bezel-Sensors task.


## Bezel Evo release versions

`VERSION` is the local product release number, shown in Studio's header and
window title and by `bezel --version`. It is independent of theme schema and
inherited crate/package versions. Every delivered update must change it.

From a shell with Cargo available, run `scripts/windows/build-evo.ps1` to
increment the patch number and build Studio and Light together. Use
`-Version 0.2.0` for an explicitly chosen release, or the same version to retry
a failed build. The script builds into the existing `target/release` directory;
copy verified executables into the existing `dist/bezel` after Studio closes.
Development builds may use Cargo directly; do not deliver them under the last
installed release number.

Validation follows the change: compile and visually check small UI changes;
run relevant integration tests for serial ownership, timing and sensors.
The complete suite is reserved for broad changes or unresolved regressions.

## Public Windows packages

`package-evo.ps1` builds the VERSION release, rebuilds sensors from the pinned
LibreHardwareMonitor 0.9.6 archive and creates Setup/portable/checksum files in
`dist`. It does not package the local installation's logs or personal data.
Use `-Version X.Y.Z` to retry a release, or `-SkipBuild` with matching binaries.
Sensor aliases are excluded by default; the personal helper build supports the
explicit `-ImportSensorNames` switch when needed.

`installer-maintenance.ps1` manages only its own `BezelEvoLight` login entry and
an owned `Bezel-Sensors` task. Its `Check` mode blocks running installations;
it does not kill Studio or Light. Setup uses process-scoped execution-policy
bypass for the bundled scripts and never changes the machine's policy.
The old `configure-startup.ps1` migration remains a separate manual tool and is
not included in the public package. See [the installer guide](../../packaging/windows/README.md).

`test-installer.ps1` tests silent installation, replacement upgrade, payload
hashes, preservation of user data and uninstall in `target/installer-smoke`.
It refuses to run if the package's installation registry keys or shortcuts
already exist. `.github/workflows/windows-evo-package.yml` is a manual build
workflow that uploads these packages and the corresponding source as artifacts;
it does not publish automatically.
