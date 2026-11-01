#!/usr/bin/env python3
"""Scores the Default lane candidates (defaultlanes.py) against the phase 3 rival
rows of Default's class: 7-Zip Normal, Zipgeist 7z Normal, Zipgeist ZIP Normal,
WinRAR Normal and xz -6, as the app benchmark of 31 October measured them. A row
is won when the candidate is smaller, faster to make and faster to extract at once.

    python3 -I bench/research/defaultlanes_report.py <results.jsonl ...>
"""

import os
import sys
from collections import defaultdict

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
from checkpoint_report import NAMES, SET_INFO, load  # noqa: E402

R = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..") + "/"
APPS, ZX, ZG, FI = (R + "results-2026-10-31-apps" + x + ".jsonl" for x in ("", "-zx", "-zipgeist", "-finder"))
SECOND_PASS = {"zstd-3", "zstd-19-long", "xz-6", "xz-9e"}
NORMAL = ["7z-normal", "zg-7z-normal", "xz-6", "zg-zip-normal", "rar-normal"]
NAMES = dict(NAMES, **{"finder-app": "Finder", "zg-7z-normal": "Zipgeist 7z Normal", "zg-zip-normal": "Zipgeist ZIP Normal"})
SHORT_RIVAL = {"7z-normal": "7z-N", "zg-7z-normal": "ZG7z-N", "xz-6": "xz-6", "zg-zip-normal": "ZGzip-N", "rar-normal": "RAR-N"}


def rivals():
    rows = {k: v for k, v in load(APPS).items() if k[1] not in SECOND_PASS and not k[1].startswith("arca")}
    for f in (ZX, ZG, FI):
        if os.path.exists(f):
            rows.update({k: v for k, v in load(f).items() if not k[1].startswith("arca")})
    return rows


def beats(a, b):
    return a["size"] < b["size"] and a["make_s"] < b["make_s"] and a["extract_s"] < b["extract_s"]


def lost_on(a, b):
    return "+".join(m for m, k in (("size", "size"), ("make", "make_s"), ("extract", "extract_s")) if a[k] >= b[k])


def mb(n):
    return n / 1e6


def main():
    files = sys.argv[1:] or sys.exit(__doc__)
    ours = {}
    for f in files:
        for k, v in load(f).items():
            if "size" in v:
                old = ours.get(k)
                if old is None:
                    ours[k] = v
                else:
                    for m in ("make_s", "extract_s"):
                        old[m] = min(old[m], v[m])
                    for m in ("make_mb", "extract_mb"):
                        if v.get(m):
                            old[m] = max(old.get(m) or 0, v[m])
    them = rivals()
    sets = [s for s in SET_INFO if any(k[0] == s for k in ours)]
    cands = []
    for k in ours:
        c = k[1].replace("arca-balanced-", "")
        if c not in cands:
            cands.append(c)
    cands.sort(key=lambda c: (c != "base", c))
    totals = defaultdict(lambda: [0, 0])
    for s in sets:
        base = ours.get((s, "arca-balanced-base"))
        print(f"== {s}: {SET_INFO[s]}")
        riv = {rv: them[(s, rv)] for rv in NORMAL if (s, rv) in them}
        print("   rivals: " + "; ".join(f"{SHORT_RIVAL[rv]} {mb(b['size']):.2f} MB {b['make_s']:.2f}/{b['extract_s']:.2f} s" for rv, b in riv.items()))
        print(f"   {'cand':6s} {'size MB':>8s} {'vs base':>8s} {'make s':>7s} {'x':>5s} {'extr s':>7s} {'x':>5s} {'peak MB':>7s}  won  lost on")
        for c in cands:
            a = ours.get((s, f"arca-balanced-{c}"))
            if a is None:
                continue
            won = [rv for rv, b in riv.items() if beats(a, b)]
            lost = [f"{SHORT_RIVAL[rv]}:{lost_on(a, b)}" for rv, b in riv.items() if not beats(a, b)]
            totals[c][0] += len(won)
            totals[c][1] += len(riv)
            d = f"{100 * (a['size'] / base['size'] - 1):+.1f}%" if base else ""
            mk = f"{a['make_s'] / base['make_s']:.2f}" if base else ""
            ex = f"{a['extract_s'] / base['extract_s']:.2f}" if base else ""
            flag = "" if a.get("identical", True) else "  MISMATCH"
            print(f"   {c:6s} {mb(a['size']):8.2f} {d:>8s} {a['make_s']:7.2f} {mk:>5s} {a['extract_s']:7.2f} {ex:>5s} {a.get('make_mb') or 0:7.0f}  {len(won)}/{len(riv)}  {' '.join(lost)}{flag}")
        if s == "mixed" and (s, "7z-ultra") in them:
            u = them[(s, "7z-ultra")]
            fi = them.get((s, "finder-app"))
            print("   headline against 7-Zip Ultra (size, make x, extract x)" + (" and Finder (extract x)" if fi else ""))
            for c in cands:
                a = ours.get((s, f"arca-balanced-{c}"))
                if a is None:
                    continue
                line = f"   {c:6s} {100 * (1 - a['size'] / u['size']):5.1f}% smaller, {u['make_s'] / a['make_s']:4.1f}x, {u['extract_s'] / a['extract_s']:4.1f}x"
                if fi:
                    line += f", Finder {fi['extract_s'] / a['extract_s']:4.1f}x"
                print(line)
        print()
    print("Normal-class rows won over the sets above:")
    for c in cands:
        w, n = totals[c]
        print(f"   {c:6s} {w} of {n}")


if __name__ == "__main__":
    main()
