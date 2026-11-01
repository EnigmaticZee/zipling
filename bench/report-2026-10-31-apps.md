# Zipling app benchmark, 2026-10-31: the app against the rival apps and tools

Status: **all three phases done.** Phase 1 drove Zipgeist 1.0.0 through its own Finder menu on
all nine sets, six settings, two rounds: 108 of 108 round trips identical. Phase 2 drove Finder's
own Compress and the double-click Expand (Archive Utility): 18 of 18 identical. Phase 3 (the night
of 31 October to 1 November) drove the installed Zipling app through its AppleScript commands at
Fast, Default, Max and Ultra*, alternating with 7-Zip 26.05, WinRAR 7.23, zstd and xz as processes,
on the same nine sets, two rounds, unattended: 252 rows, no errors, every first-round extraction
identical to the source. Every Zipling number below is the **app's own clock**, from the command's
arrival in the app to the finished file; it includes the app's listing decode, its safety walk and
its main-actor hops, and it is what a user gets.

Machine: MacBook Pro, Apple M1 Pro (8 performance + 2 efficiency cores), 32 GB, internal SSD, macOS
27.0.1. Method, versions and set descriptions: `results-2026-10-31-apps-machine.json`. Raw rows:
`results-2026-10-31-apps.jsonl` (Zipling, 7-Zip, WinRAR, first-pass zstd and xz),
`results-2026-10-31-apps-zx.jsonl` (zstd and xz, second pass), `results-2026-10-31-apps-zipgeist.jsonl`,
`results-2026-10-31-apps-finder.jsonl`. Scoring: `bench/research/levels.py`.

zstd and xz were measured twice. The main run picked up zstd 1.5.6 and XZ Utils 5.6.4 from a conda environment that sat first in the harness's PATH; a second pass of the same four settings on all nine sets (72 rows, `results-2026-10-31-apps-zx.jsonl`, Homebrew zstd 1.5.7 and XZ Utils 5.8.4, the current releases) replaces those rows everywhere below. The first-pass rows stay in the raw file for the record.

How each tool was timed. Zipling: the app's timing log (`defaults write com.observantsystems.zipling
timing -bool true`), command arrived to file finished; the AppleScript round trip itself (50 to 60 ms
of osascript start-up and event delivery, measured before each set) is not charged to the app, and
the unsubtracted wall time is kept in the row (`make_s_raw`). Zipgeist: from the keypress on its
Compress button (or the click on its Extract menu item) to the last busy moment of its processes,
with the archive's write time as a floor. Finder: from the click on Compress to the finished archive
landing, and from the double-click to Archive Utility's last busy moment. Command-line tools: wall
time of the process (`/usr/bin/time -l`), `tar | zstd` and `tar | xz` as pipelines. Zipgeist and
Finder ran under the screen-control overlay, which kept about one core busy; Zipling and the
command-line tools ran with no screen activity at all.

A row is **won** only when Zipling is smaller, faster to make and faster to extract at once, level
against level: Fast against the fast settings (Zipgeist ZIP Fast and tar.zst Fast, Finder's
Compress, zstd -3, WinRAR Fastest), Default against the normal ones (Zipgeist ZIP Normal and 7z
Normal, 7-Zip Normal, WinRAR Normal, xz -6), Max and Ultra* against the best (Zipgeist 7z Best and
tar.xz Best, 7-Zip Ultra, WinRAR Best, zstd -19 --long, xz -9e). Ultra* is experimental and not in
the released app.

## Tally, 1:1 by level

| Level | Rows won | Lost on |
|---|---:|---|
| Fast | 42 of 45 | Zipgeist tar.zst Fast: make on Silesia · zstd -3: make on Silesia · WinRAR Fastest: size on Disk images |
| Default | 23 of 45 | Zipgeist ZIP Normal: make, extract on Photos; make on PDFs and screenshots, Source code, Apple apps, Silesia, Saved folder, Mixed folder, Disk images · Zipgeist 7z Normal: size on Source code, Silesia, Third-party apps · 7-Zip Normal: size on Source code, Apple apps, Silesia, Third-party apps · WinRAR Normal: make, extract on Photos; make on PDFs and screenshots; size, make on Silesia · xz -6: extract on Photos; size on Source code, Silesia, Third-party apps |
| Max | 36 of 54 | Zipgeist 7z Best: make on PDFs and screenshots, Apple apps; make, extract on Disk images · Zipgeist tar.xz Best: make on PDFs and screenshots, Apple apps; make, extract on Disk images · 7-Zip Ultra: make on PDFs and screenshots; make, extract on Disk images · WinRAR Best: extract on Photos, Silesia; make, extract on PDFs and screenshots, Disk images; make on Apple apps · zstd -19 --long: extract on Photos, Silesia; make, extract on PDFs and screenshots, Disk images · xz -9e: extract on Disk images |
| Ultra* | 28 of 54 | Zipgeist 7z Best: make on PDFs and screenshots, Apple apps, Saved folder, Third-party apps; make, extract on Disk images · Zipgeist tar.xz Best: make on PDFs and screenshots, Apple apps, Saved folder, Mixed folder, Third-party apps; make, extract on Disk images · 7-Zip Ultra: make on PDFs and screenshots; make, extract on Disk images · WinRAR Best: extract on Photos; make, extract on PDFs and screenshots, Apple apps, Silesia, Disk images; make on Saved folder, Mixed folder, Third-party apps · zstd -19 --long: extract on Photos, Silesia; make, extract on PDFs and screenshots, Disk images · xz -9e: extract on Disk images |

**All levels: 129 of 198.** Allowing any Zipling level against each rival
result, 135 of 144 results are beaten.

By rival family, level against level: Zipgeist Fast 17/18, Default 7/18, Max 12/18, Ultra* 7/18
(any level: 53 of 54); the command-line tools (7-Zip, WinRAR, zstd, xz)
Fast 16/18, Default 16/27, Max 24/36, Ultra* 21/36 (any level: 73 of 81);
Finder's Compress 9 of 9.

## Upsets: a lower level beating a higher rival setting on all three

- **Zipling Fast beats Zipgeist ZIP Normal** on 9 of 9 sets (Photos, PDFs and screenshots, Source code, Apple apps, Silesia, Saved folder, Mixed folder, Disk images, Third-party apps): 1 to 53% smaller, 1.1 to 12.1x faster to make, 1.7 to 6.8x faster to extract.
- **Zipling Default beats Zipgeist 7z Best** on 6 of 9 sets (Photos, PDFs and screenshots, Apple apps, Saved folder, Mixed folder, Disk images): 2 to 43% smaller, 1.3 to 8.0x faster to make, 1.9 to 6.6x faster to extract.
- **Zipling Default beats Zipgeist tar.xz Best** on 6 of 9 sets (Photos, PDFs and screenshots, Apple apps, Saved folder, Mixed folder, Disk images): 2 to 43% smaller, 1.3 to 8.0x faster to make, 1.5 to 5.0x faster to extract.
- **Zipling Default beats 7-Zip Ultra** on 5 of 9 sets (Photos, PDFs and screenshots, Saved folder, Mixed folder, Disk images): 1 to 42% smaller, 2.1 to 18.5x faster to make, 2.3 to 5.3x faster to extract.
- **Zipling Default beats WinRAR Best** on 5 of 9 sets (PDFs and screenshots, Apple apps, Saved folder, Mixed folder, Disk images): 2 to 43% smaller, 1.3 to 16.9x faster to make, 1.2 to 4.4x faster to extract.
- **Zipling Default beats xz -9e** on 5 of 9 sets (Photos, PDFs and screenshots, Saved folder, Mixed folder, Disk images): 1 to 43% smaller, 10.5 to 97.3x faster to make, 2.8 to 7.9x faster to extract.
- **Zipling Default beats zstd -19 --long** on 5 of 9 sets (PDFs and screenshots, Apple apps, Saved folder, Mixed folder, Disk images): 1 to 43% smaller, 4.5 to 34.5x faster to make, 1.3 to 6.1x faster to extract.
- **Zipling Fast beats WinRAR Normal** on 4 of 9 sets (Photos, PDFs and screenshots, Source code, Saved folder): 0 to 25% smaller, 9.7 to 20.6x faster to make, 2.8 to 4.8x faster to extract.
- **Zipling Fast beats xz -6** on 1 of 9 sets (PDFs and screenshots): 5% smaller, 55.0x faster to make, 5.4x faster to extract.

## Scoreboard: one grid per level

### Zipling Fast

| Set | Zipgeist ZIP Fast | Zipgeist tar.zst Fast | Finder | zstd -3 | WinRAR Fastest |
|---|---|---|---|---|---|
| Photos | ✅ | ✅ | ✅ | ✅ | ✅ |
| PDFs and screenshots | ✅ | ✅ | ✅ | ✅ | ✅ |
| Source code | ✅ | ✅ | ✅ | ✅ | ✅ |
| Apple apps | ✅ | ✅ | ✅ | ✅ | ✅ |
| Silesia | ✅ | ❌ 1.3x slower to make | ✅ | ❌ 1.1x slower to make | ✅ |
| Saved folder | ✅ | ✅ | ✅ | ✅ | ✅ |
| Mixed folder | ✅ | ✅ | ✅ | ✅ | ✅ |
| Disk images | ✅ | ✅ | ✅ | ✅ | ❌ 0% bigger |
| Third-party apps | ✅ | ✅ | ✅ | ✅ | ✅ |

**Fast: 42 of 45 won.**

### Zipling Default

| Set | Zipgeist ZIP Normal | Zipgeist 7z Normal | 7-Zip Normal | WinRAR Normal | xz -6 |
|---|---|---|---|---|---|
| Photos | ❌ 25x slower to make, 1.3x slower to extract | ✅ | ✅ | ❌ 1.8x slower to make, 1.9x slower to extract | ❌ 1.1x slower to extract |
| PDFs and screenshots | ❌ 11x slower to make | ✅ | ✅ | ❌ 1.3x slower to make | ✅ |
| Source code | ❌ 4x slower to make | ❌ 7% bigger | ❌ 22% bigger | ✅ | ❌ 12% bigger |
| Apple apps | ❌ 1.4x slower to make | ✅ | ❌ 2% bigger | ✅ | ✅ |
| Silesia | ❌ 4x slower to make | ❌ 15% bigger | ❌ 15% bigger | ❌ 6% bigger, 1.4x slower to make | ❌ 16% bigger |
| Saved folder | ❌ 4x slower to make | ✅ | ✅ | ✅ | ✅ |
| Mixed folder | ❌ 3x slower to make | ✅ | ✅ | ✅ | ✅ |
| Disk images | ❌ 4x slower to make | ✅ | ✅ | ✅ | ✅ |
| Third-party apps | ✅ | ❌ 6% bigger | ❌ 9% bigger | ✅ | ❌ 3% bigger |

**Default: 23 of 45 won.**

### Zipling Max

| Set | Zipgeist 7z Best | Zipgeist tar.xz Best | 7-Zip Ultra | WinRAR Best | zstd -19 --long | xz -9e |
|---|---|---|---|---|---|---|
| Photos | ✅ | ✅ | ✅ | ❌ 1.8x slower to extract | ❌ 2.7x slower to extract | ✅ |
| PDFs and screenshots | ❌ 5x slower to make | ❌ 6x slower to make | ❌ 4x slower to make | ❌ 6x slower to make, 1.5x slower to extract | ❌ 1.7x slower to make, 1.1x slower to extract | ✅ |
| Source code | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Apple apps | ❌ 2.0x slower to make | ❌ 1.8x slower to make | ✅ | ❌ 2.2x slower to make | ✅ | ✅ |
| Silesia | ✅ | ✅ | ✅ | ❌ 1.3x slower to extract | ❌ 2.6x slower to extract | ✅ |
| Saved folder | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Mixed folder | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Disk images | ❌ 5x slower to make, 2.9x slower to extract | ❌ 5x slower to make, 2.5x slower to extract | ❌ 2.1x slower to make, 2.3x slower to extract | ❌ 2.3x slower to make, 2.8x slower to extract | ❌ 1.1x slower to make, 9x slower to extract | ❌ 1.6x slower to extract |
| Third-party apps | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |

**Max: 36 of 54 won.**

### Zipling Ultra*

| Set | Zipgeist 7z Best | Zipgeist tar.xz Best | 7-Zip Ultra | WinRAR Best | zstd -19 --long | xz -9e |
|---|---|---|---|---|---|---|
| Photos | ✅ | ✅ | ✅ | ❌ 1.8x slower to extract | ❌ 2.8x slower to extract | ✅ |
| PDFs and screenshots | ❌ 8x slower to make | ❌ 8x slower to make | ❌ 5x slower to make | ❌ 9x slower to make, 1.5x slower to extract | ❌ 2.4x slower to make, 1.1x slower to extract | ✅ |
| Source code | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Apple apps | ❌ 2.3x slower to make | ❌ 2.2x slower to make | ✅ | ❌ 2.7x slower to make, 1.0x slower to extract | ✅ | ✅ |
| Silesia | ✅ | ✅ | ✅ | ❌ 2.1x slower to make, 1.9x slower to extract | ❌ 4x slower to extract | ✅ |
| Saved folder | ❌ 1.2x slower to make | ❌ 1.2x slower to make | ✅ | ❌ 1.1x slower to make | ✅ | ✅ |
| Mixed folder | ✅ | ❌ 1.0x slower to make | ✅ | ❌ 1.3x slower to make | ✅ | ✅ |
| Disk images | ❌ 7x slower to make, 2.8x slower to extract | ❌ 7x slower to make, 2.4x slower to extract | ❌ 2.9x slower to make, 2.3x slower to extract | ❌ 3x slower to make, 2.8x slower to extract | ❌ 1.6x slower to make, 9x slower to extract | ❌ 1.5x slower to extract |
| Third-party apps | ❌ 1.1x slower to make | ❌ 1.1x slower to make | ✅ | ❌ 1.2x slower to make | ✅ | ✅ |

**Ultra*: 28 of 54 won.**

## Set by set

**Photos (95 MB, 400 JPEGs)**

| Tool | Size | Make | Extract |
|---|---:|---:|---:|
| **Zipling Fast** | 89.08 MB | 0.05 s | 0.08 s |
| Finder Compress / Expand | 93.32 MB | 3.11 s | 1.23 s |
| Zipgeist ZIP Fast | 98.47 MB | 0.08 s | 0.55 s |
| Zipgeist tar.zst Fast | 92.06 MB | 0.10 s | 0.48 s |
| tar + zstd -3 | 91.72 MB | 0.24 s | 0.25 s |
| WinRAR Fastest (-m1) | 92.51 MB | 1.05 s | 0.30 s |
| **Zipling Default** | 69.18 MB | 2.00 s | 0.70 s |
| Zipgeist ZIP Normal | 98.47 MB | 0.08 s | 0.52 s |
| Zipgeist 7z Normal | 86.04 MB | 2.66 s | 1.75 s |
| 7-Zip Normal (-mx5) | 85.29 MB | 6.01 s | 1.59 s |
| WinRAR Normal (-m3) | 90.93 MB | 1.12 s | 0.36 s |
| tar + xz -6 | 89.04 MB | 8.22 s | 0.65 s |
| **Zipling Max** | 69.17 MB | 2.05 s | 0.68 s |
| **Zipling Ultra*** | 69.17 MB | 2.15 s | 0.70 s |
| Zipgeist 7z Best | 86.03 MB | 2.68 s | 1.74 s |
| Zipgeist tar.xz Best | 86.63 MB | 3.06 s | 1.03 s |
| 7-Zip Ultra (-mx9) | 85.29 MB | 6.40 s | 1.58 s |
| WinRAR Best (-m5, solid) | 86.21 MB | 2.89 s | 0.38 s |
| tar + zstd -19 --long | 85.70 MB | 14.5 s | 0.25 s |
| tar + xz -9e | 85.60 MB | 38.5 s | 1.99 s |

**PDFs and screenshots (89 MB, 850 files)**

| Tool | Size | Make | Extract |
|---|---:|---:|---:|
| **Zipling Fast** | 41.06 MB | 0.08 s | 0.12 s |
| Finder Compress / Expand | 50.24 MB | 2.03 s | 1.25 s |
| Zipgeist ZIP Fast | 88.08 MB | 0.11 s | 0.58 s |
| Zipgeist tar.zst Fast | 45.24 MB | 0.11 s | 0.50 s |
| tar + zstd -3 | 45.07 MB | 0.40 s | 0.42 s |
| WinRAR Fastest (-m1) | 50.04 MB | 0.84 s | 0.39 s |
| **Zipling Default** | 22.14 MB | 1.38 s | 0.26 s |
| Zipgeist ZIP Normal | 87.89 MB | 0.13 s | 0.58 s |
| Zipgeist 7z Normal | 38.71 MB | 1.72 s | 1.75 s |
| 7-Zip Normal (-mx5) | 38.29 MB | 2.47 s | 0.80 s |
| WinRAR Normal (-m3) | 49.75 MB | 1.08 s | 0.36 s |
| tar + xz -6 | 43.29 MB | 4.14 s | 0.64 s |
| **Zipling Max** | 18.20 MB | 10.5 s | 0.45 s |
| **Zipling Ultra*** | 17.81 MB | 15.0 s | 0.46 s |
| Zipgeist 7z Best | 38.67 MB | 1.91 s | 1.73 s |
| Zipgeist tar.xz Best | 38.86 MB | 1.77 s | 1.04 s |
| 7-Zip Ultra (-mx9) | 38.25 MB | 2.86 s | 0.83 s |
| WinRAR Best (-m5, solid) | 38.74 MB | 1.74 s | 0.30 s |
| tar + zstd -19 --long | 38.82 MB | 6.27 s | 0.41 s |
| tar + xz -9e | 38.80 MB | 20.8 s | 1.15 s |

**Source code (172 MB, 7,693 files)**

| Tool | Size | Make | Extract |
|---|---:|---:|---:|
| **Zipling Fast** | 13.55 MB | 0.25 s | 0.52 s |
| Finder Compress / Expand | 25.11 MB | 6.54 s | 5.16 s |
| Zipgeist ZIP Fast | 24.05 MB | 0.45 s | 0.93 s |
| Zipgeist tar.zst Fast | 16.99 MB | 0.51 s | 0.88 s |
| tar + zstd -3 | 15.09 MB | 3.72 s | 3.53 s |
| WinRAR Fastest (-m1) | 21.82 MB | 1.72 s | 1.42 s |
| **Zipling Default** | 11.20 MB | 1.88 s | 0.53 s |
| Zipgeist ZIP Normal | 21.20 MB | 0.48 s | 0.90 s |
| Zipgeist 7z Normal | 10.47 MB | 5.42 s | 1.05 s |
| 7-Zip Normal (-mx5) | 9.15 MB | 12.9 s | 1.14 s |
| WinRAR Normal (-m3) | 18.15 MB | 2.38 s | 1.46 s |
| tar + xz -6 | 9.96 MB | 6.22 s | 3.48 s |
| **Zipling Max** | 8.31 MB | 1.55 s | 0.83 s |
| **Zipling Ultra*** | 8.09 MB | 1.70 s | 1.01 s |
| Zipgeist 7z Best | 10.09 MB | 6.57 s | 1.03 s |
| Zipgeist tar.xz Best | 9.08 MB | 5.99 s | 1.02 s |
| 7-Zip Ultra (-mx9) | 8.51 MB | 21.9 s | 1.16 s |
| WinRAR Best (-m5, solid) | 10.15 MB | 4.48 s | 1.23 s |
| tar + zstd -19 --long | 9.29 MB | 25.8 s | 3.50 s |
| tar + xz -9e | 8.35 MB | 66.1 s | 3.48 s |

**Apple apps (170 MB, 8 apps)**

| Tool | Size | Make | Extract |
|---|---:|---:|---:|
| **Zipling Fast** | 72.30 MB | 0.13 s | 0.23 s |
| Finder Compress / Expand | 87.42 MB | 6.66 s | 2.42 s |
| Zipgeist ZIP Fast | 91.82 MB | 0.49 s | 0.78 s |
| Zipgeist tar.zst Fast | 86.23 MB | 0.29 s | 0.71 s |
| tar + zstd -3 | 76.47 MB | 1.06 s | 1.14 s |
| WinRAR Fastest (-m1) | 78.68 MB | 1.24 s | 0.70 s |
| **Zipling Default** | 56.44 MB | 1.38 s | 0.26 s |
| Zipgeist ZIP Normal | 86.33 MB | 0.98 s | 0.77 s |
| Zipgeist 7z Normal | 59.58 MB | 5.79 s | 0.97 s |
| 7-Zip Normal (-mx5) | 55.26 MB | 16.8 s | 1.41 s |
| WinRAR Normal (-m3) | 64.68 MB | 2.51 s | 0.76 s |
| tar + xz -6 | 58.97 MB | 8.47 s | 1.26 s |
| **Zipling Max** | 49.15 MB | 12.1 s | 0.70 s |
| **Zipling Ultra*** | 48.55 MB | 14.5 s | 0.71 s |
| Zipgeist 7z Best | 59.30 MB | 6.16 s | 0.95 s |
| Zipgeist tar.xz Best | 58.66 MB | 6.53 s | 1.21 s |
| 7-Zip Ultra (-mx9) | 54.10 MB | 27.7 s | 1.66 s |
| WinRAR Best (-m5, solid) | 58.69 MB | 5.40 s | 0.70 s |
| tar + zstd -19 --long | 59.36 MB | 21.3 s | 1.15 s |
| tar + xz -9e | 54.33 MB | 57.9 s | 2.40 s |

**Silesia corpus (203 MB)**

| Tool | Size | Make | Extract |
|---|---:|---:|---:|
| **Zipling Fast** | 65.18 MB | 0.17 s | 0.11 s |
| Finder Compress / Expand | 68.23 MB | 6.21 s | 1.32 s |
| Zipgeist ZIP Fast | 77.28 MB | 0.39 s | 0.70 s |
| Zipgeist tar.zst Fast | 73.31 MB | 0.13 s | 0.64 s |
| tar + zstd -3 | 66.25 MB | 0.15 s | 0.21 s |
| WinRAR Fastest (-m1) | 66.68 MB | 0.92 s | 0.41 s |
| **Zipling Default** | 57.26 MB | 4.95 s | 0.13 s |
| Zipgeist ZIP Normal | 68.53 MB | 1.12 s | 0.67 s |
| Zipgeist 7z Normal | 49.95 MB | 16.3 s | 0.81 s |
| 7-Zip Normal (-mx5) | 49.60 MB | 22.9 s | 0.98 s |
| WinRAR Normal (-m3) | 54.21 MB | 3.64 s | 0.45 s |
| tar + xz -6 | 49.50 MB | 13.2 s | 0.51 s |
| **Zipling Max** | 46.68 MB | 6.22 s | 0.59 s |
| **Zipling Ultra*** | 45.92 MB | 13.5 s | 0.84 s |
| Zipgeist 7z Best | 49.15 MB | 19.7 s | 0.89 s |
| Zipgeist tar.xz Best | 49.16 MB | 19.8 s | 1.10 s |
| 7-Zip Ultra (-mx9) | 48.69 MB | 48.2 s | 1.56 s |
| WinRAR Best (-m5, solid) | 53.12 MB | 6.36 s | 0.45 s |
| tar + zstd -19 --long | 52.60 MB | 30.1 s | 0.23 s |
| tar + xz -9e | 48.42 MB | 83.8 s | 1.99 s |

**Saved folder (533 MB, 11,051 files)**

| Tool | Size | Make | Extract |
|---|---:|---:|---:|
| **Zipling Fast** | 311.2 MB | 0.55 s | 1.03 s |
| Finder Compress / Expand | 356.1 MB | 22.1 s | 9.06 s |
| Zipgeist ZIP Fast | 393.1 MB | 1.75 s | 1.80 s |
| Zipgeist tar.zst Fast | 341.0 MB | 2.17 s | 1.48 s |
| tar + zstd -3 | 328.2 MB | 6.46 s | 10.5 s |
| WinRAR Fastest (-m1) | 339.5 MB | 5.40 s | 3.23 s |
| **Zipling Default** | 248.8 MB | 8.08 s | 1.75 s |
| Zipgeist ZIP Normal | 381.8 MB | 2.26 s | 1.79 s |
| Zipgeist 7z Normal | 285.2 MB | 15.1 s | 3.29 s |
| 7-Zip Normal (-mx5) | 276.4 MB | 21.4 s | 2.84 s |
| WinRAR Normal (-m3) | 312.3 MB | 8.85 s | 3.35 s |
| tar + xz -6 | 292.9 MB | 22.7 s | 10.7 s |
| **Zipling Max** | 221.7 MB | 16.1 s | 2.73 s |
| **Zipling Ultra*** | 218.6 MB | 21.9 s | 2.82 s |
| Zipgeist 7z Best | 284.2 MB | 17.5 s | 3.28 s |
| Zipgeist tar.xz Best | 284.9 MB | 17.9 s | 2.95 s |
| 7-Zip Ultra (-mx9) | 274.0 MB | 53.2 s | 4.01 s |
| WinRAR Best (-m5, solid) | 285.0 MB | 19.7 s | 3.13 s |
| tar + zstd -19 --long | 282.7 MB | 40.5 s | 10.6 s |
| tar + xz -9e | 278.0 MB | 85.4 s | 10.6 s |

**Website mixed folder (726 MB, the five public sets)**

| Tool | Size | Make | Extract |
|---|---:|---:|---:|
| **Zipling Fast** | 281.1 MB | 0.51 s | 0.82 s |
| Finder Compress / Expand | 324.6 MB | 24.1 s | 7.90 s |
| Zipgeist ZIP Fast | 379.9 MB | 1.54 s | 1.68 s |
| Zipgeist tar.zst Fast | 313.8 MB | 2.04 s | 1.46 s |
| tar + zstd -3 | 294.6 MB | 5.63 s | 5.50 s |
| WinRAR Fastest (-m1) | 309.8 MB | 5.61 s | 3.18 s |
| **Zipling Default** | 216.0 MB | 8.39 s | 1.49 s |
| Zipgeist ZIP Normal | 362.6 MB | 2.45 s | 1.59 s |
| Zipgeist 7z Normal | 245.4 MB | 25.5 s | 3.32 s |
| 7-Zip Normal (-mx5) | 237.8 MB | 26.1 s | 2.96 s |
| WinRAR Normal (-m3) | 277.8 MB | 10.6 s | 3.26 s |
| tar + xz -6 | 251.2 MB | 25.1 s | 5.79 s |
| **Zipling Max** | 192.6 MB | 19.3 s | 2.55 s |
| **Zipling Ultra*** | 189.3 MB | 28.1 s | 2.73 s |
| Zipgeist 7z Best | 243.9 MB | 28.9 s | 3.28 s |
| Zipgeist tar.xz Best | 243.9 MB | 26.9 s | 2.90 s |
| 7-Zip Ultra (-mx9) | 235.0 MB | 48.5 s | 3.84 s |
| WinRAR Best (-m5, solid) | 247.4 MB | 21.6 s | 2.98 s |
| tar + zstd -19 --long | 245.7 MB | 46.1 s | 5.51 s |
| tar + xz -9e | 236.6 MB | 87.8 s | 7.10 s |

**Disk images (592 MB, 5 installers)**

| Tool | Size | Make | Extract |
|---|---:|---:|---:|
| **Zipling Fast** | 615.5 MB | 0.20 s | 0.24 s |
| Finder Compress / Expand | 617.3 MB | 12.6 s | 1.88 s |
| Zipgeist ZIP Fast | 620.1 MB | 0.23 s | 0.70 s |
| Zipgeist tar.zst Fast | 617.6 MB | 0.33 s | 0.64 s |
| tar + zstd -3 | 616.1 MB | 0.26 s | 0.45 s |
| WinRAR Fastest (-m1) | 615.2 MB | 3.35 s | 1.40 s |
| **Zipling Default** | 601.8 MB | 1.02 s | 0.34 s |
| Zipgeist ZIP Normal | 620.1 MB | 0.23 s | 0.75 s |
| Zipgeist 7z Normal | 611.9 MB | 8.09 s | 1.51 s |
| 7-Zip Normal (-mx5) | 610.6 MB | 9.16 s | 1.41 s |
| WinRAR Normal (-m3) | 612.3 MB | 18.7 s | 1.50 s |
| tar + xz -6 | 612.5 MB | 29.6 s | 1.17 s |
| **Zipling Max** | 455.0 MB | 40.6 s | 4.23 s |
| **Zipling Ultra*** | 448.0 MB | 55.6 s | 4.19 s |
| Zipgeist 7z Best | 611.9 MB | 8.21 s | 1.48 s |
| Zipgeist tar.xz Best | 611.9 MB | 8.17 s | 1.72 s |
| 7-Zip Ultra (-mx9) | 609.6 MB | 18.9 s | 1.82 s |
| WinRAR Best (-m5, solid) | 612.3 MB | 17.3 s | 1.50 s |
| tar + zstd -19 --long | 610.3 MB | 35.3 s | 0.46 s |
| tar + xz -9e | 610.7 MB | 99.7 s | 2.71 s |

**Third-party apps (874 MB, 6 apps)**

| Tool | Size | Make | Extract |
|---|---:|---:|---:|
| **Zipling Fast** | 504.4 MB | 0.59 s | 0.44 s |
| Finder Compress / Expand | 536.1 MB | 40.8 s | 4.43 s |
| Zipgeist ZIP Fast | 582.1 MB | 2.90 s | 2.22 s |
| Zipgeist tar.zst Fast | 569.6 MB | 0.62 s | 1.76 s |
| tar + zstd -3 | 513.0 MB | 1.47 s | 15.1 s |
| WinRAR Fastest (-m1) | 517.2 MB | 6.12 s | 3.08 s |
| **Zipling Default** | 403.0 MB | 6.82 s | 0.49 s |
| Zipgeist ZIP Normal | 540.5 MB | 7.08 s | 2.13 s |
| Zipgeist 7z Normal | 380.0 MB | 51.0 s | 3.96 s |
| 7-Zip Normal (-mx5) | 371.2 MB | 74.1 s | 2.15 s |
| WinRAR Normal (-m3) | 418.1 MB | 25.3 s | 3.95 s |
| tar + xz -6 | 390.2 MB | 51.4 s | 16.1 s |
| **Zipling Max** | 350.2 MB | 40.0 s | 1.77 s |
| **Zipling Ultra*** | 336.2 MB | 58.3 s | 1.83 s |
| Zipgeist 7z Best | 377.9 MB | 55.5 s | 4.75 s |
| Zipgeist tar.xz Best | 377.2 MB | 54.5 s | 4.24 s |
| 7-Zip Ultra (-mx9) | 355.5 MB | 89.3 s | 3.57 s |
| WinRAR Best (-m5, solid) | 399.3 MB | 47.3 s | 3.76 s |
| tar + zstd -19 --long | 382.7 MB | 72.2 s | 15.2 s |
| tar + xz -9e | 364.2 MB | 101.7 s | 17.2 s |

## Finder's own Compress and Expand against Zipling Fast

| Set | Finder size | Finder make | Expand | Fast size | Fast make | Fast extract | Fast: smaller / faster make / faster extract |
|---|---:|---:|---:|---:|---:|---:|---|
| Photos | 93.32 MB | 3.11 s | 1.23 s | 89.08 MB | 0.05 s | 0.08 s | 5% / 57x / 16x |
| PDFs and screenshots | 50.24 MB | 2.03 s | 1.25 s | 41.06 MB | 0.08 s | 0.12 s | 18% / 27x / 11x |
| Source code | 25.11 MB | 6.54 s | 5.16 s | 13.55 MB | 0.25 s | 0.52 s | 46% / 27x / 10x |
| Apple apps | 87.42 MB | 6.66 s | 2.42 s | 72.30 MB | 0.13 s | 0.23 s | 17% / 52x / 10x |
| Silesia | 68.23 MB | 6.21 s | 1.32 s | 65.18 MB | 0.17 s | 0.11 s | 4% / 36x / 12x |
| Saved folder | 356.1 MB | 22.1 s | 9.06 s | 311.2 MB | 0.55 s | 1.03 s | 13% / 41x / 9x |
| Mixed folder | 324.6 MB | 24.1 s | 7.90 s | 281.1 MB | 0.51 s | 0.82 s | 13% / 47x / 10x |
| Disk images | 617.3 MB | 12.6 s | 1.88 s | 615.5 MB | 0.20 s | 0.24 s | 0% / 63x / 8x |
| Third-party apps | 536.1 MB | 40.8 s | 4.43 s | 504.4 MB | 0.59 s | 0.44 s | 6% / 70x / 10x |

## The app against the engine

The same presets measured through the engine's command line on the morning of 31 October
(`results-2026-10-31-presets.jsonl`, quiet machine) against the app's clock in this run. Make time
is the ratio app / engine; extract time is the app's extra in milliseconds, with the ratio.

| Set | Fast make | Fast extract | Default make | Default extract | Max make | Max extract | Ultra* make | Ultra* extract |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Photos | 0.99x | +28 ms (1.60x) | 0.97x | +11 ms (1.02x) | 0.97x | -5 ms (0.99x) | 0.96x | +8 ms (1.01x) |
| PDFs and screenshots | 1.18x | +49 ms (1.71x) | 1.03x | +32 ms (1.14x) | 0.97x | +40 ms (1.10x) | 0.98x | +49 ms (1.12x) |
| Source code | 1.00x | +155 ms (1.42x) | 0.94x | +166 ms (1.46x) | 0.97x | +148 ms (1.22x) | 0.98x | +91 ms (1.10x) |
| Apple apps | 0.94x | +87 ms (1.60x) | 0.97x | +56 ms (1.27x) | 0.97x | +54 ms (1.08x) | 0.96x | +86 ms (1.14x) |
| Silesia | 1.09x | +55 ms (1.99x) | 1.05x | +37 ms (1.39x) | 1.01x | +23 ms (1.04x) | 0.99x | +28 ms (1.03x) |
| Saved folder | 1.11x | +330 ms (1.47x) | 1.00x | +226 ms (1.15x) | 0.97x | +243 ms (1.10x) | 0.99x | +236 ms (1.09x) |
| Mixed folder | 0.98x | +219 ms (1.36x) | 0.98x | +206 ms (1.16x) | 1.01x | +183 ms (1.08x) | 0.96x | +300 ms (1.12x) |
| Disk images | 1.04x | +112 ms (1.89x) | 0.97x | -2 ms (0.99x) | 0.99x | -26 ms (0.99x) | 1.00x | -8 ms (1.00x) |
| Third-party apps | 1.03x | +44 ms (1.11x) | 0.95x | +0 ms (1.00x) | 0.95x | +31 ms (1.02x) | 0.95x | -13 ms (0.99x) |

Make time is the same within noise (the geometric mean of the ratios is 0.99x). Extraction in the app
differs by -26 to +330 ms, nearly always more: it decodes the listing, hops to the main actor and walks the
extracted tree for safety, about 30 ms plus a few microseconds per file. On the quick Fast rows that is
the difference between 0.05 and 0.08 s. Scored against the same rival rows, the engine's times would win 127 of 198 and the app's win 129. No row turns on the app's extra time. Rows that go the other way, on noise: Mixed folder, Zipling Ultra* against Zipgeist 7z Best; Third-party apps, Zipling Default against Zipgeist ZIP Normal. Skipping the listing for a
whole-archive extract and a cheaper safety walk are the two follow-ups, worth 30 to 300 ms per
extraction, no row at stake.

## Peak memory while compressing (MB)

Zipling's figure is the app process's peak physical footprint sampled every 20 ms; the command-line
tools' is `/usr/bin/time -l`'s maximum resident set size. Zipgeist's stopwatch sampled its helper
processes and is not comparable, so it is left out.

| Set | Fast | Default | Max | Ultra* | 7-Zip Ultra | WinRAR Best | zstd -19 --long | xz -9e |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Photos | 1374 | 2238 | 2283 | 2201 | 983 | 571 | 409 | 850 |
| PDFs and screenshots | 1000 | 1224 | 1611 | 1455 | 920 | 567 | 359 | 794 |
| Source code | 577 | 880 | 916 | 897 | 1676 | 551 | 508 | 861 |
| Apple apps | 1082 | 1572 | 1962 | 1425 | 1759 | 574 | 547 | 899 |
| Silesia | 874 | 1011 | 2104 | 1302 | 2084 | 572 | 656 | 1080 |
| Saved folder | 2216 | 3591 | 4324 | 4038 | 6623 | 579 | 1834 | 3624 |
| Mixed folder | 1704 | 3549 | 3644 | 4213 | 6874 | 578 | 1871 | 3656 |
| Disk images | 3339 | 3744 | 4137 | 5593 | 6555 | 571 | 2052 | 3398 |
| Third-party apps | 3200 | 3223 | 3623 | 5166 | 8922 | 574 | 1989 | 5541 |

## Default: what is fixable and what is not

Default loses two kinds of rows, for two different reasons.

**1. Against Zipgeist ZIP Normal, on make time, on 8 of nine sets.** ZIP Normal is a near-store
Deflate: 0.08 s on the photos, 0.13 s on the PDFs, 2.26 s on the Saved folder. Default makes files
3 to 75% smaller than it and extracts faster on 8 of nine sets, but takes 1.0x to 25x longer, because it runs
the transforms. No preset that compresses can beat a near-store ZIP on make time; the Zipling setting
that answers it is Fast, and Fast beats ZIP Normal on all nine sets on all three measures (the first
upset above). The same holds for WinRAR Normal on the photos and PDFs, where Fast wins too. If the
website compares level against level these rows stay red; if it compares the rival's default button
with the Zipling button that answers it, they are green rows for Fast.

**2. Against the LZMA-class normal settings on size: 7-Zip Normal, Zipgeist 7z Normal and xz -6 on
Source code, Apple apps, Silesia, Third-party apps.** Default's lanes are zstd 15 and 12 with every transform; those rivals are LZMA.
Default is 2 to 12x faster to make and 2 to 7x faster to extract than 7-Zip Normal on every set, and smaller on
5 of nine; on the three code- and text-heavy sets it is 22%, 2%, 15%, 9% bigger. This is the part a codec change
could fix, and the measured options (engine runs, quiet machine, 30 and 31 October) are:

| Option | Source code | Silesia | Third-party apps | Cost |
|---|---|---|---|---|
| Default as is (this run, the app) | 11.20 MB, 1.88 s | 57.26 MB, 4.95 s | 403.0 MB, 6.82 s | — |
| Default with bsc on the text lane | 10.21 MB, 0.95 s: wins the row | 53.07 MB, 2.32 s: still 6% bigger | 402.61 MB: still 6% bigger | extract 1.3 to 4x slower on text-heavy sets (still 2x faster than 7z Normal); DMGs make 1.05 to 5.1 s |
| Max's codecs at Default (bsc text, LZMA2 7 binaries) | 8.23 MB, 1.61 s: wins | 49.41 MB, 6.14 s: wins | 350.23 MB, 39 s: wins | make 2 to 40x slower than today's Default; loses the normal-class rows it wins today on PDFs, apps, Saved, DMGs (make time) |
| The lzt codec (LZMA's parse at a symbol coder's decode speed, research stage) | within 3 to 5% of LZMA at 2x its decode speed, measured on Silesia | | | needs a format change and a quiet month; the only route to LZMA's size at Default's extract speed |

Recommendation unchanged: keep Default's codecs and present Default as the middle it is, smaller
than every normal setting on most sets and several times faster than the LZMA settings that beat it on
size on the other three. The rows Default loses on make time are Fast's rows.

## Max and Ultra*: where the transforms cost time

Max and Ultra* lose make time (and on the disk images extract time too) where the transforms buy the
most size: re-creating the compression inside PDFs and PNGs, re-coding JPEGs, expanding the chunks of
disk images. On the PDFs Max is 52% smaller than 7-Zip Ultra but takes 10.5 s against 2.86 s; on the disk
images 25% smaller in 40.6 s against 18.9 s. In every one of those rows Default already beats the same rival on
all three measures (the upsets above), so the user who wants the win on time has it one level down.
Ultra* adds 0.0 to 4.0% over Max for 5 to 117% more make time; it stays experimental.

## Wins and losses, row by row

Ratios are Zipling / rival for size, make and extract; under 1 is a win.

```
1:1 by level (ratios Arca / rival: size, make, extract; under 1 is a win)

== jpeg: Photos (400 JPEGs)
  Fast     vs tar + zstd -3               0.97    0.22    0.31  won 
  Fast     vs Zipgeist tar.zst Fast       0.97    0.54    0.16  won 
  Fast     vs Zipgeist ZIP Fast           0.90    0.68    0.14  won 
  Fast     vs WinRAR Fastest (-m1)        0.96    0.05    0.25  won 
  Fast     vs Finder Compress / Expand    0.95    0.02    0.06  won 
  Default  vs 7-Zip normal (-mx5)         0.81    0.33    0.44  won 
  Default  vs Zipgeist 7z Normal          0.80    0.75    0.40  won 
  Default  vs Zipgeist ZIP Normal         0.70   24.95    1.35  LOST on make, extract
  Default  vs WinRAR Normal (-m3)         0.76    1.79    1.92  LOST on make, extract
  Default  vs tar + xz -6                 0.78    0.24    1.08  LOST on extract
  Max      vs 7-Zip Ultra (-mx9)          0.81    0.32    0.43  won 
  Max      vs tar + zstd -19 --long       0.81    0.14    2.70  LOST on extract
  Max      vs tar + xz -9e                0.81    0.05    0.34  won 
  Max      vs WinRAR Best (-m5, solid)    0.80    0.71    1.76  LOST on extract
  Max      vs Zipgeist 7z Best            0.80    0.77    0.39  won 
  Max      vs Zipgeist tar.xz Best        0.80    0.67    0.66  won 
  Ultra    vs 7-Zip Ultra (-mx9)          0.81    0.34    0.44  won 
  Ultra    vs tar + zstd -19 --long       0.81    0.15    2.78  LOST on extract
  Ultra    vs tar + xz -9e                0.81    0.06    0.35  won 
  Ultra    vs WinRAR Best (-m5, solid)    0.80    0.74    1.82  LOST on extract
  Ultra    vs Zipgeist 7z Best            0.80    0.80    0.40  won 
  Ultra    vs Zipgeist tar.xz Best        0.80    0.70    0.68  won 

== docs: PDFs and screenshots
  Fast     vs tar + zstd -3               0.91    0.19    0.28  won 
  Fast     vs Zipgeist tar.zst Fast       0.91    0.68    0.24  won 
  Fast     vs Zipgeist ZIP Fast           0.47    0.68    0.20  won 
  Fast     vs WinRAR Fastest (-m1)        0.82    0.09    0.31  won 
  Fast     vs Finder Compress / Expand    0.82    0.04    0.09  won 
  Default  vs 7-Zip normal (-mx5)         0.58    0.56    0.33  won 
  Default  vs Zipgeist 7z Normal          0.57    0.80    0.15  won 
  Default  vs Zipgeist ZIP Normal         0.25   10.63    0.45  LOST on make
  Default  vs WinRAR Normal (-m3)         0.44    1.28    0.72  LOST on make
  Default  vs tar + xz -6                 0.51    0.33    0.41  won 
  Max      vs 7-Zip Ultra (-mx9)          0.48    3.66    0.54  LOST on make
  Max      vs tar + zstd -19 --long       0.47    1.67    1.08  LOST on make, extract
  Max      vs tar + xz -9e                0.47    0.50    0.39  won 
  Max      vs WinRAR Best (-m5, solid)    0.47    6.03    1.49  LOST on make, extract
  Max      vs Zipgeist 7z Best            0.47    5.49    0.26  LOST on make
  Max      vs Zipgeist tar.xz Best        0.47    5.92    0.43  LOST on make
  Ultra    vs 7-Zip Ultra (-mx9)          0.47    5.26    0.55  LOST on make
  Ultra    vs tar + zstd -19 --long       0.46    2.40    1.10  LOST on make, extract
  Ultra    vs tar + xz -9e                0.46    0.72    0.40  won 
  Ultra    vs WinRAR Best (-m5, solid)    0.46    8.65    1.53  LOST on make, extract
  Ultra    vs Zipgeist 7z Best            0.46    7.87    0.26  LOST on make
  Ultra    vs Zipgeist tar.xz Best        0.46    8.50    0.44  LOST on make

== source: Source code (89 Rust crates)
  Fast     vs tar + zstd -3               0.90    0.07    0.15  won 
  Fast     vs Zipgeist tar.zst Fast       0.80    0.48    0.59  won 
  Fast     vs Zipgeist ZIP Fast           0.56    0.55    0.56  won 
  Fast     vs WinRAR Fastest (-m1)        0.62    0.14    0.37  won 
  Fast     vs Finder Compress / Expand    0.54    0.04    0.10  won 
  Default  vs 7-Zip normal (-mx5)         1.22    0.15    0.46  LOST on size
  Default  vs Zipgeist 7z Normal          1.07    0.35    0.50  LOST on size
  Default  vs Zipgeist ZIP Normal         0.53    3.91    0.59  LOST on make
  Default  vs WinRAR Normal (-m3)         0.62    0.79    0.36  won 
  Default  vs tar + xz -6                 1.12    0.30    0.15  LOST on size
  Max      vs 7-Zip Ultra (-mx9)          0.98    0.07    0.72  won 
  Max      vs tar + zstd -19 --long       0.89    0.06    0.24  won 
  Max      vs tar + xz -9e                1.00    0.02    0.24  won 
  Max      vs WinRAR Best (-m5, solid)    0.82    0.35    0.68  won 
  Max      vs Zipgeist 7z Best            0.82    0.24    0.81  won 
  Max      vs Zipgeist tar.xz Best        0.91    0.26    0.81  won 
  Ultra    vs 7-Zip Ultra (-mx9)          0.95    0.08    0.87  won 
  Ultra    vs tar + zstd -19 --long       0.87    0.07    0.29  won 
  Ultra    vs tar + xz -9e                0.97    0.03    0.29  won 
  Ultra    vs WinRAR Best (-m5, solid)    0.80    0.38    0.82  won 
  Ultra    vs Zipgeist 7z Best            0.80    0.26    0.98  won 
  Ultra    vs Zipgeist tar.xz Best        0.89    0.28    0.99  won 

== apps: Apple apps (8 from /System/Applications)
  Fast     vs tar + zstd -3               0.95    0.12    0.21  won 
  Fast     vs Zipgeist tar.zst Fast       0.84    0.44    0.33  won 
  Fast     vs Zipgeist ZIP Fast           0.79    0.26    0.30  won 
  Fast     vs WinRAR Fastest (-m1)        0.92    0.10    0.33  won 
  Fast     vs Finder Compress / Expand    0.83    0.02    0.10  won 
  Default  vs 7-Zip normal (-mx5)         1.02    0.08    0.19  LOST on size
  Default  vs Zipgeist 7z Normal          0.95    0.24    0.27  won 
  Default  vs Zipgeist ZIP Normal         0.65    1.41    0.34  LOST on make
  Default  vs WinRAR Normal (-m3)         0.87    0.55    0.35  won 
  Default  vs tar + xz -6                 0.96    0.16    0.21  won 
  Max      vs 7-Zip Ultra (-mx9)          0.91    0.44    0.42  won 
  Max      vs tar + zstd -19 --long       0.83    0.57    0.60  won 
  Max      vs tar + xz -9e                0.90    0.21    0.29  won 
  Max      vs WinRAR Best (-m5, solid)    0.84    2.23    0.99  LOST on make
  Max      vs Zipgeist 7z Best            0.83    1.96    0.74  LOST on make
  Max      vs Zipgeist tar.xz Best        0.84    1.85    0.58  LOST on make
  Ultra    vs 7-Zip Ultra (-mx9)          0.90    0.52    0.43  won 
  Ultra    vs tar + zstd -19 --long       0.82    0.68    0.62  won 
  Ultra    vs tar + xz -9e                0.89    0.25    0.30  won 
  Ultra    vs WinRAR Best (-m5, solid)    0.83    2.68    1.01  LOST on make, extract
  Ultra    vs Zipgeist 7z Best            0.82    2.35    0.75  LOST on make
  Ultra    vs Zipgeist tar.xz Best        0.83    2.22    0.59  LOST on make

== silesia: Silesia corpus
  Fast     vs tar + zstd -3               0.98    1.14    0.52  LOST on make
  Fast     vs Zipgeist tar.zst Fast       0.89    1.34    0.17  LOST on make
  Fast     vs Zipgeist ZIP Fast           0.84    0.45    0.16  won 
  Fast     vs WinRAR Fastest (-m1)        0.98    0.19    0.27  won 
  Fast     vs Finder Compress / Expand    0.96    0.03    0.08  won 
  Default  vs 7-Zip normal (-mx5)         1.15    0.22    0.14  LOST on size
  Default  vs Zipgeist 7z Normal          1.15    0.30    0.16  LOST on size
  Default  vs Zipgeist ZIP Normal         0.84    4.42    0.20  LOST on make
  Default  vs WinRAR Normal (-m3)         1.06    1.36    0.29  LOST on size, make
  Default  vs tar + xz -6                 1.16    0.38    0.26  LOST on size
  Max      vs 7-Zip Ultra (-mx9)          0.96    0.13    0.38  won 
  Max      vs tar + zstd -19 --long       0.89    0.21    2.57  LOST on extract
  Max      vs tar + xz -9e                0.96    0.07    0.30  won 
  Max      vs WinRAR Best (-m5, solid)    0.88    0.98    1.32  LOST on extract
  Max      vs Zipgeist 7z Best            0.95    0.32    0.66  won 
  Max      vs Zipgeist tar.xz Best        0.95    0.31    0.54  won 
  Ultra    vs 7-Zip Ultra (-mx9)          0.94    0.28    0.54  won 
  Ultra    vs tar + zstd -19 --long       0.87    0.45    3.66  LOST on extract
  Ultra    vs tar + xz -9e                0.95    0.16    0.42  won 
  Ultra    vs WinRAR Best (-m5, solid)    0.86    2.13    1.88  LOST on make, extract
  Ultra    vs Zipgeist 7z Best            0.93    0.69    0.94  won 
  Ultra    vs Zipgeist tar.xz Best        0.93    0.68    0.76  won 

== saved: Saved folder (the 660 MB test folder)
  Fast     vs tar + zstd -3               0.95    0.08    0.10  won 
  Fast     vs Zipgeist tar.zst Fast       0.91    0.25    0.70  won 
  Fast     vs Zipgeist ZIP Fast           0.79    0.31    0.57  won 
  Fast     vs WinRAR Fastest (-m1)        0.92    0.10    0.32  won 
  Fast     vs Finder Compress / Expand    0.87    0.02    0.11  won 
  Default  vs 7-Zip normal (-mx5)         0.90    0.38    0.61  won 
  Default  vs Zipgeist 7z Normal          0.87    0.53    0.53  won 
  Default  vs Zipgeist ZIP Normal         0.65    3.57    0.98  LOST on make
  Default  vs WinRAR Normal (-m3)         0.80    0.91    0.52  won 
  Default  vs tar + xz -6                 0.85    0.36    0.16  won 
  Max      vs 7-Zip Ultra (-mx9)          0.81    0.30    0.68  won 
  Max      vs tar + zstd -19 --long       0.78    0.40    0.26  won 
  Max      vs tar + xz -9e                0.80    0.19    0.26  won 
  Max      vs WinRAR Best (-m5, solid)    0.78    0.81    0.87  won 
  Max      vs Zipgeist 7z Best            0.78    0.92    0.83  won 
  Max      vs Zipgeist tar.xz Best        0.78    0.90    0.93  won 
  Ultra    vs 7-Zip Ultra (-mx9)          0.80    0.41    0.70  won 
  Ultra    vs tar + zstd -19 --long       0.77    0.54    0.27  won 
  Ultra    vs tar + xz -9e                0.79    0.26    0.26  won 
  Ultra    vs WinRAR Best (-m5, solid)    0.77    1.11    0.90  LOST on make
  Ultra    vs Zipgeist 7z Best            0.77    1.25    0.86  LOST on make
  Ultra    vs Zipgeist tar.xz Best        0.77    1.22    0.96  LOST on make

== mixed: Website mixed folder (all five sets)
  Fast     vs tar + zstd -3               0.95    0.09    0.15  won 
  Fast     vs Zipgeist tar.zst Fast       0.90    0.25    0.56  won 
  Fast     vs Zipgeist ZIP Fast           0.74    0.33    0.49  won 
  Fast     vs WinRAR Fastest (-m1)        0.91    0.09    0.26  won 
  Fast     vs Finder Compress / Expand    0.87    0.02    0.10  won 
  Default  vs 7-Zip normal (-mx5)         0.91    0.32    0.50  won 
  Default  vs Zipgeist 7z Normal          0.88    0.33    0.45  won 
  Default  vs Zipgeist ZIP Normal         0.60    3.42    0.94  LOST on make
  Default  vs WinRAR Normal (-m3)         0.78    0.79    0.46  won 
  Default  vs tar + xz -6                 0.86    0.33    0.26  won 
  Max      vs 7-Zip Ultra (-mx9)          0.82    0.40    0.66  won 
  Max      vs tar + zstd -19 --long       0.78    0.42    0.46  won 
  Max      vs tar + xz -9e                0.81    0.22    0.36  won 
  Max      vs WinRAR Best (-m5, solid)    0.78    0.89    0.86  won 
  Max      vs Zipgeist 7z Best            0.79    0.67    0.78  won 
  Max      vs Zipgeist tar.xz Best        0.79    0.72    0.88  won 
  Ultra    vs 7-Zip Ultra (-mx9)          0.81    0.58    0.71  won 
  Ultra    vs tar + zstd -19 --long       0.77    0.61    0.50  won 
  Ultra    vs tar + xz -9e                0.80    0.32    0.38  won 
  Ultra    vs WinRAR Best (-m5, solid)    0.77    1.31    0.92  LOST on make
  Ultra    vs Zipgeist 7z Best            0.78    0.97    0.83  won 
  Ultra    vs Zipgeist tar.xz Best        0.78    1.05    0.94  LOST on make

== dmgs: Your DMGs (5 installers)
  Fast     vs tar + zstd -3               1.00    0.78    0.53  won 
  Fast     vs Zipgeist tar.zst Fast       1.00    0.61    0.37  won 
  Fast     vs Zipgeist ZIP Fast           0.99    0.87    0.34  won 
  Fast     vs WinRAR Fastest (-m1)        1.00    0.06    0.17  LOST on size
  Fast     vs Finder Compress / Expand    1.00    0.02    0.13  won 
  Default  vs 7-Zip normal (-mx5)         0.99    0.11    0.24  won 
  Default  vs Zipgeist 7z Normal          0.98    0.13    0.23  won 
  Default  vs Zipgeist ZIP Normal         0.97    4.45    0.46  LOST on make
  Default  vs WinRAR Normal (-m3)         0.98    0.05    0.23  won 
  Default  vs tar + xz -6                 0.98    0.03    0.29  won 
  Max      vs 7-Zip Ultra (-mx9)          0.75    2.14    2.33  LOST on make, extract
  Max      vs tar + zstd -19 --long       0.75    1.15    9.16  LOST on make, extract
  Max      vs tar + xz -9e                0.75    0.41    1.56  LOST on extract
  Max      vs WinRAR Best (-m5, solid)    0.74    2.34    2.82  LOST on make, extract
  Max      vs Zipgeist 7z Best            0.74    4.94    2.86  LOST on make, extract
  Max      vs Zipgeist tar.xz Best        0.74    4.96    2.46  LOST on make, extract
  Ultra    vs 7-Zip Ultra (-mx9)          0.73    2.94    2.30  LOST on make, extract
  Ultra    vs tar + zstd -19 --long       0.73    1.58    9.07  LOST on make, extract
  Ultra    vs tar + xz -9e                0.73    0.56    1.54  LOST on extract
  Ultra    vs WinRAR Best (-m5, solid)    0.73    3.22    2.79  LOST on make, extract
  Ultra    vs Zipgeist 7z Best            0.73    6.78    2.83  LOST on make, extract
  Ultra    vs Zipgeist tar.xz Best        0.73    6.81    2.43  LOST on make, extract

== thirdparty: Your third-party apps (6)
  Fast     vs tar + zstd -3               0.98    0.40    0.03  won 
  Fast     vs Zipgeist tar.zst Fast       0.89    0.95    0.25  won 
  Fast     vs Zipgeist ZIP Fast           0.87    0.20    0.20  won 
  Fast     vs WinRAR Fastest (-m1)        0.98    0.10    0.14  won 
  Fast     vs Finder Compress / Expand    0.94    0.01    0.10  won 
  Default  vs 7-Zip normal (-mx5)         1.09    0.09    0.23  LOST on size
  Default  vs Zipgeist 7z Normal          1.06    0.13    0.12  LOST on size
  Default  vs Zipgeist ZIP Normal         0.75    0.96    0.23  won 
  Default  vs WinRAR Normal (-m3)         0.96    0.27    0.12  won 
  Default  vs tar + xz -6                 1.03    0.13    0.03  LOST on size
  Max      vs 7-Zip Ultra (-mx9)          0.99    0.45    0.49  won 
  Max      vs tar + zstd -19 --long       0.92    0.55    0.12  won 
  Max      vs tar + xz -9e                0.96    0.39    0.10  won 
  Max      vs WinRAR Best (-m5, solid)    0.88    0.85    0.47  won 
  Max      vs Zipgeist 7z Best            0.93    0.72    0.37  won 
  Max      vs Zipgeist tar.xz Best        0.93    0.73    0.42  won 
  Ultra    vs 7-Zip Ultra (-mx9)          0.95    0.65    0.51  won 
  Ultra    vs tar + zstd -19 --long       0.88    0.81    0.12  won 
  Ultra    vs tar + xz -9e                0.92    0.57    0.11  won 
  Ultra    vs WinRAR Best (-m5, solid)    0.84    1.23    0.49  LOST on make
  Ultra    vs Zipgeist 7z Best            0.89    1.05    0.39  LOST on make
  Ultra    vs Zipgeist tar.xz Best        0.89    1.07    0.43  LOST on make

Rows won, 1:1 by level:
  Fast     42 of 45
  Default  23 of 45
  Max      36 of 54
  Ultra    28 of 54
  all      129 of 198

Rows lost:
  jpeg: Default vs Zipgeist ZIP Normal on make, extract  (beaten by Fast)
  jpeg: Default vs WinRAR Normal (-m3) on make, extract  (beaten by Fast)
  jpeg: Default vs tar + xz -6 on extract  (no Arca setting beats it)
  jpeg: Max vs tar + zstd -19 --long on extract  (no Arca setting beats it)
  jpeg: Max vs WinRAR Best (-m5, solid) on extract  (no Arca setting beats it)
  jpeg: Ultra vs tar + zstd -19 --long on extract  (no Arca setting beats it)
  jpeg: Ultra vs WinRAR Best (-m5, solid) on extract  (no Arca setting beats it)
  docs: Default vs Zipgeist ZIP Normal on make  (beaten by Fast)
  docs: Default vs WinRAR Normal (-m3) on make  (beaten by Fast)
  docs: Max vs 7-Zip Ultra (-mx9) on make  (beaten by Default)
  docs: Max vs tar + zstd -19 --long on make, extract  (beaten by Default)
  docs: Max vs WinRAR Best (-m5, solid) on make, extract  (beaten by Default)
  docs: Max vs Zipgeist 7z Best on make  (beaten by Default)
  docs: Max vs Zipgeist tar.xz Best on make  (beaten by Default)
  docs: Ultra vs 7-Zip Ultra (-mx9) on make  (beaten by Default)
  docs: Ultra vs tar + zstd -19 --long on make, extract  (beaten by Default)
  docs: Ultra vs WinRAR Best (-m5, solid) on make, extract  (beaten by Default)
  docs: Ultra vs Zipgeist 7z Best on make  (beaten by Default)
  docs: Ultra vs Zipgeist tar.xz Best on make  (beaten by Default)
  source: Default vs 7-Zip normal (-mx5) on size  (beaten by Max, Ultra)
  source: Default vs Zipgeist 7z Normal on size  (beaten by Max, Ultra)
  source: Default vs Zipgeist ZIP Normal on make  (beaten by Fast)
  source: Default vs tar + xz -6 on size  (beaten by Max, Ultra)
  apps: Default vs 7-Zip normal (-mx5) on size  (beaten by Max, Ultra)
  apps: Default vs Zipgeist ZIP Normal on make  (beaten by Fast)
  apps: Max vs WinRAR Best (-m5, solid) on make  (beaten by Default)
  apps: Max vs Zipgeist 7z Best on make  (beaten by Default)
  apps: Max vs Zipgeist tar.xz Best on make  (beaten by Default)
  apps: Ultra vs WinRAR Best (-m5, solid) on make, extract  (beaten by Default)
  apps: Ultra vs Zipgeist 7z Best on make  (beaten by Default)
  apps: Ultra vs Zipgeist tar.xz Best on make  (beaten by Default)
  silesia: Fast vs tar + zstd -3 on make  (no Arca setting beats it)
  silesia: Fast vs Zipgeist tar.zst Fast on make  (no Arca setting beats it)
  silesia: Default vs 7-Zip normal (-mx5) on size  (beaten by Max, Ultra)
  silesia: Default vs Zipgeist 7z Normal on size  (beaten by Max)
  silesia: Default vs Zipgeist ZIP Normal on make  (beaten by Fast)
  silesia: Default vs WinRAR Normal (-m3) on size, make  (no Arca setting beats it)
  silesia: Default vs tar + xz -6 on size  (no Arca setting beats it)
  silesia: Max vs tar + zstd -19 --long on extract  (no Arca setting beats it)
  silesia: Max vs WinRAR Best (-m5, solid) on extract  (no Arca setting beats it)
  silesia: Ultra vs tar + zstd -19 --long on extract  (no Arca setting beats it)
  silesia: Ultra vs WinRAR Best (-m5, solid) on make, extract  (no Arca setting beats it)
  saved: Default vs Zipgeist ZIP Normal on make  (beaten by Fast)
  saved: Ultra vs WinRAR Best (-m5, solid) on make  (beaten by Default, Max)
  saved: Ultra vs Zipgeist 7z Best on make  (beaten by Default, Max)
  saved: Ultra vs Zipgeist tar.xz Best on make  (beaten by Default, Max)
  mixed: Default vs Zipgeist ZIP Normal on make  (beaten by Fast)
  mixed: Ultra vs WinRAR Best (-m5, solid) on make  (beaten by Default, Max)
  mixed: Ultra vs Zipgeist tar.xz Best on make  (beaten by Default, Max)
  dmgs: Fast vs WinRAR Fastest (-m1) on size  (beaten by Default)
  dmgs: Default vs Zipgeist ZIP Normal on make  (beaten by Fast)
  dmgs: Max vs 7-Zip Ultra (-mx9) on make, extract  (beaten by Default)
  dmgs: Max vs tar + zstd -19 --long on make, extract  (beaten by Default)
  dmgs: Max vs tar + xz -9e on extract  (beaten by Default)
  dmgs: Max vs WinRAR Best (-m5, solid) on make, extract  (beaten by Default)
  dmgs: Max vs Zipgeist 7z Best on make, extract  (beaten by Default)
  dmgs: Max vs Zipgeist tar.xz Best on make, extract  (beaten by Default)
  dmgs: Ultra vs 7-Zip Ultra (-mx9) on make, extract  (beaten by Default)
  dmgs: Ultra vs tar + zstd -19 --long on make, extract  (beaten by Default)
  dmgs: Ultra vs tar + xz -9e on extract  (beaten by Default)
  dmgs: Ultra vs WinRAR Best (-m5, solid) on make, extract  (beaten by Default)
  dmgs: Ultra vs Zipgeist 7z Best on make, extract  (beaten by Default)
  dmgs: Ultra vs Zipgeist tar.xz Best on make, extract  (beaten by Default)
  thirdparty: Default vs 7-Zip normal (-mx5) on size  (beaten by Max, Ultra)
  thirdparty: Default vs Zipgeist 7z Normal on size  (beaten by Max)
  thirdparty: Default vs tar + xz -6 on size  (beaten by Max)
  thirdparty: Ultra vs WinRAR Best (-m5, solid) on make  (beaten by Max)
  thirdparty: Ultra vs Zipgeist 7z Best on make  (beaten by Max)
  thirdparty: Ultra vs Zipgeist tar.xz Best on make  (beaten by Max)

Upsets: a lower Arca level beating a rival of a higher class on all three
  jpeg        Fast     beats Zipgeist ZIP Normal          9.5% smaller,   1.5x faster make,   6.8x faster extract
  jpeg        Fast     beats WinRAR Normal (-m3)          2.0% smaller,  20.6x faster make,   4.8x faster extract
  jpeg        Default  beats 7-Zip Ultra (-mx9)          18.9% smaller,   3.2x faster make,   2.3x faster extract
  jpeg        Default  beats tar + xz -9e                19.2% smaller,  19.3x faster make,   2.8x faster extract
  jpeg        Default  beats Zipgeist 7z Best            19.6% smaller,   1.3x faster make,   2.5x faster extract
  jpeg        Default  beats Zipgeist tar.xz Best        20.1% smaller,   1.5x faster make,   1.5x faster extract
  docs        Fast     beats Zipgeist ZIP Normal         53.3% smaller,   1.7x faster make,   4.9x faster extract
  docs        Fast     beats WinRAR Normal (-m3)         17.5% smaller,  14.3x faster make,   3.1x faster extract
  docs        Fast     beats tar + xz -6                  5.1% smaller,  55.0x faster make,   5.4x faster extract
  docs        Default  beats 7-Zip Ultra (-mx9)          42.1% smaller,   2.1x faster make,   3.2x faster extract
  docs        Default  beats tar + zstd -19 --long       43.0% smaller,   4.5x faster make,   1.6x faster extract
  docs        Default  beats tar + xz -9e                42.9% smaller,  15.1x faster make,   4.4x faster extract
  docs        Default  beats WinRAR Best (-m5, solid)    42.8% smaller,   1.3x faster make,   1.2x faster extract
  docs        Default  beats Zipgeist 7z Best            42.7% smaller,   1.4x faster make,   6.6x faster extract
  docs        Default  beats Zipgeist tar.xz Best        43.0% smaller,   1.3x faster make,   4.0x faster extract
  source      Fast     beats Zipgeist ZIP Normal         36.1% smaller,   2.0x faster make,   1.7x faster extract
  source      Fast     beats WinRAR Normal (-m3)         25.4% smaller,   9.7x faster make,   2.8x faster extract
  apps        Fast     beats Zipgeist ZIP Normal         16.3% smaller,   7.7x faster make,   3.3x faster extract
  apps        Default  beats tar + zstd -19 --long        4.9% smaller,  15.4x faster make,   4.4x faster extract
  apps        Default  beats WinRAR Best (-m5, solid)     3.8% smaller,   3.9x faster make,   2.7x faster extract
  apps        Default  beats Zipgeist 7z Best             4.8% smaller,   4.4x faster make,   3.6x faster extract
  apps        Default  beats Zipgeist tar.xz Best         3.8% smaller,   4.7x faster make,   4.6x faster extract
  silesia     Fast     beats Zipgeist ZIP Normal          4.9% smaller,   6.4x faster make,   6.1x faster extract
  saved       Fast     beats Zipgeist ZIP Normal         18.5% smaller,   4.1x faster make,   1.7x faster extract
  saved       Fast     beats WinRAR Normal (-m3)          0.3% smaller,  16.2x faster make,   3.2x faster extract
  saved       Default  beats 7-Zip Ultra (-mx9)           9.2% smaller,   6.6x faster make,   2.3x faster extract
  saved       Default  beats tar + zstd -19 --long       12.0% smaller,   5.0x faster make,   6.1x faster extract
  saved       Default  beats tar + xz -9e                10.5% smaller,  10.6x faster make,   6.1x faster extract
  saved       Default  beats WinRAR Best (-m5, solid)    12.7% smaller,   2.4x faster make,   1.8x faster extract
  saved       Default  beats Zipgeist 7z Best            12.5% smaller,   2.2x faster make,   1.9x faster extract
  saved       Default  beats Zipgeist tar.xz Best        12.7% smaller,   2.2x faster make,   1.7x faster extract
  mixed       Fast     beats Zipgeist ZIP Normal         22.5% smaller,   4.8x faster make,   1.9x faster extract
  mixed       Default  beats 7-Zip Ultra (-mx9)           8.1% smaller,   5.8x faster make,   2.6x faster extract
  mixed       Default  beats tar + zstd -19 --long       12.1% smaller,   5.5x faster make,   3.7x faster extract
  mixed       Default  beats tar + xz -9e                 8.7% smaller,  10.5x faster make,   4.8x faster extract
  mixed       Default  beats WinRAR Best (-m5, solid)    12.7% smaller,   2.6x faster make,   2.0x faster extract
  mixed       Default  beats Zipgeist 7z Best            11.4% smaller,   3.4x faster make,   2.2x faster extract
  mixed       Default  beats Zipgeist tar.xz Best        11.4% smaller,   3.2x faster make,   1.9x faster extract
  dmgs        Fast     beats Zipgeist ZIP Normal          0.8% smaller,   1.1x faster make,   3.2x faster extract
  dmgs        Default  beats 7-Zip Ultra (-mx9)           1.3% smaller,  18.5x faster make,   5.3x faster extract
  dmgs        Default  beats tar + zstd -19 --long        1.4% smaller,  34.5x faster make,   1.3x faster extract
  dmgs        Default  beats tar + xz -9e                 1.5% smaller,  97.3x faster make,   7.9x faster extract
  dmgs        Default  beats WinRAR Best (-m5, solid)     1.7% smaller,  16.9x faster make,   4.4x faster extract
  dmgs        Default  beats Zipgeist 7z Best             1.6% smaller,   8.0x faster make,   4.3x faster extract
  dmgs        Default  beats Zipgeist tar.xz Best         1.6% smaller,   8.0x faster make,   5.0x faster extract
  thirdparty  Fast     beats Zipgeist ZIP Normal          6.7% smaller,  12.1x faster make,   4.8x faster extract
```

## Notes on the run

- Load gates. A set's runs start when the 1-minute load is under 2.5, waiting up to ten minutes.
  Gates in the main run: Photos 2.4, PDFs and screenshots 2.2, Source code 2.3, Apple apps 2.4, Silesia 2.3, Saved folder 2.5, Mixed folder 2.2, Disk images 3.7, Third-party apps 3.7. The last two sets went on after the wait at 3.7: an animated wallpaper and a background app kept the idle load there; their Zipling rows are within 1 to 5% of the quiet engine run at Default, Max and Ultra* and their rival rows within 7% of the quiet runs of 30 October, so they stand.
  Second pass (zstd and xz) gates, threshold 4.0: Photos 3.1, PDFs and screenshots 3.6, Source code 3.3, Apple apps 3.7, Silesia 3.6, Saved folder 3.8, Mixed folder 3.8, Disk images 3.9, Third-party apps 3.7.
- The log's clock jumps back one hour between the disk images and the third-party apps: daylight
  saving time ended at 02:00 on 1 November. The run took about two and a half hours.
- Every set is read once (tar to /dev/null) before its runs, so every tool starts with the files in
  the page cache; Zipling and the rivals alternate within a set; the second round runs in reverse order.
- The private sets (Saved folder, disk images, third-party apps) are the user's own files and stay
  local; the website shows the six public sets.
