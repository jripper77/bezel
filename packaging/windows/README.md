# Bezel Evo for Windows x64

Download the Setup executable for a normal installation, or extract the
portable ZIP into a writable folder. Start `bezel-studio.exe`, choose a display
and save a theme. Studio and Light run as your normal Windows user.

The Setup offers optional **Light at Windows sign-in** and **hardware sensors**.
Only sensor-task configuration asks for administrator approval. Use the same
Windows account for that approval. No vendor startup tasks or external Libre
installations are removed. If another Bezel installation owns `Bezel-Sensors`,
its task is left unchanged; migrate/remove that installation before configuring
this reader. Sensor setup registers the reader; check actual sensor availability
in Studio. Hardware support and existing hardware-access drivers determine which
values are available; the installer does not install hardware drivers.

For the portable version, run `setup-evo-sensors.cmd` once to configure the
reader. Double-click `start-light.cmd` for Light after saving a theme and choosing
a screen in Studio. The installer-maintenance script can also enable Light at
sign-in: `powershell -NoProfile -ExecutionPolicy Bypass -File
installer-maintenance.ps1 -Mode Startup -AppDirectory "C:\path\to\Bezel Evo"
-Startup 1`. The command-line execution policy applies only to that process.
Setup's Light option and Studio's own editor-autostart setting are separate.

Save and fully exit Studio and quit Light from the tray before updating or
uninstalling. The sensor helper exits within 30 seconds after its clients close.
The installer refuses to overwrite a running installation; it does not kill the
editor. Uninstall removes only the sensor task and Light login entry that point
to this installation. User themes, preferences and caches are retained, including
the inherited internal `io.github.slipalison.bezel` data directories used for
compatibility. Bezel Evo's installed product name and publisher identify this fork.

Setup can download Microsoft's WebView2 runtime when absent. Portable Studio
requires WebView2 already installed. The sensor helper uses .NET Framework 4.7.2
or newer. Video preview/transcoding requires an available FFmpeg installation;
FFmpeg is not bundled in this first package.

Silent Setup (`/S`) preserves optional-service choices on updates and does not
enable new login entries or display a sensor elevation prompt. For a first silent
installation, configure those services afterwards. Setup and binaries currently
have no Authenticode signature.

Bezel Evo derives from Bezel by [Alison Amorim (@slipalison)](https://github.com/slipalison).
Original authorship and copyright notices are preserved. Bezel Evo is GPL-3.0-or-later;
source and release history are at https://github.com/jripper77/bezel.
The embedded LibreHardwareMonitorLib 0.9.6 and dependencies retain their licenses
under `sensors/`. Tabler and Material Design Icons retain MIT and Apache notices.
The installer template is adapted from Tauri CLI 2.12.1 under MIT; its license is
included. No personal logs, hardware aliases or development backups are shipped.

## Maintainer build

Run `scripts/windows/package-evo.ps1` from a shell with Rust 1.98, the MSVC build
tools, Node/npm and the Windows Framework C# compiler. It builds both binaries,
downloads a SHA-256-pinned LibreHardwareMonitor release, rebuilds the helper,
and uses the pinned Tauri CLI 2.12.1 to produce NSIS and ZIP packages under `dist/`.
Use `-Version X.Y.Z` to choose a version or retry; `-SkipBuild` bundles already-built
binaries matching `VERSION`. The adapted template retains Tauri's WebView2 and
upgrade handling, adds the options page, and omits user-data deletion.
