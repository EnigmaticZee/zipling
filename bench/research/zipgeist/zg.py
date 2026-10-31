#!/usr/bin/env python3
"""Bookkeeping for the Zipgeist runs of the checkpoint (driven through Finder).

  zg.py prep SET                      clone SET into zg/, open its Finder window, arm
  zg.py made SET TOOL ROUND EXT       record the compress run, move the source out, arm
  zg.py extracted SET TOOL ROUND EXT  record the extract run and check it, reset, arm
  zg.py arca SET TOOL ROUND           an Arca run from the command line, same session
  zg.py clock SET ROUND               the Zipling app at Fast through AppleScript, same
                                      session (tool arca-fast-clock; bench/research/apprun.py)
  zg.py arm                           arm again (after a run that went wrong)

--skip N (any command): the stopwatch lets N left clicks go by before it starts the
clock, so the clicks that open the menu and the dialog can go in the same batch as
the one on Compress. --key: the clock starts at the first key press instead (Return
on the dialog's default button), whatever the clicks before it. made/extracted wait
for the stopwatch. Arming also brings Finder to the front.

Environment: ZG_WORK (where the clone and the moved-out source live; default: this
folder's parent), ZG_RESULTS (the results file; default: the 2026-10-30 Zipgeist file),
ZG_STOPWATCH (the built stopwatch), ZG_BUNDLE (the app whose processes the stopwatch
watches; default Zipgeist, or Archive Utility for Finder's own Compress).
"""

import filecmp, json, os, re, shutil, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
SCRATCH = os.path.dirname(HERE)
WORK = os.environ.get("ZG_WORK", SCRATCH)
ZG, SRC = os.path.join(WORK, "zg"), os.path.join(WORK, "zgsrc")
REPO = "/Users/zee/Documents/Building/Tools/Zip Project/.claude/worktrees/arca-research-77726f"
RESULTS = os.environ.get("ZG_RESULTS", os.path.join(REPO, "bench/results-2026-10-30-zipgeist.jsonl"))
# the stopwatch also reports the click time, so a run ends no earlier than its output
# landed: a job that is mostly disk work can look idle to the CPU.
STOPWATCH = os.environ.get("ZG_STOPWATCH", os.path.join(HERE, "stopwatch"))
BUNDLE = os.environ.get("ZG_BUNDLE", "/Applications/Zipgeist.app/")
FINDER = os.path.join(HERE, "finder.sh")
SKIP, TRIGGER = 0, "click"
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


STAGE = '''
tell application "System Events"
    if exists process "Claude" then
        tell process "Claude"
            repeat with i from 1 to count of windows
                try
                    if value of attribute "AXFullScreen" of window i is true then
                        set value of attribute "AXFullScreen" of window i to false
                        delay 1.5
                    end if
                end try
                try
                    set p to position of window i
                    if item 1 of p >= -100 and item 2 of p >= -100 then set position of window i to {-350, -1400}
                end try
            end repeat
        end tell
    end if
end tell
tell application "Finder"
    if (count of windows) > 0 then set bounds of front window to {600, 140, 1400, 640}
    activate
end tell
'''


def stage():
    """Claude's windows off the MacBook display, Finder's window in its place and in front."""
    subprocess.run(["osascript", "-e", STAGE], capture_output=True)


def arm():
    subprocess.run(["pkill", "-f", STOPWATCH])
    for p in (ARM, RESULT):
        if os.path.exists(p):
            os.remove(p)
    stage()
    subprocess.Popen([STOPWATCH, BUNDLE, ARM, RESULT, str(SKIP), TRIGGER, os.environ.get("ZG_BUSY", "0.004")],
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
    global SKIP, TRIGGER
    if "--skip" in sys.argv:
        i = sys.argv.index("--skip")
        SKIP = int(sys.argv[i + 1])
        del sys.argv[i:i + 2]
    if "--key" in sys.argv:
        TRIGGER = "key"
        sys.argv.remove("--key")
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
        out = os.path.join(ZG, f"{name}.{ext}")
        # the archive must exist, be more than Finder's zero-byte placeholder, and have
        # stopped changing (an app the stopwatch does not watch, Finder's own Compress,
        # writes a temporary elsewhere and drops the finished file in at the end)
        t, last = time.time(), None
        while time.time() - t < 1500:
            cur = (os.path.getsize(out), os.stat(out).st_mtime) if os.path.exists(out) else None
            if cur is not None and cur[0] > 0 and cur == last:
                break
            last = cur
            time.sleep(1.5)
        files = sorted(os.listdir(ZG))
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
        if "Archive Utility" in BUNDLE:
            # it quits by itself when done; never compare or clear while it is still at work
            t = time.time()
            while time.time() - t < 600 and subprocess.run(["pgrep", "-f", "Archive Utility.app"], capture_output=True).returncode == 0:
                time.sleep(0.5)
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
    elif cmd == "clock":
        rnd = int(sys.argv[3])
        sys.path.insert(0, SCRATCH)
        import apprun
        work = os.path.join(WORK, "zgclock")
        clear(work)
        rec = {"set": s, "tool": "arca-fast-clock", "round": rnd, "load": round(os.getloadavg()[0], 2), "via": "app"}
        rec.update(apprun.app_run("fast", src_of(s), work, apprun.floor(), check=True))
        with open(RESULTS, "a") as f:
            f.write(json.dumps(rec) + "\n")
        shutil.rmtree(work, ignore_errors=True)
        arm()
        print(f"zipling app fast {rec['size']/1e6:.2f} MB, {rec['make_s']:.2f} s / {rec['extract_s']:.2f} s (floor {rec['floor_s']*1000:.0f} ms), identical={rec['identical']}")
    elif cmd == "arca":
        tool, rnd = sys.argv[3], int(sys.argv[4])
        work = os.path.join(WORK, "zgarca")
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
