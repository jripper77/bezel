# IBM Plex Sans and Mono (woff2 subset)

Five unmodified woff2 files from IBM's official Plex release
(https://github.com/IBM/plex), taken from the `fonts/complete/woff2/`
folder of the npm packages published by IBM:

- `@ibm/plex-sans` 1.1.0 —
  https://registry.npmjs.org/@ibm/plex-sans/-/plex-sans-1.1.0.tgz
  (package SHA-256 `c3818979c2a2c82927ea3e7c485e389a18de0b7462887dd4f7acc9a525568962`)
- `@ibm/plex-mono` 2.5.0 —
  https://registry.npmjs.org/@ibm/plex-mono/-/plex-mono-2.5.0.tgz
  (package SHA-256 `55b5ffcfcd5e9db36ef2070b2b07a573434ca02c6f3be2b921c0b88fff29acad`)

Only the weights the Studio uses are bundled (Sans 400/500/600, Mono 400/500);
bolder requests resolve to SemiBold. The files are served from the app itself,
so the CSP stays unchanged and no font CDN is contacted.

| File | SHA-256 |
|---|---|
| `IBMPlexSans-Regular.woff2` | `ba711a3085ff9f27440b6b9c4550cfc47c97bf36591d5da958b975bb3add8c1a` |
| `IBMPlexSans-Medium.woff2` | `5660f8a658f8bb50dbc005232f885eadffd2bc1c235c4f6fbb63469d1f9cde6d` |
| `IBMPlexSans-SemiBold.woff2` | `f78048030eab62e860efa39a0df79e2e5581bf122eb95b9bc42c0b8a4988d205` |
| `IBMPlexMono-Regular.woff2` | `ba204497f16b6d334cee9d1e963a831b73e3a56e1d6300a8489d18df7214b350` |
| `IBMPlexMono-Medium.woff2` | `33faf307fa6031fb4062276d7320a6d632de890cbb347576fd80cfa01077bc25` |

Copyright © 2017 IBM Corp. with Reserved Font Name "Plex". Licensed under the
SIL Open Font License 1.1; see `LICENSE` (copied verbatim from the packages).
