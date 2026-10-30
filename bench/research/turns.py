#!/usr/bin/env python3
"""Settings taking turns on one folder: size, best wall time and mean CPU for pack
and unpack, and on the last round a check that the tree came back identical.

    python3 -I bench/research/turns.py <folder> <work dir> <rounds> <config> ...

A config is one string of pack arguments; `@path` picks the binary (default the
release build) and NAME=value tokens set the environment. Point <work dir> at a RAM
disk for tuning: every round writes the archive and the whole tree again.
"""

import hashlib
import os
import resource
import shutil
import stat
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
ARCA = os.path.join(ROOT, "engine/target/release/arca")


def run(cmd, env):
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    t = time.perf_counter()
    subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=env)
    wall = time.perf_counter() - t
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    return wall, after.ru_utime - before.ru_utime + after.ru_stime - before.ru_stime


def tree(root):
    """Types, modes, link targets and file bytes; links are not followed."""
    out = {}
    for d, dirs, files in os.walk(root):
        for n in dirs + files:
            p = os.path.join(d, n)
            st = os.lstat(p)
            rel = os.path.relpath(p, root)
            if stat.S_ISLNK(st.st_mode):
                out[rel] = ("l", os.readlink(p))
            elif stat.S_ISDIR(st.st_mode):
                out[rel] = ("d", stat.S_IMODE(st.st_mode))
            elif stat.S_ISREG(st.st_mode):
                h = hashlib.blake2b(digest_size=16)
                with open(p, "rb") as f:
                    while b := f.read(1 << 20):
                        h.update(b)
                out[rel] = ("f", stat.S_IMODE(st.st_mode), h.digest())
    return out


src, work, rounds = os.path.abspath(sys.argv[1]), sys.argv[2], int(sys.argv[3])
configs = sys.argv[4:]
os.makedirs(work, exist_ok=True)
ar, dest = os.path.join(work, "t.arca"), os.path.join(work, "x")
results = {c: dict(pack=[], pcpu=[], unpack=[], ucpu=[], size=0, same=None) for c in configs}
want = tree(src)
for r in range(rounds):
    for c in configs if r % 2 == 0 else configs[::-1]:
        arca, args, env = ARCA, [], dict(os.environ)
        for tok in c.split():
            if tok.startswith("@"):
                arca = tok[1:]
            elif tok[0].isupper() and "=" in tok:
                k, v = tok.split("=", 1)
                env[k] = v
            else:
                args.append(tok)
        if os.path.exists(ar):
            os.remove(ar)
        shutil.rmtree(dest, ignore_errors=True)
        w, u = run([arca, "pack", *args, ar, src], env)
        res = results[c]
        res["pack"].append(w)
        res["pcpu"].append(u)
        res["size"] = os.path.getsize(ar)
        w, u = run([arca, "unpack", "-o", dest, ar], env)
        res["unpack"].append(w)
        res["ucpu"].append(u)
        if r == rounds - 1:
            res["same"] = tree(os.path.join(dest, os.path.basename(src))) == want
shutil.rmtree(dest, ignore_errors=True)
if os.path.exists(ar):
    os.remove(ar)
mean = lambda v: sum(v) / len(v)
for c in configs:
    r = results[c]
    print(f"{r['size'] / 1e6:9.2f} MB  pack {min(r['pack']):7.3f} s {mean(r['pcpu']):7.2f} CPU  "
          f"unpack {min(r['unpack']):6.3f} s {mean(r['ucpu']):6.2f} CPU  "
          f"{'identical' if r['same'] else 'DIFFERENT'}  {c}")
