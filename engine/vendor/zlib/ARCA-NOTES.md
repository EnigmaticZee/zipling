# zlib in Arca

The Deflate encoder sources of zlib 1.3.2 by Jean-loup Gailly and Mark Adler
(zlib license, `zlib/LICENSE`), downloaded with the user's OK on 2026-10-29 from
`https://github.com/madler/zlib/releases/download/v1.3.2/zlib-1.3.2.tar.gz` and
`https://zlib.net/zlib-1.3.2.tar.gz` (both SHA-256
`bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16`), unchanged.

Why a pinned copy: most PNGs made on a Mac are written by stock zlib (ImageIO
uses level 1). Re-running the same encoder re-creates their Deflate streams
byte for byte, faster than preflate's analysis. The output must never change, so
Arca can't use the system's libz (an OS update could change it) or zlib-ng/zlib-rs
(different output). zlib 1.3.2 reproduces exactly the streams macOS's libz 1.2.12
writes (all 656 PNGs of the saved folder tested, 480 of them stock zlib).

Only the encoder is built (`deflate.c`, `trees.c`, `zutil.c` with its header
`gzguts.h`, and `adler32.c` and
`crc32.c`, which `deflate.c` links against), with `Z_PREFIX` so its symbols
(`z_deflate`, ...) can't clash with another zlib in the app. Inflating is done by
flate2 (zlib-rs).

`src/arca_zlib.c` and `src/lib.rs` are Arca's: re-deflate with fixed settings,
comparing with a target as it goes.

To update: never, unless a test shows the new release writes byte-identical
output for every setting Arca stores; archives depend on it.
