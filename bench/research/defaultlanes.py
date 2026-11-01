#!/usr/bin/env python3
"""Default with a stronger codec on one lane at a time, on the sets where Default
loses to the LZMA normal settings on size (source, Silesia, Apple apps, third-party
apps) and on the mixed folder, the website's headline set, for the collateral.

    python3 -I bench/research/defaultlanes.py <work dir> <results.jsonl> [set ...] [--only name,name]

Every candidate is the Default preset plus research flags; the engine's own clock
times the pack (`--json` seconds, what the app shows) and the unpack. Two rounds,
the order reversed in the second; round 1 compares the round trip with the source.
Rows look like checkpoint.py's, so checkpoint_report.load and levels.py read them.
"""

import json
import os
import shlex
import shutil
import subprocess
import sys
import time

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
from checkpoint import ARCA, SETS, same, timed  # noqa: E402

CODE = "ARCA_EXP_CODE_LANE=1"
MACHO = "ARCA_EXP_CODE_LANE=1 ARCA_EXP_MACHO_LANE=1"
# name: (environment, extra pack flags). The lane flags set the codec of one lane;
# the code lane is off at Default unless ARCA_EXP_CODE_LANE is set, and Mach-O joins
# it only with ARCA_EXP_MACHO_LANE.
CANDIDATES = {
    "base": ("", ""),
    # Text lane (zstd 15 today).
    "t19": ("", "--lane text=zstd:19"),
    "tbsc": ("", "--lane text=bsc:1"),
    # Code lane (static libraries, ELF, PE, tar): in the binary lane at zstd 12 today.
    "c12": (CODE, "--lane code=zstd:12"),
    "c19": (CODE, "--lane code=zstd:19"),
    "cbsc": (CODE, "--lane code=bsc:2"),
    "cbsce": (CODE, "--lane code=bsc:2e"),
    "clz7": (CODE, "--lane code=lzma2:7"),
    "clzt": (CODE, "--lane code=lzt:7"),
    # Mach-O too, in the code lane with the rest of the code.
    "m15": (MACHO, "--lane code=zstd:15"),
    "m19": (MACHO, "--lane code=zstd:19"),
    "mlz7": (MACHO, "--lane code=lzma2:7"),
    "mlzt": (MACHO, "--lane code=lzt:7"),
    # The whole binary lane (zstd 12 today).
    "b15": ("", "--lane binary=zstd:15"),
    "b19": ("", "--lane binary=zstd:19"),
    # Round 2: bsc on the code lane with the binary lane's zstd 12 as its fallback
    # (static libraries stay with bsc, x86 tars don't), alone and with bsc text.
    "cz": (CODE + " ARCA_EXP_BSC_FALLBACK=zstd:12", "--lane code=bsc:2e"),
    "tcz": (CODE + " ARCA_EXP_BSC_FALLBACK=zstd:12", "--lane text=bsc:1 --lane code=bsc:2e"),
    # Round 3: the same with smaller text blocks, for bsc's extraction time (more
    # blocks decode on more cores) against the size they cost.
    "tcz2": (CODE + " ARCA_EXP_BSC_FALLBACK=zstd:12", "--lane text=bsc:1 --lane code=bsc:2e --text-block-mib 2"),
    "tcz4": (CODE + " ARCA_EXP_BSC_FALLBACK=zstd:12", "--lane text=bsc:1 --lane code=bsc:2e --text-block-mib 4"),
    "tcz8": (CODE + " ARCA_EXP_BSC_FALLBACK=zstd:12", "--lane text=bsc:1 --lane code=bsc:2e --text-block-mib 8"),
}
DEFAULT_SETS = ["source", "silesia", "apps", "thirdparty", "mixed"]


def main():
    args = sys.argv[1:]
    only = None
    if "--only" in args:
        i = args.index("--only")
        only = args[i + 1].split(",")
        del args[i:i + 2]
    work, results = args[0], args[1]
    names = args[2:] or DEFAULT_SETS
    cands = [(n, CANDIDATES[n]) for n in (only or CANDIDATES)]
    os.makedirs(work, exist_ok=True)
    progress = open(results + ".log", "a")

    def log(msg):
        progress.write(time.strftime("%H:%M:%S ") + msg + "\n")
        progress.flush()

    log(f"arca {ARCA}; {len(cands)} candidates: {' '.join(n for n, _ in cands)}")
    for set_name in names:
        src = SETS[set_name]
        parent, name = os.path.split(src)
        subprocess.run(f"tar -cf - -C {shlex.quote(parent)} {shlex.quote(name)} > /dev/null", shell=True)
        log(f"{set_name}: start (load {os.getloadavg()[0]:.1f})")
        for rnd in (1, 2):
            for cand, (env, flags) in (cands if rnd == 1 else cands[::-1]):
                out = os.path.join(work, "out.zlg")
                dest = os.path.join(work, "x")
                for p in (out, dest):
                    if os.path.isdir(p):
                        shutil.rmtree(p)
                    elif os.path.exists(p):
                        os.remove(p)
                os.makedirs(dest)
                stats = os.path.join(work, "stats.json")
                make = f"{env} {ARCA} pack {shlex.quote(out)} {shlex.quote(src)} -l balanced {flags} --json > {shlex.quote(stats)}"
                extract = f"{ARCA} unpack {shlex.quote(out)} -o {shlex.quote(dest)} --json > {shlex.quote(stats)}"
                rec = {"set": set_name, "tool": f"arca-balanced-{cand}", "round": rnd, "load": round(os.getloadavg()[0], 2), "flags": f"{env} {flags}".strip()}
                try:
                    rec["make_s_wall"], rec["make_cpu"], rec["make_mb"] = timed(make)
                    rec["make_s"] = json.load(open(stats))["seconds"]
                    rec["size"] = os.path.getsize(out)
                    rec["extract_s_wall"], rec["extract_cpu"], rec["extract_mb"] = timed(extract)
                    rec["extract_s"] = json.load(open(stats))["seconds"]
                    if rnd == 1:
                        rec["identical"] = same(src, os.path.join(dest, name))
                except Exception as e:  # noqa: BLE001 - keep going, record the failure
                    rec["error"] = str(e)[-800:]
                with open(results, "a") as f:
                    f.write(json.dumps(rec) + "\n")
                log(f"{set_name} r{rnd} {cand}: {rec.get('size')} {rec.get('make_s', 0):.2f}s / {rec.get('extract_s', 0):.2f}s"
                    + ("" if rec.get("identical", True) else " MISMATCH") + ("" if "error" not in rec else " ERROR"))
        shutil.rmtree(os.path.join(work, "x"), ignore_errors=True)
    log("done")


if __name__ == "__main__":
    main()
