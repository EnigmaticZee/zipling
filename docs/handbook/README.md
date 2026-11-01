# Arca engine handbook

What this folder is: everything a person or an agent needs to pick up the Arca
engine cold. Where it stands, why each choice was made, every idea that was tried
and how it ended, every benchmark number and how to reproduce it, and the problems
still open. Read it before touching the engine or starting new research, so no
experiment is run twice and no dead end is walked again.

Arca is the compression engine of Zipling, a macOS archiver. Its archive format is
`.zlg`. The engine is Rust (`engine/`), the app is Swift (`app/`), the two meet at a
C interface (`engine/arca-ffi`).

## Read in this order

1. `engine.md`: the pipeline, the lanes, the codecs and transforms, the four presets
   and the numbers that justify each setting, the file format, the switches, how to
   build and test.
2. `benchmarks.md`: the data sets, every rival and its exact settings, the method,
   the result files, the latest standings, and how to run the whole thing again.
3. `research-index.md`: one entry per idea since 28 October 2026, taken, parked,
   dropped or open, with the measurements, the verdict and what would make it worth
   another look. This is the backlog.
4. `open-problems.md`: where the engine still loses and why, and the work that is
   planned: the recovery record, the Ultra level, memory, the app's overhead.

## Where the rest lives

- `docs/RESEARCH.md`: the full research log, 28 October to 1 November 2026, in
  the order it happened, with every measurement. The handbook is the curated view;
  the log is the record. When they disagree, the log's numbers win and the handbook
  needs a fix.
- `docs/DESIGN.md`, `docs/ROADMAP.md`, `docs/APP.md`: the product and app side.
- `docs/BENCHMARKS.md`: the public benchmark document, written from the result
  files by `bench/research/benchmarks_md.py`.
- `bench/`: every result file (`results-*.jsonl`, with `.log` files beside them),
  the reports, the harnesses and the scoring scripts. `benchmarks.md` lists them.
- The test sets are not in the repository. The six public ones can be rebuilt from
  `bench/research/sets.sh` and the notes in `benchmarks.md`; three are the owner's
  own files and appear only by name and number.
- Git tags: `before-engine-research` (the engine before the research of October
  2026), `before-engine-cleanup` (the engine as benchmarked and published on 1
  November 2026, with every research piece still in the tree). Anything the cleanup
  removed is one `git show before-engine-cleanup:<path>` away.

## Keeping it current

When a preset, a codec, a transform or the format changes, update `engine.md` in
the same commit. When an experiment ends, add or move its entry in
`research-index.md`. When a benchmark is run for publication, add its files to
`benchmarks.md` and refresh the standings. Dates are written in full
(1 November 2026), never relative.
