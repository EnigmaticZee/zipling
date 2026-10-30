## Sessions

The main run timed every Arca level and the command-line rivals together. Zipgeist (driven through Finder), WinRAR and the 7-Zip re-run on the third-party apps ran in their own sessions, each with some Arca levels alongside. Their Arca runs against the main run's (geometric mean, all sets):


The rivals from those sessions (marked †) have their times divided by that session's factor for the same set, which puts them on the main run's clock. Zipgeist's session ran slower (Finder and the screen were busy), so this makes its times shorter than measured: it errs in Zipgeist's favour. The raw times are in the results files.

## Top of every chart?

Per set, the best Arca setting against the best rival on each measure (✓ Arca leads, ✗ a rival does). DMG expansion counts as an Arca setting on the DMG set, since it is a switch at every level.

| Set | Smallest | Fastest to make | Fastest to extract | Least memory (make) | Least memory (extract) |
|---|---|---|---|---|---|
| jpeg | ✓ Ultra 71.10 vs 7-Zip normal (-mx5) 85.29 | ✓ Fast+bsc 0.16 vs tar + zstd -3 0.24 | ✓ Fast+bsc 0.19 vs tar + zstd -3 0.24 | ✗ Fast+bsc 310 vs Finder (ditto zip) 3 | ✗ Fast+bsc 195 vs Finder (ditto zip) 3 |
| docs | ✓ Ultra+bsc 17.81 vs 7-Zip Ultra (-mx9) 38.25 | ✓ Fast 0.06 vs tar + zstd -3 0.41 | ✓ Fast 0.08 vs tar + zstd -3 0.42 | ✗ Fast+bsc 164 vs Finder (ditto zip) 4 | ✗ Fast 121 vs Finder (ditto zip) 3 |
| source | ✓ Ultra+bsc 8.09 vs tar + xz -9e 8.34 | ✓ Fast 0.25 vs tar + zstd -3 3.87 | ✓ Default 0.40 vs 7-Zip normal (-mx5) 1.15 | ✗ Fast 218 vs Finder (ditto zip) 8 | ✗ Max 177 vs Finder (ditto zip) 5 |
| apps | ✓ Ultra 48.53 vs 7-Zip Ultra (-mx9) 54.10 | ✓ Fast 0.12 vs tar + zstd -3 1.09 | ✓ Fast 0.16 vs tar + zstd -3 1.17 | ✗ Fast 363 vs Finder (ditto zip) 5 | ✗ Fast 290 vs Finder (ditto zip) 4 |
| silesia | ✓ Ultra+bsc 45.92 vs tar + xz -9e 48.42 | ✗ Fast 0.17 vs tar + zstd -3 0.15 | ✓ Fast 0.07 vs tar + zstd -3 0.21 | ✗ Fast 285 vs Finder (ditto zip) 3 | ✗ Fast 199 vs Finder (ditto zip) 3 |
| saved | ✓ Ultra+bsc 220.5 vs 7-Zip Ultra (-mx9) 274.0 | ✓ Fast 0.59 vs tar + zstd -3 6.63 | ✓ Fast 0.89 vs 7-Zip normal (-mx5) 2.85 | ✗ Fast 711 vs Finder (ditto zip) 12 | ✗ Fast 412 vs tar + zstd -3 7 |
| mixed | ✓ Ultra+bsc 191.2 vs 7-Zip Ultra (-mx9) 235.0 | ✓ Fast 0.64 vs tar + zstd -3 5.55 | ✓ Fast 0.81 vs 7-Zip normal (-mx5) 2.99 | ✗ Fast 844 vs Finder (ditto zip) 10 | ✗ Fast 377 vs Finder (ditto zip) 7 |
| dmgs | ✓ Ultra+bsc 448.0 vs 7-Zip Ultra (-mx9) 609.6 | ✓ Fast+bsc 0.18 vs tar + zstd -3 0.24 | ✓ Fast+bsc 0.17 vs tar + zstd -3 0.44 | ✗ Fast+bsc 666 vs Finder (ditto zip) 3 | ✗ Fast 239 vs Finder (ditto zip) 3 |
| thirdparty | ✓ Ultra 336.2 vs 7-Zip Ultra (-mx9) 355.5 | ✓ Fast 0.55 vs tar + zstd -3 1.47 | ✓ Fast 0.40 vs 7-Zip normal (-mx5) 2.11 | ✗ Fast 1338 vs Finder (ditto zip) 5 | ✗ Fast+bsc 1087 vs Finder (ditto zip) 4 |

## Every rival setting against one Arca setting

For each rival and set: the first Arca setting (Fast → Ultra + bsc) that is at least as small, faster to make and faster to extract, all at once. ✗ when none is, with the nearest miss: of the Arca settings at least as small, the one closest on time, and how much slower it is.

| Rival | jpeg | docs | source | apps | silesia | saved | mixed | dmgs | thirdparty |
|---|---|---|---|---|---|---|---|---|---|
| Finder (ditto zip) | Fast | Fast | Fast | Fast | Fast | Fast | Fast | Fast | Fast |
| 7-Zip normal (-mx5) | Fast | Default | Max+bsc | Max | Max | Fast+bsc | Default | Default | Fast+bsc |
| 7-Zip Ultra (-mx9) | Fast | Default | Max | Max | Max+bsc | Default | Default | Default | Max |
| tar + zstd -3 | Fast | Fast | Fast | Fast | ✗ Fast: make ×1.09 | Fast | Fast | Fast | Fast |
| tar + zstd -19 --long | Fast | Default | Max | Fast+bsc | ✗ Max: extract ×1.61 | Fast+bsc | Default | ✗ Default+DMG: extract ×2.08 | Fast+bsc |
| tar + xz -6 | Fast | Fast | Max+bsc | Fast+bsc | Max | Fast+bsc | Fast+bsc | Default | Fast+bsc |
| tar + xz -9e | Fast | Default | Max | Max | Max+bsc | Fast+bsc | Default | Default | Max |

Arca beats 60 of 63 rival results on size, make and extract at once.

## How .zlg compares with every rival (all sets)

Geometric mean over the sets of Arca ÷ rival: under 1.00 means Arca is smaller or faster. Size / make time / extraction time.

| | Fast | Fast+bsc | Default | Default+bsc | Max | Max+bsc | Ultra | Ultra+bsc |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Finder (ditto zip) | 0.83 / 0.03 / 0.13 | 0.74 / 0.15 / 0.22 | 0.67 / 0.42 / 0.28 | 0.65 / 0.36 / 0.35 | 0.58 / 2.15 / 0.58 | 0.57 / 1.40 / 0.65 | 0.56 / 2.62 / 1.19 | 0.56 / 1.85 / 0.71 |
| 7-Zip normal (-mx5) | 1.17 / 0.02 / 0.16 | 1.05 / 0.08 / 0.26 | 0.94 / 0.22 / 0.34 | 0.92 / 0.19 / 0.42 | 0.81 / 1.12 / 0.70 | 0.80 / 0.73 / 0.79 | 0.79 / 1.37 / 1.45 | 0.79 / 0.97 / 0.86 |
| 7-Zip Ultra (-mx9) | 1.20 / 0.01 / 0.13 | 1.07 / 0.05 / 0.21 | 0.96 / 0.13 / 0.27 | 0.94 / 0.12 / 0.34 | 0.83 / 0.68 / 0.57 | 0.82 / 0.44 / 0.64 | 0.81 / 0.83 / 1.16 | 0.81 / 0.59 / 0.69 |
| tar + zstd -3 | 0.93 / 0.24 / 0.17 | 0.83 / 1.15 / 0.27 | 0.74 / 3.30 / 0.35 | 0.73 / 2.84 / 0.44 | 0.64 / 16.76 / 0.73 | 0.64 / 10.94 / 0.82 | 0.63 / 20.44 / 1.50 | 0.62 / 14.46 / 0.89 |
| tar + zstd -19 --long | 1.14 / 0.01 / 0.16 | 1.02 / 0.05 / 0.27 | 0.91 / 0.14 / 0.34 | 0.89 / 0.12 / 0.43 | 0.78 / 0.72 / 0.71 | 0.78 / 0.47 / 0.80 | 0.77 / 0.88 / 1.46 | 0.77 / 0.62 / 0.87 |
| tar + xz -6 | 1.11 / 0.02 / 0.10 | 0.99 / 0.08 / 0.17 | 0.89 / 0.23 / 0.22 | 0.87 / 0.19 / 0.28 | 0.77 / 1.14 / 0.46 | 0.76 / 0.75 / 0.52 | 0.75 / 1.39 / 0.95 | 0.75 / 0.99 / 0.57 |
| tar + xz -9e | 1.19 / 0.00 / 0.06 | 1.06 / 0.02 / 0.10 | 0.95 / 0.05 / 0.13 | 0.93 / 0.04 / 0.16 | 0.82 / 0.24 / 0.26 | 0.82 / 0.16 / 0.30 | 0.81 / 0.29 / 0.54 | 0.80 / 0.21 / 0.32 |

## Photos (400 JPEGs)

Load during the main run: 2.4–6.0.

| Tool | Size MB | Make s | Extract s | Make CPU s | Extract CPU s | Make peak MB | Extract peak MB | Identical | Rival ahead |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| Arca Fast | 78.26 | 0.17 | 0.19 | 1.34 | 1.73 | 320 ✗ | 197 ✗ | yes | make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Fast + bsc | 78.26 | 0.16 | 0.19 | 1.36 | 1.73 | 310 ✗ | 195 ✗ | yes | make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default | 71.10 | 2.04 ✗ | 1.05 ✗ | 19.6 | 9.48 | 1482 ✗ | 408 ✗ | yes | make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default + bsc | 71.10 | 2.03 ✗ | 1.04 ✗ | 19.6 | 9.82 | 1487 ✗ | 406 ✗ | yes | make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max | 71.10 | 2.04 ✗ | 1.04 ✗ | 19.5 | 9.96 | 1500 ✗ | 403 ✗ | yes | make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max + bsc | 71.10 | 2.03 ✗ | 1.04 ✗ | 19.5 | 9.49 | 1466 ✗ | 406 ✗ | yes | make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra | 71.10 | 2.03 ✗ | 1.04 ✗ | 19.5 | 9.96 | 1495 ✗ | 403 ✗ | yes | make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra + bsc | 71.10 | 2.03 ✗ | 1.05 ✗ | 19.0 | 9.96 | 1502 ✗ | 414 ✗ | yes | make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Finder (ditto zip) | 93.30 | 2.44 | 0.47 | 2.41 | 0.46 | 3 | 3 | yes |  |
| 7-Zip normal (-mx5) | 85.29 | 5.92 | 1.54 | 10.8 | 1.52 | 381 | 184 | yes |  |
| 7-Zip Ultra (-mx9) | 85.29 | 6.20 | 1.54 | 10.9 | 1.52 | 983 | 184 | yes |  |
| tar + zstd -3 | 91.84 | 0.24 | 0.24 | 0.41 | 0.29 | 118 | 7 | yes |  |
| tar + zstd -19 --long | 85.73 | 12.8 | 0.25 | 23.9 | 0.35 | 353 | 100 | yes |  |
| tar + xz -6 | 89.04 | 8.37 | 0.76 | 32.2 | 2.71 | 554 | 214 | yes |  |
| tar + xz -9e | 85.60 | 38.8 | 2.40 | 38.9 | 2.71 | 850 | 242 | yes |  |

## PDFs and screenshots

Load during the main run: 2.5–4.6.

| Tool | Size MB | Make s | Extract s | Make CPU s | Extract CPU s | Make peak MB | Extract peak MB | Identical | Rival ahead |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| Arca Fast | 41.06 ✗ | 0.06 | 0.08 | 0.31 | 0.25 | 167 ✗ | 121 ✗ | yes | size: 7-Zip Ultra (-mx9); make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Fast + bsc | 41.06 ✗ | 0.07 | 0.08 | 0.31 | 0.25 | 164 ✗ | 121 ✗ | yes | size: 7-Zip Ultra (-mx9); make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default | 22.14 | 1.31 ✗ | 0.22 | 7.20 | 1.65 | 1211 ✗ | 426 ✗ | yes | make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default + bsc | 22.14 | 1.31 ✗ | 0.22 | 7.22 | 1.61 | 1239 ✗ | 444 ✗ | yes | make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max | 18.20 | 9.69 ✗ | 0.41 | 26.5 | 2.60 | 1808 ✗ | 351 ✗ | yes | make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max + bsc | 18.20 | 9.79 ✗ | 0.41 | 26.6 | 2.51 | 1810 ✗ | 348 ✗ | yes | make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra | 17.81 | 13.9 ✗ | 0.40 | 39.0 | 2.54 | 1779 ✗ | 362 ✗ | yes | make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra + bsc | 17.81 | 15.2 ✗ | 0.41 | 41.0 | 2.60 | 1759 ✗ | 339 ✗ | yes | make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Finder (ditto zip) | 50.19 | 1.36 | 0.67 | 1.32 | 0.65 | 4 | 3 | yes |  |
| 7-Zip normal (-mx5) | 38.29 | 2.42 | 0.79 | 4.26 | 0.77 | 382 | 130 | yes |  |
| 7-Zip Ultra (-mx9) | 38.25 | 2.81 | 0.79 | 4.97 | 0.78 | 920 | 130 | yes |  |
| tar + zstd -3 | 45.12 | 0.41 | 0.42 | 0.49 | 0.45 | 103 | 7 | yes |  |
| tar + zstd -19 --long | 38.98 | 5.24 | 0.42 | 8.79 | 0.45 | 303 | 94 | yes |  |
| tar + xz -6 | 43.29 | 4.13 | 0.73 | 12.9 | 1.88 | 505 | 165 | yes |  |
| tar + xz -9e | 38.80 | 22.3 | 1.44 | 22.7 | 1.90 | 794 | 192 | yes |  |

## Source code (89 Rust crates)

Load during the main run: 2.6–6.2.

| Tool | Size MB | Make s | Extract s | Make CPU s | Extract CPU s | Make peak MB | Extract peak MB | Identical | Rival ahead |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| Arca Fast | 13.55 ✗ | 0.25 | 0.40 | 1.56 | 2.05 | 218 ✗ | 181 ✗ | yes | size: tar + xz -9e; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Fast + bsc | 11.06 ✗ | 0.41 | 0.53 | 3.04 | 3.48 | 532 ✗ | 429 ✗ | yes | size: tar + xz -9e; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default | 11.20 ✗ | 1.95 | 0.40 | 7.38 | 2.04 | 615 ✗ | 179 ✗ | yes | size: tar + xz -9e; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default + bsc | 10.21 ✗ | 0.95 | 0.50 | 4.03 | 3.44 | 572 ✗ | 409 ✗ | yes | size: tar + xz -9e; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max | 8.23 | 22.7 ✗ | 0.49 | 71.4 | 2.32 | 1853 ✗ | 177 ✗ | yes | make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max + bsc | 8.31 | 1.56 | 0.72 | 4.56 | 4.80 | 694 ✗ | 641 ✗ | yes | make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra | 8.23 | 22.3 ✗ | 3.72 ✗ | 53.7 | 5.53 | 1323 ✗ | 338 ✗ | yes | make: tar + zstd -3; extract: 7-Zip normal (-mx5); make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra + bsc | 8.09 | 1.74 | 0.92 | 3.83 | 5.16 | 678 ✗ | 612 ✗ | yes | make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Finder (ditto zip) | 24.81 | 5.26 | 6.46 | 5.00 | 6.33 | 8 | 5 | yes |  |
| 7-Zip normal (-mx5) | 9.15 | 13.9 | 1.15 | 19.9 | 1.18 | 794 | 170 | yes |  |
| 7-Zip Ultra (-mx9) | 8.51 | 23.7 | 1.17 | 30.0 | 1.14 | 1676 | 170 | yes |  |
| tar + zstd -3 | 15.16 | 3.87 | 3.58 | 4.10 | 3.71 | 111 | 7 | yes |  |
| tar + zstd -19 --long | 9.37 | 23.8 | 3.58 | 56.3 | 3.70 | 446 | 133 | yes |  |
| tar + xz -6 | 9.96 | 6.46 | 3.64 | 30.9 | 4.36 | 931 | 254 | yes |  |
| tar + xz -9e | 8.34 | 71.4 | 3.58 | 75.3 | 4.22 | 860 | 251 | yes |  |

## Apple apps (8 from /System/Applications)

Load during the main run: 2.3–5.8.

| Tool | Size MB | Make s | Extract s | Make CPU s | Extract CPU s | Make peak MB | Extract peak MB | Identical | Rival ahead |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| Arca Fast | 72.23 ✗ | 0.12 | 0.16 | 0.77 | 0.95 | 363 ✗ | 290 ✗ | yes | size: 7-Zip Ultra (-mx9); make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Fast + bsc | 58.76 ✗ | 4.63 ✗ | 0.27 | 39.6 | 1.75 | 2339 ✗ | 299 ✗ | yes | size: 7-Zip Ultra (-mx9); make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default | 56.44 ✗ | 1.43 ✗ | 0.20 | 6.00 | 1.32 | 1013 ✗ | 481 ✗ | yes | size: 7-Zip Ultra (-mx9); make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default + bsc | 56.39 ✗ | 1.33 ✗ | 0.22 | 5.81 | 1.31 | 1012 ✗ | 496 ✗ | yes | size: 7-Zip Ultra (-mx9); make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max | 49.16 | 13.0 ✗ | 0.64 | 36.5 | 2.57 | 2322 ✗ | 440 ✗ | yes | make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max + bsc | 49.15 | 13.2 ✗ | 0.64 | 36.3 | 2.66 | 2288 ✗ | 500 ✗ | yes | make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra | 48.53 | 15.8 ✗ | 0.65 | 48.0 | 2.74 | 2072 ✗ | 492 ✗ | yes | make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra + bsc | 48.55 | 15.9 ✗ | 0.62 | 48.0 | 2.65 | 2086 ✗ | 484 ✗ | yes | make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Finder (ditto zip) | 87.40 | 5.75 | 2.08 | 5.65 | 2.05 | 5 | 4 | yes |  |
| 7-Zip normal (-mx5) | 55.26 | 18.1 | 1.44 | 34.4 | 1.69 | 877 | 227 | yes |  |
| 7-Zip Ultra (-mx9) | 54.10 | 30.4 | 1.68 | 44.8 | 1.66 | 1759 | 225 | yes |  |
| tar + zstd -3 | 77.25 | 1.09 | 1.17 | 1.54 | 1.30 | 146 | 7 | yes |  |
| tar + zstd -19 --long | 59.57 | 19.5 | 1.20 | 46.3 | 1.36 | 487 | 133 | yes |  |
| tar + xz -6 | 58.97 | 9.19 | 1.35 | 51.0 | 3.76 | 938 | 292 | yes |  |
| tar + xz -9e | 54.31 | 66.0 | 2.68 | 67.0 | 3.52 | 899 | 290 | yes |  |

## Silesia corpus

Load during the main run: 2.2–5.4.

| Tool | Size MB | Make s | Extract s | Make CPU s | Extract CPU s | Make peak MB | Extract peak MB | Identical | Rival ahead |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| Arca Fast | 65.18 ✗ | 0.17 ✗ | 0.07 | 1.17 | 0.29 | 285 ✗ | 199 ✗ | yes | size: tar + xz -9e; make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Fast + bsc | 57.59 ✗ | 0.56 ✗ | 0.39 ✗ | 4.03 | 3.27 | 587 ✗ | 525 ✗ | yes | size: tar + xz -9e; make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default | 57.26 ✗ | 4.96 ✗ | 0.10 | 22.6 | 0.29 | 730 ✗ | 263 ✗ | yes | size: tar + xz -9e; make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default + bsc | 53.07 ✗ | 2.32 ✗ | 0.41 ✗ | 7.94 | 3.12 | 788 ✗ | 693 ✗ | yes | size: tar + xz -9e; make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max | 49.41 ✗ | 13.8 ✗ | 0.39 ✗ | 76.1 | 1.79 | 2455 ✗ | 256 ✗ | yes | size: tar + xz -9e; make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max + bsc | 46.32 | 9.99 ✗ | 0.57 ✗ | 41.9 | 4.69 | 1802 ✗ | 667 ✗ | yes | make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra | 46.93 | 14.2 ✗ | 5.91 ✗ | 52.3 | 9.01 | 1674 ✗ | 497 ✗ | yes | make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra + bsc | 45.92 | 13.7 ✗ | 0.84 ✗ | 44.9 | 5.61 | 1996 ✗ | 620 ✗ | yes | make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Finder (ditto zip) | 68.23 | 5.54 | 0.28 | 5.51 | 0.27 | 3 | 3 | yes |  |
| 7-Zip normal (-mx5) | 49.60 | 23.9 | 1.00 | 61.4 | 1.62 | 904 | 257 | yes |  |
| 7-Zip Ultra (-mx9) | 48.69 | 53.7 | 1.59 | 74.3 | 1.57 | 2084 | 255 | yes |  |
| tar + zstd -3 | 66.58 | 0.15 | 0.21 | 0.83 | 0.39 | 154 | 7 | yes |  |
| tar + zstd -19 --long | 52.65 | 28.3 | 0.24 | 87.7 | 0.42 | 592 | 133 | yes |  |
| tar + xz -6 | 49.50 | 14.0 | 0.55 | 81.5 | 2.78 | 1087 | 324 | yes |  |
| tar + xz -9e | 48.42 | 94.8 | 2.24 | 98.7 | 2.60 | 1080 | 325 | yes |  |

## Saved folder (the 660 MB test folder)

Load during the main run: 2.6–19.6.

| Tool | Size MB | Make s | Extract s | Make CPU s | Extract CPU s | Make peak MB | Extract peak MB | Identical | Rival ahead |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| Arca Fast | 300.3 ✗ | 0.59 | 0.89 | 4.64 | 5.56 | 711 ✗ | 412 ✗ | yes | size: 7-Zip Ultra (-mx9); make peak: Finder (ditto zip); extract peak: tar + zstd -3 |
| Arca Fast + bsc | 275.9 ✗ | 6.80 ✗ | 1.26 | 63.5 | 8.77 | 2810 ✗ | 652 ✗ | yes | size: 7-Zip Ultra (-mx9); make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: tar + zstd -3 |
| Arca Default | 250.7 | 7.86 ✗ | 1.96 | 55.0 | 16.1 | 3735 ✗ | 1481 ✗ | yes | make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: tar + zstd -3 |
| Arca Default + bsc | 249.7 | 7.09 ✗ | 2.16 | 51.6 | 17.6 | 4226 ✗ | 1725 ✗ | yes | make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: tar + zstd -3 |
| Arca Max | 223.6 | 22.4 ✗ | 2.69 | 203.0 | 23.2 | 5573 ✗ | 1071 ✗ | yes | make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: tar + zstd -3 |
| Arca Max + bsc | 223.7 | 15.8 ✗ | 2.89 ✗ | 129.5 | 25.6 | 6019 ✗ | 1454 ✗ | yes | make: tar + zstd -3; extract: 7-Zip normal (-mx5); make peak: Finder (ditto zip); extract peak: tar + zstd -3 |
| Arca Ultra | 220.6 | 31.2 ✗ | 4.31 ✗ | 233.5 | 27.6 | 6615 ✗ | 1185 ✗ | yes | make: tar + zstd -3; extract: 7-Zip normal (-mx5); make peak: Finder (ditto zip); extract peak: tar + zstd -3 |
| Arca Ultra + bsc | 220.5 | 21.1 ✗ | 2.97 ✗ | 178.2 | 26.1 | 7051 ✗ | 1463 ✗ | yes | make: tar + zstd -3; extract: 7-Zip normal (-mx5); make peak: Finder (ditto zip); extract peak: tar + zstd -3 |
| Finder (ditto zip) | 355.6 | 19.0 | 11.3 | 18.6 | 11.2 | 12 | 7 | yes |  |
| 7-Zip normal (-mx5) | 276.4 | 21.2 | 2.85 | 103.8 | 6.77 | 2553 | 922 | yes |  |
| 7-Zip Ultra (-mx9) | 274.0 | 52.3 | 4.01 | 127.1 | 6.62 | 6623 | 920 | yes |  |
| tar + zstd -3 | 329.5 | 6.63 | 10.6 | 7.88 | 11.1 | 156 | 7 | yes |  |
| tar + zstd -19 --long | 283.2 | 32.7 | 10.6 | 214.6 | 11.2 | 1767 | 133 | yes |  |
| tar + xz -6 | 292.9 | 22.3 | 10.8 | 190.9 | 20.4 | 1395 | 709 | yes |  |
| tar + xz -9e | 278.0 | 87.2 | 10.7 | 281.8 | 19.7 | 3624 | 1192 | yes |  |

## Website mixed folder (all five sets)

Load during the main run: 2.3–16.1.

| Tool | Size MB | Make s | Extract s | Make CPU s | Extract CPU s | Make peak MB | Extract peak MB | Identical | Rival ahead |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| Arca Fast | 270.2 ✗ | 0.64 | 0.81 | 5.00 | 4.91 | 844 ✗ | 377 ✗ | yes | size: 7-Zip Ultra (-mx9); make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Fast + bsc | 246.7 ✗ | 5.30 | 1.21 | 46.2 | 9.87 | 2538 ✗ | 865 ✗ | yes | size: 7-Zip Ultra (-mx9); make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default | 217.9 | 8.03 ✗ | 1.70 | 66.2 | 14.7 | 3472 ✗ | 1044 ✗ | yes | make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default + bsc | 212.8 | 5.44 | 2.16 | 45.8 | 19.4 | 3910 ✗ | 1549 ✗ | yes | make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max | 197.2 | 30.6 ✗ | 2.15 | 258.2 | 18.6 | 6584 ✗ | 1061 ✗ | yes | make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max + bsc | 194.5 | 18.6 ✗ | 2.74 | 145.2 | 24.1 | 6549 ✗ | 1684 ✗ | yes | make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra | 191.9 | 35.1 ✗ | 6.74 ✗ | 232.6 | 31.4 | 6630 ✗ | 1360 ✗ | yes | make: tar + zstd -3; extract: 7-Zip normal (-mx5); make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra + bsc | 191.2 | 28.1 ✗ | 2.80 | 200.2 | 25.1 | 6869 ✗ | 1710 ✗ | yes | make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Finder (ditto zip) | 324.2 | 19.8 | 9.94 | 19.5 | 9.86 | 10 | 7 | yes |  |
| 7-Zip normal (-mx5) | 237.8 | 25.6 | 2.99 | 139.2 | 6.96 | 2523 | 953 | yes |  |
| 7-Zip Ultra (-mx9) | 235.0 | 47.8 | 3.88 | 167.0 | 6.88 | 7360 | 949 | yes |  |
| tar + zstd -3 | 296.1 | 5.55 | 5.60 | 7.17 | 6.08 | 173 | 7 | yes |  |
| tar + zstd -19 --long | 246.2 | 39.1 | 5.56 | 262.6 | 6.11 | 1800 | 133 | yes |  |
| tar + xz -6 | 251.2 | 24.5 | 6.25 | 221.9 | 15.7 | 1356 | 778 | yes |  |
| tar + xz -9e | 236.6 | 89.6 | 7.67 | 311.4 | 15.0 | 3656 | 1220 | yes |  |

## Your DMGs (5 installers)

Load during the main run: 2.1–21.6.

| Tool | Size MB | Make s | Extract s | Make CPU s | Extract CPU s | Make peak MB | Extract peak MB | Identical | Rival ahead |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| Arca Fast | 615.5 ✗ | 0.18 | 0.17 | 1.40 | 0.44 | 711 ✗ | 239 ✗ | yes | size: 7-Zip Ultra (-mx9); make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Fast + bsc | 615.5 ✗ | 0.18 | 0.17 | 1.38 | 0.46 | 666 ✗ | 248 ✗ | yes | size: 7-Zip Ultra (-mx9); make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default | 560.5 | 2.49 ✗ | 0.96 ✗ | 21.1 | 7.48 | 3029 ✗ | 2535 ✗ | yes | make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default + bsc | 550.4 | 5.13 ✗ | 0.94 ✗ | 36.8 | 7.66 | 4082 ✗ | 2466 ✗ | yes | make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max | 455.0 | 39.3 ✗ | 4.07 ✗ | 341.5 | 38.3 | 7949 ✗ | 3621 ✗ | yes | make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max + bsc | 455.0 | 39.2 ✗ | 4.16 ✗ | 341.0 | 38.0 | 8002 ✗ | 3622 ✗ | yes | make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra | 448.0 | 53.0 ✗ | 4.16 ✗ | 461.2 | 37.9 | 7676 ✗ | 3693 ✗ | yes | make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra + bsc | 448.0 | 53.0 ✗ | 4.11 ✗ | 462.1 | 38.0 | 7835 ✗ | 3664 ✗ | yes | make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Fast + DMG expansion | 555.4 | 6.26 ✗ | 2.92 ✗ | 58.1 | 26.3 | 4111 ✗ | 3564 ✗ | yes | make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default + DMG expansion | 560.5 | 2.54 ✗ | 0.93 ✗ | 21.0 | 7.68 | 3255 ✗ | 2575 ✗ | yes | make: tar + zstd -3; extract: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Finder (ditto zip) | 617.3 | 11.2 | 0.58 | 11.1 | 0.57 | 3 | 3 | yes |  |
| 7-Zip normal (-mx5) | 610.6 | 8.73 | 1.38 | 74.5 | 2.91 | 2805 | 1217 | yes |  |
| 7-Zip Ultra (-mx9) | 609.6 | 18.4 | 1.79 | 70.9 | 3.13 | 6564 | 1215 | yes |  |
| tar + zstd -3 | 616.3 | 0.24 | 0.44 | 0.74 | 0.64 | 163 | 7 | yes |  |
| tar + zstd -19 --long | 610.3 | 24.0 | 0.45 | 202.2 | 0.71 | 1996 | 133 | yes |  |
| tar + xz -6 | 612.5 | 29.1 | 1.31 | 234.7 | 5.94 | 1601 | 730 | yes |  |
| tar + xz -9e | 610.7 | 101.2 | 3.20 | 305.7 | 5.99 | 3398 | 1384 | yes |  |

## Your third-party apps (6)

Load during the main run: 1.9–33.4.

| Tool | Size MB | Make s | Extract s | Make CPU s | Extract CPU s | Make peak MB | Extract peak MB | Identical | Rival ahead |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| Arca Fast | 504.4 ✗ | 0.55 | 0.40 | 4.44 | 2.36 | 1338 ✗ | 1725 ✗ | yes | size: 7-Zip Ultra (-mx9); make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Fast + bsc | 367.6 ✗ | 35.6 ✗ | 1.37 | 339.1 | 12.2 | 3433 ✗ | 1087 ✗ | yes | size: 7-Zip Ultra (-mx9); make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default | 403.0 ✗ | 6.68 ✗ | 0.44 | 54.2 | 2.59 | 3467 ✗ | 2118 ✗ | yes | size: 7-Zip Ultra (-mx9); make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Default + bsc | 402.6 ✗ | 6.44 ✗ | 0.45 | 53.4 | 2.88 | 3373 ✗ | 2118 ✗ | yes | size: 7-Zip Ultra (-mx9); make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max | 350.2 | 39.2 ✗ | 1.74 | 331.9 | 13.1 | 5020 ✗ | 1660 ✗ | yes | make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Max + bsc | 350.2 | 39.0 ✗ | 1.65 | 328.5 | 13.6 | 4913 ✗ | 1825 ✗ | yes | make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra | 336.2 | 62.4 ✗ | 1.91 | 486.4 | 13.5 | 5589 ✗ | 1965 ✗ | yes | make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Arca Ultra + bsc | 336.2 | 61.7 ✗ | 1.87 | 485.5 | 13.2 | 5633 ✗ | 2007 ✗ | yes | make: tar + zstd -3; make peak: Finder (ditto zip); extract peak: Finder (ditto zip) |
| Finder (ditto zip) | 536.2 | 37.6 | 3.51 | 37.5 | 3.46 | 5 | 4 | yes |  |
| 7-Zip normal (-mx5) | 371.2 | 73.2 | 2.11 | 351.3 | 12.6 | 2510 | 1573 | yes |  |
| 7-Zip Ultra (-mx9) | 355.5 | 87.3 | 3.33 | 448.0 | 11.6 | 12186 | 1555 | yes |  |
| tar + zstd -3 | 515.6 | 1.47 | 15.2 | 5.31 | 16.4 | 161 | 7 | yes |  |
| tar + zstd -19 --long | 386.6 | 60.9 | 15.3 | 499.0 | 16.7 | 1919 | 133 | yes |  |
| tar + xz -6 | 390.2 | 56.6 | 16.3 | 468.3 | 35.2 | 1324 | 840 | yes |  |
| tar + xz -9e | 364.2 | 109.2 | 17.8 | 604.0 | 33.5 | 5540 | 1976 | yes |  |

## What changed against the website's numbers

The website (site/data.js) was measured on 2026-10-25/26; its Finder, zstd and xz rows in a separate session. Size MB / compress s / extract s, website → now.

**Website mixed folder (all five sets)**

| Tool | Size MB | Compress s | Extract s |
|---|---|---|---|
| Finder (ditto zip) | 322.7 → 324.2 (+0%) | 19.8 → 19.8 (+0%) | 9.32 → 9.94 (+7%) |
| 7-Zip Ultra (-mx9) | 233.4 → 235.0 (+1%) | 49.3 → 47.8 (-3%) | 3.84 → 3.88 (+1%) |
| tar + zstd -19 --long | 244.1 → 246.2 (+1%) | 46.5 → 39.1 (-16%) | 5.52 → 5.56 (+1%) |
| tar + xz -9e | 235.1 → 236.6 (+1%) | 97.9 → 89.6 (-8%) | 7.28 → 7.67 (+5%) |
| Arca Fast | 286.8 → 270.2 (-6%) | 1.4 → 0.64 (-55%) | 0.88 → 0.81 (-8%) |
| Arca Default | 221.5 → 217.9 (-2%) | 9.7 → 8.03 (-17%) | 2.3 → 1.70 (-26%) |
| Arca Max | 192.8 → 197.2 (+2%) | 51.6 → 30.6 (-41%) | 3.5 → 2.15 (-38%) |
| Arca Ultra | 191 → 191.9 (+0%) | 37.9 → 35.1 (-7%) | 7.45 → 6.74 (-10%) |

**PDFs and screenshots**

| Tool | Size MB | Compress s | Extract s |
|---|---|---|---|
| Finder (ditto zip) | 48.6 → 50.2 (+3%) | 1.5 → 1.36 (-9%) | 0.68 → 0.67 (-1%) |
| 7-Zip Ultra (-mx9) | 36.6 → 38.2 (+5%) | 2.7 → 2.81 (+4%) | 0.82 → 0.79 (-3%) |
| tar + zstd -19 --long | 37.1 → 39.0 (+5%) | 8 → 5.24 (-34%) | 0.42 → 0.42 (-0%) |
| tar + xz -9e | 36.9 → 38.8 (+5%) | 22.3 → 22.3 (+0%) | 1.1 → 1.44 (+31%) |
| Arca Fast | 40 → 41.1 (+3%) | 0.1 → 0.06 (-38%) | 0.09 → 0.08 (-11%) |
| Arca Default | 20.8 → 22.1 (+6%) | 2 → 1.31 (-35%) | 0.26 → 0.22 (-16%) |
| Arca Max | 16.5 → 18.2 (+10%) | 29.3 → 9.69 (-67%) | 1.2 → 0.41 (-66%) |
| Arca Ultra | 16.5 → 17.8 (+8%) | 29.4 → 13.9 (-53%) | 1.19 → 0.40 (-66%) |

**Photos (400 JPEGs)**

| Tool | Size MB | Compress s | Extract s |
|---|---|---|---|
| Finder (ditto zip) | 93.3 → 93.3 (-0%) | 2.5 → 2.44 (-2%) | 0.47 → 0.47 (+1%) |
| 7-Zip Ultra (-mx9) | 85.2 → 85.3 (+0%) | 6 → 6.20 (+3%) | 1.58 → 1.54 (-3%) |
| tar + zstd -19 --long | 85.7 → 85.7 (+0%) | 15.1 → 12.8 (-15%) | 0.27 → 0.25 (-6%) |
| tar + xz -9e | 85.8 → 85.6 (-0%) | 43.3 → 38.8 (-11%) | 2.04 → 2.40 (+18%) |
| Arca Fast | 95 → 78.3 (-18%) | 0.1 → 0.17 (+68%) | 0.06 → 0.19 (+224%) |
| Arca Default | 71 → 71.1 (+0%) | 2.4 → 2.04 (-15%) | 1.24 → 1.05 (-16%) |
| Arca Max | 71 → 71.1 (+0%) | 2.5 → 2.04 (-18%) | 1.25 → 1.04 (-17%) |
| Arca Ultra | 71 → 71.1 (+0%) | 2.5 → 2.03 (-19%) | 1.29 → 1.04 (-19%) |

**Apple apps (8 from /System/Applications)**

| Tool | Size MB | Compress s | Extract s |
|---|---|---|---|
| Finder (ditto zip) | 87.4 → 87.4 (+0%) | 5.8 → 5.75 (-1%) | 1.96 → 2.08 (+6%) |
| 7-Zip Ultra (-mx9) | 54.1 → 54.1 (+0%) | 27.6 → 30.4 (+10%) | 1.69 → 1.68 (-1%) |
| tar + zstd -19 --long | 59.4 → 59.6 (+0%) | 23.2 → 19.5 (-16%) | 1.15 → 1.20 (+5%) |
| tar + xz -9e | 54.4 → 54.3 (-0%) | 67 → 66.0 (-1%) | 2.43 → 2.68 (+10%) |
| Arca Fast | 72.4 → 72.2 (-0%) | 0.4 → 0.12 (-69%) | 0.22 → 0.16 (-27%) |
| Arca Default | 62.4 → 56.4 (-10%) | 2.4 → 1.43 (-41%) | 0.28 → 0.20 (-27%) |
| Arca Max | 49.8 → 49.2 (-1%) | 23.5 → 13.0 (-44%) | 1.04 → 0.64 (-39%) |
| Arca Ultra | 49.8 → 48.5 (-3%) | 23.5 → 15.8 (-33%) | 1.03 → 0.65 (-37%) |

**Source code (89 Rust crates)**

| Tool | Size MB | Compress s | Extract s |
|---|---|---|---|
| Finder (ditto zip) | 24.8 → 24.8 (+0%) | 5.2 → 5.26 (+1%) | 6.09 → 6.46 (+6%) |
| 7-Zip Ultra (-mx9) | 8.5 → 8.51 (+0%) | 22 → 23.7 (+8%) | 1.22 → 1.17 (-4%) |
| tar + zstd -19 --long | 9.3 → 9.37 (+1%) | 26.6 → 23.8 (-11%) | 3.45 → 3.58 (+4%) |
| tar + xz -9e | 8.3 → 8.34 (+1%) | 71.5 → 71.4 (-0%) | 3.55 → 3.58 (+1%) |
| Arca Fast | 13.5 → 13.5 (+0%) | 0.5 → 0.25 (-51%) | 0.54 → 0.40 (-26%) |
| Arca Default | 11.2 → 11.2 (+0%) | 2.1 → 1.95 (-7%) | 0.51 → 0.40 (-22%) |
| Arca Max | 8.3 → 8.23 (-1%) | 25.4 → 22.7 (-11%) | 0.72 → 0.49 (-32%) |
| Arca Ultra | 8.3 → 8.23 (-1%) | 24.9 → 22.3 (-10%) | 3.57 → 3.72 (+4%) |

**Silesia corpus**

| Tool | Size MB | Compress s | Extract s |
|---|---|---|---|
| Finder (ditto zip) | 68.2 → 68.2 (+0%) | 5.8 → 5.54 (-4%) | 0.3 → 0.28 (-5%) |
| 7-Zip Ultra (-mx9) | 48.7 → 48.7 (-0%) | 48.3 → 53.7 (+11%) | 1.53 → 1.59 (+4%) |
| tar + zstd -19 --long | 52.6 → 52.7 (+0%) | 32.9 → 28.3 (-14%) | 0.27 → 0.24 (-11%) |
| tar + xz -9e | 48.4 → 48.4 (+0%) | 99.3 → 94.8 (-5%) | 2.15 → 2.24 (+4%) |
| Arca Fast | 65.7 → 65.2 (-1%) | 0.3 → 0.17 (-45%) | 0.08 → 0.07 (-17%) |
| Arca Default | 57.3 → 57.3 (-0%) | 5.1 → 4.96 (-3%) | 0.15 → 0.10 (-31%) |
| Arca Max | 48.6 → 49.4 (+2%) | 33 → 13.8 (-58%) | 0.89 → 0.39 (-57%) |
| Arca Ultra | 47 → 46.9 (-0%) | 25.1 → 14.2 (-43%) | 5.96 → 5.91 (-1%) |

