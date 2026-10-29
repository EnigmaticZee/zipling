# zlib in Arca

The Deflate encoder sources of zlib 1.3.2 by Jean-loup Gailly and Mark Adler
(zlib license, `zlib/LICENSE`), downloaded with the user's OK on 2026-10-29 from
`https://github.com/madler/zlib/releases/download/v1.3.2/zlib-1.3.2.tar.gz` and
`https://zlib.net/zlib-1.3.2.tar.gz` (both SHA-256
`bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16`). One change,
marked "Arca" in `deflate.c` and `deflate.h`: a faster match search for levels 8
and 9 that finds exactly the matches zlib's own search finds (below).

Why a pinned copy: most PNGs made on a Mac are written by stock zlib (ImageIO
uses level 1). Re-running the same encoder re-creates their Deflate streams
byte for byte, faster than preflate's analysis. The output must never change, so
Arca can't use the system's libz (an OS update could change it) or zlib-ng/zlib-rs
(different output). zlib 1.3.2 reproduces exactly the streams macOS's libz 1.2.12
writes (all 656 PNGs of the saved folder tested, 480 of them stock zlib).

Only the encoder is built (`deflate.c`, `trees.c`, `zutil.c` with its header
`gzguts.h`, and `adler32.c` and
`crc32.c`, which `deflate.c` links against), with `Z_PREFIX` so its symbols
(`z_deflate`, ...) can't clash with another zlib in the app. Inflating is done by
flate2 (zlib-rs).

`src/arca_zlib.c` and `src/lib.rs` are Arca's: re-deflate with fixed settings,
comparing with a target as it goes.

## The faster search (levels 8 and 9)

zlib's `longest_match` walks the hash chain of the current string (a 15-bit hash
of 3 bytes) from the newest entry, up to `max_chain_length` entries (4096 at
level 9, 1024 at level 8, a quarter of that once the previous match reached
`good_match`), and keeps the newest string with the longest match; at levels 8
and 9 `nice_match` is 258, the longest match there is, so nothing else stops the
walk early. On app disk images that is 62 chain entries per search, 81% of them
real 3-byte matches that can't beat the current best.

`fast_match` gives the same answer from three smaller sets: strings sharing the
first 8 bytes (their own chain), else the first 4, else 3 (the newest string on
zlib's chain whose first two bytes match: same hash and two equal bytes means
three). Strings join the 4- and 8-byte chains in order, the same strings
`deflate_slow` adds to the hash chain. To honour zlib's chain cut-off exactly,
each bucket counts the strings added to it (`cnt3`) and each string records the
count when it was added (`ord3`): a string's place in zlib's walk is the
difference, and anything past `chain` places is skipped as zlib's walk would
skip it. The last 266 bytes of the input, and a chain head right at the window's
edge, use zlib's own walk. On by itself (`arca_fast`) for levels 8 and 9 with
windows of 4 KiB or more; levels 1-7 run the original code, and so does the rest
of a stream once its history is cleared another way or its settings change (a
full flush, a preset dictionary, `deflateParams`, `deflateTune`; Arca uses
none of them).

Same output, tested: `fast_search_writes_what_zlib_writes` (levels 8-9, four
window and memory settings, data built to hit the chain cut-off); 145 MB of real
files (PNG rows, source, Mach-O, JPEG) and synthetic data at nine settings
(levels 1, 6-9; windows 11-15 bits; memory levels 3, 8, 9; both strategies);
Discord's level-9 disk image chunk for chunk. Speed: 3.4-5.7x on that 145 MB at
levels 8-9, 2x on the disk image (7.8 → 3.9 s for 122 MB). Even if the two
searches ever differed, nothing would break: every stream Arca re-runs is
compared with the original bytes when packing, and falls back to preflate.

To update: never, unless a test shows the new release writes byte-identical
output for every setting Arca stores; archives depend on it.
