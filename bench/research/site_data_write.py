#!/usr/bin/env python3
"""Replaces the datasets block and the provenance comment of site/data.js with the app
benchmark's numbers (site_data.py prints the block)."""
import subprocess
import sys
import os

WT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")) + "/"
S = os.path.dirname(os.path.abspath(__file__)) + "/"
out = subprocess.run([sys.executable, "-I", S + "site_data.py"], capture_output=True, text=True).stdout
assert "WARNING" not in out and "  datasets: [" in out, out[:800]
block = out[out.index("  datasets: ["):out.index("  ],") + len("  ],")]
path = WT + "site/data.js"
js = open(path).read()
start = js.index("  datasets: [")
end = js.index("  ],", start) + len("  ],")
js = js[:start] + block + js[end:]
head_end = js.index("window.ZIPLING")
header = """// Data shared by every page. Benchmarks come from the app benchmark of 31 October to
// 1 November 2026 (bench/results-2026-10-31-apps*.jsonl): the installed Zipling app driven
// through its AppleScript commands and timed by its own clock, 7-Zip, WinRAR, zstd and xz as
// processes in the same session, Finder measured in Finder itself; competitor capabilities
// from MacPacker's comparison (macpacker.app/en/compare, 10 Aug 2026).

"""
js = header + js[head_end:]
open(path, "w").write(js)
print(block)
print(out[out.index("second pass"):])
