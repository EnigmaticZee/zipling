# lzfse in Arca

The sources of lzfse 1.0, Apple's reference implementation of LZFSE (BSD-3-Clause,
`lzfse/LICENSE`), downloaded with the user's OK on 2026-10-29 from
`https://github.com/lzfse/lzfse/archive/refs/tags/lzfse-1.0.tar.gz`
(SHA-256 `cf85f373f09e9177c0b21dbfbb427efaedc02d035d2aade65eb58a3cbf9ad267`, the
same as Homebrew's formula records). The command-line tool, tests and build files
are left out.

Why a pinned copy: macOS writes LZFSE in disk images (ULFO) and asset catalogs
(`Assets.car`). Re-running the same encoder re-creates those streams byte for
byte, so Arca can store them decoded; the encoder's output must never change, so
Arca can't use the system's libcompression.

One change, in two lines (`lzfse_internal.h`, `lzfse_encode_base.c`): the number
of 315-byte literal runs that may wait before the encoder emits a pending match
is a field of the encoder state (`literal_runs`, 0 meaning lzfse 1.0's 8).
macOS's libcompression today uses 3: with it, lzfse 1.0 re-creates all 193
chunks of an `hdiutil` ULFO image (84.5 MB) and 351 of the 408 MB of LZFSE in
the installed apps' `Assets.car` files (the rest: LZVN streams for inputs under
4 KB, whose encoder differs, and streams from other encoder versions). With 8 it
re-creates 127 of the 193 chunks.

`src/arca_lzfse.c` (Arca's): the encoder entry point with that setting (a copy of
`lzfse_encode_buffer_with_scratch` for inputs under 4 GB) and an exact-size
decode. `src/prefix.h` gives the library's global symbols an `arca_` prefix.

To update: never, unless a test shows the new code writes byte-identical output
for every setting Arca stores; archives depend on it.
