# Zipling app benchmark, 2026-10-31

## Scoreboard (phases 1 and 2: Zipgeist and Finder; the Zipling side is provisional until phase 3)

Each grid is one Zipling level against the rival settings in its class. A row is won
only when Zipling is smaller **and** faster to make **and** faster to extract; where it is
not, the cell says which of the three failed and by how much.

### Zipling Fast

| Set | Zipgeist ZIP Fast | Zipgeist tar.zst Fast | Finder Compress |
|---|---|---|---|
| Photos | ✅ | ✅ | ✅ |
| PDFs | ✅ | ✅ | ✅ |
| Source code | ✅ | ✅ | ✅ |
| Apple apps | ✅ | ✅ | ✅ |
| Silesia | ✅ | ❌ 1.2x slower to make | ✅ |
| Saved folder | ✅ | ✅ | ✅ |
| Mixed folder | ✅ | ✅ | ✅ |
| Disk images | ✅ | ✅ | ✅ |
| 3rd-party apps | ✅ | ✅ | ✅ |

**Fast: 26 of 27 won.**

### Zipling Default

| Set | Zipgeist ZIP Normal | Zipgeist 7z Normal |
|---|---|---|
| Photos | ❌ 26x slower to make, 1.3x slower to extract | ✅ |
| PDFs | ❌ 10x slower to make | ✅ |
| Source code | ❌ 4x slower to make | ❌ 7% bigger |
| Apple apps | ❌ 1.5x slower to make | ✅ |
| Silesia | ❌ 4x slower to make | ❌ 15% bigger |
| Saved folder | ❌ 4x slower to make | ✅ |
| Mixed folder | ❌ 3x slower to make | ✅ |
| Disk images | ❌ 5x slower to make | ✅ |
| 3rd-party apps | ❌ 1.0x slower to make | ❌ 6% bigger |

**Default: 6 of 18 won.**

### Zipling Max

| Set | Zipgeist 7z Best | Zipgeist tar.xz Best |
|---|---|---|
| Photos | ✅ | ✅ |
| PDFs | ❌ 6x slower to make | ❌ 6x slower to make |
| Source code | ✅ | ✅ |
| Apple apps | ❌ 2.0x slower to make | ❌ 1.9x slower to make |
| Silesia | ✅ | ✅ |
| Saved folder | ✅ | ✅ |
| Mixed folder | ✅ | ✅ |
| Disk images | ❌ 5x slower to make, 2.9x slower to extract | ❌ 5x slower to make, 2.5x slower to extract |
| 3rd-party apps | ✅ | ✅ |

**Max: 12 of 18 won.**

### Zipling Ultra

| Set | Zipgeist 7z Best | Zipgeist tar.xz Best |
|---|---|---|
| Photos | ✅ | ✅ |
| PDFs | ❌ 8x slower to make | ❌ 9x slower to make |
| Source code | ✅ | ✅ |
| Apple apps | ❌ 2.4x slower to make | ❌ 2.3x slower to make |
| Silesia | ✅ | ✅ |
| Saved folder | ❌ 1.3x slower to make | ❌ 1.2x slower to make |
| Mixed folder | ❌ 1.0x slower to make | ❌ 1.1x slower to make |
| Disk images | ❌ 7x slower to make, 2.8x slower to extract | ❌ 7x slower to make, 2.4x slower to extract |
| 3rd-party apps | ❌ 1.1x slower to make | ❌ 1.1x slower to make |

**Ultra: 6 of 18 won.**

**All levels: 50 of 81 rows won.** A row counts only when all three are better at once; Zipling is the smaller file in 78 of 81 rows and the faster extractor in 76 of 81.


---

# Details
Status: **phases 1 and 2 done, phase 3 pending.** Phase 1 drove Zipgeist 1.0.0 through its
own Finder menu on all nine sets, six settings, two rounds: 108 of 108 round trips
identical. Phase 2 drove Finder's own Compress and the double-click Expand (Archive
Utility): 18 of 18 identical. Phase 3 (the Zipling app at Fast, Default, Max and Ultra
alternating with 7-Zip 26.05, WinRAR 7.23, zstd 1.5.6 and xz 5.6.4, 252 runs, unattended)
runs tonight; until then the Zipling side of every comparison below is **this morning's
engine run of the same locked presets** (`results-2026-10-31-presets.jsonl`) and is marked
provisional. The app adds 2 to 7 ms to a pack and about 30 ms to an extract, so the final
numbers will move little.

Machine: MacBook Pro, Apple M1 Pro (8 performance + 2 efficiency cores), 32 GB, internal
SSD, macOS 27.0.1. Method, versions and set descriptions:
`results-2026-10-31-apps-machine.json`. Raw rows: `results-2026-10-31-apps-zipgeist.jsonl`
and `results-2026-10-31-apps-finder.jsonl`.

How the rivals were timed. Zipgeist: from the keypress on its Compress button (or the
click on its Extract menu item) to the last busy moment of its processes, with the
archive's write time as a floor; six settings (ZIP Fast, ZIP Normal, tar.zst Fast, 7z
Normal, 7z Best, tar.xz Best). Finder: from the click on Compress to the finished
archive landing (Finder writes a zero-byte placeholder and drops the file in at the end),
and from the double-click to Archive Utility's last busy moment. Both ran under the
screen-control overlay, which keeps about one core busy; a Zipling Fast run through the
app in the same session shows the effect (table at the end).

A row is **won** only when Zipling is smaller, faster to make and faster to extract at
once, level against level: Fast against the fast settings, Default against the normal
ones, Max and Ultra against the best.

## Tally, 1:1 by level (Zipling side provisional)

| Level | Rows won | Lost on |
|---|---:|---|
| Fast | 26 of 27 | Silesia vs Zipgeist tar.zst Fast, make time only (0.16 vs 0.13 s; Fast is 11% smaller and extracts 7x faster) |
| Default | 6 of 18 | ZIP Normal on all 9 sets, make time only; 7z Normal on source, Silesia and third-party apps, size only |
| Max | 12 of 18 | make time on PDFs, apps and DMGs (and extract on DMGs), where the transforms buy 17 to 47% of size |
| Ultra | 6 of 18 | make time on PDFs, apps, Saved, mixed, DMGs, third-party apps |

Against Zipgeist alone, any Zipling level allowed against each of its results, the engine
run wins 53 of 54 (the one left is the Silesia tar.zst Fast make time).

## Upsets: a lower level beating a higher rival setting on all three

- **Fast beats Zipgeist ZIP Normal** on 9 of 9 sets: Photos, PDFs and screenshots, Source code, Apple apps, Silesia, Saved folder, Website mixed folder, Disk images, Third-party apps.
- **Default beats Zipgeist 7z Best** on 6 of 9 sets: Photos, PDFs and screenshots, Apple apps, Saved folder, Website mixed folder, Disk images.
- **Default beats Zipgeist tar.xz Best** on 6 of 9 sets: Photos, PDFs and screenshots, Apple apps, Saved folder, Website mixed folder, Disk images.
- **Fast beats Finder's Compress** on all 9 sets: 5 to 46% smaller, 25 to 70x faster to make, 11 to 25x faster to extract.

## Finder's own Compress and Expand against Zipling Fast

| Set | Finder size | Finder make | Expand | Fast size | Fast make | Fast extract |
|---|---:|---:|---:|---:|---:|---:|
| Photos | 93.32 MB | 3.11 s | 1.23 s | 89.08 MB | 0.05 s | 0.05 s |
| PDFs and screenshots | 50.24 MB | 2.03 s | 1.25 s | 41.06 MB | 0.06 s | 0.07 s |
| Source code | 25.11 MB | 6.54 s | 5.16 s | 13.55 MB | 0.25 s | 0.37 s |
| Apple apps | 87.42 MB | 6.66 s | 2.42 s | 72.30 MB | 0.14 s | 0.15 s |
| Silesia | 68.23 MB | 6.21 s | 1.32 s | 65.18 MB | 0.16 s | 0.06 s |
| Saved folder | 356.08 MB | 22.12 s | 9.06 s | 311.18 MB | 0.49 s | 0.70 s |
| Website mixed folder | 324.62 MB | 24.11 s | 7.90 s | 281.11 MB | 0.52 s | 0.60 s |
| Disk images | 617.31 MB | 12.61 s | 1.88 s | 615.46 MB | 0.19 s | 0.13 s |
| Third-party apps | 536.15 MB | 40.84 s | 4.43 s | 504.36 MB | 0.57 s | 0.40 s |

## Zipgeist, set by set

**Photos (95 MB, 400 JPEGs)** (Zipling: engine run of this morning, provisional)

| Tool | Size | Make | Extract |
|---|---:|---:|---:|
| **Zipling Fast** | 89.08 MB | 0.05 s | 0.05 s |
| Finder Compress / Archive Utility | 93.32 MB | 3.11 s | 1.23 s |
| Zipgeist ZIP Fast | 98.47 MB | 0.08 s | 0.55 s |
| Zipgeist tar.zst Fast | 92.06 MB | 0.10 s | 0.48 s |
| **Zipling Default** | 69.18 MB | 2.05 s | 0.69 s |
| Zipgeist ZIP Normal | 98.47 MB | 0.08 s | 0.52 s |
| Zipgeist 7z Normal | 86.04 MB | 2.66 s | 1.75 s |
| **Zipling Max** | 69.17 MB | 2.12 s | 0.68 s |
| **Zipling Ultra** | 69.17 MB | 2.23 s | 0.69 s |
| Zipgeist 7z Best | 86.03 MB | 2.68 s | 1.74 s |
| Zipgeist tar.xz Best | 86.63 MB | 3.06 s | 1.03 s |

**PDFs and screenshots (89 MB)** (Zipling: engine run of this morning, provisional)

| Tool | Size | Make | Extract |
|---|---:|---:|---:|
| **Zipling Fast** | 41.06 MB | 0.06 s | 0.07 s |
| Finder Compress / Archive Utility | 50.24 MB | 2.03 s | 1.25 s |
| Zipgeist ZIP Fast | 88.08 MB | 0.11 s | 0.58 s |
| Zipgeist tar.zst Fast | 45.24 MB | 0.11 s | 0.50 s |
| **Zipling Default** | 22.14 MB | 1.34 s | 0.23 s |
| Zipgeist ZIP Normal | 87.89 MB | 0.13 s | 0.58 s |
| Zipgeist 7z Normal | 38.71 MB | 1.72 s | 1.75 s |
| **Zipling Max** | 18.20 MB | 10.77 s | 0.41 s |
| **Zipling Ultra** | 17.81 MB | 15.37 s | 0.41 s |
| Zipgeist 7z Best | 38.67 MB | 1.91 s | 1.73 s |
| Zipgeist tar.xz Best | 38.86 MB | 1.77 s | 1.04 s |

**Source code (172 MB)** (Zipling: engine run of this morning, provisional)

| Tool | Size | Make | Extract |
|---|---:|---:|---:|
| **Zipling Fast** | 13.55 MB | 0.25 s | 0.37 s |
| Finder Compress / Archive Utility | 25.11 MB | 6.54 s | 5.16 s |
| Zipgeist ZIP Fast | 24.05 MB | 0.45 s | 0.93 s |
| Zipgeist tar.zst Fast | 16.99 MB | 0.51 s | 0.88 s |
| **Zipling Default** | 11.20 MB | 1.99 s | 0.36 s |
| Zipgeist ZIP Normal | 21.20 MB | 0.48 s | 0.90 s |
| Zipgeist 7z Normal | 10.47 MB | 5.42 s | 1.05 s |
| **Zipling Max** | 8.31 MB | 1.61 s | 0.68 s |
| **Zipling Ultra** | 8.09 MB | 1.74 s | 0.92 s |
| Zipgeist 7z Best | 10.09 MB | 6.57 s | 1.03 s |
| Zipgeist tar.xz Best | 9.08 MB | 5.99 s | 1.02 s |

**Apple apps (170 MB)** (Zipling: engine run of this morning, provisional)

| Tool | Size | Make | Extract |
|---|---:|---:|---:|
| **Zipling Fast** | 72.30 MB | 0.14 s | 0.15 s |
| Finder Compress / Archive Utility | 87.42 MB | 6.66 s | 2.42 s |
| Zipgeist ZIP Fast | 91.82 MB | 0.49 s | 0.78 s |
| Zipgeist tar.zst Fast | 86.23 MB | 0.29 s | 0.71 s |
| **Zipling Default** | 56.44 MB | 1.43 s | 0.21 s |
| Zipgeist ZIP Normal | 86.33 MB | 0.98 s | 0.77 s |
| Zipgeist 7z Normal | 59.58 MB | 5.79 s | 0.97 s |
| **Zipling Max** | 49.15 MB | 12.46 s | 0.64 s |
| **Zipling Ultra** | 48.55 MB | 15.04 s | 0.63 s |
| Zipgeist 7z Best | 59.30 MB | 6.16 s | 0.95 s |
| Zipgeist tar.xz Best | 58.66 MB | 6.53 s | 1.21 s |

**Silesia corpus (203 MB)** (Zipling: engine run of this morning, provisional)

| Tool | Size | Make | Extract |
|---|---:|---:|---:|
| **Zipling Fast** | 65.18 MB | 0.16 s | 0.06 s |
| Finder Compress / Archive Utility | 68.23 MB | 6.21 s | 1.32 s |
| Zipgeist ZIP Fast | 77.28 MB | 0.39 s | 0.70 s |
| Zipgeist tar.zst Fast | 73.31 MB | 0.13 s | 0.64 s |
| **Zipling Default** | 57.26 MB | 4.73 s | 0.10 s |
| Zipgeist ZIP Normal | 68.53 MB | 1.12 s | 0.67 s |
| Zipgeist 7z Normal | 49.95 MB | 16.26 s | 0.81 s |
| **Zipling Max** | 46.68 MB | 6.14 s | 0.57 s |
| **Zipling Ultra** | 45.92 MB | 13.63 s | 0.81 s |
| Zipgeist 7z Best | 49.15 MB | 19.73 s | 0.89 s |
| Zipgeist tar.xz Best | 49.16 MB | 19.78 s | 1.10 s |

**Saved folder (533 MB)** (Zipling: engine run of this morning, provisional)

| Tool | Size | Make | Extract |
|---|---:|---:|---:|
| **Zipling Fast** | 311.18 MB | 0.49 s | 0.70 s |
| Finder Compress / Archive Utility | 356.08 MB | 22.12 s | 9.06 s |
| Zipgeist ZIP Fast | 393.09 MB | 1.75 s | 1.80 s |
| Zipgeist tar.zst Fast | 340.98 MB | 2.17 s | 1.48 s |
| **Zipling Default** | 248.82 MB | 8.11 s | 1.52 s |
| Zipgeist ZIP Normal | 381.77 MB | 2.26 s | 1.79 s |
| Zipgeist 7z Normal | 285.18 MB | 15.11 s | 3.29 s |
| **Zipling Max** | 221.74 MB | 16.53 s | 2.49 s |
| **Zipling Ultra** | 218.57 MB | 22.06 s | 2.58 s |
| Zipgeist 7z Best | 284.23 MB | 17.53 s | 3.28 s |
| Zipgeist tar.xz Best | 284.85 MB | 17.90 s | 2.95 s |

**Website mixed folder (726 MB)** (Zipling: engine run of this morning, provisional)

| Tool | Size | Make | Extract |
|---|---:|---:|---:|
| **Zipling Fast** | 281.11 MB | 0.52 s | 0.60 s |
| Finder Compress / Archive Utility | 324.62 MB | 24.11 s | 7.90 s |
| Zipgeist ZIP Fast | 379.85 MB | 1.54 s | 1.68 s |
| Zipgeist tar.zst Fast | 313.80 MB | 2.04 s | 1.46 s |
| **Zipling Default** | 215.97 MB | 8.54 s | 1.28 s |
| Zipgeist ZIP Normal | 362.57 MB | 2.45 s | 1.59 s |
| Zipgeist 7z Normal | 245.36 MB | 25.49 s | 3.32 s |
| **Zipling Max** | 192.57 MB | 19.11 s | 2.37 s |
| **Zipling Ultra** | 189.26 MB | 29.40 s | 2.43 s |
| Zipgeist 7z Best | 243.86 MB | 28.93 s | 3.28 s |
| Zipgeist tar.xz Best | 243.87 MB | 26.87 s | 2.90 s |

**Disk images (592 MB)** (Zipling: engine run of this morning, provisional)

| Tool | Size | Make | Extract |
|---|---:|---:|---:|
| **Zipling Fast** | 615.46 MB | 0.19 s | 0.13 s |
| Finder Compress / Archive Utility | 617.31 MB | 12.61 s | 1.88 s |
| Zipgeist ZIP Fast | 620.15 MB | 0.23 s | 0.70 s |
| Zipgeist tar.zst Fast | 617.58 MB | 0.33 s | 0.64 s |
| **Zipling Default** | 601.83 MB | 1.05 s | 0.35 s |
| Zipgeist ZIP Normal | 620.15 MB | 0.23 s | 0.75 s |
| Zipgeist 7z Normal | 611.90 MB | 8.09 s | 1.51 s |
| **Zipling Max** | 454.96 MB | 41.10 s | 4.26 s |
| **Zipling Ultra** | 447.96 MB | 55.41 s | 4.19 s |
| Zipgeist 7z Best | 611.88 MB | 8.21 s | 1.48 s |
| Zipgeist tar.xz Best | 611.88 MB | 8.17 s | 1.72 s |

**Third-party apps (874 MB)** (Zipling: engine run of this morning, provisional)

| Tool | Size | Make | Extract |
|---|---:|---:|---:|
| **Zipling Fast** | 504.36 MB | 0.57 s | 0.40 s |
| Finder Compress / Archive Utility | 536.15 MB | 40.84 s | 4.43 s |
| Zipgeist ZIP Fast | 582.14 MB | 2.90 s | 2.22 s |
| Zipgeist tar.zst Fast | 569.63 MB | 0.62 s | 1.76 s |
| **Zipling Default** | 402.77 MB | 7.15 s | 0.49 s |
| Zipgeist ZIP Normal | 540.46 MB | 7.08 s | 2.13 s |
| Zipgeist 7z Normal | 380.03 MB | 51.04 s | 3.96 s |
| **Zipling Max** | 350.25 MB | 42.33 s | 1.74 s |
| **Zipling Ultra** | 336.19 MB | 61.58 s | 1.85 s |
| Zipgeist 7z Best | 377.94 MB | 55.48 s | 4.75 s |
| Zipgeist tar.xz Best | 377.17 MB | 54.50 s | 4.24 s |



## Wins and losses, row by row

```
== jpeg (Zipling engine run of this morning, provisional)
  Fast     vs Zipgeist ZIP Fast      size  0.90x  make  0.68x  extract  0.09x  WON
  Fast     vs Zipgeist tar.zst Fast  size  0.97x  make  0.55x  extract  0.10x  WON
  Fast     vs Finder Compress      size  0.95x  make  0.02x  extract  0.04x  WON
  Default  vs Zipgeist ZIP Normal    size  0.70x  make 25.65x  extract  1.33x  lost on make, extract
  Default  vs Zipgeist 7z Normal     size  0.80x  make  0.77x  extract  0.39x  WON
  Max      vs Zipgeist 7z Best       size  0.80x  make  0.79x  extract  0.39x  WON
  Max      vs Zipgeist tar.xz Best   size  0.80x  make  0.69x  extract  0.66x  WON
  Ultra    vs Zipgeist 7z Best       size  0.80x  make  0.83x  extract  0.40x  WON
  Ultra    vs Zipgeist tar.xz Best   size  0.80x  make  0.73x  extract  0.67x  WON
== docs (Zipling engine run of this morning, provisional)
  Fast     vs Zipgeist ZIP Fast      size  0.47x  make  0.58x  extract  0.12x  WON
  Fast     vs Zipgeist tar.zst Fast  size  0.91x  make  0.58x  extract  0.14x  WON
  Fast     vs Finder Compress      size  0.82x  make  0.03x  extract  0.06x  WON
  Default  vs Zipgeist ZIP Normal    size  0.25x  make 10.32x  extract  0.39x  lost on make
  Default  vs Zipgeist 7z Normal     size  0.57x  make  0.78x  extract  0.13x  WON
  Max      vs Zipgeist 7z Best       size  0.47x  make  5.64x  extract  0.24x  lost on make
  Max      vs Zipgeist tar.xz Best   size  0.47x  make  6.08x  extract  0.39x  lost on make
  Ultra    vs Zipgeist 7z Best       size  0.46x  make  8.05x  extract  0.24x  lost on make
  Ultra    vs Zipgeist tar.xz Best   size  0.46x  make  8.68x  extract  0.39x  lost on make
== source (Zipling engine run of this morning, provisional)
  Fast     vs Zipgeist ZIP Fast      size  0.56x  make  0.55x  extract  0.39x  WON
  Fast     vs Zipgeist tar.zst Fast  size  0.80x  make  0.48x  extract  0.42x  WON
  Fast     vs Finder Compress      size  0.54x  make  0.04x  extract  0.07x  WON
  Default  vs Zipgeist ZIP Normal    size  0.53x  make  4.14x  extract  0.40x  lost on make
  Default  vs Zipgeist 7z Normal     size  1.07x  make  0.37x  extract  0.34x  lost on size
  Max      vs Zipgeist 7z Best       size  0.82x  make  0.24x  extract  0.66x  WON
  Max      vs Zipgeist tar.xz Best   size  0.91x  make  0.27x  extract  0.67x  WON
  Ultra    vs Zipgeist 7z Best       size  0.80x  make  0.27x  extract  0.89x  WON
  Ultra    vs Zipgeist tar.xz Best   size  0.89x  make  0.29x  extract  0.90x  WON
== apps (Zipling engine run of this morning, provisional)
  Fast     vs Zipgeist ZIP Fast      size  0.79x  make  0.28x  extract  0.19x  WON
  Fast     vs Zipgeist tar.zst Fast  size  0.84x  make  0.47x  extract  0.21x  WON
  Fast     vs Finder Compress      size  0.83x  make  0.02x  extract  0.06x  WON
  Default  vs Zipgeist ZIP Normal    size  0.65x  make  1.45x  extract  0.27x  lost on make
  Default  vs Zipgeist 7z Normal     size  0.95x  make  0.25x  extract  0.21x  WON
  Max      vs Zipgeist 7z Best       size  0.83x  make  2.02x  extract  0.68x  lost on make
  Max      vs Zipgeist tar.xz Best   size  0.84x  make  1.91x  extract  0.53x  lost on make
  Ultra    vs Zipgeist 7z Best       size  0.82x  make  2.44x  extract  0.66x  lost on make
  Ultra    vs Zipgeist tar.xz Best   size  0.83x  make  2.30x  extract  0.52x  lost on make
== silesia (Zipling engine run of this morning, provisional)
  Fast     vs Zipgeist ZIP Fast      size  0.84x  make  0.41x  extract  0.08x  WON
  Fast     vs Zipgeist tar.zst Fast  size  0.89x  make  1.23x  extract  0.09x  lost on make
  Fast     vs Finder Compress      size  0.96x  make  0.03x  extract  0.04x  WON
  Default  vs Zipgeist ZIP Normal    size  0.84x  make  4.22x  extract  0.14x  lost on make
  Default  vs Zipgeist 7z Normal     size  1.15x  make  0.29x  extract  0.12x  lost on size
  Max      vs Zipgeist 7z Best       size  0.95x  make  0.31x  extract  0.64x  WON
  Max      vs Zipgeist tar.xz Best   size  0.95x  make  0.31x  extract  0.51x  WON
  Ultra    vs Zipgeist 7z Best       size  0.93x  make  0.69x  extract  0.91x  WON
  Ultra    vs Zipgeist tar.xz Best   size  0.93x  make  0.69x  extract  0.74x  WON
== saved (Zipling engine run of this morning, provisional)
  Fast     vs Zipgeist ZIP Fast      size  0.79x  make  0.28x  extract  0.39x  WON
  Fast     vs Zipgeist tar.zst Fast  size  0.91x  make  0.23x  extract  0.47x  WON
  Fast     vs Finder Compress      size  0.87x  make  0.02x  extract  0.08x  WON
  Default  vs Zipgeist ZIP Normal    size  0.65x  make  3.59x  extract  0.85x  lost on make
  Default  vs Zipgeist 7z Normal     size  0.87x  make  0.54x  extract  0.46x  WON
  Max      vs Zipgeist 7z Best       size  0.78x  make  0.94x  extract  0.76x  WON
  Max      vs Zipgeist tar.xz Best   size  0.78x  make  0.92x  extract  0.84x  WON
  Ultra    vs Zipgeist 7z Best       size  0.77x  make  1.26x  extract  0.79x  lost on make
  Ultra    vs Zipgeist tar.xz Best   size  0.77x  make  1.23x  extract  0.88x  lost on make
== mixed (Zipling engine run of this morning, provisional)
  Fast     vs Zipgeist ZIP Fast      size  0.74x  make  0.34x  extract  0.36x  WON
  Fast     vs Zipgeist tar.zst Fast  size  0.90x  make  0.26x  extract  0.41x  WON
  Fast     vs Finder Compress      size  0.87x  make  0.02x  extract  0.08x  WON
  Default  vs Zipgeist ZIP Normal    size  0.60x  make  3.49x  extract  0.81x  lost on make
  Default  vs Zipgeist 7z Normal     size  0.88x  make  0.34x  extract  0.39x  WON
  Max      vs Zipgeist 7z Best       size  0.79x  make  0.66x  extract  0.72x  WON
  Max      vs Zipgeist tar.xz Best   size  0.79x  make  0.71x  extract  0.82x  WON
  Ultra    vs Zipgeist 7z Best       size  0.78x  make  1.02x  extract  0.74x  lost on make
  Ultra    vs Zipgeist tar.xz Best   size  0.78x  make  1.09x  extract  0.84x  lost on make
== dmgs (Zipling engine run of this morning, provisional)
  Fast     vs Zipgeist ZIP Fast      size  0.99x  make  0.84x  extract  0.18x  WON
  Fast     vs Zipgeist tar.zst Fast  size  1.00x  make  0.59x  extract  0.20x  WON
  Fast     vs Finder Compress      size  1.00x  make  0.02x  extract  0.07x  WON
  Default  vs Zipgeist ZIP Normal    size  0.97x  make  4.57x  extract  0.46x  lost on make
  Default  vs Zipgeist 7z Normal     size  0.98x  make  0.13x  extract  0.23x  WON
  Max      vs Zipgeist 7z Best       size  0.74x  make  5.01x  extract  2.88x  lost on make, extract
  Max      vs Zipgeist tar.xz Best   size  0.74x  make  5.03x  extract  2.47x  lost on make, extract
  Ultra    vs Zipgeist 7z Best       size  0.73x  make  6.75x  extract  2.83x  lost on make, extract
  Ultra    vs Zipgeist tar.xz Best   size  0.73x  make  6.78x  extract  2.44x  lost on make, extract
== thirdparty (Zipling engine run of this morning, provisional)
  Fast     vs Zipgeist ZIP Fast      size  0.87x  make  0.20x  extract  0.18x  WON
  Fast     vs Zipgeist tar.zst Fast  size  0.89x  make  0.92x  extract  0.23x  WON
  Fast     vs Finder Compress      size  0.94x  make  0.01x  extract  0.09x  WON
  Default  vs Zipgeist ZIP Normal    size  0.75x  make  1.01x  extract  0.23x  lost on make
  Default  vs Zipgeist 7z Normal     size  1.06x  make  0.14x  extract  0.12x  lost on size
  Max      vs Zipgeist 7z Best       size  0.93x  make  0.76x  extract  0.37x  WON
  Max      vs Zipgeist tar.xz Best   size  0.93x  make  0.78x  extract  0.41x  WON
  Ultra    vs Zipgeist 7z Best       size  0.89x  make  1.11x  extract  0.39x  lost on make
  Ultra    vs Zipgeist tar.xz Best   size  0.89x  make  1.13x  extract  0.44x  lost on make
totals so far: Fast 26/27, Default 6/18, Max 12/18, Ultra 6/18
```

## Default: what is fixable and what is not

Default loses two kinds of rows, for two different reasons.

**1. Against Zipgeist ZIP Normal, on make time, all nine sets.** ZIP Normal is a
near-store Deflate: 0.08 s on the photos, 0.13 s on the PDFs, 2.3 s on the Saved folder.
Default makes files 16 to 75% smaller than it and extracts faster on eight sets, but
takes 1.5x (apps) to 25x (photos) longer, because it runs the transforms. No preset that
compresses can beat a near-store ZIP on make time; the only Zipling setting that does is
Fast, and Fast already beats ZIP Normal on all nine sets on all three measures (an upset
above). So these nine rows are not a Default problem to fix: they are the reason Fast
exists. If the website compares level against level, these rows stay red; if it compares
Zipgeist's default button with the Zipling button that answers it, they are nine green
rows for Fast.

**2. Against Zipgeist 7z Normal, on size, on source code, Silesia and third-party apps.**
7z Normal is LZMA; Default's lanes are zstd 15 and 12 with all transforms. Default is 3
to 8x faster to make and 3 to 8x faster to extract on every set, and smaller on six of
nine; on the three code- and text-heavy sets it is 7%, 15% and 6% bigger. This is the
fixable part, and the measured options are:

| Option | Source code | Silesia | Third-party apps | Cost |
|---|---|---|---|---|
| Default as is | 11.20 MB, 1.95 s | 57.26 MB, 4.73 s | 402.98 MB, 6.68 s | — |
| Default with bsc on the text lane (measured 2026-10-30) | 10.21 MB, 0.95 s: **wins** the row | 53.07 MB, 2.32 s: still 6% bigger | 402.61 MB: still 6% bigger | extract 1.3 to 4x slower on text-heavy sets (still 2x faster than 7z Normal); DMGs make 1.05 to 5.1 s |
| Max's codecs at Default (bsc text, LZMA2 7 binaries) | 8.23 MB, 1.61 s: wins | 49.41 MB, 6.14 s: wins | 350.23 MB, 39 s: wins | make 2 to 40x slower than today's Default; loses the 7z Normal rows it wins today on PDFs, apps, Saved, DMGs (make time) |
| The lzt codec (LZMA's parse at a symbol coder's decode speed, research stage) | within 3 to 5% of LZMA at 2x its decode speed, measured on Silesia | | | needs a format change and a quiet month; the only route to LZMA's size at Default's extract speed |

Recommendation: keep Default's codecs, turn on bsc for the text lane only if the extract
cost on text sets is acceptable (it takes the source-code row and 7% off Silesia), and
present Default as the middle it is: smaller than every normal setting on six of nine
sets, faster than the only one that beats it on size on the other three. The rows Default
loses on make time are Fast's rows; the website should show them as such.

## The session's clock: Zipling Fast through the app under the screen-control overlay

| Set | App make, this session | Engine make, quiet | App extract, this session | Engine extract, quiet |
|---|---:|---:|---:|---:|
| Photos | 0.06 s | 0.05 s | 0.07 s | 0.05 s |
| PDFs and screenshots | 0.07 s | 0.06 s | 0.12 s | 0.07 s |
| Source code | 0.28 s | 0.25 s | 0.56 s | 0.37 s |
| Apple apps | 0.14 s | 0.14 s | 0.21 s | 0.15 s |
| Silesia | 0.20 s | 0.16 s | 0.10 s | 0.06 s |
| Saved folder | 0.55 s | 0.49 s | 1.10 s | 0.70 s |
| Website mixed folder | 0.59 s | 0.52 s | 0.89 s | 0.60 s |
| Disk images | 0.22 s | 0.19 s | 0.30 s | 0.13 s |
| Third-party apps | 0.62 s | 0.57 s | 0.70 s | 0.40 s |

The app's own clock (from the AppleScript command's arrival to the finished file) under
the overlay runs 0 to 50% slower than the quiet engine run on the quick sets; phase 3
runs without the overlay.
