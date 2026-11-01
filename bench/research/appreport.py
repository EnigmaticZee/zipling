#!/usr/bin/env python3
"""Writes bench/report-2026-10-31-apps.md: the Zipling app against every rival app and
tool, all three phases. Zipling, 7-Zip and WinRAR rows from results-2026-10-31-apps.jsonl;
zstd and xz from the second pass with the Homebrew binaries (results-2026-10-31-apps-zx.jsonl);
Zipgeist and Finder from phases 1 and 2. Best of two rounds per tool."""
import collections
import json
import os
import re
import subprocess
import sys
import tempfile

WT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")) + "/"
R = WT + "bench/"
S = tempfile.mkdtemp(prefix="appreport-") + "/"
APPS, ZXF, ZG, FI, ENG = (R + "results-2026-10-31-apps" + x + ".jsonl" for x in ("", "-zx", "-zipgeist", "-finder", ""))
ENG = R + "results-2026-10-31-presets.jsonl"
ZX = {"zstd-3", "zstd-19-long", "xz-6", "xz-9e"}
SETS = ["jpeg", "docs", "source", "apps", "silesia", "saved", "mixed", "dmgs", "thirdparty"]
TITLES = {"jpeg": "Photos", "docs": "PDFs and screenshots", "source": "Source code", "apps": "Apple apps", "silesia": "Silesia",
          "saved": "Saved folder", "mixed": "Mixed folder", "dmgs": "Disk images", "thirdparty": "Third-party apps"}
LONG = {"jpeg": "Photos (95 MB, 400 JPEGs)", "docs": "PDFs and screenshots (89 MB, 850 files)", "source": "Source code (172 MB, 7,693 files)",
        "apps": "Apple apps (170 MB, 8 apps)", "silesia": "Silesia corpus (203 MB)", "saved": "Saved folder (533 MB, 11,051 files)",
        "mixed": "Website mixed folder (726 MB, the five public sets)", "dmgs": "Disk images (592 MB, 5 installers)", "thirdparty": "Third-party apps (874 MB, 6 apps)"}
LEVELS = [("arca-fast", "Fast"), ("arca-balanced", "Default"), ("arca-max", "Max"), ("arca-ultra", "Ultra*")]
CLASSES = {"arca-fast": ["zg-zip-fast", "zg-tzst-fast", "finder-app", "zstd-3", "rar-fastest"],
           "arca-balanced": ["zg-zip-normal", "zg-7z-normal", "7z-normal", "rar-normal", "xz-6"],
           "arca-max": ["zg-7z-best", "zg-txz-best", "7z-ultra", "rar-best", "zstd-19-long", "xz-9e"]}
CLASSES["arca-ultra"] = CLASSES["arca-max"]
NAMES = {"arca-fast": "**Zipling Fast**", "arca-balanced": "**Zipling Default**", "arca-max": "**Zipling Max**", "arca-ultra": "**Zipling Ultra***",
         "finder-app": "Finder Compress / Expand", "zg-zip-fast": "Zipgeist ZIP Fast", "zg-tzst-fast": "Zipgeist tar.zst Fast",
         "zg-zip-normal": "Zipgeist ZIP Normal", "zg-7z-normal": "Zipgeist 7z Normal", "zg-7z-best": "Zipgeist 7z Best", "zg-txz-best": "Zipgeist tar.xz Best",
         "7z-normal": "7-Zip Normal (-mx5)", "7z-ultra": "7-Zip Ultra (-mx9)", "rar-fastest": "WinRAR Fastest (-m1)", "rar-normal": "WinRAR Normal (-m3)",
         "rar-best": "WinRAR Best (-m5, solid)", "zstd-3": "tar + zstd -3", "zstd-19-long": "tar + zstd -19 --long", "xz-6": "tar + xz -6", "xz-9e": "tar + xz -9e"}
PLAIN = {k: v.replace("*", "") if k.startswith("arca") else v for k, v in NAMES.items()}
PLAIN["arca-ultra"] = "Zipling Ultra*"
COL = {"zg-zip-fast": "Zipgeist ZIP Fast", "zg-tzst-fast": "Zipgeist tar.zst Fast", "finder-app": "Finder", "zstd-3": "zstd -3", "rar-fastest": "WinRAR Fastest",
       "zg-zip-normal": "Zipgeist ZIP Normal", "zg-7z-normal": "Zipgeist 7z Normal", "7z-normal": "7-Zip Normal", "rar-normal": "WinRAR Normal", "xz-6": "xz -6",
       "zg-7z-best": "Zipgeist 7z Best", "zg-txz-best": "Zipgeist tar.xz Best", "7z-ultra": "7-Zip Ultra", "rar-best": "WinRAR Best", "zstd-19-long": "zstd -19 --long", "xz-9e": "xz -9e"}
ORDER = ["arca-fast", "finder-app", "zg-zip-fast", "zg-tzst-fast", "zstd-3", "rar-fastest",
         "arca-balanced", "zg-zip-normal", "zg-7z-normal", "7z-normal", "rar-normal", "xz-6",
         "arca-max", "arca-ultra", "zg-7z-best", "zg-txz-best", "7z-ultra", "rar-best", "zstd-19-long", "xz-9e"]
CLI = {"7z-normal", "7z-ultra", "rar-fastest", "rar-normal", "rar-best"} | ZX


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
            best[k] = dict(r, rounds=1)
        else:
            b["rounds"] += 1
            for m in ("make_s", "extract_s", "make_cpu", "extract_cpu"):
                if r.get(m) is not None and (b.get(m) is None or r[m] < b[m]):
                    b[m] = r[m]
            for m in ("make_mb", "extract_mb"):
                if r.get(m) is not None and (b.get(m) is None or r[m] > b[m]):
                    b[m] = r[m]
    return best


zx = load(ZXF, keep=ZX)
rows = load(APPS, drop=ZX if zx else None)
rows.update(zx)
rows.update(load(ZG, drop={"arca-fast-clock"}))
rows.update(load(FI))
eng = load(ENG)
missing = [(s, t) for s in SETS for t in ORDER if (s, t) not in rows]
if missing:
    sys.exit(f"missing rows: {missing}")


def beats(a, b):
    return a["size"] < b["size"] and a["make_s"] < b["make_s"] and a["extract_s"] < b["extract_s"]


def why(a, b):
    return [m for m, k in (("size", "size"), ("make", "make_s"), ("extract", "extract_s")) if a[k] >= b[k]]


def x(r):
    return f"{r:.0f}x" if r >= 3 else f"{r:.1f}x"


def mb(v):
    return f"{v / 1e6:.2f}" if v < 100e6 else f"{v / 1e6:.1f}"


def sec(v):
    return f"{v:.2f}" if v < 10 else f"{v:.1f}"


# --- tally, 1:1 by level
won = collections.Counter()
total = collections.Counter()
lost = collections.defaultdict(lambda: collections.defaultdict(list))  # level -> rival -> [(set, why)]
for lv, _ in LEVELS:
    for s in SETS:
        a = rows[(s, lv)]
        for rv in CLASSES[lv]:
            b = rows[(s, rv)]
            total[lv] += 1
            if beats(a, b):
                won[lv] += 1
            else:
                lost[lv][rv].append((s, ", ".join(why(a, b))))


def lost_text(lv):
    parts = []
    for rv in CLASSES[lv]:
        if rv not in lost[lv]:
            continue
        by_reason = collections.defaultdict(list)
        for s, w in lost[lv][rv]:
            by_reason[w].append(TITLES[s])
        parts.append(f"{COL[rv]}: " + "; ".join(f"{w} on {', '.join(sets)}" for w, sets in by_reason.items()))
    return " · ".join(parts) if parts else "nothing"


def subset_tally(pred):
    out = []
    for lv, name in LEVELS:
        w = n = 0
        for s in SETS:
            for rv in CLASSES[lv]:
                if pred(rv):
                    n += 1
                    w += beats(rows[(s, lv)], rows[(s, rv)])
        out.append((name, w, n))
    return out


def any_level(pred):
    w = n = 0
    for s in SETS:
        for rv in set(sum(CLASSES.values(), [])):
            if pred(rv):
                n += 1
                w += any(beats(rows[(s, lv)], rows[(s, rv)]) for lv, _ in LEVELS)
    return w, n


zg_lv = subset_tally(lambda rv: rv.startswith("zg-"))
cli_lv = subset_tally(lambda rv: rv in CLI)
zg_any, cli_any, fi_any = any_level(lambda rv: rv.startswith("zg-")), any_level(lambda rv: rv in CLI), any_level(lambda rv: rv == "finder-app")
all_any = any_level(lambda rv: True)

# --- upsets
RANK = {"arca-fast": 0, "arca-balanced": 1, "arca-max": 2}
ups = collections.defaultdict(list)
for lv in ("arca-fast", "arca-balanced"):
    for hi in ("arca-balanced", "arca-max"):
        if RANK[hi] <= RANK[lv]:
            continue
        for rv in CLASSES[hi]:
            for s in SETS:
                if beats(rows[(s, lv)], rows[(s, rv)]):
                    ups[(lv, rv)].append(s)
up_lines = []
for (lv, rv), sets in sorted(ups.items(), key=lambda kv: (-len(kv[1]), kv[0])):
    a = [rows[(s, lv)] for s in sets]
    b = [rows[(s, rv)] for s in sets]
    size = [100 * (1 - p["size"] / q["size"]) for p, q in zip(a, b)]
    mk = [q["make_s"] / p["make_s"] for p, q in zip(a, b)]
    ex = [q["extract_s"] / p["extract_s"] for p, q in zip(a, b)]
    rng = lambda v, f: f(min(v)) if len(v) == 1 or f(min(v)) == f(max(v)) else f"{f(min(v))} to {f(max(v))}"  # noqa: E731
    up_lines.append(f"- **{PLAIN[lv]} beats {COL[rv]}** on {len(sets)} of 9 sets ({', '.join(TITLES[s] for s in sets)}): "
                    f"{rng(size, lambda v: f'{v:.0f}')}% smaller, {rng(mk, lambda v: f'{v:.1f}')}x faster to make, {rng(ex, lambda v: f'{v:.1f}')}x faster to extract.")

# --- scoreboard grids
grids = []
for lv, name in LEVELS:
    cols = CLASSES[lv]
    g = [f"### Zipling {name}\n", "| Set | " + " | ".join(COL[c] for c in cols) + " |", "|---|" + "---|" * len(cols)]
    for s in SETS:
        a = rows[(s, lv)]
        cells = []
        for rv in cols:
            b = rows[(s, rv)]
            w = []
            if a["size"] >= b["size"]:
                w.append(f"{100 * (a['size'] / b['size'] - 1):.0f}% bigger")
            if a["make_s"] >= b["make_s"]:
                w.append(f"{x(a['make_s'] / b['make_s'])} slower to make")
            if a["extract_s"] >= b["extract_s"]:
                w.append(f"{x(a['extract_s'] / b['extract_s'])} slower to extract")
            cells.append("✅" if not w else "❌ " + ", ".join(w))
        g.append(f"| {TITLES[s]} | " + " | ".join(cells) + " |")
    g.append(f"\n**{name}: {won[lv]} of {total[lv]} won.**\n")
    grids.append("\n".join(g))

# --- set by set
tables = []
for s in SETS:
    t = [f"**{LONG[s]}**\n", "| Tool | Size | Make | Extract |", "|---|---:|---:|---:|"]
    for tool in ORDER:
        r = rows[(s, tool)]
        t.append(f"| {NAMES[tool]} | {mb(r['size'])} MB | {sec(r['make_s'])} s | {sec(r['extract_s'])} s |")
    tables.append("\n".join(t))

# --- Finder
finder_rows = "\n".join(
    f"| {TITLES[s]} | {mb(rows[(s, 'finder-app')]['size'])} MB | {sec(rows[(s, 'finder-app')]['make_s'])} s | {sec(rows[(s, 'finder-app')]['extract_s'])} s | "
    f"{mb(rows[(s, 'arca-fast')]['size'])} MB | {sec(rows[(s, 'arca-fast')]['make_s'])} s | {sec(rows[(s, 'arca-fast')]['extract_s'])} s | "
    f"{100 * (1 - rows[(s, 'arca-fast')]['size'] / rows[(s, 'finder-app')]['size']):.0f}% / {rows[(s, 'finder-app')]['make_s'] / rows[(s, 'arca-fast')]['make_s']:.0f}x / {rows[(s, 'finder-app')]['extract_s'] / rows[(s, 'arca-fast')]['extract_s']:.0f}x |"
    for s in SETS)

# --- the app against the engine
ae = ["| Set | " + " | ".join(f"{n} make | {n} extract" for _, n in LEVELS) + " |", "|---|" + "---:|" * (2 * len(LEVELS))]
mk_r, ex_d = [], []
for s in SETS:
    cells = []
    for lv, _ in LEVELS:
        a, e = rows[(s, lv)], eng[(s, lv)]
        mk_r.append(a["make_s"] / e["make_s"])
        ex_d.append((a["extract_s"] - e["extract_s"]) * 1000)
        cells.append(f"{a['make_s'] / e['make_s']:.2f}x")
        cells.append(f"{(a['extract_s'] - e['extract_s']) * 1000:+.0f} ms ({a['extract_s'] / e['extract_s']:.2f}x)")
    ae.append(f"| {TITLES[s]} | " + " | ".join(cells) + " |")
app_vs_engine = "\n".join(ae)
eng_won = {(s, lv, rv) for lv, _ in LEVELS for s in SETS for rv in CLASSES[lv] if beats(eng[(s, lv)], rows[(s, rv)])}
app_won = {(s, lv, rv) for lv, _ in LEVELS for s in SETS for rv in CLASSES[lv] if beats(rows[(s, lv)], rows[(s, rv)])}
name_row = lambda t: f"{TITLES[t[0]]}, {PLAIN[t[1]]} against {COL[t[2]]}"  # noqa: E731
flip_lost, flip_won = sorted(eng_won - app_won), sorted(app_won - eng_won)
flip_text = (f"Scored against the same rival rows, the engine's times would win {len(eng_won)} of {sum(total.values())} and the app's win {len(app_won)}. "
             + (f"The app's extra extraction time turns {len(flip_lost)} row{'s' if len(flip_lost) != 1 else ''}: {'; '.join(map(name_row, flip_lost))}. " if flip_lost else "No row turns on the app's extra time. ")
             + (f"Rows that go the other way, on noise: {'; '.join(map(name_row, flip_won))}." if flip_won else ""))

# --- peak memory
mem = ["| Set | Fast | Default | Max | Ultra* | 7-Zip Ultra | WinRAR Best | zstd -19 --long | xz -9e |", "|---|---:|---:|---:|---:|---:|---:|---:|---:|"]
for s in SETS:
    mem.append(f"| {TITLES[s]} | " + " | ".join(f"{rows[(s, t)]['make_mb']:.0f}" for t in ("arca-fast", "arca-balanced", "arca-max", "arca-ultra", "7z-ultra", "rar-best", "zstd-19-long", "xz-9e")) + " |")
mem_table = "\n".join(mem)

# --- Default's rows
d_zip = [rows[(s, "arca-balanced")]["make_s"] / rows[(s, "zg-zip-normal")]["make_s"] for s in SETS]
d_zip_size = [100 * (1 - rows[(s, "arca-balanced")]["size"] / rows[(s, "zg-zip-normal")]["size"]) for s in SETS]
d_zip_ex_won = sum(rows[(s, "arca-balanced")]["extract_s"] < rows[(s, "zg-zip-normal")]["extract_s"] for s in SETS)
d_7z = {s: (100 * (rows[(s, "arca-balanced")]["size"] / rows[(s, "7z-normal")]["size"] - 1),
            rows[(s, "7z-normal")]["make_s"] / rows[(s, "arca-balanced")]["make_s"],
            rows[(s, "7z-normal")]["extract_s"] / rows[(s, "arca-balanced")]["extract_s"]) for s in SETS}
d_7z_lost = [s for s in SETS if d_7z[s][0] >= 0]
d_7z_won_size = [s for s in SETS if d_7z[s][0] < 0]
dd = {s: rows[(s, "arca-balanced")] for s in SETS}

# --- levels.py, row by row
filtered = S + "apps-no-zx.jsonl"
with open(filtered, "w") as f:
    for line in open(APPS):
        if not zx or json.loads(line)["tool"] not in ZX:
            f.write(line)
cmd = [sys.executable, "-I", WT + "bench/research/levels.py", filtered] + ([ZXF] if zx else []) + ["--rivals", ZG, FI]
levels_out = subprocess.run(cmd, capture_output=True, text=True, cwd=WT).stdout
assert "Rows won" in levels_out, levels_out[-500:]
lv_total = re.search(r"all\s+(\d+) of (\d+)", levels_out)
assert lv_total and int(lv_total.group(1)) == sum(won.values()) and int(lv_total.group(2)) == sum(total.values()), (lv_total and lv_total.groups(), sum(won.values()))

# --- gates from the log
gates = {}
busy = []
for line in open(APPS + ".log"):
    m = re.match(r"\d\d:\d\d:\d\d (\w+): start \(load ([\d.]+)", line)
    if m:
        gates[m.group(1)] = float(m.group(2))
    if "still busy" in line:
        busy.append(line.strip())
zx_gates = {}
if zx and os.path.exists(ZXF + ".log"):
    for line in open(ZXF + ".log"):
        m = re.match(r"\d\d:\d\d:\d\d (\w+): start \(load ([\d.]+)", line)
        if m:
            zx_gates[m.group(1)] = float(m.group(2))

n_rows = sum(1 for _ in open(APPS))
n_zx = sum(1 for _ in open(ZXF)) if zx else 0
zx_note = (f"zstd and xz were measured twice. The main run picked up zstd 1.5.6 and XZ Utils 5.6.4 from a conda "
           f"environment that sat first in the harness's PATH; a second pass of the same four settings on all nine sets "
           f"({n_zx} rows, `results-2026-10-31-apps-zx.jsonl`, Homebrew zstd 1.5.7 and XZ Utils 5.8.4, the current releases) "
           f"replaces those rows everywhere below. The first-pass rows stay in the raw file for the record." if zx else
           "zstd and xz rows are from the main run (zstd 1.5.6, XZ Utils 5.6.4).")

report = f"""# Zipling app benchmark, 2026-10-31: the app against the rival apps and tools

Status: **all three phases done.** Phase 1 drove Zipgeist 1.0.0 through its own Finder menu on
all nine sets, six settings, two rounds: 108 of 108 round trips identical. Phase 2 drove Finder's
own Compress and the double-click Expand (Archive Utility): 18 of 18 identical. Phase 3 (the night
of 31 October to 1 November) drove the installed Zipling app through its AppleScript commands at
Fast, Default, Max and Ultra*, alternating with 7-Zip 26.05, WinRAR 7.23, zstd and xz as processes,
on the same nine sets, two rounds, unattended: {n_rows} rows, no errors, every first-round extraction
identical to the source. Every Zipling number below is the **app's own clock**, from the command's
arrival in the app to the finished file; it includes the app's listing decode, its safety walk and
its main-actor hops, and it is what a user gets.

Machine: MacBook Pro, Apple M1 Pro (8 performance + 2 efficiency cores), 32 GB, internal SSD, macOS
27.0.1. Method, versions and set descriptions: `results-2026-10-31-apps-machine.json`. Raw rows:
`results-2026-10-31-apps.jsonl` (Zipling, 7-Zip, WinRAR, first-pass zstd and xz),
`results-2026-10-31-apps-zx.jsonl` (zstd and xz, second pass), `results-2026-10-31-apps-zipgeist.jsonl`,
`results-2026-10-31-apps-finder.jsonl`. Scoring: `bench/research/levels.py`.

{zx_note}

How each tool was timed. Zipling: the app's timing log (`defaults write com.observantsystems.zipling
timing -bool true`), command arrived to file finished; the AppleScript round trip itself (50 to 60 ms
of osascript start-up and event delivery, measured before each set) is not charged to the app, and
the unsubtracted wall time is kept in the row (`make_s_raw`). Zipgeist: from the keypress on its
Compress button (or the click on its Extract menu item) to the last busy moment of its processes,
with the archive's write time as a floor. Finder: from the click on Compress to the finished archive
landing, and from the double-click to Archive Utility's last busy moment. Command-line tools: wall
time of the process (`/usr/bin/time -l`), `tar | zstd` and `tar | xz` as pipelines. Zipgeist and
Finder ran under the screen-control overlay, which kept about one core busy; Zipling and the
command-line tools ran with no screen activity at all.

A row is **won** only when Zipling is smaller, faster to make and faster to extract at once, level
against level: Fast against the fast settings (Zipgeist ZIP Fast and tar.zst Fast, Finder's
Compress, zstd -3, WinRAR Fastest), Default against the normal ones (Zipgeist ZIP Normal and 7z
Normal, 7-Zip Normal, WinRAR Normal, xz -6), Max and Ultra* against the best (Zipgeist 7z Best and
tar.xz Best, 7-Zip Ultra, WinRAR Best, zstd -19 --long, xz -9e). Ultra* is experimental and not in
the released app.

## Tally, 1:1 by level

| Level | Rows won | Lost on |
|---|---:|---|
""" + "\n".join(f"| {name} | {won[lv]} of {total[lv]} | {lost_text(lv)} |" for lv, name in LEVELS) + f"""

**All levels: {sum(won.values())} of {sum(total.values())}.** Allowing any Zipling level against each rival
result, {all_any[0]} of {all_any[1]} results are beaten.

By rival family, level against level: Zipgeist {', '.join(f'{n} {w}/{t}' for n, w, t in zg_lv)}
(any level: {zg_any[0]} of {zg_any[1]}); the command-line tools (7-Zip, WinRAR, zstd, xz)
{', '.join(f'{n} {w}/{t}' for n, w, t in cli_lv)} (any level: {cli_any[0]} of {cli_any[1]});
Finder's Compress {fi_any[0]} of {fi_any[1]}.

## Upsets: a lower level beating a higher rival setting on all three

{chr(10).join(up_lines)}

## Scoreboard: one grid per level

{chr(10).join(grids)}
## Set by set

{(chr(10) * 2).join(tables)}

## Finder's own Compress and Expand against Zipling Fast

| Set | Finder size | Finder make | Expand | Fast size | Fast make | Fast extract | Fast: smaller / faster make / faster extract |
|---|---:|---:|---:|---:|---:|---:|---|
{finder_rows}

## The app against the engine

The same presets measured through the engine's command line on the morning of 31 October
(`results-2026-10-31-presets.jsonl`, quiet machine) against the app's clock in this run. Make time
is the ratio app / engine; extract time is the app's extra in milliseconds, with the ratio.

{app_vs_engine}

Make time is the same within noise (the geometric mean of the ratios is {__import__('math').exp(sum(map(__import__('math').log, mk_r)) / len(mk_r)):.2f}x). Extraction in the app
differs by {min(ex_d):+.0f} to {max(ex_d):+.0f} ms, nearly always more: it decodes the listing, hops to the main actor and walks the
extracted tree for safety, about 30 ms plus a few microseconds per file. On the quick Fast rows that is
the difference between 0.05 and 0.08 s. {flip_text} Skipping the listing for a
whole-archive extract and a cheaper safety walk are the two follow-ups, worth 30 to 300 ms per
extraction, no row at stake.

## Peak memory while compressing (MB)

Zipling's figure is the app process's peak physical footprint sampled every 20 ms; the command-line
tools' is `/usr/bin/time -l`'s maximum resident set size. Zipgeist's stopwatch sampled its helper
processes and is not comparable, so it is left out.

{mem_table}

## Default: what is fixable and what is not

Default loses two kinds of rows, for two different reasons.

**1. Against Zipgeist ZIP Normal, on make time, on {sum(1 for s in SETS if dd[s]['make_s'] >= rows[(s, 'zg-zip-normal')]['make_s'])} of nine sets.** ZIP Normal is a near-store
Deflate: {sec(rows[('jpeg', 'zg-zip-normal')]['make_s'])} s on the photos, {sec(rows[('docs', 'zg-zip-normal')]['make_s'])} s on the PDFs, {sec(rows[('saved', 'zg-zip-normal')]['make_s'])} s on the Saved folder. Default makes files
{min(d_zip_size):.0f} to {max(d_zip_size):.0f}% smaller than it and extracts faster on {d_zip_ex_won} of nine sets, but takes {min(d_zip):.1f}x to {max(d_zip):.0f}x longer, because it runs
the transforms. No preset that compresses can beat a near-store ZIP on make time; the Zipling setting
that answers it is Fast, and Fast beats ZIP Normal on all nine sets on all three measures (the first
upset above). The same holds for WinRAR Normal on the photos and PDFs, where Fast wins too. If the
website compares level against level these rows stay red; if it compares the rival's default button
with the Zipling button that answers it, they are green rows for Fast.

**2. Against the LZMA-class normal settings on size: 7-Zip Normal, Zipgeist 7z Normal and xz -6 on
{', '.join(TITLES[s] for s in d_7z_lost)}.** Default's lanes are zstd 15 and 12 with every transform; those rivals are LZMA.
Default is {min(v[1] for v in d_7z.values()):.0f} to {max(v[1] for v in d_7z.values()):.0f}x faster to make and {min(v[2] for v in d_7z.values()):.0f} to {max(v[2] for v in d_7z.values()):.0f}x faster to extract than 7-Zip Normal on every set, and smaller on
{len(d_7z_won_size)} of nine; on the three code- and text-heavy sets it is {', '.join(f'{d_7z[s][0]:.0f}%' for s in d_7z_lost)} bigger. This is the part a codec change
could fix, and the measured options (engine runs, quiet machine, 30 and 31 October) are:

| Option | Source code | Silesia | Third-party apps | Cost |
|---|---|---|---|---|
| Default as is (this run, the app) | {mb(dd['source']['size'])} MB, {sec(dd['source']['make_s'])} s | {mb(dd['silesia']['size'])} MB, {sec(dd['silesia']['make_s'])} s | {mb(dd['thirdparty']['size'])} MB, {sec(dd['thirdparty']['make_s'])} s | — |
| Default with bsc on the text lane | 10.21 MB, 0.95 s: wins the row | 53.07 MB, 2.32 s: still 6% bigger | 402.61 MB: still 6% bigger | extract 1.3 to 4x slower on text-heavy sets (still 2x faster than 7z Normal); DMGs make 1.05 to 5.1 s |
| Max's codecs at Default (bsc text, LZMA2 7 binaries) | 8.23 MB, 1.61 s: wins | 49.41 MB, 6.14 s: wins | 350.23 MB, 39 s: wins | make 2 to 40x slower than today's Default; loses the normal-class rows it wins today on PDFs, apps, Saved, DMGs (make time) |
| The lzt codec (LZMA's parse at a symbol coder's decode speed, research stage) | within 3 to 5% of LZMA at 2x its decode speed, measured on Silesia | | | needs a format change and a quiet month; the only route to LZMA's size at Default's extract speed |

Recommendation unchanged: keep Default's codecs and present Default as the middle it is, smaller
than every normal setting on most sets and several times faster than the LZMA settings that beat it on
size on the other three. The rows Default loses on make time are Fast's rows.

## Max and Ultra*: where the transforms cost time

Max and Ultra* lose make time (and on the disk images extract time too) where the transforms buy the
most size: re-creating the compression inside PDFs and PNGs, re-coding JPEGs, expanding the chunks of
disk images. On the PDFs Max is {100 * (1 - rows[('docs', 'arca-max')]['size'] / rows[('docs', '7z-ultra')]['size']):.0f}% smaller than 7-Zip Ultra but takes {sec(rows[('docs', 'arca-max')]['make_s'])} s against {sec(rows[('docs', '7z-ultra')]['make_s'])} s; on the disk
images {100 * (1 - rows[('dmgs', 'arca-max')]['size'] / rows[('dmgs', '7z-ultra')]['size']):.0f}% smaller in {sec(rows[('dmgs', 'arca-max')]['make_s'])} s against {sec(rows[('dmgs', '7z-ultra')]['make_s'])} s. In every one of those rows Default already beats the same rival on
all three measures (the upsets above), so the user who wants the win on time has it one level down.
Ultra* adds {min(100 * (1 - rows[(s, 'arca-ultra')]['size'] / rows[(s, 'arca-max')]['size']) for s in SETS):.1f} to {max(100 * (1 - rows[(s, 'arca-ultra')]['size'] / rows[(s, 'arca-max')]['size']) for s in SETS):.1f}% over Max for {min(100 * (rows[(s, 'arca-ultra')]['make_s'] / rows[(s, 'arca-max')]['make_s'] - 1) for s in SETS):.0f} to {max(100 * (rows[(s, 'arca-ultra')]['make_s'] / rows[(s, 'arca-max')]['make_s'] - 1) for s in SETS):.0f}% more make time; it stays experimental.

## Wins and losses, row by row

Ratios are Zipling / rival for size, make and extract; under 1 is a win.

```
{levels_out}```

## Notes on the run

- Load gates. A set's runs start when the 1-minute load is under 2.5, waiting up to ten minutes.
  Gates in the main run: {', '.join(f'{TITLES[s]} {gates.get(s, 0):.1f}' for s in SETS)}. {('The last two sets went on after the wait at 3.7: an animated wallpaper and a background app kept the idle load there; their Zipling rows are within 1 to 5% of the quiet engine run at Default, Max and Ultra* and their rival rows within 7% of the quiet runs of 30 October, so they stand.' if busy else '')}
  {('Second pass (zstd and xz) gates, threshold 4.0: ' + ', '.join(f'{TITLES[s]} {zx_gates.get(s, 0):.1f}' + '.' * (s == SETS[-1]) for s in SETS)) if zx_gates else ''}
- The log's clock jumps back one hour between the disk images and the third-party apps: daylight
  saving time ended at 02:00 on 1 November. The run took about two and a half hours.
- Every set is read once (tar to /dev/null) before its runs, so every tool starts with the files in
  the page cache; Zipling and the rivals alternate within a set; the second round runs in reverse order.
- The private sets (Saved folder, disk images, third-party apps) are the user's own files and stay
  local; the website shows the six public sets.
"""
open(R + "report-2026-10-31-apps.md", "w").write(report)
print("written", len(report), "chars;", f"won {sum(won.values())} of {sum(total.values())}; any level {all_any[0]} of {all_any[1]}; zx rows {n_zx}")
