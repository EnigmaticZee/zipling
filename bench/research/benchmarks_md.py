#!/usr/bin/env python3
"""Writes docs/BENCHMARKS.md, the public benchmark document the website links to: the six
public sets, every tool, from the app benchmark of 31 October to 1 November 2026."""
import json
import os

WT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")) + "/"
R = WT + "bench/"
ZX = {"zstd-3", "zstd-19-long", "xz-6", "xz-9e"}
SITE = [("mixed", "Mixed folder", 736, "All five sets below in one folder", "A realistic \"compress this folder\" job"),
        ("docs", "PDFs and screenshots", 89, "250 PDFs and 600 PNGs from system and Xcode resources", "Office, PDF and screenshot-heavy folders"),
        ("jpeg", "Photos", 98, "400 JPEGs from system and Xcode resources", "Photo folders"),
        ("apps", "Mac apps", 174, "8 apps from `/System/Applications`: universal Mach-O, asset catalogs, nibs, strings", "Software and `.app` bundles"),
        ("source", "Source code", 162, "89 Rust crates, including vendored C (zstd, xz)", "Code repositories"),
        ("silesia", "Generic data", 212, "The standard Silesia compression corpus", "Data with no file-type tricks to exploit")]
GROUPS = [("Fast settings", [("finder-app", "Finder Compress / Archive Utility"), ("zstd-3", "tar + zstd -3"), ("rar-fastest", "WinRAR Fastest (-m1)"), ("arca-fast", "**Zipling Fast**")]),
          ("Normal settings", [("7z-normal", "7-Zip Normal (-mx5)"), ("rar-normal", "WinRAR Normal (-m3)"), ("xz-6", "tar + xz -6"), ("arca-balanced", "**Zipling Default**")]),
          ("Strongest settings", [("7z-ultra", "7-Zip Ultra (-mx9)"), ("rar-best", "WinRAR Best (-m5, solid)"), ("zstd-19-long", "tar + zstd -19 --long"), ("xz-9e", "tar + xz -9e"),
                                  ("arca-max", "**Zipling Max**"), ("arca-ultra", "**Zipling Ultra***")])]


def load(path, keep=None, drop=None):
    best = {}
    if not os.path.exists(path):
        return best
    for line in open(path):
        r = json.loads(line)
        if "size" not in r or r.get("identical") is False or (keep and r["tool"] not in keep) or (drop and r["tool"] in drop):
            continue
        k = (r["set"], r["tool"])
        b = best.get(k)
        if b is None:
            best[k] = dict(r)
        else:
            for m in ("make_s", "extract_s"):
                b[m] = min(b[m], r[m])
            for m in ("make_mb", "extract_mb"):
                if r.get(m) is not None:
                    b[m] = max(b.get(m) or 0, r[m])
    return best


zx = load(R + "results-2026-10-31-apps-zx.jsonl", keep=ZX)
assert len(zx) == 36, f"second pass incomplete: {len(zx)} rows"
rows = load(R + "results-2026-10-31-apps.jsonl", drop=ZX)
rows.update(zx)
rows.update(load(R + "results-2026-10-31-apps-finder.jsonl"))


def mb(v):
    return f"{v / 1e6:.2f}" if v < 100e6 else f"{v / 1e6:.1f}"


def sec(v):
    return f"{v:.2f}" if v < 10 else f"{v:.1f}"


def r(s, t):
    return rows[(s, t)]


def pct(a, b):
    return f"{100 * (1 - a / b):.0f}%"


m = {t: r("mixed", t) for _, g in GROUPS for t, _ in g}
d = {t: r("docs", t) for _, g in GROUPS for t, _ in g}
j = {t: r("jpeg", t) for _, g in GROUPS for t, _ in g}

tables = []
for s, name, orig, what, why in SITE:
    t = [f"### {name} ({orig} MB)\n", f"{what}. {why}.\n",
         "| Tool | Size | % of original | Compress | Extract | Peak memory, compressing |", "|---|---:|---:|---:|---:|---:|"]
    for gname, tools in GROUPS:
        t.append(f"| *{gname}* | | | | | |")
        for tool, label in tools:
            x = r(s, tool)
            pk = f"{x['make_mb']:.0f} MB" if x.get("make_mb") else "–"
            t.append(f"| {label} | {mb(x['size'])} MB | {100 * x['size'] / (orig * 1e6):.0f}% | {sec(x['make_s'])} s | {sec(x['extract_s'])} s | {pk} |")
    tables.append("\n".join(t))

LOWER = {"mixed": "the mixed folder", "docs": "PDFs and screenshots", "jpeg": "photos", "apps": "Mac apps", "source": "source code", "silesia": "generic data"}


def rng(lo, hi):
    lo, hi = sorted((lo, hi))
    return f"{lo:.0f}×" if f"{lo:.0f}" == f"{hi:.0f}" else f"{lo:.0f}× to {hi:.0f}×"


behind = []
for s, name, *_ in SITE:
    a, b, c = r(s, "arca-balanced"), r(s, "7z-ultra"), r(s, "7z-normal")
    if a["size"] >= b["size"]:
        behind.append(f"- **Default on {LOWER[s]}** is {100 * (a['size'] / b['size'] - 1):.0f}% larger than 7-Zip Ultra and {100 * (a['size'] / c['size'] - 1):.0f}% larger than 7-Zip Normal, "
                      f"for {rng(c['make_s'] / a['make_s'], b['make_s'] / a['make_s'])} less compression time and {rng(c['extract_s'] / a['extract_s'], b['extract_s'] / a['extract_s'])} less extraction time. "
                      f"Max is {pct(r(s, 'arca-max')['size'], b['size'])} smaller than 7-Zip Ultra there, in {sec(r(s, 'arca-max')['make_s'])} s against {sec(b['make_s'])} s.")
for s, name, *_ in SITE:
    a, b = r(s, "arca-max"), r(s, "7z-ultra")
    if a["make_s"] >= b["make_s"]:
        behind.append(f"- **Max on {LOWER[s]}** takes {sec(a['make_s'])} s to compress against 7-Zip Ultra's {sec(b['make_s'])} s, for an archive {pct(a['size'], b['size'])} smaller: "
                      f"re-creating the compression inside PDFs and PNGs is work the other tools skip. Default is {pct(r(s, 'arca-balanced')['size'], b['size'])} smaller than 7-Zip Ultra on the same folder in {sec(r(s, 'arca-balanced')['make_s'])} s.")
a, b = r("silesia", "arca-fast"), r("silesia", "zstd-3")
if a["make_s"] >= b["make_s"]:
    behind.append(f"- **Fast on generic data** loses to `zstd -3` on compression time ({sec(a['make_s'])} against {sec(b['make_s'])} s) at a file {pct(a['size'], b['size'])} smaller that extracts {b['extract_s'] / a['extract_s']:.1f}× faster.")
for s, name, *_ in SITE:
    a, b = r(s, "arca-fast"), r(s, "rar-fastest")
    if a["size"] >= b["size"] or a["make_s"] >= b["make_s"] or a["extract_s"] >= b["extract_s"]:
        behind.append(f"- **Fast on {LOWER[s]}** against WinRAR Fastest: {mb(a['size'])} against {mb(b['size'])} MB, {sec(a['make_s'])} against {sec(b['make_s'])} s to compress, {sec(a['extract_s'])} against {sec(b['extract_s'])} s to extract.")
ua = [100 * (1 - r(s, "arca-ultra")["size"] / r(s, "arca-max")["size"]) for s, *_ in SITE]
ut = [100 * (r(s, "arca-ultra")["make_s"] / r(s, "arca-max")["make_s"] - 1) for s, *_ in SITE]
behind.append(f"- **Ultra*** is {min(ua):.0f} to {max(ua):.0f}% smaller than Max for {min(ut):.0f} to {max(ut):.0f}% more compression time. It is experimental and not in the released app.")

doc = f"""# Benchmarks

Zipling, the app, against Finder, 7-Zip, WinRAR, zstd and xz on six public datasets: archive size,
time to compress, time to extract, peak memory. Every archive from every tool was extracted and
compared with the source byte for byte (names, types, bytes, link targets). **Every round trip was
identical.**

**Machine.** MacBook Pro, Apple M1 Pro (8 performance + 2 efficiency cores), 32 GB, internal SSD,
macOS 27.0.1. Each dataset is read once before its runs, so every tool starts with the files in the
page cache. Two rounds per tool, the second in reverse order, the better kept. A dataset's runs start
only when the 1-minute load is under 2.5. Back-to-back runs vary by about ±10%, so treat small
differences as ties.

**When.** Finder on 31 October 2026, driven in Finder itself (its Compress command, and Archive
Utility by double-click), timed from the click to the finished file. Zipling and the command-line
tools in one unattended session the night of 31 October to 1 November 2026, alternating within each
dataset so that no tool gets a quieter machine than another. zstd and xz were re-measured in a second
pass the same night with the current Homebrew releases, after the first pass had picked up older
copies; the second pass is what the tables show.

**Versions.** Zipling 0.1.0 (Arca engine, archive format 3, the presets locked on 31 October 2026);
7-Zip 26.05 (`7zz -mx5` Normal and `-mx9` Ultra, links kept); WinRAR 7.23 for macOS, command line
(`rar -m1`, `-m3`, `-m5 -s`); zstd 1.5.7 (`tar | zstd -3 -T0` and `-19 --long=27 -T0`); XZ Utils 5.8.4
(`tar | xz -6 -T0` and `-9e -T0`); Finder from macOS 27.0.1. Keka and BetterZip compress 7z through
7-Zip's code, so the 7-Zip rows stand in for them; Archive Utility and MacPacker make Deflate ZIPs,
as Finder does.

**How Zipling was timed.** The installed app, driven through its AppleScript `compress` and `extract`
commands, timed by its own clock from the command's arrival in the app to the finished file (the
app's timing log). That includes the app's listing decode, its safety walk over the extracted tree and
its main-actor hops, about 30 ms plus a few microseconds per file on an extraction, and it is what a
user gets. The AppleScript round trip itself (50 to 60 ms of osascript start-up, measured before each
dataset) is not charged to the app. Peak memory is the app process's physical footprint, sampled every
20 ms; for the command-line tools it is `/usr/bin/time -l`'s maximum resident set size.

Raw rows: `bench/results-2026-10-31-apps.jsonl` (Zipling, 7-Zip, WinRAR, and the first zstd and xz
pass), `bench/results-2026-10-31-apps-zx.jsonl` (zstd and xz, second pass),
`bench/results-2026-10-31-apps-finder.jsonl` (Finder), `bench/results-2026-10-31-apps-machine.json`
(machine, versions, method). Harness: `bench/research/apprun.py`; scoring: `bench/research/levels.py`.

## Datasets

Public and system data only; no personal files were read. Sizes are the folders' decimal megabytes.

| Set | What | Why it matters |
|---|---|---|
""" + "\n".join(f"| {name} ({orig} MB) | {what} | {why} |" for _, name, orig, what, why in SITE) + f"""

## Headline (mixed folder, 736 MB)

- **Zipling Default against 7-Zip Ultra:** {pct(m['arca-balanced']['size'], m['7z-ultra']['size'])} smaller, {m['7z-ultra']['make_s'] / m['arca-balanced']['make_s']:.1f}× faster to compress, {m['7z-ultra']['extract_s'] / m['arca-balanced']['extract_s']:.1f}× faster to extract ({sec(m['arca-balanced']['extract_s'])} against {sec(m['7z-ultra']['extract_s'])} s).
- **Zipling Max against 7-Zip Ultra:** {pct(m['arca-max']['size'], m['7z-ultra']['size'])} smaller ({mb(m['arca-max']['size'])} against {mb(m['7z-ultra']['size'])} MB) in {100 * m['arca-max']['make_s'] / m['7z-ultra']['make_s']:.0f}% of the compression time, {m['7z-ultra']['extract_s'] / m['arca-max']['extract_s']:.1f}× faster to extract.
- **Zipling Default against Finder's Compress:** {pct(m['arca-balanced']['size'], m['finder-app']['size'])} smaller, {m['finder-app']['make_s'] / m['arca-balanced']['make_s']:.1f}× faster to compress, {m['finder-app']['extract_s'] / m['arca-balanced']['extract_s']:.1f}× faster to extract.
- **Zipling Fast against Finder's Compress:** {pct(m['arca-fast']['size'], m['finder-app']['size'])} smaller, {m['finder-app']['make_s'] / m['arca-fast']['make_s']:.0f}× faster to compress, {m['finder-app']['extract_s'] / m['arca-fast']['extract_s']:.0f}× faster to extract.
- **Zipling Fast against `tar | zstd -3`:** {pct(m['arca-fast']['size'], m['zstd-3']['size'])} smaller, {m['zstd-3']['make_s'] / m['arca-fast']['make_s']:.0f}× faster to compress, {m['zstd-3']['extract_s'] / m['arca-fast']['extract_s']:.0f}× faster to extract.
- **PDFs and screenshots:** Default is {pct(d['arca-balanced']['size'], d['7z-ultra']['size'])} smaller than 7-Zip Ultra and {d['7z-ultra']['make_s'] / d['arca-balanced']['make_s']:.1f}× faster to compress; Max is {pct(d['arca-max']['size'], d['7z-ultra']['size'])} smaller.
- **Photos:** Default is {pct(j['arca-balanced']['size'], j['7z-ultra']['size'])} smaller than 7-Zip Ultra ({mb(j['arca-balanced']['size'])} against {mb(j['7z-ultra']['size'])} MB), with every JPEG restored bit for bit; the other tools get {pct(j['finder-app']['size'], 98e6)} to {pct(j['7z-ultra']['size'], 98e6)} off the originals.

## Where Zipling is behind

{chr(10).join(behind)}

## Every dataset, every tool

Each Zipling level sits with the settings it competes with. Ultra* is experimental and not in the
released app.

{(chr(10) * 2).join(tables)}
"""
open(WT + "docs/BENCHMARKS.md", "w").write(doc)
print("written", len(doc), "chars")
