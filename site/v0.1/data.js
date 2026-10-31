// Data shared by every page. Benchmarks come from docs/BENCHMARKS.md; competitor
// capabilities from MacPacker's comparison (macpacker.app/en/compare, 10 Aug 2026).

window.ZIPLING = {
  // Tools in chart order. "sep" marks rows measured in a separate session (times not comparable).
  tools: [
    { id: "finder", name: "Finder Compress", detail: "ZIP (Deflate), like Archive Utility and MacPacker", us: false, sep: true },
    { id: "7z", name: "7-Zip Ultra", detail: "7z -mx9, the engine behind Keka and BetterZip", us: false },
    { id: "zst", name: "tar + zstd -19", detail: "tar.zst -19 --long", us: false, sep: true },
    { id: "xz", name: "tar + xz -9e", detail: "tar.xz -9e", us: false, sep: true },
    { id: "fast", name: "Zipling Fast", detail: "Fastest setting", us: true },
    { id: "balanced", name: "Zipling Default", detail: "Balanced, the default", us: true },
    { id: "max", name: "Zipling Max", detail: "Smallest archives", us: true },
    { id: "ultra", name: "Zipling Ultra", detail: "Max plus a PPMd text model, opt-in", us: true },
  ],

  // [size MB, compress s, extract s] per tool id.
  datasets: [
    { id: "mixed", name: "Mixed folder", mb: 736, what: "All five sets below in one folder", why: "A realistic \"compress this folder\" job",
      r: { finder: [322.7, 19.8, 9.32], "7z": [233.4, 49.3, 3.84], zst: [244.1, 46.5, 5.52], xz: [235.1, 97.9, 7.28],
           fast: [286.8, 1.4, 0.88], balanced: [221.5, 9.7, 2.30], max: [192.8, 51.6, 3.50], ultra: [191.0, 37.9, 7.45] } },
    { id: "docs", name: "PDFs & screenshots", mb: 89, what: "250 PDFs and 600 PNGs from system and Xcode resources", why: "Office, PDF and screenshot-heavy folders",
      r: { finder: [48.6, 1.5, 0.68], "7z": [36.6, 2.7, 0.82], zst: [37.1, 8.0, 0.42], xz: [36.9, 22.3, 1.10],
           fast: [40.0, 0.1, 0.09], balanced: [20.8, 2.0, 0.26], max: [16.5, 29.3, 1.20], ultra: [16.5, 29.4, 1.19] } },
    { id: "jpeg", name: "Photos", mb: 98, what: "400 JPEGs from system and Xcode resources", why: "Photo folders",
      r: { finder: [93.3, 2.5, 0.47], "7z": [85.2, 6.0, 1.58], zst: [85.7, 15.1, 0.27], xz: [85.8, 43.3, 2.04],
           fast: [95.0, 0.1, 0.06], balanced: [71.0, 2.4, 1.24], max: [71.0, 2.5, 1.25], ultra: [71.0, 2.5, 1.29] } },
    { id: "apps", name: "Mac apps", mb: 174, what: "8 apps from /System/Applications: universal Mach-O, asset catalogs, nibs, strings", why: "Software and .app bundles",
      r: { finder: [87.4, 5.8, 1.96], "7z": [54.1, 27.6, 1.69], zst: [59.4, 23.2, 1.15], xz: [54.4, 67.0, 2.43],
           fast: [72.4, 0.4, 0.22], balanced: [62.4, 2.4, 0.28], max: [49.8, 23.5, 1.04], ultra: [49.8, 23.5, 1.03] } },
    { id: "source", name: "Source code", mb: 162, what: "89 Rust crates, including vendored C (zstd, xz)", why: "Code repositories",
      r: { finder: [24.8, 5.2, 6.09], "7z": [8.5, 22.0, 1.22], zst: [9.3, 26.6, 3.45], xz: [8.3, 71.5, 3.55],
           fast: [13.5, 0.5, 0.54], balanced: [11.2, 2.1, 0.51], max: [8.3, 25.4, 0.72], ultra: [8.3, 24.9, 3.57] } },
    { id: "silesia", name: "Generic data", mb: 212, what: "The standard Silesia compression corpus", why: "Data with no file-type tricks to exploit",
      r: { finder: [68.2, 5.8, 0.30], "7z": [48.7, 48.3, 1.53], zst: [52.6, 32.9, 0.27], xz: [48.4, 99.3, 2.15],
           fast: [65.7, 0.3, 0.08], balanced: [57.3, 5.1, 0.15], max: [48.6, 33.0, 0.89], ultra: [47.0, 25.1, 5.96] } },
  ],

  // Comparison. Cell codes: "y" yes, "p" partial, "n" no, "plan" planned for Zipling 1.0.
  // Format cells are [open, create].
  apps: ["Zipling", "MacPacker", "7-Zip", "The Unarchiver", "Keka", "BetterZip"],
  compare: [
    { group: "Compression", rows: [
      { name: "Recompresses photos losslessly", note: "JPEGs about a quarter smaller, restored bit for bit", cells: ["y", "n", "n", "n", "n", "n"] },
      { name: "Unpacks PDFs, PNGs and Office files", note: "Reverses their built-in Deflate and recompresses it", cells: ["y", "n", "n", "n", "n", "n"] },
      { name: "Stores repeated data once", note: "Deduplication across a whole folder. 7-Zip finds repeats only within its compression window", cells: ["y", "n", "p", "n", "p", "p"] },
      { name: "Cryptographic integrity check", note: "BLAKE3 hash for every block. Others use CRC32", cells: ["y", "n", "n", "n", "n", "n"] },
      { name: "Smaller than 7-Zip Ultra", note: "17% on the mixed benchmark folder, at Max", cells: ["y", "n", "n", "n", "n", "n"] },
    ] },
    { group: "Everyday use", rows: [
      { name: "Archive browser", note: "See what's inside before extracting", cells: ["y", "y", "p", "n", "n", "y"] },
      { name: "Command-line tool", cells: ["y", "n", "y", "p", "y", "y"] },
      { name: "Finder and Quick Look integration", cells: ["y", "y", "n", "n", "y", "y"] },
      { name: "Browse archives inside archives", cells: ["y", "y", "n", "n", "n", "n"] },
      { name: "Open password-protected archives", cells: ["y", "y", "y", "y", "y", "y"] },
      { name: "Create encrypted archives", note: "AES-256", cells: ["y", "n", "y", "n", "y", "y"] },
      { name: "Open split archives", cells: ["y", "p", "y", "y", "y", "y"] },
      { name: "Create split archives", cells: ["y", "n", "y", "n", "y", "y"] },
      { name: "Edit an archive in place", cells: ["y", "p", "y", "n", "n", "y"] },
    ] },
    { group: "Formats", formats: true, rows: [
      { name: "Zipling (.zlg)", cells: [["y", "y"], ["n", "n"], ["n", "n"], ["n", "n"], ["n", "n"], ["n", "n"]] },
      { name: "ZIP", cells: [["y", "y"], ["y", "y"], ["y", "y"], ["y", "n"], ["y", "y"], ["y", "y"]] },
      { name: "7z", cells: [["y", "y"], ["y", "n"], ["y", "y"], ["y", "n"], ["y", "y"], ["y", "y"]] },
      { name: "RAR", cells: [["y", "n"], ["y", "n"], ["y", "n"], ["y", "n"], ["y", "n"], ["y", "p"]] },
      { name: "TAR, GZIP, XZ", cells: [["y", "y"], ["y", "n"], ["y", "y"], ["y", "n"], ["y", "y"], ["y", "y"]] },
      { name: "Zstandard", cells: [["y", "n"], ["n", "n"], ["y", "n"], ["n", "n"], ["y", "y"], ["y", "y"]] },
      { name: "Disk images (DMG, ISO)", cells: [["y", "n"], ["y", "n"], ["y", "n"], ["p", "n"], ["y", "y"], ["y", "y"]] },
      { name: "StuffIt and classic Mac", cells: [["y", "n"], ["y", "n"], ["n", "n"], ["y", "n"], ["n", "n"], ["y", "n"]] },
    ] },
  ],
};
