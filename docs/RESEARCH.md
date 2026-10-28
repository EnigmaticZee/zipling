# Arca engine research log

Started 2026-10-28 on branch `zee/arca-research-77726f`. Backup: tag
`before-engine-research` (commit `cc46547`).

**Resuming?** Read this file top to bottom first. "State" says where the work is;
everything below it is the record.

## State

- Step: 1 (log) done; 2 (test sets) in progress.
- Next: rebuild the website sets, then profile every level.
- Waiting on the user: nothing yet.

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

1. Max PNG rebuild cost: (to do)
2. Max memory on 8 GB Macs: (to do)
3. Max blocks vs cores: (to do)
4. Fast's duplicate and fingerprint cost: (to do)

## Deep research

(to fill: codecs, transforms, dedup, scheduling, writing files, GPU, lzraven)

## Engine ZIP reader and writer

(to do)

## Dead ends

(none yet)
