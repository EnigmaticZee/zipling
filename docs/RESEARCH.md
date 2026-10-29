# Arca engine research log

Started 2026-10-28 on branch `zee/arca-research-77726f`. Backup: tag
`before-engine-research` (commit `cc46547`).

**Resuming?** Read this file top to bottom first. "State" says where the work is;
everything below it is the record.

## State

Saved folder, CLI, busy Mac (load 9-12), 2026-10-28 late evening; rivals from the
2026-10-28 head-to-head:

| Level | Make | Size | Extract | Rival (make / size / extract) |
|---|---:|---:|---:|---|
| Fast | 0.7-1.2 s | 301.3 MiB (316 MB) | 0.74-0.88 s | Zipgeist tar.zst 1.24 s / 341 MB / 1.6-1.9 s |
| Default (preset) | 9.4-9.7 s | 245.47 MiB (257 MB) | 2.2-2.3 s | Zipgeist 7z 17.2 s / 285 MB / 3.0 s |
| Default, all options* | **6.8-7.4 s** | **233.66 MiB (245 MB)** | 2.7-2.9 s | |
| Max (preset) | 32.8-33.6 s | 211.69 MiB (222 MB) | 3.0 s | Zipgeist best 20.0 s / 284 MB / 3.7 s |
| Max + bsc** | **20.1-20.7 s** | **211.58 MiB** | 3.35 s | |
| Ultra + bsc** | (mixed set: 32.0 s vs 40.9) | smaller | 2-3x faster than PPMd | |
| ZIP extract | 0.91-0.99 s | | | Zipgeist 2.1 s |

\* `ARCA_EXP_PIXELS=1 --lane text=bsc:1 --text-block-mib 32 --image-block-mib 32`
\*\* `--lane text=bsc:1 --lane code=bsc:2e`

The options are in the engine but not in the presets (user, 2026-10-28: decide
at the final quiet-Mac benchmarks). Each trades a little extraction time on
text-heavy or photo-heavy archives for much faster making and smaller archives.

Done today: pixel codec 2x faster to decode, extraction writes (creation gate,
write queue, one-call metadata), Lepton without LLVM's memset idiom, engine-side
quarantine, bsc (BWT) codec with threaded decoding, level-9 PNG skip at Default,
image-lane block size. Dead ends: long-range pre-pass, hint-guided LZMA, LZMA
match finders, parallel LZMA2 segments, band-parallel pixel decoding, multi-
symbol Lepton size classes, ARM64 byte planes, GPU, LZRAVEN.

Next: ZIP writer in the app; final quiet-Mac benchmarks of the presets against
the options (ask the user first).

## Deep research agenda (from the user, 2026-10-28 evening)

The goal is a breakthrough, not tuning: beat every archiver (7-Zip, Zipgeist,
Keka, BetterZip, WinRAR, Finder, zstd, xz) at every level on speed and size,
aiming for multiples. Keep what works (pixel codec, ZIP reader and writer, LZMA
SDK, memory fix); replace a part only if a test shows the new one is faster and
at least as small. Steps before the final benchmarks:

1. Default several times faster: Lepton, preflate and zstd are its time; e.g.
   the pixel codec with fast decoding.
2. Max under 20 s; Ultra extracting on all cores.
3. PNG token coding; re-creating one Deflate stream on several cores.
4. Apple Silicon GPU: can it take any stage? It must help ordinary Macs, not
   only big-GPU ones.
5. Duplicate detection, scheduling, writing files, and the app's extraction
   overhead (2.2 s in the app vs 0.9 s in the engine).
6. The newest papers and techniques: codecs and transforms nobody else ships.

Status of each is kept under its own heading below as it progresses.

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

## All website sets, before and after (2026-10-28)

Every rebuilt set at every level, this morning's engine (tag
`before-engine-research`, built in a separate worktree) against commit e891199.
Sizes exact; times rough (the baseline ran while other tests ran). All 48 round
trips byte-identical. Raw: `bench/research/2026-10-28-sets-*.tsv`, script
`bench/research/sets.sh`.

| Set | Level | Size before | Size now | Δ | Make before → now |
|---|---|---:|---:|---:|---|
| docs | fast | 41.50 MB | 41.52 MB | +0.06% | 0.29 → 0.24 s |
| docs | default | 22.21 | 22.17 | −0.17% | 2.28 → 2.06 |
| docs | max | 18.00 | 18.20 | +1.12% | 21.4 → 11.4 |
| docs | ultra | 18.00 | 17.82 | −1.03% | 22.1 → 16.2 |
| jpeg | all | 71.09–95.10 | +0.00–0.01% | | 1.2–1.7x faster |
| apps | fast | 72.40 | 72.46 | +0.09% | 0.51 → 0.38 |
| apps | default | 62.43 | 62.39 | −0.08% | 2.74 → 2.25 |
| apps | max | 49.84 | 49.17 | −1.34% | 26.5 → 18.0 |
| apps | ultra | 49.81 | 48.53 | −2.56% | 26.9 → 17.3 |
| source | fast | 13.55 | 13.57 | +0.18% | 0.57 → 0.42 |
| source | default | 11.22 | 11.23 | +0.11% | 2.31 → 2.15 |
| source | max | 8.29 | 9.28 | **+11.94%** | 27.6 → 6.5 |
| source | ultra | 8.29 | 8.26 | −0.36% | 27.5 → 23.2 |
| silesia | fast | 65.74 | 65.67 | −0.10% | 0.31 → 0.25 |
| silesia | default | 57.27 | 57.27 | 0.00% | 5.89 → 5.54 |
| silesia | max | 48.65 | 49.31 | +1.37% | 36.0 → 14.8 |
| silesia | ultra | 47.00 | 46.90 | −0.21% | 27.4 → 17.9 |
| mixed | fast | 288.27 | 288.37 | +0.04% | 1.45 → 1.04 |
| mixed | default | 223.39 | 223.25 | −0.06% | 10.9 → 10.4 |
| mixed | max | 195.48 | 196.76 | +0.66% | 47.2 → 31.9 |
| mixed | ultra | 193.63 | 192.26 | −0.71% | 37.9 → 37.1 |

Findings: the fast Max (the user's choice, made on the saved folder's +0.65%)
costs 12% on source code; Fast and Default have small regressions (≤0.18%) from
the new file order; Default is barely faster (its CPU is Lepton, preflate and
zstd, untouched so far); Ultra is smaller everywhere but still extracts slowly
(PPMd: 4.2 s source, 7.2 s Silesia, 8.0 s mixed).

### Max settings across every set

Three Max settings (A = the fast preset the user picked: text fb 64 in 32 MiB
blocks, binaries fb 32; B = text fb 273 in 64 MiB blocks, binaries and pixels
fb 64; C = fb 273 everywhere), size against this morning's Max:

| Set | A | B | C |
|---|---|---|---|
| docs | +1.1%, 1.9x faster | −0.4%, 1.6x | −1.0%, 1.3x |
| apps | −1.3%, 1.5x | −2.3%, 1.7x | −2.6%, 1.5x |
| source | **+11.9%**, 4.2x | +6.1%, 1.9x | −0.4%, 1.2x |
| silesia | +1.4%, 2.4x | −0.04%, 1.3x | −0.2%, 1.1x |
| mixed (website) | +0.7%, 1.5x | −0.3%, 1.3x | −0.7%, 1.0x |
| saved folder | 212.61 MiB, 21.8 s | 211.17 MiB, 25.5 s | 210.32 MiB, 34.4 s |

(This morning: saved folder 211.47 MiB in 105 s.) Source code is where long
matches matter most: its ~2,800 static libraries repeat whole object files.
Raw: `bench/research/2026-10-28-max-candidates.tsv`.

**Index format v2**: entries column by column, paths front-coded against the
previous path, times and object numbers delta-coded (version 1 archives still
open). Saved folder at Fast: index 251.37 KiB (v1, zstd-9) / 232.61 KiB (v1,
zstd-19) → **206.75 KiB** (v2, zstd-9, 5 ms); raw index 1.3 MB → 0.5 MB. Fast on
the source set is now within 858 bytes of this morning (was +24.8 KB).

### Max: a code lane (the user's direction, 2026-10-28)

User: keep the fastest Max, but give code and text long matches so Max beats 7-Zip
Ultra on source (under 8.5 MB) and Silesia (under 48.7 MB), with a small time cost.
New `Class::Code` (by content: ELF, PE, static libraries `!<arch>`, Java class,
WebAssembly, LLVM bitcode, tar) and a code lane with its own codec and block size
(Max and Ultra only; Fast and Default keep it in the binary lane, unchanged).

| Max variant | source | Silesia | saved folder |
|---|---|---|---|
| fast preset (text fb 64 / 32 MiB, binary fb 32) | 9.28 MB | 49.31 MB | 212.61 MiB, 21.8 s |
| + code lane fb 273 incl. Mach-O | 8.53 MB | 49.02 MB | 212.55 MiB, 37.4 s |
| + code and text fb 273, 64 MiB, Mach-O in code | 8.23 MB | 48.63 MB | 212.26 MiB, 41.9 s |
| **code and text fb 273, 64 MiB, Mach-O stays binary** | **8.23 MB** | **48.63 MB** | **211.69 MiB, 33.4 s** |
| same, text in 32 MiB blocks | 8.42 MB | 48.74 MB | 211.88 MiB, 33.3 s |
| code fb 64 | 8.77 MB | 48.71 MB | 212.20 MiB, 24.1 s |
| code fb 273 in 32 MiB blocks | 8.46 MB | 48.76 MB | 211.91 MiB, 32.4 s |

Mach-O gained almost nothing from long matches on real apps (−0.06 MiB for +15 s),
so it stays in the binary lane. The code lane on the saved folder is ~99 MiB of
static libraries that compress 46:1; at fb 273 one 64 MiB block of it takes ~31 s,
which is the +11 s. **Chosen:** text and code fb 273 in 64 MiB blocks (both targets
met; saved folder 211.69 MiB, smaller than this morning's 211.47 MiB-in-105 s Max
0.1%, in ~33 s instead of 105 s). Next: a long-range pre-pass to take that repetition
off LZMA (see deep research).

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

### The LZMA SDK as Max's codec (done)

With the user's OK, 7-Zip 26.05's public-domain LZMA SDK files are vendored in
`engine/vendor/lzma-sdk` (checksum-verified pinned release; every file says
"Public domain"; see its ARCA-NOTES.md). New block codec `lzma2` (tag 4): raw
LZMA2 behind its property byte, no xz container or CRC-64 (blocks carry BLAKE3).
Max and Ultra use it (fb 273 = xz -9e's long matches; match finder in its own
thread). Decoding uses 7-Zip's ARM64 assembly loop. Measured on the Mach-O block
at fb 32, nothing else running: one thread 21.2 s wall / 21.2 s CPU, two threads
12.4 s wall / **17.7 s CPU**, output 10,529,319 vs 10,535,673 bytes (+0.06%; the
same on every run, so archives stay deterministic).

Whole saved folder, Max, busy Mac:
- size 210.84 → **210.60 MiB**; **LZMA decoding 4.76 → 3.18 s of CPU**; extraction
  3.99 → 3.83 s;
- no gain in make time (40 s): with ten blocks compressing at once there is no
  spare core for the second match-finder thread. Pack CPU 259 → 270 s.

**File order.** Most binary blocks were created only at 11.3 s, after the packer
had worked through every slow PNG/PDF. Orders tried (Max):

| Order within the binary group | Size | Make |
|---|---:|---:|
| Mach-O, ZIP, PNG, PDF, gzip, other binaries (before) | 210.60 MiB | 40.4 s |
| Mach-O, other binaries, gzip, ZIP, PDF, PNG | 211.66 MiB | 35.8 s |
| **Mach-O, ZIP, PDF, gzip, other binaries, PNG** | **210.35 MiB** | 39.6 s |

Kept the last (PNGs last: their pixels mostly go to the store lane now). Default
245.34 → 245.43 MiB with it (+0.04%), Fast unchanged.

**LZMA effort against CPU** (SDK, one 64 MiB block each, one thread, CPU s):

| Setting | Text | Mach-O | PDF + PNG tail | Other binaries |
|---|---|---|---|---|
| fb 273 | 6.249 MB / 28.1 | 10.155 / 33.6 | 6.407 / 33.8 | 20.561 / 25.6 |
| fb 128 | 6.285 / 23.6 | 10.170 / 31.3 | 6.523 / 24.8 | 20.593 / 22.1 |
| fb 64 | 6.366 / 20.0 | 10.233 / 28.8 | 6.641 / 19.3 | 20.631 / 19.3 |
| fb 48 | 6.431 / 18.2 | 10.303 / 27.8 | 6.680 / 17.4 | 20.653 / 18.2 |
| fb 32 | 6.593 / 15.6 | 10.529 / 23.1 | 6.769 / 14.7 | 20.734 / 16.2 |

Max is now CPU-bound (275 s on 10 cores), so make time follows LZMA effort.
Whole folder (Mac at load ~20):

| Max variant | Size | Make | CPU |
|---|---:|---:|---:|
| start of 2026-10-28 | 211.47 MiB | 105 s | 722 s |
| fb 273 (smallest) | 210.35 MiB | ~40 s | 275 s |
| fb 64, 32 MiB text blocks | 211.50 MiB | 26.8 s | 194 s |
| fb 32, 32 MiB text blocks | 212.85 MiB | 23.2 s | 168 s |

Under 20 s on a quiet Mac likely needs fb 32-ish and cheaper preflate/Lepton
work (34 s and 22 s of CPU), or a new idea for LZMA itself.

**The user chose "Max = fastest"** (2026-10-28): Max is now LZMA2 fb 32 with
32 MiB text blocks: **212.85 MiB in 23.3 s** on the saved folder (CPU 168 s),
extraction 3.67–3.85 s, round trip byte-identical. Ultra keeps fb 273 + PPMd text.

Further Max work:
- preflate checks the step budget proportionally (four times the share so far):
  PNG analysis 21.9 → 18.9 s CPU on the docs part, same size. Twice the share was
  21.9 → 15.5 s but rejected one more PNG (+0.10 MiB); exactly the share,
  14.1 s and +0.29 MiB.
- Dead end: running only the leading hash candidate after 2 MB of plain text
  (estimation was 25% of preflate's samples): 2% faster, 2 PNGs lost.
- Dead end: thread QoS. Block threads at user-interactive alone: no change
  (23.5 vs 23.6 s). Block threads high and file tasks at utility: 21.5 vs 23.1 s
  once, then 33–38 s vs 23 s in three alternating rounds (macOS moves utility
  threads to the efficiency cores under load). Too unpredictable.
- Max's floor: 168 s of CPU over 8 performance + 2 efficiency cores is ~19 s at
  perfect packing. The app-code blocks start first but are starved by 20+
  runnable threads and finish last, leaving cores idle for ~5 s.
- Extraction timeline (`ARCA_DEBUG_TIMING`, busy cores per quarter second): all
  10 busy from 0.25 s to 3.25 s, so extraction is CPU-bound (Lepton 11.4 s,
  Deflate + pixels 10 s, LZMA2 3.2 s, writing 2.3 s).

### Ultra, and Max on Silesia

Ultra (PPMd order 16 for text, LZMA2 fb 273, pixel codec) on the saved folder:
210.36 MiB in 34 s, extracting in 5.05 s; one 60 MB PPMd text block takes 4.1 s to
decode on one core. Here PPMd gains nothing over LZMA2 fb 273 (both 5.96 MiB of
text); on Silesia, prose, Ultra is 44.73 MiB against Max's 47.53. 16 MiB text
blocks: 210.61 MiB, extraction 4.26 s (Silesia +0.9%). Ultra stays "smallest".

The fast Max loses size on generic data: Silesia 47.53 MiB (49.8 MB) against 7-Zip
Ultra's 48.7 MB (website, 2026-10-26). The text lane is small and cheap, so Max
now gives it 64-byte matches: saved folder 212.85 → 212.65 MiB, Silesia 47.53 →
47.03 MiB, no time cost. 273-byte text matches: 212.55 / 46.76 MiB but +2 s on
the saved folder.

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

**Max's critical path** (block timeline, `ARCA_DEBUG_TIMING=1` now prints each
block's queue, start and end): files are done at 11.9 s, but each 64 MiB xz-9e
block takes 17–32 s with ten running, the one text block 27.5 s, and most binary
blocks were only created at 9.2 s because the in-order packer waited on slow
PNG/PDF files ahead of them. Smaller blocks (busy Mac, so sizes only):

| Blocks | Size | Text lane | Binary lane |
|---|---:|---:|---:|
| 64 MiB text and binary (Max) | 210.84 MiB | 5.97 | 92.34 |
| text 32 MiB | 211.03 MiB | 6.16 | 92.34 |
| text 16 MiB | 211.16 MiB | 6.29 | 92.34 |
| binary 32 MiB, text 32 | 212.82 MiB | 6.16 | 94.12 |

32 MiB binary blocks cost 2% of the binary lane: not the lever. Spreading one
block over two threads with identical output (7-Zip's threaded match finder)
is. The pixel codec's 16-entry table search and update now use NEON.

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

### Default: where its time goes now (2026-10-28 evening)

Saved folder, Default, busy Mac: 245.48 MiB in 9.4–9.6 s, 65.6 s of CPU. Thread
seconds: Lepton encode + verify 23.0, preflate on PNGs 10.7 + verify 2.3, PDFs
1.5, blocks ~24 (zstd-15 text 58 MB, zstd-12 binary 358 MB and image 536 MB).
Every file is transformed at 7.9 s; the last image block ends at 9.4 s.

**Lepton** (`examples/leptbench.rs`, one thread per file): encode 9–10 MB/s,
decode 9–10 MB/s per core, so `encode_lepton_verify` doubles the work; that is the
price of proving every JPEG comes back exactly. ~10% of its samples are
`memset_pattern16` inside `adv_predict_dc_pix` (vector constants from the `wide`
crate); its SIMD is NEON on ARM. The calls are LLVM's loop idiom: every vector
constant in `wide` is an array fill, which LLVM turns into `memset_pattern16`
on Apple targets (19 calls of 32 bytes per block in the DC predictor alone;
patching `wide` to build `i32x8` from halves only made it 30 calls of 16 bytes,
4% faster). **Building with `-C llvm-args=-disable-loop-idiom-memset`**
(`engine/.cargo/config.toml`, so the app's build too): Lepton encode 9.8 →
8.6 s, decode 10.4 → 9.1 s on the photos (−12%), byte-identical output; whole
saved folder, three alternating runs: Default extraction 2.41–2.51 → 2.28–2.44 s,
Max 3.08–3.23 → 3.00–3.09 s, Fast unchanged, packing CPU 1.5–2 s lower, sizes
identical.

**PNGs made by stock zlib** (`scratchpad zreplay.c`, every PNG of the saved
folder): 480 of 644 PNGs (81.9 of 122.9 MB of Deflate) are byte for byte what
macOS's zlib makes: level 1 56.1 MB (ImageIO's default), level 9 18.2 MB, level 6
5.7 MB. On those, preflate at Default takes 3.9 s forward + 1.35 s verify (level
1) where inflate + one zlib re-run takes 1.8 s and is its own check; re-creation
costs the same either way (zlib -1 1.53 s, preflate 1.35 s). Level 9 is 6 MB/s in
zlib, slower than preflate. Default also spends 2.7 s rejecting 92 level-9 PNGs
its chain limit refuses (the zlib header's FLEVEL = 3 says so up front). A replay
path needs a zlib whose output never changes: macOS's libz could change with an
OS update, so it would need a pinned copy (zlib source is not on this Mac; a
download). Worth ~7 s of Default's CPU (~0.8 s).

**Level-9 PNGs skipped at Default.** Default's search limit (256) makes preflate
reject almost every PNG from zlib levels 7-9 after a full analysis. The zlib
header's FLEVEL (3 for levels 7-9) now skips them up front when the limit is
under 1024. Saved folder, three alternating runs: PNG analysis 11.0 → 6.3 s of
CPU, pack CPU 63.2 → 57.9 s, size 245.48 → 245.47 MiB; wall 9.1-9.2 → 8.8-9.2 s.

**Default's tail is two image blocks.** The block timeline shows every file
transformed at 6.5 s, then two 64 MiB image blocks (zstd-12 on flat PNG rows,
where its binary-tree search slows down) finishing at 7.7 and 9.0 s. Options
(saved folder, three runs each; image lane / total / make):

| Image lane | Image lane | Total | Make |
|---|---:|---:|---:|
| zstd-12, 64 MiB blocks (Default) | 46.69 MiB | 245.47 MiB | 9.1-9.5 s |
| zstd-12, **32 MiB blocks** | 46.80 | 245.59 (+0.05%) | **7.8-7.9 s** |
| zstd-12, 16 MiB blocks | 46.86 | 245.65 (+0.07%) | 7.4-7.7 s |
| zstd-12, 8 MiB blocks | 46.99 | 245.78 (+0.13%) | 7.2-7.8 s |
| zstd-11 | 46.73 | | 8.8 s |
| zstd-9 | 47.44 | | 7.0 s |
| LZMA2 fb 32 | 41.04 | | 43 s |

Smaller blocks for every lane cost the binary lane +1.35 MiB at 32 MiB, so the
image lane got its own block size (`image_block_size`, `--image-block-mib`).
Not switched on (it trades 0.05% of size for 16% of time): for the final
decision, with the pixel codec at Default, which removes most of this lane.

**The pixel codec at Default** (`ARCA_EXP_PIXELS=1`): 245.48 → **233.57 MiB**
(−4.9%) and make 9.4 → 7.8–8.2 s (the codec replaces zstd-12 on 536 MB of
pixels), but extraction 2.74 → 3.0–3.3 s, behind Zipgeist's 3.0 s. Extraction is
CPU-bound from 0.25 s to the end (Lepton 11.8 s, Deflate + pixels 8.6 s).

**Faster pixel decoding, same format.** Counting instructions (`/usr/bin/time -l`)
showed the decoder executing ~100 instructions per byte at 4.7 per cycle: bound by
instruction count, not by dependency chains or branch misses. Hence:

| Try | Decode (docs PNGs, 1 GB of pixels) | Size |
|---|---|---|
| engine before | 110 MB/s | 64.659 MB |
| two or four bands decoded side by side, one core | 108 / 107 MB/s | +0.36 / +0.93% |
| branchless rANS refill | 112 MB/s | same |
| branchless top bit (decode always, keep when needed) | 95 MB/s | same |
| **one decoder per channel count, flat pixels skip the context, table-driven update and tokens** | **171–182 MB/s** | **same bytes** |

The encoder got the same treatment (103 → 119 MB/s) and writes byte-identical
output on all 559 PNGs, so archives are unchanged. PNG row filters now run one
loop per filter type: refilter 595 MB/s → 10.8 GB/s, unfilter 634 → 842 MB/s.
Flat graphics decode at ~300 MB/s, photo-like images at ~135 MB/s (82
instructions per byte; going further needs a format change, e.g. static tables).

**Photos were the slow case.** Pixel coding at Default only where it saves the
most (a byte per N pixel bytes, against 0.9x zstd-1 ≈ zstd-12) cut extraction
CPU far less than the pixel count: the images it keeps are photo-like, and they
decoded at 60–67 MB/s (154 instructions per byte, IPC 3.3; bands side by side
did not help there either). The compiled loop kept both rANS states in memory
indexed by symbol parity, did not unroll the channels, bounds-checked every table
index and branched on MED's comparisons and on each binary symbol's bit. Now:
the states swap in registers, a macro writes the channels out with constant
indexes, MED is a clamp, binary symbols and the refill use selects, and the
token table has a sentinel entry. Same bytes in and out:

| | Photo-like PNGs (175) | Flat (32) | All 604 PNGs |
|---|---:|---:|---:|
| decode, start of the evening | 67 MB/s | 145 MB/s | 110 MB/s |
| **decode now** | **101 MB/s** | **353 MB/s** | **225 MB/s** |
| encode, start → now | 58 → 75 MB/s | 134 → 149 | 103 → 117 |

Default with the pixel codec, saved folder (busy Mac, three runs each):

| Pixel codec at Default | Size | Extract | Extract CPU (user) |
|---|---:|---|---:|
| off (Default today) | 245.48 MiB | 2.45–2.68 s | 14.5 s |
| where it saves a byte per 50 pixel bytes | 234.63 MiB | 2.48–2.54 s | 16.2 s |
| where it saves a byte per 30 | 235.19 MiB | 2.57–2.65 s | 15.8 s |
| wherever it wins | 233.57 MiB | 2.55–2.63 s | 16.6 s |

Make time drops by 0.5–1.3 s with it on (the codec replaces zstd-12 on the
pixels). Not switched on: extraction gets ~0.05–0.15 s slower, so it is the
user's call (`ARCA_EXP_PIXELS=1`, `ARCA_EXP_PIXEL_GAIN=N` to try it). Max
extraction CPU drops 1 s with the faster decoder.

## Writing files (extraction)

Fast extraction of the saved folder was 1.0 s with 1.1 s of user CPU and **5.0 s
of kernel CPU**. `examples/writebench.rs` recreates the extracted tree from memory:
creating 11,050 empty files costs 3.3 s of kernel time on 10 threads; writing their
661 MB adds nothing measurable. APFS serialises file creation:

| Threads creating | Wall | Kernel CPU |
|---|---:|---:|
| 1 | 0.95 s | 0.90 s |
| 2 | 0.63 s | 1.15 s |
| 4 | 0.55 s | 1.72 s |
| 10 | 0.50–0.53 s | 3.1 s |
| 10, at most 4 inside `open` | 0.43 s | 2.15 s |

Gating `close` does nothing; `openat` per directory does nothing. Changes:
- **at most 4 threads create files at once** (`create_file`, used by the
  extractor and the ZIP reader, and clones);
- **duplicates under 64 KiB are written again** while the content is in hand
  (1,002 duplicate files cost 0.87 s of kernel time as clones in a 0.14 s tail
  phase); larger ones are still cloned (they share space);
- **mode and times in one call** (`fsetattrlist` / `setattrlist`) instead of a
  chmod and a utimes, for files and directories;
- directories that files were written into are not `mkdir`ed again.

Extracted trees are identical to before (14,505 items: contents, mode, mtime to
the nanosecond, flags, xattrs). Busy Mac, three runs each:

| Extraction | Before | After | Kernel CPU |
|---|---:|---:|---:|
| Fast | 0.97–1.02 s | **0.74–0.96 s** | 5.0 → 3.4 s |
| Default | 2.72–2.80 s | **2.57–2.63 s** | 5.2 → 3.6 s |
| Default + pixel codec | 2.98–3.04 s | 2.80–2.97 s | 4.8 → 3.6 s |
| ZIP (`ditto-mixed.zip`) | 1.05–1.39 s | **0.91–0.99 s** | 3.6–4.0 → 3.0–3.3 s |

**Writes alongside the rest.** The extraction timeline then showed every plain
write at the end (Max: 2.75–3.5 s), because the scheduler ran the costliest work
first and writes are cheap; at the end only four threads could create files and
the other cores waited. Writes now have their own queue: up to four run at any
time, the other threads decode and rebuild. Saved folder, three alternating runs:

| Extraction | Before | After |
|---|---:|---:|
| Fast | 0.80–0.86 s | 0.74–0.88 s |
| Default | 2.38–2.50 s | **2.19–2.28 s** |
| Max | 3.05–3.20 s | **2.92–3.02 s** |

**The app's own overhead**, measured with a temporary test in the app's test
target (optimised build, saved folder, not committed): Fast extraction through
`Automation.extract` takes 0.99–1.15 s, of which the engine is 0.87 s, opening
the archive (its listing) 0.09 s and the safety walk 0.10 s. The 2.21 s measured
on 2026-10-26 is gone with the engine's own speed-ups. For a downloaded archive,
setting the quarantine on all 14,505 items by path added 0.15–0.4 s; the engine
now sets it through each file's descriptor as it writes (`ExtractOptions::
quarantine`, also in the FFI requests for `.zlg` and ZIP), and the app's pass
runs again only if it merged AppleDouble files. All 58 app tests pass; a new
engine test checks every kind of item gets the quarantine.

Kernel CPU 3.6 → 2.9 s at Default and Max. Max's extraction is CPU-bound from
0.25 s to the end with no single-file tail (Lepton 10.4 s, Deflate 4.4 s, LZMA2
3.3 s, pixels 2.6 s), so re-creating one Deflate stream on several cores would
not shorten it on this folder.

### LZMA on the code lane: match finders and hint-guided parsing (dead ends)

The code lane's first block (67.1 MB of static libraries, 46:1) at fb 273 on one
thread spends 42% of its samples in `Bt4_MatchFinder_GetMatches`, 26% in
`Bt4_MatchFinder_Skip` and 31% in the encoder (optimal parse). Match finders
(`examples/lzone.rs`, one encode each, one thread):

| Match finder | Size | CPU |
|---|---:|---:|
| bt4 (Max) | 1.391 MB | 33.7 s |
| bt4, 32 cycles | 1.436 MB (+3.2%) | 27.5 s |
| hc4 | 1.452 MB (+4.4%) | 30.4 s |
| hc5 | 1.435 MB (+3.2%) | 31.1 s |
| bt2 / bt3 | 1.400 / 1.397 MB | 34.4 / 34.7 s |

**Hint-guided LZMA** (tried and reverted; the SDK files are unchanged): the
long-range pre-pass hands LZMA its repeats ([start, end, distance]); inside one,
the encoder takes it as a single match (checked byte for byte) and skips the
search, optionally without inserting the skipped positions into the tree. Taking
hints of 256+ bytes: 1.524–1.528 MB (+10%) in 23.3 s without inserting, 33.4 s
with; 512+: 1.409 MB (+1.3%) in 27.9 s. Taking a hint only where LZMA's own
longest match is shorter: 1.390 MB in 32.6 s, i.e. LZMA already finds these
repeats, and its optimal parse codes them ~10% better than one long match each
(repeat distances and split points). The time is the parse over long repeats,
not missed matches.

### BWT (libbsc) for the text and code lanes: the Max breakthrough

With the user's OK, libbsc 3.3.12 (Apache-2.0, bundling libsais 2.10.4) is
vendored in `engine/vendor/libbsc` (see its ARCA-NOTES.md). Block codec `bsc`
(tag 5). One block on one core each (`examples/bscbench.rs`), default bsc
settings (LZP 15/72, static QLFC) against Max's LZMA2 fb 273:

| Block | bsc | LZMA2 fb 273 | bsc size | bsc speed |
|---|---|---|---:|---:|
| saved folder text, 60.4 MB | 6.233 MB, 1.8 s (decode 1.3) | 6.249 MB, 28.3 s (0.22) | −0.3% | 16x |
| Silesia text, 67.7 MB | 7.389 MB, 2.4 s | 9.159 MB, 44.2 s | **−19%** | 18x |
| Silesia text, 17.5 MB | 2.878 MB, 0.8 s | 3.678 MB, 12.1 s | **−22%** | 16x |
| static libraries, 67.1 MB | 1.452 MB, 0.65 s | 1.391 MB, 40.5 s | +4.4% | 62x |
| static libraries, 36.8 MB | 0.880 MB, 0.3 s | 0.859 MB, 22.9 s | +2.4% | 76x |
| Silesia code (x86 tar), 67.2 MB | 21.590 MB, 3.3 s | 18.143 MB, 29.7 s | +19% | 9x |
| Mach-O binaries, 67.6 MB | 13.301 MB, 2.8 s | 10.155 MB, 31.8 s | +31% | 11x |
| Silesia binary, 42.4 MB | 14.482 MB, 2.3 s | 15.802 MB, 20.6 s | −8% | 9x |

The adaptive QLFC coder with a larger LZP closes the code gap: static libraries
1.375 MB with LZP 24/64 (−1.2% against LZMA2, 0.75 s), 1.358 MB without LZP
(−2.4%, 1.7 s); text 6.167 MB with LZP 18/128 (−1.3%) and Silesia 7.321 MB
(−20%). Against PPMd order 16 (Ultra's text codec): 6.167 vs 6.254 MB,
7.321 vs 8.108 MB, 2.858 vs 3.086 MB, and 2–3x faster both ways.

Presets: `bsc:1` text (LZP 18/128, adaptive), `bsc:2` code (LZP 24/64, adaptive);
`e` falls back to LZMA2 fb 273 for a block bsc gets under 16:1 (machine code
without long repeats, where LZMA2 wins).

**Saved folder, Max** (busy Mac): current 211.69 MiB in 32.4 s (212 s CPU);
**with `--lane text=bsc:1 --lane code=bsc:2e`: 211.58 MiB in 19.8 s (136 s
CPU)**: the text block takes 2.7 s instead of ~24 s, the 64 MiB code block 1.0 s
instead of ~30 s. Extraction 2.87–2.95 → 3.15–3.31 s: bsc decodes at ~40 MB/s
against LZMA2's ~250 MB/s (+2.2 s of CPU). Round trip identical.

Website sets (busy Mac; make / extract in seconds; every round trip identical):

| Set | Max | Max + bsc | Ultra (PPMd text) | Ultra + bsc |
|---|---|---|---|---|
| source | 8.232 MB, 23.1 / 0.50 | **8.094 MB, 1.85** / 1.78 | 8.235 MB, 22.9 / 4.05 | **8.093 MB, 1.86 / 1.80** |
| silesia | 48.629 MB, 26.6 / 0.70 | **45.971 MB, 18.4** / 2.04 | 46.933 MB, 14.9 / 6.41 | **45.918 MB**, 18.1 / **2.02** |
| docs | 18.203 MB, 11.8 / 0.45 | same | 17.814 MB, 15.8 / 0.46 | same |
| apps | 49.158 MB, 13.8 / 0.72 | 49.151 MB, 13.5 / 0.73 | 48.526 MB, 16.4 / 0.76 | 48.550 MB (+0.05%), 16.3 / 0.72 |
| mixed (all five) | 194.785 MB, 44.5 / 2.30 | **192.337 MB, 24.1** / 3.01 | 191.906 MB, 40.9 / 7.27 | **191.295 MB, 32.0 / 3.06** |
Both 7-Zip Ultra targets hold with room to spare (source 8.09 < 8.5 MB, Silesia
45.97 < 48.7 MB). Extraction of text-heavy sets is slower at Max: a 64 MiB bsc
block decodes on one core in ~1.6 s, and those sets have few blocks.

**Smaller text blocks** cost size on source code (Max + bsc, size / make /
extract): source 16 MiB 8.368 MB / 1.22 / 0.76 s, 32 MiB 8.274 / 1.27 / 1.12 s,
64 MiB 8.094 / 1.86 / 1.75 s; Silesia 16 MiB 46.137 MB / 0.73 s, 32 MiB 45.975 /
1.08 s, 64 MiB 45.971 / 2.02 s; the saved folder hardly changes (221.85–222.12
MB, 3.1–3.3 s). Deciding the LZMA2 fallback on a 4 MiB sample brought Silesia's
Max make time from 18.4 to 14.9 s.

**Decoding one bsc block on several cores.** A bsc block's decode is 54% QLFC
(entropy), 41% inverse BWT and 5% LZP (samples on Silesia text). bsc's format
already splits the QLFC stream into 24–32 independent sub-blocks for a 64 MiB
block and stores 8–16 sampled BWT indexes, but only an OpenMP build decodes them
in parallel. `vendor/libbsc/src/arca_bsc.cpp` runs bsc_decompress's stages with
plain threads (same format, same checks) and `src/arca_sais.c` (which includes
libsais.c unchanged) adds the threaded inverse BWT. Byte-identical; one block:

| Block | one thread | threaded (wall) |
|---|---:|---:|
| saved folder text, 60 MB | 1.47 s | 0.51 s |
| Silesia text, 68 MB | 1.91 s | 0.67 s |
| static libraries, 67 MB | 0.52 s | 0.22 s |
| Silesia x86 tar, 67 MB | 3.93 s | 0.92 s |

Extraction, Max against Max + bsc (three runs each): source 0.50 s → 0.97–1.00 s
(1.78 s before threading), Silesia 0.69–0.70 → 0.94–1.03 s (2.04 s), saved folder
2.86–3.03 → 3.25–3.38 s (CPU-bound there). bsc needs ~4–6x LZMA2's decode CPU,
so text-heavy archives extract slower than with LZMA2, still well ahead of the
7-Zip and Zipgeist numbers measured on 2026-10-26.

**User's decision (2026-10-28): not in the presets yet**; bsc stays an option
(`--lane text=bsc:1 --lane code=bsc:2e`) until the final quiet-Mac benchmarks.

**Default with bsc text** (instead of zstd-15; size / make / extract):

| Set | Default | bsc text, 16 MiB blocks | bsc text, 32 MiB |
|---|---|---|---|
| source | 11.201 MB, 2.08 / 0.39 s | 10.121 MB (−9.6%), 1.02 / 0.70 s | 10.027 MB (−10.5%), 1.19 / 0.64 s |
| silesia | 57.317 MB, 5.42 / 0.17 s | 52.999 MB (−7.5%), 2.46 / 0.63 s | 52.838 MB (−7.8%), 2.49 / 0.67 s |
| saved folder | 257.399 MB, 9.71 / 2.17 s | 256.250 MB, 8.66 / 2.47 s | 256.178 MB, 8.74 / 2.60 s |

Same trade as at Max: smaller and faster to make, slower to extract (zstd
decodes text at ~1 GB/s).

**Lepton's decoder, decode only** (`LEPT_DECODE_REPS`, samples): 7x7 coefficients
29%, edge coefficients 19%, per-block driver 12%, JPEG Huffman output 12.5%, DC
prediction 7%, DC 6%, non-zero counts 14%. The coding parts are ~67%: a coder
with multi-symbol decisions instead of one binary decision per bit could make
Lepton ~1.5-1.8x faster. Its bool decoder and count-based probabilities (64 KB
lookup) are already lean.

**Multi-symbol size classes for Lepton (measured, dead end).** A shadow model in
a scratchpad copy of Lepton: wherever Lepton codes a coefficient's size class as
a unary chain of binary decisions, a 16-symbol adaptive table for the same
context records what it would cost (ideal code lengths). Photos, 98.4 MB → 73.8 MB:

| Size classes | Lepton | 16-symbol tables | Size |
|---|---|---|---:|
| 7x7 | 22.233 MB, 249 M decisions | 22.596 MB, 134 M symbols | +1.6% |
| edge | 10.952 MB, 148 M decisions | 11.052 MB, 58 M symbols | +0.9% |
| DC | 3.754 MB, 46 M decisions | 3.754 MB, 21 M symbols | 0.0% |

Best variant +0.63% of Lepton's output (final rate 7); finer warm-up +0.83%,
two-speed tables +0.70%, count-based tables with halving +0.63%. Lepton's binary
chain learns each node with its own counts, which this data rewards. And size
classes are only 443 M of Lepton's 930 M decisions (signs, residual bits and
counts are the rest), so the coder would do ~25% less work, not a multiple.
Fails "at least as small"; the JPEG lever needs a different idea.

### Apple Silicon GPU (agenda item 4): no stage worth moving

Measured CPU by stage (saved folder, thread seconds):

| Work | Max make (~210 s) | Max extract (~25 s) | Default make (~62 s) | Fast make (~3.4 s) |
|---|---:|---:|---:|---:|
| LZMA / zstd blocks (adaptive range coding, match search) | ~75% | 13% | ~38% | ~75% |
| Lepton (adaptive binary arithmetic coding per coefficient) | ~10% | 41% | ~33% | — |
| preflate (replays a Deflate encoder byte by byte) | ~8% | 18% | ~20% | — |
| pixel codec (adaptive rANS; prediction is the cheap part) | ~4% | 10% | — | — |
| hashing, chunking, sniffing (data-parallel) | ~2% | — | ~5% | ~35% |
| writing files (kernel, serialised by APFS) | — | ~12% | — | — |

Everything heavy is a serial adaptive model: each symbol's probabilities depend
on the one before, which is what makes these coders small, and is exactly what a
GPU cannot spread out. Published GPU codecs (DietGPU, GPULZ, Recoil; all CUDA, no
Metal) get their speed from static tables and independent tiles, at a cost in
size the brief rules out. The data-parallel part is small: BLAKE3 with NEON on
the CPU cores already hashes the 660 MB in well under 0.1 s, and Fast is bound by
file I/O and serial phases, not hashing. A base M1's 7–8 GPU cores share the CPU's
memory bandwidth, so moving that ~2–5% would add transfers and launch latency for
no wall-clock gain. Revisit only if a GPU-friendly coder (static-table rANS over
independent tiles, or a GPU suffix sort for BWT) is ever adopted for size reasons.

### Long-range pre-pass before LZMA (dead end on size)

Goal: take the code lane's repetition (static libraries, 46:1) off LZMA fb 273,
which spends ~31 s on one 64 MiB block. Prototype `examples/lrp.rs`: gear rolling
hash over 64 bytes, content-defined anchors (~1 in 32), a 4M-slot table, matches
verified and extended both ways, kept when at least MIN bytes; references stored
as three columns (literal-run lengths, match lengths, distances with 0 = repeat),
each LZMA-coded; the literals LZMA fb 273 coded. Round trip checked.

Saved folder's first code block (67.1 MB), one thread, CPU:

| | Size | CPU |
|---|---:|---:|
| LZMA fb 273 alone | 1.391 MB | 33.7 s |
| pre-pass, MIN 256 (95,729 refs cover 52.8%) | 1.276 + refs 0.295 = **1.571 MB** | **17.2 s** |
| pre-pass, MIN 1024 | 1.398 MB | 31.9 s |
| pre-pass, MIN 4096 | 1.397 MB | 32.2 s |
| pre-pass, MIN 16384 | 1.394 MB | 32.9 s |

Twice as fast at MIN 256, but 13% bigger: LZMA codes the same matches in ~1.2
bytes each (repeat distances, context-coded lengths), the reference columns in
~3.1. Even free references would only reach 1.276 MB, and the repeats are mostly
256–1,024 bytes (whole duplicate objects are already removed by chunk dedup), so
longer minimums cover 1–2%. Fails "at least as small"; FreeArc's REP (512-byte
minimum) has the same trade-off.

### Literature survey (background agent, 2026-10-28)

Ranked leads from 2024–26 papers and projects; each still needs a test here:

1. **BWT (libbsc + libsais, Apache-2.0) instead of PPMd for text.** LTCB enwik9:
   bsc 163.9 MB at 23 ns/B compress, 8 ns/B decompress; 7-Zip PPMd 179.0 MB at
   ~500 ns/B both ways. Blocks decode in parallel: the route to Ultra extracting
   on all cores. Needs the source (download: ask).
2. **Deflate replay before preflate**: when a stream is exactly zlib (level,
   memLevel, window), store those bytes and re-run the encoder. Preflate's author
   calls preflate "quite slow"; brute-force zlib matching (precomp, reflate) is
   50–500% faster on small streams. First check which encoders made our PNGs.
3. **Fast LZMA2** radix match finder (BSD-3): 20–100% faster, 1–5% larger. Known.
4. **LZRAVEN/LZMESH**: tested above (dead end, needs macOS 27).
5. **Similarity grouping** of near-duplicate files into the same block (compsort:
   −11.5% on an initrd with xz). MinHash over the CDC chunk hashes already computed.
6. **Mach-O**: code-signature page hashes (SHA-256 per 4 KB page, 0.8–1.3% of a
   binary, incompressible) can be recomputed at extraction and checked at pack
   time; per-slice filters for universal binaries; lc/lp for 4-byte code.
7. **Pixel codec**: JPEG XL's gain from e2 to e7 is learned context trees and
   per-channel predictor choice; a small tree keeps decoding fast.
8. **JPEG**: nothing beats Lepton (2023: Lepton 42 ms encode, 22 ms decode, 19.5%;
   JPEG XL transcode 53 / 68 ms, 14.3%). Our encode+verify is twice the decode
   because `encode_lepton_verify` decodes everything again.
9. **OpenZL** (Meta, BSD): format-aware, fits CSV/columnar data; format unstable.
10. **GPU**: no Metal compression code exists; published GPU codecs are CUDA
    (DietGPU rANS, GPULZ, Recoil). Arca's hot loops are serial context models;
    only tile-parallel pixel residuals or BWT suffix sorting could map to a GPU.
11. Approximate LZ77 via string synchronizing sets (arXiv 2609.30193): 12–19x
    faster parsing, no code published.
12. Extraction: `clonefile()` for duplicate files on APFS.

## Engine ZIP reader and writer

**Reader** (`arca-core/src/zip.rs`, CLI `arca unzip` / `arca ziptest`): parses the
central directory (ZIP64, extended-timestamp and Info-ZIP Unix time fields),
inflates stored and Deflate entries on every core with zlib-rs (flate2's
`zlib-rs` backend, already in the local registry), checks every CRC-32, streams
entries over 64 MiB piece by piece, decodes `._` AppleDouble twins in memory
(ported from the app's Swift parser: no quarantine, no ACLs, Finder's hidden bit
becomes `UF_HIDDEN`) and applies them through each file's descriptor, creates
links last and leaves out any that point outside, and can set the archive's own
quarantine on everything. Anything else (encryption, methods other than store
and Deflate, names that aren't UTF-8, backslashes, duplicate names, split
archives) fails with "unsupported: ...", for the app to fall back to 7-Zip.

On the saved Mac-made ZIP (`ditto-mixed.zip`: 30,682 entries, 632 MiB):

| | Time |
|---|---:|
| check every CRC, nothing written (`ziptest`) | 0.36 s |
| **extract** (`unzip`) | **1.06–1.12 s** |
| `ditto -x -k` (Finder's engine) | 11.8 s |
| the app through 7-Zip, metadata in memory (main, 2026-10-28) | 4.7 s |
| Zipgeist | 2.1 s |

Compared item by item with ditto's extraction (contents, mode, mtime, BSD flags,
extended attributes): 0 differences over 14,506 items. The saved ZIP's twins only
carry `com.apple.provenance`, so a second test folder has a Finder colour label,
tags, a resource fork, an attribute on a folder, a hidden file, an executable, an
80 MB file and two links: identical to ditto except that the escaping link is
left out (intended) and the other link keeps its archived date (ditto doesn't
restore link dates). That case is now `arca-core/tests/zip.rs`, with a damaged
archive and an encrypted one (refused as unsupported).

**In the app:** a tenth FFI function, `arca_zip_extract` (the other nine are
unchanged), and `Engine.extractZip` in the Swift wrapper. `Archives.extract` tries
it for password-less ZIPs and falls back to the existing 7-Zip path when the engine
answers "unsupported:"; the safety pass afterwards is unchanged. The app's test
`hostileZipStaysInside` showed that escaping entries must be left out and the rest
extracted (as 7-Zip does), not the whole archive refused: fixed, reported in
`skipped`, and covered by a new engine test. The inspector labels `lzma2` blocks
"LZMA2" like `xz` ones. **All 58 app tests pass**; the test build was unregistered
with `lsregister -u`.

## Dead ends

- Long-range pre-pass before LZMA (2x faster on code, 13% bigger): see Deep research.
- Multi-symbol size classes inside Lepton: +0.63% for ~25% less coder work.
- One LZMA2 block encoded in parallel segments (state reset, no dictionary
  reset; each segment's match finder first indexes the 24-32 MiB before it; a
  shim including LzmaEnc.c, tried and removed). A 32 MiB window alone costs only
  +0.03-0.08% on Mach-O (16 MiB: +3.5-17%, universal binaries repeat 16-32 MiB
  apart), but indexing the preload costs as much as coding it: 64 MiB Mach-O
  block, 16 MiB segments, 32 MiB window: 10.538 MB, 42.7 s CPU, 12.3 s wall,
  against 10.536 MB, 17.3 s CPU, 12.2 s wall with the two-thread match finder.
- Hash-chain match finders on Mach-O (fb 32, one thread): hc4 10.800 MB / 18.7 s,
  hc5 11.005 MB / 18.4 s against bt4 10.529 MB / 22.6 s: +2.6-4.5% for 17-19% less CPU.
- ARM64 code in byte planes (57.6 MB of the apps' `__text`): LZMA fb 273 13.15 →
  17.79 MB, bsc 17.26 → 22.03 MB. LZMA stays best on raw machine code.
- Code-signature page hashes: only 0.19 MB in 99.5 MB of app binaries (16 KB
  pages on current macOS), ~0.4% of the compressed apps set. Kept as a small
  idea, not a lever.
- GPU: every heavy stage is a serial adaptive coder; the data-parallel 2–5%
  is not worth moving (see Deep research).
- Hint-guided LZMA (the pre-pass's repeats taken as single matches): +1.3–10%
  for 17–31% less CPU; other match finders: +3–4% for ≤18%. See Deep research.
- Decoding image bands side by side on one core, and branchless top-bit decoding:
  the pixel decoder is bound by instruction count (see Default).
- LZRAVEN (needs macOS 27), Apple LZMA, QoS classes, narrowing preflate
  candidates, 32 MiB binary blocks: see their sections.
