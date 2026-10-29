# Checkpoint benchmark, 2026-10-29

The user's spec is in `docs/RESEARCH.md` ("Checkpoint benchmark"); the findings
and what to fix are under "Checkpoint results" there. This file is the full set
of tables.

**Machine:** Apple M1 Pro (8P + 2E cores), 32 GB, macOS 27.0.1, warm page cache.
**Arca:** release build of f30182a. **Rivals:** 7-Zip 26.05 (`7zz`), zstd 1.5.7,
XZ Utils 5.8.4, `ditto` (what Finder's Compress uses), RAR 7.23 for macOS
(WinRAR's engine, from rarlab.com, with the user's OK), and Zipgeist 1.0.0
with every setting it offers (ZIP Fast and Normal, tar.zst Fast, 7z Normal and
Best, tar.xz Best). Keka and BetterZip weren't installed; both make 7z with the
7-Zip engine, so the 7-Zip rows stand for them.

**Method:** `bench/checkpoint.py`. Two rounds, Arca and rivals alternating, the
second round in reverse order; each time is the better of the two, each peak
memory the higher. Every archive was extracted and compared with its source
(names, types, bytes, link targets): **every round trip was identical**.
Zipgeist was driven through Finder (Compress As > Options…, and Extract), one
round, timed from the click to the job's last activity or its output's last
write. Its memory is the footprint of all its processes, app included, so it
isn't comparable with the command-line tools' maximum resident size.

**Sessions:** main run 09:25-11:36 (every Arca level and the command-line
rivals); Zipgeist after it; WinRAR 13:14-14:16; the 7-Zip re-run on the
third-party apps 14:19-14:27. The main run's 7-Zip extraction of the
third-party apps skipped links that point through other links ("Dangerous link
via another link was ignored"); the re-run extracts with `-snld20`, which
restores them.

Regenerate the tables with:

    python3 -I bench/checkpoint_report.py bench/results-2026-10-29-checkpoint.jsonl \
        bench/results-2026-10-29-zipgeist.jsonl bench/results-2026-10-29-checkpoint-rar.jsonl \
        bench/results-2026-10-29-checkpoint-7z.jsonl

## Sessions

The main run timed every Arca level and the command-line rivals together. Zipgeist (driven through Finder), WinRAR and the 7-Zip re-run on the third-party apps ran in their own sessions, each with some Arca levels alongside. Their Arca runs against the main run's (geometric mean, all sets):

- `results-2026-10-29-zipgeist.jsonl`: 27 Arca runs, make ×1.30, extract ×1.18, sizes identical
- `results-2026-10-29-checkpoint-rar.jsonl`: 27 Arca runs, make ×1.01, extract ×1.00, sizes identical
- `results-2026-10-29-checkpoint-7z.jsonl`: 2 Arca runs, make ×1.01, extract ×1.00, sizes identical

The rivals from those sessions (marked †) have their times divided by that session's factor for the same set, which puts them on the main run's clock. Zipgeist's session ran slower (Finder and the screen were busy), so this makes its times shorter than measured: it errs in Zipgeist's favour. The raw times are in the results files.

## Top of every chart?

Per set, the best Arca setting against the best rival on each measure (✓ Arca leads, ✗ a rival does). DMG expansion counts as an Arca setting on the DMG set, since it is a switch at every level.

| Set | Smallest | Fastest to make | Fastest to extract | Least memory (make) | Least memory (extract) |
|---|---|---|---|---|---|
| jpeg | ✓ Max 71.10 vs 7-Zip normal (-mx5) 85.29 | ✓ Fast 0.11 vs Zipgeist ZIP Normal 0.17 | ✓ Fast 0.06 vs tar + zstd -3 0.27 | ✗ Fast 245 vs Finder (ditto zip) 3 | ✗ Fast+bsc 172 vs Finder (ditto zip) 2 |
| docs | ✓ Ultra 17.81 vs 7-Zip Ultra (-mx9) 38.25 | ✓ Fast+bsc 0.09 vs Zipgeist tar.zst Fast 0.17 | ✓ Fast+bsc 0.08 vs WinRAR Best (-m5, solid) 0.27 | ✗ Fast+bsc 181 vs Finder (ditto zip) 3 | ✗ Fast 126 vs Finder (ditto zip) 3 |
| source | ✓ Ultra+bsc 8.09 vs tar + xz -9e 8.34 | ✓ Fast 0.24 vs Zipgeist ZIP Normal 0.45 | ✓ Fast 0.39 vs Zipgeist tar.zst Fast 0.77 | ✗ Fast 242 vs Finder (ditto zip) 8 | ✗ Max 177 vs Finder (ditto zip) 5 |
| apps | ✓ Ultra 48.53 vs 7-Zip Ultra (-mx9) 54.10 | ✓ Fast+bsc 0.28 vs Zipgeist tar.zst Fast 0.34 | ✓ Fast 0.18 vs Zipgeist tar.zst Fast 0.67 | ✗ Fast 396 vs Finder (ditto zip) 5 | ✗ Fast 273 vs Finder (ditto zip) 3 |
| silesia | ✓ Ultra+bsc 45.92 vs tar + xz -9e 48.42 | ✗ Fast 0.24 vs tar + zstd -3 0.15 | ✓ Fast 0.08 vs tar + zstd -3 0.21 | ✗ Fast 295 vs Finder (ditto zip) 2 | ✗ Fast 161 vs Finder (ditto zip) 2 |
| saved | ✓ Ultra+bsc 220.5 vs 7-Zip Ultra (-mx9) 274.0 | ✓ Fast 0.73 vs Zipgeist tar.zst Fast 0.97 | ✓ Fast 0.80 vs Zipgeist tar.zst Fast 1.24 | ✗ Fast 926 vs Finder (ditto zip) 12 | ✗ Fast 380 vs Finder (ditto zip) 7 |
| mixed | ✓ Ultra+bsc 191.2 vs 7-Zip Ultra (-mx9) 235.0 | ✓ Fast 0.72 vs Zipgeist tar.zst Fast 0.84 | ✓ Fast 0.67 vs Zipgeist tar.zst Fast 1.27 | ✗ Fast 836 vs Finder (ditto zip) 10 | ✗ Fast 327 vs Finder (ditto zip) 6 |
| dmgs | ✓ Ultra+bsc 447.9 vs 7-Zip Ultra (-mx9) 609.6 | ✗ Fast 0.69 vs Zipgeist tar.zst Fast 0.25 | ✓ Fast+bsc 0.19 vs tar + zstd -3 0.43 | ✗ Fast 762 vs Finder (ditto zip) 2 | ✗ Fast+bsc 238 vs Finder (ditto zip) 2 |
| thirdparty | ✓ Ultra 336.2 vs 7-Zip Ultra (-mx9) 355.5 | ✗ Fast 2.19 vs Zipgeist tar.zst Fast 0.55 | ✓ Default 0.77 vs Zipgeist tar.zst Fast 1.57 | ✗ Fast 1703 vs Finder (ditto zip) 5 | ✗ Fast 894 vs Finder (ditto zip) 3 |

## Every rival setting against one Arca setting

For each rival and set: the first Arca setting (Fast → Ultra + bsc) that is at least as small, faster to make and faster to extract, all at once. ✗ when none is, with the nearest miss: of the Arca settings at least as small, the one closest on time, and how much slower it is.

| Rival | jpeg | docs | source | apps | silesia | saved | mixed | dmgs | thirdparty |
|---|---|---|---|---|---|---|---|---|---|
| Finder (ditto zip) | ✗ Ultra: extract ×2.27 | Fast | Fast | Fast | Fast | Fast | Fast | ✗ Default+DMG: make ×1.05, extract ×5.42 | Fast |
| 7-Zip normal (-mx5) | Default | Default | Max+bsc | Max | Max+bsc | Default | Default | ✗ Default+DMG: make ×1.29, extract ×2.28 | Max † |
| 7-Zip Ultra (-mx9) | Default | Default | Max+bsc | Max | Max | Default | Default | ✗ Default+DMG: extract ×1.75 | Max † |
| tar + zstd -3 | ✗ Ultra+bsc: make ×8.65, extract ×4.37 | Fast | Fast | Fast | ✗ Fast: make ×1.58 | Fast | Fast | ✗ Fast+DMG: make ×28.21, extract ×11.04 | ✗ Fast: make ×1.44 |
| tar + zstd -19 --long | ✗ Ultra: extract ×4.15 | Default | Max | Max | ✗ Max: extract ×3.04 | Default | Default | ✗ Default+DMG: extract ×6.94 | Max |
| tar + xz -6 | ✗ Ultra: extract ×1.73 | Fast | Max+bsc | ✗ Max+bsc: make ×1.43 | ✗ Max+bsc: make ×1.01, extract ×1.73 | Default | Default | ✗ Default+DMG: extract ×2.59 | Max |
| tar + xz -9e | Default | Default | Max | Max | Max+bsc | Default | Default | ✗ Default+DMG: extract ×1.16 | Max |
| Zipgeist ZIP Fast † | Fast | Fast | Fast | Fast | Fast | Fast | Fast | ✗ Fast: make ×2.65 | Fast |
| Zipgeist tar.zst Fast † | ✗ Ultra+bsc: make ×11.84, extract ×2.91 | Fast | Fast | Fast | ✗ Fast: make ×1.43 | Fast | Fast | ✗ Fast+DMG: make ×30.36, extract ×8.24 | ✗ Fast: make ×3.96 |
| Zipgeist ZIP Normal † | Fast | Fast | Fast | Fast | Fast | Fast | Fast | ✗ Fast: make ×2.72 | Fast |
| Zipgeist 7z Normal † | Default | ✗ Default: make ×1.16 | Default+bsc | ✗ Max+bsc: make ×2.26 | ✗ Max+bsc: extract ×1.26 | Default | Default | ✗ Default+DMG: make ×1.73, extract ×2.59 | ✗ Max+bsc: make ×1.05 |
| Zipgeist 7z Best † | Default | ✗ Default: make ×1.05 | ✗ Max+bsc: extract ×1.01 | ✗ Max+bsc: make ×2.09 | ✗ Max+bsc: extract ×1.26 | Default | Default | ✗ Default+DMG: make ×1.71, extract ×2.57 | Max |
| Zipgeist tar.xz Best † | ✗ Ultra: extract ×1.33 | ✗ Default: make ×1.14 | ✗ Max+bsc: extract ×1.03 | ✗ Max+bsc: make ×2.00 | Max+bsc | Default | Default | ✗ Default+DMG: make ×1.72, extract ×2.15 | Max+bsc |
| WinRAR Fastest (-m1) † | ✗ Ultra: make ×2.00, extract ×3.64 | Fast | Fast | Fast | Fast | Fast | Fast | ✗ Default+DMG: make ×3.55, extract ×2.32 | Fast |
| WinRAR Normal (-m3) † | ✗ Ultra: make ×1.94, extract ×3.01 | Fast | Fast | Default | ✗ Default+bsc: extract ×1.20 | Default | Default | ✗ Default+DMG: extract ×2.18 | Default |
| WinRAR Best (-m5, solid) † | ✗ Ultra: extract ×2.95 | ✗ Default: make ×1.10 | Default+bsc | ✗ Max+bsc: make ×2.31, extract ×1.01 | ✗ Default+bsc: extract ×1.22 | Default | Default | ✗ Default+DMG: extract ×2.18 | ✗ Max+bsc: make ×1.02 |

Arca beats 96 of 144 rival results on size, make and extract at once.

## How .zlg compares with every rival (all sets)

Geometric mean over the sets of Arca ÷ rival: under 1.00 means Arca is smaller or faster. Size / make time / extraction time.

| | Fast | Fast+bsc | Default | Default+bsc | Max | Max+bsc | Ultra | Ultra+bsc |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Finder (ditto zip) | 0.86 / 0.05 / 0.14 | 0.82 / 0.06 / 0.19 | 0.69 / 0.41 / 0.28 | 0.67 / 0.33 / 0.35 | 0.57 / 2.54 / 0.65 | 0.56 / 1.60 / 0.75 | 0.56 / 2.63 / 1.22 | 0.56 / 1.85 / 0.74 |
| 7-Zip normal (-mx5) | 1.21 / 0.03 / 0.16 | 1.16 / 0.03 / 0.23 | 0.97 / 0.22 / 0.34 | 0.95 / 0.18 / 0.42 | 0.80 / 1.35 / 0.79 | 0.79 / 0.85 / 0.90 | 0.79 / 1.39 / 1.47 | 0.79 / 0.98 / 0.90 |
| 7-Zip Ultra (-mx9) | 1.23 / 0.02 / 0.13 | 1.18 / 0.02 / 0.19 | 0.99 / 0.13 / 0.27 | 0.97 / 0.11 / 0.34 | 0.82 / 0.83 / 0.63 | 0.81 / 0.52 / 0.73 | 0.81 / 0.85 / 1.19 | 0.81 / 0.60 / 0.72 |
| tar + zstd -3 | 0.96 / 0.37 / 0.17 | 0.92 / 0.48 / 0.24 | 0.77 / 3.12 / 0.35 | 0.75 / 2.56 / 0.44 | 0.64 / 19.45 / 0.82 | 0.63 / 12.21 / 0.94 | 0.63 / 20.10 / 1.53 | 0.63 / 14.15 / 0.93 |
| tar + zstd -19 --long | 1.18 / 0.01 / 0.17 | 1.13 / 0.02 / 0.24 | 0.94 / 0.12 / 0.35 | 0.92 / 0.10 / 0.43 | 0.78 / 0.73 / 0.80 | 0.77 / 0.46 / 0.92 | 0.77 / 0.75 / 1.51 | 0.77 / 0.53 / 0.92 |
| tar + xz -6 | 1.15 / 0.03 / 0.12 | 1.10 / 0.03 / 0.17 | 0.92 / 0.22 / 0.24 | 0.90 / 0.18 / 0.30 | 0.76 / 1.40 / 0.56 | 0.75 / 0.88 / 0.64 | 0.75 / 1.44 / 1.04 | 0.75 / 1.02 / 0.63 |
| tar + xz -9e | 1.23 / 0.01 / 0.07 | 1.18 / 0.01 / 0.10 | 0.98 / 0.05 / 0.14 | 0.96 / 0.04 / 0.18 | 0.81 / 0.30 / 0.33 | 0.81 / 0.19 / 0.38 | 0.81 / 0.31 / 0.61 | 0.80 / 0.22 / 0.37 |
| Zipgeist ZIP Fast | 0.76 / 0.65 / 0.31 | 0.73 / 0.84 / 0.43 | 0.61 / 5.43 / 0.63 | 0.60 / 4.46 / 0.79 | 0.50 / 33.89 / 1.47 | 0.50 / 21.27 / 1.69 | 0.50 / 35.01 / 2.75 | 0.50 / 24.66 / 1.67 |
| Zipgeist tar.zst Fast | 0.90 / 1.04 / 0.34 | 0.87 / 1.33 / 0.49 | 0.72 / 8.67 / 0.71 | 0.71 / 7.11 / 0.88 | 0.60 / 54.05 / 1.64 | 0.59 / 33.92 / 1.89 | 0.59 / 55.84 / 3.08 | 0.59 / 39.33 / 1.87 |
| Zipgeist ZIP Normal | 0.80 / 0.52 / 0.31 | 0.77 / 0.67 / 0.45 | 0.64 / 4.38 / 0.65 | 0.63 / 3.59 / 0.81 | 0.53 / 27.32 / 1.51 | 0.53 / 17.14 / 1.73 | 0.52 / 28.22 / 2.82 | 0.52 / 19.88 / 1.71 |
| Zipgeist 7z Normal | 1.17 / 0.05 / 0.17 | 1.12 / 0.06 / 0.25 | 0.94 / 0.40 / 0.36 | 0.91 / 0.33 / 0.45 | 0.77 / 2.50 / 0.84 | 0.77 / 1.57 / 0.96 | 0.76 / 2.58 / 1.57 | 0.76 / 1.82 / 0.95 |
| Zipgeist 7z Best | 1.17 / 0.04 / 0.18 | 1.13 / 0.06 / 0.25 | 0.94 / 0.36 / 0.36 | 0.92 / 0.30 / 0.45 | 0.78 / 2.25 / 0.84 | 0.77 / 1.41 / 0.97 | 0.77 / 2.33 / 1.58 | 0.77 / 1.64 / 0.96 |
| Zipgeist tar.xz Best | 1.19 / 0.04 / 0.18 | 1.14 / 0.06 / 0.26 | 0.95 / 0.36 / 0.37 | 0.93 / 0.30 / 0.46 | 0.79 / 2.26 / 0.86 | 0.78 / 1.42 / 0.99 | 0.78 / 2.34 / 1.62 | 0.78 / 1.65 / 0.98 |
| WinRAR Fastest (-m1) | 0.90 / 0.17 / 0.24 | 0.86 / 0.22 / 0.34 | 0.72 / 1.45 / 0.49 | 0.70 / 1.19 / 0.61 | 0.60 / 9.04 / 1.14 | 0.59 / 5.67 / 1.31 | 0.59 / 9.34 / 2.13 | 0.59 / 6.57 / 1.30 |
| WinRAR Normal (-m3) | 1.01 / 0.08 / 0.22 | 0.96 / 0.10 / 0.31 | 0.81 / 0.68 / 0.46 | 0.79 / 0.55 / 0.57 | 0.67 / 4.21 / 1.06 | 0.66 / 2.64 / 1.22 | 0.66 / 4.35 / 1.99 | 0.66 / 3.07 / 1.21 |
| WinRAR Best (-m5, solid) | 1.16 / 0.04 / 0.24 | 1.11 / 0.06 / 0.34 | 0.93 / 0.37 / 0.49 | 0.91 / 0.30 / 0.61 | 0.77 / 2.31 / 1.13 | 0.76 / 1.45 / 1.30 | 0.76 / 2.39 / 2.12 | 0.75 / 1.68 / 1.29 |

## Photos (400 JPEGs)

Load during the main run: 2.5–10.8.

| Tool | Size MB | Make s | Extract s | Make CPU s | Extract CPU s | Make peak MB | Extract peak MB | Identical | Rival ahead |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| Arca Fast | 95.10 ✗ | 0.11 | 0.06 | 0.29 | 0.33 | 245 ✗ | 172 ✗ | yes | size: 7-Zip normal (-mx5); make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Fast + bsc | 95.10 ✗ | 0.12 | 0.06 | 0.27 | 0.33 | 254 ✗ | 172 ✗ | yes | size: 7-Zip normal (-mx5); make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default | 71.10 | 2.23 ✗ | 1.20 ✗ | 19.5 | 9.64 | 1572 ✗ | 448 ✗ | yes | make: Zipgeist ZIP Normal; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default + bsc | 71.10 | 2.23 ✗ | 1.17 ✗ | 19.3 | 10.0 | 1563 ✗ | 429 ✗ | yes | make: Zipgeist ZIP Normal; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max | 71.10 | 2.23 ✗ | 1.17 ✗ | 19.2 | 9.88 | 1564 ✗ | 414 ✗ | yes | make: Zipgeist ZIP Normal; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max + bsc | 71.10 | 2.25 ✗ | 1.19 ✗ | 19.5 | 9.79 | 1565 ✗ | 425 ✗ | yes | make: Zipgeist ZIP Normal; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra | 71.10 | 2.23 ✗ | 1.16 ✗ | 19.6 | 10.0 | 1566 ✗ | 422 ✗ | yes | make: Zipgeist ZIP Normal; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra + bsc | 71.10 | 2.22 ✗ | 1.16 ✗ | 19.3 | 9.77 | 1544 ✗ | 453 ✗ | yes | make: Zipgeist ZIP Normal; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Finder (ditto zip) | 93.30 | 2.51 | 0.51 | 2.47 | 0.49 | 3 | 2 | yes |  |
| 7-Zip normal (-mx5) | 85.29 | 6.03 | 1.67 | 11.1 | 1.64 | 381 | 184 | yes |  |
| 7-Zip Ultra (-mx9) | 85.29 | 6.47 | 1.67 | 11.4 | 1.65 | 983 | 184 | yes |  |
| tar + zstd -3 | 91.72 | 0.26 | 0.27 | 0.48 | 0.32 | 118 | 7 | yes |  |
| tar + zstd -19 --long | 85.70 | 15.6 | 0.28 | 28.3 | 0.37 | 409 | 100 | yes |  |
| tar + xz -6 | 89.04 | 8.79 | 0.67 | 33.7 | 2.34 | 554 | 214 | yes |  |
| tar + xz -9e | 85.60 | 43.5 | 2.05 | 43.5 | 2.41 | 850 | 243 | yes |  |
| Zipgeist ZIP Fast † | 98.47 | 0.21 | 0.42 | 0.14 | 0.19 | 1107 | 1107 | yes |  |
| Zipgeist tar.zst Fast † | 92.06 | 0.19 | 0.40 | 0.22 | 0.23 | 1139 | 1216 | yes |  |
| Zipgeist ZIP Normal † | 98.47 | 0.17 | 0.43 | 0.11 | 0.20 | 1247 | 1247 | yes |  |
| Zipgeist 7z Normal † | 86.04 | 2.60 | 1.41 | 8.53 | 2.57 | 2006 | 2083 | yes |  |
| Zipgeist 7z Best † | 86.03 | 2.61 | 1.41 | 8.66 | 2.59 | 2131 | 2131 | yes |  |
| Zipgeist tar.xz Best † | 86.63 | 3.00 | 0.87 | 12.2 | 1.93 | 2610 | 2778 | yes |  |
| WinRAR Fastest (-m1) † | 92.51 | 1.12 | 0.32 | 3.30 | 0.93 | 104 | 29 | yes |  |
| WinRAR Normal (-m3) † | 90.93 | 1.15 | 0.38 | 5.16 | 1.01 | 131 | 38 | yes |  |
| WinRAR Best (-m5, solid) † | 86.21 | 2.91 | 0.39 | 15.3 | 1.01 | 571 | 42 | yes |  |

## PDFs and screenshots

Load during the main run: 2.3–4.9.

| Tool | Size MB | Make s | Extract s | Make CPU s | Extract CPU s | Make peak MB | Extract peak MB | Identical | Rival ahead |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| Arca Fast | 41.52 ✗ | 0.09 | 0.08 | 0.43 | 0.31 | 182 ✗ | 126 ✗ | yes | size: 7-Zip Ultra (-mx9); make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Fast + bsc | 41.52 ✗ | 0.09 | 0.08 | 0.45 | 0.31 | 181 ✗ | 126 ✗ | yes | size: 7-Zip Ultra (-mx9); make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default | 22.16 | 1.72 ✗ | 0.25 | 7.46 | 1.83 | 997 ✗ | 453 ✗ | yes | make: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default + bsc | 22.16 | 1.73 ✗ | 0.25 | 7.39 | 1.82 | 1002 ✗ | 445 ✗ | yes | make: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max | 18.20 | 9.70 ✗ | 0.41 ✗ | 26.8 | 2.70 | 1788 ✗ | 353 ✗ | yes | make: Zipgeist tar.zst Fast; extract: WinRAR Best (-m5, solid); make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max + bsc | 18.20 | 9.76 ✗ | 0.42 ✗ | 26.8 | 2.72 | 1767 ✗ | 342 ✗ | yes | make: Zipgeist tar.zst Fast; extract: WinRAR Best (-m5, solid); make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra | 17.81 | 14.1 ✗ | 0.41 ✗ | 39.6 | 2.71 | 1708 ✗ | 360 ✗ | yes | make: Zipgeist tar.zst Fast; extract: WinRAR Best (-m5, solid); make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra + bsc | 17.81 | 14.2 ✗ | 0.41 ✗ | 39.7 | 2.70 | 1692 ✗ | 351 ✗ | yes | make: Zipgeist tar.zst Fast; extract: WinRAR Best (-m5, solid); make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Finder (ditto zip) | 50.19 | 1.40 | 0.68 | 1.37 | 0.66 | 3 | 3 | yes |  |
| 7-Zip normal (-mx5) | 38.29 | 2.46 | 0.81 | 4.30 | 0.79 | 382 | 130 | yes |  |
| 7-Zip Ultra (-mx9) | 38.25 | 2.83 | 0.80 | 4.98 | 0.78 | 920 | 130 | yes |  |
| tar + zstd -3 | 45.07 | 0.40 | 0.43 | 0.49 | 0.45 | 106 | 7 | yes |  |
| tar + zstd -19 --long | 38.82 | 6.16 | 0.43 | 10.3 | 0.46 | 359 | 94 | yes |  |
| tar + xz -6 | 43.29 | 4.05 | 0.64 | 12.6 | 1.59 | 505 | 165 | yes |  |
| tar + xz -9e | 38.80 | 20.9 | 1.15 | 21.2 | 1.60 | 794 | 192 | yes |  |
| Zipgeist ZIP Fast † | 88.08 | 0.23 | 0.48 | 0.46 | 0.38 | 1998 | 1998 | yes |  |
| Zipgeist tar.zst Fast † | 45.24 | 0.17 | 0.43 | 0.15 | 0.33 | 1517 | 1606 | yes |  |
| Zipgeist ZIP Normal † | 87.89 | 0.21 | 0.49 | 0.57 | 0.38 | 1620 | 1620 | yes |  |
| Zipgeist 7z Normal † | 38.71 | 1.49 | 1.48 | 4.82 | 1.74 | 2556 | 2689 | yes |  |
| Zipgeist 7z Best † | 38.67 | 1.63 | 1.46 | 5.80 | 1.70 | 2689 | 2820 | yes |  |
| Zipgeist tar.xz Best † | 38.86 | 1.50 | 0.90 | 5.75 | 1.17 | 2912 | 3030 | yes |  |
| WinRAR Fastest (-m1) † | 50.04 | 0.77 | 0.36 | 3.28 | 0.66 | 99 | 45 | yes |  |
| WinRAR Normal (-m3) † | 49.75 | 0.95 | 0.32 | 3.45 | 0.60 | 126 | 45 | yes |  |
| WinRAR Best (-m5, solid) † | 38.74 | 1.56 | 0.27 | 7.02 | 0.54 | 567 | 41 | yes |  |

## Source code (89 Rust crates)

Load during the main run: 2.3–4.8.

| Tool | Size MB | Make s | Extract s | Make CPU s | Extract CPU s | Make peak MB | Extract peak MB | Identical | Rival ahead |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| Arca Fast | 13.55 ✗ | 0.24 | 0.39 | 1.68 | 2.10 | 242 ✗ | 179 ✗ | yes | size: tar + xz -9e; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Fast + bsc | 11.05 ✗ | 0.43 | 0.59 | 3.46 | 3.94 | 477 ✗ | 554 ✗ | yes | size: tar + xz -9e; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default | 11.20 ✗ | 1.95 ✗ | 0.41 | 8.79 | 2.16 | 635 ✗ | 179 ✗ | yes | size: tar + xz -9e; make: Zipgeist ZIP Normal; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default + bsc | 10.12 ✗ | 1.00 ✗ | 0.58 | 5.36 | 3.92 | 602 ✗ | 488 ✗ | yes | size: tar + xz -9e; make: Zipgeist ZIP Normal; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max | 8.23 | 22.0 ✗ | 0.52 | 70.9 | 2.42 | 1679 ✗ | 177 ✗ | yes | make: Zipgeist ZIP Normal; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max + bsc | 8.09 | 1.77 ✗ | 0.91 ✗ | 5.15 | 5.26 | 745 ✗ | 632 ✗ | yes | make: Zipgeist ZIP Normal; extract: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra | 8.23 | 21.9 ✗ | 3.39 ✗ | 54.2 | 5.31 | 1333 ✗ | 338 ✗ | yes | make: Zipgeist ZIP Normal; extract: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra + bsc | 8.09 | 1.77 ✗ | 0.92 ✗ | 5.14 | 5.35 | 706 ✗ | 632 ✗ | yes | make: Zipgeist ZIP Normal; extract: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Finder (ditto zip) | 24.81 | 4.98 | 6.36 | 4.78 | 6.28 | 8 | 5 | yes |  |
| 7-Zip normal (-mx5) | 9.15 | 13.0 | 1.17 | 18.8 | 1.18 | 794 | 170 | yes |  |
| 7-Zip Ultra (-mx9) | 8.51 | 21.9 | 1.17 | 27.9 | 1.14 | 1676 | 169 | yes |  |
| tar + zstd -3 | 15.09 | 3.66 | 3.59 | 3.90 | 3.63 | 111 | 7 | yes |  |
| tar + zstd -19 --long | 9.29 | 25.6 | 3.53 | 60.8 | 3.63 | 508 | 133 | yes |  |
| tar + xz -6 | 9.96 | 6.13 | 3.55 | 29.1 | 4.21 | 932 | 254 | yes |  |
| tar + xz -9e | 8.34 | 66.1 | 3.56 | 69.8 | 4.17 | 861 | 252 | yes |  |
| Zipgeist ZIP Fast † | 24.05 | 0.58 | 0.83 | 1.43 | 2.25 | 2277 | 1901 | yes |  |
| Zipgeist tar.zst Fast † | 16.99 | 0.48 | 0.77 | 0.71 | 2.22 | 1928 | 2031 | yes |  |
| Zipgeist ZIP Normal † | 21.20 | 0.45 | 0.81 | 1.71 | 2.20 | 2034 | 2034 | yes |  |
| Zipgeist 7z Normal † | 10.47 | 4.48 | 0.90 | 22.6 | 2.01 | 3786 | 3786 | yes |  |
| Zipgeist 7z Best † | 10.09 | 5.34 | 0.90 | 32.2 | 2.00 | 3774 | 3794 | yes |  |
| Zipgeist tar.xz Best † | 9.08 | 4.92 | 0.88 | 33.0 | 2.61 | 3793 | 3972 | yes |  |
| WinRAR Fastest (-m1) † | 21.82 | 1.69 | 1.43 | 3.87 | 1.47 | 47 | 42 | yes |  |
| WinRAR Normal (-m3) † | 18.15 | 2.32 | 1.42 | 6.34 | 1.45 | 59 | 42 | yes |  |
| WinRAR Best (-m5, solid) † | 10.15 | 4.35 | 1.23 | 15.2 | 1.27 | 552 | 40 | yes |  |

## Apple apps (8 from /System/Applications)

Load during the main run: 2.2–5.5.

| Tool | Size MB | Make s | Extract s | Make CPU s | Extract CPU s | Make peak MB | Extract peak MB | Identical | Rival ahead |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| Arca Fast | 72.45 ✗ | 0.29 | 0.18 | 1.33 | 0.99 | 396 ✗ | 273 ✗ | yes | size: 7-Zip Ultra (-mx9); make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Fast + bsc | 72.36 ✗ | 0.28 | 0.18 | 1.54 | 1.18 | 430 ✗ | 310 ✗ | yes | size: 7-Zip Ultra (-mx9); make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default | 62.43 ✗ | 2.08 ✗ | 0.24 | 5.79 | 1.58 | 767 ✗ | 527 ✗ | yes | size: 7-Zip Ultra (-mx9); make: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default + bsc | 62.38 ✗ | 2.07 ✗ | 0.25 | 5.77 | 1.61 | 739 ✗ | 512 ✗ | yes | size: 7-Zip Ultra (-mx9); make: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max | 49.16 | 12.1 ✗ | 0.70 ✗ | 35.0 | 2.77 | 2066 ✗ | 473 ✗ | yes | make: Zipgeist tar.zst Fast; extract: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max + bsc | 49.15 | 12.0 ✗ | 0.72 ✗ | 34.7 | 2.83 | 2072 ✗ | 491 ✗ | yes | make: Zipgeist tar.zst Fast; extract: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra | 48.53 | 14.7 ✗ | 0.73 ✗ | 46.4 | 2.85 | 2079 ✗ | 484 ✗ | yes | make: Zipgeist tar.zst Fast; extract: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra + bsc | 48.55 | 14.7 ✗ | 0.70 ✗ | 46.6 | 2.81 | 2067 ✗ | 489 ✗ | yes | make: Zipgeist tar.zst Fast; extract: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Finder (ditto zip) | 87.40 | 5.68 | 2.07 | 5.58 | 2.05 | 5 | 3 | yes |  |
| 7-Zip normal (-mx5) | 55.26 | 16.6 | 1.43 | 32.3 | 1.69 | 877 | 227 | yes |  |
| 7-Zip Ultra (-mx9) | 54.10 | 27.7 | 1.66 | 41.5 | 1.64 | 1759 | 225 | yes |  |
| tar + zstd -3 | 76.47 | 1.13 | 1.16 | 1.60 | 1.29 | 145 | 7 | yes |  |
| tar + zstd -19 --long | 59.36 | 20.8 | 1.16 | 48.7 | 1.32 | 547 | 133 | yes |  |
| tar + xz -6 | 58.97 | 8.35 | 1.27 | 47.2 | 3.41 | 938 | 292 | yes |  |
| tar + xz -9e | 54.31 | 58.6 | 2.38 | 59.7 | 3.19 | 899 | 290 | yes |  |
| Zipgeist ZIP Fast † | 91.82 | 0.52 | 0.75 | 1.79 | 0.77 | 2802 | 2801 | yes |  |
| Zipgeist tar.zst Fast † | 86.23 | 0.34 | 0.67 | 0.63 | 0.70 | 2013 | 2081 | yes |  |
| Zipgeist ZIP Normal † | 86.33 | 0.81 | 0.71 | 4.04 | 0.75 | 2113 | 2113 | yes |  |
| Zipgeist 7z Normal † | 59.58 | 5.29 | 0.91 | 40.6 | 2.10 | 3939 | 3987 | yes |  |
| Zipgeist 7z Best † | 59.30 | 5.72 | 0.90 | 45.6 | 2.11 | 3910 | 3970 | yes |  |
| Zipgeist tar.xz Best † | 58.65 | 5.97 | 1.13 | 46.0 | 2.32 | 3917 | 4147 | yes |  |
| WinRAR Fastest (-m1) † | 78.68 | 1.22 | 0.70 | 4.70 | 1.29 | 271 | 67 | yes |  |
| WinRAR Normal (-m3) † | 64.68 | 2.48 | 0.76 | 12.2 | 1.26 | 299 | 72 | yes |  |
| WinRAR Best (-m5, solid) † | 58.69 | 5.19 | 0.71 | 29.3 | 1.16 | 573 | 44 | yes |  |

## Silesia corpus

Load during the main run: 1.8–5.8.

| Tool | Size MB | Make s | Extract s | Make CPU s | Extract CPU s | Make peak MB | Extract peak MB | Identical | Rival ahead |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| Arca Fast | 65.67 ✗ | 0.24 ✗ | 0.08 | 1.49 | 0.43 | 295 ✗ | 161 ✗ | yes | size: tar + xz -9e; make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Fast + bsc | 58.07 ✗ | 0.58 ✗ | 0.55 ✗ | 4.10 | 4.89 | 594 ✗ | 629 ✗ | yes | size: tar + xz -9e; make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default | 57.32 ✗ | 4.81 ✗ | 0.17 | 22.3 | 0.41 | 729 ✗ | 263 ✗ | yes | size: tar + xz -9e; make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default + bsc | 53.00 ✗ | 2.32 ✗ | 0.55 ✗ | 7.71 | 4.89 | 787 ✗ | 782 ✗ | yes | size: tar + xz -9e; make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max | 48.63 ✗ | 23.5 ✗ | 0.69 ✗ | 85.8 | 1.77 | 2219 ✗ | 255 ✗ | yes | size: tar + xz -9e; make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max + bsc | 45.97 | 12.9 ✗ | 0.90 ✗ | 41.0 | 6.02 | 2007 ✗ | 641 ✗ | yes | make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra | 46.93 | 13.5 ✗ | 5.78 ✗ | 50.2 | 8.99 | 1673 ✗ | 497 ✗ | yes | make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra + bsc | 45.92 | 12.8 ✗ | 0.90 ✗ | 43.0 | 6.04 | 2008 ✗ | 641 ✗ | yes | make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Finder (ditto zip) | 68.23 | 5.47 | 0.28 | 5.44 | 0.26 | 2 | 2 | yes |  |
| 7-Zip normal (-mx5) | 49.60 | 22.4 | 0.98 | 58.1 | 1.57 | 904 | 257 | yes |  |
| 7-Zip Ultra (-mx9) | 48.69 | 47.7 | 1.52 | 67.5 | 1.50 | 2084 | 255 | yes |  |
| tar + zstd -3 | 66.25 | 0.15 | 0.21 | 0.84 | 0.39 | 154 | 7 | yes |  |
| tar + zstd -19 --long | 52.60 | 29.2 | 0.23 | 90.4 | 0.40 | 656 | 133 | yes |  |
| tar + xz -6 | 49.50 | 12.8 | 0.52 | 75.4 | 2.55 | 1088 | 325 | yes |  |
| tar + xz -9e | 48.42 | 82.6 | 2.02 | 86.0 | 2.37 | 1080 | 325 | yes |  |
| Zipgeist ZIP Fast † | 77.28 | 0.39 | 0.62 | 2.00 | 0.47 | 2006 | 2019 | yes |  |
| Zipgeist tar.zst Fast † | 73.31 | 0.17 | 0.56 | 0.56 | 0.33 | 2052 | 2110 | yes |  |
| Zipgeist ZIP Normal † | 68.53 | 0.86 | 0.57 | 5.95 | 0.42 | 2112 | 2112 | yes |  |
| Zipgeist 7z Normal † | 49.95 | 13.1 | 0.71 | 73.5 | 1.93 | 3922 | 3995 | yes |  |
| Zipgeist 7z Best † | 49.15 | 15.8 | 0.71 | 85.3 | 1.92 | 3968 | 4046 | yes |  |
| Zipgeist tar.xz Best † | 49.16 | 15.8 | 0.95 | 85.5 | 2.01 | 3961 | 4223 | yes |  |
| WinRAR Fastest (-m1) † | 66.68 | 0.90 | 0.41 | 4.77 | 0.98 | 271 | 43 | yes |  |
| WinRAR Normal (-m3) † | 54.21 | 3.53 | 0.46 | 22.3 | 0.92 | 297 | 44 | yes |  |
| WinRAR Best (-m5, solid) † | 53.12 | 6.15 | 0.46 | 41.2 | 0.90 | 572 | 44 | yes |  |

## Saved folder (the 660 MB test folder)

Load during the main run: 2.1–21.9.

| Tool | Size MB | Make s | Extract s | Make CPU s | Extract CPU s | Make peak MB | Extract peak MB | Identical | Rival ahead |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| Arca Fast | 315.9 ✗ | 0.73 | 0.80 | 5.46 | 4.56 | 926 ✗ | 380 ✗ | yes | size: 7-Zip Ultra (-mx9); make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Fast + bsc | 313.3 ✗ | 0.88 | 0.97 | 6.91 | 6.32 | 1150 ✗ | 687 ✗ | yes | size: 7-Zip Ultra (-mx9); make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default | 257.4 | 8.02 ✗ | 2.03 ✗ | 58.8 | 16.3 | 4006 ✗ | 1435 ✗ | yes | make: Zipgeist tar.zst Fast; extract: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default + bsc | 256.2 | 7.76 ✗ | 2.28 ✗ | 53.8 | 19.3 | 4013 ✗ | 1809 ✗ | yes | make: Zipgeist tar.zst Fast; extract: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max | 222.0 | 30.0 ✗ | 2.74 ✗ | 209.1 | 23.6 | 6508 ✗ | 1041 ✗ | yes | make: Zipgeist tar.zst Fast; extract: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max + bsc | 221.8 | 17.8 ✗ | 3.03 ✗ | 134.6 | 26.5 | 7077 ✗ | 1505 ✗ | yes | make: Zipgeist tar.zst Fast; extract: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra | 220.6 | 32.3 ✗ | 4.53 ✗ | 235.5 | 27.8 | 6687 ✗ | 1239 ✗ | yes | make: Zipgeist tar.zst Fast; extract: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra + bsc | 220.5 | 22.1 ✗ | 3.04 ✗ | 182.0 | 26.6 | 7053 ✗ | 1490 ✗ | yes | make: Zipgeist tar.zst Fast; extract: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Finder (ditto zip) | 355.6 | 19.0 | 11.6 | 18.6 | 11.4 | 12 | 7 | yes |  |
| 7-Zip normal (-mx5) | 276.4 | 21.1 | 2.85 | 103.9 | 6.75 | 2553 | 923 | yes |  |
| 7-Zip Ultra (-mx9) | 274.0 | 52.7 | 4.01 | 127.7 | 6.64 | 6623 | 920 | yes |  |
| tar + zstd -3 | 328.2 | 6.66 | 10.6 | 8.03 | 11.1 | 155 | 7 | yes |  |
| tar + zstd -19 --long | 282.7 | 40.4 | 10.6 | 235.4 | 11.2 | 1834 | 133 | yes |  |
| tar + xz -6 | 292.9 | 22.2 | 10.8 | 185.9 | 18.9 | 1395 | 699 | yes |  |
| tar + xz -9e | 278.0 | 85.1 | 10.7 | 273.7 | 18.4 | 3624 | 1192 | yes |  |
| Zipgeist ZIP Fast † | 393.1 | 1.53 | 1.44 | 5.29 | 3.84 | 3153 | 2368 | yes |  |
| Zipgeist tar.zst Fast † | 341.0 | 0.97 | 1.24 | 2.05 | 3.78 | 2393 | 2601 | yes |  |
| Zipgeist ZIP Normal † | 381.8 | 1.59 | 1.42 | 8.38 | 3.85 | 2638 | 2638 | yes |  |
| Zipgeist 7z Normal † | 285.2 | 12.8 | 2.64 | 104.8 | 10.1 | 4529 | 4858 | yes |  |
| Zipgeist 7z Best † | 284.2 | 15.2 | 2.58 | 124.4 | 9.93 | 4728 | 5031 | yes |  |
| Zipgeist tar.xz Best † | 284.9 | 15.5 | 2.40 | 130.9 | 9.54 | 5093 | 5370 | yes |  |
| WinRAR Fastest (-m1) † | 339.5 | 5.36 | 3.20 | 18.0 | 5.36 | 280 | 76 | yes |  |
| WinRAR Normal (-m3) † | 312.3 | 8.71 | 3.30 | 36.8 | 5.29 | 309 | 76 | yes |  |
| WinRAR Best (-m5, solid) † | 285.0 | 19.4 | 3.07 | 101.7 | 5.23 | 579 | 44 | yes |  |

## Website mixed folder (all five sets)

Load during the main run: 2.5–15.6.

| Tool | Size MB | Make s | Extract s | Make CPU s | Extract CPU s | Make peak MB | Extract peak MB | Identical | Rival ahead |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| Arca Fast | 288.3 ✗ | 0.72 | 0.67 | 5.65 | 3.87 | 836 ✗ | 327 ✗ | yes | size: 7-Zip Ultra (-mx9); make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Fast + bsc | 278.2 ✗ | 1.26 ✗ | 1.28 ✗ | 9.88 | 9.87 | 1249 ✗ | 938 ✗ | yes | size: 7-Zip Ultra (-mx9); make: Zipgeist tar.zst Fast; extract: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default | 223.7 | 8.61 ✗ | 1.81 ✗ | 67.8 | 15.0 | 3480 ✗ | 1092 ✗ | yes | make: Zipgeist tar.zst Fast; extract: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default + bsc | 218.3 | 5.89 ✗ | 2.51 ✗ | 47.3 | 21.5 | 3481 ✗ | 1786 ✗ | yes | make: Zipgeist tar.zst Fast; extract: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max | 194.8 | 41.0 ✗ | 2.25 ✗ | 264.1 | 19.0 | 6416 ✗ | 1019 ✗ | yes | make: Zipgeist tar.zst Fast; extract: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max + bsc | 192.2 | 24.5 ✗ | 2.95 ✗ | 170.2 | 25.6 | 6818 ✗ | 1716 ✗ | yes | make: Zipgeist tar.zst Fast; extract: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra | 191.9 | 36.1 ✗ | 7.07 ✗ | 235.1 | 32.1 | 6685 ✗ | 1411 ✗ | yes | make: Zipgeist tar.zst Fast; extract: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra + bsc | 191.2 | 29.3 ✗ | 2.95 ✗ | 202.1 | 25.1 | 6911 ✗ | 1732 ✗ | yes | make: Zipgeist tar.zst Fast; extract: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Finder (ditto zip) | 324.2 | 19.9 | 9.89 | 19.5 | 9.79 | 10 | 6 | yes |  |
| 7-Zip normal (-mx5) | 237.8 | 25.9 | 2.95 | 140.3 | 6.92 | 2523 | 953 | yes |  |
| 7-Zip Ultra (-mx9) | 235.0 | 47.6 | 3.85 | 166.6 | 6.87 | 7360 | 949 | yes |  |
| tar + zstd -3 | 294.6 | 5.53 | 5.55 | 7.29 | 6.07 | 175 | 7 | yes |  |
| tar + zstd -19 --long | 245.7 | 45.5 | 5.50 | 287.1 | 6.06 | 1871 | 133 | yes |  |
| tar + xz -6 | 251.2 | 24.9 | 5.77 | 215.6 | 14.2 | 1357 | 744 | yes |  |
| tar + xz -9e | 236.6 | 86.7 | 7.05 | 301.9 | 13.7 | 3656 | 1220 | yes |  |
| Zipgeist ZIP Fast † | 379.9 | 1.20 | 1.39 | 5.65 | 3.68 | 4398 | 3329 | yes |  |
| Zipgeist tar.zst Fast † | 313.8 | 0.84 | 1.27 | 2.08 | 3.57 | 2998 | 3103 | yes |  |
| Zipgeist ZIP Normal † | 362.6 | 1.84 | 1.32 | 12.2 | 3.60 | 3186 | 2950 | yes |  |
| Zipgeist 7z Normal † | 245.4 | 21.5 | 2.46 | 154.7 | 11.1 | 4922 | 5347 | yes |  |
| Zipgeist 7z Best † | 243.9 | 24.5 | 2.43 | 182.7 | 11.0 | 5537 | 5891 | yes |  |
| Zipgeist tar.xz Best † | 243.9 | 23.1 | 2.41 | 185.6 | 9.70 | 5681 | 6088 | yes |  |
| WinRAR Fastest (-m1) † | 309.8 | 5.62 | 3.16 | 19.7 | 5.37 | 278 | 76 | yes |  |
| WinRAR Normal (-m3) † | 277.8 | 10.6 | 3.25 | 49.5 | 5.24 | 306 | 76 | yes |  |
| WinRAR Best (-m5, solid) † | 247.4 | 21.4 | 2.94 | 116.3 | 4.89 | 578 | 44 | yes |  |

## Your DMGs (5 installers)

Load during the main run: 2.5–16.1.

| Tool | Size MB | Make s | Extract s | Make CPU s | Extract CPU s | Make peak MB | Extract peak MB | Identical | Rival ahead |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| Arca Fast | 619.7 ✗ | 0.69 ✗ | 0.20 | 1.61 | 1.11 | 762 ✗ | 245 ✗ | yes | size: 7-Zip Ultra (-mx9); make: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Fast + bsc | 619.7 ✗ | 0.69 ✗ | 0.19 | 1.61 | 1.13 | 782 ✗ | 238 ✗ | yes | size: 7-Zip Ultra (-mx9); make: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default | 619.7 ✗ | 0.74 ✗ | 0.22 | 1.64 | 1.20 | 1551 ✗ | 1178 ✗ | yes | size: 7-Zip Ultra (-mx9); make: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default + bsc | 619.7 ✗ | 0.74 ✗ | 0.22 | 1.66 | 1.21 | 1615 ✗ | 1178 ✗ | yes | size: 7-Zip Ultra (-mx9); make: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max | 450.6 | 47.2 ✗ | 4.34 ✗ | 377.7 | 39.2 | 8022 ✗ | 3705 ✗ | yes | make: Zipgeist tar.zst Fast; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max + bsc | 450.6 | 46.9 ✗ | 4.36 ✗ | 378.6 | 38.9 | 8051 ✗ | 3686 ✗ | yes | make: Zipgeist tar.zst Fast; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra | 447.9 | 55.2 ✗ | 4.29 ✗ | 459.1 | 39.0 | 8002 ✗ | 3685 ✗ | yes | make: Zipgeist tar.zst Fast; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra + bsc | 447.9 | 55.3 ✗ | 4.35 ✗ | 458.9 | 38.9 | 8159 ✗ | 3730 ✗ | yes | make: Zipgeist tar.zst Fast; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Fast + DMG expansion | 566.7 | 7.45 ✗ | 4.80 ✗ | 59.2 | 26.9 | 4068 ✗ | 2233 ✗ | yes | make: Zipgeist tar.zst Fast; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default + DMG expansion | 517.1 | 11.6 ✗ | 3.13 ✗ | 96.1 | 27.8 | 4886 ✗ | 3737 ✗ | yes | make: Zipgeist tar.zst Fast; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Finder (ditto zip) | 617.3 | 11.1 | 0.58 | 11.1 | 0.56 | 2 | 2 | yes |  |
| 7-Zip normal (-mx5) | 610.6 | 9.04 | 1.38 | 72.8 | 2.94 | 2805 | 1217 | yes |  |
| 7-Zip Ultra (-mx9) | 609.6 | 18.4 | 1.79 | 70.5 | 3.14 | 6564 | 1215 | yes |  |
| tar + zstd -3 | 616.1 | 0.26 | 0.43 | 0.79 | 0.64 | 164 | 7 | yes |  |
| tar + zstd -19 --long | 610.3 | 34.8 | 0.45 | 210.5 | 0.72 | 2053 | 133 | yes |  |
| tar + xz -6 | 612.5 | 29.0 | 1.21 | 229.5 | 4.97 | 1531 | 674 | yes |  |
| tar + xz -9e | 610.7 | 100.3 | 2.70 | 302.6 | 5.09 | 3399 | 1384 | yes |  |
| Zipgeist ZIP Fast † | 620.1 | 0.26 | 0.64 | 0.27 | 0.32 | 4923 | 4923 | yes |  |
| Zipgeist tar.zst Fast † | 617.6 | 0.25 | 0.58 | 0.49 | 0.51 | 4031 | 4335 | yes |  |
| Zipgeist ZIP Normal † | 620.1 | 0.25 | 0.65 | 0.24 | 0.33 | 4335 | 4336 | yes |  |
| Zipgeist 7z Normal † | 611.9 | 6.72 | 1.21 | 66.4 | 3.28 | 6264 | 6408 | yes |  |
| Zipgeist 7z Best † | 611.9 | 6.82 | 1.22 | 67.3 | 3.26 | 7096 | 7110 | yes |  |
| Zipgeist tar.xz Best † | 611.9 | 6.77 | 1.46 | 67.3 | 3.64 | 6956 | 7603 | yes |  |
| WinRAR Fastest (-m1) † | 615.2 | 3.27 | 1.35 | 16.7 | 5.52 | 269 | 40 | yes |  |
| WinRAR Normal (-m3) † | 612.3 | 18.1 | 1.44 | 123.3 | 5.63 | 297 | 40 | yes |  |
| WinRAR Best (-m5, solid) † | 612.3 | 16.8 | 1.44 | 107.2 | 5.64 | 571 | 41 | yes |  |

## Your third-party apps (6)

Load during the main run: 2.0–15.8.

| Tool | Size MB | Make s | Extract s | Make CPU s | Extract CPU s | Make peak MB | Extract peak MB | Identical | Rival ahead |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| Arca Fast | 465.9 ✗ | 2.19 ✗ | 1.48 | 9.56 | 3.57 | 1703 ✗ | 894 ✗ | yes | size: 7-Zip Ultra (-mx9); make: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Fast + bsc | 465.5 ✗ | 2.19 ✗ | 1.51 | 9.83 | 3.76 | 1717 ✗ | 938 ✗ | yes | size: 7-Zip Ultra (-mx9); make: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default | 411.7 ✗ | 7.06 ✗ | 0.77 | 52.1 | 3.98 | 3152 ✗ | 2152 ✗ | yes | size: 7-Zip Ultra (-mx9); make: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default + bsc | 411.5 ✗ | 7.17 ✗ | 0.80 | 51.1 | 4.30 | 3170 ✗ | 2202 ✗ | yes | size: 7-Zip Ultra (-mx9); make: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max | 339.5 | 48.8 ✗ | 1.93 ✗ | 365.4 | 14.4 | 6225 ✗ | 2048 ✗ | yes | make: Zipgeist tar.zst Fast; extract: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max + bsc | 339.5 | 47.2 ✗ | 1.95 ✗ | 361.0 | 14.4 | 6034 ✗ | 1967 ✗ | yes | make: Zipgeist tar.zst Fast; extract: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra | 336.2 | 58.3 ✗ | 1.91 ✗ | 468.5 | 14.8 | 6140 ✗ | 1895 ✗ | yes | make: Zipgeist tar.zst Fast; extract: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra + bsc | 336.2 | 58.3 ✗ | 1.87 ✗ | 473.5 | 14.7 | 6100 ✗ | 1963 ✗ | yes | make: Zipgeist tar.zst Fast; extract: Zipgeist tar.zst Fast; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Finder (ditto zip) | 536.2 | 37.7 | 3.50 | 37.5 | 3.47 | 5 | 3 | yes |  |
| 7-Zip normal (-mx5) † | 371.2 | 73.5 | 2.16 | 355.1 | 12.6 | 2511 | 1573 | yes |  |
| 7-Zip Ultra (-mx9) † | 355.5 | 88.1 | 3.52 | 455.4 | 11.7 | 10608 | 1555 | yes |  |
| tar + zstd -3 | 513.0 | 1.52 | 15.3 | 5.53 | 16.5 | 161 | 7 | yes |  |
| tar + zstd -19 --long | 382.7 | 70.9 | 15.2 | 544.1 | 16.6 | 1989 | 133 | yes |  |
| tar + xz -6 | 390.2 | 50.0 | 16.2 | 430.1 | 33.2 | 1325 | 740 | yes |  |
| tar + xz -9e | 364.2 | 100.7 | 17.3 | 558.7 | 31.5 | 5541 | 1976 | yes |  |
| Zipgeist ZIP Fast † | 582.1 | 2.48 | 1.99 | 15.6 | 3.22 | 6485 | 5570 | yes |  |
| Zipgeist tar.zst Fast † | 569.6 | 0.55 | 1.57 | 3.07 | 2.21 | 5581 | 6284 | yes |  |
| Zipgeist ZIP Normal † | 540.5 | 5.55 | 1.86 | 40.1 | 2.95 | 6299 | 6300 | yes |  |
| Zipgeist 7z Normal † | 380.0 | 45.0 | 3.24 | 402.4 | 20.8 | 8125 | 8106 | yes |  |
| Zipgeist 7z Best † | 377.9 | 48.9 | 3.16 | 446.7 | 20.8 | 8203 | 8854 | yes |  |
| Zipgeist tar.xz Best † | 377.2 | 48.5 | 3.78 | 448.9 | 14.2 | 8953 | 9425 | yes |  |
| WinRAR Fastest (-m1) † | 517.2 | 6.11 | 3.02 | 30.9 | 7.49 | 273 | 76 | yes |  |
| WinRAR Normal (-m3) † | 418.1 | 24.8 | 3.90 | 156.7 | 7.85 | 300 | 72 | yes |  |
| WinRAR Best (-m5, solid) † | 399.3 | 46.1 | 3.79 | 304.8 | 7.62 | 574 | 44 | yes |  |

## What changed against the website's numbers

The website (site/data.js) was measured on 2026-10-25/26; its Finder, zstd and xz rows in a separate session. Size MB / compress s / extract s, website → now.

**Website mixed folder (all five sets)**

| Tool | Size MB | Compress s | Extract s |
|---|---|---|---|
| Finder (ditto zip) | 322.7 → 324.2 (+0%) | 19.8 → 19.9 (+0%) | 9.32 → 9.89 (+6%) |
| 7-Zip Ultra (-mx9) | 233.4 → 235.0 (+1%) | 49.3 → 47.6 (-3%) | 3.84 → 3.85 (+0%) |
| tar + zstd -19 --long | 244.1 → 245.7 (+1%) | 46.5 → 45.5 (-2%) | 5.52 → 5.50 (-0%) |
| tar + xz -9e | 235.1 → 236.6 (+1%) | 97.9 → 86.7 (-11%) | 7.28 → 7.05 (-3%) |
| Arca Fast | 286.8 → 288.3 (+1%) | 1.4 → 0.72 (-49%) | 0.88 → 0.67 (-24%) |
| Arca Default | 221.5 → 223.7 (+1%) | 9.7 → 8.61 (-11%) | 2.3 → 1.81 (-21%) |
| Arca Max | 192.8 → 194.8 (+1%) | 51.6 → 41.0 (-20%) | 3.5 → 2.25 (-36%) |
| Arca Ultra | 191 → 191.9 (+0%) | 37.9 → 36.1 (-5%) | 7.45 → 7.07 (-5%) |

**PDFs and screenshots**

| Tool | Size MB | Compress s | Extract s |
|---|---|---|---|
| Finder (ditto zip) | 48.6 → 50.2 (+3%) | 1.5 → 1.40 (-6%) | 0.68 → 0.68 (-0%) |
| 7-Zip Ultra (-mx9) | 36.6 → 38.2 (+5%) | 2.7 → 2.83 (+5%) | 0.82 → 0.80 (-3%) |
| tar + zstd -19 --long | 37.1 → 38.8 (+5%) | 8 → 6.16 (-23%) | 0.42 → 0.43 (+2%) |
| tar + xz -9e | 36.9 → 38.8 (+5%) | 22.3 → 20.9 (-6%) | 1.1 → 1.15 (+4%) |
| Arca Fast | 40 → 41.5 (+4%) | 0.1 → 0.09 (-9%) | 0.09 → 0.08 (-6%) |
| Arca Default | 20.8 → 22.2 (+7%) | 2 → 1.72 (-14%) | 0.26 → 0.25 (-3%) |
| Arca Max | 16.5 → 18.2 (+10%) | 29.3 → 9.70 (-67%) | 1.2 → 0.41 (-66%) |
| Arca Ultra | 16.5 → 17.8 (+8%) | 29.4 → 14.1 (-52%) | 1.19 → 0.41 (-65%) |

**Photos (400 JPEGs)**

| Tool | Size MB | Compress s | Extract s |
|---|---|---|---|
| Finder (ditto zip) | 93.3 → 93.3 (-0%) | 2.5 → 2.51 (+0%) | 0.47 → 0.51 (+8%) |
| 7-Zip Ultra (-mx9) | 85.2 → 85.3 (+0%) | 6 → 6.47 (+8%) | 1.58 → 1.67 (+6%) |
| tar + zstd -19 --long | 85.7 → 85.7 (-0%) | 15.1 → 15.6 (+3%) | 0.27 → 0.28 (+3%) |
| tar + xz -9e | 85.8 → 85.6 (-0%) | 43.3 → 43.5 (+0%) | 2.04 → 2.05 (+1%) |
| Arca Fast | 95 → 95.1 (+0%) | 0.1 → 0.11 (+13%) | 0.06 → 0.06 (+5%) |
| Arca Default | 71 → 71.1 (+0%) | 2.4 → 2.23 (-7%) | 1.24 → 1.20 (-3%) |
| Arca Max | 71 → 71.1 (+0%) | 2.5 → 2.23 (-11%) | 1.25 → 1.17 (-7%) |
| Arca Ultra | 71 → 71.1 (+0%) | 2.5 → 2.23 (-11%) | 1.29 → 1.16 (-10%) |

**Apple apps (8 from /System/Applications)**

| Tool | Size MB | Compress s | Extract s |
|---|---|---|---|
| Finder (ditto zip) | 87.4 → 87.4 (+0%) | 5.8 → 5.68 (-2%) | 1.96 → 2.07 (+6%) |
| 7-Zip Ultra (-mx9) | 54.1 → 54.1 (+0%) | 27.6 → 27.7 (+0%) | 1.69 → 1.66 (-2%) |
| tar + zstd -19 --long | 59.4 → 59.4 (-0%) | 23.2 → 20.8 (-10%) | 1.15 → 1.16 (+1%) |
| tar + xz -9e | 54.4 → 54.3 (-0%) | 67 → 58.6 (-12%) | 2.43 → 2.38 (-2%) |
| Arca Fast | 72.4 → 72.5 (+0%) | 0.4 → 0.29 (-28%) | 0.22 → 0.18 (-16%) |
| Arca Default | 62.4 → 62.4 (+0%) | 2.4 → 2.08 (-13%) | 0.28 → 0.24 (-13%) |
| Arca Max | 49.8 → 49.2 (-1%) | 23.5 → 12.1 (-49%) | 1.04 → 0.70 (-32%) |
| Arca Ultra | 49.8 → 48.5 (-3%) | 23.5 → 14.7 (-37%) | 1.03 → 0.73 (-29%) |

**Source code (89 Rust crates)**

| Tool | Size MB | Compress s | Extract s |
|---|---|---|---|
| Finder (ditto zip) | 24.8 → 24.8 (+0%) | 5.2 → 4.98 (-4%) | 6.09 → 6.36 (+4%) |
| 7-Zip Ultra (-mx9) | 8.5 → 8.51 (+0%) | 22 → 21.9 (-1%) | 1.22 → 1.17 (-4%) |
| tar + zstd -19 --long | 9.3 → 9.29 (-0%) | 26.6 → 25.6 (-4%) | 3.45 → 3.53 (+2%) |
| tar + xz -9e | 8.3 → 8.34 (+1%) | 71.5 → 66.1 (-8%) | 3.55 → 3.56 (+0%) |
| Arca Fast | 13.5 → 13.5 (+0%) | 0.5 → 0.24 (-53%) | 0.54 → 0.39 (-27%) |
| Arca Default | 11.2 → 11.2 (+0%) | 2.1 → 1.95 (-7%) | 0.51 → 0.41 (-19%) |
| Arca Max | 8.3 → 8.23 (-1%) | 25.4 → 22.0 (-13%) | 0.72 → 0.52 (-28%) |
| Arca Ultra | 8.3 → 8.23 (-1%) | 24.9 → 21.9 (-12%) | 3.57 → 3.39 (-5%) |

**Silesia corpus**

| Tool | Size MB | Compress s | Extract s |
|---|---|---|---|
| Finder (ditto zip) | 68.2 → 68.2 (+0%) | 5.8 → 5.47 (-6%) | 0.3 → 0.28 (-8%) |
| 7-Zip Ultra (-mx9) | 48.7 → 48.7 (-0%) | 48.3 → 47.7 (-1%) | 1.53 → 1.52 (-0%) |
| tar + zstd -19 --long | 52.6 → 52.6 (-0%) | 32.9 → 29.2 (-11%) | 0.27 → 0.23 (-15%) |
| tar + xz -9e | 48.4 → 48.4 (+0%) | 99.3 → 82.6 (-17%) | 2.15 → 2.02 (-6%) |
| Arca Fast | 65.7 → 65.7 (-0%) | 0.3 → 0.24 (-20%) | 0.08 → 0.08 (+0%) |
| Arca Default | 57.3 → 57.3 (+0%) | 5.1 → 4.81 (-6%) | 0.15 → 0.17 (+13%) |
| Arca Max | 48.6 → 48.6 (+0%) | 33 → 23.5 (-29%) | 0.89 → 0.69 (-22%) |
| Arca Ultra | 47 → 46.9 (-0%) | 25.1 → 13.5 (-46%) | 5.96 → 5.78 (-3%) |

## Errors

- thirdparty 7z-normal failed in the main run (2 runs: see the results file); its row is from `results-2026-10-29-checkpoint-7z.jsonl`.
- thirdparty 7z-ultra failed in the main run (2 runs: see the results file); its row is from `results-2026-10-29-checkpoint-7z.jsonl`.
