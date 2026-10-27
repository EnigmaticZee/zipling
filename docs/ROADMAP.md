# Product plan: getting every row green

Reference: <https://macpacker.app/en/compare> (MacPacker 0.19, 7-Zip 26.02, The
Unarchiver 4.3.9, Keka 1.6.7, BetterZip 6.0.4). Totals there: MacPacker reads 39 /
writes 1, Keka 33 / 16, BetterZip 34 / 15, The Unarchiver 42 / 0, 7-Zip 43 / 9.

**Target: read all 68 listed format rows, write every format that can be written legally
(~30), and green on every capability row.** Nobody on that page writes more than 16.

## 1. Format backends

One Swift protocol (`ArchiveProvider`: list, read entry stream, extract, write,
update), several engines behind it. All third-party parsers run in a sandboxed XPC
helper.

| Backend | Licence | Covers |
|---|---|---|
| **Zipling engine** (ours, Rust) | ours | `.zlg` read/write, plus the transform pipeline reused to write *optimised* ZIP/7z |
| **libarchive** | BSD-2 | zip, 7z, rar/rar5 (read), tar + gz/bz2/xz/lzma/Z/lz4/zst/lzip/lzop, cpio, iso9660, xar/pkg, cab, lha, ar/deb, rpm, warc, mtree — and *writes* zip, 7z, tar.*, cpio, iso9660, xar, ar, warc |
| **7-Zip** (as a dylib) | LGPL + unRAR restriction | disk images and filesystems (dmg, vhd/vhdx, vmdk, qcow2, vdi, apfs, hfs, ntfs, fat, ext, squashfs, cramfs, udf, mbr/gpt), wim/swm/esd (r/w), chm, msi/ole, nsis/sfx exe, arj, swf, raw lzma, rar edge cases, zipx methods |
| **XADMaster** (The Unarchiver's engine) | LGPL | StuffIt sit/sitx/sea, Compact Pro, DiskDoubler, PackIt, BinHex, MacBinary, LZX, Amiga ADF/DMS/PowerPacker, CP/M LBR/Squeeze/Crunch, ARC/PAK, Zoo, ALZip, old ACE, game data (nds/nsa/sar), PDF image extraction |
| **Apple frameworks** | system | Apple Archive aar/yaa/aea (r/w), dmg create/convert via DiskImages (`hdiutil`), LZFSE |
| Small libs | MIT/BSD | Brotli, Snappy, LZO (lzokay, read), TNEF/winmail.dat (own parser), lrzip (own reader), ACE 2 via `unace` if licensable |

## 2. Write matrix (where the competition is weakest)

| Format | Plan | Note |
|---|---|---|
| ZIP (+ zip64, AES-256, split, Windows-clean: no `__MACOSX`/`._*`, UTF-8 flag) | libarchive + own writer | "Optimised ZIP": zopfli/libdeflate levels, still opens everywhere |
| 7z (LZMA2, AES-256 + header encryption, solid, split) | 7-Zip lib | multi-threaded |
| tar, tar.gz/bz2/xz/zst/br/lz4/lz/Z | libarchive | single pass (7-Zip needs two) |
| gz, bz2, xz, zst, br, lz4, lz, lzma, Z (single file) | libs | |
| cpio, iso9660, xar, ar, warc, wim | libarchive / 7-Zip | |
| Apple Archive, dmg (UDZO/ULFO/ULMO, encrypted) | system | |
| **Zipling (.zlg)** | ours | the "smallest + fastest" choice |
| cab (MSZIP/LZX) | own writer | no Mac app writes CAB |
| lha (lh5) | own encoder | |
| zipx | own (zstd/LZMA methods) | |
| RAR | optional download of RARLAB's `rar` (user accepts its licence) | RAR's encoder is proprietary; BetterZip does the same |
| ARJ, ACE, StuffIt, CHM, Compact Pro… | read-only | proprietary or obsolete encoders — nobody writes these; leaving them read-only is the honest answer |

## 3. Capability rows

| Capability | Plan |
|---|---|
| Open split / multi-volume sets | zip (.z01, .zip.001), 7z.001, rar (.partN/.r00), .zlg — auto-detected from any part |
| Create split volumes | zip, 7z, zlg, tar — size presets (FAT32 4 GB, email 25 MB, custom) |
| Open password-protected | zip (ZipCrypto/AES), 7z, rar, .zlg, dmg; passwords optionally kept in Keychain behind Touch ID |
| Write encrypted | zip AES-256, 7z AES-256 + encrypted names, .zlg XChaCha20-Poly1305 / AES-256-GCM with Argon2id, encrypted dmg |
| Modify in place | add / delete / rename / move in zip, 7z, tar, .zlg (.zlg appends only new chunks — no full rewrite) |
| Archive browser | Finder-like list / columns / icons; instant even for 1M entries (index-only open) |
| Browse nested archives | recursive virtual file system, any depth, any format mix |
| Finder / Quick Look extensions | Quick Look preview + thumbnails for archives, inner-file Quick Look, Finder context menu, Share extension |
| Command-line interface | `zipling` CLI covering every backend (`zipling x anything`, `zipling a -f 7z …`) |

## 4. Beyond the table (differentiators)

1. **Zipling format (.zlg)**: dedup + format-aware transforms + parallel blocks (benchmarks in BENCHMARKS.md).
2. **Mount any archive as a Finder volume** (FSKit, read-only): open files in place, no extraction.
3. **Spotlight finds files inside archives** (importer for archive listings).
4. **Self-healing archives**: Reed–Solomon recovery records (configurable %, like RAR's), plus `repair`.
5. **Versioned archives**: `zipling update` appends a snapshot that shares unchanged chunks — Time-Machine-style history in one file.
6. **Optimise existing archives**: convert a zip/7z/rar to .zlg, or to a smaller but universally compatible zip, losslessly.
7. **Size & time estimate before you compress** (sampling), with a "best for…" recommendation (sharing with Windows → zip, Mac only → .zlg, maximum → Zipling max).
8. **Filename-encoding repair** for legacy Windows zips (CP437/Shift-JIS/GBK/CP866 detection).
9. **Smart extraction**: folder only when needed, optional move-to-Trash, quarantine propagated to extracted files like Archive Utility (Gatekeeper safety).
10. **Automation**: App Intents / Shortcuts, AppleScript, Folder Actions, a menu-bar drop target.
11. **Privacy**: no telemetry, no network except optional update checks.
12. **One format, one decoder, every level**: fast, balanced, max and ultra are a setting, not separate formats. Each block records its own codec, so any .zlg archive opens the same way, and one archive can mix codecs (it already does, per lane). An **Auto** setting picks the level from the job (sharing → balanced, archiving or backup → max, quick handoff → fast).
13. **Compress fast now, shrink later**: write the archive at *fast* (about 1.5 s for 736 MB), then, while the Mac is idle and on power, re-pack it block by block to *max* in the background. Blocks are independent, so the upgrade is resumable and swapped in atomically. The content is byte-for-byte unchanged; only the size drops (about 33% on the mixed benchmark folder).

## 5. Phases

| Phase | Scope | Exit criteria |
|---|---|---|
| **0 — Engine proof** *(done in this repo)* | Zipling format, transforms, dedup, parallel blocks, CLI, round-trip tests, benchmarks | beats 7z ultra on size and speed on real Mac data, lossless |
| **1 — App MVP** | SwiftUI app shell, Rust FFI (UniFFI), libarchive backend in XPC helper, browser, extract/compress sheets, drag & drop, progress, Quick Look preview | daily-driver for zip/7z/rar/tar/.zlg |
| **2 — Breadth** | 7-Zip + XADMaster backends, all read formats, write matrix, split + encryption everywhere, edit in place | every comparison row green |
| **3 — Differentiators** | Finder mount (FSKit), Spotlight, recovery records, versioned archives, optimise-archive, estimate | features nobody else has |
| **4 — Engine v2** | encryption + recovery in .zlg; multi-threaded Lepton decoding for folders of a few huge photos (max extraction already beats 7z on mixed data, but not yet on PDF/PNG folders); a multi-threaded LZMA match finder for inputs with fewer blocks than cores; x86-64 BCJ; recursive unwrapping (zip-in-zip, JPEG-in-PDF); PNG pixel model; WAV/AIFF predictor; context-mixing text codec for ultra; memory budgeting for 8 GB Macs; deterministic block order | another 10–30% on mixed data, and extraction at least as fast as 7z at every level |
| **5 — Ship** | notarisation, Sparkle updates or Mac App Store, localisation, docs site | 1.0 |

## 6. Open decisions

* ~~Product name~~: **Zipling**, archives use **`.zlg`** (decided 2026-10-26).
* Licence: closed-source commercial (a paid utility) vs open source. This constrains dependencies:
  MacPacker is GPL-3 (its code can't be reused in a closed app); LGPL libraries
  (7-Zip, XADMaster) are fine when dynamically linked.
* Distribution: direct (notarised + Sparkle; everything possible) vs Mac App Store
  (sandbox; some extensions and the optional `rar` download get harder) — or both.
* Minimum macOS: proposal macOS 15 (FSKit needs 15.4+); the UI targets the macOS 26
  design language.
* Free vs Pro: which features sit in each (to discuss).

## 7. Backlog

Parked on purpose, to revisit before a future release.

**Strong encryption** (wanted for a future release, not designed yet). People should be able to use Zipling to encrypt their files, not only to compress them:

* Password-protected `.zlg`: every block encrypted with a key derived from the password (a slow, memory-hard key function so guessing is expensive), names hidden by encrypting the index, wrong passwords told apart from damage, and the per-block hashes made so they reveal nothing about the contents. This is a change to the engine and the format: older Zipling versions couldn't open encrypted archives.
* Encrypting existing ZIPs (and other archives) in place, and encrypting plain files and folders.
* Split `.zlg` archives (parts on several files), also an engine change.

**Requires an engine change** (Stage 8 features that change the Arca engine and, for the first two, the `.zlg` format; parked until the engine is opened up again):

* **Versioned archives.** One `.zlg` keeps several snapshots of a folder; each update adds only what changed, and any earlier version can be restored. Requires an engine change: a chain of indexes sharing data blocks, and an "add a snapshot" operation. Format change.
* **Recovery data inside the `.zlg`.** Self-healing stored in the archive itself, so one file can repair its own damage. Requires an engine change: the engine writes the recovery data and uses it when reading. Format change. (A separate recovery file next to any archive needs no engine change and is in the app.)
* **Shrink later, block by block.** Re-pack a Fast archive to Max one block at a time, so the job can pause, resume and survive a restart. Requires an engine change: swapping individual blocks and rewriting the index. (Re-packing the whole archive in one go needs no engine change and is in the app.)

**Waiting on an Apple Developer account:**

* Stage 7, mounting an archive in Finder as a read-only disk (FSKit). FSKit needs a restricted entitlement that only a paid Developer Program account can provision, so it isn't written until it can be run and tested.
* A top-level Zipling menu in Finder (Finder Sync extension). Today the right-click actions come from macOS Services.
* Quick Look in Finder for 7z, RAR and TAR. Today it covers `.zlg` and ZIP: the sandboxed extension can't run the helpers.
* Developer ID signing and notarisation for anything shipped.
