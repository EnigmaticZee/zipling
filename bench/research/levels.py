#!/usr/bin/env python3
"""Each Arca level against the rival settings in its own class, 1:1, on every set:
Fast against the fast settings, Default against the normal ones, Max against the
best, Ultra as our extra against the best. A row is won when Arca is smaller,
faster to make and faster to extract at once. Then the upsets: where a lower
Arca level beats a rival of a higher class on all three.

    python3 -I bench/research/levels.py <arca results.jsonl> [rival results.jsonl ...]

Without rival files, the quiet runs of 2026-10-30 (quiet-all, quiet-rar,
zipgeist) supply the rivals. Arca rows come only from the files given first;
rival rows from the rival files (an Arca file's rival rows count too).
"""

import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
from checkpoint_report import NAMES, SET_INFO, SHORT, load  # noqa: E402

HERE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
RIVALS_DEFAULT = ["results-2026-10-30-quiet-all.jsonl", "results-2026-10-30-quiet-rar.jsonl", "results-2026-10-30-zipgeist.jsonl"]
CLASSES = {
    "arca-fast": ["zstd-3", "zg-tzst-fast", "zg-zip-fast", "rar-fastest", "finder-zip"],
    "arca-balanced": ["7z-normal", "zg-7z-normal", "zg-zip-normal", "rar-normal", "xz-6"],
    "arca-max": ["7z-ultra", "zstd-19-long", "xz-9e", "rar-best", "zg-7z-best", "zg-txz-best"],
}
CLASSES["arca-ultra"] = CLASSES["arca-max"]
LEVELS = ["arca-fast", "arca-balanced", "arca-max", "arca-ultra"]
RANK = {"arca-fast": 0, "arca-balanced": 1, "arca-max": 2, "arca-ultra": 2}


def merge(into, more, arca):
    for k, b in more.items():
        if "size" not in b or k[1].startswith("arca") != arca:
            continue
        old = into.get(k)
        if old is None:
            into[k] = b
        else:
            for m in ("make_s", "extract_s"):
                if b.get(m) is not None and (old.get(m) is None or b[m] < old[m]):
                    old[m] = b[m]


args = sys.argv[1:]
if not args:
    sys.exit(__doc__)
arca_files = [a for a in args if "rival" not in a]
rival_files = [os.path.join(HERE, f) for f in RIVALS_DEFAULT]
ours, theirs = {}, {}
for f in arca_files:
    merge(ours, load(f), arca=True)
    merge(theirs, load(f), arca=False)
for f in rival_files:
    merge(theirs, load(f), arca=False)


def ratio_line(a, b):
    return f"{a['size'] / b['size']:5.2f}  {a['make_s'] / b['make_s']:6.2f}  {a['extract_s'] / b['extract_s']:6.2f}"


def beats(a, b):
    return a["size"] < b["size"] and a["make_s"] < b["make_s"] and a["extract_s"] < b["extract_s"]


print("1:1 by level (ratios Arca / rival: size, make, extract; under 1 is a win)\n")
total = {lv: [0, 0] for lv in LEVELS}
losses = []
for s in SET_INFO:
    if not any(k[0] == s for k in ours):
        continue
    print(f"== {s}: {SET_INFO[s]}")
    for lv in LEVELS:
        a = ours.get((s, lv))
        if a is None:
            continue
        for rv in CLASSES[lv]:
            b = theirs.get((s, rv))
            if b is None:
                continue
            won = beats(a, b)
            total[lv][0] += won
            total[lv][1] += 1
            mark = "won " if won else "LOST"
            why = "" if won else " on " + ", ".join(m for m, key in (("size", "size"), ("make", "make_s"), ("extract", "extract_s")) if a[key] >= b[key])
            print(f"  {SHORT[lv]:8s} vs {NAMES[rv]:26s} {ratio_line(a, b)}  {mark}{why}")
            if not won:
                losses.append((s, lv, rv, why))
    print()
print("Rows won, 1:1 by level:")
for lv in LEVELS:
    w, n = total[lv]
    print(f"  {SHORT[lv]:8s} {w} of {n}")
print(f"  all      {sum(t[0] for t in total.values())} of {sum(t[1] for t in total.values())}")
if losses:
    print("\nRows lost:")
    for s, lv, rv, why in losses:
        covered = [SHORT[o] for o in ("arca-max-lzt", "arca-balanced", "arca-max", "arca-ultra", "arca-fast") if (s, o) in ours and o != lv and beats(ours[(s, o)], theirs[(s, rv)])]
        print(f"  {s}: {SHORT[lv]} vs {NAMES[rv]}{why}" + (f"  (beaten by {', '.join(covered)})" if covered else "  (no Arca setting beats it)"))

print("\nUpsets: a lower Arca level beating a rival of a higher class on all three")
for s in SET_INFO:
    for lv in ("arca-fast", "arca-balanced"):
        a = ours.get((s, lv))
        if a is None:
            continue
        for hi in LEVELS:
            if RANK[hi] <= RANK[lv] or hi == "arca-ultra":
                continue
            for rv in CLASSES[hi]:
                b = theirs.get((s, rv))
                if b is not None and beats(a, b):
                    print(f"  {s:11s} {SHORT[lv]:8s} beats {NAMES[rv]:26s} {100 * (1 - a['size'] / b['size']):5.1f}% smaller, {b['make_s'] / a['make_s']:5.1f}x faster make, {b['extract_s'] / a['extract_s']:5.1f}x faster extract")
