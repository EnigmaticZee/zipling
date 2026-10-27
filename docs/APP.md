# macOS app: architecture and UI concept

## Process layout

```
┌──────────────────────────── Zipling.app (SwiftUI + AppKit) ────────────────────────────┐
│  Browser windows · Compress/Extract sheets · Job queue · Settings · Menu-bar extra │
│        │ async/await                                                               │
│  ArchiveKit (Swift package): ArchiveProvider protocol, VFS for nested archives,    │
│  job scheduler, progress/cancel, Keychain passwords                                │
└────────┬───────────────────────────────────────────────────────────────────────────┘
         │ XPC (one helper per job, sandboxed: no network, only granted files)
┌────────▼───────────────────────────────────────────────────────────────────────────┐
│ ZiplingWorker.xpc                                                                    │
│   arca-core (Rust, via UniFFI)  libarchive  7-Zip dylib  XADMaster  AppleArchive   │
└────────────────────────────────────────────────────────────────────────────────────┘
Extensions: Quick Look preview + thumbnail · Finder context menu (Finder Sync /
Services) · Share · FSKit mount module · Spotlight importer · App Intents
```

**Why XPC:** archives arrive from the internet, and decoders for RAR, StuffIt, ISO and
the rest are large C/C++ codebases. Running them out of process means a malicious
archive can't crash or compromise the app, and a hung decoder is just a killed helper.

**Why Rust for the engine:** memory safety on untrusted input, plus first-class
access to the components that make Zipling archives small (Lepton, preflate, BLAKE3, FastCDC).
UniFFI generates Swift bindings, and a single universal static library is linked
into the XPC helper.

## UI concept

Native SwiftUI in the macOS 26 design language: Liquid Glass sidebar and toolbar,
SF Symbols, full keyboard navigation, VoiceOver, light and dark modes. AppKit
`NSOutlineView` sits underneath the file list so 1M-entry archives scroll at 120 Hz.

* **Browser window:** sidebar (Recents, Favourites, Jobs) | file list (list, columns
  or gallery with real thumbnails of inner files) | inspector. A breadcrumb path bar
  steps through nested archives (`Project.zip › assets.7z › images`).
* **Inspector:** archive health (format, encryption, integrity, recovery record);
  per file, the compressed size, method and *why* it got smaller ("Lepton −24%",
  "Deflate expanded, then LZMA").
* **Compress sheet:** drop files, and Zipling recommends a format based on the
  destination ("Sharing with Windows → ZIP", "Mac only, smallest → Zipling") and shows
  a live estimate of size and time from sampling. Toggles cover encrypt, split,
  exclude Mac junk (`.DS_Store`, `__MACOSX`, `._*`), recovery record and level.
* **Extraction:** one click "Extract here" with smart folder creation; dragging out
  to Finder uses file promises, so nothing is decoded until the drop lands. Progress
  shows as a floating HUD, a Dock badge and a notification.
* **Quick Look everywhere:** press Space on an archive in Finder to see its contents
  tree. Press Space on a file inside the browser to preview it, extracting only that
  file.
* **Menu-bar extra:** a drop target that compresses with your last settings.

## Milestone 1 (app MVP) checklist

Done (first slice):

- [x] Xcode project (XcodeGen), macOS 15+ target, `.zlg` registered as the app's document type
- [x] `arca-ffi` C API: list, pack, extract (selected paths), verify, progress, cancel; panic-safe
- [x] Home window: drop zone, Choose Files / Open Archive, recent archives
- [x] Compress sheet: four levels, destination, live progress with ETA, cancel, "why it got smaller" results
- [x] Browser window: Finder-style table (icons, sort, search, breadcrumbs, Saved meter, method), inspector, extract selected/all, verify, double-click to open a file
- [x] Safe extraction: staging folder, Finder-style names ("docs 2"), nothing overwritten, nothing left on cancel
- [x] App icon; app tests through the real engine (`ZiplingTests`)
- [x] Every other format through the unmodified 7-Zip helper (`tools/7zip`), run as a separate process: ZIP, 7z, TAR (+ gz/bz2/xz/zst, opened as their tar), DMG (HFS+ and APFS), ISO, WIM, split archives (`.001`) and ~50 more
- [x] Encrypted archives: the password goes to the helper on stdin (never the command line), and a wrong one asks again
- [x] Nested archives: double-click an archive inside an archive to browse it, with breadcrumbs (`Backup.dmg › photos.7z`); Back leaves it
- [x] Safe extraction for every format: paths cleaned the way 7-Zip writes them, entries 7-Zip refuses (writes through links) reported instead of failing, links pointing outside removed, quarantine copied from the archive to every item, `__MACOSX`/`._` metadata merged back (extended attributes only, never ACLs), a free-space check up front

- [x] Classic Mac and long-tail formats (StuffIt, BinHex, MacBinary, Compact Pro…) through The Unarchiver's unmodified `lsar`/`unar` (`tools/unar`), resource forks and Finder info included
- [x] File-name repair: ZIPs whose raw names are in a legacy code page (read from the central directory, since 7-Zip drops those bytes) are read with detected encodings (Shift-JIS, GB18030, Big5, CP437 as ZIP's fallback…), and the inspector lets you pick another
- [x] RAR 4 and RAR 5 samples in the tests (embedded, from libarchive's suite)
- [x] Browse: folder tree sidebar, Quick Look on inner files (space, ⌘Y; follows the selection), a live preview pane, search inside nested archives, Finder keys (type-to-select, ⌘↑, ⌘↓, ⌘[, ⌘F), 100k entries open in ~1.5 s
- [x] Get files out: drag out to Finder (extracted on drop), Extract Here (one item comes out as is, several go in a folder named after the archive), Keep Both / Replace (to the Trash) / Skip / Ask on conflicts, move to Trash only after a complete, checksummed extraction, background jobs with Dock progress, batch Extract Here
- [x] Create: Zipling, ZIP (AES-256, UTF-8 names), 7z (hidden names), TAR.GZ, TAR.XZ; split presets; Mac clutter left out; saved presets; a suggestion based on what's being compressed; every archive read back before it's handed over
- [x] Edit in place: add/replace (drop on the list), delete, rename, new folder; ZIP, 7z and TAR through 7-Zip, Zipling and compressed tars rebuilt; edit a file in its own app and put it back
- [x] System: Finder right-click (Services), Quick Look extension for `.zlg` and ZIP, Share, Dock drops (Option to extract), menu-bar drop zone with progress, Settings

Next:

- [ ] Switch to the official `rustup` toolchain (Homebrew's Rust standard library targets macOS 27 only) and build a universal (arm64 + x86_64) engine
- [ ] Run `tools/7zip/build.sh` and `tools/unar/build.sh` to bundle the helpers (Release builds require them)
- [ ] Finder Sync extension (a top-level Zipling menu), once there's a Developer ID to sign it
- [ ] Quick Look for 7z, RAR and TAR archives (the sandboxed extension can't run the helpers yet)
- [ ] Mounting archives as disks (FSKit)
- [ ] Developer ID signing, hardened runtime, notarisation, Sparkle
