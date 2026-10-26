# Zipling

A macOS archive manager built around Arca, a new compression engine. Its own archive format uses the `.zlg` extension. Zipling is in development and isn't available to download yet.

**Website:** https://www.zipling.app/

* `app/`: the macOS app (SwiftUI). `project.yml` is the XcodeGen spec, and `Zipling/Resources/AppIcon.icon` is the Icon Composer app icon.
* `engine/`: the Arca engine in Rust (`arca-core`), its C API for the app (`arca-ffi`), and the `arca` CLI. It provides the Zipling format (`.zlg`), format-aware lossless transforms, deduplication and parallel solid blocks.
* `site/`: the landing page, a static site deployed to GitHub Pages by `.github/workflows/pages.yml`.
* `design/icon/`: the app icon's generator (`zpull.py --style metal indigo`) and its review sheets.
* `docs/DESIGN.md`: how the engine works, the on-disk format, and the safety model.
* `docs/BENCHMARKS.md`: head-to-head results against Finder zip, 7-Zip, xz and zstd.
* `docs/ROADMAP.md`: the plan for turning every row of the MacPacker comparison green, and going further.
* `bench/bench.py`: the reproducible benchmark harness.

## Build and run the app

Requires Xcode, Rust (`brew install rust`) and XcodeGen (`brew install xcodegen`). Xcode builds the Rust engine automatically as a build phase.

```bash
cd app && xcodegen generate && open Zipling.xcodeproj
```

Or build and test from the command line:

```bash
cd app && xcodebuild -scheme Zipling -derivedDataPath build/DerivedData test
```

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
