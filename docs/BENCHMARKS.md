# Benchmarks

Zipling, the app, against Finder, 7-Zip, WinRAR, zstd and xz on six public datasets: archive size,
time to compress, time to extract, peak memory. Every archive from every tool was extracted and
compared with the source byte for byte (names, types, bytes, link targets). **Every round trip was
identical.**

**Machine.** MacBook Pro, Apple M1 Pro (8 performance + 2 efficiency cores), 32 GB, internal SSD,
macOS 27.0.1. Each dataset is read once before its runs, so every tool starts with the files in the
page cache. Two rounds per tool, the second in reverse order, the better time kept; where the two
rounds differ in size (Zipling's blocks take one or two threads as the machine allows, 0.05 to 0.3%),
the larger archive counts. A dataset's runs start
only when the 1-minute load is under 2.5. Back-to-back runs vary by about ±10%, so treat small
differences as ties.

**When.** Finder on 31 October 2026, driven in Finder itself (its Compress command, and Archive
Utility by double-click), timed from the click to the finished file. Zipling and the command-line
tools in one unattended session the night of 31 October to 1 November 2026, alternating within each
dataset so that no tool gets a quieter machine than another. zstd and xz were re-measured in a second
pass the same night with the current Homebrew releases, after the first pass had picked up older
copies; the second pass is what the tables show. Zipling's Default was measured again on the morning
of 1 November 2026, in the app rebuilt after the engine's code lane change (static libraries and
executables compressed with BWT where it pays), with the same harness and clock; those rows replace
the earlier Default rows in the tables. Nothing else was re-run.

**Versions.** Zipling 0.1.0 (Arca engine, archive format 3, the presets locked on 31 October 2026,
Default's code lane added 1 November);
7-Zip 26.05 (`7zz -mx5` Normal and `-mx9` Ultra, links kept); WinRAR 7.23 for macOS, command line
(`rar -m1`, `-m3`, `-m5 -s`); zstd 1.5.7 (`tar | zstd -3 -T0` and `-19 --long=27 -T0`); XZ Utils 5.8.4
(`tar | xz -6 -T0` and `-9e -T0`); Finder from macOS 27.0.1. Keka and BetterZip compress 7z through
7-Zip's code, so the 7-Zip rows stand in for them; Archive Utility and MacPacker make Deflate ZIPs,
as Finder does.

**How Zipling was timed.** The installed app, driven through its AppleScript `compress` and `extract`
commands, timed by its own clock from the command's arrival in the app to the finished file (the
app's timing log). That includes the app's listing decode, its safety walk over the extracted tree and
its main-actor hops, about 30 ms plus a few microseconds per file on an extraction, and it is what a
user gets. The AppleScript round trip itself (50 to 60 ms of osascript start-up, measured before each
dataset) is not charged to the app. Peak memory is the app process's physical footprint, sampled every
20 ms; for the command-line tools it is `/usr/bin/time -l`'s maximum resident set size.

Raw rows: `bench/results-2026-10-31-apps.jsonl` (Zipling, 7-Zip, WinRAR, and the first zstd and xz
pass), `bench/results-2026-10-31-apps-zx.jsonl` (zstd and xz, second pass),
`bench/results-2026-10-31-apps-finder.jsonl` (Finder), `bench/results-2026-11-01-apps-default.jsonl`
(Default, 1 November), `bench/results-2026-10-31-apps-machine.json` (machine, versions, method).
Harness: `bench/research/apprun.py`; scoring: `bench/research/levels.py`.

## Datasets

Public and system data only; no personal files were read. Sizes are the folders' decimal megabytes.

| Set | What | Why it matters |
|---|---|---|
| Mixed folder (736 MB) | All five sets below in one folder | A realistic "compress this folder" job |
| PDFs and screenshots (89 MB) | 250 PDFs and 600 PNGs from system and Xcode resources | Office, PDF and screenshot-heavy folders |
| Photos (98 MB) | 400 JPEGs from system and Xcode resources | Photo folders |
| Mac apps (174 MB) | 8 apps from `/System/Applications`: universal Mach-O, asset catalogs, nibs, strings | Software and `.app` bundles |
| Source code (162 MB) | 89 Rust crates, including vendored C (zstd, xz) | Code repositories |
| Generic data (212 MB) | The standard Silesia compression corpus | Data with no file-type tricks to exploit |

## Headline (mixed folder, 736 MB)

- **Zipling Default against 7-Zip Ultra:** 9% smaller, 5.5× faster to compress, 2.4× faster to extract (1.62 against 3.84 s).
- **Zipling Max against 7-Zip Ultra:** 18% smaller (192.6 against 235.0 MB) in 40% of the compression time, 1.5× faster to extract.
- **Zipling Default against Finder's Compress:** 34% smaller, 2.7× faster to compress, 4.9× faster to extract.
- **Zipling Fast against Finder's Compress:** 13% smaller, 47× faster to compress, 10× faster to extract.
- **Zipling Fast against `tar | zstd -3`:** 5% smaller, 11× faster to compress, 7× faster to extract.
- **PDFs and screenshots:** Default is 42% smaller than 7-Zip Ultra and 2.1× faster to compress; Max is 52% smaller.
- **Photos:** Default is 19% smaller than 7-Zip Ultra (69.18 against 85.29 MB), with every JPEG restored bit for bit; the other tools get 5% to 13% off the originals.

## Where Zipling is behind

- **Default on Mac apps** is 4% larger than 7-Zip Ultra and 2% larger than 7-Zip Normal, for 12× to 20× less compression time and 5× to 6× less extraction time. Max is 9% smaller than 7-Zip Ultra there, in 12.1 s against 27.7 s.
- **Default on source code** is 11% larger than 7-Zip Ultra and 3% larger than 7-Zip Normal, for 7× to 12× less compression time and 2× less extraction time. Max is 2% smaller than 7-Zip Ultra there, in 1.55 s against 21.9 s.
- **Default on generic data** is 18% larger than 7-Zip Ultra and 15% larger than 7-Zip Normal, for 5× to 10× less compression time and 7× to 11× less extraction time. Max is 4% smaller than 7-Zip Ultra there, in 6.22 s against 48.2 s.
- **Max on PDFs and screenshots** takes 10.5 s to compress against 7-Zip Ultra's 2.86 s, for an archive 52% smaller: re-creating the compression inside PDFs and PNGs is work the other tools skip. Default is 42% smaller than 7-Zip Ultra on the same folder in 1.35 s.
- **Fast on generic data** loses to `zstd -3` on compression time (0.17 against 0.15 s) at a file 2% smaller that extracts 1.9× faster.
- **Ultra*** is 0 to 3% smaller than Max for 5 to 117% more compression time. It is experimental and not in the released app.

## Every dataset, every tool

Each Zipling level sits with the settings it competes with. Ultra* is experimental and not in the
released app.

### Mixed folder (736 MB)

All five sets below in one folder. A realistic "compress this folder" job.

| Tool | Size | % of original | Compress | Extract | Peak memory, compressing |
|---|---:|---:|---:|---:|---:|
| *Fast settings* | | | | | |
| Finder Compress / Archive Utility | 324.6 MB | 44% | 24.1 s | 7.90 s | – |
| tar + zstd -3 | 294.6 MB | 40% | 5.63 s | 5.50 s | 172 MB |
| WinRAR Fastest (-m1) | 309.8 MB | 42% | 5.61 s | 3.18 s | 279 MB |
| **Zipling Fast** | 281.1 MB | 38% | 0.51 s | 0.82 s | 1704 MB |
| *Normal settings* | | | | | |
| 7-Zip Normal (-mx5) | 237.8 MB | 32% | 26.1 s | 2.96 s | 2523 MB |
| WinRAR Normal (-m3) | 277.8 MB | 38% | 10.6 s | 3.26 s | 306 MB |
| tar + xz -6 | 251.2 MB | 34% | 25.1 s | 5.79 s | 1357 MB |
| **Zipling Default** | 214.3 MB | 29% | 8.80 s | 1.62 s | 3972 MB |
| *Strongest settings* | | | | | |
| 7-Zip Ultra (-mx9) | 235.0 MB | 32% | 48.5 s | 3.84 s | 6874 MB |
| WinRAR Best (-m5, solid) | 247.4 MB | 34% | 21.6 s | 2.98 s | 578 MB |
| tar + zstd -19 --long | 245.7 MB | 33% | 46.1 s | 5.51 s | 1871 MB |
| tar + xz -9e | 236.6 MB | 32% | 87.8 s | 7.10 s | 3656 MB |
| **Zipling Max** | 192.6 MB | 26% | 19.3 s | 2.55 s | 3644 MB |
| **Zipling Ultra*** | 189.3 MB | 26% | 28.1 s | 2.73 s | 4213 MB |

### PDFs and screenshots (89 MB)

250 PDFs and 600 PNGs from system and Xcode resources. Office, PDF and screenshot-heavy folders.

| Tool | Size | % of original | Compress | Extract | Peak memory, compressing |
|---|---:|---:|---:|---:|---:|
| *Fast settings* | | | | | |
| Finder Compress / Archive Utility | 50.24 MB | 56% | 2.03 s | 1.25 s | – |
| tar + zstd -3 | 45.07 MB | 51% | 0.40 s | 0.42 s | 105 MB |
| WinRAR Fastest (-m1) | 50.04 MB | 56% | 0.84 s | 0.39 s | 99 MB |
| **Zipling Fast** | 41.06 MB | 46% | 0.08 s | 0.12 s | 1000 MB |
| *Normal settings* | | | | | |
| 7-Zip Normal (-mx5) | 38.29 MB | 43% | 2.47 s | 0.80 s | 382 MB |
| WinRAR Normal (-m3) | 49.75 MB | 56% | 1.08 s | 0.36 s | 126 MB |
| tar + xz -6 | 43.29 MB | 49% | 4.14 s | 0.64 s | 505 MB |
| **Zipling Default** | 22.14 MB | 25% | 1.35 s | 0.26 s | 1452 MB |
| *Strongest settings* | | | | | |
| 7-Zip Ultra (-mx9) | 38.25 MB | 43% | 2.86 s | 0.83 s | 920 MB |
| WinRAR Best (-m5, solid) | 38.74 MB | 44% | 1.74 s | 0.30 s | 567 MB |
| tar + zstd -19 --long | 38.82 MB | 44% | 6.27 s | 0.41 s | 359 MB |
| tar + xz -9e | 38.80 MB | 44% | 20.8 s | 1.15 s | 794 MB |
| **Zipling Max** | 18.20 MB | 20% | 10.5 s | 0.45 s | 1611 MB |
| **Zipling Ultra*** | 17.81 MB | 20% | 15.0 s | 0.46 s | 1455 MB |

### Photos (98 MB)

400 JPEGs from system and Xcode resources. Photo folders.

| Tool | Size | % of original | Compress | Extract | Peak memory, compressing |
|---|---:|---:|---:|---:|---:|
| *Fast settings* | | | | | |
| Finder Compress / Archive Utility | 93.32 MB | 95% | 3.11 s | 1.23 s | – |
| tar + zstd -3 | 91.72 MB | 94% | 0.24 s | 0.25 s | 118 MB |
| WinRAR Fastest (-m1) | 92.51 MB | 94% | 1.05 s | 0.30 s | 104 MB |
| **Zipling Fast** | 89.08 MB | 91% | 0.05 s | 0.08 s | 1374 MB |
| *Normal settings* | | | | | |
| 7-Zip Normal (-mx5) | 85.29 MB | 87% | 6.01 s | 1.59 s | 381 MB |
| WinRAR Normal (-m3) | 90.93 MB | 93% | 1.12 s | 0.36 s | 131 MB |
| tar + xz -6 | 89.04 MB | 91% | 8.22 s | 0.65 s | 554 MB |
| **Zipling Default** | 69.18 MB | 71% | 1.98 s | 0.66 s | 2270 MB |
| *Strongest settings* | | | | | |
| 7-Zip Ultra (-mx9) | 85.29 MB | 87% | 6.40 s | 1.58 s | 983 MB |
| WinRAR Best (-m5, solid) | 86.21 MB | 88% | 2.89 s | 0.38 s | 571 MB |
| tar + zstd -19 --long | 85.70 MB | 87% | 14.5 s | 0.25 s | 409 MB |
| tar + xz -9e | 85.60 MB | 87% | 38.5 s | 1.99 s | 850 MB |
| **Zipling Max** | 69.17 MB | 71% | 2.05 s | 0.68 s | 2283 MB |
| **Zipling Ultra*** | 69.17 MB | 71% | 2.15 s | 0.70 s | 2201 MB |

### Mac apps (174 MB)

8 apps from `/System/Applications`: universal Mach-O, asset catalogs, nibs, strings. Software and `.app` bundles.

| Tool | Size | % of original | Compress | Extract | Peak memory, compressing |
|---|---:|---:|---:|---:|---:|
| *Fast settings* | | | | | |
| Finder Compress / Archive Utility | 87.42 MB | 50% | 6.66 s | 2.42 s | – |
| tar + zstd -3 | 76.47 MB | 44% | 1.06 s | 1.14 s | 146 MB |
| WinRAR Fastest (-m1) | 78.68 MB | 45% | 1.24 s | 0.70 s | 272 MB |
| **Zipling Fast** | 72.30 MB | 42% | 0.13 s | 0.23 s | 1082 MB |
| *Normal settings* | | | | | |
| 7-Zip Normal (-mx5) | 55.26 MB | 32% | 16.8 s | 1.41 s | 878 MB |
| WinRAR Normal (-m3) | 64.68 MB | 37% | 2.51 s | 0.76 s | 300 MB |
| tar + xz -6 | 58.97 MB | 34% | 8.47 s | 1.26 s | 938 MB |
| **Zipling Default** | 56.44 MB | 32% | 1.39 s | 0.26 s | 973 MB |
| *Strongest settings* | | | | | |
| 7-Zip Ultra (-mx9) | 54.10 MB | 31% | 27.7 s | 1.66 s | 1759 MB |
| WinRAR Best (-m5, solid) | 58.69 MB | 34% | 5.40 s | 0.70 s | 574 MB |
| tar + zstd -19 --long | 59.36 MB | 34% | 21.3 s | 1.15 s | 547 MB |
| tar + xz -9e | 54.33 MB | 31% | 57.9 s | 2.40 s | 899 MB |
| **Zipling Max** | 49.15 MB | 28% | 12.1 s | 0.70 s | 1962 MB |
| **Zipling Ultra*** | 48.55 MB | 28% | 14.5 s | 0.71 s | 1425 MB |

### Source code (162 MB)

89 Rust crates, including vendored C (zstd, xz). Code repositories.

| Tool | Size | % of original | Compress | Extract | Peak memory, compressing |
|---|---:|---:|---:|---:|---:|
| *Fast settings* | | | | | |
| Finder Compress / Archive Utility | 25.11 MB | 16% | 6.54 s | 5.16 s | – |
| tar + zstd -3 | 15.09 MB | 9% | 3.72 s | 3.53 s | 111 MB |
| WinRAR Fastest (-m1) | 21.82 MB | 13% | 1.72 s | 1.42 s | 47 MB |
| **Zipling Fast** | 13.55 MB | 8% | 0.25 s | 0.52 s | 577 MB |
| *Normal settings* | | | | | |
| 7-Zip Normal (-mx5) | 9.15 MB | 6% | 12.9 s | 1.14 s | 794 MB |
| WinRAR Normal (-m3) | 18.15 MB | 11% | 2.38 s | 1.46 s | 59 MB |
| tar + xz -6 | 9.96 MB | 6% | 6.22 s | 3.48 s | 932 MB |
| **Zipling Default** | 9.46 MB | 6% | 1.80 s | 0.61 s | 1101 MB |
| *Strongest settings* | | | | | |
| 7-Zip Ultra (-mx9) | 8.51 MB | 5% | 21.9 s | 1.16 s | 1676 MB |
| WinRAR Best (-m5, solid) | 10.15 MB | 6% | 4.48 s | 1.23 s | 551 MB |
| tar + zstd -19 --long | 9.29 MB | 6% | 25.8 s | 3.50 s | 508 MB |
| tar + xz -9e | 8.35 MB | 5% | 66.1 s | 3.48 s | 861 MB |
| **Zipling Max** | 8.31 MB | 5% | 1.55 s | 0.83 s | 916 MB |
| **Zipling Ultra*** | 8.09 MB | 5% | 1.70 s | 1.01 s | 897 MB |

### Generic data (212 MB)

The standard Silesia compression corpus. Data with no file-type tricks to exploit.

| Tool | Size | % of original | Compress | Extract | Peak memory, compressing |
|---|---:|---:|---:|---:|---:|
| *Fast settings* | | | | | |
| Finder Compress / Archive Utility | 68.23 MB | 32% | 6.21 s | 1.32 s | – |
| tar + zstd -3 | 66.25 MB | 31% | 0.15 s | 0.21 s | 153 MB |
| WinRAR Fastest (-m1) | 66.68 MB | 31% | 0.92 s | 0.41 s | 270 MB |
| **Zipling Fast** | 65.18 MB | 31% | 0.17 s | 0.11 s | 874 MB |
| *Normal settings* | | | | | |
| 7-Zip Normal (-mx5) | 49.60 MB | 23% | 22.9 s | 0.98 s | 904 MB |
| WinRAR Normal (-m3) | 54.21 MB | 26% | 3.64 s | 0.45 s | 297 MB |
| tar + xz -6 | 49.50 MB | 23% | 13.2 s | 0.51 s | 1088 MB |
| **Zipling Default** | 57.26 MB | 27% | 4.95 s | 0.15 s | 1407 MB |
| *Strongest settings* | | | | | |
| 7-Zip Ultra (-mx9) | 48.69 MB | 23% | 48.2 s | 1.56 s | 2084 MB |
| WinRAR Best (-m5, solid) | 53.12 MB | 25% | 6.36 s | 0.45 s | 572 MB |
| tar + zstd -19 --long | 52.60 MB | 25% | 30.1 s | 0.23 s | 656 MB |
| tar + xz -9e | 48.42 MB | 23% | 83.8 s | 1.99 s | 1080 MB |
| **Zipling Max** | 46.68 MB | 22% | 6.22 s | 0.59 s | 2104 MB |
| **Zipling Ultra*** | 45.92 MB | 22% | 13.5 s | 0.84 s | 1302 MB |
