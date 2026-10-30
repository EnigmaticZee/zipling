# LZMA SDK in Arca

Files from the 7-Zip 26.05 source release (`7z2605-src.tar.xz`, SHA-256
`0b6080b8c3e7a579161d2d495224c0e51c467576cb4c7048b0ab485ef44cc22e`, the same
release `tools/7zip/build.sh` pins), unchanged. Every one of them carries Igor
Pavlov's "Public domain" notice; none of 7-Zip's LGPL code is included.

- `C/`: LZMA and LZMA2 encoder and decoder, the binary-tree match finder and its
  threaded variant (`LzFindMt`), allocation, CPU detection, threads.
- `Asm/arm64/LzmaDecOpt.S`: the hand-written ARM64 decoder loop 7-Zip's own
  macOS build uses (`-DZ7_LZMA_DEC_OPT`).
- `src/arca_lzma.c`: Arca's two-function shim (encode a buffer as one solid LZMA2
  stream; decode it). `src/lib.rs`: the Rust side.

To update: copy the same files from a newer release and re-run `cargo test`
here (round trip, identical output with one or two threads, damaged input).

One change to the SDK's files: `LzmaEnc.c` carries a research tap under
`#ifdef ARCA_TOKEN_HOOK` (set by the `token-hook` feature) that reports every
parse decision to a hook before it is range-coded. Release builds don't define it,
and compile the file as released.
