"""Bytes per JPEG segment type across a folder: scan data vs headers vs metadata."""
import os, sys, collections, struct
tot = collections.Counter(); kinds = collections.Counter(); n = 0; after_eoi = 0; samp = collections.Counter()
for root, _, files in os.walk(sys.argv[1]):
    for f in files:
        d = open(os.path.join(root, f), "rb").read()
        if d[:2] != b"\xff\xd8": kinds["not jpeg"] += 1; continue
        n += 1; i = 2; sof = None
        while i < len(d):
            if d[i] != 0xFF: break
            m = d[i + 1]
            if m == 0xD9: i += 2; break
            if m in (0x01,) or 0xD0 <= m <= 0xD7: i += 2; continue
            ln = struct.unpack(">H", d[i + 2:i + 4])[0]
            name = {0xE0: "APP0", 0xE1: "APP1 (EXIF/XMP)", 0xE2: "APP2 (ICC/MPF)", 0xDB: "DQT", 0xC4: "DHT", 0xDD: "DRI", 0xFE: "COM",
                    0xC0: "SOF0", 0xC1: "SOF1", 0xC2: "SOF2", 0xDA: "SOS"}.get(m, f"APP{m-0xE0}" if 0xE0 <= m <= 0xEF else hex(m))
            tot[name] += ln + 2
            if m in (0xC0, 0xC1, 0xC2):
                sof = name; nc = d[i + 9]
                samp[(name, nc, tuple(d[i + 11 + 3 * c] for c in range(nc)))] += 1
            i += 2 + ln
            if m == 0xDA:
                j = i
                while True:
                    j = d.index(b"\xff", j)
                    if d[j + 1] == 0 or 0xD0 <= d[j + 1] <= 0xD7: j += 2; continue
                    break
                tot["scan data"] += j - i; i = j
        after_eoi += len(d) - i
        kinds[sof] += 1
alls = sum(tot.values()) + after_eoi
print(n, "JPEGs,", dict(kinds))
for k, v in tot.most_common(): print(f"  {k:18} {v/1e6:8.2f} MB  {100*v/alls:5.1f}%")
print(f"  {'after EOI':18} {after_eoi/1e6:8.2f} MB  {100*after_eoi/alls:5.1f}%")
for k, v in samp.most_common(8): print("  ", k, v)
