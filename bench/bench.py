#!/usr/bin/env python3
"""Head-to-head archiver benchmark.

Usage: bench.py --arca PATH --work DIR DATASET_DIR [DATASET_DIR ...]

For every dataset and tool: compress (wall time), size, extract (wall time),
and a `diff -r` check that the extracted tree matches the source.
Results are printed as a Markdown table and written to DIR/results.json.
"""

import argparse
import json
import os
import shlex
import shutil
import subprocess
import sys
import time


def tools(arca):
    a = shlex.quote(arca)
    # (name, compress cmd, extract cmd); {src} {parent} {name} {out} {dest}
    return [
        ("Finder zip (ditto)", "ditto -c -k --sequesterRsrc --keepParent {src} {out}.zip",
         "ditto -x -k {out}.zip {dest}", ".zip"),
        ("7-Zip ultra (7z -mx9)", "7zz a -t7z -mx=9 -bd -bso0 -bsp0 {out}.7z {src}",
         "7zz x -bd -bso0 -bsp0 -o{dest} {out}.7z", ".7z"),
        ("tar.zst -19 --long", "tar -cf - -C {parent} {name} | zstd -19 --long=27 -T0 -q -o {out}.tar.zst",
         "zstd -dc --long=27 -q {out}.tar.zst | tar -xf - -C {dest}", ".tar.zst"),
        ("tar.xz -9e", "tar -cf - -C {parent} {name} | xz -9e -T0 > {out}.tar.xz",
         "xz -dc -T0 {out}.tar.xz | tar -xf - -C {dest}", ".tar.xz"),
        ("Arca fast", f"{a} pack {{out}}.arca {{src}} -l fast >/dev/null",
         f"{a} unpack {{out}}.arca -o {{dest}} >/dev/null", ".arca"),
        ("Arca balanced", f"{a} pack {{out}}.arca {{src}} -l balanced >/dev/null",
         f"{a} unpack {{out}}.arca -o {{dest}} >/dev/null", ".arca"),
        ("Arca max", f"{a} pack {{out}}.arca {{src}} -l max >/dev/null",
         f"{a} unpack {{out}}.arca -o {{dest}} >/dev/null", ".arca"),
        ("Arca ultra", f"{a} pack {{out}}.arca {{src}} -l ultra >/dev/null",
         f"{a} unpack {{out}}.arca -o {{dest}} >/dev/null", ".arca"),
    ]


def du(path):
    total = 0
    for root, _, files in os.walk(path):
        for f in files:
            p = os.path.join(root, f)
            if not os.path.islink(p):
                total += os.path.getsize(p)
    return total


def run(cmd):
    t = time.perf_counter()
    r = subprocess.run(cmd, shell=True, capture_output=True, text=True)
    dt = time.perf_counter() - t
    if r.returncode != 0:
        raise RuntimeError(f"command failed ({r.returncode}): {cmd}\n{r.stderr[-2000:]}")
    return dt


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--arca", required=True)
    ap.add_argument("--work", required=True)
    ap.add_argument("--only", help="comma-separated tool names to run")
    ap.add_argument("datasets", nargs="+")
    args = ap.parse_args()

    os.makedirs(args.work, exist_ok=True)
    selected = tools(os.path.abspath(args.arca))
    if args.only:
        keep = [s.strip() for s in args.only.split(",")]
        selected = [t for t in selected if t[0] in keep]

    results = []
    for ds in args.datasets:
        src = os.path.abspath(ds)
        parent, name = os.path.split(src)
        raw = du(src)
        # Warm the page cache so every tool starts from the same state.
        run(f"tar -cf - -C {shlex.quote(parent)} {shlex.quote(name)} > /dev/null")
        print(f"\n## {name}  ({raw / 1e6:.1f} MB)\n", flush=True)
        print("| Tool | Size (MB) | % of original | Compress (s) | MB/s | Extract (s) | MB/s | Round trip |")
        print("|---|---:|---:|---:|---:|---:|---:|---|", flush=True)
        for tool, ccmd, xcmd, ext in selected:
            out = os.path.join(args.work, f"{name}-{tool.split()[0].lower()}-{len(results)}")
            dest = out + ".x"
            for p in (out + ext, dest):
                if os.path.isdir(p):
                    shutil.rmtree(p)
                elif os.path.exists(p):
                    os.remove(p)
            os.makedirs(dest)
            fmt = dict(src=shlex.quote(src), parent=shlex.quote(parent), name=shlex.quote(name),
                       out=shlex.quote(out), dest=shlex.quote(dest))
            try:
                ct = run(ccmd.format(**fmt))
                size = os.path.getsize(out + ext)
                xt = run(xcmd.format(**fmt))
                same = subprocess.run(["diff", "-rq", src, os.path.join(dest, name)],
                                      capture_output=True).returncode == 0
            except RuntimeError as e:
                print(f"| {tool} | failed | | | | | | {str(e).splitlines()[0][:60]} |", flush=True)
                continue
            row = dict(dataset=name, raw=raw, tool=tool, size=size, compress_s=ct, extract_s=xt, identical=same)
            results.append(row)
            print(f"| {tool} | {size / 1e6:.2f} | {100 * size / raw:.1f}% | {ct:.2f} | {raw / 1e6 / ct:.0f} | "
                  f"{xt:.2f} | {raw / 1e6 / xt:.0f} | {'identical' if same else 'MISMATCH'} |", flush=True)
            shutil.rmtree(dest, ignore_errors=True)
            os.remove(out + ext)

    with open(os.path.join(args.work, "results.json"), "w") as f:
        json.dump(results, f, indent=1)


if __name__ == "__main__":
    sys.exit(main())
