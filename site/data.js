// Data shared by every page. Benchmarks come from the app benchmark of 31 October to
// 1 November 2026 (bench/results-2026-10-31-apps*.jsonl): the installed Zipling app driven
// through its AppleScript commands and timed by its own clock, 7-Zip, WinRAR, zstd and xz as
// processes in the same session, Finder measured in Finder itself; Default measured again on
// 1 November 2026 in the app rebuilt with the engine's code lane (bench/results-2026-11-01-apps-
// default.jsonl); competitor capabilities from MacPacker's comparison (macpacker.app/en/compare,
// 10 Aug 2026).

window.ZIPLING = {
  // Tools in chart order, grouped by class so each Zipling level sits next to the
  // settings it is compared with. "sep" marks rows measured in a separate session.
  groups: { fast: "Fast settings", normal: "Normal settings", best: "Strongest settings" },
  tools: [
    { id: "finder", group: "fast", name: "Finder Compress", detail: "Finder's Compress and Archive Utility, timed in Finder", us: false },
    { id: "zst3", group: "fast", name: "tar + zstd -3", detail: "zstd's default level", us: false },
    { id: "rarf", group: "fast", name: "WinRAR Fastest", detail: "rar -m1", us: false },
    { id: "fast", group: "fast", name: "Zipling Fast", detail: "Fastest setting", us: true },
    { id: "7zn", group: "normal", name: "7-Zip Normal", detail: "7z -mx5, 7-Zip's default", us: false },
    { id: "rarn", group: "normal", name: "WinRAR Normal", detail: "rar -m3, WinRAR's default", us: false },
    { id: "xz6", group: "normal", name: "tar + xz -6", detail: "xz's default level", us: false },
    { id: "balanced", group: "normal", name: "Zipling Default", detail: "Balanced, the default", us: true },
    { id: "7z", group: "best", name: "7-Zip Ultra", detail: "7z -mx9, the engine behind Keka and BetterZip", us: false },
    { id: "rar", group: "best", name: "WinRAR Best", detail: "rar -m5, solid", us: false },
    { id: "zst", group: "best", name: "tar + zstd -19", detail: "tar.zst -19 --long", us: false },
    { id: "xz", group: "best", name: "tar + xz -9e", detail: "tar.xz -9e", us: false },
    { id: "max", group: "best", name: "Zipling Max", detail: "Smallest archives at speed", us: true },
    { id: "ultra", group: "best", name: "Zipling Ultra*", detail: "Experimental, not in the released app", us: true },
  ],

  // [size MB, compress s, extract s] per tool id.
  datasets: [
    { id: "mixed", name: "Mixed folder", mb: 736, what: "All five sets below in one folder", why: "A realistic \"compress this folder\" job",
      r: { finder: [324.6, 24.1, 7.90], zst3: [294.6, 5.6, 5.50], rarf: [309.8, 5.6, 3.18], fast: [281.1, 0.51, 0.82],
           "7zn": [237.8, 26.1, 2.96], rarn: [277.8, 10.6, 3.26], xz6: [251.2, 25.1, 5.79], balanced: [214.3, 8.8, 1.62],
           "7z": [235.0, 48.5, 3.84], rar: [247.4, 21.6, 2.98], zst: [245.7, 46.1, 5.51], xz: [236.6, 87.8, 7.10], max: [192.6, 19.3, 2.55], ultra: [189.3, 28.1, 2.73] } },
    { id: "docs", name: "PDFs & screenshots", mb: 89, what: "250 PDFs and 600 PNGs from system and Xcode resources", why: "Office, PDF and screenshot-heavy folders",
      r: { finder: [50.2, 2.0, 1.25], zst3: [45.1, 0.40, 0.42], rarf: [50.0, 0.84, 0.39], fast: [41.1, 0.08, 0.12],
           "7zn": [38.3, 2.5, 0.80], rarn: [49.8, 1.1, 0.36], xz6: [43.3, 4.1, 0.64], balanced: [22.1, 1.4, 0.26],
           "7z": [38.2, 2.9, 0.83], rar: [38.7, 1.7, 0.30], zst: [38.8, 6.3, 0.41], xz: [38.8, 20.8, 1.15], max: [18.2, 10.5, 0.45], ultra: [17.8, 15.0, 0.46] } },
    { id: "jpeg", name: "Photos", mb: 98, what: "400 JPEGs from system and Xcode resources", why: "Photo folders",
      r: { finder: [93.3, 3.1, 1.23], zst3: [91.7, 0.24, 0.25], rarf: [92.5, 1.0, 0.30], fast: [89.1, 0.05, 0.08],
           "7zn": [85.3, 6.0, 1.59], rarn: [90.9, 1.1, 0.36], xz6: [89.0, 8.2, 0.65], balanced: [69.2, 2.0, 0.66],
           "7z": [85.3, 6.4, 1.58], rar: [86.2, 2.9, 0.38], zst: [85.7, 14.5, 0.25], xz: [85.6, 38.5, 1.99], max: [69.2, 2.1, 0.68], ultra: [69.2, 2.1, 0.70] } },
    { id: "apps", name: "Mac apps", mb: 174, what: "8 apps from /System/Applications: universal Mach-O, asset catalogs, nibs, strings", why: "Software and .app bundles",
      r: { finder: [87.4, 6.7, 2.42], zst3: [76.5, 1.1, 1.14], rarf: [78.7, 1.2, 0.70], fast: [72.3, 0.13, 0.23],
           "7zn": [55.3, 16.8, 1.41], rarn: [64.7, 2.5, 0.76], xz6: [59.0, 8.5, 1.26], balanced: [56.4, 1.4, 0.26],
           "7z": [54.1, 27.7, 1.66], rar: [58.7, 5.4, 0.70], zst: [59.4, 21.3, 1.15], xz: [54.3, 57.9, 2.40], max: [49.2, 12.1, 0.70], ultra: [48.6, 14.5, 0.71] } },
    { id: "source", name: "Source code", mb: 162, what: "89 Rust crates, including vendored C (zstd, xz)", why: "Code repositories",
      r: { finder: [25.1, 6.5, 5.16], zst3: [15.1, 3.7, 3.53], rarf: [21.8, 1.7, 1.42], fast: [13.5, 0.25, 0.52],
           "7zn": [9.2, 12.9, 1.14], rarn: [18.2, 2.4, 1.46], xz6: [10.0, 6.2, 3.48], balanced: [9.5, 1.8, 0.61],
           "7z": [8.5, 21.9, 1.16], rar: [10.2, 4.5, 1.23], zst: [9.3, 25.8, 3.50], xz: [8.3, 66.1, 3.48], max: [8.3, 1.6, 0.83], ultra: [8.1, 1.7, 1.01] } },
    { id: "silesia", name: "Generic data", mb: 212, what: "The standard Silesia compression corpus", why: "Data with no file-type tricks to exploit",
      r: { finder: [68.2, 6.2, 1.32], zst3: [66.3, 0.15, 0.21], rarf: [66.7, 0.92, 0.41], fast: [65.2, 0.17, 0.11],
           "7zn": [49.6, 22.9, 0.98], rarn: [54.2, 3.6, 0.45], xz6: [49.5, 13.2, 0.51], balanced: [57.3, 5.0, 0.15],
           "7z": [48.7, 48.2, 1.56], rar: [53.1, 6.4, 0.45], zst: [52.6, 30.1, 0.23], xz: [48.4, 83.8, 1.99], max: [46.7, 6.2, 0.59], ultra: [45.9, 13.5, 0.84] } },
  ],

  // Comparison. Cell codes: "y" yes, "p" partial, "n" no, "plan" planned for Zipling 1.0.
  // Format cells are [open, create].
  apps: ["Zipling", "MacPacker", "7-Zip", "The Unarchiver", "Keka", "BetterZip"],
  compare: [
    { group: "Compression", rows: [
      { name: "Recompresses photos losslessly", note: "JPEGs about 30% smaller with Zipling's own coefficient coder, restored bit for bit", cells: ["y", "n", "n", "n", "n", "n"] },
      { name: "Unpacks PDFs, PNGs, Office files and disk images", note: "Reverses the compression already inside them and recompresses it; disk images a quarter smaller at Max", cells: ["y", "n", "n", "n", "n", "n"] },
      { name: "Stores repeated data once", note: "Deduplication across a whole folder. 7-Zip finds repeats only within its compression window", cells: ["y", "n", "p", "n", "p", "p"] },
      { name: "Checks every block on extraction", note: "A keyed checksum on every block and a BLAKE3 hash on the index. Others use CRC32", cells: ["y", "n", "n", "n", "n", "n"] },
      { name: "Smaller than 7-Zip Ultra", note: "18% on the mixed benchmark folder at Max, 8% at the default setting", cells: ["y", "n", "n", "n", "n", "n"] },
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
