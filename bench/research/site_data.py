#!/usr/bin/env python3
"""Prints the `datasets` block of site/data.js from the app benchmark: Zipling, 7-Zip
and WinRAR rows from results-2026-10-31-apps.jsonl, zstd and xz rows from the second
pass with the Homebrew binaries (results-2026-10-31-apps-zx.jsonl) when it exists,
Finder from results-2026-10-31-apps-finder.jsonl. Best of two rounds per tool."""
import json
import os

R = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")) + "/"
ZX = {"zstd-3", "zstd-19-long", "xz-6", "xz-9e"}
IDS = [("finder", "finder-app"), ("zst3", "zstd-3"), ("rarf", "rar-fastest"), ("fast", "arca-fast"),
       ("7zn", "7z-normal"), ("rarn", "rar-normal"), ("xz6", "xz-6"), ("balanced", "arca-balanced"),
       ("7z", "7z-ultra"), ("rar", "rar-best"), ("zst", "zstd-19-long"), ("xz", "xz-9e"), ("max", "arca-max"), ("ultra", "arca-ultra")]
SITE = [("mixed", "Mixed folder", 736, "All five sets below in one folder", "A realistic \\\"compress this folder\\\" job"),
        ("docs", "PDFs & screenshots", 89, "250 PDFs and 600 PNGs from system and Xcode resources", "Office, PDF and screenshot-heavy folders"),
        ("jpeg", "Photos", 98, "400 JPEGs from system and Xcode resources", "Photo folders"),
        ("apps", "Mac apps", 174, "8 apps from /System/Applications: universal Mach-O, asset catalogs, nibs, strings", "Software and .app bundles"),
        ("source", "Source code", 162, "89 Rust crates, including vendored C (zstd, xz)", "Code repositories"),
        ("silesia", "Generic data", 212, "The standard Silesia compression corpus", "Data with no file-type tricks to exploit")]


def load(path, keep=None, drop=None):
    best = {}
    if not os.path.exists(path):
        return best
    for line in open(path):
        r = json.loads(line)
        if "size" not in r or r.get("identical") is False:
            continue
        if keep and r["tool"] not in keep or drop and r["tool"] in drop:
            continue
        k = (r["set"], r["tool"])
        b = best.get(k)
        if b is None:
            best[k] = dict(r)
        else:
            b["make_s"], b["extract_s"] = min(b["make_s"], r["make_s"]), min(b["extract_s"], r["extract_s"])
            b["size"] = max(b["size"], r["size"])  # rounds can differ by 0.05-0.3%: the larger counts
    return best


zx = load(R + "results-2026-10-31-apps-zx.jsonl", keep=ZX)
rows = load(R + "results-2026-10-31-apps.jsonl")
rows.update(zx)
rows.update(load(R + "results-2026-10-31-apps-finder.jsonl"))
# Default again on 1 November, in the app rebuilt with the engine's code lane.
default = load(R + "results-2026-11-01-apps-default.jsonl", keep={"arca-balanced"})
rows.update(default)
stale = [(s, t) for s, *_ in SITE for t in ZX if (s, t) not in zx]
if default and len(default) < 9:
    print("WARNING: the Default re-run has only", len(default), "sets")
if stale:
    print("WARNING: first-pass zstd/xz rows still in use for", stale)


def t(v):
    return f"{v:.2f}" if v < 1 else f"{v:.1f}"


def cell(s, tool):
    r = rows[(s, tool)]
    return f"[{r['size'] / 1e6:.1f}, {t(r['make_s'])}, {r['extract_s']:.2f}]"


print("  datasets: [")
for s, name, mb, what, why in SITE:
    c = {i: cell(s, tool) for i, tool in IDS}
    q = lambda i: f'"{i}"' if i[0].isdigit() else i  # noqa: E731
    print(f'    {{ id: "{s}", name: "{name}", mb: {mb}, what: "{what}", why: "{why}",')
    print(f"      r: {{ finder: {c['finder']}, zst3: {c['zst3']}, rarf: {c['rarf']}, fast: {c['fast']},")
    print(f"           \"7zn\": {c['7zn']}, rarn: {c['rarn']}, xz6: {c['xz6']}, balanced: {c['balanced']},")
    print(f"           \"7z\": {c['7z']}, rar: {c['rar']}, zst: {c['zst']}, xz: {c['xz']}, max: {c['max']}, ultra: {c['ultra']} }} }},")
print("  ],")
print()
print("second pass (Homebrew zstd/xz) rows present:", len(zx))
# the numbers the pages quote
m = {tool: rows[("mixed", tool)] for _, tool in IDS}
d = {tool: rows[("docs", tool)] for _, tool in IDS}
print(f"mixed: Default {m['arca-balanced']['size']/1e6:.1f} MB {m['arca-balanced']['make_s']:.1f} s / {m['arca-balanced']['extract_s']:.2f} s;"
      f" Max {m['arca-max']['size']/1e6:.1f} MB {m['arca-max']['make_s']:.1f} s / {m['arca-max']['extract_s']:.2f} s;"
      f" Fast {m['arca-fast']['size']/1e6:.1f} MB {m['arca-fast']['make_s']:.2f} s / {m['arca-fast']['extract_s']:.2f} s;"
      f" 7z Ultra {m['7z-ultra']['size']/1e6:.1f} MB {m['7z-ultra']['make_s']:.1f} s / {m['7z-ultra']['extract_s']:.2f} s;"
      f" Finder {m['finder-app']['size']/1e6:.1f} MB {m['finder-app']['make_s']:.1f} s / {m['finder-app']['extract_s']:.2f} s;"
      f" 7z Normal {m['7z-normal']['size']/1e6:.1f}; WinRAR Normal {m['rar-normal']['size']/1e6:.1f}")
print(f"  Default vs 7z Ultra: {100*(1-m['arca-balanced']['size']/m['7z-ultra']['size']):.1f}% smaller, make {m['7z-ultra']['make_s']/m['arca-balanced']['make_s']:.1f}x faster, extract {m['7z-ultra']['extract_s']/m['arca-balanced']['extract_s']:.1f}x faster; vs Finder extract {m['finder-app']['extract_s']/m['arca-balanced']['extract_s']:.1f}x")
print(f"  Max vs 7z Ultra: {100*(1-m['arca-max']['size']/m['7z-ultra']['size']):.1f}% smaller, make {100*m['arca-max']['make_s']/m['7z-ultra']['make_s']:.0f}% of the time, extract {m['7z-ultra']['extract_s']/m['arca-max']['extract_s']:.1f}x")
print(f"  Max vs Fast size: {100*(1-m['arca-max']['size']/m['arca-fast']['size']):.1f}% smaller; convert scene: zip {m['finder-app']['size']/1e6:.0f} 7z {m['7z-normal']['size']/1e6:.0f} rar {m['rar-normal']['size']/1e6:.0f} zlg {m['arca-balanced']['size']/1e6:.0f}"
      f" -> {100*(1-m['arca-balanced']['size']/m['finder-app']['size']):.0f}% {100*(1-m['arca-balanced']['size']/m['7z-normal']['size']):.0f}% {100*(1-m['arca-balanced']['size']/m['rar-normal']['size']):.0f}% smaller")
print(f"docs: Max {d['arca-max']['size']/1e6:.1f} MB in {d['arca-max']['make_s']:.1f} s vs 7z Ultra {d['7z-ultra']['size']/1e6:.1f} MB in {d['7z-ultra']['make_s']:.1f} s: {100*(1-d['arca-max']['size']/d['7z-ultra']['size']):.0f}% smaller;"
      f" Default {d['arca-balanced']['size']/1e6:.1f} MB {d['arca-balanced']['make_s']:.1f} s: {100*(1-d['arca-balanced']['size']/d['7z-ultra']['size']):.0f}% smaller, {d['7z-ultra']['make_s']/d['arca-balanced']['make_s']:.1f}x faster")
for s in ("apps", "source", "silesia"):
    r = {tool: rows[(s, tool)] for _, tool in IDS}
    print(f"{s}: Default vs 7z Ultra size {100*(r['arca-balanced']['size']/r['7z-ultra']['size']-1):+.0f}%, vs 7z Normal {100*(r['arca-balanced']['size']/r['7z-normal']['size']-1):+.0f}%,"
          f" make {r['7z-ultra']['make_s']/r['arca-balanced']['make_s']:.0f}x/{r['7z-normal']['make_s']/r['arca-balanced']['make_s']:.0f}x faster, extract {r['7z-ultra']['extract_s']/r['arca-balanced']['extract_s']:.0f}x/{r['7z-normal']['extract_s']/r['arca-balanced']['extract_s']:.0f}x;"
          f" Max vs 7z Ultra {100*(1-r['arca-max']['size']/r['7z-ultra']['size']):.1f}% smaller, make {r['arca-max']['make_s']:.1f} vs {r['7z-ultra']['make_s']:.1f} s, extract {r['arca-max']['extract_s']:.2f} vs {r['7z-ultra']['extract_s']:.2f}")
r = {tool: rows[("silesia", tool)] for _, tool in IDS}
print(f"silesia Fast vs zstd -3: make {r['arca-fast']['make_s']:.2f} vs {r['zstd-3']['make_s']:.2f} s, size {100*(1-r['arca-fast']['size']/r['zstd-3']['size']):.1f}% smaller, extract {r['arca-fast']['extract_s']:.2f} vs {r['zstd-3']['extract_s']:.2f}")
ua = [100 * (1 - rows[(s, 'arca-ultra')]['size'] / rows[(s, 'arca-max')]['size']) for s, *_ in SITE]
ut = [100 * (rows[(s, 'arca-ultra')]['make_s'] / rows[(s, 'arca-max')]['make_s'] - 1) for s, *_ in SITE]
print(f"Ultra vs Max: {min(ua):.1f} to {max(ua):.1f}% smaller for {min(ut):.0f} to {max(ut):.0f}% more make time")
