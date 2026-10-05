# Windows improvements in this fork

This is a personal development fork of [slipalison/bezel](https://github.com/slipalison/bezel), based on `3f6144fe7dce7c14f2d79a8f23bbe27e0315e353`.
The original project remains upstream. `main` follows the original; `windows-improvements` carries the locally tested changes.

## Implemented

- **Windows serial I/O:** bounded 64 KiB writes, transfer diagnostics and a Windows write timeout; retry transient access-denied errors when opening a port. Read timeouts are restored after sending. Turing Rev C framing and command bytes are unchanged.
- **LibreHardwareMonitor sensors:** retain WMI support and fall back to the fixed `http://127.0.0.1:8085/data.json` endpoint when WMI fails. Requests disable proxies and redirects, have a 2-second timeout and a 4 MiB response limit. Enable LibreHardwareMonitor's Remote Web Server. Studio does not launch or configure the helper automatically.
- **Vector icons:** Media > Icons includes 6,220 offline Tabler Icons 3.48.0 under the MIT license, with search, click and drag insertion. SVGs render at the requested resolution. The inspector edits color, opacity, outline width and shadow blur/color/opacity. Paint and effects persist in SVG assets; each edit is undoable and independent of other copies. The MIT notice is embedded in each generated asset.

- **Dates and clocks:** system/Italian/English/Portuguese language per object, readable format presets, custom patterns, live examples and normal/uppercase/title case. These settings round-trip in theme.json; old clock fields remain readable. Runtime date language follows the system independently of Studio's UI language.
- **Editor clipboard:** Ctrl+C/Ctrl+V and toolbar buttons copy single or multiple objects within the current theme. Properties and asset references are preserved, copies get unique names/IDs and a small position offset, and each paste is one undo step. Text fields retain native shortcuts. Opening another theme clears the object clipboard.

- **Weather object:** Widgets > Weather adds a city, temperature, localised conditions and a vector icon. Search selects exact coordinates; language, Celsius/Fahrenheit, icon visibility and normal text appearance persist in the theme and survive copy/paste. Open-Meteo requests run on background workers only for visible weather objects; successful locations refresh every 10 minutes, failures retry after 1 minute, and identical coordinates share a cache. Up to 8 distinct shown locations are fetched. HTTPS endpoints are fixed; redirects/proxies are disabled, timeout is 8 seconds and JSON is limited to 1 MiB. The free service is for personal/non-commercial use; data attribution is included in the inspector.

## Validation already performed

- Actual Windows PC and Turing Smart Screen 8.8 Rev C ROM 1.90: full frame transfers and live updates completed after the serial change. Python on the same hardware served as the comparison.
- Actual LibreHardwareMonitor 0.9.6: CPU temperature, fan speed and power were read through the local server.
- 139 core tests, 27 renderer tests, 59 sensor tests, 183 Studio tests, 51 theme tests and 276 UI unit tests passed; clippy passed without warnings for renderer and Studio.
- Icon browser tests passed in four light/dark and English/Portuguese configurations, using the native Content Security Policy; they cover search, drag insertion, inspector edits and Undo. Native Rust tests check colored translucent shadow pixels.
- Clock and clipboard browser tests passed in all four light/dark and English/Portuguese configurations. Native tests verify Italian date rendering and backward-compatible theme persistence.
- Weather browser tests passed in all four configurations, including city choice, language, units, icon visibility, Undo and clipboard. Live Open-Meteo city search and current weather were verified separately. Native pixels were inspected for the multiline weather object and its vector icon. Weather rendering on the physical display is still to be confirmed.
- Other screen models and Linux runtime behavior have not been physically verified for these additions.

## Pending

- Reliable login startup of the correct Studio executable and the sensor helper.

The serial fix, sensor fallback, icon additions, dates, editor clipboard and weather are separate commits so they can be reviewed independently. No upstream pull request has been submitted yet.

Bezel remains GPL-3.0-or-later. Tabler SVGs retain their MIT notice; see `apps/bezel-studio/src/assets/tabler/LICENSE`.

Weather data: [Open-Meteo](https://open-meteo.com/) under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/); geocoding data: [GeoNames](https://www.geonames.org/). Current conditions are model-based. See [the API usage terms](https://open-meteo.com/en/terms) for public/commercial use.
