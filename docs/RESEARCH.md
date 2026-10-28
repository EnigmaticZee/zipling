# Arca engine research log

Started 2026-10-28 on branch `zee/arca-research-77726f`. Backup: tag
`before-engine-research` (commit `cc46547`).

**Resuming?** Read this file top to bottom first. "State" says where the work is;
everything below it is the record.

## State

- Steps done: log, test sets (silesia pending a download OK), baseline profiles.
- Known fixes: Fast mostly done (see 4); Max PNG rebuild mechanism done, size
  trade-off needs the user's call (see 1); memory and blocks-vs-cores not started.
- Next: memory cap for Max; then deep research on Max make time (pixel codec,
  faster LZMA match finding), PNG token coding to win back size.
- Asked the user (2026-10-28): may Max grow ~3% for 2.7x faster extraction, and
  may I download Silesia?

## The task, in short

Make Arca much faster at every level (aim: ~50%+, "unimaginably fast"), while at
least as small, so each level beats every rival's comparable setting on speed and
size. Format may change freely until 1.0. Full brief:
`research_notes/arca-research-brief.md` (main checkout, git-ignored).

Rules that shape the work:
- Ask the user before any benchmark. Profiling runs to see where time goes are fine.
  Final numbers: quiet Mac, user away, alternating A/B.
- Keep the 9 `arca-ffi` functions and the Swift wrapper working; 58 app tests pass.
  No UI changes.
- Commits as Observant Systems, no AI lines, never push or merge without the OK.

## Machine

Apple M1 Pro, 8 performance + 2 efficiency cores, 32 GB, macOS 27.0.1.
Rust 1.99.1, 7-Zip 26.05 (`7zz`), zstd 1.5.7, xz 5.8.4.

## Targets per level

Saved folder: `research_notes/engine-bench.noindex/Mixed` (660,849,010 bytes,
11,050 files). Rival numbers from the 2026-10-28 head-to-head (busy Mac, so treat
as approximate until re-measured quietly):

| Level | Comparable rival | Rival make | Rival size | Rival extract | Arca today (make / size / extract) |
|---|---|---:|---:|---:|---|
| Fast | Zipgeist fastest (tar.zst) | 1.24 s | 341.0 MB | 1.6–1.9 s | 1.8–2.2 s / 315.9 MB / 2.2 s (app) |
| Default | Zipgeist standard 7z | 17.2 s | 285.2 MB | 3.0 s | ~11 s / 257 MB / ? |
| Max | Zipgeist best 7z | 20.0 s | 284.2 MB | 3.7 s | 117 s / 221.7 MB / 12.7 s |
| Ultra | Zipgeist best, 7-Zip Ultra | 20.0 s | 284.2 MB | 3.7 s | not measured |

Also in scope: ZIP. Mac-made ZIP (`ditto-mixed.zip`) extracts in 4.7 s via 7-Zip
on one core in the app; Zipgeist 2.1 s.

So the gaps, before any work: Fast make ~1.5x too slow; Max make ~6x too slow and
Max extract ~3.4x too slow; ZIP extract ~2.2x too slow. Default already wins both
ways but should get faster too.

## Test sets

| Set | Where | Status |
|---|---|---|
| Saved folder (Mixed, 660 MB) | `research_notes/engine-bench.noindex/Mixed` | on disk |
| Rivals' archives | `research_notes/engine-bench.noindex/archives` | on disk |
| website sets | `research_notes/engine-bench.noindex/sets/` | 4 of 5 rebuilt; silesia needs a download (ask) |

Website sets were rebuilt with the exact recipe of the 2026-10-25 session (found in its
transcript):

| Set | Recipe | Bytes now | Bytes on 2026-10-25 |
|---|---|---:|---:|
| apps | `cp -cR` of Calculator, Chess, TextEdit, Preview, Stickies, Notes, Maps, Mail from `/System/Applications` | 173,844,185 | 173,844,185 (identical) |
| source | the 89 crates in `~/.cargo/registry/src/index.crates.io-*` unpacked before 2026-10-25 20:04 | 161,981,441 | 161,981,441 (identical) |
| jpeg | first 400 `*.jpg`/`*.jpeg` over 20 KB under `/System/Library /Library /Applications/Xcode.app` (`find` order) | 98,403,602 | 98,428,070 (−0.02%) |
| docs | first 250 PDFs over 20 KB (same dirs) + first 600 PNGs over 10 KB under `/System/Library /Library` | 90,904,052 | 89,428,138 (+1.7%) |
| silesia | `silesia.zip` from sun.aei.polsl.pl (public corpus) | — | 211,938,580 |
| mixed | all five in one folder (`cp -cR`) | — | 735,620,414 |

jpeg and docs drifted slightly because system files changed since 10-25; old website
numbers for those two sets are not directly comparable to new ones.

## Baseline profiles

Saved folder, CLI (`engine/target/release/arca`), busy Mac (load ~5), 2026-10-28.
Profiles use `ARCA_DEBUG_PROFILE=1` (added in `arca-core/src/prof.rs`: exclusive
thread seconds per stage) and `ARCA_DEBUG_TIMING=1`. Not final numbers.

| Level | Make | Size | Peak RSS | CPU (user) | Extract | Extract CPU |
|---|---:|---:|---:|---:|---:|---:|
| Fast | 1.5–1.9 s | 315.9 MB (301.31 MiB) | 0.96 GB | 4.1 s | 0.93 s | 1.1 + 4.7 sys |
| Default | 10.1–10.6 s | 257.5 MB (245.55 MiB) | 3.6 GB | 64–65 s | 2.6–2.7 s | 15.9 + 5.3 sys |
| Max | 105.4 s | 221.7 MB (211.47 MiB) | **10.3 GB** | 722 s | 9.6 s | 74 + 5.6 sys |

**Fast make** (1.46 s run): scan 0.19 s (one thread), sniff 0.25 s, files+blocks
0.68 s, index 0.33 s. Thread seconds: sniff 2.38 (opens every file; 177 µs each),
zstd-3 blocks 2.6, BLAKE3 1.2 (every small file is hashed twice: whole file, then
as its single chunk), CDC + chunk hashes 0.63, **index zstd-19 0.33 s on one
thread** (1.3 MB raw index → 233 KB). Only ~4 s CPU in 1.5 s: Fast is bound by
serial phases, not by work.

**Default make** (10.6 s): thread seconds: Lepton encode+verify 38.6, zstd-12 binary
blocks 27.9, preflate analysis 21.5, zstd-15 text blocks 11.3 (60 MB at 5 MB/s),
preflate verify 6.8, hashing 4.5. 64 s CPU in 10.6 s: cores ~60% busy.

**Max make** (105 s): files done at 39 s, blocks at 105 s. LZMA-9e over 1.30 GiB of
binary lane (1.0 GiB of it is expanded Deflate, mostly PNG pixels). Max expands 793
Deflate files (132 MB → 1.0 GiB) where Default expands 594 (101 MB → 588 MB): the
extra 199 files are the level-9 PNGs that Default's chain limit (256) rejects.

**Max extract** (9.6 s, 74 s CPU): rebuilding Deflate 64.9 s CPU (Default: 2.9 s),
Lepton 14.6 s, xz decode 10.0 s, writing 4.8 s. Single PNGs take up to 3.0 s each
(`png-417-photoPaperHiDPI-DogEarLeft.png`: 1.16 MB → 16.7 MB of pixels). System
zlib -9 needs 4.4 s to compress that same image, so preflate is already faster than
zlib: the cost is inherent to replaying a level-9 Deflate encoder one byte at a time.
Tool: `cargo run --release --example objtime ARCHIVE` lists the costliest rebuilds.

**Default extract** (2.6 s): Lepton 12.6 s CPU, writing files 4.8 s, Deflate 2.9 s.
**Fast extract** (0.93 s): writing files 5.3 s of thread time (kernel), zstd 0.3 s.

Profiler caveat found on the way: hashing files over 1 MiB uses `update_rayon` inside
the busy pack pool. While it waits on the join, the thread steals unrelated tasks
(e.g. a 3 s PNG), so one file's hash can finish seconds late and stall the in-order
packer. To check as part of the scheduling work.

## Known fixes

1. **Max PNG rebuild cost** (in progress).

   What the data is: at Max, 554 docs PNGs expand from 115 MB to **1,008 MB** of
   filtered pixel rows, two-thirds of all lane data. Other lane data: Mach-O 159 MB,
   source "binary" 104 MB, app binaries 81 MB, source text 58 MB, PDF streams 43 MB.

   Max on the docs part alone (850 files, 138 MiB), varying preflate:

   | Setting | Size | Make | Extract | Rebuild CPU |
   |---|---:|---:|---:|---:|
   | chain 256 (Default's) | 77.22 MiB | 51.6 s | 1.02 s | 2.8 s |
   | chain 4096 (Max today) | 71.37 MiB | 90.2 s | 6.62 s | 59.0 s |
   | chain 4096 capped to 256 (new patch) | 73.65 MiB | 86.1 s | 4.81 s | 40.9 s |
   | chain 4096 capped to 1024 | 72.48 MiB | 86.1 s | 5.70 s | 49.8 s |
   | chain 4096, budget 64 steps/byte | 77.00 MiB | 55.8 s | 1.05 s | 3.5 s |
   | chain 4096, budget 160 steps/byte | 76.44 MiB | 62.4 s | 1.10 s | 4.7 s |

   Rebuild cost is very lopsided: 72 of 793 Deflate files (13 MB) cause 42.7 s of the
   60 s rebuild CPU; 140 files (23 MB) cause 55 s. They are photo-like images saved
   by level-9 encoders: nearly every position starts a short match, so replaying the
   encoder means millions of searches (sampling: all time in preflate's
   `match_token_0/1`). Capping the search depth does not help much (40.9 s), so the
   cost is the number of searches, not their depth.

   A search step costs a steady ~3.1 ns (414 M steps = 1.28 s; 1,066 M = 3.28 s;
   738 M = 2.36 s), and preflate's analysis at pack time performs exactly the same
   searches as the rebuild. So **a step count measured while packing predicts the
   rebuild cost exactly and deterministically**. Patches in the vendored
   `engine/vendor/preflate-rs`: a step counter, a per-byte step budget checked every
   1,024 tokens (a stream over budget is abandoned and stored as is; a rejected PNG
   now costs 0.07 s at pack time instead of 0.43 s), and an optional chain cap.

   But the size gain lives in exactly those costly files: a budget keeps almost none
   of the 5.85 MiB (77.0 vs 77.2 MiB). So the honest trade at Max today is
   about 2.7% of size against ~6x faster docs extraction and ~1.6x faster docs
   packing. To win back the size cheaply, see "PNG token coding" under deep research.

   Whole saved folder at Max (busy Mac; times rough), with a separate image lane
   for PNG pixels (`--lane`):

   | Variant | Size | Make | Extract | Pack CPU |
   |---|---:|---:|---:|---:|
   | Max today | 211.47 MiB | 105 s | 9.6 s | 722 s |
   | A: budget 64 steps/byte | 217.71 MiB | 80 s | 3.6 s | 403 s |
   | B: A + xz-9 for binary and image lanes | 218.67 MiB | 56 s | 3.7 s | 286 s |
   | C: B + xz-9 text, zstd-19 image | 221.17 MiB | 51 s | 3.9 s | 280 s |
   | D: A + zstd-19 image | 220.45 MiB | 51 s | 5.7 s* | 328 s |

   (*D's extraction ran under heavy load.) In A, the image lane (546 MiB raw) took
   275 s of CPU with xz-9e, 167 s with xz-9 and 156–170 s with zstd-19; the binary
   lane (400 MiB) 224 s with xz-9e and 138 s with xz-9.

   Codec comparison on one 64 MiB block per data type (single thread, 4 jobs at
   once), compressed MB / seconds:

   | Block | xz-9e | xz-9 | xz-6 | zstd-19 | zstd-15 | zstd-12 |
   |---|---|---|---|---|---|---|
   | 000 source text | 6.26 / 20.3 | 6.37 / 15.3 | 6.65 / 12.8 | 6.61 / 17.3 | 7.41 / 4.8 | 7.60 / 1.2 |
   | 001 Mach-O | 10.20 / 25.8 | 10.25 / 22.4 | 12.09 / 20.3 | 12.10 / 23.2 | 14.47 / 7.8 | 14.70 / 1.8 |
   | 008 synthetic PNG pixels | 0.68 / 17.4 | 0.77 / 2.6 | 0.80 / 2.5 | 0.77 / 4.1 | 0.84 / 0.7 | 0.86 / 0.4 |
   | 011 photo-like PNG pixels | 14.47 / 45.4 | 14.53 / 39.7 | 14.59 / 25.7 | 15.47 / 27.6 | 16.37 / 10.7 | 16.65 / 2.4 |
   | 013 PNG tail + PDF | 6.48 / 24.0 | 6.67 / 14.1 | 6.78 / 12.6 | 6.98 / 23.8 | 7.94 / 3.8 | 8.14 / 1.1 |
   | 015 other binaries | 20.60 / 20.2 | 20.66 / 15.9 | 20.80 / 12.8 | 21.52 / 16.0 | 22.93 / 4.9 | 23.06 / 1.1 |

   Over these six blocks -9e is 0.95% smaller than -9 for 1.39x the time; zstd-19
   is never worth it; LZMA pays most on Mach-O (30% under zstd-12) and least on
   photo-like pixels (13%), where it is also slowest. Cheaper LZMA match finding
   (`mf=hc4`, `mode=fast`, `nice=32`, `depth=12`) loses 2–18% and is still slow on
   pixels (6–22 s per block). **Conclusion: no xz setting reaches Max < 20 s on
   this folder; that needs a pixel codec and a faster LZMA match finder.**
2. **Max memory on 8 GB Macs** (done). Peak was 8.3–10.3 GB: ten xz-9e encoders
   (~674 MiB each, mostly BT4's tree for a 64 MiB dictionary), 20 queued 64 MiB
   blocks, and a read-ahead window that counted files at their input size while an
   expanded PNG waiting its turn holds ~15x that. Now:
   - the number of block encoders comes from a memory budget (half of
     `hw.memsize`; `ARCA_MEM_BYTES` simulates other Macs), using liblzma's own
     `lzma_easy_encoder_memusage` plus the block and its output per encoder;
   - queued blocks: encoders + 2 (was 2 x threads);
   - read-ahead: min(512 MiB, budget/16), counting expanded bytes too.

   | Simulated Mac | Encoders | Peak RSS | Max make (busy Mac) |
   |---|---:|---:|---:|
   | 8 GB | 3 | 4.8 GB | 124 s |
   | 16 GB | 8 | 8.5 GB | 72 s |

   Archives are byte-identical whatever the memory. The ~0.7 GB above budget is
   page cache of mapped input files (reclaimable) and allocator slack.
3. Max blocks vs cores: (to do)
4. **Fast's duplicate and fingerprint cost** (mostly done). Fast make on the saved
   folder went from 1.46–1.9 s to 0.80–1.0 s (busy Mac, even at load 35), same
   size (301.33 MiB; +0.02 from the index level), round trip byte-identical:
   - Each file is hashed once. Untransformed files are hashed only chunk by chunk;
     the file's identity is the single chunk's BLAKE3, or for several chunks a
     hash of the (length, hash) list under its own derive-key domain, so no file's
     bytes can pose as another file's chunk list. Large files no longer hash with
     `update_rayon` inside the busy pool (it stole unrelated tasks).
   - The index uses zstd-9 at Fast and Default: 0.01 s instead of 0.31 s for
     257.5 KB instead of 238.5 KB.
   - The folder walk runs on the pack pool (0.20 s → 0.07–0.09 s).
   - No separate sniffing pass: files are ordered by extension before anything is
     read, and each task reads, classifies and packs its file. Opening files does
     not scale past ~2 threads on APFS (11,050 open+read+close: 0.26 s on 1
     thread, ~0.12 s on 2–10), so it is better overlapped with compression than
     done as a phase.
   - Ordering by extension costs size at the bigger levels, so Default, Max and
     Ultra still sniff (a 4 KiB head read per file, ~0.1 s) and Fast doesn't:

     | Level | Extension order | Sniffed order | Before this work |
     |---|---:|---:|---:|
     | Fast | 301.33 MiB, 0.67 s | 301.33 MiB, 0.78 s | 301.31 MiB, 1.5–1.9 s |
     | Default | 245.72 MiB | **245.34 MiB** | 245.55 MiB |
     | Max (budget 64) | 219.05 MiB | **217.71 MiB** | — |

     The separate image lane makes Default 0.21 MiB smaller than before.
   - Dead end: `-C target-cpu=apple-m1` (Lepton and preflate within noise).

## Deep research

(to fill: codecs, transforms, dedup, scheduling, writing files, GPU, lzraven)

### Codecs

**Apple's LZRAVEN and LZMESH** (macOS 27 SDK, `compression.h`: `COMPRESSION_LZRAVEN
= 0xD05`, `COMPRESSION_LZMESH = 0xE05`, buffer API only). Apple: LZRAVEN is
"faster than, and generally compresses better than LZMA"; LZMESH replaces zlib and
LZFSE. Per the Apple Wiki it appeared in 26.4b1 as "Mammoth". Tested on the
dumped 64 MiB blocks with `compression_encode_buffer` (single thread; tool in the
scratchpad, `rv.c`), compressed MB / encode s / decode s:

| Block | LZRAVEN | LZMESH | Apple LZMA (level 6) | xz-9e | xz-9 |
|---|---|---|---|---|---|
| 008 synthetic pixels | 0.789 / 2.39 / 0.018 | 1.070 / 0.06 / 0.007 | 0.800 / 2.65 / 0.034 | 0.678 / 17.4 | 0.768 / 2.6 |
| 000 source text | 6.701 / 11.45 / 0.101 | 9.081 / 0.18 / 0.028 | 6.654 / 13.90 / 0.240 | 6.258 / 20.3 | 6.371 / 15.3 |
| 001 Mach-O | 11.655 / 18.13 / 0.211 | — | — | 10.201 / 25.8 | 10.251 / 22.4 |

LZRAVEN sits between xz-6 and xz-9 on ratio (7% over xz-9e on text, 14% on
Mach-O), compresses no faster, and decodes ~2.4x faster than LZMA. It needs macOS
27 (Zipling supports 15+), so an archive using it would not open on older Macs.
**Dead end for Max**; its fast decoding is noted in case Zipling ever requires 27.

**7-Zip's LZMA encoder** (LZMA SDK, public domain), `7zz a -txz -mx9 -mfb=273`
against `xz -9e`, alternating, same blocks: Mach-O 10.159 MB in 15.5 s vs 10.201 MB
in 27.7 s; synthetic pixels 0.698 MB in 13.8 s vs 0.678 MB in 17.4 s. But 7-Zip
used ~1.6 threads even at `-mmt1` (its match finder runs in its own thread):
user time 24.1 s vs 27.7 s, so **~13% less CPU and slightly smaller**, and a
two-thread match finder for when there are fewer blocks than cores.
Worth vendoring (needs the 7-Zip source download).

**Where Max's CPU goes** (thread CPU time, `prof.rs` now uses
`CLOCK_THREAD_CPUTIME_ID`): Max 429 s user: pixel blocks 212 s (600 MB, xz-9e),
binary blocks 145 s (415 MB), text blocks 23 s, Lepton 23 s, preflate 23 + 4 s.
**Blocks are 89% of Max.** Under 20 s needs ~4x less block CPU. Default 67 s
user: Lepton 24 s, preflate 12 + 3 s, pixels 11 s, binaries 9 s, text 7 s.

**Fast LZMA2** (Conor McCarthy, BSD-3/GPL-2; parallel radix match finder, standard
LZMA2): claims 20–100% faster than 7-Zip's LZMA2 for 1–5% larger output. Useful
to spread one block over several cores, but not the ~4x Max needs, and the size
loss breaks the brief. Kept as a fallback for blocks-vs-cores.

### A pixel codec for PNG data (most promising lead)

Per-PNG study of the saved folder (644 PNGs, 1,106 MB of pixels; standalone xz-9 per
image): photo-like images (under 6:1) are 56% of the compressed pixel bytes but
29% of the CPU; flat UI graphics (over 30:1) are 6.5% of the output and 13% of the
CPU. 75% of the whole gain from expanding PNGs costs 21% of the LZMA CPU.

Prototype (`examples/pixbench.rs`): undo the PNG row filters, predict each pixel
with MED (LOCO-I), code green's residual, then red and blue as differences from
green's, then alpha, with an LZMA-style adaptive binary range coder (12-bit
probabilities; contexts: 12 activity buckets x 3 previous-error classes; residual
as zero flag, sign, Elias-gamma exponent, mantissa). No run mode, no tuning.
On the 559 8-bit non-interlaced docs PNGs (1,042 MB of pixels):

| | Size | CPU |
|---|---:|---:|
| xz-9 on the filtered rows, per image | 70.44 MB | 167.8 s |
| pixel codec v0, per image | 70.62 MB | 22.3 s encode, 21.7 s decode |
| better of the two, per image | **61.77 MB** | |

By type (xz ratio): under 3:1 ours is 0.894 of xz; 3–6: 0.854; 6–12: 0.940;
12–30: 1.185; over 30: 1.753 (flat areas need a run mode). Decoding runs at ~48 MB/s
per core, slower than LZMA's ~150 MB/s: a cost on extraction to keep in view.

Iterations (docs PNGs, 1,042 MB of pixels; xz-9 per image = 70.44 MB):

| Version | Change | Size | Best of codec/xz per image | Encode / decode CPU |
|---|---|---:|---:|---:|
| v0 | MED, per-channel zero flag | 70.62 MB | 61.77 MB | 22 s / 22 s |
| v1 | whole-pixel zero flag; green's residual as context | 64.32 MB | 60.49 MB | 18 s / 19 s |
| v2 | 16-bit probabilities, fast+slow rates averaged | 63.80 MB | 60.00 MB | 19 s / 21 s |
| v3 | blend of 6 predictors weighted by neighbours' errors | 63.86 MB | 59.10 MB | 42 s / 63 s |

v3 is better on photos (0.80 of xz) and worse on flat images, which go to LZMA
anyway. **On the 439 PNGs Max actually expands** (the budget skips the costliest
photo-like ones): xz-9 49.23 MB; MED codec 43.80 MB; blend 43.20 MB; better of
blend/xz per image **39.75 MB (−19%)**, which would take ~9 MB (~4%) off Max and
most of the pixel lane's 212 s of CPU.

The catch is decoding: 13–30 MB/s per core (about ten adaptive binary decisions
per byte) would add ~10 s of CPU to Max extraction, losing to Zipgeist there. The
production design needs fast decoding: integer-only prediction, static per-stripe
statistics, table-driven interleaved rANS for a combined zero/sign/exponent token
with raw mantissa bits, aiming at 200+ MB/s.

**v4, the fast design** (same PNGs Max expands, 409 eligible, 573 MB of pixels;
times with nothing else running except other sessions):

| Step | Size | Encode / decode CPU |
|---|---:|---:|
| v2 (adaptive binary, MED) for reference | 43.80 MB | 14.5 s / 24.9 s |
| v4: 16-token adaptive tables (LZNA-style) + rANS, raw remainder bits | 46.32 MB | 11.0 s / 16.3 s |
| zero-padded image, LUT activity, zero-flag row instead of `memcmp` | 46.37 MB | 7.5 s / 7.3 s |
| top remainder bit modelled | 45.69 MB | 7.9 s / 7.5 s |
| table adaptation rate 6 (was 4) | 44.38 MB | 8.1 s / 7.5 s |
| warm-up (fast at first, settling to rate 7) | **44.03 MB** | 8.4 s / 7.8 s |

So v4 is within 0.5% of v2 at ~3x the decoding speed (~75 MB/s per core). For
comparison LZMA decodes these pixels at ~133 MB/s in the real Max archive, so the
extra extraction CPU is ~2 s (~0.2 s wall) for the images the codec takes.

**Choosing per image without running xz:** pixel codec when its output is under
0.9x the zstd-1 size of the filtered rows, else the rows go to the LZMA image lane.
That gives 41.30 MB against 40.95 MB for a perfect choice and 49.23 MB for xz-9
alone (−16%), with the codec taking 484 of the 573 MB of pixels.

**In the engine** (`transform/pixels.rs`, Deflate layout version 2): a PNG whose
pixels the codec takes stores layout, corrections, literal bytes and row filter
types (binary lane) followed by the coded pixels (store lane); extraction decodes
the pixels, re-applies the row filters and re-creates the Deflate stream. Unit
tests cover round trips, filters and malformed input. On at Max and Ultra.

Whole saved folder, Max (Mac at load ~24 when it started, so times are rough):

| | Size | Make | Extract | Pack CPU |
|---|---:|---:|---:|---:|
| Max at the start of 2026-10-28 | 211.47 MiB | 105 s | 9.6 s | 722 s |
| + rebuild budget | 217.17 MiB | 73 s | 3.58 s | 429 s |
| **+ pixel codec** | **210.84 MiB** | **38.3 s** | 3.99 s | 259 s |

Round trip byte-identical. The codec took 409 PNGs (573 MB of pixels); the image
lane shrank to 117 MiB of flat images (4.73 MiB compressed, 26 s of LZMA instead
of 212 s). Remaining pack CPU: binary lane 145 s (xz-9e), preflate 24 + 12 s
(verification now includes decoding pixels), Lepton 24 s, text 22 s, pixels 8 s.
Extraction: pixel decoding adds ~7 s of CPU and LZMA decoding drops 3.4 s.

(Earlier plan, kept for the record: build it into the engine as a PNG mode of the Deflate transform (pixels coded
in the file task, routed to the store lane; layout, corrections and row filter
types to the binary lane), at Max and Ultra first. Default would need faster
decoding first (it would add ~0.7 s to Default's 2.6 s extraction). (pixels + row filter
types + preflate corrections), choosing per image between the pixel codec and the
LZMA image lane, measured in real archives (solid blocks find cross-image matches
that standalone numbers miss).

Ideas queued, with why:

- **PNG token coding.** For costly PNGs, instead of replaying the encoder, store the
  Deflate token stream (literal / match) coded with a model that sees the image
  rebuilt so far, and rebuild by Huffman-coding the tokens again. Rebuild cost
  becomes entropy decoding (no searches). Could recover the 5.85 MiB without the
  59 s of rebuild CPU.
- **Parallel rebuild of one Deflate stream.** Preflate's only state across Deflate
  blocks is the hash chain (pending lazy match resets per block; the CABAC coder
  is already fresh per chunk). Priming a predictor with the 32 KB before a block
  lets segments of one stream rebuild on all cores. Removes the single-PNG critical
  path (3 s for one 16.7 MB image). Pack-time verification guarantees exactness.
- **A pixel codec for PNG data** instead of LZMA on filtered rows (1 GB of lane data
  at Max; LZMA-9e there is most of Max's pack time).
- **App extraction overhead.** The app measured Fast extraction at 2.21 s against
  0.93 s for the CLI. After the engine finishes, `SafeExtraction.finalize` walks
  every extracted item and sets the quarantine attribute by path (~14,500 calls on
  the saved folder). The engine could set it through the descriptor it already
  holds. Needs an optional field in the extract request (FFI stays 9 functions).

## Engine ZIP reader and writer

(to do)

## Dead ends

(none yet)
