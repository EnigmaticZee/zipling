// Data shared by every page. Benchmarks come from docs/RESEARCH.md ("Presets locked,
// every level on every set", 2026-10-31), the quiet runs of 2026-10-30 for the
// command-line tools, and Finder measured in Finder itself on 2026-10-31
// (bench/results-2026-10-31-apps-finder.jsonl); competitor capabilities from
// MacPacker's comparison (macpacker.app/en/compare, 10 Aug 2026).

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
      r: { finder: [324.6, 24.1, 7.90], zst3: [296.1, 5.6, 5.60], rarf: [309.8, 5.7, 3.10], fast: [281.1, 0.52, 0.60],
           "7zn": [237.8, 25.6, 2.99], rarn: [277.8, 10.4, 3.26], xz6: [251.2, 24.5, 6.25], balanced: [216.0, 8.5, 1.28],
           "7z": [235.0, 47.8, 3.88], rar: [247.4, 20.8, 2.97], zst: [246.2, 39.1, 5.56], xz: [236.6, 89.6, 7.67], max: [192.6, 19.1, 2.37], ultra: [189.3, 29.4, 2.43] } },
    { id: "docs", name: "PDFs & screenshots", mb: 89, what: "250 PDFs and 600 PNGs from system and Xcode resources", why: "Office, PDF and screenshot-heavy folders",
      r: { finder: [50.2, 2.0, 1.25], zst3: [45.1, 0.41, 0.42], rarf: [50.0, 0.77, 0.36], fast: [41.1, 0.06, 0.07],
           "7zn": [38.3, 2.4, 0.79], rarn: [49.8, 0.98, 0.34], xz6: [43.3, 4.1, 0.73], balanced: [22.1, 1.3, 0.23],
           "7z": [38.2, 2.8, 0.79], rar: [38.7, 1.6, 0.28], zst: [39.0, 5.2, 0.42], xz: [38.8, 22.3, 1.44], max: [18.2, 10.8, 0.41], ultra: [17.8, 15.4, 0.41] } },
    { id: "jpeg", name: "Photos", mb: 98, what: "400 JPEGs from system and Xcode resources", why: "Photo folders",
      r: { finder: [93.3, 3.1, 1.23], zst3: [91.8, 0.24, 0.24], rarf: [92.5, 1.1, 0.30], fast: [89.1, 0.05, 0.05],
           "7zn": [85.3, 5.9, 1.54], rarn: [90.9, 1.1, 0.36], xz6: [89.0, 8.4, 0.76], balanced: [69.2, 2.1, 0.69],
           "7z": [85.3, 6.2, 1.54], rar: [86.2, 2.9, 0.37], zst: [85.7, 12.8, 0.25], xz: [85.6, 38.8, 2.40], max: [69.2, 2.1, 0.68], ultra: [69.2, 2.2, 0.69] } },
    { id: "apps", name: "Mac apps", mb: 174, what: "8 apps from /System/Applications: universal Mach-O, asset catalogs, nibs, strings", why: "Software and .app bundles",
      r: { finder: [87.4, 6.7, 2.42], zst3: [77.2, 1.1, 1.17], rarf: [78.7, 1.2, 0.67], fast: [72.3, 0.14, 0.15],
           "7zn": [55.3, 18.1, 1.44], rarn: [64.7, 2.4, 0.75], xz6: [59.0, 9.2, 1.35], balanced: [56.4, 1.4, 0.21],
           "7z": [54.1, 30.4, 1.68], rar: [58.7, 4.9, 0.68], zst: [59.6, 19.5, 1.20], xz: [54.3, 66.0, 2.68], max: [49.2, 12.5, 0.64], ultra: [48.6, 15.0, 0.63] } },
    { id: "source", name: "Source code", mb: 162, what: "89 Rust crates, including vendored C (zstd, xz)", why: "Code repositories",
      r: { finder: [25.1, 6.5, 5.16], zst3: [15.2, 3.9, 3.58], rarf: [21.8, 1.7, 1.42], fast: [13.5, 0.25, 0.37],
           "7zn": [9.2, 13.9, 1.15], rarn: [18.2, 2.4, 1.41], xz6: [10.0, 6.5, 3.64], balanced: [11.2, 2.0, 0.36],
           "7z": [8.5, 23.7, 1.17], rar: [10.2, 4.4, 1.21], zst: [9.4, 23.8, 3.58], xz: [8.3, 71.4, 3.58], max: [8.3, 1.6, 0.68], ultra: [8.1, 1.7, 0.92] } },
    { id: "silesia", name: "Generic data", mb: 212, what: "The standard Silesia compression corpus", why: "Data with no file-type tricks to exploit",
      r: { finder: [68.2, 6.2, 1.32], zst3: [66.6, 0.15, 0.21], rarf: [66.7, 0.9, 0.41], fast: [65.2, 0.16, 0.06],
           "7zn": [49.6, 23.9, 1.00], rarn: [54.2, 3.4, 0.45], xz6: [49.5, 14.0, 0.55], balanced: [57.3, 4.7, 0.10],
           "7z": [48.7, 53.7, 1.59], rar: [53.1, 6.1, 0.45], zst: [52.7, 28.3, 0.24], xz: [48.4, 94.8, 2.24], max: [46.7, 6.1, 0.57], ultra: [45.9, 13.6, 0.81] } },
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
