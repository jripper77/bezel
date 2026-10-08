# Local development and delivery

- Work in the existing repository and update `dist/bezel` in place. Do not create a new installation directory for each change.
- Every delivered update gets a new product version from `VERSION`. Build Windows releases with `scripts/windows/build-evo.ps1`, which increments the patch number and builds CLI and Studio together. Use `-Version X.Y.Z` to choose a feature release or retry the same failed build. Raw Cargo builds are fine during development, but do not deliver changed executables under an already delivered version.
- Select validation by impact. A small visual label change needs compilation and a visual smoke check, not the entire test suite. Changes to serial ownership, scheduling, sensors or shared rendering need the relevant native integration/regression tests. Broaden checks when failures or cross-component changes justify it.
- Preserve unsaved Studio documents. Build first; if Studio is still running, ask the user to save and close it before replacing the executable. Verify installed executable hashes after copying.
