# Arca research index: every idea, what it measured, where it ended

This file is the index of every idea tried on the Arca engine during the research
of 28 October to 1 November 2026 (branch `zee/arca-research-77726f`), plus the
ideas that were planned or costed but never built. It is written for someone
starting cold. The full record with every table is `docs/RESEARCH.md`; the
product plan is `docs/ROADMAP.md`; the design as shipped is `docs/DESIGN.md`;
the latest verdicts by level are in `bench/report-2026-10-31-apps.md`.

How to read it. There is one entry per idea, in four groups:

- **Taken**: in the engine today, in the presets.
- **Parked**: in the engine, behind a research switch or a CLI flag, not in a preset.
- **Dropped**: rejected on evidence. The code was removed in the cleanup of
  1 November 2026, or it only ever lived in a research example, or it was reverted
  the same day.
- **Open**: planned, costed on paper, or measured once and left with a known path.

Within a group, entries run in the order they were tried. Each entry says when,
what was tried, what it measured, the verdict and why, where the code is, and what
would make it worth trying again. Sizes are in MB (or MiB where the log used MiB),
times in seconds, as the log gives them.

Two git tags matter. `before-engine-research` (commit `cc46547`) is the engine
before any of this work. `before-engine-cleanup` is the engine as benchmarked and
published on 1 November 2026, before the cleanup: it holds every removed piece
(the PPMd and xz codecs, the vendored patched liblzma, the light JPEG coder, the
readers for format versions 1 and 2, the Mach-O lane switch, sixteen research
example programs and the GPU matchfinder). A third tag,
`arca-research-2026-10-31`, freezes the research branch as of 31 October.

Machine for every number: Apple M1 Pro, 8 performance and 2 efficiency cores,
32 GB. The nine test sets: photos (400 JPEGs, 98.4 MB), PDFs and screenshots
("docs"), source code (89 crates), Apple apps, Silesia, the saved folder (660 MB,
11,050 files), the website's mixed folder, DMGs (five installers) and third-party
apps (six apps, 1.25 GB). "The board" is one Arca setting against each of 144
rival results (16 rival settings on 9 sets) on size, make time and extract time at
once; "1:1 by level" scores each level against the rivals' matching class.

---

## Taken (in the engine today)

### Content classes and lanes
**When:** before 2026-10-28 (text, binary, store); 2026-10-28 (image lane, code class and code lane at Max); 2026-10-30 (quick lane at Fast); 2026-11-01 (quick lane and code lane at Default).
**What was tried:** Files are classified by magic bytes, never extensions, and chunks are routed to lanes (text, binary, image, code, store, quick) that each fill their own solid blocks with their own codec and block size. The code class covers ELF, PE, static libraries (`!<arch>`), Java class, WebAssembly, LLVM bitcode and tar. Mach-O stays in the binary lane at every level.
**Measured:** Code lane at Max, text and code at fb 273 in 64 MiB blocks: source 9.28 → 8.23 MB, Silesia 49.31 → 48.63 MB, saved folder 212.61 MiB in 21.8 s → 211.69 MiB in 33.4 s (2026-10-28). The separate image lane made Default 0.21 MiB smaller.
**Verdict and why:** Taken. Lanes keep each block's statistics clean and let each block take the codec that suits it; the code lane is what later made bsc pay on static libraries.
**Where the code is:** `engine/arca-core/src/classify.rs`, `engine/arca-core/src/pack.rs` (`Lane`, `PackOptions::preset`).
**Revisit if:** a new content class shows a lane-specific codec gain; the Mach-O lane test of 2026-11-01 (Dropped) is the template for that experiment.

### Content-defined dedup with a keyed hash (FastCDC and POLYVAL)
**When:** before 2026-10-28 (FastCDC and BLAKE3); 2026-10-28 (each file hashed once); 2026-10-29 (POLYVAL, format version 3).
**What was tried:** Chunks (FastCDC 128 KiB / 512 KiB / 2 MiB) and whole files are deduplicated by hash across the whole archive. BLAKE3 ran about four times over every byte (file, chunk, block, compressed block, and twice more on unpack). It was replaced by POLYVAL under a 128-bit key drawn after the files exist, so two different inputs collide with probability at most (blocks + 2) / 2^128 by proof, crafted or not. The key sits in the index, which stays under BLAKE3. An earlier proposal of a fast 128-bit hash plus a byte comparison was dropped: the first copy's bytes may be compressed and gone by then.
**Measured:** BLAKE3 (NEON) 1,800 MB/s; POLYVAL 8 blocks through general registers 8,900; vector registers only 11,300; 16 blocks 14,600 MB/s (8.1x BLAKE3). Fast, old and new binaries taking turns: photos pack 0.274 → 0.244 s (−11%), website mixed 0.934 → 0.797 s (−15%), saved folder 0.861 → 0.739 s (−14%); unpack −6 to −11%. Bytes flipped at a quarter, half and three quarters of two archives were all caught.
**Verdict and why:** Taken. What is given up: with the key, a tamperer can make different bytes hash alike, so the index hash no longer commits to the block contents; archives are not signed anyway.
**Where the code is:** `engine/arca-core/src/hash.rs`, `engine/arca-core/src/format.rs` (version 3).
**Revisit if:** archives are ever signed; the hash would then have to commit to contents again.

### Parallel solid blocks and the pipeline without batches
**When:** before 2026-10-28; refined through 2026-10-30.
**What was tried:** Every file is its own task; the in-order packer fills lane blocks that compress in their own pool as they fill; extraction is a dependency graph with one priority queue (block decodes first, then the costliest remaining work). This is the frame every other entry fits into.
**Measured:** Baseline 2026-10-28 on the saved folder, busy Mac: Fast 1.5-1.9 s to make, Default 10.1-10.6 s, Max 105.4 s with 722 s of CPU and a 10.3 GB peak. End of research, quiet (2026-10-31): Fast 0.49 s, Default 8.11 s, Max 16.6 s.
**Verdict and why:** Taken; it is the engine's shape.
**Where the code is:** `engine/arca-core/src/pack.rs`, `engine/arca-core/src/unpack.rs`.
**Revisit if:** no path seen; see the scheduling entries below for the parts that moved.

### Deflate expansion with preflate, and Lepton (the inherited transforms)
**When:** before 2026-10-28.
**What was tried:** Every Deflate stream in ZIP-family files, PNG IDAT, PDF FlateDecode and gzip is expanded with preflate, which records the diff to regenerate the exact bytes; JPEGs were re-coded with Lepton. Each forward transform is followed by its inverse and a byte comparison.
**Measured:** Baseline Max extract 9.6 s with 74 s CPU: rebuilding Deflate 64.9 s CPU, Lepton 14.6 s, xz decode 10.0 s, writing 4.8 s. Default make 10.6 s: Lepton encode and verify 38.6 thread-seconds, zstd-12 binary 27.9, preflate analysis 21.5, zstd-15 text 11.3.
**Verdict and why:** Kept as the base. Every later transform entry (zlib and LZFSE replay, the step budget, our JPEG coder) attacks these numbers.
**Where the code is:** `engine/arca-core/src/transform/deflate.rs`, `transform/jpeg.rs`, `engine/vendor/preflate-rs`.
**Revisit if:** PNG token coding or a parallel Deflate rebuild (Open) is built.

### Fast's duplicate and fingerprint cost
**When:** 2026-10-28.
**What was tried:** Each file hashed once (chunk by chunk; a file's identity is its single chunk's hash, or a keyed hash of the (length, hash) list for several chunks); large files no longer hash with `update_rayon` inside the busy pool; the index at zstd-9 at Fast and Default; the folder walk on the pack pool; no separate sniffing pass at Fast (files ordered by extension, each task reads, classifies and packs); Default and up still sniff a 4 KiB head.
**Measured:** Fast make on the saved folder 1.46-1.9 s → 0.80-1.0 s at the same size (301.33 MiB); index 0.31 → 0.01 s for 257.5 KB instead of 238.5 KB; folder walk 0.20 → 0.07-0.09 s; 11,050 open+read+close 0.26 s on 1 thread, ~0.12 s on 2-10. Extension order against sniffed order: Fast 301.33 MiB both; Default 245.72 vs 245.34 MiB; Max (budget 64) 219.05 vs 217.71 MiB.
**Verdict and why:** Taken. Sniffing stays at Default and up because order costs size there.
**Where the code is:** `engine/arca-core/src/pack.rs`, `scan.rs`, `hash.rs`. Research: `examples/scanbench.rs` (the folder scan, removed 2026-11-01, in tag before-engine-cleanup).
**Revisit if:** a cheaper sniff makes sniffed order affordable at Fast.

### Index format v2: column-wise, front-coded paths
**When:** 2026-10-28.
**What was tried:** Entries written column by column, paths front-coded against the previous path, times and object numbers delta-coded.
**Measured:** Saved folder at Fast: index 251.37 KiB (v1, zstd-9) / 232.61 KiB (v1, zstd-19) → 206.75 KiB (v2, zstd-9, 5 ms); raw index 1.3 MB → 0.5 MB. Fast on the source set within 858 bytes of the morning's size (was +24.8 KB).
**Verdict and why:** Taken; the layout carried into format version 3.
**Where the code is:** `engine/arca-core/src/format.rs`, `wire.rs`.
**Revisit if:** no path seen.

### LZMA2 through the LZMA SDK, with the two-thread match finder
**When:** 2026-10-28.
**What was tried:** 7-Zip 26.05's public-domain LZMA SDK vendored; block codec `lzma2` (tag 4): raw LZMA2 behind its property byte, no xz container, the match finder in its own thread, 7-Zip's ARM64 assembly decoder loop. It replaced the xz codec (tag 2, Dropped).
**Measured:** `7zz -mx9 -mfb=273` against `xz -9e` on the Mach-O block: 10.159 MB in 15.5 s vs 10.201 MB in 27.7 s; user time 24.1 vs 27.7 s (~13% less CPU). Mach-O block at fb 32: one thread 21.2 s wall / 21.2 s CPU; two threads 12.4 s wall / 17.7 s CPU; output 10,529,319 vs 10,535,673 bytes (+0.06%), the same on every run. Saved folder Max: 210.84 → 210.60 MiB; LZMA decoding 4.76 → 3.18 s of CPU; extraction 3.99 → 3.83 s; no gain in make time (40 s) with ten blocks compressing at once.
**Verdict and why:** Taken. Smaller, less CPU, and a second thread per block for when there are fewer blocks than cores.
**Where the code is:** `engine/arca-core/src/codec.rs`, `engine/vendor/lzma-sdk` (ARCA-NOTES.md; `LzmaEnc.c` carries the research token tap under the `token-hook` feature). Research: `examples/lzthreads.rs` (one or two match-finder threads) and `examples/lzbench.rs` (SDK settings on dumped blocks), both removed 2026-11-01, in tag before-engine-cleanup.
**Revisit if:** an SDK update changes the match finder; re-run the blocks with `ARCA_EXP_FB`.

### Max's LZMA effort: fb 32 on binaries, long matches on text and code
**When:** 2026-10-28.
**What was tried:** fb 273, 128, 64, 48 and 32 measured per lane. The user chose "Max = fastest": LZMA2 fb 32 with 32 MiB text blocks. Then the text lane got 64-byte matches at no time cost, then text and code fb 273 in 64 MiB blocks (replaced by bsc on those lanes on 2026-10-31). Binaries stay at fb 32 at Max and fb 273 at Ultra.
**Measured:** One 64 MiB block, one thread, CPU s: text fb 273 6.249 MB / 28.1 s, fb 32 6.593 / 15.6; Mach-O fb 273 10.155 / 33.6, fb 32 10.529 / 23.1; other binaries fb 273 20.561 / 25.6, fb 32 20.734 / 16.2. Saved folder: fb 273 210.35 MiB in ~40 s (275 s CPU); fb 64 with 32 MiB text blocks 211.50 MiB, 26.8 s (194 s); fb 32 with 32 MiB text blocks 212.85 MiB, 23.2 s (168 s). Text fb 64: saved 212.85 → 212.65 MiB, Silesia 47.53 → 47.03 MiB, no time cost; fb 273 text 212.55 / 46.76 MiB but +2 s.
**Verdict and why:** Taken: Max is LZMA2 preset 7 (fb 32) on binaries, Ultra 9e (fb 273); fb 32 is half the CPU of fb 273 for 1-6% more output.
**Where the code is:** `engine/arca-core/src/pack.rs` (`PackOptions::preset`), `codec.rs`.
**Revisit if:** bsc stops being the text and code codec; the fb 273 text numbers then apply again.

### File order within the binary group
**When:** 2026-10-28.
**What was tried:** The order Mach-O, ZIP, PDF, gzip, other binaries, PNG (PNGs last: their pixels mostly go to the store lane now).
**Measured:** Max, saved folder: before 210.60 MiB, 40.4 s; Mach-O, other binaries, gzip, ZIP, PDF, PNG 211.66 MiB, 35.8 s; the chosen order 210.35 MiB, 39.6 s. Default 245.34 → 245.43 MiB (+0.04%), Fast unchanged.
**Verdict and why:** Taken; smallest of the three.
**Where the code is:** `engine/arca-core/src/pack.rs` (sort groups).
**Revisit if:** similarity ordering (Open) is tried.

### Preflate's step budget and chain limit
**When:** 2026-10-28.
**What was tried:** A preflate search step costs a steady ~3.1 ns and the analysis at pack time performs exactly the rebuild's searches, so a step count measured while packing predicts the rebuild cost exactly. Patches to the vendored preflate-rs: a step counter, a per-byte step budget checked every 1,024 tokens (a stream over budget is abandoned and stored as is), an optional chain cap. The budget check is proportional (four times the share so far). Presets: chain 256 at Default, 4096 at Max, 128 steps per byte.
**Measured:** Max on the docs part (850 files): chain 4096 71.37 MiB, 90.2 s make, 6.62 s extract, 59.0 s rebuild CPU; budget 64 steps/byte 77.00 MiB, 55.8 s, 1.05 s, 3.5 s; 160 steps/byte 76.44 MiB, 62.4 s, 1.10 s, 4.7 s; chain capped to 256 73.65 MiB, 4.81 s, 40.9 s; capped to 1024 72.48 MiB, 5.70 s, 49.8 s. 72 of 793 Deflate files (13 MB) cause 42.7 of 60 s. Whole saved folder at Max with budget 64: 211.47 → 217.71 MiB, 105 → 80 s, extract 9.6 → 3.6 s. Proportional check: PNG analysis 21.9 → 18.9 s CPU, same size (twice the share: 15.5 s but one more PNG rejected, +0.10 MiB).
**Verdict and why:** Taken. The cost is the number of searches, not their depth, so the cap does not help much. The trade at Max is about 2.7% of size for ~6x faster docs extraction; the size lives in exactly the costliest files.
**Where the code is:** `engine/arca-core/src/transform/deflate.rs`, `engine/vendor/preflate-rs`. Knobs `ARCA_EXP_STEPS`, `ARCA_EXP_CAP`. Research: `examples/objtime.rs` (lists the costliest rebuilds in an archive) and `examples/deflbench.rs` (times one container's inverse), removed 2026-11-01, in tag before-engine-cleanup.
**Revisit if:** PNG token coding (Open) is built; it would recover the size without the searches.

### Level-9 PNGs skipped at Default by the zlib header
**When:** 2026-10-28.
**What was tried:** The zlib header's FLEVEL (3 for levels 7-9) skips a PNG up front when the chain limit is under 1024, instead of a full analysis that preflate's limit of 256 then rejects.
**Measured:** Saved folder, three alternating runs: PNG analysis 11.0 → 6.3 s of CPU, pack CPU 63.2 → 57.9 s, size 245.48 → 245.47 MiB, wall 9.1-9.2 → 8.8-9.2 s. Before it, Default spent 2.7 s rejecting 92 level-9 PNGs.
**Verdict and why:** Taken.
**Where the code is:** `engine/arca-core/src/transform/deflate.rs`.
**Revisit if:** no path seen.

### A memory budget for block encoders (8 GB Macs)
**When:** 2026-10-28.
**What was tried:** The number of block encoders comes from a memory budget (half of `hw.memsize`; `ARCA_MEM_BYTES` simulates other Macs) using the encoder's own memory estimate plus the block and its output; queued blocks encoders + 2 (was 2 x threads); read-ahead min(512 MiB, budget/16), counting expanded bytes too.
**Measured:** Peak was 8.3-10.3 GB (ten xz-9e encoders at ~674 MiB each, 20 queued 64 MiB blocks, a read-ahead that counted an expanded PNG at ~1/15 of its size). Simulated 8 GB: 3 encoders, 4.8 GB peak, Max make 124 s (busy); 16 GB: 8 encoders, 8.5 GB, 72 s. Archives byte-identical whatever the memory.
**Verdict and why:** Taken. Memory was then set aside by the user's rule (no more than the hungriest rival on the same files; the 8 GB test comes after every rival is beaten).
**Where the code is:** `engine/arca-core/src/pack.rs`.
**Revisit if:** the 8 GB Mac test is run; Max at 6-8 GB on the large sets is the first to cut (Open).

### The pixel codec for PNG data at Max and Ultra
**When:** 2026-10-28.
**What was tried:** Undo the PNG row filters, predict each pixel with MED (LOCO-I), code green's residual then red and blue as differences from green, then alpha. v0-v3 used an adaptive binary range coder (`pixbench`); v4, the fast design, 16-token adaptive tables (LZNA-style) with rANS and raw remainder bits (`pixfast`). Per image, the codec is used when its output is under 0.9x the zstd-1 size of the filtered rows, else the rows go to the LZMA image lane. In the engine as `transform/pixels.rs` (Deflate layout version 2): layout, corrections, literals and filter types to the binary lane, coded pixels to the store lane.
**Measured:** 559 docs PNGs (1,042 MB of pixels), per image: xz-9 70.44 MB / 167.8 s; v0 70.62 MB / 22.3 s encode; v1 64.32; v2 63.80; v3 (blend of 6 predictors) 63.86 at 42 s / 63 s, best of blend/xz 59.10. On the 439 PNGs Max expands: xz-9 49.23 MB, MED codec 43.80, blend 43.20, best of blend/xz 39.75 MB (−19%). v4 on 409 eligible (573 MB): 44.03 MB, 8.4 s / 7.8 s against v2's 43.80 MB, 14.5 / 24.9 s. The choice rule gives 41.30 MB against 40.95 for a perfect choice and 49.23 for xz alone. Saved folder at Max: with rebuild budget 217.17 MiB / 73 s / 3.58 s → with pixel codec 210.84 MiB / 38.3 s / 3.99 s (pack CPU 429 → 259 s); the image lane shrank to 117 MiB of flat images, 26 s of LZMA instead of 212.
**Verdict and why:** Taken at Max and Ultra. v3's blend was not taken (3x the CPU, worse on flat images). Decoding speed kept it out of Default until the next entry.
**Where the code is:** `engine/arca-core/src/transform/pixels.rs`. Research: `examples/pixbench.rs` (removed 2026-11-01, in tag before-engine-cleanup); `examples/pixfast.rs` removed likewise, its module `examples/pixfast/codec.rs` is still in the tree; `examples/filtbench.rs` (row filter timing) removed.
**Revisit if:** WebP-style tools (palette, 2D LZ77, colour cache) are added for flat images, where the codec is 1.75x xz above 30:1 (Open).

### Faster pixel decoding, same format
**When:** 2026-10-28 (evening).
**What was tried:** Instruction counting showed ~100 instructions per byte at 4.7 per cycle: bound by instruction count. One decoder per channel count, flat pixels skip the context, table-driven update and tokens; then the rANS states swap in registers, a macro writes the channels with constant indexes, MED is a clamp, binary symbols and the refill use selects, the token table has a sentinel. PNG row filters run one loop per filter type.
**Measured:** docs PNGs, 1 GB of pixels: 110 → 171-182 MB/s; then photo-like PNGs (175) 67 → 101 MB/s, flat (32) 145 → 353, all 604 PNGs 110 → 225 MB/s; encode 103 → 117 MB/s, byte-identical output. Refilter 595 MB/s → 10.8 GB/s, unfilter 634 → 842 MB/s. Max extraction CPU −1 s.
**Verdict and why:** Taken.
**Where the code is:** `engine/arca-core/src/transform/pixels.rs`.
**Revisit if:** a format change is accepted (static tables per stripe); photo-like images sit at 82 instructions per byte.

### Lepton built without LLVM's memset idiom
**When:** 2026-10-28.
**What was tried:** About 10% of Lepton's samples were `memset_pattern16` inside the DC predictor: LLVM turns every vector constant of the `wide` crate into an array fill (19 calls of 32 bytes per block). Building with `-C llvm-args=-disable-loop-idiom-memset` in `engine/.cargo/config.toml` (so the app's build too).
**Measured:** Lepton encode 9.8 → 8.6 s, decode 10.4 → 9.1 s on the photos (−12%), byte-identical; saved folder Default extraction 2.41-2.51 → 2.28-2.44 s, Max 3.08-3.23 → 3.00-3.09 s, packing CPU 1.5-2 s lower. Patching `wide` to build `i32x8` from halves only gave 4%.
**Verdict and why:** Taken.
**Where the code is:** `engine/.cargo/config.toml`. Research: `examples/leptbench.rs` (Lepton's encode and decode CPU over a folder), removed 2026-11-01, in tag before-engine-cleanup.
**Revisit if:** no path seen.

### bsc (LZP + BWT) on the text and code lanes at Max and Ultra
**When:** 2026-10-28 (vendored and measured, held as an option by the user); 2026-10-31 (preset).
**What was tried:** libbsc 3.3.12 with libsais 2.10.4, block codec `bsc` (tag 5). `bsc:1` for text (LZP 18/128, adaptive QLFC), `bsc:2` for code (LZP 24/64); the `e` suffix falls back to LZMA2 fb 273 for a block bsc gets under 16:1, decided on a 4 MiB sample.
**Measured:** One block on one core (`bscbench`): saved text 60.4 MB, bsc 6.233 MB in 1.8 s against LZMA2 fb 273 6.249 MB in 28.3 s (16x); Silesia text 67.7 MB 7.389 vs 9.159 MB (−19%, 18x); static libraries 67.1 MB 1.452 vs 1.391 MB (+4.4%, 62x), 1.375 MB with LZP 24/64 (−1.2%); Silesia x86 tar +19%, Mach-O +31%, Silesia binary −8%. Against PPMd order 16: 6.167 vs 6.254 MB, 7.321 vs 8.108 MB, 2.858 vs 3.086 MB, 2-3x faster both ways. Saved folder Max 211.69 MiB in 32.4 s (212 s CPU) → 211.58 MiB in 19.8 s (136 s CPU); extraction 2.87-2.95 → 3.15-3.31 s. Source Max 8.232 MB in 23.1 s → 8.094 MB in 1.85 s; Silesia 48.629 → 45.971 MB; mixed 194.785 MB in 44.5 s → 192.337 MB in 24.1 s. The sample-based fallback: Silesia Max make 18.4 → 14.9 s. Smaller text blocks cost size (source 16 MiB 8.368 MB, 32 MiB 8.274, 64 MiB 8.094), so 64 MiB stays. Presets locked, quiet, 1:1 by level: 103 → 131 of 198, bsc being the difference.
**Verdict and why:** Taken at Max and Ultra. Both 7-Zip Ultra targets held with room (source 8.09 < 8.5 MB, Silesia 45.97 < 48.7). The one row given back: Silesia against xz -6 on extraction alone (0.57 vs 0.55 s).
**Where the code is:** `engine/arca-core/src/codec.rs`, `engine/vendor/libbsc` (ARCA-NOTES.md). Research: `examples/bscbench.rs`, removed 2026-11-01, in tag before-engine-cleanup.
**Revisit if:** a faster bsc decode (static QLFC, a wider parallel inverse BWT) is built; that is also the gate for Default's text lane (Open).

### bsc blocks decoded on several cores
**When:** 2026-10-28; 2026-10-30 (one thread per 8 MiB of block).
**What was tried:** A bsc decode is 54% QLFC, 41% inverse BWT, 5% LZP. bsc's format already splits the QLFC stream into 24-32 sub-blocks per 64 MiB block and stores 8-16 sampled BWT indexes, but only an OpenMP build used them. `arca_bsc.cpp` runs the stages with plain threads, `arca_sais.c` adds the threaded inverse BWT; same format, same checks.
**Measured:** saved folder text 60 MB 1.47 → 0.51 s; Silesia text 68 MB 1.91 → 0.67 s; static libraries 67 MB 0.52 → 0.22 s; Silesia x86 tar 3.93 → 0.92 s. Extraction Max → Max + bsc: source 0.50 → 0.97-1.00 s (1.78 s before threading), Silesia 0.69-0.70 → 0.94-1.03 s (2.04 s before). Default + bsc on Silesia with one thread per 8 MiB: 0.60 → 0.47 s.
**Verdict and why:** Taken. bsc still needs ~4-6x LZMA2's decode CPU, which is the whole reason it stays off Default's text lane.
**Where the code is:** `engine/vendor/libbsc/src/arca_bsc.cpp`, `engine/vendor/libbsc/src/arca_sais.c`.
**Revisit if:** the QLFC stage (54%) gets a static or wider-parallel coder.

### Writing files on extraction
**When:** 2026-10-28.
**What was tried:** APFS serialises file creation. At most 4 threads create files at once; duplicates under 64 KiB are written again while the content is in hand (larger ones are cloned); mode and times in one `fsetattrlist` call; directories not re-created. Then a write queue: up to four writes run at any time while other threads decode and rebuild, instead of all writes landing at the end.
**Measured:** `writebench`, 11,050 empty files: 1 thread 0.95 s wall / 0.90 s kernel; 10 threads 0.50-0.53 s / 3.1 s; 10 with at most 4 inside `open` 0.43 s / 2.15 s. Extraction: Fast 0.97-1.02 → 0.74-0.96 s, Default 2.72-2.80 → 2.57-2.63 s, ZIP 1.05-1.39 → 0.91-0.99 s; kernel CPU 5.0 → 3.4 s. The write queue: Default 2.38-2.50 → 2.19-2.28 s, Max 3.05-3.20 → 2.92-3.02 s.
**Verdict and why:** Taken. Gating `close` and `openat` per directory did nothing; 2, 3 or 6 creators against 4 were tried and dropped.
**Where the code is:** `engine/arca-core/src/unpack.rs`. Research: `examples/writebench.rs`, removed 2026-11-01, in tag before-engine-cleanup.
**Revisit if:** no path seen; the kernel's serialisation sets the floor.

### Quarantine set by the engine
**When:** 2026-10-28.
**What was tried:** `ExtractOptions::quarantine`: the engine sets the attribute through each file's descriptor as it writes (also in the FFI requests for `.zlg` and ZIP); the app's own pass runs again only if it merged AppleDouble files.
**Measured:** Setting quarantine on all 14,505 items by path had added 0.15-0.4 s in the app. App Fast extraction 0.99-1.15 s, of which the engine 0.87 s, the listing 0.09 s, the safety walk 0.10 s.
**Verdict and why:** Taken; a new engine test checks every kind of item gets it.
**Where the code is:** `engine/arca-core/src/unpack.rs`, `arca-ffi`.
**Revisit if:** no path seen.

### The ZIP reader
**When:** 2026-10-28.
**What was tried:** `zip.rs` parses the central directory (ZIP64, timestamp fields), inflates stored and Deflate entries on every core with zlib-rs, checks every CRC-32, streams entries over 64 MiB, decodes `._` AppleDouble twins in memory and applies them through each file's descriptor, creates links last and leaves out any that escape. Anything else fails with "unsupported" so the app falls back to 7-Zip. Tenth FFI function `arca_zip_extract`.
**Measured:** `ditto-mixed.zip` (30,682 entries, 632 MiB): check every CRC 0.36 s; extract 1.06-1.12 s; `ditto -x -k` 11.8 s; the app through 7-Zip 4.7 s; Zipgeist 2.1 s. 0 differences over 14,506 items against ditto's extraction.
**Verdict and why:** Taken.
**Where the code is:** `engine/arca-core/src/zip.rs`, `engine/arca-core/tests/zip.rs`.
**Revisit if:** encrypted ZIPs or other methods should stop falling back to 7-Zip.

### The ZIP writer
**When:** 2026-10-28.
**What was tried:** `zipwrite.rs` at Deflate level 7; files of 4 MiB and up compressed in 1 MiB pieces on several cores as pigz does (each primed with the 32 KiB before it, joined by sync flushes). FFI `arca_zip_create` and `arca_zip_verify`.
**Measured:** Saved folder: level 6 352.0 MB in 1.20-1.38 s; level 7 351.56 MB in 1.39-1.42 s; level 8 351.11 MB in 1.75-1.78 s; level 9 347.8 MB in 2.35 s; 7-Zip -mx1 351.80 MB in 1.71-1.74 s; -mx7 344.3 MB in 14.1 s; -mx9 343.6 MB in 35.4 s. Level 7 went 1.80 → 1.4 s with the pieces. The reader checks a ZIP in 0.22 s against 1.86 s for 7-Zip's test; the app's Fast and Default ZIPs 3.6 → 1.6 s including the check.
**Verdict and why:** Taken for Fast and Default ZIPs; Max and Ultra ZIPs stay with 7-Zip, whose optimal-parsing Deflate is 1% smaller.
**Where the code is:** `engine/arca-core/src/zipwrite.rs`.
**Revisit if:** an optimal-parsing Deflate is wanted for Max ZIPs.

### zlib replay for Deflate streams (levels 1-5)
**When:** 2026-10-28 (study); 2026-10-29 (in the engine).
**What was tried:** zlib 1.3.2's encoder vendored with `Z_PREFIX`. A zlib-wrapped stream (PNG, PDF) whose header says level 1-5 is re-run with zlib and compared, stopping at the first differing byte; otherwise preflate. Layout: version flag `REPLAY`, one method byte per stream.
**Measured:** 480 of 644 PNGs (81.9 of 122.9 MB of Deflate) are byte for byte what macOS's zlib makes (level 1 56.1 MB, level 9 18.2 MB, level 6 5.7 MB); the pinned 1.3.2 re-creates exactly the same 480 of 656. Level 1, 301 PNGs: preflate 3.81 / 1.35 s → replay 1.53 / 1.18 s. Saved folder Default pack CPU 59.2 → 54.9-55.8 s (−6.6%), 257,398,331 → 257,395,835 bytes; Max pack CPU 214.0-214.7 → 212.1-212.7 s. Level 6 re-runs slower than preflate rebuilds (1.04 vs 0.59 s for 76 PNGs); level 9 at 6 MB/s slower too; a wrong guess at level 9 costs 80 s over 123 PNGs.
**Verdict and why:** Taken for levels 1-5 only.
**Where the code is:** `engine/arca-core/src/transform/deflate.rs`, `engine/vendor/zlib`. Knob `ARCA_EXP_NO_REPLAY`. Research: `examples/preftime.rs` (preflate forward and inverse CPU per file), removed 2026-11-01, in tag before-engine-cleanup.
**Revisit if:** no path seen; levels 8-9 are covered by the next entry.

### An exact, faster zlib match search for levels 8-9
**When:** 2026-10-29.
**What was tried:** At levels 8-9 zlib's walk returns the newest string with the longest match among the first `max_chain` strings of its bucket. The pinned zlib's `fast_match` finds that directly (chains of strings sharing 8 and 4 bytes plus per-bucket counts that honour the cut-off exactly), with the same output. Used for disk image chunks at Max and Ultra; a disk image's chunks share one setting, found on the first level 6-9 chunk.
**Measured:** Discord's chunks: 41 M searches, 2.54 G chain steps (61.6 per search), 81% real 3-byte matches that cannot beat the best. One core: 145 MB corpus level 9 28.3 → 5.0 s (5.7x); level 8 14.2 → 3.7 s (3.8x); Discord's chunks (122 MB) 7.8 → 3.9 s. Discord's DMG: Max 152.73 → 145.03 MB (−5.0%), make 32.2-32.8 → 30.0 s, extract 3.6-3.7 → 3.1 s (with stock zlib the same re-run cost 33.4 s / 4.9 s, slower than preflate). The dmgs set: Max 457.86 → 450.56 MB, 54.9 → 51.0 s, 5.80 → 4.69 s; Ultra 455.42 → 447.89 MB, 65.4 → 60.1 s, 5.89 → 4.56 s. At Default the same re-run was 18% smaller for 4x the make time and 7x the extraction (2.0 → 8.5 s, 0.33 → 2.3 s), so it stayed off there. A hybrid (preflate, then zlib for what it refuses) was 145.6 MB / 35 s / 5.0 s at Max, worse.
**Verdict and why:** Taken at Max and Ultra; smaller and faster both ways there.
**Where the code is:** `engine/vendor/zlib` (ARCA-NOTES.md), `engine/arca-core/src/transform/deflate.rs`.
**Revisit if:** no path seen for the search itself; what Default expands is now the rebuild budget's call.

### The x86 branch filter for universal binaries
**When:** 2026-10-29; across cores 2026-10-30.
**What was tried:** XZ's x86 BCJ (CALL/JMP rel32 made absolute) ported to Rust and applied to the instruction sections of x86_64 slices; the index stores those ranges under transform tag 4 (arm64-only files keep tag 1). Ranges are cut into 2 MiB spans so the encoder and decoder split alike.
**Measured:** 64 MB of x86 code: xz -9 20.51 → 19.86 MB (−3.2%), 7% less CPU. Seven executables from /Applications (660 MB): Max 210.386 → 208.677 MB (−0.81%), Default 255.904 → 254.032 MB (−0.73%). Across cores: third-party apps at Fast 1.05 → 0.91 s, size unchanged.
**Verdict and why:** Taken. Installed third-party apps hold 564 MB of x86_64 code beside 702 MB of arm64.
**Where the code is:** `engine/arca-core/src/transform/arm64.rs` (`filter_x86`). Knob `ARCA_EXP_NO_X86`.
**Revisit if:** no path seen.

### The arm64 branch filter, split across cores and unverified at Fast
**When:** before 2026-10-28 (the filter); 2026-10-29 (regions split, check in 2 MiB pieces); 2026-10-30 (round-trip check off at Fast).
**What was tried:** BL and ADRP immediates made absolute in instruction sections of arm64 slices. The filter looks at one instruction at a time, so regions split anywhere; arm64 and x86 regions run side by side; the check compares in 2 MiB pieces without copying the file. At Fast the check is off: the filters are bijections, held to it by a 400-round stress test.
**Measured:** Third-party apps packing 1.32 → 1.03 s (with the ARM64 filter and chunking off one thread); Fast's quick lane then 0.52 s.
**Verdict and why:** Taken.
**Where the code is:** `engine/arca-core/src/transform/arm64.rs`, `transform/mod.rs`.
**Revisit if:** no path seen.

### Disk images (DMG) expanded and rebuilt
**When:** 2026-10-29 (Max and Ultra); 2026-10-30 (Default, without zlib level 7-9 chunks, on a zstd -5 lane); 2026-10-31 (rebuild budget, next entry).
**What was tried:** A UDIF image is a chunk table and a `koly` trailer. The Deflate transform takes `.dmg` files (`DeflateKind::Dmg`): the trailer's XML lists the `mish` tables, each zlib chunk is replayed (or preflate), each LZFSE chunk replayed with the pinned lzfse, bytes outside chunks stay literal; expanded data capped at 512 MiB per file; `--disk-images on|off` per level, off at Fast (expansion does not keep up with Fast's codec).
**Measured:** UDZO image of the apps set: Default 93.96 → 74.62 MB (−21%), 2.8 s / 0.49 s; Max 86.80 → 60.68 MB (−30%), 12.0 s / 0.97 s. ULFO: Default 87.84 → 75.09 MB (−14.5%); Max 86.96 → 59.25 MB (−31.9%). The dmgs set on and off: Default 557.0 MB, 7.6-7.7 s, 1.3 s against 619.7 MB, 0.74-0.77 s, 0.22 s; Max 450.6 MB, 50.4-51.2 s, 4.5-4.7 s against 610.6 MB, 11.3 s, 1.2 s. Off, compressing the DMG bytes saves −0.1% / −1.5%. Per image at Default: CleanMyMac saves 45.7 MB (22%) for 4.7 CPU-s, Discord 39.8 MB for 19.4 CPU-s, Nuvio 13.8 MB for 3.6 CPU-s. Default without level 7-9 chunks: 615 → 550 MB, 5.4 s, 0.95 s; on a zstd -5 lane 5.4 → 2.6 s for 1.8%.
**Verdict and why:** Taken at every level but Fast. The time goes into compressing the expanded data (2-3x the DMG bytes), not into re-creating the chunks; no archiver that stores DMGs as they are can get near the "on" sizes.
**Where the code is:** `engine/arca-core/src/transform/deflate.rs`, `engine/arca-core/src/pack.rs` (`Lane::Quick`, `quick_codec`).
**Revisit if:** bzip2 (UDBZ) and LZMA (ULMO) chunks get a re-creation path (Open).

### LZFSE replay: macOS's encoder is lzfse 1.0 with one constant changed
**When:** 2026-10-29.
**What was tried:** lzfse 1.0 (Apple's reference, BSD-3) vendored. Out of the box it re-created 66 of the ULFO image's 193 chunks. Comparing token streams showed every difference starts where macOS emits a pending match in a long literal run that lzfse 1.0 keeps waiting on: lzfse 1.0 emits after 8 x 315 literals, macOS after 3 x 315. The vendored copy carries that number in the encoder state; each stored stream records the value that re-creates it (3, else 8).
**Measured:** With 3: the ULFO image 193 of 193 chunks (84.5 MB) (1, 2 and 4 gave 162, 177, 180); the saved folder's `Assets.car` 606 of 784 streams (10.16 of 12.08 MB); the installed apps' 239 catalogs 20,861 of 29,023 streams (351 of 408 MB). The rest: LZVN streams under 4 KiB (a different encoder) and 275 LZFSE streams from some other version.
**Verdict and why:** Taken; it is what makes ULFO images and asset catalogs expandable with a pinned encoder.
**Where the code is:** `engine/vendor/lzfse`, `engine/arca-core/src/transform/deflate.rs`. Knob `ARCA_EXP_NO_LZFSE`.
**Revisit if:** an exact LZFSE encoder faster than Apple's is wanted for the DMG extraction row (Open).

### A rebuild budget for disk images at Default
**When:** 2026-10-31.
**What was tried:** The chunk table says each chunk's decoded size, so before any chunk is expanded Default knows what re-creating it will cost (the pinned encoders' measured pace) and what it is worth (the ratio the image got). Chunks are taken in order of ratio until 4.5 ms of rebuild per MB of image is spent; chunks under 1.1:1 are left alone. Max and Ultra expand everything (the ratio filter cost Max 2.3% on the set).
**Measured:** DMGs set, Default: expanding everything 560.5 MB, 2.9 s, 1.05 s; budget 3 ms/MB 606.3 MB, 1.1 s, 0.27 s; 4.5 ms/MB 601.8 MB, 1.1 s, 0.35 s; 6 ms/MB 597.8 MB, 1.3 s, 0.41 s; zstd -19 --long 610.27 MB, 24.0 s, 0.45 s. Quiet: 601.83 MB, make 1.04-1.25 s, extract 0.35-0.37 s. Board 139 of 144, DMGs a clean sweep.
**Verdict and why:** Taken. Default gives up 7% on disk images for extracting them 3x faster, with a 23% margin on the row.
**Where the code is:** `engine/arca-core/src/transform/deflate.rs` (`DMG_REBUILD_S_PER_BYTE`). Knob `ARCA_EXP_DMG_BUDGET_MS`.
**Revisit if:** the rebuild gets faster; the budget then buys more size at the same extract time.

### Three opens at a time
**When:** 2026-10-29.
**What was tried:** `open` does not scale on APFS: ten threads opening at once spin. `open_file` lets three opens run at a time.
**Measured:** 10,279 files, CPU per call / wall for all: 1 thread 18 µs / 188 ms; 3 threads 25 µs / 87 ms; 10 threads 120 µs / 168 ms (7x the CPU and twice the wall of three); `lstat` and `listxattr` show the path walk is cheap. About −1 CPU-s of 6.5 on the website and saved folders. Opening relative to the parent folder: 20 µs against 18, no help.
**Verdict and why:** Taken.
**Where the code is:** `engine/arca-core/src/pack.rs` (`open_file`). Research: `examples/openbench.rs`, removed 2026-11-01, in tag before-engine-cleanup.
**Revisit if:** no path seen.

### Fast's zstd settings: tables 18/17, long-distance matching off on binaries, the 8:1 retry
**When:** 2026-10-29; tables re-measured 2026-10-31.
**What was tried:** Fast's binary lanes run zstd with hash 18 and chain 17 tables and no long-distance matching; the text lane keeps the long search (far repeats are 5% of a source tree). A block that shrinks more than 8:1 is compressed again with the search on, which keeps static libraries' far repeats without a per-format rule. Also dropped: zstd's XXH64 per frame (the block hash covers it) and FastCDC on a photo's coded tail.
**Measured:** Long-distance matching off alone: +0.5-0.8% in size, −11 to −24% CPU. Tables 18/17 with it off, against it on: website +0.02% / −10% CPU, saved −0.02% / −8%, Silesia −0.49% / −9%, apps −0.49% / −3%, docs +0.13% / −23%, third-party −0.75% / −11%, source +4.95% before the retry, then byte for byte. Whole engine, busy Mac: photos pack 0.270 → 0.207 s, third-party 2.256 → 1.493 s and unpack 1.502 → 0.891 s, docs +0.04 MB (0.1%).
**Verdict and why:** Taken. Tables 18/17 stay: zstd's own 17/16 are 9% faster on Silesia for 0.9% but win no row (Parked).
**Where the code is:** `engine/arca-core/src/codec.rs` (`FAST_TABLES`), `engine/arca-core/src/pack.rs`.
**Revisit if:** a Fast that favours time is wanted (`ARCA_EXP_FAST_TABLES=17,16`).

### Big files and the extraction scheduler
**When:** 2026-10-29; 2026-10-30 (spans at Fast only).
**What was tried:** Unpacking held at most 20 decoded blocks; Firefox's 352 MB `XUL` spans 44 and held them all, so past 20 the scheduler decoded one at a time. More are admitted while cores are idle under the same 2 GiB cap. Blocks that hold up the most rebuilding decode first (a photo's headers sit in a late block). Streams over 128 MiB are chunked in 64 MiB spans.
**Measured:** Third-party apps unpack 0.92 → 0.45 s; website 0.86 → 0.78 s; pack 1.32 → 1.03 s. 32 MiB spans lost 0.2% to chunks split at the seams, 64 MiB nothing at Fast. At Default, 64 MiB spans cost third-party apps 0.25% (Firefox's expanded `omni.ja` files dedupe against each other), so spans are Fast's alone and Default is back to 411.70 MB.
**Verdict and why:** Taken.
**Where the code is:** `engine/arca-core/src/unpack.rs`, `engine/arca-core/src/pack.rs`.
**Revisit if:** no path seen.

### F_BARRIERFSYNC instead of F_FULLFSYNC
**When:** 2026-10-30.
**What was tried:** The archive is fsynced with a write barrier before the rename, so a half archive cannot appear under its name; a full flush only on volumes without barriers.
**Measured:** Fast on DMGs, five rounds: full flush 0.311 s, `fsync` 0.143 s, `F_BARRIERFSYNC` 0.143 s, no sync 0.142 s. No rival flushes.
**Verdict and why:** Taken. Given up: the guarantee against losing the drive's cache in a power cut.
**Where the code is:** `engine/arca-core/src/pack.rs` (the writer).
**Revisit if:** no path seen.

### The incompressibility probe no longer routes
**When:** 2026-10-30.
**What was tried:** Chunks whose samples looked random went to the store lane, hidden from everything else in their block. The probe now routes nothing: a block made mostly of such chunks runs zstd -3 first and keeps that result when it saves under 5%. Fast is level 3 already and does not probe.
**Measured:** Fast and Default: Apple apps 71.70 → 67.05 MB (−6.5%) and 62.43 → 56.27 MB (−9.9%); third-party 462.37 → 455.94 and 411.70 → 402.43; website 270.40 → 265.76 and 223.72 → 217.88; PDFs 41.56 → 41.06 and 22.16 → 22.13; DMGs 619.69 → 615.46 and 619.67 → 615.01. The one loss was DMGs at Default, where level 12 on data that really is incompressible went 0.29 → 1.76 s, hence the zstd -3 first rule.
**Verdict and why:** Taken. The apps' chunks repeat earlier data that one chunk's sample cannot see.
**Where the code is:** `engine/arca-core/src/pack.rs`, `engine/arca-core/src/codec.rs` (`compress_hard`).
**Revisit if:** no path seen; long-distance matching in the quick pass was tried and reverted (Dropped).

### Chunking across cores, with seams where FastCDC would cut
**When:** 2026-10-30.
**What was tried:** A 50 MB file chunked on one thread held every core idle for 30% of Silesia's time at Fast. Files are chunked in 8 MiB spans across cores, with the seam exactly where FastCDC itself would cut (its gear table and a harder mask).
**Measured:** Seams at fixed offsets cost dedup 0.8%; a rolling hash on its own 0.8% too for shifts over 1 MiB; a FastCDC seam costs nothing: third-party apps find the same 32.86 MiB of duplicates.
**Verdict and why:** Taken at every level.
**Where the code is:** `engine/arca-core/src/hash.rs`, `engine/arca-core/src/pack.rs`.
**Revisit if:** no path seen.

### zstd workers at Default
**When:** 2026-10-30.
**What was tried:** With 64 MiB blocks a machine holds only a few, and one block on one core was what the rest waited for. A block takes two workers with 32 MiB jobs and a full-window overlap when cores are idle (file tasks count as busy); blocks over 20:1 go round again on one thread, which keeps static libraries' far repeats.
**Measured:** PDFs 1.76 → 1.40 s, Apple's apps 2.2 → 1.4 s, for 0.01% and 0.3%. Seen 2026-11-01: the app's first round on Apple's apps came out 0.3% smaller than the second (56.28 vs 56.44 MB) because the worker rule depends on blocks in flight; the loaders now count the larger of a tool's two sizes.
**Verdict and why:** Taken.
**Where the code is:** `engine/arca-core/src/pack.rs` (`BlockSink::close`), `engine/arca-core/src/codec.rs`.
**Revisit if:** a deterministic size matters more than the 0.05-0.3%.

### Blocks split at Max by a quick measure
**When:** 2026-10-30.
**What was tried:** When a block fills at Max, a quick zstd -1 of it whole and in two halves measures what a split costs; the block splits when that is under 0.5%, then the halves likewise, down to 16 MiB. Only under 512 MiB of input. Split text and code parts take match length 64 (no far repeats left to find); parts are queued longest first.
**Measured:** Silesia's blocks measure 0.11-0.46%, Apple's apps 0.72-1.04%, source up to 8.7%, PDFs 2.5%. Silesia Max 48.63 → 49.10 MB (+1.0%), make 23.6 → 13.1 s, extract 0.66 → 0.38 s. Longest first: 10.5 or 13.3 s from one run to the next, now 10.9 s. The minimum part went 32 → 16 MiB on 2026-10-31 and changed nothing: the quick pass refuses the quarters at 0.5%.
**Verdict and why:** Taken.
**Where the code is:** `engine/arca-core/src/pack.rs`.
**Revisit if:** lzt lanes are ever taken; 8 MiB parts or a looser line would widen the Silesia margin (the forced 16 MiB figures are what a 1% line would buy).

### Encoders at Max: eight under 256 MiB, six above
**When:** 2026-10-30.
**What was tried:** The number of block encoders by input size; ten was no better.
**Measured:** One run of eight on third-party apps took 254 s against 53 s, doubling its CPU, shortly before the Mac's battery died; the cause is not known.
**Verdict and why:** Taken; large inputs stay at six until that run is understood.
**Where the code is:** `engine/arca-core/src/pack.rs` (`FEW_BLOCKS_BELOW`). Knob `ARCA_EXP_ENCODERS`.
**Revisit if:** the 254 s run is explained.

### 32 MiB blocks at Max for inputs of 512 MiB or more
**When:** 2026-10-30.
**What was tried:** Large inputs take 32 MiB blocks at Max: LZMA spends less on each byte and six encoders get through more. A measured split did not suit them (the check and queueing fought the encoders).
**Measured:** Max, checkpoint → now: Silesia 48.63 → 49.10 MB (+1.0%), 23.5 → 13.1 s, 0.69 → 0.38 s; saved folder 221.96 → 223.64 (+0.8%), 30.0 → 23.0 s, 2.74 → 2.67 s; website mixed 194.78 → 197.17 (+1.2%), 41.0 → 31.6 s; third-party apps 339.46 → 350.23 (+3.2%), 48.8 → 40.8 s, 1.93 → 1.70 s. The measured split on large inputs: 0.8% in size, no faster.
**Verdict and why:** Taken. These four are the exceptions to "at least as small": Max traded size it had in hand for rows it was losing; Ultra stays the smallest.
**Where the code is:** `engine/arca-core/src/pack.rs` (`large_input_block`).
**Revisit if:** a faster LZMA-class encoder removes the need.

### A quick lane at Fast
**When:** 2026-10-30.
**What was tried:** zstd -1 with 2^15 tables for filtered binaries, ZIP, gzip and asset catalogs; everything else stays at level 3, which Silesia needs. Level 1 had looked no faster than level 3 because level 3's 2^18 tables were being used for it and most of its time was cache misses.
**Measured:** Third-party apps at Fast 0.52 s (Zipgeist tar.zst Fast 0.55 s), 504.4 MB (zstd -3 515.6 MB); the checkpoint had Fast at 465.87 MB in 2.19 s.
**Verdict and why:** Taken: +8.3% in size on purpose for about 4x the speed, still smaller than zstd -3.
**Where the code is:** `engine/arca-core/src/pack.rs`, `engine/arca-core/src/codec.rs`.
**Revisit if:** no path seen.

### One zstd context per encoder thread
**When:** 2026-10-30.
**What was tried:** A reused zstd context per encoder thread instead of a fresh one for every block.
**Measured:** Silesia at Fast 0.166 → 0.155 s, same bytes; source and apps unchanged.
**Verdict and why:** Taken.
**Where the code is:** `engine/arca-core/src/codec.rs` (`thread_local`).
**Revisit if:** no path seen.

### Photos at Fast: stored through zstd -1, not re-coded
**When:** 2026-10-31.
**What was tried:** `PackOptions::quick_photos`: a photo no coder touched goes to the image lane, whose codec at Fast is zstd -1 without long matches (the lane was unused at Fast). The light JPEG coder went off at Fast (Dropped). The rule it follows: a transform runs at Fast only if it keeps up with Fast's own codec, about 4-5 ns a byte for zstd -3; the arm64 filter does, the light coder at 13 ns a byte does not, and disk-image expansion is off at Fast for the same reason.
**Measured:** Zipgeist does tar then zstd -1 over the bytes, 0.06-0.08 s for the set at 92.06 MB. The photo set in Arca: 89,078,183 B in 0.035 s (was 78,261,816 B in 0.148 s), unpack 0.039 s (was 0.192); dedup finds all 15 duplicates where zstd -1's 512 KiB window misses them. Quiet: photos 89.08 MB, make 0.050 / 0.083 s, extract 0.050 / 0.068 s, against zstd -3 91.84 MB, 0.232 / 0.239, 0.211 / 0.213. Saved +3.62% (300.31 → 311.18 MB), mixed +4.03% (270.22 → 281.11 MB), apps +0.10%, the other five identical. Board 142 of 144.
**Verdict and why:** Taken; the three photo rows won. Nothing that reads JPEG symbols fits in the rows' time (the parse floor alone is 0.07 s on ten cores).
**Where the code is:** `engine/arca-core/src/pack.rs` (`quick_photos`).
**Revisit if:** a setting wants photos 12% smaller at 4x the time (the light coder, in tag before-engine-cleanup).

### Small inputs: whole blocks cut into zstd's own jobs
**When:** 2026-10-31.
**What was tried:** Default cut an input under 64 MiB into blocks of a core's share (1 MiB at least), and Fast every input, so every core had a block; a block that small loses every repeat farther back than itself. Blocks now stay whole and a small input's blocks are cut into jobs of a core's share (`CodecConfig::job`), each overlapping the one before by an eighth of zstd's job window.
**Measured:** Default, RAM disk: dickens 3,283,865 B in 0.39 s → −4.7% in 0.69 s (a quarter overlap −6.2% at 1.33 s; one block one thread −7.1% at 2.75 s); webster −1.7%; Burnish.dmg 6,954,315 B → −11.1%; a 31.5 MB source tree −4.4%; 44.9 MB of PDFs 19,848,266 B → −25.3% in 1.13 s; 33.2 MB of apps −3.2%; the 96.1 MB mixed slice, over the line, the same bytes. Fast gains 0.2-1.2% (the disk image 6.5%) for 2-9 ms on 24-70. Peak memory rises: PDFs 910 → 1254 MB, webster 650 → 821.
**Verdict and why:** Taken with the eighth: most of the size for the least time. No bench set is under 64 MiB, so the board does not move.
**Where the code is:** `engine/arca-core/src/codec.rs` (`small_overlap_log`). Knob `ARCA_EXP_OVLOG`.
**Revisit if:** small archives' memory matters.

### Fast on Silesia: the CPU floor, measured
**When:** 2026-10-30 (late); 2026-10-31 (02:35-07:00).
**What was tried:** The two Silesia Fast make rows (zstd -3 at 0.152 s piped from tar; Zipgeist's tar.zst Fast at 0.130 s, which is zstd -1 at 73.3 MB) taken apart with a per-file timeline and a histogram of running blocks. Kept: the CLI exits once the archive is reported instead of freeing buffers first (4 of the 8 ms outside the engine's clock).
**Measured:** Every file read and cut by 0.020 s; ten blocks run without a gap from 0.03 to 0.13 s; the tail is one or two blocks on the efficiency cores (8 MiB in 60-70 ms against 40-50). Blocks are 1.07-1.12 thread-seconds of work; the floor is about 0.125 s on the engine's clock. `zstd -3 -T10 -B8M` with Fast's parameters: 0.91 CPU-s, 0.14 s wall. Fast 0.146-0.153 s; in the quiet harness a tie (0.153-0.157 vs 0.151-0.154; the harness puts about 15 ms around either tool). Zipgeist does zstd -1's work, 0.57 CPU-s.
**Verdict and why:** The rows are at the machine's floor for zstd -3's ratio: a tie with zstd -3, and Zipgeist's zstd -1 speed is out of reach without giving up size. Everything tried for the tail and ramp was rejected (Dropped).
**Where the code is:** `engine/arca-core/src/pack.rs`, `engine/arca-cli/src/main.rs`.
**Revisit if:** a codec faster than zstd -3 at its ratio exists.

### Our JPEG coefficient coder (`jpegrans`, tag 6), with Lepton as fallback
**When:** 2026-10-29 (rounds 1-5 in `examples/jpegmodel`); 2026-10-31 (in the engine, pressure-tested).
**What was tried:** A baseline JPEG's coefficients re-coded with a static model per image: model G (expected magnitude from four neighbours, half-octave levels) plus block copy (a block with 6+ nonzeros may copy the AC values of an earlier block with the same count; 57% of the 12.1 M blocks repeat an earlier block exactly, which Lepton, Brunsli and packJPG do not model), one symbol per value (class plus top bit), sign raw, LOCO-I DC, four interleaved rANS states, 64-bucket tables per context, tables sent with a small adaptive binary coder. `encode_verified` decodes its own output before the engine keeps it. Lepton takes progressive JPEGs, tiny ones, and competes where ours saves under 15% (synthetic textures: ours 9.5-15%, Lepton 24%; our prefix counted at what zstd -1 makes of it). The engine version adds bounds checks on every read, a scalar twin of the NEON context arithmetic, and a decline over 4M blocks.
**Measured:** Model estimates on the photo set: A (no neighbours) 76.02 MB, B (neighbour contexts) 72.77, E (static tables) 74.46, G + block copy 70.63, with built-in tables 69.91. Built: 382 unique photos (94.74 MB): Lepton 70.79 MB at 10 MB/s both ways; ours 69.01 MB (−2.5%), encode 16 MB/s, decode 27 → 35 → 38 MB/s plus a Huffman writer at 95 MB/s (112 instructions per symbol, IPC 3.4); 10 threads 0.57 s on a busy Mac. In the engine at Default: photos 69.18 MB, make 2.10 s, unpack 0.71 s against Lepton's 71.10 MB, 2.19 s, 1.20 s (2.7% smaller, 1.7x faster out); saved folder 250.73 → 248.82 MB; mixed 217.88 → 215.97; apps 0.00%. 388 of 388 baseline photos exact; about 100,000 mutated photos, 52,406 parsed, 24,485 refused by the round-trip check, the rest exact; about 13,000 mutated stored streams decoded without a panic. Of 400 photos, 373 take ours, 12 Lepton, 15 are duplicates.
**Verdict and why:** Taken from Default up, agreed as the last format change before release. Rejected on the way: 256 buckets (table setup 0.27 → 0.47 s for no gain), zero-run symbols (zeros are 47% of symbols, so a run plus a value is not fewer symbols), lazy context groups (no change: the chain, not the work, sets the pace). Two traps: the raw-bit reader needs sixteen bytes of padding, and a failed round trip must fall to Lepton, not to raw storage.
**Where the code is:** `engine/arca-core/src/transform/jpegrans.rs`; `transform/jpeg.rs` (Lepton). Research: `engine/arca-core/examples/jpegmodel/` (`fast.rs`, `codec.rs`, the models), still in the tree.
**Revisit if:** two block rows are decoded at once with their own streams, or the format allows finer tables (Open).

### The code lane at Default: bsc with the binary lane's zstd as fallback
**When:** 2026-11-01 (06:30-08:10).
**What was tried:** After the app benchmark the user asked for a lane-by-lane experiment to decide on results. Default's code lane is on with `bsc:2e`, and the lane's fallback is a preset field (`PackOptions::code_fallback`, `codec::compress_auto_with`): the binary lane's zstd 12 at Default (static libraries stay with bsc, which LZP makes quick to decode; x86 tars and PE go to zstd 12 as before), LZMA2 fb 273 at Max and Ultra. Disk images, which had ridden on the code lane being zstd 5, got `Lane::Quick`.
**Measured:** Round 1: bsc:2 takes source's static libraries 3.77 → 2.11 MiB, source 9.46 MB in 1.74 s; on Silesia's x86 tars +1.4% for 1.18 s of extraction (BWT at 70 MB/s where LZP finds nothing). Round 2, candidate `cz`: source 9.46 MB, 2.00 / 0.51 s, 3 of 5 rows; saved −0.9%; mixed 214.3 MB; DMGs' make 1.04 → 1.97 s from routing, fixed. Taken, two rounds, every round trip identical: source 11.20 → 9.46 MB (0.52 s to extract), saved folder 248.8 → 246.7, mixed 216.0 → 214.3, the other six sets byte for byte the same; rows won 23 → 25 of 45 (source: Zipgeist 7z Normal and xz -6; 7-Zip Normal's 9.15 MB stays ahead). Through the rebuilt app: source 9.46 MB in 1.80 / 0.61 s, mixed 214.3 MB in 8.8 / 1.62 s. The 52 engine tests and 58 app tests pass.
**Verdict and why:** Taken: +2 rows for no time anywhere. The user kept the extraction margin ("only the code lane then") and left the text lane (Open).
**Where the code is:** `engine/arca-core/src/pack.rs`, `engine/arca-core/src/codec.rs`. Results `bench/results-2026-11-01-default-lanes-r{1,2,3}.jsonl`, `-default-codelane.jsonl`; scripts `bench/research/defaultlanes.py`, `defaultlanes_report.py`.
**Revisit if:** the text lane's extraction cost is solved (see Open).

### Presets locked for 1.0
**When:** 2026-10-31 (12:00-13:10); Default's code lane added 2026-11-01.
**What was tried:** Fast: zstd 3 lanes, the quick lane at zstd -1, photos through zstd -1, arm64 and x86 filters, nothing else. Default: zstd 15 text / 12 binary, bsc code lane with zstd 12 fallback, disk images on a zstd 5 quick lane with the rebuild budget, every transform, our JPEG coder. Max: bsc text and code, LZMA2 7 (fb 32) binaries, pixel codec. Ultra: bsc text and code, LZMA2 9e; it stays experimental ("Ultra*").
**Measured:** Quiet, every level on all nine sets (`bench/results-2026-10-31-presets.jsonl`), two rounds each, every round trip identical. Mixed folder: Fast 281.1 MB, 0.53 / 0.60 s; Default 216.0 MB, 8.54 / 1.32 s; Max 192.6 MB, 19.5 / 2.38 s; Ultra 189.3 MB, 29.4 / 2.44 s. Silesia: Fast 65.18 MB 0.16 / 0.06 s; Default 57.26, 5.04 / 0.10; Max 46.68, 6.55 / 0.57; Ultra 45.92, 13.6 / 0.81. Board 141 of 144; 1:1 by level 131 of 198 (Fast 42 of 45, Default 24 then 25, Max 36 of 54, Ultra 29 of 54). Ultra* adds 0.0 to 4.0% over Max for 5 to 117% more make time.
**Verdict and why:** Taken. The 67 rows lost are none of them a tuning matter: Default's size against LZMA-class normal settings (11), Default's make against near-store ZIPs (10, which are Fast's rows), the time transforms take at Max and Ultra for 19-55% smaller archives (32), Ultra's make time on the big folders (6), Silesia's frontier rows (7), DMGs Fast against WinRAR Fastest on size (1).
**Where the code is:** `engine/arca-core/src/pack.rs` (`PackOptions::preset`).
**Revisit if:** the website compares the rival's default button with the Zipling button that answers it; then the ZIP Normal rows are Fast's and green.

---

### The recovery record inside the archive (format version 4)
**When:** 2026-11-01, decided in the morning, built in the afternoon.
**What was tried:** Reed-Solomon parity inside the `.zlg`, off unless asked for, 1 to 50% of the data (3% the app's default), with the `reed-solomon-simd` crate (GF(2^16), O(n log n)). The record sits between the blocks and the index: parity covers the header and the blocks in 128 MiB regions of 64 KiB slices, a BLAKE3-128 hash per slice locates damage, and the record carries a copy of the index and the footer, its own header twice (before and after the parity) and a trailer, so a truncated tail is rebuilt from the copies and the record is found even when the footer or one header copy is gone. Repair writes a corrected copy and names the files it would lose when damage exceeds the budget. The separate `.recovery` file stays for ZIP and 7z.
**Measured:** Mixed-folder archive, 214 MB, 3%: compress 10.7 → 11.4-11.8 s, +6.75 MB (3.15%); check 0.14 s; repair of forty 2,000-byte damaged spots 0.73 s, the result extracts identical; extraction of an undamaged archive unchanged. Seven engine tests: scattered sectors, damaged index and footer, a tail truncated into the parity, damage past the budget (reported with the lost files), protecting an existing archive, version 3 archives opening.
**Verdict and why:** Taken: the only Mac archiver feature of its kind, worth a day before the format froze; off by default so benchmarks and speed are unchanged.
**Where the code is:** `engine/arca-core/src/recovery.rs`, `format.rs` (footer bytes 40..56, version 4), `arca-cli` (`pack --recovery`, `check`, `repair`, `protect`), `arca-ffi` (`arca_recovery_info/check/repair/add`); app: `CreateOptions.recovery`, `ArchiveModel.addRecord`, the compress sheet, inspector and menu.
**Revisit if:** the record's cost should be measured on the app's clock; a shift larger than the search window (four slices or 64 KiB) is not found. The realignment scan was built the same evening: 8-byte slice anchors in the record (record version 2), a search near the expected place, the hash as confirmation; repair rebuilds the copy piece by piece at the original offsets. The reader and the record were fuzzed (`examples/fuzzarchive.rs`): 10,000 cases found no panic or hang, and the one failure class (a repaired copy that did not open after an insertion or deletion near the record) is what the piecewise repair fixed.

## Parked (in the engine behind a flag)

### A codec with LZMA's ratio at a symbol coder's speed: the estimate (`lztok`)
**When:** 2026-10-30 (evening).
**What was tried:** The Silesia row against zstd -19 --long (52.60 MB, 29.3 s, 0.23 s) needs LZMA's size at zstd's decode speed. The SDK's encoder got a research tap (`token-hook` feature) that reports every parse decision; `examples/lztok` verifies the tokens and counts the bits four decodable coders would spend: S (zstd's way, semi-static), C (S with LZMA's contexts), A (C with adaptive counts), A+ (A plus alignment bits, position bits, and the match byte's top bits for the literal after a match).
**Measured:** All of Silesia (211.9 MB): LZMA 48.36 MB, zstd -19 52.64, S 53.60, C 52.53, A 52.03, A+ 50.23. LZMA's parse with zstd's coding is worse than zstd -19 on the binaries; the advantage is the modelling, and A+ is 3.9% behind LZMA and 4.6% ahead of zstd -19 (alignment bits alone took mozilla from +12.4% to +5.6%). The other condition, extraction under 0.23 s, needs about 300 MB/s a core, 2.5x LZMA's 120. Max with zstd lanes instead was measured first: binary lane zstd -19 51.18 MB / 16.8 s / 0.349 s; every lane zstd -22 52.57 MB / 41.2 s / 0.125 s; a per-block choice would never pick zstd (LZMA 8-13% smaller on every lane).
**Verdict and why:** The estimate said a build was worth it; it became `lzs` and then `lzt` (next two entries). The example stays as the estimator.
**Where the code is:** `engine/arca-core/examples/lztok.rs`; `engine/vendor/lzma-sdk` (`LzmaEnc.c`, `ARCA_TOKEN_HOOK`).
**Revisit if:** a parse priced for the new coder's costs is wanted; the estimator is where to count it.

### `lzs`: LZMA's parse coded with adaptive nibble models and rANS
**When:** 2026-10-30 (evening) to 2026-10-31 (00:00).
**What was tried:** The first build (`lzs.rs`): LZMA's parse through the tap, coded with adaptive 16-symbol models and two interleaved rANS states under A+'s contexts; models update as AV1's entropy coder does (four vector operations instead of a 12-step prefix sum), branch-free renormalisation, states swapping registers.
**Measured:** dickens 2.831 → 2.918 MB (+3.1%), x-ray +4.4%, samba +5.5%, mozilla +5.0% (the estimate said +4.2%; the independent-lane update costs 0.5%). Decode: dickens 133-144 MB/s against LZMA's 142; samba 219 vs 183; mozilla 120-128 vs 110. Measured file by file it was no faster than LZMA at all; a match token cost about 250 cycles (eight adaptive symbols, each a 16-lane compare and a dependent chain).
**Verdict and why:** Parked as a research module. Size as estimated, speed 1.1-1.2x LZMA against the 2.5x needed; the way out was tables (next entry). The LZNA-class plan (semi-static per-chunk literal tables under the same contexts, kind and length class as one symbol, four states) was not started.
**Where the code is:** `engine/arca-core/src/lzs.rs`; `examples/lzsbench.rs`, `examples/lzslit.rs` (the literal path alone).
**Revisit if:** an LZNA-class coder is wanted; the log puts that at years of work for its makers.

### `lzt`: the table coder, `Codec::Lzt` (tag 6), `ARCA_EXP_LZT`
**When:** 2026-10-31 (00:00-04:30 built and measured; 01:06-01:34 quiet run).
**What was tried:** The estimator showed that with LZMA's contexts, fixed frequencies come within 1% of adaptive ones on dickens, x-ray, xml and osdb: the contexts carry the modelling. `lzt.rs` keeps the parse and the contexts and makes every model a table: a symbol decodes with one lookup, a multiply and an add, two rANS states interleaved, raw bits in their own stream; each context rebuilds its own table after 32, 64, ... 4096 of its symbols (LZHAM's way), escapes for unseen symbols; kind and length as one symbol under LZMA's twelve-state history. In the engine: `Codec::Lzt` in the format, the block codec (the LZMA encoder runs with the tap capturing, its own output dropped) and the decode path; `ARCA_EXP_LZT` puts it on Max's text and binary lanes in LZMA2's place.
**Measured:** Silesia file by file, LZMA → lzt: all 48.358 → 50.936 MB (+5.3%), decode 1.8-2.4x (dickens 148 → 323 MB/s, mozilla 114 → 217, nci 500 → 970); about 3% smaller than zstd -19 --long with a decoder twice as fast as LZMA's. Steps: fixed tables per block +2.5% dickens at 292 MB/s; compact header and 16-byte copies 383 MB/s (2.5x); per-segment lagging tables worse everywhere and −25% decode, dropped; per-context rebuilds +1.4% dickens at 313 MB/s. In the engine on Silesia: Max LZMA2 64 MiB blocks 49.41 MB, 13.2 s, 0.362 s; lzt 64 MiB 50.52 MB, 14.3 s, 0.320 s; lzt 16 MiB 50.18 MB, 12.3 s, 0.222 s; LZMA2 16 MiB 49.15 MB, 12.1 s, 0.341 s. Every set, Max → Max with lzt lanes and 16 MiB blocks: Silesia +1.6%, photos same, saved +5.2%, mixed +6.5%, DMGs +5.7%, third-party +8.2% (crosses 7-Zip Ultra's 355.52), Apple apps +14.8% (crosses 54.10), source +16.6%, PDFs +20.7%; extraction 1.3-2x faster everywhere. Half the loss is the block size, half the codec. Quiet `arca-max-lzt`: Silesia 50.18 MB, 12.0-12.1 s, 0.23-0.25 s against zstd -19 --long's 52.65 MB, 28.3 s, 0.241 s: the row won, board 138 of 144 at the time.
**Verdict and why:** Parked behind `ARCA_EXP_LZT`. It cannot replace Max (15-21% bigger on PDFs, source and apps); as a setting beside Max it takes the Silesia extract row, and whether it stands beside Max is the user's call. The rest of the gap is modelling depth on high-entropy binaries (mozilla's literals and matched literals) and far distances' raw bits that both pay alike.
**Where the code is:** `engine/arca-core/src/lzt.rs`, `engine/arca-core/src/codec.rs`, `format.rs` (tag 6), `pack.rs` (`ARCA_EXP_LZT`); `examples/lzsbench.rs --lzt`. Results `bench/results-2026-10-31-max-lzt.jsonl`.
**Revisit if:** a matched-literal model in LZMA's shape, 64 distance slots and 12-bit literal tables close the gap on binaries; or a Max setting that extracts 1.6x faster for 1.6% in size is wanted on the board.

### The pixel codec at Default (`ARCA_EXP_PIXELS`, `ARCA_EXP_PIXEL_GAIN`)
**When:** 2026-10-28.
**What was tried:** The Max pixel codec switched on at Default, with a threshold: only images where it saves a byte per N pixel bytes (`ARCA_EXP_PIXEL_GAIN=N`, default 50).
**Measured:** Saved folder: 245.48 → 233.57 MiB (−4.9%), make 9.4 → 7.8-8.2 s, but extraction 2.74 → 3.0-3.3 s before the faster decoder. After it: off 245.48 MiB, 2.45-2.68 s, 14.5 s CPU; a byte per 50 pixel bytes 234.63 MiB, 2.48-2.54 s, 16.2 s CPU; per 30 235.19 MiB, 2.57-2.65 s; wherever it wins 233.57 MiB, 2.55-2.63 s, 16.6 s CPU. Make time drops 0.5-1.3 s with it on.
**Verdict and why:** Parked: extraction gets about 0.05-0.15 s slower, and the user's rule for Default is the extraction margin. The images it keeps are photo-like, the slow case for the decoder (101 MB/s).
**Where the code is:** `engine/arca-core/src/pack.rs` (`png_pixels`, `png_pixels_per_byte_saved`), `transform/pixels.rs`.
**Revisit if:** photo-like decoding gets past about 150 MB/s, or a Default that trades 0.1 s of extraction for 5% is wanted.

### The image lane's own block size (`--image-block-mib`)
**When:** 2026-10-28.
**What was tried:** Default's tail was two 64 MiB image blocks (zstd-12 on flat PNG rows, where its binary-tree search slows down). The image lane got its own block size (`image_block_size`), since smaller blocks for every lane cost the binary lane +1.35 MiB at 32 MiB.
**Measured:** Saved folder, image lane / total / make: zstd-12 64 MiB 46.69 MiB / 245.47 MiB / 9.1-9.5 s; 32 MiB 46.80 / 245.59 (+0.05%) / 7.8-7.9 s; 16 MiB 46.86 / 245.65 / 7.4-7.7 s; 8 MiB 46.99 / 245.78 / 7.2-7.8 s; zstd-11 46.73 / 8.8 s; zstd-9 47.44 / 7.0 s; LZMA2 fb 32 41.04 / 43 s.
**Verdict and why:** Parked as a CLI option, not in the preset: it trades 0.05% of size for 16% of time, and the pixel codec at Default would remove most of this lane anyway. The zstd workers of 2026-10-30 took the tail a different way.
**Where the code is:** `engine/arca-core/src/pack.rs` (`image_block_size`), `engine/arca-cli/src/main.rs` (`--image-block-mib`, with `--block-mib` and `--text-block-mib`).
**Revisit if:** the pixel codec goes on at Default; the lane's remainder is then small.

### `Assets.car` expansion (`ARCA_EXP_CAR`)
**When:** 2026-10-29.
**What was tried:** Compiled asset catalogs keep most images LZFSE-compressed (`bvx2` blocks). With the pinned lzfse the decoded renditions (flat images, about 17x the LZFSE bytes) are expanded and rebuilt like DMG chunks.
**Measured:** Saved folder's 23 catalogs (38.2 MB): 12.08 MB of LZFSE streams decoding to 202 MB; on 10.45 MB of replayable streams, LZMA fb 32 as stored 8.398 MB in 1.3 s, decoded 5.963 MB (−29%) in 11.5 s. In the engine: Default 257.396 → 256.170 MB (−0.48%), make CPU 55.9 → 60.8 s, extract 2.62 → 2.78 s; Max 221.963 → 219.538 MB (−1.09%), 211.6 → 231.6 s CPU, 2.88 → 3.68 s. 64 MB of renditions: bsc (LZP 18/128) 3.354 MB in 0.67 s (decode 0.27 s threaded); LZMA fb 32 3.377 MB in 4.71 s; zstd-12 4.31 MB, worse than keeping the LZFSE. The installed apps hold 239 catalogs (669 MB) decoding to 4.58 GB.
**Verdict and why:** Parked. The codec decides it: cheap only with bsc blocks for this data, and the volume (11x) makes LZMA cost ~9x the CPU. Not decided at the checkpoint.
**Where the code is:** `engine/arca-core/src/pack.rs` (`asset_catalogs`), `transform/deflate.rs`.
**Revisit if:** renditions get a bsc lane, or the pixel codec is applied to decoded renditions (width, height and pixel format are in the rendition header).

### zstd's own tables at Fast (`ARCA_EXP_FAST_TABLES`)
**When:** 2026-10-31.
**What was tried:** Fast's binary-lane tables (hash 18, chain 17) against zstd's own (17, 16) and the sizes between; the 2x tables cost nothing on one thread, which is how they were chosen, and 33% more CPU on ten (1.5 MB of tables a thread in a shared 12 MB cache).
**Measured:** Silesia as one tar, 10 threads, 8 MiB jobs: 17/16 0.70 s CPU, 0.11 s wall, 66.45 MB; 18/17 0.89 s, 0.14 s, 65.52 MB. In the engine, every set at Fast: 18/16 +0.54% Silesia at 0.149 s; 17/17 +0.32% at 0.142 s; 17/16 +0.93% at 0.133 s (apps +0.21%, mixed +0.38%, docs +0.46%); 16/16 +1.34%. Quiet harness with 17/16: Silesia 0.153-0.157 s against zstd -3's 0.151-0.154, a tie.
**Verdict and why:** Parked: 9% off Fast's time on Silesia for under 1% in size, every size row keeping its margin, but it wins no row, so by the standing rule the tables stay at 18/17.
**Where the code is:** `engine/arca-core/src/codec.rs` (`FAST_TABLES`, `fast_tables()`).
**Revisit if:** a Fast that favours time over size is wanted; it is one constant.

### Lepton alone at Default (`ARCA_EXP_LEPTON`)
**When:** 2026-10-31.
**What was tried:** `ARCA_EXP_LEPTON=1` leaves photos to Lepton alone, for comparison with our coder.
**Measured:** Photos at Default: Lepton 71.10 MB, make 2.19 s, unpack 1.20 s; ours 69.18 MB, 2.10 s, 0.71 s. Saved folder 250.73 vs 248.82 MB; mixed 217.88 vs 215.97.
**Verdict and why:** Parked as the comparison switch; Lepton stays in the engine as the fallback regardless.
**Where the code is:** `engine/arca-core/src/pack.rs` (`jpeg_rans`), `transform/jpeg.rs`.
**Revisit if:** a photo set shows our coder behind Lepton beyond the 15% rule.

### The bsc fallback codec (`ARCA_EXP_BSC_FALLBACK`)
**When:** 2026-11-01.
**What was tried:** `ARCA_EXP_BSC_FALLBACK=zstd:12` (or `lzma2:7`, `lzt:7`) names the codec a bsc lane falls back to under 16:1, in place of the preset's field.
**Measured:** Default's code lane with the LZMA2 fallback: Silesia 53.18 MB in 14.5 s, mixed 209.4 MB in 26 s (the fallback doing the damage); with `lzma2:7` on the whole lane Silesia 53.49 MB in 11.7 s / 0.63 s; with zstd 12 (taken) no time cost anywhere.
**Verdict and why:** Parked as the research knob; the preset field carries the decision.
**Where the code is:** `engine/arca-core/src/codec.rs` (`bsc_fallback`).
**Revisit if:** a lane wants a different fallback than its binary lane's codec.

### The small-input job overlap (`ARCA_EXP_OVLOG`)
**When:** 2026-10-31.
**What was tried:** How much of the block before it a small input's zstd job sees: 9 the whole window, 8 half, 7 a quarter, 6 an eighth (the default).
**Measured:** dickens: eighth −4.7% at 0.69 s, quarter −6.2% at 1.33 s, half −7.0% at 2.65 s; webster −1.7% / −2.4% / −3.1%; PDFs −25.3% / −25.6% / −26.1%.
**Verdict and why:** Parked knob; the eighth is the preset.
**Where the code is:** `engine/arca-core/src/codec.rs` (`small_overlap_log`).
**Revisit if:** a size-first small-archive setting is wanted.

### The deflate knobs and the other research switches
**When:** 2026-10-28 to 2026-11-01.
**What was tried:** Switches left in the engine for measurement, none in a preset: `ARCA_EXP_CAP` (cap preflate's chain at the limit instead of refusing), `ARCA_EXP_STEPS` (steps per byte), `ARCA_EXP_DMG_BUDGET_MS` (the disk image rebuild budget), `ARCA_EXP_NO_LZFSE` (no LZFSE chunk expansion), `ARCA_EXP_NO_REPLAY` (preflate only, no zlib re-run) in `transform/deflate.rs`; `ARCA_EXP_NO_X86` (no x86 filter) in `transform/mod.rs`; `ARCA_EXP_FB` (LZMA fb) in `codec.rs`; `ARCA_EXP_ENCODERS` (cap on block encoders) in `pack.rs`; on the CLI `--lane LANE=CODEC:LEVEL`, `--block-mib`, `--text-block-mib`, `--image-block-mib`, `--disk-images on|off`. Also `ARCA_DEBUG_PROFILE` and `ARCA_DEBUG_TIMING` for profiles and timelines, `ARCA_MEM_BYTES` to simulate another Mac's memory.
**Measured:** Each has its numbers under the entry it serves: the chain cap under the step budget (capped to 256: 73.65 MiB, 4.81 s extract, 40.9 s rebuild; to 1024: 72.48 MiB, 5.70 s, 49.8 s), `ARCA_EXP_NO_LZFSE` under the DMG routes (zlib-only expansion 601.5 MB, 1.5 s, 0.51 s), `--lane` under every bsc entry.
**Verdict and why:** Parked. (`ARCA_EXP_JPEGLITE`, `ARCA_EXP_CODE_LANE`, `ARCA_EXP_MACHO_LANE` and `--text-codec` went with the cleanup of 2026-11-01; see Dropped.)
**Where the code is:** as listed; `engine/arca-cli/src/main.rs` for the flags.
**Revisit if:** a measurement needs one; they cost nothing while unset.

---

## Dropped (removed, evidence in)

### The xz codec with the patched vendored liblzma (format codec tag 2)
**When:** before 2026-10-28 (the patches); replaced 2026-10-28; removed 2026-11-01.
**What was tried:** Max and Ultra compressed binaries with liblzma (`-9e` class settings) through a vendored `liblzma-sys` 0.4.9 (xz 5.8.4) with two speed patches that changed no output byte: `TUKLIB_FAST_UNALIGNED_ACCESS` defined on arm64 and x86-64 as xz's own configure does (the crate's hand-written `config.h` left it out, so the BT4 match finder compared lengths byte by byte), and a prefetch of the hash bucket four positions ahead in `lzma_mf_bt4_find` and `lzma_mf_bt4_skip`.
**Measured:** The unaligned path: LZMA -9e had been 1.18x (binaries) to 1.27x (text) slower per block without it; the prefetch saved ~5% of cycles with ten encoders running (prefetching both children of each tree node was 15-20% slower); together 1.3-1.45x faster `-9e` blocks. Against the LZMA SDK on the Mach-O block: xz -9e 10.201 MB in 27.7 s against 7-Zip's 10.159 MB in 15.5 s with ~13% less CPU and a second match-finder thread.
**Verdict and why:** Dropped. The LZMA SDK (tag 4) is smaller, uses less CPU and spreads one block over two threads; nothing released used tag 2. The tag stays reserved.
**Where the code is:** removed 2026-11-01, in tag before-engine-cleanup: `engine/vendor/liblzma-sys/` (with `ARCA-PATCHES.md`), the `xz` arm of `codec.rs`.
**Revisit if:** no path seen; the SDK covers it.

### The PPMd text codec (format codec tag 3)
**When:** before 2026-10-28 (Ultra's text codec); measured 2026-10-28; replaced by bsc in the presets 2026-10-31; removed 2026-11-01.
**What was tried:** PPMd order 16 on Ultra's text lane (`--text-codec ppmd` after bsc took the preset).
**Measured:** Ultra on the saved folder: 210.36 MiB in 34 s, extracting in 5.05 s; one 60 MB PPMd text block takes 4.1 s to decode on one core; there it gained nothing over LZMA2 fb 273 (both 5.96 MiB of text), on Silesia it gave 44.73 MiB against Max's 47.53. Against bsc: text 6.254 vs 6.167 MB, Silesia 8.108 vs 7.321 MB, 3.086 vs 2.858 MB, bsc 2-3x faster both ways. Quiet run 2026-10-30, Ultra against Ultra + bsc: source extract 3.72 → 0.92 s, Silesia 5.91 → 0.84 s, mixed 6.74 → 2.80 s, mixed size 191.9 → 191.2 MB.
**Verdict and why:** Dropped: bsc is 0.4% smaller on the mixed folder and 2.4x faster to extract, with blocks that decode in parallel. The tag stays reserved.
**Where the code is:** removed 2026-11-01, in tag before-engine-cleanup: the `ppmd` arm of `codec.rs` and the `--text-codec` flag.
**Revisit if:** no path seen; a context-mixing codec for Ultra (Open) would be the next step up, not PPMd.

### Readers for archive format versions 1 and 2
**When:** version 1 before 2026-10-28; version 2 (the column index) 2026-10-28; version 3 (POLYVAL) 2026-10-29; the old readers removed 2026-11-01.
**What was tried:** Version 1 had BLAKE3 hashes and a row-wise index; version 2 the column-wise index, still under BLAKE3; version 3 the keyed POLYVAL hashes. The readers for 1 and 2 were kept through the research ("version 2 archives still open and extract", 2026-10-29).
**Measured:** Index sizes under the v2 entry above; the hash speeds under the POLYVAL entry.
**Verdict and why:** Dropped: nothing released used versions 1 or 2.
**Where the code is:** removed 2026-11-01, in tag before-engine-cleanup: the version 1 and 2 arms of `format.rs` and `wire.rs`.
**Revisit if:** no path seen; no archive of those versions exists outside the research sets.

### Building with `-C target-cpu=apple-m1`
**When:** 2026-10-28.
**What was tried:** A target-CPU flag for the whole engine.
**Measured:** Lepton and preflate within noise.
**Verdict and why:** Dropped.
**Where the code is:** nothing kept.
**Revisit if:** no path seen.

### xz-9 and zstd-19 as the binary and image lane codecs at Max (variants B, C, D)
**When:** 2026-10-28.
**What was tried:** With the step budget in place, cheaper codecs on the lanes that burn the CPU: B = xz-9 for binary and image lanes; C = B plus xz-9 text and zstd-19 image; D = zstd-19 image. Also the cheaper LZMA match finding in xz (`mf=hc4`, `mode=fast`, `nice=32`, `depth=12`).
**Measured:** Saved folder at Max: A (budget 64 only) 217.71 MiB, 80 s, 3.6 s, 403 s CPU; B 218.67 MiB, 56 s, 3.7 s, 286 s; C 221.17 MiB, 51 s, 3.9 s, 280 s; D 220.45 MiB, 51 s, 5.7 s (under load), 328 s. Per block, -9e is 0.95% smaller than -9 for 1.39x the time; zstd-19 is never worth it; LZMA pays most on Mach-O (30% under zstd-12) and least on photo-like pixels (13%). The cheaper match finding loses 2-18% and is still 6-22 s per pixel block.
**Verdict and why:** Dropped: no xz setting reaches Max under 20 s on this folder; that needed the pixel codec and the SDK's match finder, which came next.
**Where the code is:** nothing kept; the LZMA SDK replaced xz.
**Revisit if:** no path seen.

### 32 MiB binary blocks at Max
**When:** 2026-10-28.
**What was tried:** Smaller binary blocks so that fewer cores wait on one 64 MiB xz-9e block (each took 17-32 s with ten running).
**Measured:** Saved folder at Max: 64 MiB text and binary 210.84 MiB (binary lane 92.34); text 32 MiB 211.03 (text lane 6.16); text 16 MiB 211.16; binary 32 MiB with text 32 212.82 MiB (binary lane 94.12).
**Verdict and why:** Dropped: 32 MiB binary blocks cost 2% of the binary lane, not the lever; the two-thread match finder was. (32 MiB blocks did come back for inputs of 512 MiB or more, a different trade; see Taken.)
**Where the code is:** nothing kept beyond the `--block-mib` flag.
**Revisit if:** no path seen.

### Thread QoS classes
**When:** 2026-10-28.
**What was tried:** Block threads at user-interactive; then block threads high and file tasks at utility.
**Measured:** Block threads high alone: 23.5 vs 23.6 s, no change. Both: 21.5 vs 23.1 s once, then 33-38 s against 23 s in three alternating rounds (macOS moves utility threads to the efficiency cores under load).
**Verdict and why:** Dropped: too unpredictable.
**Where the code is:** nothing kept.
**Revisit if:** no path seen.

### Narrowing preflate's candidates after 2 MB of plain text
**When:** 2026-10-28.
**What was tried:** Running only the leading hash candidate once 2 MB of a stream has been analysed (estimation was 25% of preflate's samples).
**Measured:** 2% faster, 2 PNGs lost.
**Verdict and why:** Dropped: fails "at least as small".
**Where the code is:** nothing kept.
**Revisit if:** no path seen.

### Apple's LZRAVEN and LZMESH
**When:** 2026-10-28.
**What was tried:** The macOS 27 SDK's `COMPRESSION_LZRAVEN` (0xD05) and `COMPRESSION_LZMESH` (0xE05) through `compression_encode_buffer`, single thread, on the dumped 64 MiB blocks (tool `rv.c` in a scratchpad).
**Measured:** Compressed MB / encode s / decode s. Synthetic pixels: LZRAVEN 0.789 / 2.39 / 0.018; LZMESH 1.070 / 0.06 / 0.007; xz-9e 0.678 / 17.4; xz-9 0.768 / 2.6. Source text: LZRAVEN 6.701 / 11.45 / 0.101; LZMESH 9.081 / 0.18 / 0.028; xz-9e 6.258 / 20.3; xz-9 6.371 / 15.3. Mach-O: LZRAVEN 11.655 / 18.13 / 0.211; xz-9e 10.201 / 25.8; xz-9 10.251 / 22.4.
**Verdict and why:** Dropped for Max: LZRAVEN sits between xz-6 and xz-9 on ratio (7% over xz-9e on text, 14% on Mach-O), compresses no faster, decodes ~2.4x faster than LZMA, and needs macOS 27 where Zipling supports 15+.
**Where the code is:** nothing kept.
**Revisit if:** Zipling ever requires macOS 27; its decode speed is noted.

### Apple's LZMA (libcompression level 6)
**When:** 2026-10-28.
**What was tried:** `COMPRESSION_LZMA` through the same buffer API, as a system-provided encoder.
**Measured:** Synthetic pixels 0.800 MB / 2.65 s / 0.034 s; source text 6.654 MB / 13.90 s / 0.240 s, against xz-9e's 6.258 MB.
**Verdict and why:** Dropped: larger than xz-9e and no faster than the SDK.
**Where the code is:** nothing kept.
**Revisit if:** no path seen.

### A long-range pre-pass before LZMA (`lrp`)
**When:** 2026-10-28.
**What was tried:** Take the code lane's repetition (static libraries, 46:1) off LZMA fb 273, which spent ~31 s on one 64 MiB block. A gear rolling hash over 64 bytes, content-defined anchors (~1 in 32), a 4M-slot table, matches verified and extended both ways, kept at MIN bytes or more; references stored as three LZMA-coded columns, the literals LZMA fb 273 coded. Round trip checked.
**Measured:** The first code block (67.1 MB), one thread: LZMA fb 273 alone 1.391 MB, 33.7 s; pre-pass MIN 256 (95,729 references cover 52.8%) 1.276 + 0.295 = 1.571 MB, 17.2 s; MIN 1024 1.398 MB, 31.9 s; MIN 4096 1.397 MB, 32.2 s; MIN 16384 1.394 MB, 32.9 s.
**Verdict and why:** Dropped: twice as fast at MIN 256 but 13% bigger. LZMA codes the same matches in ~1.2 bytes each (repeat distances, context-coded lengths), the reference columns in ~3.1; even free references would only reach 1.276 MB; whole duplicate objects are already removed by chunk dedup. FreeArc's REP has the same trade-off.
**Where the code is:** `examples/lrp.rs`, removed 2026-11-01, in tag before-engine-cleanup.
**Revisit if:** no path seen; bsc later took the code lane at 62x LZMA's speed.

### Hint-guided LZMA
**When:** 2026-10-28.
**What was tried:** The pre-pass hands LZMA its repeats ([start, end, distance]); inside one, the encoder takes it as a single match (checked byte for byte) and skips the search, optionally without inserting the skipped positions into the tree. Patched in a copy of the SDK's encoder and reverted.
**Measured:** Hints of 256+ bytes: 1.524-1.528 MB (+10%) in 23.3 s without inserting, 33.4 s with; 512+: 1.409 MB (+1.3%) in 27.9 s; a hint only where LZMA's own longest match is shorter: 1.390 MB in 32.6 s.
**Verdict and why:** Dropped: LZMA already finds these repeats, and its optimal parse codes them ~10% better than one long match each (repeat distances, split points). The time is the parse over long repeats, not missed matches.
**Where the code is:** reverted the same day; the SDK files are unchanged.
**Revisit if:** no path seen.

### Hash-chain match finders and fewer search cycles (`lzone`)
**When:** 2026-10-28.
**What was tried:** LZMA SDK match finders other than bt4 on the code block at fb 273 and on the Mach-O block at fb 32; fewer search cycles (`mc`).
**Measured:** Code block at fb 273: bt4 1.391 MB, 33.7 s; bt4 with 32 cycles 1.436 MB (+3.2%), 27.5 s; hc4 1.452 MB (+4.4%), 30.4 s; hc5 1.435 MB (+3.2%), 31.1 s; bt2 / bt3 1.400 / 1.397 MB, 34.4 / 34.7 s. Mach-O at fb 32, one thread: hc4 10.800 MB / 18.7 s, hc5 11.005 MB / 18.4 s against bt4 10.529 MB / 22.6 s (+2.6-4.5% for 17-19% less CPU). Cycles (default 32): 24 +0.28% for 2.4% less CPU, 16 +0.85% for 10%, 8 +3.0% for 29%.
**Verdict and why:** Dropped: every cheaper finder fails "at least as small"; the profile is 42% `GetMatches`, 26% `Skip`, 31% the optimal parse, so the search is not the whole cost.
**Where the code is:** `examples/lzone.rs`, removed 2026-11-01, in tag before-engine-cleanup.
**Revisit if:** a size loss of 1-3% is ever acceptable at Max for ~10-20% of CPU.

### One LZMA2 block encoded in parallel segments
**When:** 2026-10-28.
**What was tried:** State reset without a dictionary reset at segment boundaries; each segment's match finder first indexes the 24-32 MiB before it; a shim including `LzmaEnc.c`.
**Measured:** A 32 MiB window alone costs +0.03-0.08% on Mach-O (16 MiB: +3.5-17%, universal binaries repeat 16-32 MiB apart). 64 MiB Mach-O block, 16 MiB segments, 32 MiB window: 10.538 MB, 42.7 s CPU, 12.3 s wall, against 10.536 MB, 17.3 s CPU, 12.2 s wall with the two-thread match finder.
**Verdict and why:** Dropped: indexing the preload costs as much as coding it; the two-thread match finder gives the same wall time for 2.5x less CPU.
**Where the code is:** the shim was removed the same day; nothing kept.
**Revisit if:** no path seen.

### ARM64 code in byte planes
**When:** 2026-10-28.
**What was tried:** The apps' `__text` (57.6 MB) split into four byte planes before LZMA and bsc.
**Measured:** LZMA fb 273 13.15 → 17.79 MB; bsc 17.26 → 22.03 MB.
**Verdict and why:** Dropped: LZMA stays best on raw machine code.
**Where the code is:** nothing kept.
**Revisit if:** no path seen.

### Mach-O split into code, strings and tables, the rest to bsc (`machosplit`)
**When:** 2026-10-28.
**What was tried:** Mach-O files are ~60% machine code and ~40% sections that look like text (method names, C strings, Swift names, symbol tables). The 24 Mach-O files of the apps set (99.5 MB) were arm64-filtered as packing does and split into code (61.5 MB), strings and symbol names (13.2 MB) and everything else (24.8 MB).
**Measured:** One thread, dictionary over the whole input: whole LZMA fb 32 14.545 MB / 30.4 s, fb 273 14.000 / 40.2 s; code fb 32 9.166 / 21.8 s, fb 273 8.704 / 26.4 s; not code (38.0 MB) fb 32 5.360 / 7.6 s, fb 273 5.272 / 11.7 s, bsc (LZP 18/128) 7.865 / 1.4 s, bsc without LZP 7.821 / 1.4 s; strings fb 32 1.423 / 2.5 s, fb 273 1.404 / 4.1 s, bsc 1.524 / 0.37 s, without LZP 1.426 / 0.36 s. Code and the rest as two LZMA streams: 14.526 MB (−0.13%) for 3% less CPU; three streams 14.626 MB.
**Verdict and why:** Dropped: bsc loses by 46% on the non-code part (binary tables are LZ data) and only ties on the strings; splitting the strings out costs LZMA 0.1 MB of cross-references; bsc's only gain is ~2 s of CPU in 30 at 7x the decode time.
**Where the code is:** `examples/machosplit.rs`, removed 2026-11-01, in tag before-engine-cleanup.
**Revisit if:** no path seen.

### Multi-symbol size classes inside Lepton
**When:** 2026-10-28.
**What was tried:** Lepton's decoder profile (decode only): 7x7 coefficients 29%, edge coefficients 19%, per-block driver 12%, Huffman output 12.5%, DC prediction 7%, DC 6%, non-zero counts 14%; the coding parts ~67%. A shadow model in a scratchpad copy: wherever Lepton codes a coefficient's size class as a unary chain of binary decisions, a 16-symbol adaptive table for the same context records what it would cost.
**Measured:** Photos (98.4 MB → 73.8 MB): 7x7 classes Lepton 22.233 MB with 249 M decisions, tables 22.596 MB with 134 M symbols (+1.6%); edge 10.952 MB / 148 M against 11.052 MB / 58 M (+0.9%); DC 3.754 MB both (46 M decisions, 21 M symbols). Best variant +0.63% of Lepton's output (final rate 7); finer warm-up +0.83%, two-speed tables +0.70%.
**Verdict and why:** Dropped: fails "at least as small", and size classes are only 443 M of Lepton's 930 M decisions, so the coder would do ~25% less work, not a multiple. The JPEG lever needed a different idea (our static-table coder).
**Where the code is:** a scratchpad copy of Lepton, nothing kept.
**Revisit if:** no path seen.

### Decoding image bands side by side, and branchless top-bit decoding (pixel codec)
**When:** 2026-10-28.
**What was tried:** Two or four bands of a PNG decoded side by side on one core so independent chains overlap; a branchless rANS refill; a branchless top bit (decode always, keep when needed).
**Measured:** docs PNGs, 1 GB of pixels: engine before 110 MB/s; two or four bands 108 / 107 MB/s at +0.36 / +0.93% size; branchless refill 112 MB/s; branchless top bit 95 MB/s. On photo-like PNGs (154 instructions per byte) bands did not help either.
**Verdict and why:** Dropped: the decoder is bound by instruction count, not by dependency chains; the register and table rewrite (Taken) was the answer.
**Where the code is:** reverted; nothing kept.
**Revisit if:** no path seen.

### Mach-O in the code lane (`ARCA_EXP_MACHO_LANE`)
**When:** 2026-10-28 (at Max); 2026-11-01 (at Default, "Default, lane by lane"); the switch removed 2026-11-01.
**What was tried:** Mach-O routed to the code lane so it gets that lane's codec: long LZMA matches at Max in 2026-10-28; at Default, zstd 15, zstd 19, lzma2:7 and lzt:7 on a Mach-O lane of its own.
**Measured:** Max, 2026-10-28: code lane fb 273 with Mach-O 212.55 MiB in 37.4 s against 211.69 MiB in 33.4 s with Mach-O in the binary lane (−0.06 MiB for +15 s). Default, 2026-11-01: zstd 15 is 4.4x the make time of zstd 12 for −0.4% (Apple apps 56.23 MB in 5.8 s); zstd 19 53.46 MB in 14.7 s; lzma2:7 51.33 MB in 12.4 s / 0.49 s (beats 7-Zip Normal, loses Zipgeist 7z Normal, xz -6 and WinRAR Normal on make: 1 row of 5 from 3); lzt:7 53.16 MB in 12.6 s. Third-party apps with lzma2:7: 342 MB in 39 s / 1.41 s, 3 of 5 rows from 2 at 6x today's make time, and the mixed folder's 95 MiB of Mach-O takes it from 9.0 to 18.6 s. Nothing under 2.5 s gains the 2.1% that 7-Zip Normal has on Apple's apps.
**Verdict and why:** Dropped. Mach-O only shrinks with LZMA-class time, which loses more rows than it wins; that row is LZMA's.
**Where the code is:** removed 2026-11-01, in tag before-engine-cleanup (`ARCA_EXP_MACHO_LANE` in `pack.rs`). Results `bench/results-2026-11-01-default-lanes-r1.jsonl`.
**Revisit if:** a Mach-O codec exists that is smaller than zstd 12 at under 2.5 s on the apps set; the slice-delta transform (Open) is the one idea with that shape.

### Apple Silicon GPU for any stage (the stage analysis)
**When:** 2026-10-29.
**What was tried:** Agenda item 4: can the GPU take any stage? CPU by stage, saved folder, thread seconds.
**Measured:** Max make (~210 s): LZMA/zstd blocks ~75%, Lepton ~10%, preflate ~8%, pixel codec ~4%, hashing and chunking ~2%. Max extract (~25 s): Lepton 41%, preflate 18%, blocks 13%, writing files ~12%, pixels 10%. Fast make (~3.4 s): blocks ~75%, hashing and chunking ~35% before the hash work. BLAKE3 on the CPU cores already hashes 660 MB in well under 0.1 s.
**Verdict and why:** Dropped: every heavy stage is a serial adaptive model, which a GPU cannot spread out; published GPU codecs (DietGPU, GPULZ, Recoil; all CUDA, no Metal) get their speed from static tables and independent tiles at a cost in size the brief rules out; the data-parallel 2-5% would add transfers and launch latency for no wall-clock gain.
**Where the code is:** nothing built at this stage; see the next entry for the one stage that was.
**Revisit if:** a GPU-friendly coder (static-table rANS over independent tiles, or a GPU suffix sort for BWT) is adopted for size reasons.

### A GPU matchfinder for Fast (`bench/research/gpumatch`)
**When:** 2026-10-31 (09:45-10:45).
**What was tried:** The user asked why the two Silesia rows were a frontier rather than attacked with something new. zstd's match finder is a hash-table search that is parallel per position, the GPU shares memory with the CPU, and zstd 1.5.7's `ZSTD_compressSequences` takes an external parse and writes an ordinary frame, so the format would not change. Built as an Objective-C++ tool with a Metal kernel: every 8 MiB block split into G segments, each walked by a 1024-thread group in windows of 1024-8192 positions, zstd's long and short hashes into two per-block tables plus a threadgroup table; the CPU runs a lazy parse and calls `ZSTD_compressSequences` (level 3, window 2^23), decompresses and compares. One format trap: with no block delimiters a final sequence with matchLength 0 reads as a delimiter.
**Measured:** All of Silesia, 212 MB in 31 blocks, M1 Pro (16 GPU cores), best of three: 4 groups, 1 position per thread: kernel 241-270 ms, +7.0% against zstd -3's 66.16 MB; 16 groups 191 ms, +13.2%; 32 groups 187 ms, +15.6%; 4 groups 4 positions 226 ms, +9.5%; 8 positions 213 ms, +11.4%. CPU left over: the parse over the match array 420-450 ms, `ZSTD_compressSequences` 290-320 ms; plain zstd -3 on the same blocks 740-790 ms in all, the matcher about 60% of it, and ten CPU cores do that matching in about 55 ms of wall.
**Verdict and why:** Dropped, a dead end with numbers: the kernel is bound by random memory access (about 5 G accesses a second into tables that do not fit the GPU's caches, where each CPU core keeps its window in L2), so the GPU is 4x slower at the job and 7% bigger with a simple parse; even a free matcher would leave the entropy stage and the parse on the CPU, at most 1.6x when the Zipgeist row needs 2x. More segments save time and lose size, in the direction the table already shows.
**Where the code is:** `bench/research/gpumatch/gpumatch.mm`, removed 2026-11-01, in tag before-engine-cleanup.
**Revisit if:** a future codec's matcher is the whole cost.

### Edge prediction in the JPEG model (model F)
**When:** 2026-10-29 (round 2).
**What was tried:** Edge coefficients and DC predicted from pixel continuity across block edges, as residuals; with the gradient on both sides matched; and edge prediction as a context instead of a residual.
**Measured:** Coefficients, adaptive cost: B (neighbour contexts) 72.77 MB; F 79.15 MB; F with matched gradients 81.00 MB; F as context 72.81 MB.
**Verdict and why:** Dropped: a third of the set is screen graphics with hard edges, where pixels do not continue across block edges. Model G plus block copy won.
**Where the code is:** `engine/arca-core/examples/jpegmodel/modelf.rs`, still in the tree as the record.
**Revisit if:** no path seen.

### The light JPEG coder (`transform/jpeglite.rs`, transform tag 5)
**When:** 2026-10-29 (estimate as model H, then built as model R and put in the engine at Fast); 2026-10-30 (parse taken apart to the cycle); 2026-10-31 (switched off at Fast); removed 2026-11-01.
**What was tried:** For Fast, where the size bar is the fast rivals' 85.3-92.5 MB rather than Lepton's. Model H estimate: the same joint symbols with contexts of band and count bucket, static Huffman per image (81.68 MB, −13.8%). Built as something better: re-code JPEG's own symbols (DC size; AC run/size, EOB, ZRL) with static Huffman tables per image and per context (component class, band of the next position, neighbours' nonzero counts), keeping JPEG's raw bits; nothing rebuilt, one pass each way. Headers and tables to the binary lane, coded scans to the store lane. Checked by re-writing every scan from the parsed symbols while encoding, later by invariants (prefix codes, FF 00 stuffing, padding of ones before the next marker). Speed work: raw bits in their own stream, one loop over locals with raw pointers, Huffman tables by counting sort and a two-queue merge, code stream four symbols to a write.
**Measured:** 382 unique photos: 78.25 MB (−15.6% of scans), pack 0.21 s and unpack 0.20 s on 10 threads, 60 / 61 MB/s one core; after tuning 73 / 70 MB/s, 0.163 / 0.167 s. Fast in the real archive, busy Mac: 78.26 MB (−17.7% against storing), make 0.27 s, extract 0.23 s; quiet 78.26 MB, 0.19 / 0.20 s against zstd -3's 91.84 MB, 0.25 / 0.26 s and WinRAR Fastest's 92.5 MB, 1.02 / 0.30 s. Fuzz: 120,000 mutants, 36,593 accepted, every one back byte for byte, no crash. The parse: 157.3M symbols, 0.90-1.11 CPU-s, 18-20 cycles a symbol; every store removed saves 10%; a branch-free block loop 1.14 s (no change); two symbols a pass 1.19 s; one call per run of blocks 1.08 s (kept). Then Zipgeist re-measured: it stores the JPEGs in 0.06 s; the parse floor of 8 cycles a symbol is still 0.07 s on ten cores before the rest of the pack.
**Verdict and why:** Dropped from Fast and then from the engine: nothing that reads JPEG symbols fits in the photo rows' time, only byte-level LZ does, and Fast stores photos through zstd -1 (Taken). It stayed in the engine for a day for a setting wanting photos 12% smaller at 4x the time, then went in the cleanup; the tag stays reserved. Also rejected on the way: the prefix in the store lane instead of the binary lane (nothing measurable, cost 2.5 MB), reusing its buffers (no gain), a refill on every symbol (56 MB/s), bands from the position's top bit (0.6 MB bigger).
**Where the code is:** removed 2026-11-01, in tag before-engine-cleanup: `engine/arca-core/src/transform/jpeglite.rs`, `ARCA_EXP_JPEGLITE`. The research form is still in the tree: `engine/arca-core/examples/jpegmodel/lite.rs`, `modelh.rs`, `modelr.rs`.
**Revisit if:** a level between Fast and Default wants photos 12% smaller at 4x Fast's time; or two restart intervals interleaved per thread cut a third off the parse (Open).

### Level 8-9 PNGs re-run with the fast zlib search
**When:** 2026-10-29.
**What was tried:** Re-creating level 8-9 PNGs with the fast zlib search, as the DMG chunks are.
**Measured:** Saved folder, two alternating runs: Default 257.40 → 256.76 MB (−0.25%) for 9.0-9.4 → 16.5-18.2 s to make and 2.2 → 4.7-4.8 s to extract; Max 221.96 → 219.66 MB (−1.0%) for 30.1-32.9 → 36.8-42.1 s and 2.7-3.0 → 4.9-5.3 s.
**Verdict and why:** Dropped: their pixels are flat images, ~30x their Deflate bytes, so every byte saved costs a lot of re-running; preflate keeps them at Max as before and Default still skips them.
**Where the code is:** nothing kept; the search itself is in `engine/vendor/zlib`.
**Revisit if:** PNG token coding (Open) makes the rebuild cheap.

### Opening files relative to their folder
**When:** 2026-10-29 and 2026-10-30.
**What was tried:** `openat` relative to the parent directory instead of full paths.
**Measured:** 20 µs against 18 µs per open; `lstat` shows the path walk is cheap.
**Verdict and why:** Dropped.
**Where the code is:** nothing kept; `examples/openbench.rs` measured it (removed 2026-11-01).
**Revisit if:** no path seen.

### zstd levels 1 and 2 at Fast
**When:** 2026-10-30.
**What was tried:** Level 1 or 2 instead of 3 across Fast; level 1 or 2 only on the hard files.
**Measured:** Across Fast: cost Silesia its lead over zstd -3. On the hard files: x-ray +15%, sao +12% at level 1; +13% at level 2.
**Verdict and why:** Dropped: Silesia needs level 3. Level 1 came back only for the quick lane (filtered binaries, ZIP, gzip, asset catalogs).
**Where the code is:** nothing kept.
**Revisit if:** no path seen.

### Tiny DMGs expanded at Fast
**When:** 2026-10-30.
**What was tried:** Disk-image expansion at Fast for small images.
**Measured:** 0.06 s more; four rows lost to win one.
**Verdict and why:** Dropped; Fast stores disk images.
**Where the code is:** nothing kept (`--disk-images on` still allows it per run).
**Revisit if:** no path seen.

### Compressible DMG chunks in their own lane
**When:** 2026-10-30.
**What was tried:** A separate lane for the chunks that compress.
**Measured:** Worse (no figure recorded beyond "worse").
**Verdict and why:** Dropped; disk images got the quick zstd 5 lane instead.
**Where the code is:** nothing kept.
**Revisit if:** no path seen.

### zstd levels picked per block from a sample
**When:** 2026-10-30.
**What was tried:** A per-block level chosen from a sample of the block.
**Measured:** 0.09 MB.
**Verdict and why:** Dropped: nothing to gain.
**Where the code is:** nothing kept.
**Revisit if:** no path seen.

### Smaller blocks at Fast (2 and 4 MiB; a 2 MiB binary lane)
**When:** 2026-10-30; 2026-10-31.
**What was tried:** 2 or 4 MiB blocks across Fast; then a 2 MiB binary lane alone.
**Measured:** 2 MiB blocks: Silesia 0.133 s, +0.9% (source +2.5%); 4 MiB 0.142 s, +0.5%. A 2 MiB binary lane: Silesia 0.140 s (7% ahead of zstd -3) but +0.6-1.7% on apps, docs, saved, mixed and third-party apps and +0.2% on DMGs, where 616.77 MB would lose the row to zstd -3's 616.33.
**Verdict and why:** Dropped: wins Silesia's time and loses rows elsewhere.
**Where the code is:** nothing kept beyond `--block-mib`.
**Revisit if:** no path seen.

### A 256 KB chunk minimum at Fast
**When:** 2026-10-30.
**What was tried:** FastCDC minimum 256 KB instead of 128.
**Measured:** −0.007 s on Silesia; third-party apps +0.64% (dedup lost).
**Verdict and why:** Dropped.
**Where the code is:** nothing kept.
**Revisit if:** no path seen.

### The text lane without long-distance matching at Fast
**When:** 2026-10-30.
**What was tried:** Long-distance matching off on Fast's text lane too.
**Measured:** Silesia 65.01 MB but +0.1 CPU-s (nci at 11:1 trips the 8:1 retry); source +1.5%. Native zstd: the text lane pays a third more for LDM (0.36 vs 0.27 s on text) and gets 3.2% of the text back.
**Verdict and why:** Dropped: source trees need the far repeats.
**Where the code is:** nothing kept.
**Revisit if:** no path seen.

### Fast's tail and ramp on Silesia: six scheduling ideas
**When:** 2026-10-31 (02:35-07:00).
**What was tried:** Blocks that shrink with what is left (guided self-scheduling, down to 1 MiB); a queue of 40 blocks; three files open at a time in total; a lookahead of 48-128 MiB instead of 512; files of a later sort group held until the packer reaches it; the first blocks' buffers pre-mapped on every core; splitting the last wave of blocks across idle cores.
**Measured:** None moved Silesia outside 0.14-0.155 s. Self-scheduling: the tail ends compactly, the total does not move, +0.24-0.41% on text-heavy sets. 40 blocks: the feed finishes at 0.045 s instead of 0.09 but the encoders were never waiting. Three files open: folders of many small files take 1.3-3x longer (jpeg 0.146 → 0.437 s). Lookahead: no change. Held sort groups: the text files are cut by 0.01 s either way. Pre-mapped buffers: the packer's appends fall from 20 to 10 ms of CPU but mapping 120 MB first delays the first block by 10 ms. Last wave split: no gain.
**Verdict and why:** Dropped: the encoders are the bound; the floor is about 0.125 s on the engine's clock.
**Where the code is:** nothing kept.
**Revisit if:** no path seen.

### Long-distance matching in the quick pass of `compress_hard`
**When:** 2026-10-31.
**What was tried:** LDM on in the zstd -3 first pass that decides whether a mostly-incompressible block is worth more.
**Measured:** Apps 0.3% smaller, 38% slower at Default.
**Verdict and why:** Reverted.
**Where the code is:** nothing kept.
**Revisit if:** no path seen.

### Not expanding DMGs with a stronger image lane; zlib-only expansion
**When:** 2026-10-31 (01:40-02:10).
**What was tried:** For the DMG row against zstd -19 --long (610.27 MB, 0.45 s to extract): no expansion with the image lane at zstd 12, 15 or 19; expansion of zlib chunks only (`ARCA_EXP_NO_LZFSE`).
**Measured:** Default on the DMGs set: expanding 560.5 MB, 2.9 s, 1.05 s; zlib only 601.5 MB, 1.5 s, 0.51 s; not expanding 614.5 MB, 0.6 s, 0.21 s; image lane zstd 12 / 15 / 19 613.2 / 612.7 / 611.0 MB in 1.7 / 6.2 / 27 s, 0.16 s to extract. The rebuild: pinned LZFSE 3.73 CPU-s for 454 MB (122 MB/s a thread), pinned zlib level 1 2.99 CPU-s for 251 MB (84 MB/s), both at -O3 running their own algorithms. A side finding: a 7 MB image on its own compressed 12% worse than it could because small inputs got blocks sized to busy every core, which led to the small-input jobs (Taken).
**Verdict and why:** Dropped: not expanding never reaches zstd's size (repeats beyond a 64 MiB block that its 128 MiB window sees); LZFSE expansion is the better deal (41 MB for 3.7 CPU-s against 13 MB for 3.0). The rebuild budget (Taken) took the row.
**Where the code is:** `ARCA_EXP_NO_LZFSE` remains as a knob; nothing else kept.
**Revisit if:** an exact LZFSE encoder faster than Apple's exists (Open).

### Max's lanes at Default (`ARCA_EXP_CODE_LANE`, bsc text and code with the LZMA2 fallback)
**When:** 2026-10-31 (13:30-14:00); the switch removed 2026-11-01.
**What was tried:** The user asked whether Default could come in under Zipgeist's 7z Normal on size everywhere while staying fast (it misses on source 11.20 vs 10.47 MB, Silesia 57.3 vs 50.0, third-party apps 403 vs 380). Default with Max's text and code lanes (`ARCA_EXP_CODE_LANE=1 --lane text=bsc:1 --lane code=bsc:2e`, the code lane falling back to LZMA2 fb 273). Also "Max's codecs at Default" in the report (bsc text, LZMA2 7 binaries).
**Measured:** Source 8.47 MB in 0.88 s; Silesia 48.99 MB but 13.4 s to make and 0.75 s to extract against 4.8 and 0.09; mixed 204 MB but 23 s against 9.2; apps, DMGs and third-party apps unchanged. Max's codecs at Default: source 8.23 MB in 1.61 s, Silesia 49.41 MB in 6.14 s, third-party apps 350.23 MB in 39 s: make 2 to 40x slower, losing the normal-class rows Default wins on PDFs, apps, saved and DMGs. Third-party apps and Silesia's binaries only go under Zipgeist with LZMA-class codecs on the binary lane at 2-6x the time.
**Verdict and why:** Dropped, verdict with the user: Default stays as it is; no setting can be smaller than Zipgeist's 7z and faster than its ZIP at once. Counted against Zipgeist alone: 53 of 54 of its results beaten by some Arca setting. The code-lane half came back on 2026-11-01 with the zstd fallback (Taken).
**Where the code is:** removed 2026-11-01, in tag before-engine-cleanup (`ARCA_EXP_CODE_LANE` in `pack.rs`); `--lane` still allows the setting per run.
**Revisit if:** no path seen as a Default; the text lane alone is Open.

### Stronger zstd levels per lane at Default; LZMA2 and lzt on the code lane
**When:** 2026-11-01 (round 1, fifteen candidates).
**What was tried:** One lane changed at a time: text lane zstd 19; code lane zstd 19, lzma2:7, lzt:7, bsc:2e with the LZMA2 fallback; the whole binary lane at zstd 15 and zstd 19.
**Measured:** Text zstd 19: −6% on source at 2.6x the make time, −0.9% on mixed at 1.6x, nothing won. Code lane: lzma2:7 Silesia 53.49 MB in 11.7 s / 0.63 s, mixed 210.4 MB in 16.9 s; zstd 19 20-28 s; lzt:7 2.7x the make time for −5% on source; bsc:2e with LZMA2 fallback Silesia 53.18 MB in 14.5 s, mixed 209.4 in 26 s. Whole binary lane zstd 15: 1.6-4.5x the make time for −0.5 to −0.8%; zstd 19: 3.6-12x for −4 to −8%, every make row lost. The LZMA rows on Silesia (49.5-50.0 MB) are out of reach of every candidate: they need LZMA on tars and binaries both, at 2.4-4x the make time and 7x the extraction.
**Verdict and why:** Dropped, all but the code lane with the zstd fallback (Taken) and the text lane (Open).
**Where the code is:** nothing kept; `--lane` and `ARCA_EXP_BSC_FALLBACK` reproduce each run. Results `bench/results-2026-11-01-default-lanes-r1.jsonl`.
**Revisit if:** no path seen.

### The research tools removed in the cleanup
**When:** removed 2026-11-01; all in tag before-engine-cleanup.
**What was tried:** Sixteen example programs under `engine/arca-core/examples/` and one tool under `bench/research/`, each named in the entry it served: `bscbench` (libbsc against the LZMA SDK on dumped blocks), `deflbench` (one Deflate container expanded, its inverse timed), `filtbench` (PNG row filter timing), `leptbench` (Lepton's encode and decode CPU over a folder), `lrp` (the long-range pre-pass), `lzbench` (LZMA SDK settings on dumped blocks), `lzone` (one LZMA encode: match finder, fb, cycles, threads), `lzthreads` (one or two match-finder threads), `machosplit` (Mach-O into code, strings, tables), `objtime` (the costliest rebuilds in an archive), `openbench` (open and read the head of every file at several thread counts), `pixbench` (the pixel codec v0-v3 against LZMA), `pixfast` (faster pixel decoding; its `pixfast/codec.rs` module is still in the tree), `preftime` (preflate forward and inverse CPU per file), `scanbench` (the folder scan single-threaded and in parallel), `writebench` (where extraction's kernel time goes); and `bench/research/gpumatch/gpumatch.mm` (the GPU matchfinder). Still in the tree: `jpegmodel` (the JPEG coder's rounds), `lzsbench`, `lzslit`, `lztok` (the lzt codec), `treecmp` (compares two extracted trees).
**Measured:** see each entry.
**Verdict and why:** Removed to keep the engine tree to what ships; the tag holds them all.
**Where the code is:** tag before-engine-cleanup, `engine/arca-core/examples/` and `bench/research/gpumatch/`.
**Revisit if:** any entry above is reopened; check out the tag's copy of its tool first.

---

## Open (planned or measured only on paper)

### PNG token coding
**When:** idea 2026-10-28.
**What was tried:** Not built. For costly PNGs (level-9 encoders, nearly every position a short match), instead of replaying the encoder, store the Deflate token stream (literal / match) coded with a model that sees the image rebuilt so far, and rebuild by Huffman-coding the tokens again. The rebuild cost becomes entropy decoding, with no searches.
**Measured:** The size at stake: the step budget keeps almost none of the 5.85 MiB those files hold at Max (77.0 against 77.2 MiB on the docs part at budget 64); their rebuild is 59 s of CPU; 72 of 793 files cause 42.7 s of it.
**Verdict and why:** Open, the way back to the 2.7% Max gives up on PNG-heavy folders.
**Where the code is:** none.
**Revisit if:** Max's docs rows (4x 7-Zip Ultra's make time for 42-55% smaller) are to be attacked on time without losing size.

### Parallel rebuild of one Deflate stream
**When:** idea 2026-10-28.
**What was tried:** Not built. Preflate's only state across Deflate blocks is the hash chain; priming a predictor with the 32 KB before a block would let segments of one stream rebuild on all cores, removing a single-PNG critical path (3 s for one 16.7 MB image).
**Measured:** By 2026-10-28 evening Max's extraction was CPU-bound from 0.25 s to the end with no single-file tail (Lepton 10.4 s, Deflate 4.4 s, LZMA2 3.3 s, pixels 2.6 s), so it would not shorten extraction on the saved folder.
**Verdict and why:** Open; useful only for archives of a few huge Deflate files.
**Where the code is:** none.
**Revisit if:** a folder of a few large PNGs or PDFs shows a one-file tail in `ARCA_DEBUG_TIMING`.

### Fast LZMA2 (radix match finder)
**When:** noted 2026-10-28.
**What was tried:** Not tested. Conor McCarthy's Fast LZMA2 (BSD-3/GPL-2): a parallel radix match finder producing standard LZMA2; claims 20-100% faster than 7-Zip's for 1-5% larger output.
**Measured:** none here.
**Verdict and why:** Open, kept as a fallback for blocks-vs-cores; the size loss breaks the brief and the SDK's two-thread finder covered the case.
**Where the code is:** none.
**Revisit if:** inputs with fewer blocks than cores at Max become the bottleneck again.

### Similarity ordering of files into blocks
**When:** literature 2026-10-28 and 2026-10-29.
**What was tried:** Not built. Order near-duplicate files into the same block (compsort: −6 to −11.5% with xz on firmware and initrds), with MinHash over the CDC chunk hashes already computed; delta of near-duplicate binaries (zstd `--patch-from`).
**Measured:** none here; `--patch-from` did worse than LZMA on the arm64e slices (5.26 MB for Maps' second slice against 2.90 MB inside one LZMA stream).
**Verdict and why:** Open lead.
**Where the code is:** none.
**Revisit if:** a set with many near-duplicate files shows blocks missing matches that a 64 MiB window would otherwise see.

### Code-signature page hashes recomputed at extraction
**When:** counted 2026-10-28.
**What was tried:** Not built. Mach-O code-signature page hashes are incompressible and could be recomputed at extraction and checked at pack time.
**Measured:** Only 0.19 MB in 99.5 MB of app binaries (16 KB pages on current macOS), ~0.4% of the compressed apps set.
**Verdict and why:** Open, a small idea, not a lever.
**Where the code is:** none.
**Revisit if:** everything larger on the apps set is done.

### Mixing plus block copy for photos at Max and Ultra
**When:** estimated 2026-10-29 (round 2).
**What was tried:** Not built. Model M (three context models mixed with a logistic mixer and APM) scored 71.61 MB of coefficients; with block copy the estimate is about 69.3 MB, against Lepton's 70.79 MB on the same photos and our static coder's 69.01 MB.
**Measured:** M 71.61 MB adaptive; G + block copy 70.63 MB adaptive; static bound 68.69 MB.
**Verdict and why:** Open; our static coder already beats Lepton, and an adaptive mixer would be slow to decode, which only Max and Ultra could afford.
**Where the code is:** `engine/arca-core/examples/jpegmodel/modelm.rs` (the estimate).
**Revisit if:** Max and Ultra want photos smaller than Default's at Lepton's speed or less.

### A slice-delta transform for Apple's arm64e slices
**When:** measured 2026-10-29.
**What was tried:** Not built. Every large binary in the apps set holds two arm64e slices built from the same source (`__text` 15.46 vs 15.04 MB in Maps). A transform that matches functions across slices (`LC_FUNCTION_STARTS`) and diffs instructions with PC-relative fields mapped through the address shift, so slice 2 costs a fraction of slice 1 and the slices become independent blocks.
**Measured:** With xz -9 and the arm64 filter: Maps slice 1 alone 4.73 MB, slice 2 alone 4.79, both 7.63, the second slice's extra cost 2.90 MB (61% of alone); Mail 1.41 / 1.42 / 2.12 / 0.71 MB (50%). LZMA recovers only 39-50% of the overlap. If it reaches about 20% of a slice, Default on the apps set drops from 62.4 to about 58 MB (xz -6 59.0, WinRAR Best 58.7).
**Verdict and why:** Open; it was third in the build order of 2026-10-29 and was not reached. It is the one idea with the shape to take 7-Zip Normal's 2.1% on Apple's apps at Default (report, "Default: what is fixable").
**Where the code is:** none.
**Revisit if:** the apps rows at Default are the next target.

### WebP-style tools for flat images in the pixel codec
**When:** lead 2026-10-29.
**What was tried:** Not built. WebP lossless is 30-45% smaller than PNG on screenshots and icons; its tools (palette, 2D LZ77 with a colour cache) are what the pixel codec lacks on flat images, where it is weakest (1.75x xz above 30:1, so those go to the LZMA image lane).
**Measured:** The codec's ratio against xz by type: under 3:1 0.894, 3-6 0.854, 6-12 0.940, 12-30 1.185, over 30 1.753.
**Verdict and why:** Open lead.
**Where the code is:** none.
**Revisit if:** the image lane's flat images (117 MiB raw, 4.73 MiB compressed on the saved folder) are worth more than LZMA gives.

### bzip2 (UDBZ) and LZMA (ULMO) disk image chunks
**When:** lead 2026-10-29.
**What was tried:** Not built. UDBZ and ULMO chunks stay as they are; a 387 MB bzip2 installer was left out of the dmgs set for lack of bzip2 re-creation.
**Measured:** none.
**Verdict and why:** Open; needs an exact re-creation path for each encoder.
**Where the code is:** none.
**Revisit if:** such installers show up in the sets; `DeflateKind::Dmg` is the place.

### `Assets.car`: token coding of LZFSE streams, or the pixel codec on renditions
**When:** lead 2026-10-29.
**What was tried:** Not built. Two cheaper routes than expanding renditions to LZMA: code the LZFSE token stream (literals, lengths, distances) with an adaptive model and re-create the FSE coding from the tokens, no match search either way; or the pixel codec on decoded renditions (width, height and pixel format are in the rendition header).
**Measured:** The volume problem: decoded data is 11x larger (installed apps: 408 MB of LZFSE decoding to 4.58 GB); bsc on 64 MB of renditions 3.354 MB in 0.67 s against LZMA fb 32's 4.71 s.
**Verdict and why:** Open; `ARCA_EXP_CAR` (Parked) is the plain route.
**Where the code is:** none.
**Revisit if:** asset catalogs become a target (NordVPN's 73 MB alone in the third-party set).

### Profile-guided optimisation for release builds
**When:** measured 2026-10-29.
**What was tried:** An instrumented build trained on the saved folder (Fast, Default, Max, Max + bsc, Default + pixels) and Silesia and source at Ultra, then `-Cprofile-use`. Only the Rust code is profiled; LZMA, zstd, bsc and zlib are C built by Apple clang.
**Measured:** Saved folder: Fast make CPU 3.52 → 3.37-3.58 s; Default 54.7-56.4 → 54.5-54.6 s (−2%), extract CPU 14.1 → 13.4-13.6 s (−4%); Max 209.0-209.7 → 207.4-207.7 s (−1%), extract 20.6-20.8 → 20.1 s (−3%), wall 2.85-2.97 → 2.77-2.78 s. Sizes identical.
**Verdict and why:** Open: worth a step in release builds (train, then build with the profile), not a lever; the log does not record it being wired into the build.
**Where the code is:** none in the tree.
**Revisit if:** the release build pipeline is set up (`docs/ROADMAP.md`, phase 5).

### Two restart intervals of one photo interleaved on one thread (JPEG parse)
**When:** 2026-10-30; 2026-10-31.
**What was tried:** Not done. 244 of the 400 photos restart every MCU row; running two restart intervals interleaved on one thread overlaps two dependency chains. A flat parser with no end-of-block branch was also estimated.
**Measured:** Estimated a third off the parse for those photos, for a rewrite of the coder's event layout; the flat machine about 11% of the parse for a large rewrite; a two-symbol table lookup about 1.3x on the chain. None of it reaches the photo rows' 0.06 s.
**Verdict and why:** Open; it belonged to the light coder (Dropped) but applies to any JPEG symbol reader, including our coefficient coder's parse.
**Where the code is:** none.
**Revisit if:** a JPEG re-coder's packing time matters again at any level.

### Two block rows decoded at once in our JPEG coder
**When:** 2026-10-29 (round 5).
**What was tried:** Not built. The contexts allow a staggered wavefront, so two block rows with their own streams could run side by side in one core; also a table-driven Huffman reader for packing and finer bucket tables.
**Measured:** A replay with the contexts given takes 1.07 s of the 2.55 s decode (4.2 ns a symbol); the model's own work about 1.2 s; neither dominates.
**Verdict and why:** Open; the coder already extracts 1.7x faster than Lepton and it is in the format now, so this is a decoder-side change if the stream layout allows it.
**Where the code is:** none; `transform/jpegrans.rs` is the base.
**Revisit if:** photo extraction at Default (0.69 s against zstd -19's 0.25 s on the photo set) is a row to win.

### bsc on Default's text lane (not taken), and a faster bsc decode
**When:** measured 2026-10-28 (16 and 32 MiB blocks), 2026-10-30 (12 MiB), 2026-10-31 and 2026-11-01 (rounds 2 and 3 of "Default, lane by lane").
**What was tried:** `--lane text=bsc:1` at Default, with the text block tried at 2, 4, 8, 12, 16 and 32 MiB.
**Measured:** 2026-11-01, Default with bsc text (`tbsc`): source 11.20 → 10.21 MB in 0.93 s instead of 1.96; Silesia 57.26 → 53.07 MB in 2.3 s instead of 5.1; mixed 216.0 → 211.0 MB in 5.7 s instead of 9.0; extraction slower wherever text is: 0.39 → 0.50, 0.09 → 0.41, 1.35 → 1.81 s; docs 0.21 → 0.34 s, saved 1.56 → 1.76 s; rows 23 → 25 of 45, and with the code lane too (`tcz`) 28 of 45, source 8.47 MB in 0.89 s (smaller than 7-Zip Ultra's 8.51 MB in 21.9 s). The block size does not move extraction (source 0.67-0.74 s at every size) and costs size below 12 MiB (source 8.47 → 8.52 / 8.71 / 8.88 MB at 8 / 4 / 2 MiB): the time is bsc's decode itself (QLFC and the inverse BWT), not idle cores. The site's figures would go from 2.6x over 7-Zip and 5x over Finder on extraction to about 2.0x and 4.1x; make from 5.8x to 8x; size from 8% to 11% smaller.
**Verdict and why:** Open, not taken: 2-7% smaller and 37-54% faster to make wherever text is, but 1.3-4.6x slower to extract there; the user keeps the extraction margin. It stays an option on record.
**Where the code is:** `--lane text=bsc:1` on the CLI; the codec in `codec.rs`. Results `bench/results-2026-11-01-default-lanes-r2.jsonl`, `-r3.jsonl`.
**Revisit if:** libbsc's static QLFC coder and a wider parallel inverse BWT bring the decode down; the log names those two things as what to try before revisiting.

### An exact LZFSE encoder faster than Apple's
**When:** 2026-10-31.
**What was tried:** Not built. The DMG row against zstd -19 --long needs the rebuild about 15% faster with zlib-only expansion or 2.3x faster with both; the budget took the row at Default's 601.8 MB, but a faster LZFSE rebuild would let the budget expand more.
**Measured:** The pinned LZFSE rebuilds at 122 MB/s a thread (3.73 CPU-s for 454 MB), built at -O3 running its own algorithm; no cheap 2x in it.
**Verdict and why:** Open, named in the log as the research question left on disk images.
**Where the code is:** `engine/vendor/lzfse` is the base.
**Revisit if:** the Default DMG size (601.8 against 560.5 MB expanding everything) is worth a week on an encoder.

### Next steps for `lzt` on binaries
**When:** 2026-10-31.
**What was tried:** Not started: a matched-literal model in LZMA's shape (the length of the matching prefix as one symbol, the remaining bits under the prefix), 64 distance slots, 12-bit literal tables; a parse priced for the coder's own costs; 8 MiB parts or a looser split line for lzt lanes.
**Measured:** The gap on mozilla, samba and nci is +8.2-9.8%, modelling depth on high-entropy binaries; the estimator's A+ is +5.6% there, so about half is recoverable by modelling.
**Verdict and why:** Open; `lzt` is Parked until Max's size margin on apps and third-party apps (7% over 7-Zip Ultra) can take it.
**Where the code is:** `engine/arca-core/src/lzt.rs`, `examples/lztok.rs`.
**Revisit if:** a Max setting that extracts 1.3-2x faster is wanted on compressible sets without crossing 7-Zip Ultra.

### Memory: Max at 6-8 GB on the large sets
**When:** 2026-10-29.
**What was tried:** Not worked on, by the user's decision. The rule: no more than the hungriest rival on the same files; the 8 GB Mac test comes after Arca beats everyone.
**Measured:** Peak memory while compressing (the app, 2026-10-31): mixed folder Fast 1704 MB, Default 3972, Max 3644, Ultra 4213, against 7-Zip Ultra 6874, WinRAR Best 578, zstd -19 --long 1871, xz -9e 3656; third-party apps Max 3623, Ultra 5166, 7-Zip Ultra 8922. No Arca level comes near `ditto`'s 2-12 MB streaming.
**Verdict and why:** Open; within the rule today, and streaming like `ditto` is out of reach for a multi-core compressor.
**Where the code is:** `engine/arca-core/src/pack.rs` (the memory budget).
**Revisit if:** the 8 GB test is run with every rival under the same limit.

### The app's extraction overhead
**When:** 2026-10-31.
**What was tried:** Not done, since it costs no row: skip the listing call for a whole-archive extract, and take the safety walk's per-file cost down.
**Measured:** The app adds 2-7 ms to packing; extracting, about 30 ms before the engine starts (listing, JSON decode, format check, two main-actor hops) and about 9 µs a file after (the safety walk); worth about 0.1 s on a 7,700-file tree. AppleScript's transport adds 60 ms per round trip outside the app's path.
**Verdict and why:** Open; the thinnest app-level margin is photos at Fast, 43 ms against Zipgeist's 60 ms.
**Where the code is:** the app's `Engine.swift` (timing marks under `defaults write com.observantsystems.zipling timing -bool true`); `bench/research/appvsengine.py`.
**Revisit if:** app-against-app timing is published for small archives.

### Multi-threaded Lepton decoding for folders of a few huge photos
**When:** `docs/ROADMAP.md`, engine v2.
**What was tried:** Not built. Lepton decodes at ~10 MB/s per core; a folder of a few huge photos would wait on one file.
**Measured:** none; our coder now takes 373 of 400 photos at 35 MB/s a core, which reduces the case to the photos Lepton still takes (progressive, tiny, synthetic textures).
**Verdict and why:** Open, lower priority since the coefficient coder.
**Where the code is:** none.
**Revisit if:** a set of large progressive JPEGs appears.

### Recursive unwrapping (zip-in-zip, JPEG-in-PDF)
**When:** `docs/ROADMAP.md`, engine v2; `docs/DESIGN.md`, known gaps.
**What was tried:** Not built. Transforms applied to streams found inside expanded containers.
**Measured:** none.
**Verdict and why:** Open.
**Where the code is:** none; `transform/deflate.rs` already finds the inner streams.
**Revisit if:** a set shows JPEGs inside PDFs or nested archives as a measurable share.

### A WAV/AIFF predictor
**When:** `docs/ROADMAP.md`, engine v2.
**What was tried:** Not built.
**Measured:** none; no test set holds audio.
**Verdict and why:** Open.
**Where the code is:** none.
**Revisit if:** an audio set is added.

### A context-mixing text codec for Ultra; kanzi
**When:** `docs/ROADMAP.md`, engine v2; literature 2026-10-29.
**What was tried:** Not built. The literature: context mixing with exe models is 25-50% smaller than LZMA but symmetric and ~100x slower (ooffice: 7z 2426 KB, zpaq m5 1756, paq8px 1205); kanzi -8/-9 is ~15% smaller than bsc on mixed data at 2.5-3.5 MB/s, an Ultra-only candidate.
**Measured:** none here.
**Verdict and why:** Open; Ultra stays experimental and already loses make-time rows on the big folders (22-62 s against 15-50 s), so a slower text codec would need its own tier.
**Where the code is:** none.
**Revisit if:** an "archival" tier beyond Ultra is wanted.

### Encryption, versioned archives, split archives, shrink later block by block
**When:** `docs/ROADMAP.md`, backlog and engine v2.
**What was tried:** Not designed. Password-protected `.zlg` (every block encrypted, names hidden, a memory-hard key function); Reed-Solomon recovery records inside the archive; snapshots sharing unchanged chunks; parts on several files; re-packing a Fast archive to Max one block at a time. Each is a format change.
**Measured:** none.
**Verdict and why:** Open, parked until the engine is opened up again. The recovery record left this list on 1 November 2026 (Taken, below in that group); the format is now version 4.
**Where the code is:** none.
**Revisit if:** a release after 1.0 opens the format; the hash design (keyed POLYVAL, index under BLAKE3) is where encryption would start.

### Literature leads not tested: OpenZL, approximate LZ77, JPEG XL context trees, Brunsli
**When:** 2026-10-28 and 2026-10-29 (two background surveys).
**What was tried:** Not tested. OpenZL (Meta, BSD; format-aware, fits CSV and columnar data, format unstable); approximate LZ77 via string synchronizing sets (12-19x faster parsing, no code published); JPEG XL's learned context trees and per-channel predictor choice for the pixel codec (a small tree keeps decoding fast); Brunsli, the one JPEG coder faster than Lepton (encode 0.16 vs 0.5 s, decode 0.13 vs 0.23 s on one 1 MB image) for ~1% less saving.
**Measured:** none here. For machine code no published method reaches LZMA's size 3x faster with fast decoding; fast-decoding LZ codecs are 2-8% larger (zstd -22 +7.4%, brotli -11 +3.6%). For Deflate re-creation nothing new beyond zlib replay. No cheaper safe JPEG verification is published (Dropbox and Microsoft both decode everything).
**Verdict and why:** Open; our coefficient coder supersedes Brunsli's niche (35 MB/s against Lepton's 10, smaller than both).
**Where the code is:** none; the reports lived in session scratchpads.
**Revisit if:** a new content class (columnar data) or the pixel codec's flat-image gap is the target.
