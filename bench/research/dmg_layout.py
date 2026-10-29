"""Where a DMG's bytes are: compressed chunks by type, raw chunks, and bytes no chunk covers."""
import plistlib, struct, sys, collections, zlib
TYPES = {0: "zero", 1: "raw", 2: "ignore", 0x80000004: "adc", 0x80000005: "zlib", 0x80000006: "bzip2",
         0x80000007: "lzfse", 0x80000008: "lzma", 0x7ffffffe: "comment", 0xffffffff: "end"}
for path in sys.argv[1:]:
    data = open(path, "rb").read()
    k = data[-512:]
    assert k[:4] == b"koly", path
    df_off, df_len = struct.unpack(">QQ", k[24:40])
    xml_off, xml_len = struct.unpack(">QQ", k[216:232])
    pl = plistlib.loads(data[xml_off:xml_off + xml_len])
    covered = bytearray(len(data) // 4096 + 1)
    by = collections.Counter(); cnt = collections.Counter(); dec = collections.Counter()
    zl = collections.Counter()
    spans = []
    for blk in pl["resource-fork"]["blkx"]:
        m = blk["Data"]
        n = struct.unpack(">I", m[200:204])[0]
        for i in range(n):
            e = m[204 + 40 * i: 244 + 40 * i]
            t, _, sec, nsec, off, ln = struct.unpack(">IIQQQQ", e)
            name = TYPES.get(t, hex(t))
            by[name] += ln; cnt[name] += 1; dec[name] += nsec * 512
            if ln: spans.append((df_off + off, ln))
            if name == "zlib" and ln >= 2:
                c = data[df_off + off + 1]
                zl[(c >> 6)] += 1
    spans.sort()
    cov = 0; end = 0
    for s, l in spans:
        if s + l > end:
            cov += s + l - max(s, end); end = s + l
    other = len(data) - cov
    print(f"\n{path.split('/')[-1]}: {len(data)/1e6:.1f} MB, data fork {df_len/1e6:.1f} MB, not in any chunk {other/1e6:.2f} MB")
    for name in by:
        print(f"  {name:8} {cnt[name]:6} chunks  {by[name]/1e6:8.2f} MB stored  {dec[name]/1e6:8.1f} MB decoded")
    if zl: print("  zlib FLEVEL (0=fastest,1=fast,2=default,3=max):", dict(zl))
