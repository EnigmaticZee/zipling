# Arca — engine design

> "Arca" is a working codename (Latin: *chest, strongbox*). Rename freely.

## 1. The honest premise

General-purpose compression is a mature field. zstd, LZMA and Brotli are within a few
percent of what their model class can achieve, and "a brand-new algorithm that
compresses everything 10× better" does not exist — anyone who claims it is selling
something, or losing data.

Where big, *real* wins still live is in **what we feed the codec**:

| Lever | Why mainstream archivers miss it | Typical win |
|---|---|---|
| Already-compressed formats (JPEG, PNG, DOCX/XLSX/PPTX, PDF, JAR, .tgz) | ZIP/7z treat them as opaque, incompressible bytes | 20–75% on those files |
| Executable code (Mach-O arm64) | Relative branch targets look random to LZ | 5–15% on binaries |
| Duplicates far apart (backups, node_modules, photo libraries) | LZ windows are 8–64 MB; duplicates 2 GB apart are invisible | Up to N× |
| Single-threaded solid decompression | 7z/xz solid streams decode on one core | 5–10× faster extraction |

Arca's "novel algorithm" is therefore a **pipeline**: format-aware, reversible
transforms + global content-defined dedup + content lanes + parallel independent
solid blocks, wrapped in an integrity-first format. Every component is
battle-tested in production elsewhere (Dropbox/Microsoft Lepton, Microsoft
preflate, zstd, XZ's ARM64 BCJ, BLAKE3, FastCDC); the combination in one
consumer archive format is what's new. That is a deliberate safety choice: novel
*composition* of proven, verified parts, never an unproven entropy coder holding
someone's only copy of their photos.

## 2. Pipeline

```
scan ─► sniff (parallel; small files read whole) ─► order by group/type/name
   ─► per-file tasks, up to 512 MB ahead (file pool):
        read + hash ─► claim content (first hasher wins) ─► reversible transform
        (verified) ─► FastCDC chunks + per-chunk lane probe
   ─► in-order packer: whole-file dedup ─► chunk dedup ─► lanes ─► solid blocks
   ─► block compression (separate pool, starts as each block fills) ─► writer
```

There are no batches. Every file is its own task, and the packer consumes results
strictly in sort order, so the archive layout is deterministic. Blocks fill and start
compressing continuously, and one slow file never idles the other cores. Duplicate
content is transformed once: the first task to hash it claims it, and later
duplicates skip the work. Block compression has its own thread pool, so blocks
never queue behind file tasks. Files are sorted text → binary → media, and each
lane's final block is sealed the moment its group has passed, so there's no
compression tail at the end.

### 2.1 Classification (`classify.rs`)
Magic bytes, never extensions: JPEG, PNG, ZIP family, PDF, gzip, Mach-O (thin/fat;
disambiguated from Java `CAFEBABE`), known-compressed media/archives (mp4/mov/heic,
zstd, xz, 7z, rar, …), text, generic binary. Known-compressed formats go straight to
the **store** lane and cost zero CPU.

Routing of generic binary data is decided **per chunk, not per file**: at fast and
balanced levels each ~512 KiB chunk gets a zstd-1 probe over three 16 KiB samples,
and only incompressible chunks are stored raw. Files that mix compressed and
compressible regions (asset catalogs, disk images, PDFs with embedded images) are
split accordingly. At max level the probe is skipped: the asset catalogs in a
typical .app are 23% compressible *across* chunks even when every sample looks
random, and at max level ratio matters more than CPU.

### 2.2 Transforms (`transform/`)
Each forward transform is followed by its inverse and a byte comparison before it
is accepted. A transform bug can therefore cost ratio, never data.

* **Lepton** (`jpeg.rs`) — re-codes JPEG DCT coefficients with an adaptive
  arithmetic coder. ~22–25% smaller, bit-exact.
* **Light JPEG** (`jpeglite.rs`, Fast) — keeps JPEG's own symbols and raw bits and
  re-codes the symbols with static Huffman tables per image and per context (band
  of the next position, neighbours' nonzero counts). ~16% smaller at Huffman speed
  both ways. Headers and tables go to the binary lane, the coded scans to the
  store lane. Instead of a separate inverse, every scan is re-written from the
  parsed symbols while encoding and compared with the original.
* **Deflate expansion** (`deflate.rs`): finds every Deflate stream in ZIP-family
  files (docx/xlsx/pptx/jar/apk/epub/ipa/pages/numbers…), PNG IDAT, PDF
  FlateDecode and gzip members, expands them with preflate (which records the tiny
  diff needed to regenerate the *exact* original compressed bytes), and stores the
  layout + literal bytes + corrections + plain text. The archive codec then sees
  plain XML/pixels/text instead of Deflate noise.
* **ARM64 branch filter** (`arm64.rs`) — parses Mach-O load commands (thin and
  universal) and converts `BL`/`ADRP` immediates to absolute targets **only in
  instruction sections of arm64 slices**, leaving data and x86_64 slices untouched.

### 2.3 Dedup
* Whole-file: a keyed POLYVAL hash of content (see the safety model) → one object,
  many entries.
* Sub-file: FastCDC (128 KiB / 512 KiB / 2 MiB) over each transformed stream ≥1 MiB;
  identical chunks (by the same hash) are stored once across the whole archive, regardless
  of distance.
* On extraction, duplicate files are materialised with **APFS `clonefile`** —
  instant and zero extra disk space.

### 2.4 Lanes and blocks
Three lanes keep statistics clean: **text**, **binary**, **store**. Each lane fills
solid blocks, which are compressed independently in parallel (blocks shrink to one
per core for small inputs: always at fast, below 64 MB at balanced):

| Level | Text lane | Binary lane | Transforms | Blocks (text / binary) |
|---|---|---|---|---|
| fast | zstd 3 | zstd 3 | ARM64 filter | 8 / 8 MiB |
| balanced (default) | zstd 15 (binary-tree search) | zstd 12 (lazy search) | all; preflate depth 256 | 16 / 64 MiB |
| max | LZMA2 preset 9e (xz) | same | all | 64 / 64 MiB |
| ultra | PPMd var. H, order 16 | LZMA2 preset 9e | all | 64 / 64 MiB |

zstd runs with window = block and long-distance matching. Balanced splits codecs by
lane because the costs differ. Text needs zstd-15's binary-tree search (lighter
strategies lose 2.6%), while binaries lose under 0.4% with zstd-12 at 3–4× the
speed, and the bigger 64 MiB binary blocks win that back. Together with the pipeline
in section 2, balanced packs the mixed benchmark folder 1.8× faster than the earlier
zstd-15-everywhere setup, and the archive is 0.3% smaller.

PPMd (context modelling) beats LZ codecs on prose, but its decoder is as slow as its
encoder. So it lives in *ultra* only, where it gains 3.4% on Silesia at about 7× the
extraction time. *Max* keeps LZMA's fast decoding. Separate text and binary lanes
let each block use the codec that suits it, and give each its own statistics.

LZMA comes from a vendored copy of liblzma with two speed patches that leave
every output byte unchanged. The fast unaligned-load path is turned on, as xz's
own build does, and the BT4 match finder prefetches the next hash buckets.
Together they make `-9e` blocks 1.3–1.45× faster (`engine/vendor/liblzma-sys/ARCA-PATCHES.md`).

A block that would expand is stored raw.

### 2.5 Extraction
Extraction runs as a dependency graph rather than in waves. Every core takes
the best ready item from one priority queue: block decodes first (they unlock
everything else, and cheap stored blocks go first because they hold the JPEGs),
then the costliest remaining work (a large JPEG before a small one), so no long
task starts last. A transformed object is rebuilt the moment its last block
decodes. Directories are created by the first file that needs them, because
APFS serialises `mkdir`: creating them all up front kept every core waiting
for 0.1–1 s. Decoded
blocks are capped at two per thread (and 2 GiB). When everything in memory is
waiting on a block that isn't, one more is admitted, so odd layouts cost
memory, never a deadlock.

Single-chunk files, the vast majority, are created, written and given their
permissions, timestamps and xattrs through one file descriptor (no repeated path
lookups). Larger plain objects stream chunk-by-chunk into their final files with
`pwrite`, so a 50 GB video never sits in RAM. A file's Deflate streams are
re-created in parallel. A block stays in memory only while some pending work
still needs it. Nothing is written until the block holding it has passed both
of its hash checks, and every transformed object is checked again after its
inverse transform runs.

## 3. File format v1

```
header (32 B)   "ARCA\0\r\n\x1a" | u16 version | u16 flags | reserved
blocks          independent compressed blocks (any order; table records offsets)
index           zstd-19 compressed, LEB128-encoded
footer (64 B)   u64 index_offset | u64 comp_len | u64 raw_len | 16 B BLAKE3 | … | "ARCAEND\0"
```

The magic includes `\r\n\x1a` (like PNG) so line-ending conversion or text-mode
transfer is detected immediately. The footer-at-end layout means:

* archives are written in a single streaming pass;
* opening a 100 GB archive reads only the footer and index — browsing is instant;
* any single file is extractable by decoding only the blocks it touches.

Index contents: entries (path, kind, mode, mtime ns, xattrs incl. resource forks
and Finder info), objects (size, transform + params, chunk list, and a hash of
the original content for transformed objects), chunks (block, offset, len;
delta-coded), and blocks (offset, sizes, codec, hash of compressed bytes, hash of
decoded bytes). Version 3 also holds the 128-bit key of those hashes. The decoder
validates every cross-reference before use.

Hash budget: 32 bytes per *block* plus 16 per *transformed* file. An earlier draft
stored 48 bytes of hash per file, which was 4.5% of a source-code archive.

## 4. Safety model

* **Integrity:** POLYVAL (RFC 8452) under a random key drawn for each archive, on
  every block (compressed and decoded) and every transformed object; BLAKE3-128 on
  the index, which holds the key. Dedup uses the same hash. The key is drawn after
  the files exist, so a false dedup match has a proven bound of (blocks + 2) / 2^128
  per pair, however the files were made. Damage is caught with the same bound. An
  attacker who edits an archive can make its block checks pass, as with any
  unsigned archive; these checks are not a signature. The `verify` command decodes
  everything without writing.
* **Atomic writes:** archives are written to `<name>.partial`, `fsync`ed with a write
  barrier (`F_BARRIERFSYNC`, or a full flush where a volume has no barriers), renamed.
* **Hostile archives:** entry paths are validated (no absolute paths, `..`, NUL);
  files are opened with `O_NOFOLLOW`; nothing is written beneath a symlink the
  archive creates; symlinks are materialised last; existing files are never
  overwritten unless asked; decompression sizes are declared and checked
  (zip-bomb resistant).
* **Memory-safe engine:** Rust throughout, panics from third-party decoders are
  caught and surfaced as errors. In the app, all parsing of *foreign* formats (C
  and C++ libraries) runs inside a sandboxed XPC service with no network and no file
  access beyond what the user granted, so a malicious RAR exploiting a C parser
  can't touch the app or the user's data.

## 5. Known gaps in the prototype (tracked in ROADMAP.md)

Encryption, recovery records, appending/updating, ACLs and BSD file flags, sparse
files, x86-64 BCJ, recursive unwrapping (zip-in-zip, JPEGs inside PDFs),
deterministic block order, streaming progress callbacks, the Swift FFI.
