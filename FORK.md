# Windows improvements in this fork

This is a personal development fork of [slipalison/bezel](https://github.com/slipalison/bezel), based on `3f6144fe7dce7c14f2d79a8f23bbe27e0315e353`.
The original project remains upstream. `main` follows the original; `windows-improvements` carries the locally tested changes.

## Implemented

- **Windows serial I/O:** bounded 64 KiB writes, transfer diagnostics and a Windows write timeout; retry transient access-denied errors when opening a port. Read timeouts are restored after sending. Turing Rev C framing and command bytes are unchanged.
- **LibreHardwareMonitor sensors:** retain WMI support and fall back to the fixed `http://127.0.0.1:8085/data.json` endpoint when WMI fails. Requests disable proxies and redirects, have a 2-second timeout and a 4 MiB response limit. Enable LibreHardwareMonitor's Remote Web Server. Studio does not launch or configure the helper automatically.
- **Vector icons:** Media > Icons includes 6,220 offline Tabler Icons 3.48.0 under the MIT license, with search, click and drag insertion. SVGs render at the requested resolution. The inspector edits color, opacity, outline width and shadow blur/color/opacity. Paint and effects persist in SVG assets; each edit is undoable and independent of other copies. The MIT notice is embedded in each generated asset.

## Validation already performed

- Actual Windows PC and Turing Smart Screen 8.8 Rev C ROM 1.90: full frame transfers and live updates completed after the serial change. Python on the same hardware served as the comparison.
- Actual LibreHardwareMonitor 0.9.6: CPU temperature, fan speed and power were read through the local server.
- 26 renderer tests, 183 Studio tests and 271 UI unit tests passed; clippy passed without warnings for renderer and Studio.
- Icon browser tests passed in four light/dark and English/Portuguese configurations, using the native Content Security Policy; they cover search, drag insertion, inspector edits and Undo. Native Rust tests check colored translucent shadow pixels.
- Other screen models and Linux runtime behavior have not been physically verified for these additions.

## Pending

- Reliable login startup of the correct Studio executable and the sensor helper.
- Italian weekday/month names and configurable date language. The existing clock pattern field can change formatting, but the current build supports English and Portuguese names.

The serial fix, sensor fallback and icon additions are separate commits so they can be reviewed independently. No upstream pull request has been submitted yet.

Bezel remains GPL-3.0-or-later. Tabler SVGs retain their MIT notice; see `apps/bezel-studio/src/assets/tabler/LICENSE`.
