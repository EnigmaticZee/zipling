# Benchmarks

Measured on an Apple M1 Pro (8P + 2E cores, 32 GB RAM) running macOS 27.0.1, with a warm page cache. Every number is wall-clock time, and every tool's output was extracted and compared with `diff -r` against the source. **Every round trip was byte-identical.** Back-to-back runs vary by about ±10% (thermal and scheduling noise), so treat small time differences as ties.

The Arca and 7-Zip numbers were measured together on 2026-10-26 on an otherwise idle machine. The Finder zip and tar rows are from 2026-10-25. That session ran LZMA-heavy jobs about 20% faster: the same Arca build packed the mixed folder at max in 56.8 s then and 68.6 s on 2026-10-26. So compare times only within one session.

The raw data is in `bench/results-2026-10-25-*.json` (`all` holds the competitors, `speed-a`/`speed-b` the first speed pass) and `bench/results-2026-10-26-max-speed.json` (the max-level pass, before and after, plus 7-Zip). The harness is `bench/bench.py`.

Versions: Arca engine 0.1 (this repo), 7-Zip 26.02 (`7zz`), zstd 1.5.7, XZ Utils 5.8.4, and `ditto` from macOS 27, which is what Finder's *Compress* uses. Keka and BetterZip create 7z through the 7-Zip engine, and Archive Utility and MacPacker create ZIP (Deflate), so those rows stand in for them.

## Datasets

All sets use public or system data only; no personal files were read.

| Set | What | Why it matters |
|---|---|---|
| mixed | All five sets below in one folder | A realistic "compress this folder" job |
| docs | 250 PDFs + 600 PNGs from system and Xcode resources | Office, PDF and screenshot-heavy folders |
| jpeg | 400 JPEGs from system and Xcode resources | Photo folders |
| apps | 8 apps from `/System/Applications` (universal Mach-O, asset catalogs, nibs, strings) | Software, `.app` bundles |
| source | 89 Rust crates incl. vendored C (zstd, xz) | Code repositories |
| silesia | The standard Silesia corpus | Generic data; there are no transform opportunities here |

## Headline (mixed folder, 736 MB)

* **Arca balanced (the default) vs 7-Zip Ultra:** **5% smaller, 5.1× faster to compress, 1.7× faster to extract.**
* **Arca max vs 7-Zip Ultra:** **17% smaller and 1.1× faster to extract** (3.50 s vs 3.84 s); compression is within 5% (51.6 s vs 49.3 s). Before the max-level speed pass, Arca max was 1.2× slower to extract and 1.4× slower to compress.
* **Arca balanced vs Finder's Compress:** **31% smaller, 2× faster to compress, 4× faster to extract.**
* **Arca fast vs Finder's Compress:** 11% smaller, **14× faster to compress, 10× faster to extract.**
* **PDF/PNG folders:** balanced is **43% smaller than 7-Zip Ultra and faster too** (2.0 s vs 2.7 s). Max is 55% smaller.
* **JPEG photos:** **17% smaller than 7-Zip Ultra**, losslessly, because of Lepton, and faster to extract (1.25 s vs 1.58 s). Every other tool is stuck at 87–95%.
* **Generic data (Silesia):** max ties 7-Zip Ultra on size, compresses 1.5× faster and extracts 1.7× faster. Ultra is 3% smaller than 7-Zip.

## Speed work, part 2: the max level

The rule was the same as in part 1: faster, with no archive larger. None of these changes alters an archive. Items 1–3 only touch extraction, and items 4–5 produce byte-identical compressed blocks. The sizes below differ by at most +0.0001%, a few bytes of index timestamp.

1. **Extraction is a dependency graph, not a series of waves.** A trace of the old code showed why max extraction trailed 7-Zip. Blocks decoded in waves of one per core, each wave waiting for its slowest block, and the stored blocks holding the Lepton data came last. The JPEGs, the largest share of the work (12.5 s of CPU on the mixed folder), therefore only started at 2.9 s of 4.7 s. Now every core takes the best ready item from one priority queue: block decodes first (cheap stored blocks before LZMA ones), then the costliest work, largest JPEG first. An object is rebuilt the moment its last block decodes. On the mixed folder all 10 cores stay busy from 0.2 s to the end. Decoded blocks are capped at two per core, and peak memory is unchanged (1.9 → 2.0 GB).
2. **Directories are created by the first file that needs them.** APFS serialises `mkdir`, so creating the mixed folder's 2,112 directories took ~0.1 s even on 10 threads. Done as a separate step overlapping the decodes, it took 0.6–1.2 s on one core while every write waited for it. Path checks and directory metadata now run in parallel too.
3. **Costs are estimated from what the work actually is.** Re-creating Deflate scales with the expanded size, which can be 50× the file's. A 0.5 MB PNG holding one huge image takes 0.9 s, so it has to start early, not after the JPEGs.
4. **liblzma is built the way xz's own build does.** The `liblzma-sys` crate compiles xz with a hand-written `config.h` that leaves out `TUKLIB_FAST_UNALIGNED_ACCESS`, so the BT4 match finder, 77% of LZMA's time, compared match lengths byte by byte. With the flag, a 64 MiB `-9e` block compresses **1.31× faster on text (28.7 → 21.9 s) and 1.45× faster on binaries (24.4 → 16.8 s)**, with identical bytes.
5. **The match finder prefetches the hash bucket four positions ahead.** That is a random access into a table of up to 64 MiB. With 10 encoders sharing memory, which is how a max archive is packed, it saves ~5% of cycles (804–811 → 767–770 billion over 10 concurrent blocks; 32.6–34.5 → 31.0–31.4 s), with identical bytes. Items 4 and 5 live in a vendored copy of `liblzma-sys` (`engine/vendor/liblzma-sys/ARCA-PATCHES.md`).

Best of two runs per cell, alternating old and new builds:

| Set | Level | Size before → after (MB) | Pack before → after (s) | Speed-up | Extract before → after (s) | Speed-up |
|---|---|---:|---:|---:|---:|---:|
| mixed | balanced | 221.46 → 221.46 | 9.3 → 9.7 | 0.96× | 2.63 → 2.30 | 1.15× |
| mixed | max | 192.80 → 192.80 | 68.6 → 51.6 | **1.33×** | 4.58 → 3.50 | **1.31×** |
| docs | balanced | 20.79 → 20.79 | 2.0 → 2.0 | 1.00× | 0.30 → 0.26 | 1.17× |
| docs | max | 16.46 → 16.46 | 36.6 → 29.3 | 1.25× | 1.63 → 1.20 | 1.36× |
| jpeg | balanced | 71.02 → 71.02 | 2.5 → 2.4 | 1.01× | 1.51 → 1.24 | 1.22× |
| jpeg | max | 71.02 → 71.02 | 2.5 → 2.5 | 1.00× | 1.42 → 1.25 | 1.14× |
| apps | balanced | 62.43 → 62.43 | 2.4 → 2.4 | 1.00× | 0.47 → 0.28 | 1.65× |
| apps | max | 49.84 → 49.84 | 28.8 → 23.5 | 1.23× | 1.17 → 1.04 | 1.13× |
| source | balanced | 11.22 → 11.22 | 2.0 → 2.1 | 0.96× | 0.58 → 0.51 | 1.14× |
| source | max | 8.29 → 8.29 | 35.6 → 25.4 | 1.40× | 0.83 → 0.72 | 1.15× |
| silesia | balanced | 57.27 → 57.27 | 5.1 → 5.1 | 1.01× | 0.18 → 0.15 | 1.24× |
| silesia | max | 48.65 → 48.65 | 39.6 → 33.0 | 1.20× | 0.92 → 0.89 | 1.04× |

Balanced packing runs the same code as before; its ±4% is noise. **Max extraction now beats 7-Zip Ultra on the mixed folder.** Five alternating runs each gave a median of 3.52 s (3.38–3.59) for Arca against 3.85 s (3.83–3.87) for 7-Zip, and Arca won every pair.

Dead ends, kept for the record:

* *Starting every LZMA block at once* (a 16-thread compression pool instead of 10). The trace showed 14 blocks on 10 threads, the last two running nearly alone for ~9 s, and that looked like idle cores to fill. It made no difference (57.98 s vs 58.27 s). Compression is bound by total LZMA throughput, which is memory-bound: one 64 MiB block takes 23 s alone and 40–49 s with ten running. So the fix had to make LZMA itself cheaper (items 4 and 5).
* *Prefetching both children of each BT4 tree node.* 15–20% slower.
* *Smaller blocks when there are fewer blocks than cores.* Docs has five 64 MiB blocks for 10 cores, so half the machine idles at max. With 48 MiB blocks it packs 1.39× faster but grows 0.17%; with 32 MiB, 1.56× faster and 0.94% larger. Both fail the size rule.
* *More extraction threads than cores* (12 or 14), to hide file-system waits. No gain.
* *Multi-threaded Lepton decoding.* Not needed once the largest JPEGs start first: the cores stay busy to the end. The JPEGs are already stored with up to eight Lepton partitions, so it stays available for folders of a few huge photos.

## Speed work, part 1: same compression, faster

Each change below either leaves the archive bytes unchanged or was kept only because it made archives the same size or smaller:

1. **Per-lane codecs.** The profile showed zstd-15's binary-tree match finder using ~40% of all CPU. Text really needs it (lighter strategies cost 2.6% on text), but binaries lose under 0.4% with zstd-12's lazy matcher at 3–4× the speed. Doubling binary blocks to 64 MiB, which costs no CPU, wins that back and more.
2. **One verification instead of two** for Deflate expansion. Preflate's internal check duplicated the engine's own end-to-end round trip, which is stronger, so the internal check is off.
3. **Parallel Deflate streams within a file**, for both packing and extraction. A PDF or DOCX holds hundreds of independent streams.
4. **Shallower preflate search at balanced** (depth 256, which covers zlib levels 1–7). Archives come out the same size within 0.15% in the worst case, and preflate runs 1.7× faster.
5. **A streaming pack pipeline.** Every file is its own task, running up to 512 MB ahead of an in-order packer. Blocks compress on a separate pool as soon as they fill, and each lane's last block is sealed the moment its file group has passed. The archive layout stays deterministic.
6. **Extraction overlaps block decoding with file rebuilding.** Single-chunk files are created, written and given their permissions, timestamps and xattrs through one file descriptor. Directories are created once each.
7. **Small files are read once** when packing (whole, during type detection), and the largest JPEGs are scheduled first.

| Set | Level | Size before → after (MB) | Pack before → after (s) | Speed-up | Extract before → after (s) | Speed-up |
|---|---|---:|---:|---:|---:|---:|
| mixed | fast | 286.76 → 286.76 | 1.5 → 1.4 | 1.06× | 1.49 → 1.00 | 1.49× |
| mixed | balanced | 222.07 → 221.46 | 19.8 → 11.1 | 1.79× | 3.94 → 2.76 | 1.42× |
| mixed | max | 192.80 → 192.80 | 56.1 → 56.8 | 0.99× | 5.91 → 4.73 | 1.25× |
| docs | fast | 40.01 → 40.01 | 0.1 → 0.2 | 0.86× | 0.12 → 0.09 | 1.26× |
| docs | balanced | 23.31 → 20.79 | 6.9 → 2.4 | 2.87× | 1.76 → 0.34 | 5.23× |
| docs | max | 16.46 → 16.46 | 33.2 → 32.0 | 1.04× | 1.83 → 1.88 | 0.97× |
| jpeg | fast | 95.00 → 95.00 | 0.1 → 0.1 | 1.00× | 0.09 → 0.07 | 1.30× |
| jpeg | balanced | 71.02 → 71.02 | 2.6 → 3.0 | 0.89× | 1.58 → 1.63 | 0.97× |
| jpeg | max | 71.02 → 71.02 | 2.8 → 2.8 | 1.00× | 1.70 → 1.54 | 1.10× |
| apps | fast | 72.40 → 72.40 | 0.4 → 0.5 | 0.99× | 0.37 → 0.31 | 1.22× |
| apps | balanced | 64.51 → 62.43 | 3.9 → 2.6 | 1.53× | 0.49 → 0.46 | 1.06× |
| apps | max | 49.84 → 49.84 | 28.7 → 27.8 | 1.03× | 1.24 → 1.19 | 1.04× |
| source | fast | 13.55 → 13.55 | 0.7 → 0.5 | 1.29× | 0.80 → 0.52 | 1.54× |
| source | balanced | 11.33 → 11.21 | 2.4 → 2.3 | 1.04× | 0.83 → 0.56 | 1.49× |
| source | max | 8.29 → 8.29 | 28.8 → 27.9 | 1.03× | 1.16 → 0.86 | 1.36× |
| silesia | fast | 65.74 → 65.74 | 0.3 → 0.3 | 0.90× | 0.14 → 0.10 | 1.43× |
| silesia | balanced | 57.05 → 57.27 | 6.9 → 5.2 | 1.33× | 0.16 → 0.19 | 0.85× |
| silesia | max | 48.65 → 48.65 | 38.3 → 37.1 | 1.03× | 0.97 → 0.95 | 1.02× |
What didn't move, and why:

* *Max* compression time is LZMA `-9e` itself. Plain `-9` was 1.1–1.2× faster on samples but cost up to 6% on real folders, so it was reverted. The product answer is the roadmap's "compress fast now, shrink later" (pack at *fast* in 1.4 s, then upgrade to *max* in the background), not a weaker codec.
* *Fast* compression was already near I/O speed (500–800 MB/s).
* *Silesia at balanced* is the one archive that grew, by 0.4% (57.05 → 57.27 MB). Its large distinct binaries are where zstd-15's deeper search still pays. Every real-world folder got smaller.
* JPEG compression is Lepton's encode plus verification, about 5 MB/s per core. The vector (SIMD) paths are already in use on Apple Silicon.

Dead ends, kept for the record: a smaller zstd window (no effect), zstd minimum-match tuning (helped 32 MB samples, hurt full folders), and a per-file "is expansion worth it?" estimate. That estimate helped the docs set alone but hurt the mixed folder, because files that look unprofitable alone become profitable next to similar files in a big block.

## Full results

Arca and 7-Zip rows: 2026-10-26, best of two runs for Arca balanced and max. † Finder zip and tar rows: 2026-10-25, a session that ran LZMA-heavy jobs about 20% faster (see the top of this page).

### mixed (736 MB)

| Tool | Size (MB) | % of original | vs 7-Zip ultra | Compress (s) | Extract (s) | Round trip |
|---|---:|---:|---:|---:|---:|---|
| Finder zip (ditto) † | 322.7 | 43.9% | +38% | 19.8 | 9.32 | identical |
| 7-Zip ultra (7z -mx9) | 233.4 | 31.7% | +0% | 49.3 | 3.84 | identical |
| tar.zst -19 --long † | 244.1 | 33.2% | +5% | 46.5 | 5.52 | identical |
| tar.xz -9e † | 235.1 | 32.0% | +1% | 97.9 | 7.28 | identical |
| **Arca fast** | 286.8 | 39.0% | +23% | 1.4 | 0.88 | identical |
| **Arca balanced** | 221.5 | 30.1% | -5% | 9.7 | 2.30 | identical |
| **Arca max** | 192.8 | 26.2% | -17% | 51.6 | 3.50 | identical |
| **Arca ultra** | 191.0 | 26.0% | -18% | 37.9 | 7.45 | identical |

### docs (89 MB)

| Tool | Size (MB) | % of original | vs 7-Zip ultra | Compress (s) | Extract (s) | Round trip |
|---|---:|---:|---:|---:|---:|---|
| Finder zip (ditto) † | 48.6 | 54.3% | +33% | 1.5 | 0.68 | identical |
| 7-Zip ultra (7z -mx9) | 36.6 | 40.9% | +0% | 2.7 | 0.82 | identical |
| tar.zst -19 --long † | 37.1 | 41.5% | +1% | 8.0 | 0.42 | identical |
| tar.xz -9e † | 36.9 | 41.3% | +1% | 22.3 | 1.10 | identical |
| **Arca fast** | 40.0 | 44.7% | +9% | 0.1 | 0.09 | identical |
| **Arca balanced** | 20.8 | 23.2% | -43% | 2.0 | 0.26 | identical |
| **Arca max** | 16.5 | 18.4% | -55% | 29.3 | 1.20 | identical |
| **Arca ultra** | 16.5 | 18.4% | -55% | 29.4 | 1.19 | identical |

### jpeg (98 MB)

| Tool | Size (MB) | % of original | vs 7-Zip ultra | Compress (s) | Extract (s) | Round trip |
|---|---:|---:|---:|---:|---:|---|
| Finder zip (ditto) † | 93.3 | 94.8% | +10% | 2.5 | 0.47 | identical |
| 7-Zip ultra (7z -mx9) | 85.2 | 86.6% | +0% | 6.0 | 1.58 | identical |
| tar.zst -19 --long † | 85.7 | 87.1% | +1% | 15.1 | 0.27 | identical |
| tar.xz -9e † | 85.8 | 87.2% | +1% | 43.3 | 2.04 | identical |
| **Arca fast** | 95.0 | 96.5% | +12% | 0.1 | 0.06 | identical |
| **Arca balanced** | 71.0 | 72.2% | -17% | 2.4 | 1.24 | identical |
| **Arca max** | 71.0 | 72.2% | -17% | 2.5 | 1.25 | identical |
| **Arca ultra** | 71.0 | 72.2% | -17% | 2.5 | 1.29 | identical |

### apps (174 MB)

| Tool | Size (MB) | % of original | vs 7-Zip ultra | Compress (s) | Extract (s) | Round trip |
|---|---:|---:|---:|---:|---:|---|
| Finder zip (ditto) † | 87.4 | 50.3% | +62% | 5.8 | 1.96 | identical |
| 7-Zip ultra (7z -mx9) | 54.1 | 31.1% | +0% | 27.6 | 1.69 | identical |
| tar.zst -19 --long † | 59.4 | 34.1% | +10% | 23.2 | 1.15 | identical |
| tar.xz -9e † | 54.4 | 31.3% | +0% | 67.0 | 2.43 | identical |
| **Arca fast** | 72.4 | 41.6% | +34% | 0.4 | 0.22 | identical |
| **Arca balanced** | 62.4 | 35.9% | +15% | 2.4 | 0.28 | identical |
| **Arca max** | 49.8 | 28.7% | -8% | 23.5 | 1.04 | identical |
| **Arca ultra** | 49.8 | 28.7% | -8% | 23.5 | 1.03 | identical |

### source (162 MB)

| Tool | Size (MB) | % of original | vs 7-Zip ultra | Compress (s) | Extract (s) | Round trip |
|---|---:|---:|---:|---:|---:|---|
| Finder zip (ditto) † | 24.8 | 15.3% | +191% | 5.2 | 6.09 | identical |
| 7-Zip ultra (7z -mx9) | 8.5 | 5.3% | +0% | 22.0 | 1.22 | identical |
| tar.zst -19 --long † | 9.3 | 5.7% | +9% | 26.6 | 3.45 | identical |
| tar.xz -9e † | 8.3 | 5.1% | -2% | 71.5 | 3.55 | identical |
| **Arca fast** | 13.5 | 8.4% | +59% | 0.5 | 0.54 | identical |
| **Arca balanced** | 11.2 | 6.9% | +32% | 2.1 | 0.51 | identical |
| **Arca max** | 8.3 | 5.1% | -3% | 25.4 | 0.72 | identical |
| **Arca ultra** | 8.3 | 5.1% | -3% | 24.9 | 3.57 | identical |

### silesia (212 MB)

| Tool | Size (MB) | % of original | vs 7-Zip ultra | Compress (s) | Extract (s) | Round trip |
|---|---:|---:|---:|---:|---:|---|
| Finder zip (ditto) † | 68.2 | 32.2% | +40% | 5.8 | 0.30 | identical |
| 7-Zip ultra (7z -mx9) | 48.7 | 23.0% | +0% | 48.3 | 1.53 | identical |
| tar.zst -19 --long † | 52.6 | 24.8% | +8% | 32.9 | 0.27 | identical |
| tar.xz -9e † | 48.4 | 22.9% | -1% | 99.3 | 2.15 | identical |
| **Arca fast** | 65.7 | 31.0% | +35% | 0.3 | 0.08 | identical |
| **Arca balanced** | 57.3 | 27.0% | +18% | 5.1 | 0.15 | identical |
| **Arca max** | 48.6 | 23.0% | -0% | 33.0 | 0.89 | identical |
| **Arca ultra** | 47.0 | 22.2% | -3% | 25.1 | 5.96 | identical |

## Where Arca is still behind (and the fixes)

* *Max* extraction of PDF/PNG folders is slower than 7z (1.20 s vs 0.82 s on docs, for an archive 55% smaller). Two PNGs that each hold one huge image take 0.8–0.9 s apiece to re-create, and preflate can only re-create a stream sequentially. Everything else in the folder is done by then.
* *Max* compression is within 5% of 7z on the mixed folder, but slower on docs (29.3 s vs 2.7 s) and source (25.4 s vs 22.0 s). Docs expands its PDFs and PNGs 3.6× and LZMA-compresses the result, which is where its 55% comes from, and it has only five blocks for 10 cores. The remaining lever is a 7-Zip-style multi-threaded match finder (one thread finds matches while another encodes, with identical output). That is a substantial liblzma change.
* Writing 10,000 small files costs ~3.5 s of kernel time across 10 threads against 1.5 s on one thread, because of contention inside APFS. Spread over 10 cores this is ~0.3 s of the mixed folder's extraction.
* *Ultra* (PPMd text lane) helps only prose-heavy data (−3.4% on Silesia, −1% on mixed) and extracts ~2× slower, so it stays opt-in.
* *Fast* skips Lepton and preflate to stay at ~500–800 MB/s, so on JPEG/PDF-only folders it's slightly larger than 7z.

## Reproduce

```bash
brew install sevenzip zstd xz
cd engine && cargo build --release && cd ..
python3 bench/bench.py --arca engine/target/release/arca --work /tmp/arca-bench DIR1 DIR2 ...
```

Set `ARCA_DEBUG_TIMING=1` to print pack or unpack phases, or `ARCA_DEBUG_ROUTING=1` to print each file's class, lane and transform.
