# Benchmarks

Measured on 2026-10-25 on an Apple M1 Pro (8P + 2E cores, 32 GB RAM) running macOS 27.0.1, with a warm page cache. Every number is wall-clock time, and every tool's output was extracted and compared with `diff -r` against the source. **Every round trip was byte-identical.** Back-to-back runs vary by about ±10% (thermal and scheduling noise), so treat small time differences as ties.

The raw data is in `bench/results-2026-10-25-*.json` (`all` holds the competitors, `speed-a`/`speed-b` the current engine), and the harness is `bench/bench.py`.

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

* **Arca balanced (the default) vs 7-Zip Ultra:** **5% smaller, 4.4× faster to compress, 1.4× faster to extract.**
* **Arca balanced vs Finder's Compress:** **31% smaller, 1.8× faster to compress, 3.4× faster to extract.**
* **Arca fast vs Finder's Compress:** 11% smaller, **14× faster to compress, 9× faster to extract.**
* **Arca max vs 7-Zip Ultra:** **17% smaller**; compression is 16% slower and extraction 1.25× slower.
* **PDF/PNG folders:** balanced is **43% smaller than 7-Zip Ultra and now faster too** (2.4 s vs 2.8 s). Max is 55% smaller.
* **JPEG photos:** **17% smaller than 7-Zip Ultra**, losslessly, because of Lepton. Every other tool is stuck at 87–95%.
* **Generic data (Silesia):** max ties 7-Zip Ultra on size, compresses 1.5× faster and extracts 1.8× faster. Ultra is 3% smaller than 7-Zip.

## Speed work: same compression, faster

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

### mixed (736 MB)

| Tool | Size (MB) | % of original | vs 7-Zip ultra | Compress (s) | Extract (s) | Round trip |
|---|---:|---:|---:|---:|---:|---|
| Finder zip (ditto) | 322.7 | 43.9% | +38% | 19.8 | 9.32 | identical |
| 7-Zip ultra (7z -mx9) | 233.4 | 31.7% | +0% | 49.0 | 3.78 | identical |
| tar.zst -19 --long | 244.1 | 33.2% | +5% | 46.5 | 5.52 | identical |
| tar.xz -9e | 235.1 | 32.0% | +1% | 97.9 | 7.28 | identical |
| **Arca fast** | 286.8 | 39.0% | +23% | 1.4 | 1.00 | identical |
| **Arca balanced** | 221.5 | 30.1% | -5% | 11.1 | 2.76 | identical |
| **Arca max** | 192.8 | 26.2% | -17% | 56.8 | 4.73 | identical |
| **Arca ultra** | 191.0 | 26.0% | -18% | 43.1 | 10.92 | identical |

### docs (89 MB)

| Tool | Size (MB) | % of original | vs 7-Zip ultra | Compress (s) | Extract (s) | Round trip |
|---|---:|---:|---:|---:|---:|---|
| Finder zip (ditto) | 48.6 | 54.3% | +33% | 1.5 | 0.68 | identical |
| 7-Zip ultra (7z -mx9) | 36.6 | 40.9% | +0% | 2.8 | 0.84 | identical |
| tar.zst -19 --long | 37.1 | 41.5% | +1% | 8.0 | 0.42 | identical |
| tar.xz -9e | 36.9 | 41.3% | +1% | 22.3 | 1.10 | identical |
| **Arca fast** | 40.0 | 44.7% | +9% | 0.2 | 0.09 | identical |
| **Arca balanced** | 20.8 | 23.2% | -43% | 2.4 | 0.34 | identical |
| **Arca max** | 16.5 | 18.4% | -55% | 32.0 | 1.88 | identical |
| **Arca ultra** | 16.5 | 18.4% | -55% | 32.0 | 1.70 | identical |

### jpeg (98 MB)

| Tool | Size (MB) | % of original | vs 7-Zip ultra | Compress (s) | Extract (s) | Round trip |
|---|---:|---:|---:|---:|---:|---|
| Finder zip (ditto) | 93.3 | 94.8% | +10% | 2.5 | 0.47 | identical |
| 7-Zip ultra (7z -mx9) | 85.2 | 86.6% | +0% | 6.3 | 1.67 | identical |
| tar.zst -19 --long | 85.7 | 87.1% | +1% | 15.1 | 0.27 | identical |
| tar.xz -9e | 85.8 | 87.2% | +1% | 43.3 | 2.04 | identical |
| **Arca fast** | 95.0 | 96.5% | +12% | 0.1 | 0.07 | identical |
| **Arca balanced** | 71.0 | 72.2% | -17% | 3.0 | 1.63 | identical |
| **Arca max** | 71.0 | 72.2% | -17% | 2.8 | 1.54 | identical |
| **Arca ultra** | 71.0 | 72.2% | -17% | 2.6 | 1.50 | identical |

### apps (174 MB)

| Tool | Size (MB) | % of original | vs 7-Zip ultra | Compress (s) | Extract (s) | Round trip |
|---|---:|---:|---:|---:|---:|---|
| Finder zip (ditto) | 87.4 | 50.3% | +62% | 5.8 | 1.96 | identical |
| 7-Zip ultra (7z -mx9) | 54.1 | 31.1% | +0% | 31.6 | 1.69 | identical |
| tar.zst -19 --long | 59.4 | 34.1% | +10% | 23.2 | 1.15 | identical |
| tar.xz -9e | 54.4 | 31.3% | +0% | 67.0 | 2.43 | identical |
| **Arca fast** | 72.4 | 41.6% | +34% | 0.5 | 0.31 | identical |
| **Arca balanced** | 62.4 | 35.9% | +15% | 2.6 | 0.46 | identical |
| **Arca max** | 49.8 | 28.7% | -8% | 27.8 | 1.19 | identical |
| **Arca ultra** | 49.8 | 28.7% | -8% | 27.8 | 1.22 | identical |

### source (162 MB)

| Tool | Size (MB) | % of original | vs 7-Zip ultra | Compress (s) | Extract (s) | Round trip |
|---|---:|---:|---:|---:|---:|---|
| Finder zip (ditto) | 24.8 | 15.3% | +191% | 5.2 | 6.09 | identical |
| 7-Zip ultra (7z -mx9) | 8.5 | 5.3% | +0% | 24.3 | 1.18 | identical |
| tar.zst -19 --long | 9.3 | 5.7% | +9% | 26.6 | 3.45 | identical |
| tar.xz -9e | 8.3 | 5.1% | -2% | 71.5 | 3.55 | identical |
| **Arca fast** | 13.5 | 8.4% | +59% | 0.5 | 0.52 | identical |
| **Arca balanced** | 11.2 | 6.9% | +32% | 2.3 | 0.56 | identical |
| **Arca max** | 8.3 | 5.1% | -3% | 27.9 | 0.86 | identical |
| **Arca ultra** | 8.3 | 5.1% | -3% | 28.3 | 4.23 | identical |

### silesia (212 MB)

| Tool | Size (MB) | % of original | vs 7-Zip ultra | Compress (s) | Extract (s) | Round trip |
|---|---:|---:|---:|---:|---:|---|
| Finder zip (ditto) | 68.2 | 32.2% | +40% | 5.8 | 0.30 | identical |
| 7-Zip ultra (7z -mx9) | 48.7 | 23.0% | +0% | 57.1 | 1.71 | identical |
| tar.zst -19 --long | 52.6 | 24.8% | +8% | 32.9 | 0.27 | identical |
| tar.xz -9e | 48.4 | 22.9% | -1% | 99.3 | 2.15 | identical |
| **Arca fast** | 65.7 | 31.0% | +35% | 0.3 | 0.10 | identical |
| **Arca balanced** | 57.3 | 27.0% | +18% | 5.2 | 0.19 | identical |
| **Arca max** | 48.6 | 23.0% | -0% | 37.1 | 0.95 | identical |
| **Arca ultra** | 47.0 | 22.2% | -3% | 29.9 | 7.36 | identical |

## Where Arca is still behind (and the fixes)

* *Max* extraction on the mixed folder is 1.25× slower than 7z (4.7 s vs 3.8 s). That's the cost of reconstructing JPEGs and Deflate streams, which 7z doesn't do. The fix is moving reconstruction onto more cores per file (Lepton supports multi-threaded decoding for large images).
* *Max* compression on the mixed folder is 16% slower than 7z, for the LZMA `-9e` reason above.
* *Ultra* (PPMd text lane) helps only prose-heavy data (−3.4% on Silesia, −1% on mixed) and extracts ~2× slower, so it stays opt-in.
* *Fast* skips Lepton and preflate to stay at ~500–800 MB/s, so on JPEG/PDF-only folders it's slightly larger than 7z.

## Reproduce

```bash
brew install sevenzip zstd xz
cd engine && cargo build --release && cd ..
python3 bench/bench.py --arca engine/target/release/arca --work /tmp/arca-bench DIR1 DIR2 ...
```

Set `ARCA_DEBUG_TIMING=1` to print pack phases, or `ARCA_DEBUG_ROUTING=1` to print each file's class, lane and transform.
