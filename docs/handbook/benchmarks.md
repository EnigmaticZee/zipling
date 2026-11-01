# Benchmarks: sets, rivals, method, results, how to run them again

## The sets

Nine folders. Six are public and are what the website shows; three are the
owner's own files and appear only by name and number. They live outside the
repository in `research_notes/engine-bench.noindex/sets/` (and `Mixed/` for the
saved folder); `bench/checkpoint.py` holds the paths.

| Name | What | Size | Files |
|---|---|---|---|
| jpeg (Photos) | 400 JPEGs from system and Xcode resources, 3.3 MB of exact duplicates | 95 MB | 400 |
| docs (PDFs and screenshots) | 250 PDFs and 600 PNGs from system and Xcode resources | 89 MB | 850 |
| source (Source code) | 89 Rust crates, including vendored C | 172 MB | 7,693 |
| apps (Apple apps) | 8 apps from /System/Applications | 170 MB | 1,324 |
| silesia (Generic data) | the Silesia compression corpus | 203 MB | 12 |
| mixed (Mixed folder) | the five sets above in one folder; the website's headline set | 726 MB | 10,279 |
| saved (Saved folder) | the owner's mixed folder (private) | 533 MB | 11,051 |
| dmgs (Disk images) | five installers (private) | 592 MB | 5 |
| thirdparty (Third-party apps) | six apps (private) | 874 MB | 1,400 |

## The rivals and their exact settings

| Row name | Tool and version | Make | Extract |
|---|---|---|---|
| finder-app | Finder's Compress and Archive Utility, macOS 27.0.1, driven in Finder | | |
| finder-zip | `ditto -c -k --sequesterRsrc --keepParent` (the same engine from the shell) | `ditto -x -k` | |
| 7z-normal / 7z-ultra | 7-Zip 26.05 (`7zz`) | `7zz a -t7z -mx=5` / `-mx=9`, `-snl` | `7zz x -snld20` |
| rar-fastest / rar-normal / rar-best | RAR 7.23 for macOS (command line; WinRAR has no Mac app) | `rar a -m1` / `-m3` / `-m5 -s`, `-ol` | `rar x -ol` |
| zstd-3 / zstd-19-long | zstd 1.5.7 (Homebrew) | `tar \| zstd -3 -T0` / `-19 --long=27 -T0` | `zstd -dc \| tar` |
| xz-6 / xz-9e | XZ Utils 5.8.4 (Homebrew) | `tar \| xz -6 -T0` / `-9e -T0` | `xz -dc \| tar` |
| zg-zip-fast, zg-zip-normal, zg-tzst-fast, zg-7z-normal, zg-7z-best, zg-txz-best | Zipgeist 1.0.0, driven through its Finder menu | | |

Keka and BetterZip compress 7z through 7-Zip's code, so the 7-Zip rows stand in
for them; Archive Utility and MacPacker make Deflate ZIPs, as Finder does.
Zipgeist is never named on the website; it is named in `bench/` and here.

## The method

- Every tool runs on every set twice, the second round in reverse order. The
  better time of the two counts; where the two rounds differ in size (Zipling's
  blocks take one or two zstd workers as cores allow, 0.05 to 0.3%), the larger
  archive counts. The first round's extracted tree is compared with the source
  byte for byte: names, types, bytes, link targets.
- Each set is read once (`tar` to `/dev/null`) before its runs so every tool starts
  with the files in the page cache. Archives are written to the same APFS volume.
- A set's runs start only when the Mac's 1-minute load is under 2.5 (the harness
  waits up to ten minutes, `APPRUN_LOAD` raises the gate). The idle load of this
  Mac sits near 3.5 with the animated wallpaper on; turn it off for a quiet run.
  Runs vary by about ±10% back to back; smaller differences are ties.
- Zipling's published rows are the installed app's, driven through its AppleScript
  `compress` and `extract` commands (`bench/research/apprun.py`), timed by the app's
  own clock from the command's arrival to the finished file (its timing log, on
  with `defaults write com.observantsystems.zipling timing -bool true`). That
  includes the app's listing decode, safety walk and main-actor hops, about 30 ms
  plus 9 µs a file on an extraction. The engine's own clock agrees within noise
  (geometric mean 0.99x on compression).
- Rivals run as processes under `/usr/bin/time -l` for wall, CPU and peak memory.
  Zipgeist and Finder need the screen; they ran under full-screen control, timed by
  a stopwatch on their processes with the archive's write time as a floor.
- Scoring: a row is won when Zipling is smaller, faster to make and faster to
  extract at once. Level for level: Fast against the rivals' fast settings, Default
  against their normal ones, Max and Ultra against their best
  (`bench/research/levels.py`). "Any level" counts a rival result beaten if any
  Zipling level beats it (`bench/research/board.py`).

## The result files

| File | What |
|---|---|
| `results-2026-10-29-checkpoint*.jsonl` | the first checkpoint of the research, engine CLI |
| `results-2026-10-30-quiet-all.jsonl`, `-quiet-rar.jsonl`, `-zipgeist.jsonl` | the rivals' quiet engine-side runs of 30 October |
| `results-2026-10-31-presets.jsonl` | every Zipling level on every set, engine CLI, the locked presets, quiet Mac, RAM disk |
| `results-2026-10-31-apps.jsonl` | **the published run**: the app and the command-line rivals, 00:05 to 02:32 on 1 November (DST ended at 02:00, the log's clock jumps back) |
| `results-2026-10-31-apps-zx.jsonl` | zstd and xz again with the Homebrew releases (the main run had picked up zstd 1.5.6 and xz 5.6.4 from a conda environment first in the PATH) |
| `results-2026-10-31-apps-zipgeist.jsonl`, `-finder.jsonl` | Zipgeist and Finder in their own windows |
| `results-2026-10-31-apps-machine.json` | machine, versions, method, files |
| `results-2026-11-01-default-lanes-r1/r2/r3.jsonl` | Default with one lane changed at a time (the code lane study) |
| `results-2026-11-01-default-codelane.jsonl` | the new Default through the engine CLI |
| `results-2026-11-01-apps-default.jsonl` | the new Default through the rebuilt app; replaces the Default rows of the main run |
| `report-2026-10-31-apps.md` | the full report, written by `bench/research/appreport.py` |
| `results-2026-10-31-{dmg-budget,fast-tables,max-lzt,photo-lane}.jsonl` | single-question runs named in the research log |

## The standings (1 November 2026)

Level for level: Fast 42 of 45, Default 25 of 45, Max 36 of 54, Ultra 28 of 54,
**131 of 198**. Any level against each rival result: **135 of 144**. Finder: 9 of 9.
Zipgeist: 53 of 54 at some level, 38 of 54 level for level by name.

Mixed folder, the headline: Default 214.3 MB in 8.8 s, extracts in 1.62 s;
7-Zip Ultra 235.0 MB in 48.5 s, 3.84 s; Finder 324.6 MB in 24.1 s, 7.90 s. Max
192.6 MB in 19.3 s, 2.55 s. Peak memory while compressing the mixed folder: Max
3.6 GB, 7-Zip Ultra 6.9 GB, xz -9e 3.7 GB, WinRAR Best 0.6 GB; Fast 1.0 to 3.3 GB.

Where Zipling loses and why is in `open-problems.md`.

## Running it again

1. Quiet Mac: animated wallpaper off, nothing else running, lid open, plugged in.
   Check `which zstd xz 7zz` (Homebrew first in the PATH), note versions.
2. Engine side, every level and rival on every set:
   `CHECKPOINT_RAR=<rar path> python3 -I bench/checkpoint.py <work dir> bench/results-<date>.jsonl`
   (`CHECKPOINT_TOOLS=a,b,c` keeps only named tools; `CHECKPOINT_NO_WAIT` skips the gate).
3. App side, the published method: install the app (`/Applications/Zipling.app`,
   the only registered copy), turn the timing log on, then
   `CHECKPOINT_RAR=<rar path> python3 -I bench/research/apprun.py <work dir> bench/results-<date>-apps.jsonl`.
   Zipgeist and Finder are driven by hand or by the `bench/research/zipgeist`
   tools under full-screen control.
4. Score: `python3 -I bench/research/levels.py <our files> --rivals <rival files>`;
   the report: `bench/research/appreport.py`; the public document:
   `bench/research/benchmarks_md.py`; the website's data: `bench/research/site_data_write.py`,
   then the handful of numbers quoted in `site/index.html`, `site/benchmarks.html`
   and `site/arca.html` by hand.
5. Rivals do not need re-running for an engine change: overlay the new Zipling rows
   (the scripts take an extra file, as `results-2026-11-01-apps-default.jsonl` shows).
