#!/usr/bin/env python3
"""Turns checkpoint.py's results into the checkpoint report (Markdown).

    python3 -I bench/checkpoint_report.py <main.jsonl> [extra pass.jsonl ...] > report.md

Times are the better of the rounds; peak memory the higher. Arca's rows come
from the main run. Extra passes (Zipgeist through Finder, WinRAR, re-runs) add
their rivals and ran some Arca levels alongside them: a pass's rival times are
put on the main run's clock by how much slower or faster those Arca runs were
on that set (geometric mean), and marked †. In each set's table an Arca cell
carries ✗ when some rival does better on that measure.
"""

import json
import math
import sys
from collections import defaultdict

SET_INFO = {
    "jpeg": "Photos (400 JPEGs)",
    "docs": "PDFs and screenshots",
    "source": "Source code (89 Rust crates)",
    "apps": "Apple apps (8 from /System/Applications)",
    "silesia": "Silesia corpus",
    "saved": "Saved folder (the 660 MB test folder)",
    "mixed": "Website mixed folder (all five sets)",
    "dmgs": "Your DMGs (5 installers)",
    "thirdparty": "Your third-party apps (6)",
}
# The website's numbers (site/data.js): [size MB, compress s, extract s].
WEBSITE = {
    "mixed": {"finder": [322.7, 19.8, 9.32], "7z": [233.4, 49.3, 3.84], "zst": [244.1, 46.5, 5.52], "xz": [235.1, 97.9, 7.28],
              "fast": [286.8, 1.4, 0.88], "balanced": [221.5, 9.7, 2.30], "max": [192.8, 51.6, 3.50], "ultra": [191.0, 37.9, 7.45]},
    "docs": {"finder": [48.6, 1.5, 0.68], "7z": [36.6, 2.7, 0.82], "zst": [37.1, 8.0, 0.42], "xz": [36.9, 22.3, 1.10],
             "fast": [40.0, 0.1, 0.09], "balanced": [20.8, 2.0, 0.26], "max": [16.5, 29.3, 1.20], "ultra": [16.5, 29.4, 1.19]},
    "jpeg": {"finder": [93.3, 2.5, 0.47], "7z": [85.2, 6.0, 1.58], "zst": [85.7, 15.1, 0.27], "xz": [85.8, 43.3, 2.04],
             "fast": [95.0, 0.1, 0.06], "balanced": [71.0, 2.4, 1.24], "max": [71.0, 2.5, 1.25], "ultra": [71.0, 2.5, 1.29]},
    "apps": {"finder": [87.4, 5.8, 1.96], "7z": [54.1, 27.6, 1.69], "zst": [59.4, 23.2, 1.15], "xz": [54.4, 67.0, 2.43],
             "fast": [72.4, 0.4, 0.22], "balanced": [62.4, 2.4, 0.28], "max": [49.8, 23.5, 1.04], "ultra": [49.8, 23.5, 1.03]},
    "source": {"finder": [24.8, 5.2, 6.09], "7z": [8.5, 22.0, 1.22], "zst": [9.3, 26.6, 3.45], "xz": [8.3, 71.5, 3.55],
               "fast": [13.5, 0.5, 0.54], "balanced": [11.2, 2.1, 0.51], "max": [8.3, 25.4, 0.72], "ultra": [8.3, 24.9, 3.57]},
    "silesia": {"finder": [68.2, 5.8, 0.30], "7z": [48.7, 48.3, 1.53], "zst": [52.6, 32.9, 0.27], "xz": [48.4, 99.3, 2.15],
                "fast": [65.7, 0.3, 0.08], "balanced": [57.3, 5.1, 0.15], "max": [48.6, 33.0, 0.89], "ultra": [47.0, 25.1, 5.96]},
}
WEB_TOOL = {"finder": "finder-zip", "7z": "7z-ultra", "zst": "zstd-19-long", "xz": "xz-9e",
            "fast": "arca-fast", "balanced": "arca-balanced", "max": "arca-max", "ultra": "arca-ultra"}
NAMES = {"arca-fast": "Arca Fast", "arca-fast-bsc": "Arca Fast + bsc", "arca-balanced": "Arca Default",
         "arca-balanced-bsc": "Arca Default + bsc", "arca-max": "Arca Max", "arca-max-bsc": "Arca Max + bsc",
         "arca-ultra": "Arca Ultra", "arca-ultra-bsc": "Arca Ultra + bsc", "arca-max-lzt": "Arca Max, fast extract (lzt)",
         "arca-fast-dmg": "Arca Fast + DMG expansion", "arca-balanced-dmg": "Arca Default + DMG expansion",
         "finder-zip": "Finder (ditto zip)", "7z-normal": "7-Zip normal (-mx5)", "7z-ultra": "7-Zip Ultra (-mx9)",
         "zstd-3": "tar + zstd -3", "zstd-19-long": "tar + zstd -19 --long", "xz-6": "tar + xz -6", "xz-9e": "tar + xz -9e",
         "zg-zip-fast": "Zipgeist ZIP Fast", "zg-tzst-fast": "Zipgeist tar.zst Fast", "zg-zip-normal": "Zipgeist ZIP Normal",
         "zg-7z-normal": "Zipgeist 7z Normal", "zg-7z-best": "Zipgeist 7z Best", "zg-txz-best": "Zipgeist tar.xz Best",
         "rar-fastest": "WinRAR Fastest (-m1)", "rar-normal": "WinRAR Normal (-m3)", "rar-best": "WinRAR Best (-m5, solid)"}
SHORT = {"arca-fast": "Fast", "arca-fast-bsc": "Fast+bsc", "arca-balanced": "Default", "arca-balanced-bsc": "Default+bsc",
         "arca-max": "Max", "arca-max-bsc": "Max+bsc", "arca-ultra": "Ultra", "arca-ultra-bsc": "Ultra+bsc",
         "arca-max-lzt": "Max+lzt", "arca-fast-dmg": "Fast+DMG", "arca-balanced-dmg": "Default+DMG"}
METRICS = [("size", "Size MB"), ("make_s", "Make s"), ("extract_s", "Extract s"),
           ("make_mb", "Make peak MB"), ("extract_mb", "Extract peak MB")]
CHARTS = ["size", "make_s", "extract_s"]


def gmean(xs):
    return math.exp(sum(map(math.log, xs)) / len(xs)) if xs else None


def load(path):
    runs = defaultdict(list)
    for line in open(path):
        r = json.loads(line)
        runs[(r["set"], r["tool"])].append(r)
    best = {}
    for key, rs in runs.items():
        ok = [r for r in rs if "error" not in r]
        b = {"tool": key[1], "errors": [r["error"] for r in rs if "error" in r], "load": [r["load"] for r in rs]}
        if ok:
            b["size"] = ok[0]["size"]
            for m in ("make_s", "extract_s", "make_cpu", "extract_cpu"):
                vals = [r[m] for r in ok if r.get(m) is not None]
                b[m] = min(vals) if vals else None
            for m in ("make_mb", "extract_mb"):
                vals = [r[m] for r in ok if r.get(m) is not None]
                b[m] = max(vals) if vals else None
            b["identical"] = all(r.get("identical", True) for r in ok)
        best[key] = b
    return best


def fmt(v, m):
    if v is None:
        return "–"
    if m == "size":
        return f"{v / 1e6:.2f}" if v < 100e6 else f"{v / 1e6:.1f}"
    if m in ("make_mb", "extract_mb"):
        return f"{v:.0f}"
    return f"{v:.2f}" if v < 10 else f"{v:.1f}"


def main():
    best = load(sys.argv[1])
    passes = []
    for extra in sys.argv[2:]:
        more = load(extra)
        name = extra.split("/")[-1]
        # How much slower this pass ran Arca than the main run, per set.
        scale, sizes_match, n = {}, True, 0
        for s in SET_INFO:
            pairs = [(b, best[k]) for k, b in more.items() if k[0] == s and k[1].startswith("arca")
                     and k in best and b.get("make_s") and best[k].get("make_s")]
            if pairs:
                n += len(pairs)
                scale[s] = {m: gmean([b[m] / a[m] for b, a in pairs]) for m in ("make_s", "extract_s")}
                sizes_match &= all(abs(b["size"] / a["size"] - 1) < 1e-4 for b, a in pairs)
        passes.append((name, n, scale, sizes_match))
        for k, b in more.items():
            if k[1].startswith("arca") or (k in best and "size" in best[k]):
                continue
            replaced = k in best
            if k[0] in scale and "size" in b:
                b["raw"] = {m: b[m] for m in ("make_s", "extract_s")}
                for m in ("make_s", "extract_s"):
                    b[m] = b[m] / scale[k[0]][m]
                b["pass"] = name
            b["replaces_errors"] = best[k]["errors"] if replaced else []
            best[k] = b
    sets = [s for s in SET_INFO if any(k[0] == s for k in best)]
    arca_rows = lambda s: [b for (st, t), b in best.items() if st == s and t.startswith("arca") and "size" in b]
    rival_rows = lambda s: [b for (st, t), b in best.items() if st == s and not t.startswith("arca") and "size" in b]
    order = list(NAMES)
    out = []
    p = out.append

    p("## Sessions\n")
    p("The main run timed every Arca level and the command-line rivals together. Zipgeist (driven through "
      "Finder), WinRAR and the 7-Zip re-run on the third-party apps ran in their own sessions, each with "
      "some Arca levels alongside. Their Arca runs against the main run's (geometric mean, all sets):\n")
    for name, n, scale, same in passes:
        mk = gmean([v["make_s"] for v in scale.values()])
        ex = gmean([v["extract_s"] for v in scale.values()])
        p(f"- `{name}`: {n} Arca runs, make ×{mk:.2f}, extract ×{ex:.2f}, sizes {'identical' if same else 'DIFFER'}")
    p("\nThe rivals from those sessions (marked †) have their times divided by that session's factor for "
      "the same set, which puts them on the main run's clock. Zipgeist's session ran slower (Finder and the "
      "screen were busy), so this makes its times shorter than measured: it errs in Zipgeist's favour. "
      "The raw times are in the results files.\n")

    # The website's charts: is Arca at the top of each?
    p("## Top of every chart?\n")
    p("Per set, the best Arca setting against the best rival on each measure (✓ Arca leads, ✗ a rival does). "
      "DMG expansion counts as an Arca setting on the DMG set, since it is a switch at every level.\n")
    p("| Set | Smallest | Fastest to make | Fastest to extract | Least memory (make) | Least memory (extract) |")
    p("|---|---|---|---|---|---|")
    for s in sets:
        cells = []
        for m, _ in METRICS:
            a = min((b for b in arca_rows(s) if b.get(m) is not None), key=lambda b: b[m])
            r = min((b for b in rival_rows(s) if b.get(m) is not None), key=lambda b: b[m])
            mark = "✓" if a[m] <= r[m] else "✗"
            cells.append(f"{mark} {SHORT[a['tool']]} {fmt(a[m], m)} vs {NAMES[r['tool']]} {fmt(r[m], m)}")
        p(f"| {s} | " + " | ".join(cells) + " |")
    p("")

    # Every rival setting: does one Arca setting beat it on size, make and extract at once?
    p("## Every rival setting against one Arca setting\n")
    p("For each rival and set: the first Arca setting (Fast → Ultra + bsc) that is at least as small, faster "
      "to make and faster to extract, all at once. ✗ when none is, with the nearest miss: of the Arca "
      "settings at least as small, the one closest on time, and how much slower it is.\n")
    rivals = [t for t in order if not t.startswith("arca") and any((s, t) in best for s in sets)]
    p("| Rival | " + " | ".join(sets) + " |")
    p("|---|" + "---|" * len(sets))
    beaten = total = 0
    for t in rivals:
        cells = []
        for s in sets:
            R = best.get((s, t))
            if not R or "size" not in R:
                cells.append("–")
                continue
            total += 1
            cands = sorted(arca_rows(s), key=lambda b: order.index(b["tool"]))
            win = [b for b in cands if b["size"] <= R["size"] and b["make_s"] < R["make_s"] and b["extract_s"] < R["extract_s"]]
            if win:
                beaten += 1
                cells.append(SHORT[win[0]["tool"]])
                continue
            smaller = [b for b in cands if b["size"] <= R["size"]]
            if not smaller:
                b = min(cands, key=lambda b: b["size"])
                cells.append(f"✗ none as small ({SHORT[b['tool']]} +{(b['size'] / R['size'] - 1) * 100:.1f}%)")
                continue
            b = min(smaller, key=lambda b: max(b["make_s"] / R["make_s"], b["extract_s"] / R["extract_s"]))
            slow = [f"{w} ×{b[m] / R[m]:.2f}" for m, w in (("make_s", "make"), ("extract_s", "extract")) if b[m] >= R[m]]
            cells.append(f"✗ {SHORT[b['tool']]}: " + ", ".join(slow))
        marks = [bool(best.get((s, t), {}).get("pass")) for s in sets if (s, t) in best]
        if any(marks) and not all(marks):
            cells = [c + " †" if best.get((s, t), {}).get("pass") else c for c, s in zip(cells, sets)]
        p(f"| {NAMES[t]}{' †' if all(marks) else ''} | " + " | ".join(cells) + " |")
    p(f"\nArca beats {beaten} of {total} rival results on size, make and extract at once.\n")

    # How .zlg does against every rival, over every set: geometric means.
    p("## How .zlg compares with every rival (all sets)\n")
    p("Geometric mean over the sets of Arca ÷ rival: under 1.00 means Arca is smaller or faster. "
      "Size / make time / extraction time.\n")
    levels = [t for t in order if t.startswith("arca") and not t.endswith("-dmg")]
    p("| | " + " | ".join(SHORT[a] for a in levels) + " |")
    p("|---|" + "---:|" * len(levels))
    for r in rivals:
        cells = []
        for a in levels:
            ratios = {m: [] for m in CHARTS}
            for s in sets:
                A, R = best.get((s, a)), best.get((s, r))
                if A and R:
                    for m in CHARTS:
                        if A.get(m) and R.get(m):
                            ratios[m].append(A[m] / R[m])
            cells.append(" / ".join("–" if not ratios[m] else f"{gmean(ratios[m]):.2f}" for m in CHARTS))
        p(f"| {NAMES[r]} | " + " | ".join(cells) + " |")
    p("")

    # One table per set.
    for s in sets:
        A, R = arca_rows(s), rival_rows(s)
        p(f"## {SET_INFO[s]}\n")
        loads = sorted({l for b in A + R if not b.get("pass") for l in b["load"]})
        if loads:
            p(f"Load during the main run: {min(loads):.1f}–{max(loads):.1f}.\n")
        p("| Tool | Size MB | Make s | Extract s | Make CPU s | Extract CPU s | Make peak MB | Extract peak MB | Identical | Rival ahead |")
        p("|---|---:|---:|---:|---:|---:|---:|---:|---|---|")
        for b in sorted(A, key=lambda b: order.index(b["tool"])) + sorted(R, key=lambda b: order.index(b["tool"])):
            ahead, cells = [], {}
            for m in [m for m, _ in METRICS] + ["make_cpu", "extract_cpu"]:
                cell = fmt(b.get(m), m)
                if b["tool"].startswith("arca") and b.get(m) is not None and m in dict(METRICS):
                    better = [(r[m], r["tool"]) for r in R if r.get(m) is not None and r[m] < b[m]]
                    if better:
                        cell += " ✗"
                        ahead.append(f"{dict(METRICS)[m].replace(' MB', '').replace(' s', '').lower()}: {NAMES[min(better)[1]]}")
                cells[m] = cell
            ident = "yes" if b.get("identical") else "**NO**"
            p(f"| {NAMES[b['tool']]}{' †' if b.get('pass') else ''} | {cells['size']} | {cells['make_s']} | {cells['extract_s']} | "
              f"{cells['make_cpu']} | {cells['extract_cpu']} | {cells['make_mb']} | {cells['extract_mb']} | {ident} | {'; '.join(ahead)} |")
        p("")

    # Against the website.
    p("## What changed against the website's numbers\n")
    p("The website (site/data.js) was measured on 2026-10-25/26; its Finder, zstd and xz rows in a separate "
      "session. Size MB / compress s / extract s, website → now.\n")
    for s in [s for s in WEBSITE if s in sets]:
        p(f"**{SET_INFO[s]}**\n")
        p("| Tool | Size MB | Compress s | Extract s |")
        p("|---|---|---|---|")
        for k, (ws, wc, we) in WEBSITE[s].items():
            b = best.get((s, WEB_TOOL[k]))
            if not b or "size" not in b:
                continue
            ch = lambda old, new: f"{old:g} → {new:.2f} ({(new / old - 1) * 100:+.0f}%)" if new < 10 else f"{old:g} → {new:.1f} ({(new / old - 1) * 100:+.0f}%)"
            p(f"| {NAMES[WEB_TOOL[k]]} | {ch(ws, b['size'] / 1e6)} | {ch(wc, b['make_s'])} | {ch(we, b['extract_s'])} |")
        p("")

    errs = [(k, b["errors"]) for k, b in best.items() if b["errors"]]
    fixed = [(k, b["replaces_errors"], b["pass"]) for k, b in best.items() if b.get("replaces_errors")]
    if errs or fixed:
        p("## Errors\n")
        for (s, t), e in errs:
            p(f"- {s} {t}: {e[0][:300]}")
        for (s, t), e, name in fixed:
            why = next((l.strip() for l in e[0].splitlines() if "ERROR" in l or "rror:" in l), "see the results file")
            p(f"- {s} {t} failed in the main run ({len(e)} runs: {why[:160]}); "
              f"its row is from `{name}`.")
    print("\n".join(out))


if __name__ == "__main__":
    main()
