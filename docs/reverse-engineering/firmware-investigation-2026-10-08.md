# Rev C ROM 1.90: investigation of file download support

## Scope and outcome

The goal is to determine whether a stored video can be read back from the user's
8.8-inch Rev C display (`chs_88inch.dev1_rom1.90`) for Studio's preview.
This investigation is static only: no firmware was flashed and no experimental
opcodes were sent to the display.

**Unresolved:** the correct Rev C ROM 1.90 executable has not been obtained.
The absence of a file-download command in the host implementations does not prove
that the device firmware lacks one.

## Files examined

The local `C:/Users/danie/Desktop/TURZX-V3.1.0-ENG/fw` directory contains:

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| `turzx_88inch_0015` | 93,380 | `2842795cc04e27fa2d2618d9e7dc7bbe505ea61c3d166126f881206e803a8a10` |
| `turzx_8inch_0015` | 93,380 | same |

Both are identical ELF32 little-endian MIPS executables. The embedded version
string is `turzx_0001_0015`. They retain function symbols, including `main`,
`GetDir`, `GetFileSize`, `DeleteFile`, `save_file`, and `get_file_mem`.

The official English application archive was downloaded from the link on
[the vendor's 8.8-inch download page](https://www.turzx.com/2025/05/26/88_inch/):
`https://down.turzx.com/TURZX-V3.1.0-ENG.rar`.
The HTTP response reported 492,363,733 bytes and Last-Modified
`Mon, 29 Jun 2026 05:34:21 GMT`. Its 270-entry directory lists only the two
firmware files above under `fw/`; it contains no other filename matching
`firmware`, `rom1`, or `update.app`. This is a package-directory check, not
proof that no embedded payload exists elsewhere.

## Protocol identification

Disassembly of `main` shows:

- `0x402cc0`: checks byte `0x1fe` of the received buffer against `0xa1`.
- `0x402ccc`: loads byte `0x1ff`; the next branch checks against `0x1a`.
- `0x402ce4`: calls `des_decrypt` at `0x40b354`.
- `0x402cf8` onward: checks decrypted header bytes against `0x1a, 0x6d`.

This matches the encrypted 512-byte TUR_USB protocol documented in
[protocol-turing-usb.md](protocol-turing-usb.md), rather than the Rev C
`ef 69` / 250-byte protocol. Therefore these binaries are evidence about a
different protocol family and cannot answer whether Rev C ROM 1.90 supports
reading a stored file.

`get_file_mem` alone is not evidence of a host download command: its disassembly
performs local file open/seek/allocation/read operations. Establishing a download
requires following the command handler through a response written to USB/serial.

## Next evidence needed

Obtain the exact `chs_88inch` ROM 1.90 application/update image or matching SDK
from a verified source, then analyze its command dispatch and outbound writes.
The existing Rev C opcode list includes unused values such as `0x67`, but these
must not be assumed to implement download without evidence of their semantics,
payload and reply format.

Until that evidence exists, automatic read-back from the display remains an
unimplemented capability, not a proven hardware impossibility. Existing local
copies can be used for Studio preview independently of this investigation.
