#!/usr/bin/env python3
"""diff.py A B: list what differs between two trees (first 25 differences)."""
import filecmp, os, sys

a, b = sys.argv[1], sys.argv[2]


def walk(root):
    out = {}
    for d, dirs, files in os.walk(root):
        for n in dirs + files:
            p = os.path.join(d, n)
            out[os.path.relpath(p, root)] = "l" if os.path.islink(p) else "d" if os.path.isdir(p) else "f"
    return out


ta, tb = walk(a), walk(b)
diffs = []
for r in sorted(set(ta) | set(tb)):
    if r not in tb:
        diffs.append(f"missing in B: {r} ({ta[r]})")
    elif r not in ta:
        diffs.append(f"extra in B:   {r} ({tb[r]})")
    elif ta[r] != tb[r]:
        diffs.append(f"type {ta[r]}->{tb[r]}: {r}")
    elif ta[r] == "l":
        if os.readlink(os.path.join(a, r)) != os.readlink(os.path.join(b, r)):
            diffs.append(f"link target: {r}")
    elif ta[r] == "f":
        pa, pb = os.path.join(a, r), os.path.join(b, r)
        if not filecmp.cmp(pa, pb, shallow=False):
            diffs.append(f"content ({os.path.getsize(pa)} vs {os.path.getsize(pb)} bytes): {r}")
print(len(ta), "entries in A,", len(tb), "in B,", len(diffs), "differences")
for d in diffs[:25]:
    print(" ", d)
