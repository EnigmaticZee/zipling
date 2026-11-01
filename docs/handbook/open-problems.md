# Open problems and planned work (1 November 2026)

## Where the engine still loses, and why

Level for level, 67 of 198 rows are lost. Every one has a structural cause; none
is a tuning matter.

- **Default against the LZMA normal settings on size** (7-Zip Normal, Zipgeist 7z
  Normal, xz -6): Apple apps 2% behind 7-Zip Normal, source 3%, third-party apps 3
  to 9%, Silesia 15%. Default is 2 to 12 times faster to compress and 2 to 7 times
  faster to extract than them. The apps are Mach-O in the binary lane: LZMA-class
  codecs gain 9 to 15% there at 6 to 9 times the time and lose more rows than they
  win; zstd 15 on Mach-O alone costs 4.4 times the time for 0.4%. Silesia's three
  LZMA rows need LZMA on its tars and binaries, 2.4 to 4 times the make time and 7
  times the extraction. Nothing cheaper buys these.
- **Default against the near-store ZIPs on make time** (Zipgeist ZIP Normal on 8
  sets, WinRAR Normal on photos and PDFs): they do almost no work; Default runs the
  transforms. Fast wins every one of these rows on all three measures.
- **Max and Ultra on make time where the transforms buy the size**: PDFs (52%
  smaller than 7-Zip Ultra in 10.5 s against 2.9), disk images (25% smaller in 40.6
  s against 18.9, and 4.3 s to extract against 0.6 to 1.6), Apple apps against
  Zipgeist's best (6 to 10% smaller, 12.1 s against 6.2). Default already beats
  those rivals on all three measures on the same sets.
- **Fast on Silesia against zstd -3**: 0.17 against 0.15 s at 1.6% smaller and
  twice the extraction speed; the CPU floor for zstd -3's ratio.
- **Ultra** adds 0 to 3% over Max for 5 to 117% more time; experimental, not in the app.

## The text lane at Default: measured, not taken

bsc (BWT) on Default's text lane: 2 to 7% smaller and 37 to 54% faster to compress
wherever there is text (mixed folder 9.0 to 5.7 s), but extraction 1.3 to 4.6
times slower there (mixed 1.35 to 1.93 s, Silesia 0.09 to 0.41 s), still under
every rival of Default's class. Plus 2 rows. Not taken, to keep the extraction
headline (2.6x over 7-Zip, 5x over Finder, which would become about 2.0x and 4x).
Before revisiting: try libbsc's static QLFC coder and a wider parallel inverse
BWT for the decode time; smaller text blocks do not help (measured: 2 to 12 MiB,
extraction unchanged, size up to 5% worse). Numbers in
`bench/results-2026-11-01-default-lanes-r2.jsonl`, `-r3.jsonl`.

## Planned: the recovery record (next engine work, before the format freeze)

Decided with the owner on 1 November 2026. Today the app can write a separate
recovery file next to any archive (`Name.zip.recovery`, Reed-Solomon over k
slices with m parity slices in GF(256), a hash per slice; `app/Zipling/Archives/Recovery.swift`).
The plan:

- Inside the `.zlg`: a recovery section between the index and the footer: slice
  size and counts, a hash per slice, the parity, and a copy of the footer so a
  damaged archive end can be found by scanning back for the section's marker. A
  footer field points at it. Format version 4; older archives unchanged. Reed-
  Solomon over GF(2^16) so a large archive has thousands of small slices and a bad
  sector costs one small slice. A `repair` operation that locates damaged slices
  by hash and rebuilds them, exposed on the CLI and as three FFI calls (has
  record, check, repair). Tests that corrupt random slices up to the budget,
  truncate the tail within it, and damage the footer.
- In the app: one setting, Off (default), In the archive (the default once on,
  `.zlg` only), Separate file (always available, the only form for ZIP and 7z);
  the amount as a stepper or slider from 1 to 50% in whole percents, default 3%,
  quick picks at 3, 5, 10 and 25, the cost shown live ("adds about 21 MB, repairs
  up to 21 MB of damage"); the inspector shows the protection in megabytes. A
  record of X% repairs any X% of the file, wherever the damage falls; truncation
  past the record's size is unrecoverable at any percentage.
- Why off by default: on by default adds 3% to every archive and would hand back
  benchmark rows where the margin over 7-Zip is 2 to 4%.
- Estimate: engine two to three days, app one to two, tests included.

## Planned: Ultra as a context-mixing level (after release)

Direction from the owner: Ultra becomes a context-mixing compressor, the ratio of
the PAQ family, in minutes not days, extraction as slow as compression (the
decoder rebuilds the same model). Estimates, not measurements: a mid-weight
mixer at about 1 MB/s per core, Silesia around 40 MB instead of 46.7 in about 30
s instead of 6, the mixed folder around 170 MB instead of 193 in about two
minutes each way. Expected gains: 10 to 20% over bsc on text, 5 to 10% over LZMA
on code and binaries, little on photos (our coder is near the frontier). The
lanes let a text model run on text and a code model on code; the blocks let it
run on all cores. The constraint is memory: these models want hundreds of MB
each, and ten in parallel must still fit an 8 GB Mac, so the model's memory is
capped per block.

First step, one afternoon: run an existing open mixer over lane dumps
(`ARCA_DUMP_BLOCKS`) from the sets and measure the gain per lane. Adopt only if
text gains about 15% and code about 10%; otherwise drop it on evidence. The
record compressors (cmix, nncp: about 110 MB on the 1 GB Wikipedia text against
LZMA's 210) are out of reach at any speed users would accept.

## Memory

Peak while compressing the mixed folder: Max 3.6 GB, Fast 1.0 to 3.3 GB; WinRAR
Best 0.6 GB, 7-Zip Ultra 6.9 GB, xz -9e 3.7 GB. The owner's stance: use up to the
hungriest rival's memory, stay fast on 8 GB Macs, ideally beat WinRAR on memory
too. The budget is half of physical memory; the levers are blocks in flight,
block size and the LZMA2 dictionary. Not yet worked on.

## The app's overhead on extraction

About 30 ms before the engine starts (the listing call and its JSON decode, the
format check, two main-actor hops) and about 9 µs a file after it (the safety
walk over the extracted tree). Turns no benchmark row. Left for the app work:
skip the listing for a whole-archive extract, take the safety walk's per-file
cost down; worth about 0.1 s on a 7,700-file tree.

## Smaller items

- Default's archive size varies 0.05 to 0.3% between runs (one or two zstd
  workers a block, decided by cores free). The benchmark loaders take the larger
  size. A deterministic rule would cost time on big folders; not pursued.
- The lzt codec (parked) is 15 to 21% bigger than LZMA on PDFs, source and apps;
  its gap is in the token symbols (28% of its output on mozilla). It extracts
  Silesia in 0.22 s against zstd -19's 0.24 at 50.2 MB.
- Code-signature page hashes are 0.4% of compressed apps; a small idea, not a lever.
- The Fast level's two Silesia make rows are at the machine's floor; zstd's own
  tables would be 9% faster for 0.9% of size and win no row.
