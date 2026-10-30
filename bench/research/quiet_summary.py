"""Summary of the quiet-Mac Fast benchmark: best of the rounds per tool and set,
with Zipgeist's checkpoint figures (adjusted for its session, as in the
checkpoint report) alongside."""
import json
import re
import sys

res, ck_md = sys.argv[1], sys.argv[2]
rows = [json.loads(l) for l in open(res)]
best = {}
for r in rows:
    if "error" in r:
        best[(r["set"], r["tool"])] = None
        continue
    k = (r["set"], r["tool"])
    cur = best.get(k)
    v = dict(size=r["size"], make=r["make_s"], ext=r["extract_s"], mk_mb=r.get("make_mb"), ex_mb=r.get("extract_mb"),
             identical=r.get("identical", (cur or {}).get("identical")), load=r["load"])
    if cur is None:
        best[k] = v
    else:
        cur["make"] = min(cur["make"], v["make"])
        cur["ext"] = min(cur["ext"], v["ext"])
        if v["identical"] is not None:
            cur["identical"] = v["identical"]
        cur["load"] = max(cur["load"], v["load"])

# Zipgeist rows from the checkpoint report's per-set tables (already adjusted, marked †)
md = open(ck_md).read()
zg = {}
names = {"Photos": "jpeg", "saved folder": "saved", "website": "mixed"}
for sec in re.split(r"\n## ", md):
    title = sec.split("\n", 1)[0]
    set_name = next((v for k, v in names.items() if k.lower() in title.lower()), None)
    if not set_name:
        continue
    for line in sec.splitlines():
        if line.startswith("| Zipgeist"):
            cells = [c.strip() for c in line.strip("|").split("|")]
            zg.setdefault(set_name, []).append((cells[0], float(cells[1].replace(",", "")), cells[2], cells[3]))

label = {"arca-fast": "Arca Fast (light JPEG)", "zstd-3": "tar + zstd -3", "finder-zip": "Finder (ditto zip)",
         "7z-normal": "7-Zip normal (-mx5)", "rar-fastest": "WinRAR Fastest", "rar-normal": "WinRAR Normal"}
for s in ["jpeg", "saved", "mixed"]:
    print(f"\n### {s}")
    print("| Tool | Size MB | Make s | Extract s | Peak make / extract MB | Identical | Load |")
    print("|---|---:|---:|---:|---:|---|---:|")
    a = best.get((s, "arca-fast"))
    for t in label:
        v = best.get((s, t))
        if v is None:
            print(f"| {label[t]} | error | | | | | |")
            continue
        mark = ""
        if a and t != "arca-fast":
            ahead = [n for n, x, y in (("size", v["size"], a["size"]), ("make", v["make"], a["make"]), ("extract", v["ext"], a["ext"])) if x < y]
            mark = " ✗ " + ", ".join(ahead) if ahead else ""
        print(f"| {label[t]}{mark} | {v['size']/1e6:.1f} | {v['make']:.2f} | {v['ext']:.2f} | {v['mk_mb'] or 0:.0f} / {v['ex_mb'] or 0:.0f} | {v['identical']} | {v['load']:.1f} |")
    for n, size, mk, ex in zg.get(s, []):
        print(f"| {n} (checkpoint, †) | {size:.1f} | {mk} | {ex} | | | |")
