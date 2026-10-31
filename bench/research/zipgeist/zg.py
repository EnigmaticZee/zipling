#!/usr/bin/env python3
"""Bookkeeping for the Zipgeist runs of the checkpoint (driven through Finder).

  zg.py prep SET                      clone SET into zg/, open its Finder window, arm
  zg.py made SET TOOL ROUND EXT       record the compress run, move the source out, arm
  zg.py extracted SET TOOL ROUND EXT  record the extract run and check it, reset, arm
  zg.py arca SET TOOL ROUND           an Arca run from the command line, same session
  zg.py arm                           arm again (after a run that went wrong)

--skip N (any command): the stopwatch lets N left clicks go by before it starts the
clock, so the clicks that open the menu and the dialog can go in the same batch as
the one on Compress. made/extracted wait for the stopwatch.

Results go to bench/results-2026-10-30-zipgeist.jsonl in the repo.
"""

import filecmp, json, os, re, shutil, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
SCRATCH = os.path.dirname(HERE)
ZG, SRC = os.path.join(SCRATCH, "zg"), os.path.join(SCRATCH, "zgsrc")
REPO = "/Users/zee/Documents/Building/Tools/Zip Project/.claude/worktrees/arca-research-77726f"
RESULTS = os.path.join(REPO, "bench/results-2026-10-30-zipgeist.jsonl")
# the stopwatch also reports the click time, so a run ends no earlier than its output
# landed: a job that is mostly disk work can look idle to the CPU.
STOPWATCH = os.path.join(HERE, "stopwatch")
FINDER = os.path.join(HERE, "finder.sh")
SKIP = 0
PENDING = os.path.join(HERE, "pending.json")
ARM, RESULT = os.path.join(HERE, "arm"), os.path.join(HERE, "result.txt")
BENCH = "/Users/zee/Documents/Building/Tools/Zip Project/research_notes/engine-bench.noindex"
SETS = {"jpeg": "sets/jpeg", "docs": "sets/docs", "source": "sets/source", "apps": "sets/apps",
        "silesia": "sets/silesia", "saved": "Mixed", "mixed": "sets/mixed", "dmgs": "sets/dmgs",
        "thirdparty": "sets/thirdparty"}
ARCA = os.path.join(REPO, "engine/target/release/arca")
ARCA_ARGS = {"arca-fast": ["-l", "fast"], "arca-balanced": ["-l", "balanced"],
             "arca-max-bsc": ["-l", "max", "--lane", "text=bsc:1", "--lane", "code=bsc:2e"]}


def src_of(s):
    return os.path.join(BENCH, SETS[s])


def same(a, b):
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


def arm():
    subprocess.run(["pkill", "-f", "zgtools/stopwatch"])
    for p in (ARM, RESULT):
        if os.path.exists(p):
            os.remove(p)
    subprocess.Popen([STOPWATCH, "/Applications/Zipgeist.app/", ARM, RESULT, str(SKIP)],
                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
    time.sleep(0.3)
    open(ARM, "w").close()


def stopwatch(limit=1500):
    t = time.time()
    while not os.path.exists(RESULT) or not open(RESULT).read().strip():
        if time.time() - t > limit:
            print("still running after", limit, "s")
            sys.exit(2)
        time.sleep(0.2)
    return json.loads(open(RESULT).read().strip().splitlines()[-1])


def ended(sw, *paths):
    """Wall time: the click to the later of the last busy moment and the outputs' times."""
    if not sw.get("click"):
        return sw["wall"]
    landed = max(os.stat(p).st_mtime for p in paths)
    return max(sw["wall"], landed - sw["click"])


def clear(d):
    os.makedirs(d, exist_ok=True)
    for n in os.listdir(d):
        p = os.path.join(d, n)
        shutil.rmtree(p) if os.path.isdir(p) and not os.path.islink(p) else os.remove(p)


def main():
    global SKIP
    if "--skip" in sys.argv:
        i = sys.argv.index("--skip")
        SKIP = int(sys.argv[i + 1])
        del sys.argv[i:i + 2]
    cmd = sys.argv[1]
    if cmd == "arm":
        # right before the click that starts the job: the clicks that open
        # Zipgeist's dialog would otherwise start the stopwatch
        arm()
        print("armed")
        return
    s = sys.argv[2]
    name = os.path.basename(src_of(s))
    if cmd == "prep":
        clear(ZG), clear(SRC)
        subprocess.run(["cp", "-cR", src_of(s), ZG], check=True)
        subprocess.run([FINDER, ZG], check=True)
        time.sleep(1.0)
        arm()
        print("ready", os.listdir(ZG))
    elif cmd == "made":
        tool, rnd, ext = sys.argv[3], int(sys.argv[4]), sys.argv[5]
        sw = stopwatch()
        files = sorted(os.listdir(ZG))
        out = os.path.join(ZG, f"{name}.{ext}")
        if not os.path.exists(out):
            print("NO ARCHIVE", files)
            sys.exit(1)
        rec = {"set": s, "tool": tool, "round": rnd, "load": round(os.getloadavg()[0], 2),
               "make_s": ended(sw, out, ZG), "make_s_cpu_idle": sw["wall"], "make_cpu": sw["cpu"], "make_mb": sw["peak_mb"],
               "size": os.path.getsize(out)}
        json.dump(rec, open(PENDING, "w"))
        shutil.move(os.path.join(ZG, name), os.path.join(SRC, name))
        arm()
        print(f"made {tool} {rec['size']/1e6:.2f} MB in {rec['make_s']:.2f} s (idle-based {sw['wall']:.2f}) (cpu {sw['cpu']:.1f}, {sw['peak_mb']} MB); zg now {os.listdir(ZG)}")
    elif cmd == "extracted":
        sw = stopwatch()
        rec = json.load(open(PENDING))
        rec.update(extract_s=ended(sw, ZG), extract_s_cpu_idle=sw["wall"], extract_cpu=sw["cpu"], extract_mb=sw["peak_mb"],
                   identical=same(src_of(s), os.path.join(ZG, name)))
        with open(RESULTS, "a") as f:
            f.write(json.dumps(rec) + "\n")
        os.remove(PENDING)
        clear(ZG)
        shutil.move(os.path.join(SRC, name), os.path.join(ZG, name))
        arm()
        print(f"extracted {rec['tool']} in {rec['extract_s']:.2f} s (idle-based {sw['wall']:.2f}), identical={rec['identical']}; zg now {os.listdir(ZG)}")
    elif cmd == "arca":
        tool, rnd = sys.argv[3], int(sys.argv[4])
        work = os.path.join(SCRATCH, "zgarca")
        clear(work)
        out, dest = os.path.join(work, "o.zlg"), os.path.join(work, "x")

        def timed(argv):
            t = time.perf_counter()
            r = subprocess.run(["/usr/bin/time", "-l"] + argv, capture_output=True, text=True, check=True)
            wall = time.perf_counter() - t
            m = re.search(r"([\d.]+) real\s+([\d.]+) user\s+([\d.]+) sys", r.stderr)
            rss = re.search(r"(\d+)\s+maximum resident set size", r.stderr)
            return wall, float(m.group(2)) + float(m.group(3)), int(rss.group(1)) / 1048576
        rec = {"set": s, "tool": tool, "round": rnd, "load": round(os.getloadavg()[0], 2)}
        rec["make_s"], rec["make_cpu"], rec["make_mb"] = timed([ARCA, "pack", out, src_of(s)] + ARCA_ARGS[tool])
        rec["size"] = os.path.getsize(out)
        rec["extract_s"], rec["extract_cpu"], rec["extract_mb"] = timed([ARCA, "unpack", out, "-o", dest])
        rec["identical"] = same(src_of(s), os.path.join(dest, name))
        with open(RESULTS, "a") as f:
            f.write(json.dumps(rec) + "\n")
        clear(work)
        arm()
        print(f"{tool} {rec['size']/1e6:.2f} MB, {rec['make_s']:.2f} s / {rec['extract_s']:.2f} s, identical={rec['identical']}")


if __name__ == "__main__":
    main()
