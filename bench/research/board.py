#!/usr/bin/env python3
"""The checkpoint's board with a newer run laid over it.

    python3 -I bench/research/board.py <newer.jsonl>

Rows in the newer run replace the checkpoint's. Rivals it didn't run keep their
checkpoint times, scaled per set by how the two runs' shared rivals compare, but
never made slower: the scale is capped at 1 so a slow day doesn't flatter Arca.
Arca settings it didn't run keep their checkpoint times, which is conservative,
except Fast + bsc on the sets with photos: the checkpoint's Fast had no JPEG
coder, so those rows show speeds today's engine can't reach, and are dropped.
"""

import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
from checkpoint_report import CHARTS, NAMES, SET_INFO, SHORT, gmean, load  # noqa: E402

HERE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
base = os.path.join(HERE, "results-2026-10-29-")
best = load(base + "checkpoint.jsonl")
for extra in ("zipgeist", "checkpoint-rar", "checkpoint-7z"):
    more = load(base + extra + ".jsonl")
    scale = {}
    for s in SET_INFO:
        pairs = [(b, best[k]) for k, b in more.items() if k[0] == s and k[1].startswith("arca") and k in best
                 and b.get("make_s") and best[k].get("make_s")]
        if pairs:
            scale[s] = {m: gmean([b[m] / a[m] for b, a in pairs]) for m in ("make_s", "extract_s")}
    for k, b in more.items():
        if k[1].startswith("arca") or (k in best and "size" in best[k]):
            continue
        if k[0] in scale and "size" in b:
            for m in ("make_s", "extract_s"):
                b[m] /= scale[k[0]][m]
            b["pass"] = extra
        best[k] = b

new = load(sys.argv[1])
clock = {}
for s in SET_INFO:
    pairs = [(b, best[k]) for k, b in new.items() if k[0] == s and not k[1].startswith("arca") and k in best and "size" in b]
    clock[s] = {m: gmean([b[m] / a[m] for b, a in pairs]) for m in ("make_s", "extract_s")} if pairs else {"make_s": 1, "extract_s": 1}
print("Clock, newer run against the checkpoint, shared rivals (make / extract):")
for s, c in clock.items():
    print(f"  {s:<11} {c['make_s']:.2f} / {c['extract_s']:.2f}")
for k, b in best.items():
    if k not in new and not k[1].startswith("arca") and "size" in b:
        for m in ("make_s", "extract_s"):
            b[m] *= min(1, clock[k[0]][m])
best.update(new)
for s in ("jpeg", "saved", "mixed"):
    if (s, "arca-fast-bsc") not in new:
        best.pop((s, "arca-fast-bsc"), None)

order = list(NAMES)
sets = [s for s in SET_INFO if any(k[0] == s for k in best)]
arca = lambda s: sorted((b for (st, t), b in best.items() if st == s and t.startswith("arca") and "size" in b), key=lambda b: order.index(b["tool"]))
beaten = total = 0
misses = []
for t in order:
    if t.startswith("arca"):
        continue
    for s in sets:
        R = best.get((s, t))
        if not R or "size" not in R:
            continue
        total += 1
        win = [b for b in arca(s) if b["size"] <= R["size"] and b["make_s"] < R["make_s"] and b["extract_s"] < R["extract_s"]]
        if win:
            beaten += 1
            continue
        smaller = [b for b in arca(s) if b["size"] <= R["size"]]
        if smaller:
            b = min(smaller, key=lambda b: max(b["make_s"] / R["make_s"], b["extract_s"] / R["extract_s"]))
            slow = [f"{w} x{b[m] / R[m]:.2f}" for m, w in (("make_s", "make"), ("extract_s", "extract")) if b[m] >= R[m]]
            misses.append((s, NAMES[t], f"{SHORT[b['tool']]}: " + ", ".join(slow)))
        else:
            misses.append((s, NAMES[t], "none as small"))
print(f"\nBeaten on size, make and extract at once: {beaten} of {total}\n")
for s in sets:
    ms = [m for m in misses if m[0] == s]
    print(f"{s}: {len(ms)} missed")
    for _, n, why in ms:
        print(f"   {n:<28} {why}")

print("\nFast against each rival in the fast class:")
FAST = ["zstd-3", "zg-tzst-fast", "zg-zip-fast", "zg-zip-normal", "rar-fastest", "finder-zip"]
for s in sets:
    A = best[(s, "arca-fast")]
    print(f"{s}: Fast {A['size'] / 1e6:.2f} MB, {A['make_s']:.2f} / {A['extract_s']:.2f}")
    for t in FAST:
        R = best.get((s, t))
        if R and "size" in R:
            marks = "".join("Y" if c else "n" for c in (A["size"] <= R["size"], A["make_s"] < R["make_s"], A["extract_s"] < R["extract_s"]))
            print(f"   {marks} {NAMES[t]:<24} {R['size'] / 1e6:8.2f} MB {R['make_s']:6.2f} / {R['extract_s']:5.2f}")
