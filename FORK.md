# Bezel Evo ? Windows improvements

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

## Card widget (first version)

Add **Card** from Widgets, or select several objects and choose **Create card
from selection** in the inspector. The card has an editable shape base and one
initial face. Use **Displayed face** to switch, and add, rename, duplicate,
reorder or delete faces. Deleting a face also deletes its objects; Undo restores
both. At least one face remains.

New widgets added while a card or one of its objects is selected join that
face. Each object's **Card membership** controls its container and face; choose
**Shared base (always visible)** for common objects, or **No card** to detach.
Select the card through Layers to manage faces. Selecting a face object through
Layers displays its face. Move, resize, align, duplicate or copy the card to act
on all its objects, including inactive faces. Locked children move with their
container; a locked card cannot be dragged. The base stays below its objects.

Themes save face selection and ownership in optional `card` and `cardMember`
metadata. Old themes remain unchanged when saved. Studio, thumbnails and Light
render the selected face and the shared base, with inherited card visibility
and opacity. A card's objects keep their existing font and stroke settings;
resizing scales their frames, and objects are not clipped to the card bounds.
Nested cards are not supported. Timed switches, entry/exit animations and software triggers are tracked in
TODO.md and are not implemented yet.

Validation: native theme round-trip and invalid ownership rejection, renderer
pixel checks for base/face visibility and inherited opacity, editor ownership,
clipboard/Undo and group transformations, plus accessible browser checks in
both UI languages and light/dark modes. Windows binaries are built together so
Studio and Light use the same theme model.

## Card face transitions

A card's **Face animation** section offers None, Fade, Slide and Flip,
150-1500 ms in the editor (themes accept up to 3000 ms), direction and a shared
base toggle. **Animate next face** selects the next face with that transition;
the selected face remains an ordinary theme edit and can be undone. Create
a card and add faces using the normal editor controls.

The native renderer transforms the entire face bitmap, including text, icons
and indicators. Flip uses perspective projection with bilinear sampling and restrained shading, and keeps text
unmirrored. Slide is clipped to the card rectangle during the transition.
Fade adds weighted premultiplied layers to avoid a dark dip between faces.
Progress lives only in the renderer, not the theme or clipboard. Changes
interrupt the previous transition instead of building a queue. First loading a
saved card shows its saved face immediately. Preview motion respects reduced
motion; animations request at most 30 frames/s and skip late frames. Hardware
transfer speed still determines the frame rate of a real display. Transition
buffers are reused. Studio forwards transition deadlines through its shared
renderer; each screen document has separate transition state, so rendering a
second screen does not reset the first card. Sensor sampling retains the
theme refresh interval. The preview subtracts rendering/IPC time from the next
frame interval instead of adding an extra pause.

Studio Live uses the native transition renderer and existing serial protocol.
Light retains the settings and shows the saved face; autonomous/timed switching
remains planned. The browser demo matches the effect poses for UI testing.
Animated cards keep face sensors sampled at the theme's normal refresh rate,
including inactive faces, so outgoing values remain available during a change.
Tests cover pose geometry, interrupted/reduced motion, theme round-trip and
validation, native mid-transition pixels and scheduling, and changing browser
frames for every effect in both languages and light/dark modes. Actual display
smoothness is still to be judged on hardware.


Studio also releases a live connection parked in another document when the
same Rev C screen is selected through its wake port. The edited document
adopts the display port after connecting, preserving unsaved edits and any
parked alias document. Startup resolves a saved wake-port selection to the
listed display key. Renderer transition state follows each document when
switching screens.


Rev C serial status reads now wait for 10 ms of silence after a recognized
`needReSend` / `renderCnt` reply, rather than 30 ms. Other replies and incomplete
status fields keep the original 30 ms interval; no-response deadlines, packet
formats, QUERY_STATUS frequency, write/drain checks and resend handling are
unchanged. This removes 20 ms of host waiting from a normal partial frame,
but USB transfer size and device rendering still limit physical frame rate.
Set `BEZEL_REV_C_FAST_STATUS=0` when launching for the original receive timing.
The animation demo button has been removed from Widgets; animation checks use
cards constructed with ordinary editor controls.


### Independent Studio frame deliveries (0.1.4)

The refresh loop submits serial I/O to an idle-expiring worker per display rather
than waiting for all displays in sequence. The existing borrowed-link state allows
one outstanding frame per display; it skips intermediate updates instead of
building a backlog. Completion notifications wake the refresh loop through a
condition variable and an epoch counter, including completions that occur before
it starts waiting. Rendering and sensor/session state remain shared. Initial Live
activation and explicit UI pushes still report their synchronous send outcome.
Link completions carry the live generation, preventing an old connection from
replacing a newer one. Shutdown and storage continue to wait for the borrowed link.
The packet protocol, framing and serial pacing are unchanged.

Validation: a held slow display permits repeated frames and the latest edit on a
fast display; four held displays start independently and one failure is isolated;
completion before waiting is not lost; stale generations are rejected. Existing
backend, storage and Windows shutdown regressions also pass. Physical animation
FPS and USB-controller contention still need measurement after installation.


### Recent Studio frame reuse (0.1.5)

Studio preview and Live can reuse one unpainted frame per document for less than
one animation interval (33.33 ms), ending earlier at the next GIF/transition
boundary. Reuse never extends that interval or moves the original drawing clock.
Theme/asset edits, sensor samples, catalog changes and motion-policy changes
invalidate it. Video backgrounds keep separate rendering because device playback,
posters and host-decoded pictures differ. The feature is opt-in at ThemeRuntime;
CLI/Light retain their existing rendering behavior. Serial framing is unchanged.

Debug logs include `render_reused` on deliveries and preview records, preview
elapsed time and initial editor-lock wait, plus refresh-loop duration, frame
preparation lock wait and age of the requested update. These measurements separate
editor contention from serial completion; no panel Hz assumption is needed.

Validation covers reuse in both directions, expiry, backward clocks, wall-clock
changes, edit/asset/sample/catalog/motion invalidation, GIF boundaries and live
cadence. Studio document isolation, native card transitions, independent workers
and source logging restrictions remain covered by their existing regressions.
The real-device FPS improvement must be measured after installation.


### Card face surfaces (0.1.6)

Native card transitions cache unprojected transparent face surfaces, separately
from the background. Preview and Live share these surfaces when video cutouts do
not change their pixels; device-video windows retain separate surfaces and masks.
The cache is bounded to 32 MiB of pixel storage across renderer scenes. Content
revisions invalidate asset/theme/sample/catalog changes; canvas, face members,
wall clock, language and readings/history/units are also checked. GIF faces use
the original drawing path, preserving their animation. The full card projection,
shading, transition timing, background composition and serial protocol are retained.
CLI/Light and Studio supply content revisions; renderer clients without revision
tracking use the original path. Debug deliveries expose face cache hits/misses,
fresh face drawing time and other card work (copy/projection/composition).

Pixel comparisons against uncached rendering cover multiple Flip poses, both
background variants, edits, readings, clock and revision invalidation, video-window
masks and animated faces. Existing native transition/document and worker regressions
remain applicable. Physical FPS improvement still requires a post-install capture.


### Unassigned image placeholders (0.1.7)

An Image object awaiting file selection can be saved and reopened. Asset
collection excludes an empty reference while preserving the object in the
manifest. Actual resource paths still pass the existing path-safety checks.
Zip and folder round trips cover placeholder preservation alongside real assets.


### Face-cache rollback and comparison (0.1.8)

Face-surface reuse is disabled by default after a reported loss of smoothness.
The default path restores the original direct face drawing loop and skips cache
keys, image timeline probes, insertion and surface copies. Existing rendering,
worker and serial timing diagnostics remain; `face_cache_enabled` explicitly
identifies the mode in Debug render statistics. The experimental path can be
selected for an A/B comparison by launching with `BEZEL_CARD_FACE_CACHE=1`.
Use the same theme, Flip settings and face-switch sequence for both captures;
the saved effect may differ from unsaved editor settings. No serial behavior,
asset-placeholder fix, document state or delivery scheduling is reverted.


### Card hierarchy in Layers (0.1.9)

- Present each card with a separate shared base and every named face, including
  empty and inactive faces; indent member objects and display member counts.
- Highlight the active face. Its header selects the card; another face's header
  switches the active face through the existing undoable command. Selecting a
  member continues to open its face and select that object.
- Keep object visibility, locking and rename controls. Member ordering arrows
  operate between members of the same face or base, preserving membership.
- The hierarchy is presentation only: saved themes retain their flat paint order
  and need no migration. Invalid/orphan references remain visible as root rows.
- Validation: focused card/hierarchy unit tests and the card editor browser test,
  including face selection, nested membership, clipboard, Undo and accessibility.


### Timed card rotation (0.1.10)

- Optional `card.rotationSeconds` (5?3600, default absent/off) schedules one
  next face at a time in the shared core runtime, independently for each card.
  The interval includes transition time; rendering uses the configured effect.
- The runtime renders transient face choices. The saved starting face, editor
  document and Undo history remain intact. A manual face/configuration change
  resets that card's interval; late frames/standby advance once without replaying
  a queue of missed transitions. Hidden/single-face cards have no timer.
- Studio pauses automatic rotation in both preview and Live while a card/member
  is selected. Deselecting restarts from the selected starting face after a full
  interval. Layer highlights describe the editing/starting face; they are not a
  playback-position display. Light runs saved timers without an editor.
- Timed inactive faces' sensors are sampled at the normal sensor cadence; incoming
  GIFs are learned when that face becomes visible. Timer and renderer deadlines
  share the existing scheduler and never start a per-frame sensor poll.
- Old themes remain unchanged and load with timers disabled. Theme parsing
  validates interval bounds; disabling rotation removes the optional setting.
- Validation: core timer/cadence regressions, native card transition/preview tests,
  theme serialization/range validation, JS playback/IPC tests, card editor browser
  checks in both languages and color schemes, and strict Clippy with the existing
  collapsible-if allowance. Three pre-existing i18n branding assertions still
  expect the upstream Bezel name; translation-key parity and lookup checks pass.
- Event priorities, media triggers and configurable override/return policies remain
  planned; this release implements interval-based playback and editing pause.
