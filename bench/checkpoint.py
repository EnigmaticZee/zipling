#!/usr/bin/env python3
"""Checkpoint benchmark (the user's spec of 2026-10-29, docs/RESEARCH.md).

Every Arca level with and without bsc, and every command-line rival, on every
set; two rounds, Arca and rivals alternating, the second round in reverse order.
Each run records size, wall and CPU time and peak memory for making and
extracting; the first round also checks the extracted tree against the source
(names, types, bytes and link targets, links not followed).

    python3 -I bench/checkpoint.py <work dir> <results.jsonl> [set or folder ...]

A folder instead of a set name runs on that folder (named after it); with
CHECKPOINT_NO_WAIT set it doesn't wait for a quiet Mac (for a dry run).
CHECKPOINT_RAR names RAR's command-line `rar` (adds WinRAR's settings);
CHECKPOINT_TOOLS, a comma-separated list, keeps only those tools.

Results are appended one JSON line per run, so a stopped run keeps what it did.
Before each set it waits (up to 10 minutes) for the 1-minute load to drop below
2.5, so other work on the Mac doesn't land in the numbers.
"""

import filecmp
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ARCA = shlex.quote(os.path.join(ROOT, "engine/target/release/arca"))
BENCH = "/Users/zee/Documents/Building/Tools/Zip Project/research_notes/engine-bench.noindex"
SETS = {
    "jpeg": f"{BENCH}/sets/jpeg",
    "docs": f"{BENCH}/sets/docs",
    "source": f"{BENCH}/sets/source",
    "apps": f"{BENCH}/sets/apps",
    "silesia": f"{BENCH}/sets/silesia",
    "saved": f"{BENCH}/Mixed",
    "mixed": f"{BENCH}/sets/mixed",
    "dmgs": f"{BENCH}/sets/dmgs",
    "thirdparty": f"{BENCH}/sets/thirdparty",
}
BSC = "--lane text=bsc:1 --lane code=bsc:2e"


def arca_tools(set_name):
    tools = []
    for level in ["fast", "balanced", "max", "ultra"]:
        for bsc in [False, True]:
            extra = f" {BSC}" if bsc else ""
            tools.append((f"arca-{level}{'-bsc' if bsc else ''}", f"{ARCA} pack {{out}} {{src}} -l {level}{extra}"))
    if set_name == "dmgs":
        for level in ["fast", "balanced"]:
            tools.append((f"arca-{level}-dmg", f"{ARCA} pack {{out}} {{src}} -l {level} --disk-images on"))
    return [(name, make, f"{ARCA} unpack {{out}} -o {{dest}}", ".zlg") for name, make in tools]


# The website's rivals (bench/bench.py) plus 7-Zip normal, zstd -3 and xz -6.
# 7-Zip stores symbolic links as links (-snl), as Keka does, and restores links
# that point through other links (-snld20; app frameworks are full of them).
RIVALS = [
    ("finder-zip", "ditto -c -k --sequesterRsrc --keepParent {src} {out}", "ditto -x -k {out} {dest}", ".zip"),
    ("7z-normal", "7zz a -t7z -mx=5 -snl -bd -bso0 -bsp0 {out} {src}", "7zz x -snld20 -bd -bso0 -bsp0 -o{dest} {out}", ".7z"),
    ("7z-ultra", "7zz a -t7z -mx=9 -snl -bd -bso0 -bsp0 {out} {src}", "7zz x -snld20 -bd -bso0 -bsp0 -o{dest} {out}", ".7z"),
    ("zstd-3", "tar -cf - -C {parent} {name} | zstd -3 -T0 -q -o {out}", "zstd -dc -q {out} | tar -xf - -C {dest}", ".tar.zst"),
    ("zstd-19-long", "tar -cf - -C {parent} {name} | zstd -19 --long=27 -T0 -q -o {out}",
     "zstd -dc --long=27 -q {out} | tar -xf - -C {dest}", ".tar.zst"),
    ("xz-6", "tar -cf - -C {parent} {name} | xz -6 -T0 > {out}", "xz -dc -T0 {out} | tar -xf - -C {dest}", ".tar.xz"),
    ("xz-9e", "tar -cf - -C {parent} {name} | xz -9e -T0 > {out}", "xz -dc -T0 {out} | tar -xf - -C {dest}", ".tar.xz"),
]


# WinRAR (RAR 7.23 for macOS): fastest, normal (its default) and best with a
# solid archive; links kept as links (-ol); all cores by default.
RAR = os.environ.get("CHECKPOINT_RAR")
if RAR:
    rar = shlex.quote(RAR)
    for name, opts in [("rar-fastest", "-m1"), ("rar-normal", "-m3"), ("rar-best", "-m5 -s")]:
        RIVALS.append((name, f"cd {{parent}} && {rar} a {opts} -ol -idq {{out}} {{name}}",
                       f"{rar} x -ol -idq -o+ {{out}} {{dest}}/", ".rar"))


def timed(cmd):
    """Runs a shell command; returns (wall s, CPU s, peak MB) or raises."""
    t = time.perf_counter()
    r = subprocess.run(["/usr/bin/time", "-l", "sh", "-c", cmd], capture_output=True, text=True)
    wall = time.perf_counter() - t
    if r.returncode != 0:
        raise RuntimeError(f"exit {r.returncode}: {cmd}\n{r.stderr[-1500:]}")
    m = re.search(r"([\d.]+) real\s+([\d.]+) user\s+([\d.]+) sys", r.stderr)
    rss = re.search(r"(\d+)\s+maximum resident set size", r.stderr)
    cpu = float(m.group(2)) + float(m.group(3)) if m else None
    return wall, cpu, int(rss.group(1)) / 1048576 if rss else None


def same(a, b):
    """Same names, types, file bytes and link targets (links not followed)."""
    def walk(root):
        out = {}
        for d, dirs, files in os.walk(root):
            for n in dirs + files:
                p = os.path.join(d, n)
                out[os.path.relpath(p, root)] = "l" if os.path.islink(p) else "d" if os.path.isdir(p) else "f"
        return out
    ta, tb = walk(a), walk(b)
    if ta != tb:
        return False
    for r, t in ta.items():
        pa, pb = os.path.join(a, r), os.path.join(b, r)
        if t == "l" and os.readlink(pa) != os.readlink(pb):
            return False
        if t == "f" and not filecmp.cmp(pa, pb, shallow=False):
            return False
    return True


def wait_quiet(log):
    if os.environ.get("CHECKPOINT_NO_WAIT"):
        return os.getloadavg()[0]
    start = time.time()
    while time.time() - start < 600:
        load = os.getloadavg()[0]
        if load < 2.5:
            return load
        time.sleep(15)
    log(f"still busy after 10 min (load {os.getloadavg()[0]:.1f}); going on")
    return os.getloadavg()[0]


def main():
    work, results = sys.argv[1], sys.argv[2]
    names = sys.argv[3:] or list(SETS)
    os.makedirs(work, exist_ok=True)
    progress = open(results + ".log", "a")

    def log(msg):
        progress.write(time.strftime("%H:%M:%S ") + msg + "\n")
        progress.flush()

    for set_name in names:
        src = SETS.get(set_name, os.path.abspath(set_name))
        parent, name = os.path.split(src)
        set_name = set_name if set_name in SETS else name
        arca, rivals = arca_tools(set_name), RIVALS
        keep = os.environ.get("CHECKPOINT_TOOLS")
        if keep:
            keep = set(keep.split(","))
            arca, rivals = [t for t in arca if t[0] in keep], [t for t in rivals if t[0] in keep]
        # Arca and rivals alternate.
        order = []
        for i in range(max(len(arca), len(rivals))):
            order += arca[i:i + 1] + rivals[i:i + 1]
        load = wait_quiet(log)
        subprocess.run(f"tar -cf - -C {shlex.quote(parent)} {shlex.quote(name)} > /dev/null", shell=True)
        log(f"{set_name}: start (load {load:.1f}), {len(order)} tools x 2 rounds")
        for rnd in (1, 2):
            for tool, make, extract, ext in (order if rnd == 1 else order[::-1]):
                out = os.path.join(work, "out" + ext)
                dest = os.path.join(work, "x")
                for p in (out, dest):
                    if os.path.isdir(p):
                        shutil.rmtree(p)
                    elif os.path.exists(p):
                        os.remove(p)
                os.makedirs(dest)
                fmt = dict(src=shlex.quote(src), parent=shlex.quote(parent), name=shlex.quote(name),
                           out=shlex.quote(out), dest=shlex.quote(dest))
                rec = {"set": set_name, "tool": tool, "round": rnd, "load": round(os.getloadavg()[0], 2)}
                try:
                    rec["make_s"], rec["make_cpu"], rec["make_mb"] = timed(make.format(**fmt))
                    rec["size"] = os.path.getsize(out)
                    rec["extract_s"], rec["extract_cpu"], rec["extract_mb"] = timed(extract.format(**fmt))
                    if rnd == 1:
                        rec["identical"] = same(src, os.path.join(dest, name))
                except Exception as e:  # noqa: BLE001 - keep going, record the failure
                    rec["error"] = str(e)[-800:]
                with open(results, "a") as f:
                    f.write(json.dumps(rec) + "\n")
                log(f"{set_name} r{rnd} {tool}: {rec.get('size')} {rec.get('make_s', 0):.2f}s / {rec.get('extract_s', 0):.2f}s"
                    + ("" if "error" not in rec else " ERROR"))
        for p in (os.path.join(work, "x"),):
            shutil.rmtree(p, ignore_errors=True)
    log("done")


if __name__ == "__main__":
    main()
