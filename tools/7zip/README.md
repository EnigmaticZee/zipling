# 7-Zip helper

Zipling reads ZIP, 7z, RAR, TAR, DMG, ISO and about 60 other formats through
`7zz`, the official 7-Zip command-line program, run **unmodified as a separate
process**. Zipling's own code never links against 7-Zip, so it stays under its own
terms. A damaged or hostile archive can crash the helper, never Zipling.

## Build

```bash
tools/7zip/build.sh
```

The script downloads the pinned source release (7-Zip 26.05,
`https://github.com/ip7z/7zip/releases/download/26.05/7z2605-src.tar.xz`, SHA-256
`0b6080b8c3e7a579161d2d495224c0e51c467576cb4c7048b0ab485ef44cc22e`), refuses it if
the checksum differs, and builds `tools/7zip/bin/7zz` with 7-Zip's own macOS
makefile (Apple silicon, macOS 11 or later). The Xcode build embeds it in
`Zipling.app/Contents/Helpers/7zz`. Debug builds without it fall back to a
Homebrew `7zz`. Release builds fail without it.

## Licence obligations

7-Zip is copyright Igor Pavlov. Most of it is under the GNU LGPL 2.1 or later. A
few files are BSD-licensed, and the RAR decoder is under the LGPL **plus the unRAR
licence restriction**: its code may be used to extract RAR archives, but not to
re-create the RAR compression algorithm. Zipling only extracts RAR.

When distributing Zipling:

- Ship 7-Zip's `License.txt` with the app and credit 7-Zip in the app and on the
  legal page.
- Offer the exact source of the shipped helper. The pinned URL and checksum above
  identify it.
- Ship the helper unmodified and as a separate, replaceable program.
