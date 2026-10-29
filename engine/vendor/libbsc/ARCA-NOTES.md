# libbsc in Arca

The library sources of libbsc 3.3.12 by Ilya Grebnov (Apache-2.0, `LICENSE`),
downloaded with the user's OK on 2026-10-28 from
`https://github.com/IlyaGrebnov/libbsc/archive/refs/tags/v3.3.12.tar.gz`
(SHA-256 `3d8eee9351468bdd3fb3bc28dda0e24cfc0332997eb0210ea7bd548d77c3d194`),
unchanged. The release bundles libsais 2.10.4 (`libbsc/bwt/libsais`, Apache-2.0)
for the suffix sort. 3.3.12 (September 2025) rather than 3.4.0 (October 2026):
the newer release was a week old.

Left out: the command-line tool (`bsc.cpp`), CMake files and the CUDA sources
(`*.cu`); the two CUDA headers stay because the C++ files include them, and they
compile to nothing without `LIBBSC_CUDA_SUPPORT`. Built without OpenMP: Arca
compresses and decompresses blocks in parallel itself.

`src/lib.rs` is Arca's: compress a buffer as one bsc block, decompress it.

To update: copy the same files from a newer release, then run `cargo test` here
and the round trips in `arca-core`.
