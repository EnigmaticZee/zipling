# Local patches

This is `liblzma-sys` 0.4.9 from crates.io (bundling xz 5.8.4), trimmed to the
files its build script compiles, with two speed patches. Neither changes a single
output byte; `cargo test` and a byte comparison of compressed blocks confirm it.

1. `config.h` defines `TUKLIB_FAST_UNALIGNED_ACCESS` on arm64 and x86-64, as
   xz's own configure script does. The crate's hand-written `config.h` leaves it
   out, so the BT4 match finder compared lengths byte by byte: LZMA `-9e` was
   1.18× (binaries) to 1.27× (text) slower per block.
2. `xz/src/liblzma/lz/lz_encoder_mf.c` prefetches the hash bucket of the
   position 4 bytes ahead in `lzma_mf_bt4_find` and `lzma_mf_bt4_skip`
   (`bt4_prefetch_ahead`). With 10 encoders running at once this saves ~5% of
   cycles. Prefetching both children of each tree node was tried as well and
   was 15-20% slower.

To update: copy the new crate from `~/.cargo/registry/src/*/liblzma-sys-*`,
keep the same files (`Cargo.toml`, `build.rs`, `config.h`, `src/`, licences,
`xz/src/liblzma/{api,common,lzma,lz,check,delta,rangecoder,simple}`,
`xz/src/common/*.h` and the two `tuklib_*.c` files `build.rs` names), and
reapply both patches (search for "Arca patch").
