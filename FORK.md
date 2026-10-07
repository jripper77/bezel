# Windows improvements in this fork

This is a personal development fork of [slipalison/bezel](https://github.com/slipalison/bezel), based on `3f6144fe7dce7c14f2d79a8f23bbe27e0315e353`.
The original project remains upstream. `main` follows the original; `windows-improvements` carries the locally tested changes.

## Implemented

- **Multiple displays and sensor sharing:** Studio remembers per-screen themes,
  orientation and live selection. Light's `--screens-config` starts the saved
  active screens together and shares sensor sampling rather than creating a
  separate hardware reader for each screen.
- **Libre diagnostics and startup:** Studio reports reader health and can restart
  the configured task. Bounded logs describe source selection, missing readings
  and helper updates. A previous session's heartbeat no longer cancels the
  helper's startup grace period; the login launcher maintains a heartbeat while
  waiting for sensors. Moving the installation requires updating its Windows
  startup paths; changed COM ports require updating the saved screen selection.
- **Local video preview dialogs:** Media and Storage offer a separate player
  without editing the theme or opening the serial link. Storage reads only the
  catalogued local copy and offers original-file association when absent. The
  player releases its source on close and ignores late loading responses.
  Payloads are limited to 64 MiB, codecs depend on WebView, and device-only
  videos cannot be downloaded through the current protocol.
- **Windows shutdown:** Light applies the saved shutdown action through the
  worker's existing screen connection instead of releasing it afterwards.
  Screens without stored-media standby support are turned off.

- **Windows light tray and Studio handoff:** `bezel run --tray` adds a native tray menu with Open Studio and Quit, plus double-click to open the editor. The light process keeps no WebView and pauses its render worker while Studio owns the display. Opening Studio directly also yields the light connection cleanly before restoring live mode. Closing the editor with light waiting asks about unsaved edits, fully exits Studio and resumes light with the saved theme, screen and sensor options. Per-user OS file locks coordinate ownership and release automatically on process exit; no protocol bytes or other processes are changed. Studio preferences now include a Windows-only Light-on-close switch, enabled by default (including older settings). Closing standalone Studio starts the saved theme in Light before releasing display ownership; with the switch off, live Studio stays in the tray. Cancelled unsaved-edit dialogs never launch Light, launch failures keep Studio open, and explicit Quit stops both runtimes. Ordinary CLI runs retain their existing behavior. Windows CLI resources include the Common Controls v6 manifest required by the tray dependencies.
- **Windows serial I/O:** bounded 64 KiB writes, transfer diagnostics and a Windows write timeout; retry transient access-denied errors when opening a port. Read timeouts are restored after sending. Turing Rev C framing and command bytes are unchanged.
- **Integrated Windows hardware sensors:** the bundled headless .NET Framework helper embeds unmodified LibreHardwareMonitorLib 0.9.6 and its nine library/runtime DLLs, with original licenses, dependency metadata and hashes. An elevated per-user `Bezel-Sensors` task reads hardware; CLI and Studio stay unelevated and wake only this configured task on demand. The helper publishes an atomic per-user local snapshot, and exits after 30 seconds without Bezel clients. The sensor worker prioritizes snapshots with a six-second freshness limit and 4 MiB bound; WMI and the fixed external `http://127.0.0.1:8085/data.json` remain fallbacks. Sensor IDs, raw units and custom names stay compatible. Only name aliases are imported: persisted fan-control settings are ignored and the helper never invokes control APIs. One-time setup verifies CPU temperature before disabling the previous external Libre login task. It does not install hardware drivers.
- **Vector icons:** Media > Icons includes 6,220 offline Tabler Icons 3.48.0 (MIT) and a 375-icon hardware subset of Material Design Icons 7.4.47 (Apache 2.0), with collection filters, search, click and drag insertion. Italian searches recognise electrical and cooling terms, ignore connective words and accents, and rank primary symbols before decorated variants. SVGs render at the requested resolution. The inspector edits color, opacity, outline width and shadow blur/color/opacity. Paint and effects persist in SVG assets; each edit is undoable and independent of other copies. Tabler assets retain the MIT notice; MDI assets embed their own license and modification notice in SVG metadata.

- **Dates and clocks:** system/Italian/English/Portuguese language per object, readable format presets, custom patterns, live examples and normal/uppercase/title case. These settings round-trip in theme.json; old clock fields remain readable. Runtime date language follows the system independently of Studio's UI language.
- **Editor clipboard:** Ctrl+C/Ctrl+V and toolbar buttons copy single or multiple objects within the current theme. Properties and asset references are preserved, copies get unique names/IDs and a small position offset, and each paste is one undo step. Text fields retain native shortcuts. Opening another theme clears the object clipboard.

- **Weather object:** Widgets > Weather adds a city, temperature, localised conditions and a vector icon. Search selects exact coordinates; language, Celsius/Fahrenheit, icon visibility, outline/filled icon style, icon size, icon/text spacing and normal text appearance persist in the theme and survive copy/paste. Open-Meteo requests run on background workers only for visible weather objects; successful locations refresh every 10 minutes, failures retry after 1 minute, and identical coordinates share a cache. Up to 8 distinct shown locations are fetched. HTTPS endpoints are fixed; redirects/proxies are disabled, timeout is 8 seconds and JSON is limited to 1 MiB. The free service is for personal/non-commercial use; data attribution is included in the inspector.

- **Ring appearance:** optional two-color gradient along the full arc, with an adjustable 0-100 midpoint. Colors stay tied to the full scale as the sensor value changes. Clockwise/counterclockwise direction, round caps, separated blocks and alpha are supported. The inspector includes **Test: show 100%** for every ring, including solid rings, overriding only the drawn fraction. Uncheck it to restore sensor-driven rendering. The test flag and gradient settings persist in the theme and work with Undo and copy/paste. Existing imported linear gradients remain unchanged.

- **Stored screen video backgrounds and shape windows:** Screen > Storage offers **Set as background** for a single selected video, even while the theme is live. It saves the exact device path and loop flag without downloading, uploading or decoding the file on the host. The theme inspector exposes looping, replacement and the outside color when windows are shown. Shapes can reveal the full-screen device video through rectangular, rounded or elliptical windows; all windows share its playback and keep the original video coordinates. A linear transparency mask offers start/end opacity and direction for normal shapes and video windows, including their borders. Layer order, element opacity, Undo, clipboard and theme persistence are preserved. The editor displays an explicitly labelled placeholder because the protocol cannot read back decoded frames. Selected device files are protected as theme videos by cleanup.

## Validation already performed

- Latest Windows update: 100 CLI library tests, 141 core tests, 63 sensor tests
  (one ignored) and 187 Studio tests passed. All 282 UI unit tests and 28 browser
  checks for multiple screens, Libre health/restart and video previews passed
  across light/dark and English/Portuguese configurations.
- On the test PC, correcting the renamed installation's login path and the
  8.8-inch screen's changed COM port restored both displays and 449 sensor
  readings. Confirmation at the next Windows login remains pending.

- Actual Windows PC and Turing Smart Screen 8.8 Rev C ROM 1.90: full frame transfers and live updates completed after the serial change. Python on the same hardware served as the comparison.
- Actual LibreHardwareMonitor 0.9.6: CPU temperature, fan speed and power were read through the local server.
- 141 core tests, 30 renderer tests, 59 sensor tests, 184 Studio tests, 53 theme tests, 97 CLI tests and 278 UI unit tests passed; clippy passed without warnings for renderer and Studio.
- Icon browser tests passed in four light/dark and English/Portuguese configurations, using the native Content Security Policy; they cover search, drag insertion, inspector edits and Undo. Native Rust tests check colored translucent shadow pixels.
- Clock and clipboard browser tests passed in all four light/dark and English/Portuguese configurations. Native tests verify Italian date rendering and backward-compatible theme persistence.
- Weather browser tests passed in all four configurations, including city choice, language, units, icon visibility, Undo and clipboard. Live Open-Meteo city search and current weather were verified separately. Native pixels were inspected for the multiline weather object and its vector icon. Weather rendering on the physical display is still to be confirmed.
- MDI pump selection, colour/shadow editing and Undo passed browser checks in all four configurations; Rust verifies embedded Apache notices, valid SVG, preserved metadata and rendered thumbnails.
- Ring and weather configuration browser tests passed in all four UI configurations (8 checks). Native tests verify ring gradient pixels, direction, shifted midpoint, caps, block gaps, missing sensors and test-at-100 behavior, plus theme round trips and old theme defaults.
- Stored-video runtime checks passed (18 tests), including exact path selection, loop/once, stop, missing files and unsupported screens. Native pixel checks passed for window alpha, rounded/ellipse masks, border and layer order, linear opacity and direction; browser checks passed in four UI configurations for Storage selection, loop, windows, fade controls, Undo and clipboard. Physical video masking and partial alpha blending on ROM 1.90 still need confirmation.
- Other screen models and Linux runtime behavior have not been physically verified for these additions.
- Windows light runtime: the saved Midnight theme is running through the CLI on COM6, without Studio, with roughly 48 MiB working set observed. The sensor command reads CPU temperature, power and fan speed from the installed LibreHardwareMonitor. The three legacy Turing login tasks were removed; the Bezel Run entry now launches the CLI wrapper and LibreHardwareMonitor has an enabled per-user elevated login task. Actual login startup still needs a reboot/sign-out check.
- Light/Studio handoff was checked on the actual COM6 installation: direct Studio launch released the CLI connection, Studio restored the saved theme live, and clicking the native window's X exited Studio and resumed the same CLI process with the saved theme. Cooperative ownership, timeout cleanup and duplicate-instance guards passed native file-lock tests; the unsaved close decision passed its Studio test. Visual tray menu interaction remains to be confirmed by the user.
- Embedded sensor helper: 60 sensor tests passed (one ignored), including exact IDs, invariant values, null values, freshness and size bounds; the C# serialization/control-setting self-test passed and clippy passed for sensors, CLI and Studio. On this PC, the external Libre process/server was stopped and its login task disabled, while the bundled elevated helper continued supplying CPU temperature, power and fan speed through the local snapshot. Studio's live theme preview also showed CPU temperature without the external GUI. The configured sensor task can be started from the normal unelevated account. A real login/reboot check remains pending.

## Pending

- `scripts/windows` provides a saved-theme light tray launcher and the administrator setup for the bundled sensor helper. Both were configured locally; verifying a real login remains pending. The external Libre executable and its backed-up task remain available for rollback.

The serial fix, sensor fallback, icon additions, dates, editor clipboard and weather are separate commits so they can be reviewed independently. No upstream pull request has been submitted yet.

Bezel remains GPL-3.0-or-later. Tabler SVGs retain their MIT notice; see `apps/bezel-studio/src/assets/tabler/LICENSE`.

Weather data: [Open-Meteo](https://open-meteo.com/) under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/); geocoding data: [GeoNames](https://www.geonames.org/). Current conditions are model-based. See [the API usage terms](https://open-meteo.com/en/terms) for public/commercial use.

The bundled MDI hardware subset is Apache 2.0; see `apps/bezel-studio/src/assets/mdi/LICENSE` and `NOTICE`. Brand/logo icons are excluded. The Tabler catalog keeps its MIT license.
