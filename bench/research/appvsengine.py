"""The app against the engine binary on the same sets, same RAM disk: pack at two
levels and extract, the app driven through its AppleScript commands.

Usage: python3 -I appvsengine.py <arca binary> <work dir with the sets> <set names...>
"""
import hashlib
import os
import shutil
import subprocess
import sys
import time

A, W = sys.argv[1], sys.argv[2]
sets = sys.argv[3:]


def wall(cmd):
    t = time.perf_counter()
    r = subprocess.run(cmd, capture_output=True, text=True)
    return time.perf_counter() - t, r


def osa(script):
    return wall(["osascript", "-e", script])


def tree_hash(root):
    h = hashlib.blake2b()
    for dp, dn, fn in os.walk(root):
        dn.sort()
        for f in sorted(fn):
            p = os.path.join(dp, f)
            h.update(os.path.relpath(p, root).encode())
            with open(p, "rb") as fh:
                while True:
                    b = fh.read(1 << 20)
                    if not b:
                        break
                    h.update(b)
    return h.hexdigest()


def rm(p):
    if os.path.isdir(p):
        shutil.rmtree(p, ignore_errors=True)
    elif os.path.exists(p):
        os.remove(p)


osa('tell application "Zipling" to activate')
time.sleep(1)
floor = min(osa('tell application "Zipling" to get name')[0] for _ in range(5))
print(f"AppleScript round trip to the app, no work: {floor * 1000:.0f} ms (subtracted from app times below)\n")
print(f"{'set':8s} {'level':9s} {'engine pack':>11s} {'app pack':>9s} {'over':>6s}   {'engine unpack':>13s} {'app unpack':>10s} {'over':>6s}   sizes  trees")
for s in sets:
    src = os.path.join(W, s)
    for level in ("fast", "balanced"):
        cli_arc, app_arc = os.path.join(W, f"cli-{s}.zlg"), os.path.join(W, f"{s}.zlg")
        cp, ap, cu, au = [], [], [], []
        size_cli = size_app = 0
        trees = "?"
        for rnd in range(2):
            rm(cli_arc)
            t, r = wall([A, "pack", cli_arc, src, "-l", level])
            if r.returncode:
                print("engine pack failed", r.stderr[-200:])
            cp.append(t)
            size_cli = os.path.getsize(cli_arc)
            rm(app_arc)
            t, r = osa(f'tell application "Zipling" to compress {{POSIX file "{src}"}} as "zlg" level "{level}"')
            if r.returncode or not os.path.exists(app_arc):
                print("app compress failed", r.stderr.strip()[-200:], r.stdout.strip()[-100:])
                break
            ap.append(t - floor)
            size_app = os.path.getsize(app_arc)
            # both extract the app's archive
            xc, xa = os.path.join(W, "x-cli"), os.path.join(W, "x-app")
            rm(xc)
            rm(xa)
            os.makedirs(xa)
            t, r = wall([A, "unpack", app_arc, "-o", xc])
            cu.append(t)
            t, r = osa(f'tell application "Zipling" to extract {{POSIX file "{app_arc}"}} into POSIX file "{xa}"')
            if r.returncode:
                print("app extract failed", r.stderr.strip()[-200:])
                break
            au.append(t - floor)
            if rnd == 0:
                a, b = os.path.join(xc, s), os.path.join(xa, s)
                trees = "same" if os.path.isdir(a) and os.path.isdir(b) and tree_hash(a) == tree_hash(b) else "DIFFER"
            rm(xc)
            rm(xa)
        if cp and ap and cu and au:
            print(f"{s:8s} {level:9s} {min(cp):11.3f} {min(ap):9.3f} {min(ap) - min(cp):+6.3f}   {min(cu):13.3f} {min(au):10.3f} {min(au) - min(cu):+6.3f}   {'same' if size_cli == size_app else f'{size_cli} vs {size_app}'}  {trees}", flush=True)
        rm(cli_arc)
        rm(app_arc)
