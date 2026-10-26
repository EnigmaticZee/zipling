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

Next:

- [ ] Switch to the official `rustup` toolchain (Homebrew's Rust standard library targets macOS 27 only) and build a universal (arm64 + x86_64) engine
- [ ] Drag items out to Finder (file promises); Quick Look on inner files; `.zlg` document icon
- [ ] XPC helper + libarchive (zip, 7z, rar, tar.*, iso, xar, cab, lha, cpio, deb, rpm) behind an `ArchiveProvider` protocol
- [ ] Nested-archive VFS, job queue window, Dock progress
- [ ] Quick Look extension, Finder context menu, Share extension
- [ ] Developer ID signing, hardened runtime, notarisation, Sparkle
