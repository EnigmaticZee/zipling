# Arca engine research log

Started 2026-10-28 on branch `zee/arca-research-77726f`. Backup: tag
`before-engine-research` (commit `cc46547`).

**Resuming?** Read this file top to bottom first. "State" says where the work is;
everything below it is the record.

## State

Saved folder, CLI, busy Mac (load 9-12), 2026-10-28 late evening; rivals from the
2026-10-28 head-to-head:

| Level | Make | Size | Extract | Rival (make / size / extract) |
|---|---:|---:|---:|---|
| Fast | 0.7-1.2 s | 301.3 MiB (316 MB) | 0.74-0.88 s | Zipgeist tar.zst 1.24 s / 341 MB / 1.6-1.9 s |
| Default (preset) | 9.4-9.7 s | 245.47 MiB (257 MB) | 2.2-2.3 s | Zipgeist 7z 17.2 s / 285 MB / 3.0 s |
| Default, all options* | **6.8-7.4 s** | **233.66 MiB (245 MB)** | 2.7-2.9 s | |
| Max (preset) | 32.8-33.6 s | 211.69 MiB (222 MB) | 3.0 s | Zipgeist best 20.0 s / 284 MB / 3.7 s |
| Max + bsc** | **20.1-20.7 s** | **211.58 MiB** | 3.35 s | |
| Ultra + bsc** | (mixed set: 32.0 s vs 40.9) | smaller | 2-3x faster than PPMd | |
| ZIP extract | 0.91-0.99 s | | | Zipgeist 2.1 s |

\* `ARCA_EXP_PIXELS=1 --lane text=bsc:1 --text-block-mib 32 --image-block-mib 32`
\*\* `--lane text=bsc:1 --lane code=bsc:2e`

The options are in the engine but not in the presets (user, 2026-10-28: decide
at the final quiet-Mac benchmarks). Each trades a little extraction time on
text-heavy or photo-heavy archives for much faster making and smaller archives.

Done today: pixel codec 2x faster to decode, extraction writes (creation gate,
write queue, one-call metadata), Lepton without LLVM's memset idiom, engine-side
quarantine, bsc (BWT) codec with threaded decoding, level-9 PNG skip at Default,
image-lane block size. Dead ends: long-range pre-pass, hint-guided LZMA, LZMA
match finders, parallel LZMA2 segments, band-parallel pixel decoding, multi-
symbol Lepton size classes, ARM64 byte planes, GPU, LZRAVEN, Mach-O sections
to bsc.

2026-10-29: zlib replay for PNGs (Default −6.6% pack CPU), x86 filter for the
x86_64 slices of universal binaries (third-party apps −0.7 to −0.8%), DMG disk
images expanded and rebuilt (UDZO −21%/−30%, ULFO −14.5%/−32% at Default/Max),
LZFSE replay (macOS's encoder is lzfse 1.0 with one constant changed; pinned
copy vendored). `Assets.car` expansion is an option (`ARCA_EXP_CAR=1`; cheap
only with bsc). PGO measured (−1 to −4% CPU; for release builds). Later:
real-world sets added (`dmgs`, `thirdparty`, the user's own files, local only),
and an exact, faster match search in the pinned zlib for levels 8-9 (same
output, 2-5.7x faster): level-9 app installer DMGs now re-run at Max and Ultra
(Discord −5.0%, faster both ways; dmgs set −1.6%). DMG expansion is now a switch
per level (`--disk-images on|off`; on at Max and Ultra); the checkpoint measures
Default both ways. Leads:
WebP-style tools in the pixel codec for flat images, similarity ordering, bzip2
(UDBZ) and LZMA (ULMO) DMG chunks.

Checkpoint benchmark done (results below): smallest on every set, fastest to
extract on every set, fastest to make on six of nine; one Arca setting beats
96 of 144 rival results on all three measures at once. Next: the build order
in "After the checkpoint", starting with our own JPEG coder.

## Checkpoint benchmark (the user's spec, 2026-10-29)

On a quiet Mac with the user away; ask before starting, and ask before
downloading or installing anything (Keka, BetterZip, WinRAR).

- Every Arca level, each with and without bsc (BWT), so the user can choose.
- Every rival in the same run, on the same files, alternating with Arca:
  Zipgeist (every setting), 7-Zip (normal and Ultra), Finder, zstd, xz, plus Keka,
  BetterZip and WinRAR if the user approves installing them.
- Every set: the saved folder, all website sets, and the two real-world sets
  (`dmgs`, `thirdparty`; see Test sets).
- One table per set: size, make time, extract time, peak memory; mark every cell
  where any rival beats Arca.
- Show clearly how the .zlg format performs against everything.
- A separate section: what changed against the metrics published on the website
  (`site/benchmarks.html`, `site/data.js`).

## Checkpoint results (2026-10-29)

Every Arca level with and without bsc, against Finder, 7-Zip normal and Ultra,
zstd -3 and -19, xz -6 and -9e, WinRAR (RAR 7.23: Fastest, Normal, Best) and
Zipgeist (all six settings) on all nine sets; every round trip identical. Full
tables, method and versions: `bench/results-2026-10-29-checkpoint.md`; raw runs
in the four `bench/results-2026-10-29-*.jsonl` files.

Zipgeist ran in its own session, through Finder, with Arca alongside: Arca was
×1.30 slower to make and ×1.18 slower to extract there than in the main run
(Finder and the screen were busy). Its times are divided by that factor, set by
set, which errs in Zipgeist's favour. WinRAR's session matched the main run
(×1.01).

**Top of every chart.** Arca has the smallest archive on all nine sets and the
fastest extraction on all nine. Fastest to make on six; the other three:

| Set | Arca's fastest | Rival |
|---|---|---|
| silesia | Fast 0.24 s (65.7 MB) | zstd -3 0.15 s (66.3 MB) |
| dmgs | Fast 0.69 s (619.7 MB) | Zipgeist tar.zst Fast 0.25 s (617.6 MB; 0.35 s measured), zstd -3 0.26 s |
| thirdparty | Fast 2.19 s (465.9 MB) | Zipgeist tar.zst Fast 0.55 s (569.6 MB; 0.73 s measured), zstd -3 1.52 s |

Peak memory: no Arca level comes near Finder's `ditto`, which streams in
2-12 MB; zstd extracts in 7 MB and WinRAR in 29-76 MB. Arca uses 0.2-1.7 GB at
Fast and up to 8 GB at Max on the DMGs (Zipgeist 1-9 GB, 7-Zip Ultra up to
10.6 GB).

**Every rival setting against one Arca setting.** Of 144 rival results (16
rival settings × 9 sets), Arca has one setting that is at least as small,
faster to make and faster to extract, all at once, for **96**. The saved
folder and the website's mixed folder: all 16 beaten, Fast or Default suffices
for every rival. The 48 misses, by cause:

1. **DMGs, 16 of 16.** Without expansion Arca stores the images (619.7 MB),
   0.5-1.7% bigger than rivals that squeeze them (zstd -3 616.1, 7-Zip 609.6);
   with expansion it is 15-17% smaller than every rival at Default (517.1 MB)
   but extraction rebuilds the images: 3.13 s against 0.43-2.7 s. Fast also
   makes slower than storing rivals (0.69 s vs 0.25 s).
2. **Photos, 9.** Nothing between storing (95.1 MB, 0.11 s / 0.06 s) and the
   JPEG codec (71.1 MB, 2.2 s / 1.16 s). Rivals at 85-93 MB extract in
   0.27-0.87 s (zstd, Finder, xz -6, WinRAR, Zipgeist tar.xz).
3. **Silesia, 8.** Fast makes slower than zstd -3 (above). Max + bsc (46.0 MB,
   12.9 s) extracts in 0.90 s against Zipgeist 7z's 0.71 and xz -6's 0.52,
   and plain Max (0.69 s) makes in 23.5 s, slower than they do; zstd -19
   extracts in 0.23 s. Default + bsc extracts in 0.55 s against WinRAR
   Normal/Best's 0.46.
4. **Apple apps, 5.** Default (62.4 MB) is bigger than xz -6 (59.0), Zipgeist
   7z and tar.xz (58.7-59.6) and WinRAR Best (58.7); Max (49.2 MB) beats them
   on size but takes 12.0 s against their 5.2-8.4 s.
5. **PDFs and screenshots, 4.** Default is 43% smaller than Zipgeist 7z and
   tar.xz and WinRAR Best (22.2 MB vs 38.7) but 5-16% slower to make (1.72 s vs
   1.49-1.63 s).
6. **Third-party apps, 4.** Fast against zstd -3 and Zipgeist tar.zst (above);
   Max + bsc 2-5% slower to make than Zipgeist 7z Normal and WinRAR Best
   (47.2 s vs 45.0-46.1 s, within the adjustment's error), and 11-15% smaller.
7. **Source code, 2.** Max + bsc extracts 1-3% slower than Zipgeist 7z Best
   and tar.xz Best (0.91 s vs 0.88-0.90 s): a tie, and 11-20% smaller.

**Against the website** (site/data.js, 2026-10-25/26): every Arca level is
faster now, most of all Max (mixed folder 51.6 → 41.0 s, extract 3.50 → 2.25 s;
docs 29.3 → 9.7 s) and Fast (mixed 1.4 → 0.72 s). Sizes within 1% (Max +1% on
the mixed folder, the fast Max preset), apps smaller (Max −1%, Ultra −3%). The
docs set itself grew 3-5% since (system files changed), so every tool's docs
size is up. Ultra without bsc still extracts slowly (PPMd: mixed 7.07 s). Rivals
moved within noise, except xz -9e making 6-17% faster this session.

**What to fix to top every chart** (a test must show each one faster and at
least as small, as always):

- bsc on by default at Max and Ultra: as small or smaller on every set and as
  fast or faster to make (source code 22.0 → 1.8 s, Silesia 23.5 → 12.9 s,
  mixed 41.0 → 24.5 s); Ultra + bsc also extracts 1.5-6x faster than Ultra. Its
  cost is Max's extraction on text (mixed 2.25 → 2.95 s, source 0.52 → 0.91 s),
  the next item.
- Fast on big incompressible files: 1.61 CPU-s for 620 MB of DMGs against
  zstd -3's 0.79; find the fixed cost (probing, hashing, reading) and make
  storing as fast as zstd's skip. Then a light pass (zstd -1-class) over
  stored data, to be smaller than zstd -3 on DMGs and photos at the same speed.
- A fast photo setting: the JPEG codec decodes at ~10 MB/s per core. Either
  a much faster decoder, or a light JPEG mode below Default (JPEG's own
  arithmetic coding: 5-10% smaller in the literature, at near Huffman speed).
- Default on app bundles: smaller than xz -6 and Zipgeist 7z Normal (62.4 MB
  vs 59) in well under their 5-8 s. On source code and Silesia, Max + bsc is
  already quick enough to beat them.
- DMG extraction with expansion: rebuilding the deflate and LZFSE chunks is the
  time (27.8 CPU-s); faster rebuild or more parallelism.
- Max + bsc's extraction on text: Silesia 0.90 s and source code 0.91 s against
  zstd -19's 0.23 s and Zipgeist 7z's 0.71 s and 0.90 s.
- Memory: Max at 6-8 GB on the large sets is the first to cut; streaming
  like `ditto` is out of reach for a multi-core compressor.

## After the checkpoint: direction and first measurements (2026-10-29 evening)

**Decided with the user.** The aim: every level beats every rival setting in
its class on size, packing and unpacking at once (144 of 144 on the checkpoint
board, 96 today), with leads in multiples where we already win. Memory is not
worked on now. The rule is: no more than the hungriest rival on the same files.
The 8 GB Mac test comes after Arca beats everyone, with every rival under the
same limit. The research works from our own ideas and measurements; papers are
a check, not the source.

Corrections to the first idea list: zlib's deep search is already fast
(`fast_match`), and parallel LZMA2 segments are a dead end (re-indexing the
history costs as much as coding it; see Dead ends).

**Photos: a JPEG coder of our own** (`engine/arca-core/examples/jpegmodel`).
Rivals sit at 85.3-93.3 MB on the photo set (98.40 MB; 397 baseline JPEGs, 3
progressive). Lepton saves 27.7% but decodes at ~10 MB/s per core, and nothing
fast is smaller than 85.3 MB. So the target is the rivals, not Lepton.

The prototype decodes each JPEG's Huffman codes to quantised coefficients.
Re-encoding them with the file's own tables gives back all 397 scans byte for
byte, with no corrections needed. Size of the coefficients, with the cost
charged as an adaptive coder would pay it:

| Model | Coefficients | Saved | Symbols per input byte |
|---|---:|---:|---:|
| A: no neighbours (like JPEG's arithmetic mode) | 76.02 MB | 22.5% | 9.7 binary |
| B: neighbour contexts (above/left nonzero counts and magnitudes per coefficient), DC by LOCO-I prediction | 72.77 MB | 25.8% | 8.4 binary |
| D: B's contexts as multi-symbol alphabets | 73.97 MB | 24.6% | 3.3 + 1.7 raw bits |
| E: D with static tables per image (table cost included) | 74.46 MB | 24.1% | 3.3, no adaptation |

(Saved = against the 98.06 MB of decoded files.) B built as a real coder
(LZMA-style binary range coder): 95.80 MB of scans → 72.66 MB, every round
trip identical. One core: encode 22 MB/s, decode 17 MB/s, too close to
Lepton's speed. One binary decision at a time is the limit there.

E is the design: per-image static tables cost 0.5 MB over adaptive coding and
allow table-driven rANS decoding. With headers and the progressive files that
is about 76 MB, 11% under the best rival. It aims at 50-100+ MB/s per core, so
the set would pack and unpack in about 0.1-0.2 s on 10 cores (zstd -3: 0.26 s /
0.27 s). Lepton stays at Max and Ultra.

**Photos, round 2: smaller than Lepton, built for speed.** Lepton's share of
the 71.1 MB is about 70.2 MB of coefficients (headers about 0.6 MB, the
progressive files about 0.3 MB). Coefficient sizes, adaptive cost unless
marked:

| Model | Coefficients |
|---|---:|
| B (above) | 72.77 MB |
| F: edge coefficients and DC predicted from pixel continuity across block edges, as residuals | 79.15 MB |
| F, with the gradient on both sides of the edge matched | 81.00 MB |
| F, edge prediction as context instead of a residual | 72.81 MB |
| G: expected magnitude from four neighbours (above, left, above-left, above-right), half-octave levels | 72.97 MB |
| M: three context models mixed (logistic mixer + APM) | 71.61 MB |
| **G + block copy** (a block with 6+ nonzeros may copy the AC values of an earlier block with the same count) | **70.63 MB** |
| G + block copy, static tables per image, every table sent | **70.77 MB** |
| G + block copy, built-in tables trained on the other half (2-fold), own table where it pays | **69.91 MB** |
| G + block copy, static, no table cost (bound) | 68.69 MB |

Edge prediction fails on this set: a third of it is screen graphics with hard
edges, where pixels don't continue across block edges. Finer contexts hurt an
adaptive coder (too little data per context) but not a static one.

Block copy is new: Lepton, Brunsli and packJPG model only neighbouring blocks.
57% of the 12.1 M blocks with AC values repeat an earlier block exactly; the
202,435 repeats with 6+ nonzeros hold 4.3% of all nonzeros, mostly 16-256
distinct blocks back. Coding "same as the d-th earlier block with this count"
costs a flag per dense block plus the distance.

With block copy, a static-table coder (table-driven rANS, no adaptation)
matches Lepton's size, or beats it with built-in tables. Next: build it and
measure decoding speed. Mixing + block copy (an estimated ~69.3 MB) is the
candidate for Max and Ultra.

**Photos, round 3: the fixed-table coder, built** (`examples/jpegmodel`,
`fast.rs`; `--fast`). Model G + block copy, static tables per image coded
with the adaptive binary coder, two interleaved rANS states (12-bit
frequencies, 16-bit words), raw bits in their own stream. Every one of the 397
photos comes back byte for byte. Scan data 95.80 MB, one core:

| Version | Size | Decode | Notes |
|---|---:|---:|---|
| First build: magnitude class, top bit and sign as separate symbols; "nonzeros left" as context | **70.61 MB (−26.3%)** | 11 MB/s | 4.5 symbols per byte |
| One symbol per value (class + top bit), sign raw | 71.11 MB | 18 MB/s | 2.75 symbols per byte |
| Block's nonzero count instead of "left" as context | 71.47 MB | 18 MB/s | contexts known before the block |
| Branch-free value decoding, branchless raw-bit refill | 71.47 MB | 21 MB/s | |
| Block contexts with NEON; empty blocks skip them | 71.47 MB | **27 MB/s** | |

Extraction also re-writes JPEG's Huffman codes: 45 → **95 MB/s** after a
32-bit flush with a fast 0xFF check and a nonzero bitmask walk. End to end
that is about 21 MB/s per core, 2x Lepton's ~10 MB/s. Not yet the multiple.

Where the time goes (instruction counters, one decode of the set): 30.7 G
instructions in 11.1 G cycles, IPC 2.8, so **116 instructions per symbol**.
It is instruction-bound, not memory-bound: tiny tables decode at the same
speed. Next, in order:

1. A dedicated decode loop with the states in registers (the generic walk
   keeps them in memory).
2. A per-context bucket table (slot >> 7 → first symbol) instead of the NEON
   search.
3. Zero runs as part of the symbol, cutting the symbols per byte.
4. Tables for used contexts only (0.31 s of setup today).
5. A Huffman writer at 300+ MB/s.

Target: 60+ MB/s per core end to end, which puts the photo set at about
0.15 s on 10 cores against zstd -3's 0.27 s.

**Photos, round 4: measured against Lepton itself, and faster.** Earlier
"Lepton ≈ 70.2 MB" was inferred from the archive. Measured directly
(`--lepton`, lepton_jpeg 0.5.8, single thread, the engine's settings), the set
has 15 duplicate photos (3.32 MB) that the archive stores once. On the 382
unique baseline photos (94.74 MB):

| Coder | Size | Encode | Decode | Exact |
|---|---:|---:|---:|---|
| Lepton | 70.79 MB | 10 MB/s | 10 MB/s | yes |
| Ours (coefficients 68.83 MB + headers 0.19 MB solid) | **69.01 MB (−2.5%)** | 16 MB/s | **35 MB/s** + Huffman 95 MB/s | 382/382 |

Per file, headers would be 1.25 MB with zstd-19 (69.93 MB in all, still under
Lepton). This round's speed steps, size unchanged:

- The rANS states swap after each symbol and stay in registers.
- A 64-bucket table per context gives the first candidate symbol.
- Tables cover used contexts only, through a per-type map.
- Table levels are stored as bytes.

Decode went 27 → 35 MB/s. Decoding the 382 photos once: 28.3 G instructions
in 8.4 G cycles (IPC 3.4), 112 instructions per symbol. In the profile, the
symbol step is 38%, per-block bookkeeping 32%, value assembly 11%, table setup
10%, raw bits 5%.

End to end on one core: 2.63 s decoding + 0.97 s Huffman = about 26 MB/s,
2.6x Lepton. On 10 cores the photo set would extract in about 0.4 s, against
Lepton's 1.2 s today, zstd -3's 0.27 s and WinRAR's 0.37 s. Not enough yet:

1. Decode in MCU order straight into the Huffman writer, with row buffers
   instead of whole-image coefficient arrays.
2. Finer bucket tables, or 10-bit frequencies with direct lookup.
3. Fewer table decisions: send tables only for contexts with enough samples
   and fall back to a parent context.

Coding runs of zeros won't help: zeros are 47% of coefficient symbols, so a
run symbol plus a value symbol isn't fewer symbols.

**Photos, round 5: closer to the floor.** Size unchanged (69.01 MB with
headers against Lepton's 70.79 MB; 382/382 exact). One core:

| Step | Decode | Notes |
|---|---:|---|
| Round 4 | 35 MB/s | |
| Bucket entries packed (symbol, start, last slot in one u32) + 4 rANS states | **38 MB/s** | one dependent load in the common case |
| 256 buckets instead of 64 | 32 MB/s | table setup 0.27 → 0.47 s for no gain; reverted |
| Lazy context groups, direct writes into the image, NEON nonzero mask in the Huffman writer | no change | the chain, not the work, sets the pace |

A replay that decodes the symbols with their contexts given takes 1.07 s of
the 2.55 s decode (4.2 ns per symbol). The model's own work (contexts, raw bits,
value assembly, per-block bookkeeping) is about 1.2 s. Neither dominates.

Unpacking whole files, 382 photos, on a busy Mac (load ~4): one thread 3.40 s
(28 MB/s), 10 threads **0.57 s**, every file exact. Lepton's Default today:
1.2 s; zstd -3 0.27 s, WinRAR 0.30-0.38 s, zstd -19 0.28 s. The largest photo
is 4.74 MB (~0.17 s), so the scaling loss is the busy Mac, not one big file.

Still to get past the fastest rivals at this size:

1. Decode two block rows at once with their own streams. The contexts allow a
   staggered wavefront, and independent chains run side by side in one core.
2. A table-driven Huffman reader for packing (today's is a bit-at-a-time test
   reader).
3. A light JPEG mode for Fast, where the size bar is the fast rivals' 85.7-92.5 MB
   rather than Lepton: contexted static Huffman tables at Huffman speed.

**Photos: two tiers.** At Lepton's size the coder is close to its floor
(above). But Fast stores photos today, and the rivals it must beat there sit
at 85.3-92.5 MB, not at Lepton's size. Model H (`modelh.rs`, estimate) is a
light mode for Fast:

- the same joint symbols (count, value class with top bit), raw low bits and sign;
- contexts are only frequency band (8) × the block's count bucket (8), with
  the neighbours' counts for the count itself;
- static Huffman codes per image (table cost included), DC from the previous
  block as in JPEG.

On the 382 unique photos: **81.68 MB (−13.8%)**, about 82.3 MB on the full set
with duplicates stored once and the progressive files. That is still under
every rival (7-Zip Ultra 85.3 MB, zstd -19 85.7 MB). Huffman tables decode by
direct lookup, so it can run at several times the rANS coder's speed.

The plan:

- **Fast:** H, to beat zstd -3, WinRAR and Zipgeist's fast setting on both
  times.
- **Default and up:** the rANS coder, smaller than Lepton, 3.5x its decoding
  speed.
- **Max and Ultra:** Lepton, or mixing + block copy (about 69 MB).

Next: build H and measure it.

**Photos: the light mode, built** (`lite.rs`, `--lite`). Better than model H:
it re-codes JPEG's own symbols (DC size; AC run/size, EOB, ZRL) with static
Huffman tables per image and per context, keeping JPEG's raw bits. The
contexts are component class × band of the next position × neighbour nonzero
counts. Estimate (model R): 77.89 MB. No coefficients are rebuilt. Unpacking
reads a symbol with our table and writes the photo's own code for it, raw bits
copied across, in one pass. Packing parses the JPEG once, then writes.

382 unique photos (94.74 MB), every one exact; busy Mac (load ~3.4):

| | Size | Pack, 10 threads | Unpack, 10 threads | One core |
|---|---:|---:|---:|---:|
| Light mode | **78.25 MB** (−15.6% of scans, headers solid) | **0.21 s** | **0.20 s** | 60 / 61 MB/s |
| zstd -3 (full set, checkpoint) | 91.7 MB | 0.26 s | 0.27 s | |
| Zipgeist tar.zst Fast (†) | 92.1 MB | 0.21 s | 0.50 s | |
| WinRAR Fastest / Normal / Best | 92.5 / 90.9 / 86.2 MB | 1.09-2.98 s | 0.30-0.38 s | |
| zstd -19 --long | 85.7 MB | 15.6 s | 0.28 s | |
| 7-Zip Ultra | 85.3 MB | 6.5 s | 1.67 s | |

That is smaller than every rival and faster to unpack than every rival, tied
on packing with Zipgeist's fast setting. It is a harness figure: files are
already in memory and the archive's own work isn't counted.

What moved one core from 50 to 60 MB/s:

- iterating blocks without building a list;
- reading JPEG data 8 bytes at a time when no 0xFF is among them;
- a band table instead of a match;
- 12-bit lookahead in the JPEG reader;
- 9-bit primary tables per context;
- codes without decode tables when packing.

No effect: branch-free refills and flushes, 8-bit tables. What's left is the
chain from one symbol's bits to the next. Reading is 9.6 ns per symbol, near
libjpeg-turbo's 6-7 ns.

Next: build it into the engine as Fast's JPEG transform (headers to the binary
lane; verify by re-writing the parsed symbols, which is cheap) and the rANS
coder as Default's. Then measure the photo set in the real archive against
every rival on a quiet Mac.

**Photos: light mode, tuning round** (`lite.rs`: `pack_tight`, `unpack_tight`).
Same size (78.26 MB with headers on the 382 unique photos); every photo exact
after each step. One core and 10 threads, busy Mac (load ~3-4):

| Step | Pack 1 / 10 threads | Unpack 1 / 10 threads |
|---|---:|---:|
| Before | 60 MB/s / 0.208 s | 61 MB/s / 0.197 s |
| Symbol and raw bits in one step; one write per symbol | 60 / 0.207 | 61 / 0.196 |
| Branch-free light-stream writer (8-byte store, advance by whole bytes) | 63 / 0.197 | 61 / 0.196 |
| Raw bits in their own stream (events shrink to 2 bytes) | 68 / 0.177 | 60 / 0.198 |
| Bands from the position's top bit (0.6 MB bigger) | no gain, reverted | no gain |
| Closures replaced by a block iterator | 72 / 0.171 | 61 / 0.193 |
| **Unpack as one loop over locals, raw pointers, stuffing on a cold path** | 72 / 0.171 | **68 / 0.172** |
| **Pack likewise** (event buffer checked per block) | 71 / **0.175** | **70 / 0.166** |

The big lesson: the per-block closure captured the bit-reader state by
reference. LLVM kept it in memory, which meant 125 instructions per symbol and
loads and stores on every step. Instructions and cycles for unpacking were
19.7 G and 5.0 G per pass (IPC 3.9), which pointed straight at it.

Packing now is mostly the JPEG read itself: reading alone takes 0.86 s per
core (110 MB/s, ~5.4 ns per symbol). The rest is events, tables (0.16 s) and
the write (0.32 s).

Against the rivals on photos, this harness has the light mode ahead of all of
them on size (78.3 vs 85.3+ MB) and on unpacking (0.17 vs 0.27+ s). Packing
(0.175 s) is ahead of zstd -3 (0.26 s) and Zipgeist's tar.zst Fast (0.21 s),
and close to Zipgeist's ZIP (0.19 s, which Fast's store already beats). In the
archive, reading files and hashing add to packing. Ideas left for packing:
tables from a sample of the image so codes are written during the parse (no
event replay), a cheaper table build, and the faster duplicate hash from
Fast's profile.

**Photos: light mode, round 2.** Huffman tables built by sorting and a
two-queue merge with JPEG's Annex K.3 length cap, instead of a heap with
allocations: table time per run 0.16 → 0.085 s, size +0.02 MB. Pack 73 MB/s
on one core, **0.163 s** on 10 threads; unpack 70 MB/s, **0.167 s**. Every
photo exact. A refill on every symbol in the JPEG reader was slower (56 MB/s)
and was reverted.

The per-image tables are 0.69 MB raw and 0.22 MB compressed together across
files. In the archive they go with the headers to the compressed lane: about
77.8 MB for the 382 photos, 18% under the smallest rival.

**Photos: the light mode in the engine (Fast).** `transform/jpeglite.rs`, tag
5, `TransformOpts::jpeg_lite` (on at Fast; `ARCA_EXP_NO_JPEGLITE=1` turns it off,
and `--no-transforms` also covers it). Baseline JPEGs only; others stay as they
are. The stored form is a prefix (the file without its scan data, scan offsets
and tables), which compresses with the binary lane, and the code and raw
streams, which are stored. On the photo set, headers and tables are 2.73 MiB →
0.47 MiB in the binary lane.

Checking. A full decode after each encode cost as much as unpacking (1.76
CPU-s on the set). Instead, while parsing, every scan is re-written from the
parsed symbols and raw bits with the photo's own codes, restarts and padding,
and compared with the original. That covers the JPEG side and every raw bit;
our code stream is canonical Huffman over the same symbols, covered by the
tests. Cost: +0.3 s on one core (separately it was +0.6 s). Tests: round trips
of `sips`-made photos at four sizes and qualities, plus garbage rejection; the
whole engine suite passes.

The photo set, Fast, real archive (busy Mac, load 6-9; four runs):

| Fast | Size | Make | Extract | Round trip |
|---|---:|---:|---:|---|
| Before (photos stored) | 95.09 MB | 0.10 s | 0.05 s | identical |
| **Light mode** | **78.26 MB (−17.7%)** | **0.27 s** (2.16 CPU-s) | **0.23 s** (1.95 CPU-s) | identical |
| zstd -3 (checkpoint) | 91.7 MB | 0.26 s | 0.27 s | |
| Zipgeist tar.zst Fast (†) | 92.1 MB | 0.21 s | 0.50 s | |
| Zipgeist ZIP Fast / Normal (†) | 98.5 MB | 0.19-0.23 s | 0.53 s | |
| WinRAR Fastest | 92.5 MB | 1.09 s | 0.30 s | |
| Finder | 93.3 MB | 2.56 s | 0.51 s | |

One thread: encode (with the check) 1.62 s, decode 1.46 s. On size and
extraction the light mode beats every fast rival. On make it beats WinRAR and
Finder; zstd -3 is level, and Zipgeist's fast settings are ahead (0.19-0.21 s,
measured in Zipgeist's own session and scaled). The JPEG parse alone is 0.86 s
per core, so that gap needs a faster JPEG reader or a quiet-Mac measurement to
settle it.

Fast with the light mode on the folders that hold photos among other files
(busy Mac, load 5-6; two runs each):

| Fast | Saved folder | Website mixed |
|---|---|---|
| Photos stored (before) | 315.9 MB, 0.75 / 0.83 s | 288.3 MB, 0.74 / 0.67 s |
| Light mode | **298.8 MB**, 1.12 / 1.23 s | **271.1 MB**, 0.88-1.13 / 0.93 s |
| Zipgeist tar.zst Fast (fastest rival) | 341.0 MB, 1.28 / 1.54 s | 313.8 MB, 1.12 / 1.54 s |

Times are make / extract; every round trip is identical. Unpacking costs 1.6
CPU-s more for the photos, which is about +0.25 s on these folders. Putting
the prefix (headers, tables) in the store lane instead of the binary lane
changed nothing measurable and cost 2.5 MB, so it stays in the binary lane.

**Fast against the fast rivals, quieter Mac** (`bench/results-2026-10-29-fast-quiet.jsonl`,
`bench/research/quiet_summary.py`). `bench/checkpoint.py` with Arca Fast (light
JPEG), zstd -3, Finder, 7-Zip Normal, WinRAR Fastest and Normal, two rounds
alternating. Load at each set's start was 2.2-3.6. Every round trip is
identical. Zipgeist's figures are the checkpoint's, adjusted for its session
(†). Size MB, make / extract s:

| Set | Arca Fast | zstd -3 | WinRAR Fastest | Finder | Zipgeist tar.zst Fast † | Zipgeist ZIP Fast / Normal † |
|---|---|---|---|---|---|---|
| Photos | **78.3**, 0.27 / **0.22** | 91.8, **0.26** / 0.26 | 92.5, 1.02 / 0.30 | 93.3, 2.37 / 0.47 | 92.1, 0.19 / 0.40 | 98.5, 0.21 / 0.42 and 0.17 / 0.43 |
| Saved folder | **298.8**, **0.80** / **1.00** | 329.7, 6.42 / 10.45 | 339.5, 5.50 / 3.25 | 355.6, 18.53 / 11.24 | 341.0, 0.97 / 1.24 | 393.1, 1.53 / 1.44 |
| Website mixed | **271.1**, 0.87 / **0.93** | 296.1, 5.46 / 5.52 | 309.8, 5.68 / 3.14 | 324.2, 19.64 / 9.82 | 313.8, **0.84** / 1.27 | 379.9, 1.20 / 1.39 |

Fast is the smallest and the fastest to extract of the fast-class tools on all
three sets. It is fastest to make on the saved folder. On photos it is 0.01 s
behind zstd -3 (a tie) and 0.06-0.10 s behind Zipgeist's three fast settings.
On the website folder it is 0.03 s behind Zipgeist's tar.zst Fast. 7-Zip Normal
is smaller on the two folders, but it is a normal-class tool that Default
beats. What's left on photos is packing: the encoder costs ~1.6 CPU-s for the
set, and beating 0.17 s needs about 1.0 (the JPEG read alone is 0.86).

**Apple's apps: two copies of the same code.** Every large binary in the apps
set holds two arm64e slices (CPU subtypes 0x80000002 and 0x8000000c) built from
the same source, so `__text` is 15.46 vs 15.04 MB in Maps. With xz -9 (arm64
filter, 64 MiB dictionary):

| Binary | Slice 1 alone | Slice 2 alone | Both | Second slice's extra cost |
|---|---:|---:|---:|---:|
| Maps | 4.73 MB | 4.79 MB | 7.63 MB | 2.90 MB (61% of alone) |
| Mail | 1.41 MB | 1.42 MB | 2.12 MB | 0.71 MB (50%) |

LZMA recovers only 39-50% of the overlap: shifted code breaks its matches and
changes every PC-relative field. zstd `--patch-from` does worse (5.26 MB for
Maps' second slice). Idea: a slice-delta transform that matches functions
across slices (`LC_FUNCTION_STARTS`) and diffs instructions with PC-relative
fields mapped through the address shift, so slice 2 costs a fraction of slice
1. It would also make the slices independent blocks that can be packed in
parallel. If it reaches about 20% of a slice, Default on the apps set drops
below every rival's size (62.4 → about 58 MB; xz -6 59.0, WinRAR Best 58.7).

**DMGs: what each one costs** (Default; one DMG at a time; busy Mac, CPU
seconds are the reliable numbers):

| DMG | Chunks | Expanded saves | Unpack with expansion |
|---|---|---:|---:|
| CleanMyMac (210.5 MB) | 439 LZFSE | 45.7 MB (22%) | 0.80 s, 4.7 CPU-s |
| Discord (215.1 MB) | 485 zlib level 9 | 39.8 MB (18.5%) | 2.55 s, 19.4 CPU-s |
| Nuvio (186.3 MB) | 234 zlib level 1, 61 raw (64 MB) | 13.8 MB (7.4%) | 0.66 s, 3.6 CPU-s |
| Burnish (7.3 MB) | 21 zlib level 1, 2 raw | 1.1 MB | 0.03 s |

The rebuild is the match search: Discord's profile is 57% search (`walk_long`,
`longest_match`), 17% `deflate_slow`'s own loop and 7% Huffman. CleanMyMac's
is 58% LZFSE search and match coding. Hints that shorten the search can make
it at most ~2x faster. So the plan is cost-aware expansion: expand a chunk when
its saving is worth its rebuild time at that level, so Discord's level-9
chunks at Max/Ultra only. Without expansion the gap is small: raw chunks
compress 3.5% (zstd -3) to 7.5% (zstd -19); zlib and LZFSE chunks 0.3-1.1%.

**Fast's CPU** (third-party apps, busy samples): zstd 23%, BLAKE3 19%, zstd's
long-distance matching 9%, FastCDC 5%, Mach-O filters 3%. Threads waiting:
26%. Duplicate-finding is a third of Fast's work. A fast 128-bit hash with
byte comparison before a duplicate is accepted would be as safe for
deduplication at a fraction of BLAKE3's cost.

**Build order** (the gain each removes from the 48 misses):

1. The JPEG coder (photos, 9).
2. Fast without the extra cost, and big files split across cores (Silesia,
   third-party apps and DMGs against the fast rivals, about 10).
3. The slice-delta transform for Apple binaries (apps, 5).
4. Cost-aware DMG expansion and compressed raw chunks (DMGs against the strong
   rivals, about 11).
5. Faster text decoding at Max (Silesia and source code, about 8).

**Hashing: POLYVAL in place of BLAKE3 (built, format version 3).** Every byte went
through BLAKE3 about four times: file, chunks, block, compressed block (twice the
same bytes for a stored block), and twice more on unpack. Measured on one core,
256 MiB warm, Mac at load 7:

| Hash | MB/s | Notes |
|---|---:|---|
| BLAKE3 (NEON) | 1,800 | |
| POLYVAL, 8 blocks at a time, through general registers | 8,900 | moving 64-bit halves out of vector registers cost a third |
| POLYVAL, 8 blocks, vector registers only | 11,300 | |
| POLYVAL, 16 blocks | 14,600 | 8.1x BLAKE3; shipped |

The proposal above was a fast hash plus a byte comparison. The comparison needs the
first copy's bytes, which may be compressed and gone by then, so this does better:
POLYVAL is a universal hash, so with a key drawn after the files exist, two
different inputs collide with probability at most (blocks + 2) / 2^128, by proof
and for any input, crafted or not. 2^32 chunks of 2 MiB (8 PB) leave 2^-48. The
key sits in the index, which stays under BLAKE3. What is given up: with the key, a
tamperer can make different bytes hash alike, so the index hash no longer commits
to the block contents. Archives aren't signed, so an editor could always rewrite
the hashes beside the bytes; it would matter only to someone who recorded the index
hash elsewhere and trusted it, rather than hashing the file.

Also: stored blocks are hashed once and no longer copied on either side, and the
compressed-block hash moved off the single writer thread. Multi-chunk file
identities stay BLAKE3 over the hash list (short input, keeps the domains apart).

Same files, old and new binaries taking turns, seven rounds, best wall time, Mac at
load 7:

| Set | Size | Pack s | Pack CPU-s | Unpack s | Unpack CPU-s |
|---|---:|---:|---:|---:|---:|
| Photos | 78.26 MB (same) | 0.274 → 0.244 (−11%) | 2.17 → 1.96 | 0.236 → 0.221 (−6%) | 1.95 → 1.80 |
| Website mixed | 271.11 MB (same) | 0.934 → 0.797 (−15%) | 7.57 → 6.57 | 0.955 → 0.850 (−11%) | 5.63 → 5.09 |
| Saved folder | 298.83 MB (same) | 0.861 → 0.739 (−14%) | 7.40 → 6.12 | 0.981 → 0.913 (−7%) | 6.09 → 5.38 |

Round trips identical (Fast on photos and website mixed, Default on photos); version
2 archives still open and extract; bytes flipped at a quarter, half and three
quarters of two archives were all caught. Busy-Mac numbers: to be confirmed quietly.

**Fast's other overhead (built).** After the hash, a two-thread sample of the
website set showed the light coder, zstd's matchers, long-distance matching (16%
of active time) and `open` (7%).

*`open` doesn't scale.* Timing it on the 10,279 files of the website set, CPU per
call / wall time for all of them:

| Threads | 1 | 2 | 3 | 4 | 6 | 8 | 10 |
|---|---:|---:|---:|---:|---:|---:|---:|
| `open` | 18 µs / 188 ms | 24 / 122 | 25 / 87 | 34 / 88 | 80 / 137 | 140 / 184 | 120 / 168 |
| `close` | 0.6 µs / 6 ms | 0.8 / 4 | 0.9 / 3 | 1.4 / 4 | 3.4 / 6 | 5.5 / 7 | 4.0 / 7 |
| `lstat` | 2.5 µs / 26 ms | 2.8 / 14 | 2.9 / 10 | 3.3 / 9 | 5.6 / 10 | 6.0 / 8 | 7.6 / 9 |
| `listxattr` | 3.1 µs / 32 ms | 3.8 / 21 | 4.4 / 16 | 5.0 / 13 | 9.9 / 18 | 11.9 / 17 | 13.1 / 17 |

Ten threads opening at once spin: 7x the CPU and twice the wall time of three.
Opening relative to the parent folder doesn't help (20 µs against 18 µs), and
`lstat` shows the path walk is cheap. So `open_file` lets three opens run at a
time. On the website and saved folders that alone is about −1 CPU-s of 6.5.

*Long-distance matching.* Off, it costs 0.5-0.8% in size and saves 11-24% of the
CPU. zstd's hash and chain tables at twice their size win the size back for less:

| Fast binary lanes (three rounds, CPU vs LDM on) | Website | Saved | Silesia | Apps | Docs | Third-party | Source |
|---|---:|---:|---:|---:|---:|---:|---:|
| Tables 18/17, LDM off: size | +0.02% | −0.02% | −0.49% | −0.49% | +0.13% | −0.75% | +4.95% |
| CPU | −10% | −8% | −9% | −3% | −23% | −11% | −8% |

Source is the exception, in two parts: its text needs the far repeats (5% of it),
so the text lane keeps the search; and its 99 MB of `.a` static libraries compress
22:1 with object files repeated across libraries. A retry handles those without a
per-format rule: a block that shrinks more than 8:1 is compressed again with the
search on. Source is then byte-for-byte the old size. Also dropped: zstd's own
XXH64 per frame (the block hash covers it) and FastCDC on a photo's coded tail
(nothing to find short of the same file, which file dedup catches).

*The light coder's check.* Re-writing every scan to compare cost 0.3 CPU-s of 1.6.
It can only differ from the original if one of a few things went wrong, and those
can be checked directly: each table is a prefix code with one code per symbol;
stuffing was FF 00; and each stretch ends with nothing but padding of ones, right
before the next expected marker, with no made-up bits consumed. Every other bit was
a symbol's code or its raw bits. This is stricter on one point, too: nothing was
checking the old code's restart search for skipped bytes until the comparison. The
fuzz test covers what neither check does, the decoder: 120,000 mutants of the 400
real photos (244 with restart markers), one to three random changes each, 36,593
accepted, every one back byte for byte, no crash.

Old binary against new, taking turns, five rounds, best wall time, Mac at load 13-20
(not quiet, so ratios matter more than seconds):

| Set | Size MB | Pack s | Pack CPU-s | Unpack s | Unpack CPU-s |
|---|---:|---:|---:|---:|---:|
| Photos | 78.26 → 78.26 | 0.270 → 0.207 (−23%) | 2.11 → 1.56 | 0.232 → 0.217 (−7%) | 1.93 → 1.76 |
| Website mixed | 271.11 → 270.40 | 0.943 → 0.787 (−17%) | 7.37 → 5.77 | 0.969 → 0.912 (−6%) | 5.49 → 4.87 |
| Saved folder | 298.83 → 298.11 | 0.873 → 0.706 (−19%) | 6.83 → 5.23 | 1.010 → 0.905 (−10%) | 5.80 → 5.36 |
| Silesia | 65.67 → 65.24 | 0.234 → 0.181 (−23%) | 1.47 → 1.16 | 0.073 → 0.061 (−16%) | 0.43 → 0.30 |
| Source | 13.55 → 13.55 | 0.263 → 0.272 (+3%) | 1.90 → 1.58 | 0.374 → 0.390 (+4%) | 2.07 → 2.01 |
| Docs | 41.52 → 41.56 | 0.089 → 0.064 (−28%) | 0.43 → 0.28 | 0.084 → 0.072 (−14%) | 0.30 → 0.24 |
| Apps | 72.05 → 71.70 | 0.284 → 0.205 (−28%) | 1.49 → 1.12 | 0.158 → 0.145 (−8%) | 0.99 → 0.84 |
| Third-party apps | 465.87 → 462.37 | 2.256 → 1.493 (−34%) | 9.52 → 6.93 | 1.502 → 0.891 (−41%) | 3.59 → 2.16 |

Docs is 0.04 MB larger (0.1%), the one set that isn't at least as small; tables
18/18 would fix it at +5-10% CPU on Silesia and third-party apps, so it stays.
Source's wall time rose 3-4% with its CPU down 17%: noise from the load or the
retry; to be read on a quiet Mac.

## Deep research agenda (from the user, 2026-10-28 evening)

The goal is a breakthrough, not tuning: beat every archiver (7-Zip, Zipgeist,
Keka, BetterZip, WinRAR, Finder, zstd, xz) at every level on speed and size,
aiming for multiples. Keep what works (pixel codec, ZIP reader and writer, LZMA
SDK, memory fix); replace a part only if a test shows the new one is faster and
at least as small. Steps before the final benchmarks:

1. Default several times faster: Lepton, preflate and zstd are its time; e.g.
   the pixel codec with fast decoding.
2. Max under 20 s; Ultra extracting on all cores.
3. PNG token coding; re-creating one Deflate stream on several cores.
4. Apple Silicon GPU: can it take any stage? It must help ordinary Macs, not
   only big-GPU ones.
5. Duplicate detection, scheduling, writing files, and the app's extraction
   overhead (2.2 s in the app vs 0.9 s in the engine).
6. The newest papers and techniques: codecs and transforms nobody else ships.

Status of each is kept under its own heading below as it progresses.

## The task, in short

Make Arca much faster at every level (aim: ~50%+, "unimaginably fast"), while at
least as small, so each level beats every rival's comparable setting on speed and
size. Format may change freely until 1.0. Full brief:
`research_notes/arca-research-brief.md` (main checkout, git-ignored).

Rules that shape the work:
- Ask the user before any benchmark. Profiling runs to see where time goes are fine.
  Final numbers: quiet Mac, user away, alternating A/B.
- Keep the 9 `arca-ffi` functions and the Swift wrapper working; 58 app tests pass.
  No UI changes.
- Commits as Observant Systems, no AI lines, never push or merge without the OK.

## Machine

Apple M1 Pro, 8 performance + 2 efficiency cores, 32 GB, macOS 27.0.1.
Rust 1.99.1, 7-Zip 26.05 (`7zz`), zstd 1.5.7, xz 5.8.4.

## Targets per level

Saved folder: `research_notes/engine-bench.noindex/Mixed` (660,849,010 bytes,
11,050 files). Rival numbers from the 2026-10-28 head-to-head (busy Mac, so treat
as approximate until re-measured quietly):

| Level | Comparable rival | Rival make | Rival size | Rival extract | Arca today (make / size / extract) |
|---|---|---:|---:|---:|---|
| Fast | Zipgeist fastest (tar.zst) | 1.24 s | 341.0 MB | 1.6–1.9 s | 1.8–2.2 s / 315.9 MB / 2.2 s (app) |
| Default | Zipgeist standard 7z | 17.2 s | 285.2 MB | 3.0 s | ~11 s / 257 MB / ? |
| Max | Zipgeist best 7z | 20.0 s | 284.2 MB | 3.7 s | 117 s / 221.7 MB / 12.7 s |
| Ultra | Zipgeist best, 7-Zip Ultra | 20.0 s | 284.2 MB | 3.7 s | not measured |

Also in scope: ZIP. Mac-made ZIP (`ditto-mixed.zip`) extracts in 4.7 s via 7-Zip
on one core in the app; Zipgeist 2.1 s.

So the gaps, before any work: Fast make ~1.5x too slow; Max make ~6x too slow and
Max extract ~3.4x too slow; ZIP extract ~2.2x too slow. Default already wins both
ways but should get faster too.

## Test sets

| Set | Where | Status |
|---|---|---|
| Saved folder (Mixed, 660 MB) | `research_notes/engine-bench.noindex/Mixed` | on disk |
| Rivals' archives | `research_notes/engine-bench.noindex/archives` | on disk |
| website sets | `research_notes/engine-bench.noindex/sets/` | 4 of 5 rebuilt; silesia needs a download (ask) |

Website sets were rebuilt with the exact recipe of the 2026-10-25 session (found in its
transcript):

| Set | Recipe | Bytes now | Bytes on 2026-10-25 |
|---|---|---:|---:|
| apps | `cp -cR` of Calculator, Chess, TextEdit, Preview, Stickies, Notes, Maps, Mail from `/System/Applications` | 173,844,185 | 173,844,185 (identical) |
| source | the 89 crates in `~/.cargo/registry/src/index.crates.io-*` unpacked before 2026-10-25 20:04 | 161,981,441 | 161,981,441 (identical) |
| jpeg | first 400 `*.jpg`/`*.jpeg` over 20 KB under `/System/Library /Library /Applications/Xcode.app` (`find` order) | 98,403,602 | 98,428,070 (−0.02%) |
| docs | first 250 PDFs over 20 KB (same dirs) + first 600 PNGs over 10 KB under `/System/Library /Library` | 90,904,052 | 89,428,138 (+1.7%) |
| silesia | `silesia.zip` from sun.aei.polsl.pl (public corpus) | — | 211,938,580 |
| mixed | all five in one folder (`cp -cR`) | — | 735,620,414 |

jpeg and docs drifted slightly because system files changed since 10-25; old website
numbers for those two sets are not directly comparable to new ones.

**Real-world sets (local only, the user's own files, 2026-10-29).** The website
sets hold no third-party app and no disk image, so they can't show the x86
filter or the DMG transforms. With the user's OK, two sets of their files,
`cp -c` clones in `sets/` (never committed or published):

| Set | Contents | Bytes | Files |
|---|---|---:|---:|
| dmgs | 5 installers from `~/Downloads`: Burnish (UDZO, zlib level 1), Discord (zlib level 9), Figma (zlib levels 7-9), Nuvio (zlib level 1 + raw chunks), CleanMyMac 5.7.0 (ULFO, LZFSE) | 620,144,862 | 5 |
| thirdparty | 6 apps from `/Applications`: Firefox (universal, 98 MB of `omni.ja` ZIPs), Telegram, NordVPN (73 MB of `Assets.car`), Encrypto, Paws Off, PasteNow | 1,249,298,357 | 1,400 |

Left out: a 387 MB bzip2 (UDBZ) installer in `~/Downloads` (no bzip2 re-creation
yet; a lead), and Electron apps other than Discord's (its DMG holds one).
Round trips at every level identical (busy Mac, CLI):

| Set | Fast | Default | Max | Ultra |
|---|---:|---:|---:|---:|
| dmgs | 619.7 MB, 1.4 s | 557.0 MB, 7.1 s | 457.9 MB, 54.9 s | 455.4 MB, 65.4 s |
| thirdparty | 465.9 MB, 2.9 s | 411.7 MB, 7.9 s | 339.5 MB, 55.0 s | 336.2 MB, 67.7 s |

## Baseline profiles

Saved folder, CLI (`engine/target/release/arca`), busy Mac (load ~5), 2026-10-28.
Profiles use `ARCA_DEBUG_PROFILE=1` (added in `arca-core/src/prof.rs`: exclusive
thread seconds per stage) and `ARCA_DEBUG_TIMING=1`. Not final numbers.

| Level | Make | Size | Peak RSS | CPU (user) | Extract | Extract CPU |
|---|---:|---:|---:|---:|---:|---:|
| Fast | 1.5–1.9 s | 315.9 MB (301.31 MiB) | 0.96 GB | 4.1 s | 0.93 s | 1.1 + 4.7 sys |
| Default | 10.1–10.6 s | 257.5 MB (245.55 MiB) | 3.6 GB | 64–65 s | 2.6–2.7 s | 15.9 + 5.3 sys |
| Max | 105.4 s | 221.7 MB (211.47 MiB) | **10.3 GB** | 722 s | 9.6 s | 74 + 5.6 sys |

**Fast make** (1.46 s run): scan 0.19 s (one thread), sniff 0.25 s, files+blocks
0.68 s, index 0.33 s. Thread seconds: sniff 2.38 (opens every file; 177 µs each),
zstd-3 blocks 2.6, BLAKE3 1.2 (every small file is hashed twice: whole file, then
as its single chunk), CDC + chunk hashes 0.63, **index zstd-19 0.33 s on one
thread** (1.3 MB raw index → 233 KB). Only ~4 s CPU in 1.5 s: Fast is bound by
serial phases, not by work.

**Default make** (10.6 s): thread seconds: Lepton encode+verify 38.6, zstd-12 binary
blocks 27.9, preflate analysis 21.5, zstd-15 text blocks 11.3 (60 MB at 5 MB/s),
preflate verify 6.8, hashing 4.5. 64 s CPU in 10.6 s: cores ~60% busy.

**Max make** (105 s): files done at 39 s, blocks at 105 s. LZMA-9e over 1.30 GiB of
binary lane (1.0 GiB of it is expanded Deflate, mostly PNG pixels). Max expands 793
Deflate files (132 MB → 1.0 GiB) where Default expands 594 (101 MB → 588 MB): the
extra 199 files are the level-9 PNGs that Default's chain limit (256) rejects.

**Max extract** (9.6 s, 74 s CPU): rebuilding Deflate 64.9 s CPU (Default: 2.9 s),
Lepton 14.6 s, xz decode 10.0 s, writing 4.8 s. Single PNGs take up to 3.0 s each
(`png-417-photoPaperHiDPI-DogEarLeft.png`: 1.16 MB → 16.7 MB of pixels). System
zlib -9 needs 4.4 s to compress that same image, so preflate is already faster than
zlib: the cost is inherent to replaying a level-9 Deflate encoder one byte at a time.
Tool: `cargo run --release --example objtime ARCHIVE` lists the costliest rebuilds.

**Default extract** (2.6 s): Lepton 12.6 s CPU, writing files 4.8 s, Deflate 2.9 s.
**Fast extract** (0.93 s): writing files 5.3 s of thread time (kernel), zstd 0.3 s.

Profiler caveat found on the way: hashing files over 1 MiB uses `update_rayon` inside
the busy pack pool. While it waits on the join, the thread steals unrelated tasks
(e.g. a 3 s PNG), so one file's hash can finish seconds late and stall the in-order
packer. To check as part of the scheduling work.

## All website sets, before and after (2026-10-28)

Every rebuilt set at every level, this morning's engine (tag
`before-engine-research`, built in a separate worktree) against commit e891199.
Sizes exact; times rough (the baseline ran while other tests ran). All 48 round
trips byte-identical. Raw: `bench/research/2026-10-28-sets-*.tsv`, script
`bench/research/sets.sh`.

| Set | Level | Size before | Size now | Δ | Make before → now |
|---|---|---:|---:|---:|---|
| docs | fast | 41.50 MB | 41.52 MB | +0.06% | 0.29 → 0.24 s |
| docs | default | 22.21 | 22.17 | −0.17% | 2.28 → 2.06 |
| docs | max | 18.00 | 18.20 | +1.12% | 21.4 → 11.4 |
| docs | ultra | 18.00 | 17.82 | −1.03% | 22.1 → 16.2 |
| jpeg | all | 71.09–95.10 | +0.00–0.01% | | 1.2–1.7x faster |
| apps | fast | 72.40 | 72.46 | +0.09% | 0.51 → 0.38 |
| apps | default | 62.43 | 62.39 | −0.08% | 2.74 → 2.25 |
| apps | max | 49.84 | 49.17 | −1.34% | 26.5 → 18.0 |
| apps | ultra | 49.81 | 48.53 | −2.56% | 26.9 → 17.3 |
| source | fast | 13.55 | 13.57 | +0.18% | 0.57 → 0.42 |
| source | default | 11.22 | 11.23 | +0.11% | 2.31 → 2.15 |
| source | max | 8.29 | 9.28 | **+11.94%** | 27.6 → 6.5 |
| source | ultra | 8.29 | 8.26 | −0.36% | 27.5 → 23.2 |
| silesia | fast | 65.74 | 65.67 | −0.10% | 0.31 → 0.25 |
| silesia | default | 57.27 | 57.27 | 0.00% | 5.89 → 5.54 |
| silesia | max | 48.65 | 49.31 | +1.37% | 36.0 → 14.8 |
| silesia | ultra | 47.00 | 46.90 | −0.21% | 27.4 → 17.9 |
| mixed | fast | 288.27 | 288.37 | +0.04% | 1.45 → 1.04 |
| mixed | default | 223.39 | 223.25 | −0.06% | 10.9 → 10.4 |
| mixed | max | 195.48 | 196.76 | +0.66% | 47.2 → 31.9 |
| mixed | ultra | 193.63 | 192.26 | −0.71% | 37.9 → 37.1 |

Findings: the fast Max (the user's choice, made on the saved folder's +0.65%)
costs 12% on source code; Fast and Default have small regressions (≤0.18%) from
the new file order; Default is barely faster (its CPU is Lepton, preflate and
zstd, untouched so far); Ultra is smaller everywhere but still extracts slowly
(PPMd: 4.2 s source, 7.2 s Silesia, 8.0 s mixed).

### Max settings across every set

Three Max settings (A = the fast preset the user picked: text fb 64 in 32 MiB
blocks, binaries fb 32; B = text fb 273 in 64 MiB blocks, binaries and pixels
fb 64; C = fb 273 everywhere), size against this morning's Max:

| Set | A | B | C |
|---|---|---|---|
| docs | +1.1%, 1.9x faster | −0.4%, 1.6x | −1.0%, 1.3x |
| apps | −1.3%, 1.5x | −2.3%, 1.7x | −2.6%, 1.5x |
| source | **+11.9%**, 4.2x | +6.1%, 1.9x | −0.4%, 1.2x |
| silesia | +1.4%, 2.4x | −0.04%, 1.3x | −0.2%, 1.1x |
| mixed (website) | +0.7%, 1.5x | −0.3%, 1.3x | −0.7%, 1.0x |
| saved folder | 212.61 MiB, 21.8 s | 211.17 MiB, 25.5 s | 210.32 MiB, 34.4 s |

(This morning: saved folder 211.47 MiB in 105 s.) Source code is where long
matches matter most: its ~2,800 static libraries repeat whole object files.
Raw: `bench/research/2026-10-28-max-candidates.tsv`.

**Index format v2**: entries column by column, paths front-coded against the
previous path, times and object numbers delta-coded (version 1 archives still
open). Saved folder at Fast: index 251.37 KiB (v1, zstd-9) / 232.61 KiB (v1,
zstd-19) → **206.75 KiB** (v2, zstd-9, 5 ms); raw index 1.3 MB → 0.5 MB. Fast on
the source set is now within 858 bytes of this morning (was +24.8 KB).

### Max: a code lane (the user's direction, 2026-10-28)

User: keep the fastest Max, but give code and text long matches so Max beats 7-Zip
Ultra on source (under 8.5 MB) and Silesia (under 48.7 MB), with a small time cost.
New `Class::Code` (by content: ELF, PE, static libraries `!<arch>`, Java class,
WebAssembly, LLVM bitcode, tar) and a code lane with its own codec and block size
(Max and Ultra only; Fast and Default keep it in the binary lane, unchanged).

| Max variant | source | Silesia | saved folder |
|---|---|---|---|
| fast preset (text fb 64 / 32 MiB, binary fb 32) | 9.28 MB | 49.31 MB | 212.61 MiB, 21.8 s |
| + code lane fb 273 incl. Mach-O | 8.53 MB | 49.02 MB | 212.55 MiB, 37.4 s |
| + code and text fb 273, 64 MiB, Mach-O in code | 8.23 MB | 48.63 MB | 212.26 MiB, 41.9 s |
| **code and text fb 273, 64 MiB, Mach-O stays binary** | **8.23 MB** | **48.63 MB** | **211.69 MiB, 33.4 s** |
| same, text in 32 MiB blocks | 8.42 MB | 48.74 MB | 211.88 MiB, 33.3 s |
| code fb 64 | 8.77 MB | 48.71 MB | 212.20 MiB, 24.1 s |
| code fb 273 in 32 MiB blocks | 8.46 MB | 48.76 MB | 211.91 MiB, 32.4 s |

Mach-O gained almost nothing from long matches on real apps (−0.06 MiB for +15 s),
so it stays in the binary lane. The code lane on the saved folder is ~99 MiB of
static libraries that compress 46:1; at fb 273 one 64 MiB block of it takes ~31 s,
which is the +11 s. **Chosen:** text and code fb 273 in 64 MiB blocks (both targets
met; saved folder 211.69 MiB, smaller than this morning's 211.47 MiB-in-105 s Max
0.1%, in ~33 s instead of 105 s). Next: a long-range pre-pass to take that repetition
off LZMA (see deep research).

## Known fixes

1. **Max PNG rebuild cost** (in progress).

   What the data is: at Max, 554 docs PNGs expand from 115 MB to **1,008 MB** of
   filtered pixel rows, two-thirds of all lane data. Other lane data: Mach-O 159 MB,
   source "binary" 104 MB, app binaries 81 MB, source text 58 MB, PDF streams 43 MB.

   Max on the docs part alone (850 files, 138 MiB), varying preflate:

   | Setting | Size | Make | Extract | Rebuild CPU |
   |---|---:|---:|---:|---:|
   | chain 256 (Default's) | 77.22 MiB | 51.6 s | 1.02 s | 2.8 s |
   | chain 4096 (Max today) | 71.37 MiB | 90.2 s | 6.62 s | 59.0 s |
   | chain 4096 capped to 256 (new patch) | 73.65 MiB | 86.1 s | 4.81 s | 40.9 s |
   | chain 4096 capped to 1024 | 72.48 MiB | 86.1 s | 5.70 s | 49.8 s |
   | chain 4096, budget 64 steps/byte | 77.00 MiB | 55.8 s | 1.05 s | 3.5 s |
   | chain 4096, budget 160 steps/byte | 76.44 MiB | 62.4 s | 1.10 s | 4.7 s |

   Rebuild cost is very lopsided: 72 of 793 Deflate files (13 MB) cause 42.7 s of the
   60 s rebuild CPU; 140 files (23 MB) cause 55 s. They are photo-like images saved
   by level-9 encoders: nearly every position starts a short match, so replaying the
   encoder means millions of searches (sampling: all time in preflate's
   `match_token_0/1`). Capping the search depth does not help much (40.9 s), so the
   cost is the number of searches, not their depth.

   A search step costs a steady ~3.1 ns (414 M steps = 1.28 s; 1,066 M = 3.28 s;
   738 M = 2.36 s), and preflate's analysis at pack time performs exactly the same
   searches as the rebuild. So **a step count measured while packing predicts the
   rebuild cost exactly and deterministically**. Patches in the vendored
   `engine/vendor/preflate-rs`: a step counter, a per-byte step budget checked every
   1,024 tokens (a stream over budget is abandoned and stored as is; a rejected PNG
   now costs 0.07 s at pack time instead of 0.43 s), and an optional chain cap.

   But the size gain lives in exactly those costly files: a budget keeps almost none
   of the 5.85 MiB (77.0 vs 77.2 MiB). So the honest trade at Max today is
   about 2.7% of size against ~6x faster docs extraction and ~1.6x faster docs
   packing. To win back the size cheaply, see "PNG token coding" under deep research.

   Whole saved folder at Max (busy Mac; times rough), with a separate image lane
   for PNG pixels (`--lane`):

   | Variant | Size | Make | Extract | Pack CPU |
   |---|---:|---:|---:|---:|
   | Max today | 211.47 MiB | 105 s | 9.6 s | 722 s |
   | A: budget 64 steps/byte | 217.71 MiB | 80 s | 3.6 s | 403 s |
   | B: A + xz-9 for binary and image lanes | 218.67 MiB | 56 s | 3.7 s | 286 s |
   | C: B + xz-9 text, zstd-19 image | 221.17 MiB | 51 s | 3.9 s | 280 s |
   | D: A + zstd-19 image | 220.45 MiB | 51 s | 5.7 s* | 328 s |

   (*D's extraction ran under heavy load.) In A, the image lane (546 MiB raw) took
   275 s of CPU with xz-9e, 167 s with xz-9 and 156–170 s with zstd-19; the binary
   lane (400 MiB) 224 s with xz-9e and 138 s with xz-9.

   Codec comparison on one 64 MiB block per data type (single thread, 4 jobs at
   once), compressed MB / seconds:

   | Block | xz-9e | xz-9 | xz-6 | zstd-19 | zstd-15 | zstd-12 |
   |---|---|---|---|---|---|---|
   | 000 source text | 6.26 / 20.3 | 6.37 / 15.3 | 6.65 / 12.8 | 6.61 / 17.3 | 7.41 / 4.8 | 7.60 / 1.2 |
   | 001 Mach-O | 10.20 / 25.8 | 10.25 / 22.4 | 12.09 / 20.3 | 12.10 / 23.2 | 14.47 / 7.8 | 14.70 / 1.8 |
   | 008 synthetic PNG pixels | 0.68 / 17.4 | 0.77 / 2.6 | 0.80 / 2.5 | 0.77 / 4.1 | 0.84 / 0.7 | 0.86 / 0.4 |
   | 011 photo-like PNG pixels | 14.47 / 45.4 | 14.53 / 39.7 | 14.59 / 25.7 | 15.47 / 27.6 | 16.37 / 10.7 | 16.65 / 2.4 |
   | 013 PNG tail + PDF | 6.48 / 24.0 | 6.67 / 14.1 | 6.78 / 12.6 | 6.98 / 23.8 | 7.94 / 3.8 | 8.14 / 1.1 |
   | 015 other binaries | 20.60 / 20.2 | 20.66 / 15.9 | 20.80 / 12.8 | 21.52 / 16.0 | 22.93 / 4.9 | 23.06 / 1.1 |

   Over these six blocks -9e is 0.95% smaller than -9 for 1.39x the time; zstd-19
   is never worth it; LZMA pays most on Mach-O (30% under zstd-12) and least on
   photo-like pixels (13%), where it is also slowest. Cheaper LZMA match finding
   (`mf=hc4`, `mode=fast`, `nice=32`, `depth=12`) loses 2–18% and is still slow on
   pixels (6–22 s per block). **Conclusion: no xz setting reaches Max < 20 s on
   this folder; that needs a pixel codec and a faster LZMA match finder.**
2. **Max memory on 8 GB Macs** (done). Peak was 8.3–10.3 GB: ten xz-9e encoders
   (~674 MiB each, mostly BT4's tree for a 64 MiB dictionary), 20 queued 64 MiB
   blocks, and a read-ahead window that counted files at their input size while an
   expanded PNG waiting its turn holds ~15x that. Now:
   - the number of block encoders comes from a memory budget (half of
     `hw.memsize`; `ARCA_MEM_BYTES` simulates other Macs), using liblzma's own
     `lzma_easy_encoder_memusage` plus the block and its output per encoder;
   - queued blocks: encoders + 2 (was 2 x threads);
   - read-ahead: min(512 MiB, budget/16), counting expanded bytes too.

   | Simulated Mac | Encoders | Peak RSS | Max make (busy Mac) |
   |---|---:|---:|---:|
   | 8 GB | 3 | 4.8 GB | 124 s |
   | 16 GB | 8 | 8.5 GB | 72 s |

   Archives are byte-identical whatever the memory. The ~0.7 GB above budget is
   page cache of mapped input files (reclaimable) and allocator slack.
3. Max blocks vs cores: (to do)
4. **Fast's duplicate and fingerprint cost** (mostly done). Fast make on the saved
   folder went from 1.46–1.9 s to 0.80–1.0 s (busy Mac, even at load 35), same
   size (301.33 MiB; +0.02 from the index level), round trip byte-identical:
   - Each file is hashed once. Untransformed files are hashed only chunk by chunk;
     the file's identity is the single chunk's BLAKE3, or for several chunks a
     hash of the (length, hash) list under its own derive-key domain, so no file's
     bytes can pose as another file's chunk list. Large files no longer hash with
     `update_rayon` inside the busy pool (it stole unrelated tasks).
   - The index uses zstd-9 at Fast and Default: 0.01 s instead of 0.31 s for
     257.5 KB instead of 238.5 KB.
   - The folder walk runs on the pack pool (0.20 s → 0.07–0.09 s).
   - No separate sniffing pass: files are ordered by extension before anything is
     read, and each task reads, classifies and packs its file. Opening files does
     not scale past ~2 threads on APFS (11,050 open+read+close: 0.26 s on 1
     thread, ~0.12 s on 2–10), so it is better overlapped with compression than
     done as a phase.
   - Ordering by extension costs size at the bigger levels, so Default, Max and
     Ultra still sniff (a 4 KiB head read per file, ~0.1 s) and Fast doesn't:

     | Level | Extension order | Sniffed order | Before this work |
     |---|---:|---:|---:|
     | Fast | 301.33 MiB, 0.67 s | 301.33 MiB, 0.78 s | 301.31 MiB, 1.5–1.9 s |
     | Default | 245.72 MiB | **245.34 MiB** | 245.55 MiB |
     | Max (budget 64) | 219.05 MiB | **217.71 MiB** | — |

     The separate image lane makes Default 0.21 MiB smaller than before.
   - Dead end: `-C target-cpu=apple-m1` (Lepton and preflate within noise).

## Deep research

(to fill: codecs, transforms, dedup, scheduling, writing files, GPU, lzraven)

### Codecs

**Apple's LZRAVEN and LZMESH** (macOS 27 SDK, `compression.h`: `COMPRESSION_LZRAVEN
= 0xD05`, `COMPRESSION_LZMESH = 0xE05`, buffer API only). Apple: LZRAVEN is
"faster than, and generally compresses better than LZMA"; LZMESH replaces zlib and
LZFSE. Per the Apple Wiki it appeared in 26.4b1 as "Mammoth". Tested on the
dumped 64 MiB blocks with `compression_encode_buffer` (single thread; tool in the
scratchpad, `rv.c`), compressed MB / encode s / decode s:

| Block | LZRAVEN | LZMESH | Apple LZMA (level 6) | xz-9e | xz-9 |
|---|---|---|---|---|---|
| 008 synthetic pixels | 0.789 / 2.39 / 0.018 | 1.070 / 0.06 / 0.007 | 0.800 / 2.65 / 0.034 | 0.678 / 17.4 | 0.768 / 2.6 |
| 000 source text | 6.701 / 11.45 / 0.101 | 9.081 / 0.18 / 0.028 | 6.654 / 13.90 / 0.240 | 6.258 / 20.3 | 6.371 / 15.3 |
| 001 Mach-O | 11.655 / 18.13 / 0.211 | — | — | 10.201 / 25.8 | 10.251 / 22.4 |

LZRAVEN sits between xz-6 and xz-9 on ratio (7% over xz-9e on text, 14% on
Mach-O), compresses no faster, and decodes ~2.4x faster than LZMA. It needs macOS
27 (Zipling supports 15+), so an archive using it would not open on older Macs.
**Dead end for Max**; its fast decoding is noted in case Zipling ever requires 27.

**7-Zip's LZMA encoder** (LZMA SDK, public domain), `7zz a -txz -mx9 -mfb=273`
against `xz -9e`, alternating, same blocks: Mach-O 10.159 MB in 15.5 s vs 10.201 MB
in 27.7 s; synthetic pixels 0.698 MB in 13.8 s vs 0.678 MB in 17.4 s. But 7-Zip
used ~1.6 threads even at `-mmt1` (its match finder runs in its own thread):
user time 24.1 s vs 27.7 s, so **~13% less CPU and slightly smaller**, and a
two-thread match finder for when there are fewer blocks than cores.
Worth vendoring (needs the 7-Zip source download).

**Where Max's CPU goes** (thread CPU time, `prof.rs` now uses
`CLOCK_THREAD_CPUTIME_ID`): Max 429 s user: pixel blocks 212 s (600 MB, xz-9e),
binary blocks 145 s (415 MB), text blocks 23 s, Lepton 23 s, preflate 23 + 4 s.
**Blocks are 89% of Max.** Under 20 s needs ~4x less block CPU. Default 67 s
user: Lepton 24 s, preflate 12 + 3 s, pixels 11 s, binaries 9 s, text 7 s.

**Fast LZMA2** (Conor McCarthy, BSD-3/GPL-2; parallel radix match finder, standard
LZMA2): claims 20–100% faster than 7-Zip's LZMA2 for 1–5% larger output. Useful
to spread one block over several cores, but not the ~4x Max needs, and the size
loss breaks the brief. Kept as a fallback for blocks-vs-cores.

### The LZMA SDK as Max's codec (done)

With the user's OK, 7-Zip 26.05's public-domain LZMA SDK files are vendored in
`engine/vendor/lzma-sdk` (checksum-verified pinned release; every file says
"Public domain"; see its ARCA-NOTES.md). New block codec `lzma2` (tag 4): raw
LZMA2 behind its property byte, no xz container or CRC-64 (blocks carry BLAKE3).
Max and Ultra use it (fb 273 = xz -9e's long matches; match finder in its own
thread). Decoding uses 7-Zip's ARM64 assembly loop. Measured on the Mach-O block
at fb 32, nothing else running: one thread 21.2 s wall / 21.2 s CPU, two threads
12.4 s wall / **17.7 s CPU**, output 10,529,319 vs 10,535,673 bytes (+0.06%; the
same on every run, so archives stay deterministic).

Whole saved folder, Max, busy Mac:
- size 210.84 → **210.60 MiB**; **LZMA decoding 4.76 → 3.18 s of CPU**; extraction
  3.99 → 3.83 s;
- no gain in make time (40 s): with ten blocks compressing at once there is no
  spare core for the second match-finder thread. Pack CPU 259 → 270 s.

**File order.** Most binary blocks were created only at 11.3 s, after the packer
had worked through every slow PNG/PDF. Orders tried (Max):

| Order within the binary group | Size | Make |
|---|---:|---:|
| Mach-O, ZIP, PNG, PDF, gzip, other binaries (before) | 210.60 MiB | 40.4 s |
| Mach-O, other binaries, gzip, ZIP, PDF, PNG | 211.66 MiB | 35.8 s |
| **Mach-O, ZIP, PDF, gzip, other binaries, PNG** | **210.35 MiB** | 39.6 s |

Kept the last (PNGs last: their pixels mostly go to the store lane now). Default
245.34 → 245.43 MiB with it (+0.04%), Fast unchanged.

**LZMA effort against CPU** (SDK, one 64 MiB block each, one thread, CPU s):

| Setting | Text | Mach-O | PDF + PNG tail | Other binaries |
|---|---|---|---|---|
| fb 273 | 6.249 MB / 28.1 | 10.155 / 33.6 | 6.407 / 33.8 | 20.561 / 25.6 |
| fb 128 | 6.285 / 23.6 | 10.170 / 31.3 | 6.523 / 24.8 | 20.593 / 22.1 |
| fb 64 | 6.366 / 20.0 | 10.233 / 28.8 | 6.641 / 19.3 | 20.631 / 19.3 |
| fb 48 | 6.431 / 18.2 | 10.303 / 27.8 | 6.680 / 17.4 | 20.653 / 18.2 |
| fb 32 | 6.593 / 15.6 | 10.529 / 23.1 | 6.769 / 14.7 | 20.734 / 16.2 |

Max is now CPU-bound (275 s on 10 cores), so make time follows LZMA effort.
Whole folder (Mac at load ~20):

| Max variant | Size | Make | CPU |
|---|---:|---:|---:|
| start of 2026-10-28 | 211.47 MiB | 105 s | 722 s |
| fb 273 (smallest) | 210.35 MiB | ~40 s | 275 s |
| fb 64, 32 MiB text blocks | 211.50 MiB | 26.8 s | 194 s |
| fb 32, 32 MiB text blocks | 212.85 MiB | 23.2 s | 168 s |

Under 20 s on a quiet Mac likely needs fb 32-ish and cheaper preflate/Lepton
work (34 s and 22 s of CPU), or a new idea for LZMA itself.

**The user chose "Max = fastest"** (2026-10-28): Max is now LZMA2 fb 32 with
32 MiB text blocks: **212.85 MiB in 23.3 s** on the saved folder (CPU 168 s),
extraction 3.67–3.85 s, round trip byte-identical. Ultra keeps fb 273 + PPMd text.

Further Max work:
- preflate checks the step budget proportionally (four times the share so far):
  PNG analysis 21.9 → 18.9 s CPU on the docs part, same size. Twice the share was
  21.9 → 15.5 s but rejected one more PNG (+0.10 MiB); exactly the share,
  14.1 s and +0.29 MiB.
- Dead end: running only the leading hash candidate after 2 MB of plain text
  (estimation was 25% of preflate's samples): 2% faster, 2 PNGs lost.
- Dead end: thread QoS. Block threads at user-interactive alone: no change
  (23.5 vs 23.6 s). Block threads high and file tasks at utility: 21.5 vs 23.1 s
  once, then 33–38 s vs 23 s in three alternating rounds (macOS moves utility
  threads to the efficiency cores under load). Too unpredictable.
- Max's floor: 168 s of CPU over 8 performance + 2 efficiency cores is ~19 s at
  perfect packing. The app-code blocks start first but are starved by 20+
  runnable threads and finish last, leaving cores idle for ~5 s.
- Extraction timeline (`ARCA_DEBUG_TIMING`, busy cores per quarter second): all
  10 busy from 0.25 s to 3.25 s, so extraction is CPU-bound (Lepton 11.4 s,
  Deflate + pixels 10 s, LZMA2 3.2 s, writing 2.3 s).

### Ultra, and Max on Silesia

Ultra (PPMd order 16 for text, LZMA2 fb 273, pixel codec) on the saved folder:
210.36 MiB in 34 s, extracting in 5.05 s; one 60 MB PPMd text block takes 4.1 s to
decode on one core. Here PPMd gains nothing over LZMA2 fb 273 (both 5.96 MiB of
text); on Silesia, prose, Ultra is 44.73 MiB against Max's 47.53. 16 MiB text
blocks: 210.61 MiB, extraction 4.26 s (Silesia +0.9%). Ultra stays "smallest".

The fast Max loses size on generic data: Silesia 47.53 MiB (49.8 MB) against 7-Zip
Ultra's 48.7 MB (website, 2026-10-26). The text lane is small and cheap, so Max
now gives it 64-byte matches: saved folder 212.85 → 212.65 MiB, Silesia 47.53 →
47.03 MiB, no time cost. 273-byte text matches: 212.55 / 46.76 MiB but +2 s on
the saved folder.

### A pixel codec for PNG data (most promising lead)

Per-PNG study of the saved folder (644 PNGs, 1,106 MB of pixels; standalone xz-9 per
image): photo-like images (under 6:1) are 56% of the compressed pixel bytes but
29% of the CPU; flat UI graphics (over 30:1) are 6.5% of the output and 13% of the
CPU. 75% of the whole gain from expanding PNGs costs 21% of the LZMA CPU.

Prototype (`examples/pixbench.rs`): undo the PNG row filters, predict each pixel
with MED (LOCO-I), code green's residual, then red and blue as differences from
green's, then alpha, with an LZMA-style adaptive binary range coder (12-bit
probabilities; contexts: 12 activity buckets x 3 previous-error classes; residual
as zero flag, sign, Elias-gamma exponent, mantissa). No run mode, no tuning.
On the 559 8-bit non-interlaced docs PNGs (1,042 MB of pixels):

| | Size | CPU |
|---|---:|---:|
| xz-9 on the filtered rows, per image | 70.44 MB | 167.8 s |
| pixel codec v0, per image | 70.62 MB | 22.3 s encode, 21.7 s decode |
| better of the two, per image | **61.77 MB** | |

By type (xz ratio): under 3:1 ours is 0.894 of xz; 3–6: 0.854; 6–12: 0.940;
12–30: 1.185; over 30: 1.753 (flat areas need a run mode). Decoding runs at ~48 MB/s
per core, slower than LZMA's ~150 MB/s: a cost on extraction to keep in view.

Iterations (docs PNGs, 1,042 MB of pixels; xz-9 per image = 70.44 MB):

| Version | Change | Size | Best of codec/xz per image | Encode / decode CPU |
|---|---|---:|---:|---:|
| v0 | MED, per-channel zero flag | 70.62 MB | 61.77 MB | 22 s / 22 s |
| v1 | whole-pixel zero flag; green's residual as context | 64.32 MB | 60.49 MB | 18 s / 19 s |
| v2 | 16-bit probabilities, fast+slow rates averaged | 63.80 MB | 60.00 MB | 19 s / 21 s |
| v3 | blend of 6 predictors weighted by neighbours' errors | 63.86 MB | 59.10 MB | 42 s / 63 s |

v3 is better on photos (0.80 of xz) and worse on flat images, which go to LZMA
anyway. **On the 439 PNGs Max actually expands** (the budget skips the costliest
photo-like ones): xz-9 49.23 MB; MED codec 43.80 MB; blend 43.20 MB; better of
blend/xz per image **39.75 MB (−19%)**, which would take ~9 MB (~4%) off Max and
most of the pixel lane's 212 s of CPU.

The catch is decoding: 13–30 MB/s per core (about ten adaptive binary decisions
per byte) would add ~10 s of CPU to Max extraction, losing to Zipgeist there. The
production design needs fast decoding: integer-only prediction, static per-stripe
statistics, table-driven interleaved rANS for a combined zero/sign/exponent token
with raw mantissa bits, aiming at 200+ MB/s.

**v4, the fast design** (same PNGs Max expands, 409 eligible, 573 MB of pixels;
times with nothing else running except other sessions):

| Step | Size | Encode / decode CPU |
|---|---:|---:|
| v2 (adaptive binary, MED) for reference | 43.80 MB | 14.5 s / 24.9 s |
| v4: 16-token adaptive tables (LZNA-style) + rANS, raw remainder bits | 46.32 MB | 11.0 s / 16.3 s |
| zero-padded image, LUT activity, zero-flag row instead of `memcmp` | 46.37 MB | 7.5 s / 7.3 s |
| top remainder bit modelled | 45.69 MB | 7.9 s / 7.5 s |
| table adaptation rate 6 (was 4) | 44.38 MB | 8.1 s / 7.5 s |
| warm-up (fast at first, settling to rate 7) | **44.03 MB** | 8.4 s / 7.8 s |

So v4 is within 0.5% of v2 at ~3x the decoding speed (~75 MB/s per core). For
comparison LZMA decodes these pixels at ~133 MB/s in the real Max archive, so the
extra extraction CPU is ~2 s (~0.2 s wall) for the images the codec takes.

**Choosing per image without running xz:** pixel codec when its output is under
0.9x the zstd-1 size of the filtered rows, else the rows go to the LZMA image lane.
That gives 41.30 MB against 40.95 MB for a perfect choice and 49.23 MB for xz-9
alone (−16%), with the codec taking 484 of the 573 MB of pixels.

**In the engine** (`transform/pixels.rs`, Deflate layout version 2): a PNG whose
pixels the codec takes stores layout, corrections, literal bytes and row filter
types (binary lane) followed by the coded pixels (store lane); extraction decodes
the pixels, re-applies the row filters and re-creates the Deflate stream. Unit
tests cover round trips, filters and malformed input. On at Max and Ultra.

Whole saved folder, Max (Mac at load ~24 when it started, so times are rough):

| | Size | Make | Extract | Pack CPU |
|---|---:|---:|---:|---:|
| Max at the start of 2026-10-28 | 211.47 MiB | 105 s | 9.6 s | 722 s |
| + rebuild budget | 217.17 MiB | 73 s | 3.58 s | 429 s |
| **+ pixel codec** | **210.84 MiB** | **38.3 s** | 3.99 s | 259 s |

Round trip byte-identical. The codec took 409 PNGs (573 MB of pixels); the image
lane shrank to 117 MiB of flat images (4.73 MiB compressed, 26 s of LZMA instead
of 212 s). Remaining pack CPU: binary lane 145 s (xz-9e), preflate 24 + 12 s
(verification now includes decoding pixels), Lepton 24 s, text 22 s, pixels 8 s.
Extraction: pixel decoding adds ~7 s of CPU and LZMA decoding drops 3.4 s.

**Max's critical path** (block timeline, `ARCA_DEBUG_TIMING=1` now prints each
block's queue, start and end): files are done at 11.9 s, but each 64 MiB xz-9e
block takes 17–32 s with ten running, the one text block 27.5 s, and most binary
blocks were only created at 9.2 s because the in-order packer waited on slow
PNG/PDF files ahead of them. Smaller blocks (busy Mac, so sizes only):

| Blocks | Size | Text lane | Binary lane |
|---|---:|---:|---:|
| 64 MiB text and binary (Max) | 210.84 MiB | 5.97 | 92.34 |
| text 32 MiB | 211.03 MiB | 6.16 | 92.34 |
| text 16 MiB | 211.16 MiB | 6.29 | 92.34 |
| binary 32 MiB, text 32 | 212.82 MiB | 6.16 | 94.12 |

32 MiB binary blocks cost 2% of the binary lane: not the lever. Spreading one
block over two threads with identical output (7-Zip's threaded match finder)
is. The pixel codec's 16-entry table search and update now use NEON.

(Earlier plan, kept for the record: build it into the engine as a PNG mode of the Deflate transform (pixels coded
in the file task, routed to the store lane; layout, corrections and row filter
types to the binary lane), at Max and Ultra first. Default would need faster
decoding first (it would add ~0.7 s to Default's 2.6 s extraction). (pixels + row filter
types + preflate corrections), choosing per image between the pixel codec and the
LZMA image lane, measured in real archives (solid blocks find cross-image matches
that standalone numbers miss).

Ideas queued, with why:

- **PNG token coding.** For costly PNGs, instead of replaying the encoder, store the
  Deflate token stream (literal / match) coded with a model that sees the image
  rebuilt so far, and rebuild by Huffman-coding the tokens again. Rebuild cost
  becomes entropy decoding (no searches). Could recover the 5.85 MiB without the
  59 s of rebuild CPU.
- **Parallel rebuild of one Deflate stream.** Preflate's only state across Deflate
  blocks is the hash chain (pending lazy match resets per block; the CABAC coder
  is already fresh per chunk). Priming a predictor with the 32 KB before a block
  lets segments of one stream rebuild on all cores. Removes the single-PNG critical
  path (3 s for one 16.7 MB image). Pack-time verification guarantees exactness.
- **A pixel codec for PNG data** instead of LZMA on filtered rows (1 GB of lane data
  at Max; LZMA-9e there is most of Max's pack time).
- **App extraction overhead.** The app measured Fast extraction at 2.21 s against
  0.93 s for the CLI. After the engine finishes, `SafeExtraction.finalize` walks
  every extracted item and sets the quarantine attribute by path (~14,500 calls on
  the saved folder). The engine could set it through the descriptor it already
  holds. Needs an optional field in the extract request (FFI stays 9 functions).

### Default: where its time goes now (2026-10-28 evening)

Saved folder, Default, busy Mac: 245.48 MiB in 9.4–9.6 s, 65.6 s of CPU. Thread
seconds: Lepton encode + verify 23.0, preflate on PNGs 10.7 + verify 2.3, PDFs
1.5, blocks ~24 (zstd-15 text 58 MB, zstd-12 binary 358 MB and image 536 MB).
Every file is transformed at 7.9 s; the last image block ends at 9.4 s.

**Lepton** (`examples/leptbench.rs`, one thread per file): encode 9–10 MB/s,
decode 9–10 MB/s per core, so `encode_lepton_verify` doubles the work; that is the
price of proving every JPEG comes back exactly. ~10% of its samples are
`memset_pattern16` inside `adv_predict_dc_pix` (vector constants from the `wide`
crate); its SIMD is NEON on ARM. The calls are LLVM's loop idiom: every vector
constant in `wide` is an array fill, which LLVM turns into `memset_pattern16`
on Apple targets (19 calls of 32 bytes per block in the DC predictor alone;
patching `wide` to build `i32x8` from halves only made it 30 calls of 16 bytes,
4% faster). **Building with `-C llvm-args=-disable-loop-idiom-memset`**
(`engine/.cargo/config.toml`, so the app's build too): Lepton encode 9.8 →
8.6 s, decode 10.4 → 9.1 s on the photos (−12%), byte-identical output; whole
saved folder, three alternating runs: Default extraction 2.41–2.51 → 2.28–2.44 s,
Max 3.08–3.23 → 3.00–3.09 s, Fast unchanged, packing CPU 1.5–2 s lower, sizes
identical.

**PNGs made by stock zlib** (`scratchpad zreplay.c`, every PNG of the saved
folder): 480 of 644 PNGs (81.9 of 122.9 MB of Deflate) are byte for byte what
macOS's zlib makes: level 1 56.1 MB (ImageIO's default), level 9 18.2 MB, level 6
5.7 MB. On those, preflate at Default takes 3.9 s forward + 1.35 s verify (level
1) where inflate + one zlib re-run takes 1.8 s and is its own check; re-creation
costs the same either way (zlib -1 1.53 s, preflate 1.35 s). Level 9 is 6 MB/s in
zlib, slower than preflate. Default also spends 2.7 s rejecting 92 level-9 PNGs
its chain limit refuses (the zlib header's FLEVEL = 3 says so up front). A replay
path needs a zlib whose output never changes: macOS's libz could change with an
OS update, so it would need a pinned copy (zlib source is not on this Mac; a
download). Worth ~7 s of Default's CPU (~0.8 s).

**Level-9 PNGs skipped at Default.** Default's search limit (256) makes preflate
reject almost every PNG from zlib levels 7-9 after a full analysis. The zlib
header's FLEVEL (3 for levels 7-9) now skips them up front when the limit is
under 1024. Saved folder, three alternating runs: PNG analysis 11.0 → 6.3 s of
CPU, pack CPU 63.2 → 57.9 s, size 245.48 → 245.47 MiB; wall 9.1-9.2 → 8.8-9.2 s.

**zlib replay for PNGs (in the engine, 2026-10-29).** With the user's OK, zlib
1.3.2's encoder is vendored in `engine/vendor/zlib` (see its ARCA-NOTES.md),
built with `Z_PREFIX`. It re-creates exactly the same 480 of 656 PNGs as macOS's
libz 1.2.12 (`scratchpad zr3`, every PNG of the saved folder), so a pinned copy is
safe against OS updates. Stopping the re-run at the first differing byte makes a
wrong guess cheap for levels 1-6 (41 non-zlib PNGs: 0.27 s for every guess), but
not for level 9 (123 PNGs: 80 s), and zlib -9 re-creates its own streams at
6 MB/s (99 PNGs: 56 s), slower than preflate. Level 6 re-runs slower than
preflate rebuilds (1.04 against 0.59 s for 76 PNGs). So the Deflate transform
re-runs zlib only for zlib-wrapped streams (PNG, PDF) whose header says level 1-5,
and falls back to preflate when no setting matches. Layout: version byte flag
`REPLAY`, one method byte per stream (see `deflate.rs`).

PNGs at Default's settings (`examples/preftime.rs`, two runs each, CPU):

| zlib level | PNGs | preflate forward / inverse | replay forward / inverse |
|---|---:|---:|---:|
| 1 | 301 | 3.81 / 1.35 s | **1.53 / 1.18 s** |
| 3 | 3 | 0.11 / 0.05 s | 0.05 / 0.04 s |
| not zlib, header 0-6 | 41 | 0.14 / 0.04 s | 0.17 / 0.04 s |

Saved folder (busy Mac, two alternating runs each; every round trip identical):
Default 257,398,331 → 257,395,835 bytes, pack CPU 59.2 → 54.9-55.8 s (−6.6%),
wall 9.4-9.6 → 9.1-10.0 s (noise), extraction 2.2-2.3 s both; Max 221,973,037 →
221,962,595 bytes, pack CPU 214.0-214.7 → 212.1-212.7 s.

**Default's tail is two image blocks.** The block timeline shows every file
transformed at 6.5 s, then two 64 MiB image blocks (zstd-12 on flat PNG rows,
where its binary-tree search slows down) finishing at 7.7 and 9.0 s. Options
(saved folder, three runs each; image lane / total / make):

| Image lane | Image lane | Total | Make |
|---|---:|---:|---:|
| zstd-12, 64 MiB blocks (Default) | 46.69 MiB | 245.47 MiB | 9.1-9.5 s |
| zstd-12, **32 MiB blocks** | 46.80 | 245.59 (+0.05%) | **7.8-7.9 s** |
| zstd-12, 16 MiB blocks | 46.86 | 245.65 (+0.07%) | 7.4-7.7 s |
| zstd-12, 8 MiB blocks | 46.99 | 245.78 (+0.13%) | 7.2-7.8 s |
| zstd-11 | 46.73 | | 8.8 s |
| zstd-9 | 47.44 | | 7.0 s |
| LZMA2 fb 32 | 41.04 | | 43 s |

Smaller blocks for every lane cost the binary lane +1.35 MiB at 32 MiB, so the
image lane got its own block size (`image_block_size`, `--image-block-mib`).
Not switched on (it trades 0.05% of size for 16% of time): for the final
decision, with the pixel codec at Default, which removes most of this lane.

**The pixel codec at Default** (`ARCA_EXP_PIXELS=1`): 245.48 → **233.57 MiB**
(−4.9%) and make 9.4 → 7.8–8.2 s (the codec replaces zstd-12 on 536 MB of
pixels), but extraction 2.74 → 3.0–3.3 s, behind Zipgeist's 3.0 s. Extraction is
CPU-bound from 0.25 s to the end (Lepton 11.8 s, Deflate + pixels 8.6 s).

**Faster pixel decoding, same format.** Counting instructions (`/usr/bin/time -l`)
showed the decoder executing ~100 instructions per byte at 4.7 per cycle: bound by
instruction count, not by dependency chains or branch misses. Hence:

| Try | Decode (docs PNGs, 1 GB of pixels) | Size |
|---|---|---|
| engine before | 110 MB/s | 64.659 MB |
| two or four bands decoded side by side, one core | 108 / 107 MB/s | +0.36 / +0.93% |
| branchless rANS refill | 112 MB/s | same |
| branchless top bit (decode always, keep when needed) | 95 MB/s | same |
| **one decoder per channel count, flat pixels skip the context, table-driven update and tokens** | **171–182 MB/s** | **same bytes** |

The encoder got the same treatment (103 → 119 MB/s) and writes byte-identical
output on all 559 PNGs, so archives are unchanged. PNG row filters now run one
loop per filter type: refilter 595 MB/s → 10.8 GB/s, unfilter 634 → 842 MB/s.
Flat graphics decode at ~300 MB/s, photo-like images at ~135 MB/s (82
instructions per byte; going further needs a format change, e.g. static tables).

**Photos were the slow case.** Pixel coding at Default only where it saves the
most (a byte per N pixel bytes, against 0.9x zstd-1 ≈ zstd-12) cut extraction
CPU far less than the pixel count: the images it keeps are photo-like, and they
decoded at 60–67 MB/s (154 instructions per byte, IPC 3.3; bands side by side
did not help there either). The compiled loop kept both rANS states in memory
indexed by symbol parity, did not unroll the channels, bounds-checked every table
index and branched on MED's comparisons and on each binary symbol's bit. Now:
the states swap in registers, a macro writes the channels out with constant
indexes, MED is a clamp, binary symbols and the refill use selects, and the
token table has a sentinel entry. Same bytes in and out:

| | Photo-like PNGs (175) | Flat (32) | All 604 PNGs |
|---|---:|---:|---:|
| decode, start of the evening | 67 MB/s | 145 MB/s | 110 MB/s |
| **decode now** | **101 MB/s** | **353 MB/s** | **225 MB/s** |
| encode, start → now | 58 → 75 MB/s | 134 → 149 | 103 → 117 |

Default with the pixel codec, saved folder (busy Mac, three runs each):

| Pixel codec at Default | Size | Extract | Extract CPU (user) |
|---|---:|---|---:|
| off (Default today) | 245.48 MiB | 2.45–2.68 s | 14.5 s |
| where it saves a byte per 50 pixel bytes | 234.63 MiB | 2.48–2.54 s | 16.2 s |
| where it saves a byte per 30 | 235.19 MiB | 2.57–2.65 s | 15.8 s |
| wherever it wins | 233.57 MiB | 2.55–2.63 s | 16.6 s |

Make time drops by 0.5–1.3 s with it on (the codec replaces zstd-12 on the
pixels). Not switched on: extraction gets ~0.05–0.15 s slower, so it is the
user's call (`ARCA_EXP_PIXELS=1`, `ARCA_EXP_PIXEL_GAIN=N` to try it). Max
extraction CPU drops 1 s with the faster decoder.

## Writing files (extraction)

Fast extraction of the saved folder was 1.0 s with 1.1 s of user CPU and **5.0 s
of kernel CPU**. `examples/writebench.rs` recreates the extracted tree from memory:
creating 11,050 empty files costs 3.3 s of kernel time on 10 threads; writing their
661 MB adds nothing measurable. APFS serialises file creation:

| Threads creating | Wall | Kernel CPU |
|---|---:|---:|
| 1 | 0.95 s | 0.90 s |
| 2 | 0.63 s | 1.15 s |
| 4 | 0.55 s | 1.72 s |
| 10 | 0.50–0.53 s | 3.1 s |
| 10, at most 4 inside `open` | 0.43 s | 2.15 s |

Gating `close` does nothing; `openat` per directory does nothing. Changes:
- **at most 4 threads create files at once** (`create_file`, used by the
  extractor and the ZIP reader, and clones);
- **duplicates under 64 KiB are written again** while the content is in hand
  (1,002 duplicate files cost 0.87 s of kernel time as clones in a 0.14 s tail
  phase); larger ones are still cloned (they share space);
- **mode and times in one call** (`fsetattrlist` / `setattrlist`) instead of a
  chmod and a utimes, for files and directories;
- directories that files were written into are not `mkdir`ed again.

Extracted trees are identical to before (14,505 items: contents, mode, mtime to
the nanosecond, flags, xattrs). Busy Mac, three runs each:

| Extraction | Before | After | Kernel CPU |
|---|---:|---:|---:|
| Fast | 0.97–1.02 s | **0.74–0.96 s** | 5.0 → 3.4 s |
| Default | 2.72–2.80 s | **2.57–2.63 s** | 5.2 → 3.6 s |
| Default + pixel codec | 2.98–3.04 s | 2.80–2.97 s | 4.8 → 3.6 s |
| ZIP (`ditto-mixed.zip`) | 1.05–1.39 s | **0.91–0.99 s** | 3.6–4.0 → 3.0–3.3 s |

**Writes alongside the rest.** The extraction timeline then showed every plain
write at the end (Max: 2.75–3.5 s), because the scheduler ran the costliest work
first and writes are cheap; at the end only four threads could create files and
the other cores waited. Writes now have their own queue: up to four run at any
time, the other threads decode and rebuild. Saved folder, three alternating runs:

| Extraction | Before | After |
|---|---:|---:|
| Fast | 0.80–0.86 s | 0.74–0.88 s |
| Default | 2.38–2.50 s | **2.19–2.28 s** |
| Max | 3.05–3.20 s | **2.92–3.02 s** |

**The app's own overhead**, measured with a temporary test in the app's test
target (optimised build, saved folder, not committed): Fast extraction through
`Automation.extract` takes 0.99–1.15 s, of which the engine is 0.87 s, opening
the archive (its listing) 0.09 s and the safety walk 0.10 s. The 2.21 s measured
on 2026-10-26 is gone with the engine's own speed-ups. For a downloaded archive,
setting the quarantine on all 14,505 items by path added 0.15–0.4 s; the engine
now sets it through each file's descriptor as it writes (`ExtractOptions::
quarantine`, also in the FFI requests for `.zlg` and ZIP), and the app's pass
runs again only if it merged AppleDouble files. All 58 app tests pass; a new
engine test checks every kind of item gets the quarantine.

Kernel CPU 3.6 → 2.9 s at Default and Max. Max's extraction is CPU-bound from
0.25 s to the end with no single-file tail (Lepton 10.4 s, Deflate 4.4 s, LZMA2
3.3 s, pixels 2.6 s), so re-creating one Deflate stream on several cores would
not shorten it on this folder.

### LZMA on the code lane: match finders and hint-guided parsing (dead ends)

The code lane's first block (67.1 MB of static libraries, 46:1) at fb 273 on one
thread spends 42% of its samples in `Bt4_MatchFinder_GetMatches`, 26% in
`Bt4_MatchFinder_Skip` and 31% in the encoder (optimal parse). Match finders
(`examples/lzone.rs`, one encode each, one thread):

| Match finder | Size | CPU |
|---|---:|---:|
| bt4 (Max) | 1.391 MB | 33.7 s |
| bt4, 32 cycles | 1.436 MB (+3.2%) | 27.5 s |
| hc4 | 1.452 MB (+4.4%) | 30.4 s |
| hc5 | 1.435 MB (+3.2%) | 31.1 s |
| bt2 / bt3 | 1.400 / 1.397 MB | 34.4 / 34.7 s |

**Hint-guided LZMA** (tried and reverted; the SDK files are unchanged): the
long-range pre-pass hands LZMA its repeats ([start, end, distance]); inside one,
the encoder takes it as a single match (checked byte for byte) and skips the
search, optionally without inserting the skipped positions into the tree. Taking
hints of 256+ bytes: 1.524–1.528 MB (+10%) in 23.3 s without inserting, 33.4 s
with; 512+: 1.409 MB (+1.3%) in 27.9 s. Taking a hint only where LZMA's own
longest match is shorter: 1.390 MB in 32.6 s, i.e. LZMA already finds these
repeats, and its optimal parse codes them ~10% better than one long match each
(repeat distances and split points). The time is the parse over long repeats,
not missed matches.

### BWT (libbsc) for the text and code lanes: the Max breakthrough

With the user's OK, libbsc 3.3.12 (Apache-2.0, bundling libsais 2.10.4) is
vendored in `engine/vendor/libbsc` (see its ARCA-NOTES.md). Block codec `bsc`
(tag 5). One block on one core each (`examples/bscbench.rs`), default bsc
settings (LZP 15/72, static QLFC) against Max's LZMA2 fb 273:

| Block | bsc | LZMA2 fb 273 | bsc size | bsc speed |
|---|---|---|---:|---:|
| saved folder text, 60.4 MB | 6.233 MB, 1.8 s (decode 1.3) | 6.249 MB, 28.3 s (0.22) | −0.3% | 16x |
| Silesia text, 67.7 MB | 7.389 MB, 2.4 s | 9.159 MB, 44.2 s | **−19%** | 18x |
| Silesia text, 17.5 MB | 2.878 MB, 0.8 s | 3.678 MB, 12.1 s | **−22%** | 16x |
| static libraries, 67.1 MB | 1.452 MB, 0.65 s | 1.391 MB, 40.5 s | +4.4% | 62x |
| static libraries, 36.8 MB | 0.880 MB, 0.3 s | 0.859 MB, 22.9 s | +2.4% | 76x |
| Silesia code (x86 tar), 67.2 MB | 21.590 MB, 3.3 s | 18.143 MB, 29.7 s | +19% | 9x |
| Mach-O binaries, 67.6 MB | 13.301 MB, 2.8 s | 10.155 MB, 31.8 s | +31% | 11x |
| Silesia binary, 42.4 MB | 14.482 MB, 2.3 s | 15.802 MB, 20.6 s | −8% | 9x |

The adaptive QLFC coder with a larger LZP closes the code gap: static libraries
1.375 MB with LZP 24/64 (−1.2% against LZMA2, 0.75 s), 1.358 MB without LZP
(−2.4%, 1.7 s); text 6.167 MB with LZP 18/128 (−1.3%) and Silesia 7.321 MB
(−20%). Against PPMd order 16 (Ultra's text codec): 6.167 vs 6.254 MB,
7.321 vs 8.108 MB, 2.858 vs 3.086 MB, and 2–3x faster both ways.

Presets: `bsc:1` text (LZP 18/128, adaptive), `bsc:2` code (LZP 24/64, adaptive);
`e` falls back to LZMA2 fb 273 for a block bsc gets under 16:1 (machine code
without long repeats, where LZMA2 wins).

**Saved folder, Max** (busy Mac): current 211.69 MiB in 32.4 s (212 s CPU);
**with `--lane text=bsc:1 --lane code=bsc:2e`: 211.58 MiB in 19.8 s (136 s
CPU)**: the text block takes 2.7 s instead of ~24 s, the 64 MiB code block 1.0 s
instead of ~30 s. Extraction 2.87–2.95 → 3.15–3.31 s: bsc decodes at ~40 MB/s
against LZMA2's ~250 MB/s (+2.2 s of CPU). Round trip identical.

Website sets (busy Mac; make / extract in seconds; every round trip identical):

| Set | Max | Max + bsc | Ultra (PPMd text) | Ultra + bsc |
|---|---|---|---|---|
| source | 8.232 MB, 23.1 / 0.50 | **8.094 MB, 1.85** / 1.78 | 8.235 MB, 22.9 / 4.05 | **8.093 MB, 1.86 / 1.80** |
| silesia | 48.629 MB, 26.6 / 0.70 | **45.971 MB, 18.4** / 2.04 | 46.933 MB, 14.9 / 6.41 | **45.918 MB**, 18.1 / **2.02** |
| docs | 18.203 MB, 11.8 / 0.45 | same | 17.814 MB, 15.8 / 0.46 | same |
| apps | 49.158 MB, 13.8 / 0.72 | 49.151 MB, 13.5 / 0.73 | 48.526 MB, 16.4 / 0.76 | 48.550 MB (+0.05%), 16.3 / 0.72 |
| mixed (all five) | 194.785 MB, 44.5 / 2.30 | **192.337 MB, 24.1** / 3.01 | 191.906 MB, 40.9 / 7.27 | **191.295 MB, 32.0 / 3.06** |
Both 7-Zip Ultra targets hold with room to spare (source 8.09 < 8.5 MB, Silesia
45.97 < 48.7 MB). Extraction of text-heavy sets is slower at Max: a 64 MiB bsc
block decodes on one core in ~1.6 s, and those sets have few blocks.

**Smaller text blocks** cost size on source code (Max + bsc, size / make /
extract): source 16 MiB 8.368 MB / 1.22 / 0.76 s, 32 MiB 8.274 / 1.27 / 1.12 s,
64 MiB 8.094 / 1.86 / 1.75 s; Silesia 16 MiB 46.137 MB / 0.73 s, 32 MiB 45.975 /
1.08 s, 64 MiB 45.971 / 2.02 s; the saved folder hardly changes (221.85–222.12
MB, 3.1–3.3 s). Deciding the LZMA2 fallback on a 4 MiB sample brought Silesia's
Max make time from 18.4 to 14.9 s.

**Decoding one bsc block on several cores.** A bsc block's decode is 54% QLFC
(entropy), 41% inverse BWT and 5% LZP (samples on Silesia text). bsc's format
already splits the QLFC stream into 24–32 independent sub-blocks for a 64 MiB
block and stores 8–16 sampled BWT indexes, but only an OpenMP build decodes them
in parallel. `vendor/libbsc/src/arca_bsc.cpp` runs bsc_decompress's stages with
plain threads (same format, same checks) and `src/arca_sais.c` (which includes
libsais.c unchanged) adds the threaded inverse BWT. Byte-identical; one block:

| Block | one thread | threaded (wall) |
|---|---:|---:|
| saved folder text, 60 MB | 1.47 s | 0.51 s |
| Silesia text, 68 MB | 1.91 s | 0.67 s |
| static libraries, 67 MB | 0.52 s | 0.22 s |
| Silesia x86 tar, 67 MB | 3.93 s | 0.92 s |

Extraction, Max against Max + bsc (three runs each): source 0.50 s → 0.97–1.00 s
(1.78 s before threading), Silesia 0.69–0.70 → 0.94–1.03 s (2.04 s), saved folder
2.86–3.03 → 3.25–3.38 s (CPU-bound there). bsc needs ~4–6x LZMA2's decode CPU,
so text-heavy archives extract slower than with LZMA2, still well ahead of the
7-Zip and Zipgeist numbers measured on 2026-10-26.

**User's decision (2026-10-28): not in the presets yet**; bsc stays an option
(`--lane text=bsc:1 --lane code=bsc:2e`) until the final quiet-Mac benchmarks.

**Default with bsc text** (instead of zstd-15; size / make / extract):

| Set | Default | bsc text, 16 MiB blocks | bsc text, 32 MiB |
|---|---|---|---|
| source | 11.201 MB, 2.08 / 0.39 s | 10.121 MB (−9.6%), 1.02 / 0.70 s | 10.027 MB (−10.5%), 1.19 / 0.64 s |
| silesia | 57.317 MB, 5.42 / 0.17 s | 52.999 MB (−7.5%), 2.46 / 0.63 s | 52.838 MB (−7.8%), 2.49 / 0.67 s |
| saved folder | 257.399 MB, 9.71 / 2.17 s | 256.250 MB, 8.66 / 2.47 s | 256.178 MB, 8.74 / 2.60 s |

Same trade as at Max: smaller and faster to make, slower to extract (zstd
decodes text at ~1 GB/s).

**Lepton's decoder, decode only** (`LEPT_DECODE_REPS`, samples): 7x7 coefficients
29%, edge coefficients 19%, per-block driver 12%, JPEG Huffman output 12.5%, DC
prediction 7%, DC 6%, non-zero counts 14%. The coding parts are ~67%: a coder
with multi-symbol decisions instead of one binary decision per bit could make
Lepton ~1.5-1.8x faster. Its bool decoder and count-based probabilities (64 KB
lookup) are already lean.

**Multi-symbol size classes for Lepton (measured, dead end).** A shadow model in
a scratchpad copy of Lepton: wherever Lepton codes a coefficient's size class as
a unary chain of binary decisions, a 16-symbol adaptive table for the same
context records what it would cost (ideal code lengths). Photos, 98.4 MB → 73.8 MB:

| Size classes | Lepton | 16-symbol tables | Size |
|---|---|---|---:|
| 7x7 | 22.233 MB, 249 M decisions | 22.596 MB, 134 M symbols | +1.6% |
| edge | 10.952 MB, 148 M decisions | 11.052 MB, 58 M symbols | +0.9% |
| DC | 3.754 MB, 46 M decisions | 3.754 MB, 21 M symbols | 0.0% |

Best variant +0.63% of Lepton's output (final rate 7); finer warm-up +0.83%,
two-speed tables +0.70%, count-based tables with halving +0.63%. Lepton's binary
chain learns each node with its own counts, which this data rewards. And size
classes are only 443 M of Lepton's 930 M decisions (signs, residual bits and
counts are the rest), so the coder would do ~25% less work, not a multiple.
Fails "at least as small"; the JPEG lever needs a different idea.

### Mach-O split into code, strings and tables (dead end)

Idea: Mach-O files are ~60% machine code (LZMA's best case) and ~40% sections
that look like text (Objective-C method names, C strings, Swift names, symbol
tables), so send the second part to bsc. `examples/machosplit.rs` takes the 24
Mach-O files of the apps set (99.5 MB), arm64-filters them as packing does, and
splits them into code (61.5 MB), string sections and symbol names (13.2 MB),
and everything else (24.8 MB: pointers, offsets, relocations, signatures). One
thread, dictionary covering the whole input:

| Input | LZMA fb 32 | LZMA fb 273 | bsc (LZP 18/128) | bsc (no LZP) |
|---|---:|---:|---:|---:|
| whole, 99.5 MB | 14.545 MB, 30.4 s | 14.000 MB, 40.2 s | | |
| code, 61.5 MB | 9.166 MB, 21.8 s | 8.704 MB, 26.4 s | | |
| not code, 38.0 MB | 5.360 MB, 7.6 s | 5.272 MB, 11.7 s | 7.865 MB, 1.4 s | 7.821 MB, 1.4 s |
| strings, 13.2 MB | 1.423 MB, 2.5 s | 1.404 MB, 4.1 s | 1.524 MB, 0.37 s | 1.426 MB, 0.36 s |
| other, 24.8 MB | 4.037 MB, 4.8 s | | | |

Code and the rest as two LZMA streams: 14.526 MB (−0.13%) for 3% less CPU, too
little for a new transform. bsc loses by 46% on the non-code part (binary tables
are LZ data) and only ties on the strings, which are full of long repeated
prefixes (mangled Swift names, selector families) that LZ codes as well as a
BWT. Splitting the strings out also costs LZMA 0.1 MB of cross-references
(three streams: 14.626 MB). bsc's only gain is speed on the 13% of bytes that
are strings: ~2 s of CPU in 30, at 7x the decode time.

### x86_64 code in universal binaries (in the engine, 2026-10-29)

Every Mach-O file in the test sets is one of Apple's apps, universal over arm64
and arm64e only. Installed third-party apps are different: the main executables
of the 32 apps in /Applications (1.93 GB, 80 universal files) hold 564 MB of
x86_64 code next to 702 MB of arm64 code, and the arm64 filter left the x86_64
slices alone. XZ's x86 BCJ filter (CALL/JMP rel32 made absolute) on 64 MB of that
code: xz -9 20.51 → 19.86 MB (−3.2%), 7% less CPU. Ported to Rust
(`arm64::filter_x86`, a bijection tested on random data) and applied to the
instruction sections of x86_64 slices; the index stores those ranges under
transform tag 4 (arm64-only files keep tag 1).

Seven executables from /Applications (Blender, Telegram, Firefox's XUL,
qBittorrent, Slack, Discord, Figma; 660 MB), round trips identical:

| Level | arm64 filter only | + x86 filter | Make CPU |
|---|---:|---:|---:|
| Max | 210.386 MB | **208.677 MB (−0.81%)** | 264.9 → 260.2 s |
| Default | 255.904 MB | **254.032 MB (−0.73%)** | 37.8 → 37.7 s |

### Profile-guided optimisation (measured, for release builds)

Homebrew's LLVM 23.1.4 matches rustc 1.99's LLVM 23.1.3, so `llvm-profdata` can
merge Rust profiles. Instrumented build trained on the saved folder (Fast,
Default, Max, Max + bsc, Default + pixels) and Silesia and source at Ultra; then
`-Cprofile-use`. Saved folder, two alternating runs each (busy Mac), sizes
identical:

| Level | Make CPU | Extract CPU | Extract wall |
|---|---|---|---|
| Fast | 3.52 → 3.37-3.58 s | 1.02-1.06 → 1.02-1.03 s | same |
| Default | 54.7-56.4 → 54.5-54.6 s (−2%) | 14.1 → 13.4-13.6 s (−4%) | same |
| Max | 209.0-209.7 → 207.4-207.7 s (−1%) | 20.6-20.8 → 20.1 s (−3%) | 2.85-2.97 → 2.77-2.78 s |

Only the Rust code is profiled; LZMA, zstd, bsc and zlib are C built by Apple
clang, which can't share a profile runtime with rustc's. Worth a step in release
builds (train, then build with the profile), not a lever.

### Compressed data inside Apple containers: Assets.car (2026-10-29; see LZFSE replay)

Compiled asset catalogs keep most images LZFSE- or LZVN-compressed (`bvx2`/`bvxn`
blocks ending in `bvx$`), which no archiver can compress further. The saved
folder has 23 `Assets.car` files (38.2 MB): 12.08 MB of LZFSE streams that decode
to 202 MB, 3.9 MB of zlib streams (7.7 MB), the rest raw renditions and JPEGs.
Re-encoding each decoded stream with macOS's own LZFSE (libcompression) gives back
the same bytes for 769 of 784 streams (10.45 of 12.08 MB): a replay is possible,
as with zlib. On those streams (one thread):

| Input | LZMA fb 32 | zstd -19 |
|---|---:|---:|
| LZFSE streams as stored, 10.45 MB | 8.398 MB, 1.3 s | |
| decoded, 181 MB | **5.963 MB (−29%)**, 11.5 s | 7.663 MB |

The installed apps on this Mac hold 239 `Assets.car` files (669 MB): 408 MB of
LZFSE streams decoding to 4.58 GB, 362 MB of them replayable with libcompression.
The catch is volume: decoded data is 11x larger, so LZMA on it costs ~9x the CPU
and extraction must re-run LZFSE on 4.58 GB. Cheaper routes to test: code the
LZFSE token stream (literals, lengths, distances) with an adaptive model and
re-create the FSE coding from the tokens (no match search either way); or the
pixel codec on decoded renditions (width, height and pixel format are in the
rendition header). A pinned LZFSE encoder is needed (the open-source reference,
BSD-3; whether it matches libcompression byte for byte is the first test, and
the download needs the user's OK).

### Disk images (DMG) expanded and rebuilt (in the engine, 2026-10-29)

A UDIF disk image is a table of chunks (zlib, LZFSE, bzip2, LZMA, raw, zero) and
a `koly` trailer; archivers store it as it is. `hdiutil create` of the apps set
(169 MB): UDZO 102.3 MB, 178 zlib chunks (74.8 MB, 163.7 MB decoded), every one
re-created by zlib level 1 (macOS's zlib, so the pinned 1.3.2 too); ULFO 97.1 MB,
193 LZFSE chunks (84.5 MB → 179.5 MB), every one re-created by macOS's LZFSE.
Packing the disk image the UDZO holds instead of the DMG, at Max: 86.8 → 55.8 MB.

The Deflate transform now takes UDIF images (`DeflateKind::Dmg`, a `.dmg` name
with a `koly` trailer): the trailer's XML lists the `mish` tables, each zlib
chunk is a candidate stream, and zlib replay (or preflate) re-creates it; bytes
outside zlib chunks stay literal. On the UDZO image, round trips identical:

| Level | Before | Now | Make | Extract |
|---|---:|---:|---:|---:|
| Default | 93.96 MB | **74.62 MB (−21%)** | 2.8 s | 0.49 s |
| Max | 86.80 MB | **60.68 MB (−30%)** | 12.0 s | 0.97 s |

(Max stays above the 55.8 MB of the bare image: this image's data fork also
holds 11.7 MB of raw app bytes that no chunk references.) Limits: a file's
expanded data is capped at 512 MiB (`MAX_PLAIN_TOTAL`), so only the first 512 MiB
of a big image is expanded; LZMA (ULMO) and bzip2 (UDBZ) chunks stay as they are.

LZFSE chunks (ULFO) are expanded too, with the pinned lzfse (next section). The
ULFO image of the apps set (97.1 MB), round trips identical:

| Level | Before | Now | Make | Extract |
|---|---:|---:|---:|---:|
| Default | 87.84 MB | **75.09 MB (−14.5%)** | 2.8 s | 0.41 s |
| Max | 86.96 MB | **59.25 MB (−31.9%)** | 13.2 s | 0.92 s |

**What expanding DMGs costs (user's question, 2026-10-29).** The dmgs set with
the transforms (on) and with `--no-transforms` (off: the DMG bytes compressed as
they are, which is what other archivers do); busy Mac, two alternating runs each,
round trips identical:

| Level | On | Off |
|---|---|---|
| Default | 557.0 MB, 7.6-7.7 s, extract 1.3 s | 619.7 MB, 0.74-0.77 s, extract 0.22 s |
| Max | 450.6 MB, 50.4-51.2 s, extract 4.5-4.7 s | 610.6 MB, 11.3 s, extract 1.2 s |

Expanding costs ~10x the time at Default and ~4.5x at Max, for 10% and 26%
smaller. The time goes into compressing the expanded data (2-3x the DMG bytes),
not into re-creating the chunks. Off, compressing already-compressed bytes saves
almost nothing (−0.1% / −1.5%), so no archiver that stores DMGs as they are can
get near the "on" sizes.

**A switch per level (user, 2026-10-29).** DMG expansion is now an option of
its own (`TransformOpts::disk_images`; CLI `--disk-images on|off`): on at Max
and Ultra, off at Fast and Default, and any level can turn it either way. When
on, the level 6-9 probe runs at every level. The checkpoint measures Default
both ways on the dmgs set: the goal is to beat every rival at every level.

### zlib levels 8-9: an exact, faster match search (in the engine, 2026-10-29)

The real-world sets brought app installers made with `hdiutil -imagekey
zlib-level=9`: all 485 chunks of Discord's DMG (208.8 MB of 500.5 MB), and
Figma's (levels 7-9). zlib replay stopped at level 5 because zlib -9 is slow
(6-17 MB/s) and preflate rebuilt such streams faster. At Default preflate's
short search refuses them after a full analysis (12 s of CPU on Discord's DMG
for nothing); at Max it takes 437 of the 483 MiB.

Where zlib -9's time goes (122 MB of Discord's chunks, counters in a scratch
copy): 41 M searches, 2.54 G hash chain steps (61.6 per search), 81% of them real
3-byte matches that can't beat the current best; the walk is 63% of the time
(a 16-step limit runs in 2.65 s instead of 7.2 s); only 0.18% of searches hit
the 4096-step cut-off.

At levels 8-9 (`nice_match` 258) the walk returns the newest string with the
longest match among the first `max_chain` strings of its hash bucket. The pinned
zlib now finds that directly (`fast_match`, see `engine/vendor/zlib/ARCA-NOTES.md`):
chains of strings sharing 8 and 4 bytes, plus per-bucket counts that place any
string in zlib's walk, so the cut-off is honoured exactly. 16 steps per search
instead of 62, and **the same output** on everything tested (crate test; 145 MB
of real and adversarial data at nine settings; Discord's chunks). Speed, one
core:

| Data | Stock zlib | Fast search |
|---|---:|---:|
| 145 MB corpus, level 9 | 28.3 s | **5.0 s (5.7x)** |
| same, level 8 | 14.2 s | **3.7 s (3.8x)** |
| Discord's chunks (122 MB), level 9 | 7.8 s | **3.9 s (2.0x)** |
| levels 1, 6, 7 | unchanged (the original code runs) | |

A disk image's chunks share one setting: the first level 6-9 chunk is tried
with every setting its header allows, and the rest with the one that matched.
Discord's DMG (busy Mac, two alternating runs each, identical round trips):

| Level | Before | Fast zlib re-run | Make | Extract |
|---|---:|---:|---:|---:|
| Default | 214.17 MB | 175.37 MB (−18.1%) | 2.0 → 8.5 s | 0.33 → 2.3 s |
| Max | 152.73 MB | **145.03 MB (−5.0%)** | 32.2-32.8 → **30.0 s** | 3.6-3.7 → **3.1 s** |

(With stock zlib's search the same re-run cost 12.9 s / 4.4 s at Default and
33.4 s / 4.9 s at Max: slower than preflate. Hybrids measured first: preflate
then zlib for what it refuses, 145.6 MB / 35 s / 5.0 s at Max; skipping the
refused analysis at Default, 3.5x faster but 0.45% bigger.)

**In the engine: Max and Ultra.** At Max it is smaller and faster both ways. At
Default it is a trade (18% smaller, 4x the time to make, 7x to extract), so it
stays off there (now with all DMG expansion at Default; `--disk-images on`
turns it on).
The dmgs set, round trips identical:

| Level | Before | Now | Make | Extract |
|---|---:|---:|---:|---:|
| Max | 457.86 MB | **450.56 MB (−1.6%)** | 54.9 → **51.0 s** | 5.80 → **4.69 s** |
| Ultra | 455.42 MB | **447.89 MB (−1.7%)** | 65.4 → **60.1 s** | 5.89 → **4.56 s** |

**Level 8-9 PNGs: not adopted.** Re-running them as well (saved folder, two
alternating runs): Default 257.40 → 256.76 MB (−0.25%) for 9.0-9.4 → 16.5-18.2 s
to make and 2.2 → 4.7-4.8 s to extract; Max 221.96 → 219.66 MB (−1.0%) for
30.1-32.9 → 36.8-42.1 s and 2.7-3.0 → 4.9-5.3 s. Their pixels are flat images,
~30x their Deflate bytes, so every byte saved costs a lot of re-running; preflate
keeps them at Max as before and Default still skips them.

### LZFSE replay: macOS's encoder is lzfse 1.0 with one setting changed

With the user's OK, lzfse 1.0 (Apple's open-source reference, BSD-3, the 2017
tag; SHA-256 matches Homebrew's) is vendored in `engine/vendor/lzfse`. Out of the
box it does not re-create what macOS writes today: 127 of the ULFO image's 193
chunks, 606 of the saved folder's 784 `Assets.car` streams. Comparing the token
streams (a decoder that lists every literal/match token) showed every difference
starts where macOS emits a short pending match in a long literal run that lzfse
1.0 keeps waiting on, then drops when a long match arrives. lzfse 1.0 emits a
pending match once 8 x 315 literals wait (`8 * LZFSE_ENCODE_MAX_L_VALUE`);
trying other values, **3 x 315 re-creates all 193 chunks** (1, 2 and 4: 162, 177
and 180; keeping partial literal runs at block ends, emitting pending matches
before long ones, and larger hash tables all made it worse). The vendored copy
carries that number in the encoder state (`literal_runs`, two lines); each stored
stream records the value that re-creates it (3, else 8).

Coverage with 3: the ULFO image 193 of 193 chunks (84.5 MB); the saved folder's
`Assets.car` 606 of 784 streams (10.16 of 12.08 MB); the installed apps' 239
`Assets.car` 20,861 of 29,023 streams (351 of 408 MB). The rest: LZVN streams for
inputs under 4 KiB (lzfse 1.0's LZVN encoder differs; 7,887 streams, 10.5 MB on
the installed apps) and 275 LZFSE streams (46.8 MB) from some other encoder
version. macOS's own libcompression re-creates 362 MB there.

**Assets.car: an option, not in the presets** (`ARCA_EXP_CAR=1`). Its decoded
renditions are flat images, ~17x the LZFSE bytes. Saved folder (busy Mac, round
trips identical):

| Level | Without | With | Make CPU | Extract |
|---|---:|---:|---:|---:|
| Default | 257.396 MB | 256.170 MB (−0.48%) | 55.9 → 60.8 s | 2.62 → 2.78 s |
| Max | 221.963 MB | 219.538 MB (−1.09%) | 211.6 → 231.6 s | 2.88 → 3.68 s |

The codec decides it. 64 MB of decoded renditions: bsc (LZP 18/128) 3.354 MB in
0.67 s (decode 0.75 s, 0.27 s threaded); LZMA fb 32 3.377 MB in 4.71 s (decode
0.15 s); zstd-12 4.31 MB, worse than keeping the LZFSE. With bsc blocks for this
data the gain would cost little; decide together with bsc at the checkpoint.

### Literature survey 2 (background agent, 2026-10-29)

Targeted at the bottlenecks above (report in the session scratchpad,
`litsearch2.md`; encode.su was unreachable, its numbers unverified):

- Machine code: no published method reaches LZMA's size 3x faster with fast
  decoding. Context mixing with exe models is 25-50% smaller but symmetric and
  ~100x slower (ooffice: 7z 2426 KB, zpaq m5 1756, paq8px 1205); fast-decoding LZ
  codecs are 2-8% larger (zstd -22 +7.4%, brotli -11 +3.6%).
- JPEG: Brunsli is the only coder faster than Lepton (one 1 MB image, Skylake:
  encode 0.16 vs 0.5 s, decode 0.13 vs 0.23 s) for ~1% less saving; no cheaper
  safe verification is published (Dropbox and Microsoft both decode everything).
- Deflate re-creation: nothing new; zlib replay is the known shortcut.
- Synthetic images: WebP lossless is 30-45% smaller than PNG on screenshots and
  icons and 11-34% smaller than JPEG XL effort 3 there (qoir benchmarks); its
  tools (palette, 2D LZ77 with a colour cache) are what the pixel codec lacks on
  flat images, where it is weakest (1.75x xz above 30:1).
- Photos (CLIC2021 against optimised PNG): QLIC2 −24.5% at ~137 MB/s both ways,
  HALIC −24% (closed source); kanzi -8/-9 ~15% smaller than bsc on mixed data at
  2.5-3.5 MB/s, an Ultra-only candidate.
- Archive level: ordering files by similarity (compsort: −6 to −11.5% with xz on
  firmware and initrds); delta of near-duplicate binaries (zstd --patch-from).

### Apple Silicon GPU (agenda item 4): no stage worth moving

Measured CPU by stage (saved folder, thread seconds):

| Work | Max make (~210 s) | Max extract (~25 s) | Default make (~62 s) | Fast make (~3.4 s) |
|---|---:|---:|---:|---:|
| LZMA / zstd blocks (adaptive range coding, match search) | ~75% | 13% | ~38% | ~75% |
| Lepton (adaptive binary arithmetic coding per coefficient) | ~10% | 41% | ~33% | — |
| preflate (replays a Deflate encoder byte by byte) | ~8% | 18% | ~20% | — |
| pixel codec (adaptive rANS; prediction is the cheap part) | ~4% | 10% | — | — |
| hashing, chunking, sniffing (data-parallel) | ~2% | — | ~5% | ~35% |
| writing files (kernel, serialised by APFS) | — | ~12% | — | — |

Everything heavy is a serial adaptive model: each symbol's probabilities depend
on the one before, which is what makes these coders small, and is exactly what a
GPU cannot spread out. Published GPU codecs (DietGPU, GPULZ, Recoil; all CUDA, no
Metal) get their speed from static tables and independent tiles, at a cost in
size the brief rules out. The data-parallel part is small: BLAKE3 with NEON on
the CPU cores already hashes the 660 MB in well under 0.1 s, and Fast is bound by
file I/O and serial phases, not hashing. A base M1's 7–8 GPU cores share the CPU's
memory bandwidth, so moving that ~2–5% would add transfers and launch latency for
no wall-clock gain. Revisit only if a GPU-friendly coder (static-table rANS over
independent tiles, or a GPU suffix sort for BWT) is ever adopted for size reasons.

### Long-range pre-pass before LZMA (dead end on size)

Goal: take the code lane's repetition (static libraries, 46:1) off LZMA fb 273,
which spends ~31 s on one 64 MiB block. Prototype `examples/lrp.rs`: gear rolling
hash over 64 bytes, content-defined anchors (~1 in 32), a 4M-slot table, matches
verified and extended both ways, kept when at least MIN bytes; references stored
as three columns (literal-run lengths, match lengths, distances with 0 = repeat),
each LZMA-coded; the literals LZMA fb 273 coded. Round trip checked.

Saved folder's first code block (67.1 MB), one thread, CPU:

| | Size | CPU |
|---|---:|---:|
| LZMA fb 273 alone | 1.391 MB | 33.7 s |
| pre-pass, MIN 256 (95,729 refs cover 52.8%) | 1.276 + refs 0.295 = **1.571 MB** | **17.2 s** |
| pre-pass, MIN 1024 | 1.398 MB | 31.9 s |
| pre-pass, MIN 4096 | 1.397 MB | 32.2 s |
| pre-pass, MIN 16384 | 1.394 MB | 32.9 s |

Twice as fast at MIN 256, but 13% bigger: LZMA codes the same matches in ~1.2
bytes each (repeat distances, context-coded lengths), the reference columns in
~3.1. Even free references would only reach 1.276 MB, and the repeats are mostly
256–1,024 bytes (whole duplicate objects are already removed by chunk dedup), so
longer minimums cover 1–2%. Fails "at least as small"; FreeArc's REP (512-byte
minimum) has the same trade-off.

### Literature survey (background agent, 2026-10-28)

Ranked leads from 2024–26 papers and projects; each still needs a test here:

1. **BWT (libbsc + libsais, Apache-2.0) instead of PPMd for text.** LTCB enwik9:
   bsc 163.9 MB at 23 ns/B compress, 8 ns/B decompress; 7-Zip PPMd 179.0 MB at
   ~500 ns/B both ways. Blocks decode in parallel: the route to Ultra extracting
   on all cores. Needs the source (download: ask).
2. **Deflate replay before preflate**: when a stream is exactly zlib (level,
   memLevel, window), store those bytes and re-run the encoder. Preflate's author
   calls preflate "quite slow"; brute-force zlib matching (precomp, reflate) is
   50–500% faster on small streams. First check which encoders made our PNGs.
3. **Fast LZMA2** radix match finder (BSD-3): 20–100% faster, 1–5% larger. Known.
4. **LZRAVEN/LZMESH**: tested above (dead end, needs macOS 27).
5. **Similarity grouping** of near-duplicate files into the same block (compsort:
   −11.5% on an initrd with xz). MinHash over the CDC chunk hashes already computed.
6. **Mach-O**: code-signature page hashes (SHA-256 per 4 KB page, 0.8–1.3% of a
   binary, incompressible) can be recomputed at extraction and checked at pack
   time; per-slice filters for universal binaries; lc/lp for 4-byte code.
7. **Pixel codec**: JPEG XL's gain from e2 to e7 is learned context trees and
   per-channel predictor choice; a small tree keeps decoding fast.
8. **JPEG**: nothing beats Lepton (2023: Lepton 42 ms encode, 22 ms decode, 19.5%;
   JPEG XL transcode 53 / 68 ms, 14.3%). Our encode+verify is twice the decode
   because `encode_lepton_verify` decodes everything again.
9. **OpenZL** (Meta, BSD): format-aware, fits CSV/columnar data; format unstable.
10. **GPU**: no Metal compression code exists; published GPU codecs are CUDA
    (DietGPU rANS, GPULZ, Recoil). Arca's hot loops are serial context models;
    only tile-parallel pixel residuals or BWT suffix sorting could map to a GPU.
11. Approximate LZ77 via string synchronizing sets (arXiv 2609.30193): 12–19x
    faster parsing, no code published.
12. Extraction: `clonefile()` for duplicate files on APFS.

## Engine ZIP reader and writer

**Writer in the app** (2026-10-28 late evening). The engine's writer against the
7-Zip settings the app uses, saved folder:

| ZIP | Size | Time |
|---|---:|---:|
| engine, Deflate level 6 | 352.0 MB | 1.20-1.38 s |
| **engine, level 7** | **351.56 MB** | **1.39-1.42 s** |
| engine, level 8 | 351.11 MB | 1.75-1.78 s |
| engine, level 9 | 347.8 MB | 2.35 s |
| 7-Zip -mx1 (app's Fast and Default) | 351.80 MB | 1.71-1.74 s |
| 7-Zip -mx7 (app's Max) | 344.3 MB | 14.1 s |
| 7-Zip -mx9 (app's Ultra) | 343.6 MB | 35.4 s |

Files of 4 MiB and up are now compressed in 1 MiB pieces on several cores, as
pigz does (each primed with the 32 KiB before it, joined by sync flushes into
one ordinary Deflate stream): level 7 went from 1.80 to 1.4 s at the same size
(the 49 MB Maps binary had been compressing on one core). Checking afterwards:
the engine's reader takes 0.22 s against 1.86 s for 7-Zip's test. So the app
now makes Fast and Default ZIPs without a password or split volumes with the
engine at level 7 (3.6 → 1.6 s including the check, slightly smaller), and
checks every password-less ZIP with the engine's reader first, falling back to
7-Zip's test if it declines or finds damage. Max and Ultra ZIPs stay with 7-Zip,
whose optimal-parsing Deflate is 1% smaller. Two FFI functions were added
(`arca_zip_create`, `arca_zip_verify`); an engine test reads a written ZIP back
with Arca's reader and with ditto.

The app also needs `-lc++` now: libbsc is C++ (the app build broke at the bsc
commit; fixed in the same change).

**Reader** (`arca-core/src/zip.rs`, CLI `arca unzip` / `arca ziptest`): parses the
central directory (ZIP64, extended-timestamp and Info-ZIP Unix time fields),
inflates stored and Deflate entries on every core with zlib-rs (flate2's
`zlib-rs` backend, already in the local registry), checks every CRC-32, streams
entries over 64 MiB piece by piece, decodes `._` AppleDouble twins in memory
(ported from the app's Swift parser: no quarantine, no ACLs, Finder's hidden bit
becomes `UF_HIDDEN`) and applies them through each file's descriptor, creates
links last and leaves out any that point outside, and can set the archive's own
quarantine on everything. Anything else (encryption, methods other than store
and Deflate, names that aren't UTF-8, backslashes, duplicate names, split
archives) fails with "unsupported: ...", for the app to fall back to 7-Zip.

On the saved Mac-made ZIP (`ditto-mixed.zip`: 30,682 entries, 632 MiB):

| | Time |
|---|---:|
| check every CRC, nothing written (`ziptest`) | 0.36 s |
| **extract** (`unzip`) | **1.06–1.12 s** |
| `ditto -x -k` (Finder's engine) | 11.8 s |
| the app through 7-Zip, metadata in memory (main, 2026-10-28) | 4.7 s |
| Zipgeist | 2.1 s |

Compared item by item with ditto's extraction (contents, mode, mtime, BSD flags,
extended attributes): 0 differences over 14,506 items. The saved ZIP's twins only
carry `com.apple.provenance`, so a second test folder has a Finder colour label,
tags, a resource fork, an attribute on a folder, a hidden file, an executable, an
80 MB file and two links: identical to ditto except that the escaping link is
left out (intended) and the other link keeps its archived date (ditto doesn't
restore link dates). That case is now `arca-core/tests/zip.rs`, with a damaged
archive and an encrypted one (refused as unsupported).

**In the app:** a tenth FFI function, `arca_zip_extract` (the other nine are
unchanged), and `Engine.extractZip` in the Swift wrapper. `Archives.extract` tries
it for password-less ZIPs and falls back to the existing 7-Zip path when the engine
answers "unsupported:"; the safety pass afterwards is unchanged. The app's test
`hostileZipStaysInside` showed that escaping entries must be left out and the rest
extracted (as 7-Zip does), not the whole archive refused: fixed, reported in
`skipped`, and covered by a new engine test. The inspector labels `lzma2` blocks
"LZMA2" like `xz` ones. **All 58 app tests pass**; the test build was unregistered
with `lsregister -u`.

## Dead ends

- Long-range pre-pass before LZMA (2x faster on code, 13% bigger): see Deep research.
- Multi-symbol size classes inside Lepton: +0.63% for ~25% less coder work.
- One LZMA2 block encoded in parallel segments (state reset, no dictionary
  reset; each segment's match finder first indexes the 24-32 MiB before it; a
  shim including LzmaEnc.c, tried and removed). A 32 MiB window alone costs only
  +0.03-0.08% on Mach-O (16 MiB: +3.5-17%, universal binaries repeat 16-32 MiB
  apart), but indexing the preload costs as much as coding it: 64 MiB Mach-O
  block, 16 MiB segments, 32 MiB window: 10.538 MB, 42.7 s CPU, 12.3 s wall,
  against 10.536 MB, 17.3 s CPU, 12.2 s wall with the two-thread match finder.
- Hash-chain match finders on Mach-O (fb 32, one thread): hc4 10.800 MB / 18.7 s,
  hc5 11.005 MB / 18.4 s against bt4 10.529 MB / 22.6 s: +2.6-4.5% for 17-19% less CPU.
- Fewer LZMA search cycles on Mach-O (fb 32, one thread; default 32): 24 cycles
  +0.28% for 2.4% less CPU, 16 +0.85% for 10%, 8 +3.0% for 29%.
- ARM64 code in byte planes (57.6 MB of the apps' `__text`): LZMA fb 273 13.15 →
  17.79 MB, bsc 17.26 → 22.03 MB. LZMA stays best on raw machine code.
- Code-signature page hashes: only 0.19 MB in 99.5 MB of app binaries (16 KB
  pages on current macOS), ~0.4% of the compressed apps set. Kept as a small
  idea, not a lever.
- Mach-O sections to bsc: the non-code 38 MB of the apps' binaries is 46% bigger
  with bsc than LZMA, the strings alone tie (1.426 vs 1.423 MB); code and the
  rest as two LZMA streams gain 0.13%. See Deep research.
- GPU: every heavy stage is a serial adaptive coder; the data-parallel 2–5%
  is not worth moving (see Deep research).
- Hint-guided LZMA (the pre-pass's repeats taken as single matches): +1.3–10%
  for 17–31% less CPU; other match finders: +3–4% for ≤18%. See Deep research.
- Decoding image bands side by side on one core, and branchless top-bit decoding:
  the pixel decoder is bound by instruction count (see Default).
- LZRAVEN (needs macOS 27), Apple LZMA, QoS classes, narrowing preflate
  candidates, 32 MiB binary blocks: see their sections.
