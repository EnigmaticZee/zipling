#!/usr/bin/env python3
"""The Zipling app against the command-line rivals, on every set, for the published
benchmark: the app is driven through its own AppleScript commands (compress, extract)
and timed from the command's start to its return, the rivals as processes, as in
bench/checkpoint.py. Zipling and rivals alternate; two rounds, the second reversed.

    python3 -I bench/research/apprun.py <work dir> <results.jsonl> [set ...]

Needs the installed /Applications/Zipling.app running (it is launched if not). The
app writes its archive next to the source, as Finder's Compress does; the harness
moves it into the work dir (same volume, a rename) and extracts from there.

What a Zipling row records:
  make_s / extract_s   the app's own clock, from the command's arrival in the app to the
                       finished file (its timing log, on with
                       `defaults write com.observantsystems.zipling timing -bool true`,
                       read through a `log stream` running alongside); *_app keeps it,
                       *_osa is the AppleScript command's wall time minus the measured
                       cost of a round trip to the app that does no work (floor_s, the
                       least of five before each set), *_raw the unsubtracted wall time.
                       The osascript round trip (process start, script compile, event)
                       costs 50-150 ms, more than a Fast pack of a small set, so it is
                       not charged to the app; it is recorded for the record
  make_cpu / make_mb   the app process's CPU seconds and peak physical footprint (MB)
                       over the command, sampled every 20 ms through proc_pid_rusage,
                       the same figures the Zipgeist stopwatch reports
  started_at           wall-clock epoch of the command's start, so the app's own timing
                       log (defaults write com.observantsystems.zipling timing -bool true)
                       can be matched to the row afterwards
Rival rows are the checkpoint's (wall, CPU, peak RSS from /usr/bin/time -l).
CHECKPOINT_RAR names the `rar` binary (adds WinRAR's three settings);
CHECKPOINT_TOOLS keeps only the named tools; APPRUN_LOAD sets the quiet threshold
(1-minute load, default 2.5); CHECKPOINT_NO_WAIT skips the wait.
"""

import ctypes
import json
import os
import shlex
import shutil
import subprocess
import sys
import threading
import time

sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
import checkpoint  # noqa: E402
from checkpoint import RIVALS, SETS, same, timed  # noqa: E402

APP = "Zipling"
LEVELS = [("arca-fast", "fast"), ("arca-balanced", "balanced"), ("arca-max", "max"), ("arca-ultra", "ultra")]

# proc_pid_rusage(pid, RUSAGE_INFO_V4): user and system time in Mach ticks, physical
# footprint in bytes (fields 2, 3 and 9 of rusage_info_v4).
_libproc = ctypes.CDLL("/usr/lib/libproc.dylib")
_libsys = ctypes.CDLL("/usr/lib/libSystem.B.dylib")


class _Timebase(ctypes.Structure):
    _fields_ = [("numer", ctypes.c_uint32), ("denom", ctypes.c_uint32)]


_tb = _Timebase()
_libsys.mach_timebase_info(ctypes.byref(_tb))
_TICK_NS = _tb.numer / _tb.denom


def rusage(pid):
    """(cpu seconds, physical footprint MB) of one process, or None."""
    buf = (ctypes.c_uint8 * 1024)()
    if _libproc.proc_pid_rusage(pid, 4, buf) != 0:
        return None
    words = ctypes.cast(buf, ctypes.POINTER(ctypes.c_uint64))
    user, system, footprint = words[2], words[3], words[9]
    return (user + system) * _TICK_NS / 1e9, footprint / 1048576


class Sampler:
    """Peak footprint and CPU time of a process while a command runs."""

    def __init__(self, pid):
        self.pid, self.peak, self.stop = pid, 0.0, threading.Event()
        self.start_cpu = rusage(pid)[0]
        self.thread = threading.Thread(target=self.run, daemon=True)
        self.thread.start()

    def run(self):
        while not self.stop.is_set():
            u = rusage(self.pid)
            if u:
                self.peak = max(self.peak, u[1])
            self.stop.wait(0.02)

    def finish(self):
        self.stop.set()
        self.thread.join()
        u = rusage(self.pid)
        self.peak = max(self.peak, u[1]) if u else self.peak
        return (u[0] - self.start_cpu) if u else None, round(self.peak, 1)


def osa(script):
    t = time.perf_counter()
    r = subprocess.run(["osascript", "-e", script], capture_output=True, text=True)
    return time.perf_counter() - t, r


# The app's own clock: with `defaults write com.observantsystems.zipling timing -bool true`
# the app logs "compress: done: N ms" / "extract: done: N ms" from the command's arrival
# to the finished file. A `log stream` runs alongside and the harness reads it.
_LOG = {"proc": None, "path": None}


def start_log(work):
    if _LOG["proc"] is None:
        _LOG["path"] = os.path.join(work, "zipling-timing.log")
        out = open(_LOG["path"], "ab")
        _LOG["proc"] = subprocess.Popen(
            ["/usr/bin/log", "stream", "--style", "compact",
             "--predicate", 'subsystem == "com.observantsystems.zipling" AND category == "timing"'],
            stdout=out, stderr=subprocess.DEVNULL)
        import atexit
        atexit.register(_LOG["proc"].terminate)
        time.sleep(2.0)
    return _LOG["path"]


def log_pos():
    return os.path.getsize(_LOG["path"]) if _LOG["path"] and os.path.exists(_LOG["path"]) else 0


def app_time(what, pos, limit=5.0):
    """Seconds the app itself took for the last `what` command, from the log after `pos`."""
    import re
    t = time.time()
    while time.time() - t < limit:
        with open(_LOG["path"], "rb") as f:
            f.seek(pos)
            tail = f.read().decode("utf-8", "replace")
        m = re.findall(rf"{what}: done: ([\d.]+) ms", tail)
        if m:
            return float(m[-1]) / 1000
        time.sleep(0.1)
    return None


def app_pid():
    out = subprocess.run(["pgrep", "-x", APP], capture_output=True, text=True).stdout.split()
    if not out:
        subprocess.run(["open", "-gja", APP], check=True)
        time.sleep(2)
        out = subprocess.run(["pgrep", "-x", APP], capture_output=True, text=True).stdout.split()
    return int(out[0])


def floor():
    """The least of five AppleScript round trips to the app that do no work."""
    return min(osa(f'tell application "{APP}" to get name')[0] for _ in range(5))


def clear(*paths):
    for p in paths:
        if os.path.isdir(p) and not os.path.islink(p):
            shutil.rmtree(p)
        elif os.path.lexists(p):
            os.remove(p)


def app_run(level, src, work, floor_s, check):
    """One Zipling compress + extract through AppleScript; returns the record fields."""
    parent, name = os.path.split(src)
    made, out, dest = os.path.join(parent, name + ".zlg"), os.path.join(work, "out.zlg"), os.path.join(work, "x")
    clear(made, out, dest)
    os.makedirs(dest)
    pid = app_pid()
    start_log(work)
    rec = {"floor_s": round(floor_s, 4), "started_at": time.time()}
    pos = log_pos()
    sampler = Sampler(pid)
    wall, r = osa(f'tell application "{APP}" to compress {{POSIX file "{src}"}} as "zlg" level "{level}"')
    rec["make_cpu"], rec["make_mb"] = sampler.finish()
    if r.returncode or not os.path.exists(made):
        raise RuntimeError(f"compress failed: {r.stderr.strip()[-500:]} {r.stdout.strip()[-200:]}")
    rec["make_s_raw"], rec["make_s_osa"] = wall, max(wall - floor_s, 0.0)
    rec["make_s_app"] = app_time("compress", pos)
    # the app's own clock is the row's time (command arrived -> file finished); the
    # AppleScript round trip is osascript's cost, not the app's
    rec["make_s"] = rec["make_s_app"] if rec["make_s_app"] is not None else rec["make_s_osa"]
    os.rename(made, out)
    rec["size"] = os.path.getsize(out)
    pos = log_pos()
    sampler = Sampler(pid)
    wall, r = osa(f'tell application "{APP}" to extract {{POSIX file "{out}"}} into POSIX file "{dest}"')
    rec["extract_cpu"], rec["extract_mb"] = sampler.finish()
    if r.returncode:
        raise RuntimeError(f"extract failed: {r.stderr.strip()[-500:]}")
    rec["extract_s_raw"], rec["extract_s_osa"] = wall, max(wall - floor_s, 0.0)
    rec["extract_s_app"] = app_time("extract", pos)
    rec["extract_s"] = rec["extract_s_app"] if rec["extract_s_app"] is not None else rec["extract_s_osa"]
    if check:
        rec["identical"] = same(src, os.path.join(dest, name))
    clear(out, dest)
    return rec


def rival_run(make, extract, ext, src, work, check):
    parent, name = os.path.split(src)
    out, dest = os.path.join(work, "out" + ext), os.path.join(work, "x")
    clear(out, dest)
    os.makedirs(dest)
    fmt = dict(src=shlex.quote(src), parent=shlex.quote(parent), name=shlex.quote(name),
               out=shlex.quote(out), dest=shlex.quote(dest))
    rec = {}
    rec["make_s"], rec["make_cpu"], rec["make_mb"] = timed(make.format(**fmt))
    rec["size"] = os.path.getsize(out)
    rec["extract_s"], rec["extract_cpu"], rec["extract_mb"] = timed(extract.format(**fmt))
    if check:
        rec["identical"] = same(src, os.path.join(dest, name))
    clear(out, dest)
    return rec


def main():
    work, results = sys.argv[1], sys.argv[2]
    names = sys.argv[3:] or list(SETS)
    os.makedirs(work, exist_ok=True)
    progress = open(results + ".log", "a")
    threshold = float(os.environ.get("APPRUN_LOAD", "2.5"))

    def log(msg):
        progress.write(time.strftime("%H:%M:%S ") + msg + "\n")
        progress.flush()

    def wait_quiet():
        if os.environ.get("CHECKPOINT_NO_WAIT"):
            return os.getloadavg()[0]
        start = time.time()
        while time.time() - start < 600:
            load = os.getloadavg()[0]
            if load < threshold:
                return load
            time.sleep(15)
        log(f"still busy after 10 min (load {os.getloadavg()[0]:.1f}); going on")
        return os.getloadavg()[0]

    keep = os.environ.get("CHECKPOINT_TOOLS")
    keep = set(keep.split(",")) if keep else None
    app_tools = [t for t in LEVELS if keep is None or t[0] in keep]
    rivals = [t for t in RIVALS if keep is None or t[0] in keep]
    order = []
    for i in range(max(len(app_tools), len(rivals))):
        order += [("app",) + t for t in app_tools[i:i + 1]] + [("rival",) + t for t in rivals[i:i + 1]]
    app_pid()
    for set_name in names:
        src = SETS.get(set_name, os.path.abspath(set_name))
        parent, name = os.path.split(src)
        set_name = set_name if set_name in SETS else name
        load = wait_quiet()
        subprocess.run(f"tar -cf - -C {shlex.quote(parent)} {shlex.quote(name)} > /dev/null", shell=True)
        floor_s = floor()
        log(f"{set_name}: start (load {load:.1f}, AppleScript floor {floor_s * 1000:.0f} ms), {len(order)} tools x 2 rounds")
        for rnd in (1, 2):
            for entry in (order if rnd == 1 else order[::-1]):
                tool = entry[1]
                rec = {"set": set_name, "tool": tool, "round": rnd, "load": round(os.getloadavg()[0], 2)}
                if entry[0] == "app":
                    rec["via"] = "app"
                try:
                    if entry[0] == "app":
                        rec.update(app_run(entry[2], src, work, floor_s, check=rnd == 1))
                    else:
                        rec.update(rival_run(entry[2], entry[3], entry[4], src, work, check=rnd == 1))
                except Exception as e:  # noqa: BLE001 - keep going, record the failure
                    rec["error"] = str(e)[-800:]
                    clear(os.path.join(parent, name + ".zlg"))
                with open(results, "a") as f:
                    f.write(json.dumps(rec) + "\n")
                log(f"{set_name} r{rnd} {tool}: {rec.get('size')} {rec.get('make_s', 0):.2f}s / {rec.get('extract_s', 0):.2f}s"
                    + ("" if "error" not in rec else " ERROR " + rec["error"][-120:]))
        shutil.rmtree(os.path.join(work, "x"), ignore_errors=True)
    log("done")


if __name__ == "__main__":
    main()
