# Zipling

A macOS archive manager built around Arca, a new compression engine. Its own archive format uses the `.zlg` extension. Zipling is in development and isn't available to download yet.

**Website:** https://www.zipling.app/

* `app/`: the macOS app (SwiftUI). `project.yml` is the XcodeGen spec, and `Zipling/Resources/AppIcon.icon` is the Icon Composer app icon.
* `engine/`: the Arca engine in Rust (`arca-core`), its C API for the app (`arca-ffi`), and the `arca` CLI. It provides the Zipling format (`.zlg`), format-aware lossless transforms, deduplication and parallel solid blocks.
* `tools/7zip/`: builds the unmodified 7-Zip helper that reads and writes every other format.
* `tools/unar/`: builds The Unarchiver's unmodified `lsar` and `unar`, for classic Mac formats and legacy file-name encodings.
* `site/`: the landing page, a static site deployed to GitHub Pages by `.github/workflows/pages.yml`.
* `design/icon/`: the app icon's generator (`zpull.py --style metal indigo`) and its review sheets.
* `docs/DESIGN.md`: how the engine works, the on-disk format, and the safety model.
* `docs/BENCHMARKS.md`: head-to-head results against Finder zip, 7-Zip, xz and zstd.
* `docs/ROADMAP.md`: the plan for turning every row of the MacPacker comparison green, and going further.
* `bench/bench.py`: the reproducible benchmark harness.

## Build and run the app

Requires Xcode, Rust (`brew install rust`) and XcodeGen (`brew install xcodegen`). Xcode builds the Rust engine automatically as a build phase. Other archive formats go through two helpers, each built once from pinned official source: 7-Zip with `tools/7zip/build.sh` and The Unarchiver's tools with `tools/unar/build.sh` (see their READMEs). Debug builds fall back to Homebrew's `7zz` and `unar` (`brew install sevenzip unar`).

```bash
cd app && xcodegen generate && open Zipling.xcodeproj
```

Or build and test from the command line:

```bash
cd app && xcodebuild -scheme Zipling test
```

Keep the build folder outside `~/Documents` (Xcode's default is fine): the tests run inside the app, and macOS asks before an app reads Documents.

## Try the engine from the command line

The command-line tool is named after the engine, `arca`.

```bash
cd engine && cargo build --release
./target/release/arca pack photos.zlg ~/Pictures/Trip -l balanced   # fast | balanced | max | ultra
./target/release/arca list photos.zlg
./target/release/arca verify photos.zlg
./target/release/arca unpack photos.zlg -o /tmp/restore
```

## Test

```bash
cd engine && cargo test --release
```

## Benchmark

```bash
python3 bench/bench.py --arca engine/target/release/arca --work /tmp/arca-bench DATASET_DIR...
```

Requires `7zz` (`brew install sevenzip`), `xz` and `zstd`.

## Licence

Copyright © 2026 Observant Systems. All rights reserved.

The source is public so you can see how Zipling works, but no licence is granted yet. You may not reuse, redistribute or sell it without permission. A licence will be chosen before 1.0.
