# The engine as it stands (1 November 2026)

Rust workspace in `engine/`: `arca-core` (the engine), `arca-cli` (the `arca`
command used by the benchmarks), `arca-ffi` (the C interface the app links),
`vendor/` (libbsc, the LZMA SDK, zlib, LZFSE, preflate-rs, pinned and patched where
noted). About 11,000 lines of our own code after the cleanup of 1 November 2026.

## The pipeline (`arca-core/src/pack.rs`)

```
scan ─► sniff (parallel) ─► order by content class ─► per window:
  hash files (parallel) ─► whole-file dedup ─► transform + content-defined chunking (parallel)
  ─► chunk dedup + lane routing ─► blocks per lane ─► compress blocks (parallel) ─► one streaming write
```

- Files are read once. The first few KiB are sniffed for the content class
  (`classify.rs`); extensions only decide the reading order.
- Windows bound memory: file tasks may run 512 MiB ahead of the packer, and the
  number of blocks in flight is bounded by a memory budget, half of the Mac's
  physical memory unless told otherwise. Block compression of one window overlaps
  with the transforms of the next.
- Every file is hashed (keyed POLYVAL, `hash.rs`, a random key per archive stored
  in the index). The first task to hash a content claims it; duplicates become
  references. Files of 1 MiB and more are cut into content-defined chunks
  (128 KiB to 2 MiB, 512 KiB average, FastCDC) and duplicate chunks are stored once.
- Blocks are solid (many files or chunks in one block) and independent, so they
  compress and decompress in parallel. A block takes two zstd workers when fewer
  than half the cores are busy, one otherwise; that is why a Default archive's size
  can differ by 0.05 to 0.3% from one run to the next.
- Extraction (`unpack.rs`) decodes blocks in parallel on all cores, writes files as
  their chunks arrive, and verifies every block's hash before and after decoding.
  Nothing is written that did not verify.

## Content classes and lanes

Classes (`classify.rs`, by bytes): Text, Mach-O, JPEG, Deflate containers (ZIP,
PNG, PDF, gzip, disk image `.dmg`, asset catalog), already-compressed media and
archives, Code (ELF, PE, static libraries, Java classes, WebAssembly, LLVM
bitcode, tar), Binary (everything else).

Lanes (where a chunk's bytes go, each lane filling its own blocks with its own codec):

| Lane | Holds | Why separate |
|---|---|---|
| Text | text files, expanded PDF text streams | wants the expensive match finders and long-distance matching |
| Binary | Mach-O after the branch filter, generic binaries, re-coded JPEG residue | bigger tables beat long matching here |
| Code | static libraries, ELF, PE, tar (Default and up) | repeats are long and far apart; BWT with LZP wins |
| Image | PNG pixel rows (Max, Ultra; a few images at Default) | its own codec and block size |
| Quick | disk images at Default | hundreds of MB of mostly app bundles: zstd 5, not 12 |
| Store | media no coder touched | zero CPU |

Mach-O stays in the binary lane at Default and Max: on real apps long matches
bought almost nothing (212.61 to 212.55 MiB on the saved folder for +15 s), and
LZMA-class codecs cost 6 to 9 times the make time for 9 to 15% of size (measured
1 November 2026, not taken). At Fast, Mach-O and packed files go to a zstd -1 lane.

## Transforms (`arca-core/src/transform/`)

Each transform is undone before compression and redone on extraction; the forward
pass is verified by running the inverse before the result is accepted.

- **JPEG**: our coefficient coder (`jpegrans.rs`): the Huffman layer is decoded and
  the DCT coefficients re-coded with static-table rANS, about 20% smaller, 388 of
  388 test photos exact, 100,000 mutants without a panic. Lepton (`jpeg.rs`,
  Microsoft's Rust port) takes what ours declines, such as progressive JPEGs, and
  runs when ours saves under 15%. Default and up. Fast stores photos through zstd -1.
- **Deflate** (`deflate.rs`): preflate re-creates the Deflate streams inside ZIP,
  Office, PNG, PDF and gzip so their contents can be compressed properly; zlib and
  LZFSE streams are replayed exactly. Disk images are expanded chunk by chunk under
  a rebuild budget (128 search steps per byte; a few photo-like PNGs from level-9
  encoders are left alone). Search depth 256 at Default, 4,096 at Max and Ultra.
- **arm64 and x86 branch filters** (`arm64.rs`): BL and ADRP targets (and x86
  CALL/JMP) rewritten to absolute form so repeats line up. All levels.
- **PNG pixels** (`pixels.rs`): rows unfiltered and coded with our pixel codec,
  Max and Ultra; at Default only where it saves at least 50 bytes per... (see the
  `png_pixels_per_byte_saved` option) and otherwise off.
- **Disk images**: expanded from Default up.

## Codecs (`arca-core/src/codec.rs`)

| Codec | Used for | Notes |
|---|---|---|
| zstd | Fast's lanes; Default's text (15) and binary (12) lanes; the quick lane (5); the fallback of Default's code lane (12); the index | long-distance matching on text; off on binaries, which get bigger tables (`FAST_TABLES` 18/17 at level 3); a block is cut into 32 MiB jobs with two workers when cores allow; a block that shrinks past 8:1 without long matching is tried again with it |
| LZMA2 | Max's binary and image lanes (fb 32), Ultra's (fb 273) | the vendored LZMA SDK encoder with its match finder in a second thread, deterministic output; dictionary = the block |
| bsc | text and code lanes at Max and Ultra; the code lane at Default | libbsc: LZP (18/128 for text, 24/64 for code) then BWT, adaptive QLFC; a code-lane block under 16:1 falls back to the lane's fallback codec (LZMA2 fb 273 at Max and Ultra, zstd 12 at Default), decided on a 4 MiB sample for big blocks |
| lzt | research only (`ARCA_EXP_LZT`) | LZMA's parse coded with per-context tables and rANS: within 3 to 5% of LZMA at twice its decode speed on Silesia, 15 to 21% bigger on PDFs, source and apps; see `research-index.md` |
| store | the store lane and any block a codec would expand | |

Format codec tags: 0 store, 1 zstd, 4 LZMA2, 5 bsc, 6 lzt. Tags 2 (xz) and 3
(PPMd) are reserved: those codecs were removed on 1 November 2026 and an archive
naming them is refused with a clear error.

## The four presets (`PackOptions::preset`)

| | Fast | Default (Balanced) | Max | Ultra (experimental, not in the app) |
|---|---|---|---|---|
| Text lane | zstd 3, long matching | zstd 15, long matching | bsc 1 | bsc 1 |
| Binary lane | zstd 3, big tables | zstd 12, long matching | LZMA2 fb 32 | LZMA2 fb 273 |
| Code lane | (binary lane; Mach-O and packed files to a zstd 1 lane) | bsc 2 with zstd 12 fallback | bsc 2 with LZMA2 fb 273 fallback | same as Max |
| Image lane | zstd 1 (stored photos) | binary codec | LZMA2 fb 32 | LZMA2 fb 273 |
| Quick lane | | zstd 5 (disk images) | | |
| Blocks | 8 MiB | 64 MiB; shrunk to one per core under 64 MiB of input | 64 MiB; 32 MiB from 512 MiB of input; text 32 MiB; blocks may split in two | 64 MiB |
| Transforms | branch filters only | all; preflate depth 256 | all; depth 4,096; PNG pixels | same as Max |
| Index | zstd 9 | zstd 9 | zstd 19 | zstd 19 |

Why each setting is where it is, in the numbers that decided it (app benchmark of
31 October and 1 November 2026, `bench/report-2026-10-31-apps.md`):

- **Fast** beats 42 of 45 rival fast settings; the three losses are the compress
  time on Silesia against zstd -3 (0.17 against 0.15 s, the CPU floor for that
  ratio) and Zipgeist's zstd -1, and WinRAR Fastest's size on disk images. Photos
  are stored through zstd -1 because re-coding them took 4 times as long.
- **Default** is the middle: smaller than every normal setting on most sets, 2 to
  12 times faster to compress and 2 to 7 times faster to extract than 7-Zip Normal
  everywhere, 2 to 15% larger than it on apps, source, Silesia and third-party
  apps. Wins 25 of 45 normal rows. The code lane (1 November) took source from
  11.20 to 9.46 MB at no cost in time; bsc on the text lane would add 2 rows and
  make compression 37 to 54% faster where text is, but extraction 1.3 to 4.6 times
  slower there, and was not taken.
- **Max** wins 36 of 54 best rows and beats 7-Zip Ultra on size on every set, in
  40% of its time on the mixed folder; its losses are compress time where the
  transforms buy 17 to 52% (PDFs, disk images) and extraction on disk images.
- **Ultra** is 0 to 3% smaller than Max for 5 to 117% more time. It stays off
  the app until the context-mixing work (see `open-problems.md`).

Mixed folder (726 MB, the website's headline set), the app's clock:

| | Size | Compress | Extract |
|---|---|---|---|
| Zipling Fast | 281.1 MB | 0.51 s | 0.82 s |
| Zipling Default | 214.3 MB | 8.8 s | 1.62 s |
| Zipling Max | 192.6 MB | 19.3 s | 2.55 s |
| 7-Zip Ultra | 235.0 MB | 48.5 s | 3.84 s |
| Finder | 324.6 MB | 24.1 s | 7.90 s |

Level for level against the rival setting of the same class: 131 of 198 rows;
any Zipling level against each rival result: 135 of 144.

## The file format (`format.rs`, `wire.rs`), version 3

```
header   12 bytes: magic "ARCA\0\r\n\x1a", version u16, flags u16
blocks   each lane's compressed blocks, in the order they closed
index    zstd-compressed: created time, the archive's hash key, entries (paths
         front-coded, times and object numbers delta-coded), objects (size,
         transform, content hash, stored length, chunk list), chunks (block,
         offset, length), blocks (codec, offsets, lengths, checksum, raw hash)
footer   64 bytes: index offset and lengths, BLAKE3-128 of the index, magic "ARCAEND\0"
```

The footer is last so a 100 GB archive opens by reading its end. Every block
carries a hash of its compressed bytes (checked before decoding) and of its raw
bytes (checked after). Transform tags: 0 none, 1 and 4 arm64 (4 with an x86
list), 2 Lepton, 3 Deflate, 6 our JPEG coder; 5 is reserved (the removed light
coder). Versions 1 and 2 were research layouts (BLAKE3 hashes, row-wise index);
their readers were removed on 1 November 2026 and nothing released wrote them.
The rule for changes: add fields and sections, never move or redefine existing
ones, so an older app opens a newer archive; a new codec is refused by an older
app with a clear message.

## Switches, flags and tools

Environment switches read by the engine (research `ARCA_EXP_*`, debugging
`ARCA_DEBUG_*`): `ARCA_EXP_LZT` (lzt on Max's lanes), `ARCA_EXP_LEPTON` (Lepton
alone on photos), `ARCA_EXP_BSC_FALLBACK=zstd:12|lzma2:7|lzt:7` (a bsc lane's
fallback), `ARCA_EXP_FAST_TABLES=h,c`, `ARCA_EXP_OVLOG`, `ARCA_EXP_FB` (LZMA2
match length), `ARCA_EXP_PIXELS`, `ARCA_EXP_PIXEL_GAIN`, `ARCA_EXP_CAR` (asset
catalogs), `ARCA_EXP_ENCODERS`, the deflate knobs `ARCA_EXP_CAP`,
`ARCA_EXP_STEPS`, `ARCA_EXP_DMG_BUDGET_MS`, `ARCA_EXP_NO_LZFSE`,
`ARCA_EXP_NO_REPLAY`; `ARCA_DEBUG_TIMING` (block timeline), `ARCA_DEBUG_ROUTING`
(one line per file), `ARCA_DEBUG_PROFILE` (CPU per stage), `ARCA_DEBUG_DEFLATE`,
`ARCA_DUMP_BLOCKS=dir` (every block's raw bytes, for codec experiments),
`ARCA_DUMP_INDEX`, `ARCA_MEM_BYTES`, `ARCA_FUZZ_DIR` and `ARCA_FUZZ_ROUNDS` (the
JPEG coder's fuzzer), `LZT_STATS`, `LZT_CAP`.

CLI (`arca`): `pack OUT INPUTS -l fast|balanced|max|ultra [-j N] [--no-transforms]
[--no-dedup] [--codec-level N] [--text-level N] [--block-mib N] [--text-block-mib N]
[--code-block-mib N] [--image-block-mib N] [--deflate-chain N] [--disk-images on|off]
[--lane text|binary|image|code=codec:level[e]] [--json]`, `unpack`, `list`,
`verify`, and the ZIP commands `zip`, `unzip`, `ziptest`.

Research examples kept: `jpegmodel/` (the rounds that produced our JPEG coder),
`lzsbench`, `lzslit`, `lztok` (the lzt codec), `treecmp` (tree comparison).

FFI (`arca-ffi`, 12 functions): version, task new/cancel/free, string free, list,
pack, extract, verify, zip create/extract/verify. The app calls `PackOptions::preset`
through these and passes "all cores".

## Build and test

```
cd engine && DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer cargo build --release
cd engine && DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer cargo test --release
```

52 tests: round trips at every level, the ZIP reader and writer, the transforms'
inverses, the hashes, the format. The app's 58 tests build the FFI through Xcode
(`CARGO_TARGET_DIR=engine/target/xcode`). On the development Mac, Xcode is not
selected system-wide, hence `DEVELOPER_DIR`; and a conda environment shadows
Homebrew's zstd and xz on the PATH, which matters only for benchmarks.
