//! Reading, verifying and extracting archives.
//!
//! Extraction is a dependency graph on every core: blocks decode, and each
//! piece of work they unlock runs as soon as it is ready, costliest first.
//! Plain objects are streamed chunk-by-chunk straight into their files (so a
//! 50 GB video never sits in memory); transformed objects are rebuilt whole
//! the moment their last block is in.
//! Every block is checked against BLAKE3 hashes of its compressed and decoded
//! bytes before anything is written, and every transformed object is checked
//! against the hash of its original content.

use crate::codec;
use crate::hash::Hasher;
use crate::hooks::Hooks;
use crate::format::{self, BlockInfo, Codec, Entry, EntryKind, Footer, Index, Object, Transform, FOOTER_LEN, HEADER_LEN};
use crate::transform;
use crate::prof;
use anyhow::{bail, ensure, Context, Result};
use rayon::prelude::*;
use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap, HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::os::unix::fs::{FileExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub struct Archive {
    file: File,
    pub index: Index,
    hasher: Hasher,
    pub archive_bytes: u64,
}

#[derive(Debug, Default, Clone)]
pub struct ExtractOptions {
    pub threads: usize,
    pub overwrite: bool,
    /// Extract only entries whose path equals or lies under one of these.
    pub only: Vec<String>,
    /// Set as `com.apple.quarantine` on everything extracted (the archive's own),
    /// through each file's descriptor as it is written.
    pub quarantine: Option<Vec<u8>>,
    pub hooks: Hooks,
}

#[derive(Debug, Default, Clone)]
pub struct ExtractStats {
    pub files: u64,
    pub bytes: u64,
    pub blocks: u64,
    pub seconds: f64,
}

impl Archive {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
        let len = file.metadata()?.len();
        ensure!(len >= (HEADER_LEN + FOOTER_LEN) as u64, "file too small to be an Arca archive");
        let mut head = [0u8; HEADER_LEN];
        file.read_exact_at(&mut head, 0)?;
        format::check_header(&head)?;
        let mut foot = [0u8; FOOTER_LEN];
        file.read_exact_at(&mut foot, len - FOOTER_LEN as u64)?;
        let footer = Footer::parse(&foot)?;
        ensure!(
            footer.index_offset.checked_add(footer.index_comp_len) == Some(len - FOOTER_LEN as u64),
            "index location is inconsistent"
        );
        ensure!(footer.index_raw_len <= 4 << 30, "index too large");
        let mut comp = vec![0u8; footer.index_comp_len as usize];
        file.read_exact_at(&mut comp, footer.index_offset)?;
        ensure!(format::hash16(&comp) == footer.index_hash, "index checksum mismatch: archive is damaged");
        let raw = zstd::bulk::decompress(&comp, footer.index_raw_len as usize)?;
        let index = Index::decode(&raw)?;
        for b in &index.blocks {
            ensure!(
                b.file_offset >= HEADER_LEN as u64 && b.file_offset + b.comp_len <= footer.index_offset,
                "block outside archive body"
            );
        }
        let hasher = index.hasher();
        Ok(Archive { file, index, hasher, archive_bytes: len })
    }

    pub fn read_block(&self, id: u32) -> Result<Vec<u8>> {
        let b: &BlockInfo = &self.index.blocks[id as usize];
        let mut comp = vec![0u8; b.comp_len as usize];
        self.file.read_exact_at(&mut comp, b.file_offset)?;
        ensure!(self.hasher.hash(&comp) == b.checksum, "block {id} is damaged (checksum mismatch)");
        // A stored block's two hashes are of the same bytes.
        if b.codec == Codec::Store && b.raw_hash == b.checksum && b.raw_len == b.comp_len {
            return Ok(comp);
        }
        let raw = {
            let _p = prof::span(
                match b.codec {
                    Codec::Store => "decode.store",
                    Codec::Zstd => "decode.zstd",
                    Codec::Xz => "decode.xz",
                    Codec::Ppmd => "decode.ppmd",
                    Codec::Lzma2 => "decode.lzma2",
                    Codec::Bsc => "decode.bsc",
                    Codec::Lzt => "decode.lzt",
                },
                b.raw_len,
            );
            codec::decompress(&comp, b.codec, b.raw_len).with_context(|| format!("block {id}"))?
        };
        ensure!(self.hasher.hash(&raw) == b.raw_hash, "block {id} decoded to the wrong content");
        Ok(raw)
    }

    /// Decodes everything and checks every hash without writing anything.
    pub fn verify(&self, threads: usize) -> Result<ExtractStats> {
        self.run(None, &ExtractOptions { threads, ..Default::default() })
    }

    /// `verify` with progress and cancellation (`only` and `overwrite` are ignored).
    pub fn verify_with(&self, opts: &ExtractOptions) -> Result<ExtractStats> {
        self.run(None, &ExtractOptions { only: Vec::new(), ..opts.clone() })
    }

    pub fn extract(&self, dest: &Path, opts: &ExtractOptions) -> Result<ExtractStats> {
        std::fs::create_dir_all(dest)?;
        self.run(Some(dest), opts)
    }

    fn selected(&self, opts: &ExtractOptions) -> Vec<bool> {
        let entries = &self.index.entries;
        let mut sel: Vec<bool> = entries
            .iter()
            .map(|e| {
                opts.only.is_empty()
                    || opts.only.iter().any(|p| {
                        let p = p.trim_end_matches('/');
                        e.path == p || e.path.starts_with(&format!("{p}/"))
                    })
            })
            .collect();
        // Hard links need their target's content.
        for (i, e) in entries.iter().enumerate() {
            if let (true, EntryKind::HardLink { target }) = (sel[i], &e.kind) {
                sel[*target as usize] = true;
            }
        }
        sel
    }

    fn run(&self, dest: Option<&Path>, opts: &ExtractOptions) -> Result<ExtractStats> {
        let start = Instant::now();
        let idx = &self.index;
        let threads = if opts.threads == 0 { std::thread::available_parallelism().map_or(4, |n| n.get()) } else { opts.threads };
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build()?;
        let sel = self.selected(opts);

        let timing = std::env::var_os("ARCA_DEBUG_TIMING").is_some();
        let phase = |name: &str| {
            if timing {
                // Process CPU so far, user and kernel.
                let mut r: libc::rusage = unsafe { std::mem::zeroed() };
                // SAFETY: getrusage fills the struct it is given.
                unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut r) };
                let t = |v: libc::timeval| v.tv_sec as f64 + v.tv_usec as f64 / 1e6;
                eprintln!("[{:>7.3}s] {name} (cpu {:.2} user, {:.2} sys)", start.elapsed().as_secs_f64(), t(r.ru_utime), t(r.ru_stime));
            }
        };

        // Resolve and validate every output path before touching the disk.
        let mut targets: Vec<Option<PathBuf>> = vec![None; idx.entries.len()];
        let mut dirs: Vec<&Path> = Vec::new();
        if let Some(dest) = dest {
            let symlinks: std::collections::HashSet<&str> = idx
                .entries
                .iter()
                .filter(|e| matches!(e.kind, EntryKind::Symlink { .. }))
                .map(|e| e.path.as_str())
                .collect();
            // One lookup per entry, on every core: (path, already exists).
            let checked: Vec<Option<(PathBuf, bool)>> = pool.install(|| {
                idx.entries
                    .par_iter()
                    .enumerate()
                    .map(|(i, e)| -> Result<_> {
                        if !sel[i] {
                            return Ok(None);
                        }
                        let p = safe_join(dest, &e.path)?;
                        // Refuse to write through a symlink the archive itself creates.
                        let mut parent = e.path.as_str();
                        while let Some((head, _)) = parent.rsplit_once('/') {
                            ensure!(!symlinks.contains(head), "entry {} lies beneath a symlink", e.path);
                            parent = head;
                        }
                        let exists = !matches!(e.kind, EntryKind::Dir) && p.symlink_metadata().is_ok();
                        Ok(Some((p, exists)))
                    })
                    .collect::<Result<_>>()
            })?;
            for (i, c) in checked.into_iter().enumerate() {
                let Some((p, exists)) = c else { continue };
                if exists {
                    if opts.overwrite {
                        std::fs::remove_file(&p).or_else(|_| std::fs::remove_dir_all(&p))?;
                    } else {
                        bail!("{} already exists", p.display());
                    }
                }
                targets[i] = Some(p);
            }
            phase("paths checked");
            // Each directory once. Files create their parents on first use, so
            // this list only catches directories nothing is written into.
            let mut set: BTreeSet<&Path> = BTreeSet::new();
            for (i, e) in idx.entries.iter().enumerate() {
                let Some(p) = &targets[i] else { continue };
                if matches!(e.kind, EntryKind::Dir) {
                    set.insert(p);
                } else if let Some(parent) = p.parent() {
                    set.insert(parent);
                }
            }
            dirs = set.into_iter().collect();
        }

        // Objects to rebuild, and the entries that receive each one.
        let mut receivers: HashMap<u32, Vec<usize>> = HashMap::new();
        for (i, e) in idx.entries.iter().enumerate() {
            if let (true, EntryKind::File { object, .. }) = (sel[i], &e.kind) {
                receivers.entry(*object).or_default().push(i);
            }
        }
        let mut objs: Vec<u32> = receivers.keys().copied().collect();
        objs.sort_unstable();

        // What each block feeds: plain chunks it holds (written straight from the
        // block) and transformed objects that need it (rebuilt once all their
        // blocks are in).
        let nblocks = idx.blocks.len();
        let mut units: Vec<Vec<(u32, u64, u32)>> = vec![Vec::new(); nblocks]; // (object, file offset, chunk)
        let mut unlocks: Vec<Vec<u32>> = vec![Vec::new(); nblocks];
        let mut parts: HashMap<u32, Vec<u32>> = HashMap::new();
        for &o in &objs {
            let obj = &idx.objects[o as usize];
            if obj.transform == Transform::None {
                let mut off = 0u64;
                for &c in &obj.chunks {
                    let ch = &idx.chunks[c as usize];
                    units[ch.block as usize].push((o, off, c));
                    off += ch.len;
                }
            } else {
                let bs = blocks_of(idx, o);
                for &b in &bs {
                    unlocks[b as usize].push(o);
                }
                parts.insert(o, bs);
            }
        }
        // Plain chunks are written in runs of a few files, so a block's writes
        // spread over every core.
        let runs: Vec<Vec<(u32, u32)>> = units
            .iter()
            .map(|u| {
                let mut out = Vec::new();
                let (mut start, mut bytes) = (0usize, 0u64);
                for (k, &(_, _, c)) in u.iter().enumerate() {
                    bytes += idx.chunks[c as usize].len;
                    if k + 1 - start >= 16 || bytes >= 8 << 20 || k + 1 == u.len() {
                        out.push((start as u32, k as u32 + 1));
                        (start, bytes) = (k + 1, 0);
                    }
                }
                out
            })
            .collect();
        let refs: Vec<u32> = (0..nblocks).map(|b| (runs[b].len() + unlocks[b].len()) as u32).collect();
        // Cheap stored blocks first, then the blocks holding up the most rebuilding
        // (a photo's headers sit in a late block): rebuilds then run alongside the
        // writes, which only `CREATORS` threads can do, and not after them.
        let holds: Vec<u64> = unlocks
            .iter()
            .map(|os| os.iter().map(|&o| rebuild_cost(&idx.objects[o as usize])).sum())
            .collect();
        let mut order: Vec<u32> = (0..nblocks as u32).filter(|&b| refs[b as usize] > 0).collect();
        order.sort_by_key(|&b| (idx.blocks[b as usize].codec != Codec::Store, Reverse(holds[b as usize]), b));
        let wanted = order.len() as u64;

        let total: u64 = objs.iter().map(|&o| idx.objects[o as usize].size).sum();
        let quarantine = opts.quarantine.as_deref();
        let x = Extraction {
            archive: self,
            quarantine,
            dest: dest.is_some(),
            targets: &targets,
            receivers: &receivers,
            fd_done: (0..idx.entries.len()).map(|_| AtomicBool::new(false)).collect(),
            hooks: &opts.hooks,
            bytes: AtomicU64::new(0),
            total,
            units,
            runs,
            unlocks,
            parts,
            order,
            max_resident: 2 * threads.max(1),
            sched: Mutex::new(Sched {
                ready: BinaryHeap::new(),
                writes: BinaryHeap::new(),
                writing: 0,
                seq: 0,
                data: vec![None; nblocks],
                refs,
                waiting: HashMap::new(),
                next: 0,
                resident: 0,
                resident_bytes: 0,
                running: 0,
            }),
            failure: Mutex::new(None),
            failed: AtomicBool::new(false),
            trace: timing.then(|| (start, Mutex::new(Vec::new()))),
        };

        {
            let mut st = x.sched.lock().unwrap();
            for (o, bs) in &x.parts {
                st.waiting.insert(*o, bs.len() as u32);
            }
            st.admit(&x);
            let n = st.queued();
            drop(st);
            pool.scope(|s| {
                for _ in 0..n {
                    s.spawn(|s| x.job(s));
                }
            });
        }
        phase("blocks decoded, files written");
        if let Some((_, v)) = &x.trace {
            // Busy threads per quarter second, by kind of work.
            let v = v.lock().unwrap();
            let end = v.iter().map(|t| t.1).fold(0.0, f64::max);
            let mut t = 0.0;
            while t < end {
                let mut busy = [0.0f64; 3];
                for &(a, b, k) in v.iter() {
                    let overlap = (b.min(t + 0.25) - a.max(t)).max(0.0);
                    busy[k as usize] += overlap / 0.25;
                }
                eprintln!(
                    "  {:>5.2}s  decode {:>4.1}  write {:>4.1}  rebuild {:>4.1}  total {:>4.1}",
                    t,
                    busy[0],
                    busy[1],
                    busy[2],
                    busy[0] + busy[1] + busy[2]
                );
                t += 0.25;
            }
        }
        if let Some(e) = x.failure.lock().unwrap().take() {
            return Err(e);
        }
        ensure!(x.bytes.load(Ordering::Relaxed) == total, "extraction finished with objects missing");

        let mut stats = ExtractStats { files: 0, bytes: x.bytes.load(Ordering::Relaxed), blocks: wanted, seconds: 0.0 };
        if dest.is_some() {
            // Directories nothing was written into yet: every ancestor of a
            // written file exists already.
            let mut made: HashSet<&Path> = HashSet::new();
            for (i, e) in idx.entries.iter().enumerate() {
                let (Some(p), EntryKind::File { object, .. }) = (&targets[i], &e.kind) else { continue };
                let chunks = !idx.objects[*object as usize].chunks.is_empty();
                if (chunks && receivers[object][0] == i) || x.fd_done[i].load(Ordering::Relaxed) {
                    let mut a = p.parent();
                    while let Some(d) = a {
                        if !made.insert(d) {
                            break;
                        }
                        a = d.parent();
                    }
                }
            }
            let missing: Vec<&Path> = dirs.iter().copied().filter(|d| !made.contains(d)).collect();
            pool.install(|| missing.par_iter().try_for_each(std::fs::create_dir_all))?;
            // Empty files have no chunks to trigger them.
            pool.install(|| {
                objs.par_iter().filter(|&&o| idx.objects[o as usize].chunks.is_empty()).try_for_each(|&o| x.write_whole(o, &[]))
            })?;
            // Larger duplicates: APFS clones share the source's space.
            let copies: Vec<(usize, usize)> = receivers
                .values()
                .flat_map(|r| r[1..].iter().map(move |&i| (r[0], i)))
                .filter(|&(_, i)| !x.fd_done[i].load(Ordering::Relaxed))
                .collect();
            pool.install(|| {
                copies.par_iter().try_for_each(|&(from, to)| {
                    clone_or_copy(targets[from].as_ref().unwrap(), targets[to].as_ref().unwrap())
                })
            })?;
            for (i, e) in idx.entries.iter().enumerate() {
                let Some(p) = &targets[i] else { continue };
                match &e.kind {
                    EntryKind::HardLink { target } => std::fs::hard_link(targets[*target as usize].as_ref().unwrap(), p)?,
                    EntryKind::Symlink { target } => {
                        use std::os::unix::ffi::OsStrExt;
                        std::os::unix::fs::symlink(std::ffi::OsStr::from_bytes(target), p)?
                    }
                    _ => {}
                }
            }
            phase("empty files, copies and links created");
            // Remaining metadata by path; directories deepest-first so child writes
            // don't bump their mtime.
            pool.install(|| {
                idx.entries
                    .par_iter()
                    .enumerate()
                    .filter(|(i, e)| {
                        targets[*i].is_some() && !matches!(e.kind, EntryKind::Dir) && !x.fd_done[*i].load(Ordering::Relaxed)
                    })
                    .try_for_each(|(i, e)| apply_metadata(targets[i].as_ref().unwrap(), e, quarantine))
            })?;
            // One depth at a time (a parent's mode could bar access to its
            // children), each depth on every core.
            let mut dirs: Vec<(usize, usize)> = (0..idx.entries.len())
                .filter(|&i| targets[i].is_some() && matches!(idx.entries[i].kind, EntryKind::Dir))
                .map(|i| (idx.entries[i].path.matches('/').count(), i))
                .collect();
            dirs.sort_unstable_by_key(|&(depth, i)| (std::cmp::Reverse(depth), i));
            for level in dirs.chunk_by(|a, b| a.0 == b.0) {
                pool.install(|| {
                    level
                        .par_iter()
                        .try_for_each(|&(_, i)| apply_metadata(targets[i].as_ref().unwrap(), &idx.entries[i], quarantine))
                })?;
            }
            phase("metadata applied");
        }
        stats.files = idx.entries.iter().enumerate().filter(|(i, e)| sel[*i] && matches!(e.kind, EntryKind::File { .. })).count() as u64;
        stats.seconds = start.elapsed().as_secs_f64();
        prof::report("unpack");
        Ok(stats)
    }
}

/// One piece of extraction work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Work {
    /// Read, check and decode a block.
    Decode(u32),
    /// Write one run (`runs[block][i]`) of a block's plain chunks.
    Write(u32, u32),
    /// Inverse-transform an object whose blocks are all in, check it, write it.
    Rebuild(u32),
}

/// Extraction runs as a dependency graph, not in waves: an object is rebuilt
/// the moment its last block decodes, and no core waits for a slow block.
struct Sched {
    /// Ready work as (class, cost, order). Decoding comes first because it
    /// unlocks everything else; then the costliest work, so that no long task
    /// starts last.
    ready: BinaryHeap<(u8, u64, Reverse<u64>, Work)>,
    /// Ready writes of plain chunks, kept apart: up to `CREATORS` of them run
    /// alongside everything else, so they don't pile up at the end, where the
    /// cores would wait on the file-creation gate.
    writes: BinaryHeap<(u8, u64, Reverse<u64>, Work)>,
    /// Writes running right now.
    writing: usize,
    seq: u64,
    data: Vec<Option<Arc<Vec<u8>>>>,
    /// Work items that still need each block; at zero the block is dropped.
    refs: Vec<u32>,
    /// Blocks each transformed object still waits for.
    waiting: HashMap<u32, u32>,
    /// Next entry of `order` to decode.
    next: usize,
    /// Blocks decoding, or decoded and still needed, and their decoded size.
    resident: usize,
    resident_bytes: u64,
    /// Work items running right now.
    running: usize,
}

/// Cap on decoded blocks held at once, on top of the block count limit.
const MAX_RESIDENT_BYTES: u64 = 2 << 30;
/// Rough fixed cost of creating a file, in the nanoseconds `rebuild_cost` uses.
const FILE_COST: u64 = 50_000;

/// Rough CPU cost of rebuilding an object, in nanoseconds (measured on Apple M1).
/// Re-creating Deflate scales with the expanded size, which can be 50x the
/// file's (a PNG holding one huge image is a single sequential stream).
fn rebuild_cost(obj: &Object) -> u64 {
    let work = match obj.transform {
        Transform::Lepton => obj.size.saturating_mul(125),
        Transform::JpegLite => obj.size.saturating_mul(17),
        Transform::Deflate => obj.stored_len.saturating_mul(27),
        _ => obj.size,
    };
    work + FILE_COST
}

impl Sched {
    fn push(&mut self, class: u8, cost: u64, w: Work) {
        self.seq += 1;
        let q = if matches!(w, Work::Write(..)) { &mut self.writes } else { &mut self.ready };
        q.push((class, cost, Reverse(self.seq), w));
    }

    /// Items waiting to run.
    fn queued(&self) -> usize {
        self.ready.len() + self.writes.len()
    }

    /// The next item: a write while fewer than `CREATORS` run, else the best
    /// other item (or a write when nothing else is ready).
    fn pop(&mut self) -> Option<Work> {
        let write = !self.writes.is_empty() && (self.writing < CREATORS || self.ready.is_empty());
        if write {
            self.writing += 1;
            self.writes.pop().map(|t| t.3)
        } else {
            self.ready.pop().map(|t| t.3)
        }
    }

    /// Queues block decodes while memory allows. Past the count of blocks, more
    /// are admitted while cores are idle: one large file holds many blocks until
    /// its last arrives, and they mustn't decode one at a time (Firefox's 352 MB
    /// `XUL` spans 44). When nothing can run, one more block is admitted
    /// regardless, so an archive whose objects span far-apart blocks still
    /// finishes (holding more memory).
    fn admit(&mut self, x: &Extraction) {
        while let Some(&b) = x.order.get(self.next) {
            let raw = x.archive.index.blocks[b as usize].raw_len;
            let idle = self.queued() + self.running < x.max_resident / 2;
            let fits = (self.resident < x.max_resident || idle) && self.resident_bytes + raw <= MAX_RESIDENT_BYTES;
            let stalled = self.queued() == 0 && self.running == 0;
            if !fits && !stalled {
                break;
            }
            self.push(2, 0, Work::Decode(b));
            self.resident += 1;
            self.resident_bytes += raw;
            self.next += 1;
        }
    }

    /// One item that used block `b` is done with it.
    fn release(&mut self, b: u32, x: &Extraction) {
        let r = &mut self.refs[b as usize];
        *r -= 1;
        if *r == 0 {
            self.data[b as usize] = None;
            self.resident -= 1;
            self.resident_bytes -= x.archive.index.blocks[b as usize].raw_len;
        }
    }
}

struct Extraction<'a> {
    archive: &'a Archive,
    quarantine: Option<&'a [u8]>,
    dest: bool,
    targets: &'a [Option<PathBuf>],
    receivers: &'a HashMap<u32, Vec<usize>>,
    /// Entries whose metadata was applied through their file descriptor.
    fd_done: Vec<AtomicBool>,
    hooks: &'a Hooks,
    bytes: AtomicU64,
    total: u64,
    /// Per block: its plain chunks as (object, file offset, chunk) ...
    units: Vec<Vec<(u32, u64, u32)>>,
    /// ... grouped into runs of `units` indices ...
    runs: Vec<Vec<(u32, u32)>>,
    /// ... and the transformed objects that need it.
    unlocks: Vec<Vec<u32>>,
    /// Blocks of each transformed object.
    parts: HashMap<u32, Vec<u32>>,
    /// Blocks to decode, in order.
    order: Vec<u32>,
    max_resident: usize,
    sched: Mutex<Sched>,
    failure: Mutex<Option<anyhow::Error>>,
    failed: AtomicBool,
    /// Debug timing: (start, end, kind) of every work item, kind 0 decode,
    /// 1 write, 2 rebuild.
    trace: Option<(Instant, Mutex<Vec<(f64, f64, u8)>>)>,
}

impl Extraction<'_> {
    fn primary(&self, o: u32) -> usize {
        self.receivers[&o][0]
    }

    fn write_whole(&self, o: u32, content: &[u8]) -> Result<()> {
        // Small duplicates are written again while the content is at hand:
        // cheaper than a clone afterwards, which saves no space at this size.
        let all = &self.receivers[&o];
        let n = if content.len() < CLONE_MIN { all.len() } else { 1 };
        for &i in &all[..n] {
            let f = create_file(self.targets[i].as_ref().unwrap())?;
            f.write_all_at(content, 0)?;
            apply_metadata_fd(&f, &self.archive.index.entries[i], self.quarantine)?;
            self.fd_done[i].store(true, Ordering::Relaxed);
        }
        Ok(())
    }

    fn advance(&self, n: u64) {
        let done = self.bytes.fetch_add(n, Ordering::Relaxed) + n;
        self.hooks.report(done, self.total);
    }

    /// Runs the best ready item, then spawns one job per item it unlocked.
    fn job<'s>(&'s self, s: &rayon::Scope<'s>) {
        let w = {
            let mut st = self.sched.lock().unwrap();
            st.running += 1;
            st.pop().expect("one ready item per job")
        };
        let failed = self.failed.load(Ordering::Relaxed);
        let t0 = self.trace.as_ref().map(|(s, _)| s.elapsed().as_secs_f64());
        let res = if failed { Ok(None) } else { self.hooks.check().and_then(|_| self.run(w)) };
        if let (Some((s, v)), Some(t0)) = (&self.trace, t0) {
            let kind = match w {
                Work::Decode(_) => 0,
                Work::Write(..) => 1,
                Work::Rebuild(_) => 2,
            };
            v.lock().unwrap().push((t0, s.elapsed().as_secs_f64(), kind));
        }
        let spawn = {
            let mut st = self.sched.lock().unwrap();
            st.running -= 1;
            if matches!(w, Work::Write(..)) {
                st.writing -= 1;
            }
            match res {
                Ok(_) if failed => 0,
                Ok(decoded) => {
                    let before = st.queued();
                    self.finish(&mut st, w, decoded);
                    st.queued() - before
                }
                Err(e) => {
                    self.failed.store(true, Ordering::Relaxed);
                    self.failure.lock().unwrap().get_or_insert(e);
                    0
                }
            }
        };
        for _ in 0..spawn {
            s.spawn(move |s| self.job(s));
        }
    }

    fn run(&self, w: Work) -> Result<Option<Vec<u8>>> {
        let idx = &self.archive.index;
        match w {
            Work::Decode(b) => return self.archive.read_block(b).map(Some),
            Work::Write(b, r) => {
                let _p = prof::span("write.plain", 0);
                let data = self.sched.lock().unwrap().data[b as usize].clone().expect("block decoded");
                let (start, end) = self.runs[b as usize][r as usize];
                for &(o, off, c) in &self.units[b as usize][start as usize..end as usize] {
                    let ch = &idx.chunks[c as usize];
                    let bytes = &data[ch.offset as usize..(ch.offset + ch.len) as usize];
                    if self.dest {
                        if idx.objects[o as usize].chunks.len() == 1 {
                            self.write_whole(o, bytes)?;
                        } else {
                            // Chunks of a larger file arrive in any order; whichever
                            // comes first creates the file.
                            let f = create_file(self.targets[self.primary(o)].as_ref().unwrap())?;
                            f.write_all_at(bytes, off)?;
                        }
                    }
                    self.advance(ch.len);
                }
            }
            Work::Rebuild(o) => {
                let obj = &idx.objects[o as usize];
                let blocks: HashMap<u32, Arc<Vec<u8>>> = {
                    let st = self.sched.lock().unwrap();
                    self.parts[&o].iter().map(|&b| (b, st.data[b as usize].clone().expect("block decoded"))).collect()
                };
                let mut stored = Vec::with_capacity(obj.stored_len as usize);
                for &c in &obj.chunks {
                    let ch = &idx.chunks[c as usize];
                    stored.extend_from_slice(&blocks[&ch.block][ch.offset as usize..(ch.offset + ch.len) as usize]);
                }
                drop(blocks);
                let content = {
                    let _p = prof::span(
                        match obj.transform {
                            Transform::Lepton => "inverse.lepton",
                            Transform::JpegLite => "inverse.jpeglite",
                            Transform::Deflate => "inverse.deflate",
                            Transform::Arm64 { .. } => "inverse.arm64",
                            Transform::None => "inverse.none",
                        },
                        obj.size,
                    );
                    transform::inverse(stored, &obj.transform, obj.size)?
                };
                ensure!(Some(self.archive.hasher.hash(&content)) == obj.hash, "object {o} failed verification");
                if self.dest {
                    let _p = prof::span("write.rebuilt", obj.size);
                    self.write_whole(o, &content)?;
                }
                self.advance(obj.size);
            }
        }
        Ok(None)
    }

    /// After `w` ran: keep a decoded block and queue what it unlocks, drop
    /// blocks nothing needs any more, and admit more decodes.
    fn finish(&self, st: &mut Sched, w: Work, decoded: Option<Vec<u8>>) {
        let idx = &self.archive.index;
        match w {
            Work::Decode(b) => {
                st.data[b as usize] = decoded.map(Arc::new);
                for (r, &(start, end)) in self.runs[b as usize].iter().enumerate() {
                    let cost = self.units[b as usize][start as usize..end as usize]
                        .iter()
                        .map(|&(_, _, c)| idx.chunks[c as usize].len + FILE_COST)
                        .sum();
                    st.push(1, cost, Work::Write(b, r as u32));
                }
                for &o in &self.unlocks[b as usize] {
                    let left = st.waiting.get_mut(&o).expect("object waits for its blocks");
                    *left -= 1;
                    if *left == 0 {
                        st.push(1, rebuild_cost(&idx.objects[o as usize]), Work::Rebuild(o));
                    }
                }
            }
            Work::Write(b, _) => st.release(b, self),
            Work::Rebuild(o) => {
                for &b in &self.parts[&o] {
                    st.release(b, self);
                }
            }
        }
        st.admit(self);
    }
}

/// Distinct blocks an object's chunks live in, ascending.
fn blocks_of(idx: &Index, o: u32) -> Vec<u32> {
    let mut bs: Vec<u32> = idx.objects[o as usize].chunks.iter().map(|&c| idx.chunks[c as usize].block).collect();
    bs.sort_unstable();
    bs.dedup();
    bs
}

/// Threads allowed to create files at once. APFS serialises file creation:
/// past ~4 threads it gets no faster and the others spin in the kernel (on the
/// saved folder, 10 threads spent 3.1 s of CPU creating 11,050 files, 4 threads
/// 2.2 s in the same 0.43 s), CPU that decoding could use.
const CREATORS: usize = 4;

static CREATE_GATE: (Mutex<usize>, std::sync::Condvar) = (Mutex::new(0), std::sync::Condvar::new());

/// Holds one of the `CREATORS` places while alive.
struct Creating;

impl Creating {
    fn enter() -> Self {
        let mut n = CREATE_GATE.0.lock().unwrap();
        while *n >= CREATORS {
            n = CREATE_GATE.1.wait(n).unwrap();
        }
        *n += 1;
        Creating
    }
}

impl Drop for Creating {
    fn drop(&mut self) {
        *CREATE_GATE.0.lock().unwrap() -= 1;
        CREATE_GATE.1.notify_one();
    }
}

/// Opens `p` for writing, creating it and, on first use, its parent
/// directories. Creating directories as files need them spreads that work over
/// the whole extraction: APFS serialises `mkdir`, and doing them all up front
/// cost ~0.1-1 s with every core waiting.
pub(crate) fn create_file(p: &Path) -> Result<File> {
    let _gate = Creating::enter();
    let open = || OpenOptions::new().write(true).create(true).truncate(false).custom_flags(libc::O_NOFOLLOW).open(p);
    let f = match open() {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
            }
            open()
        }
        r => r,
    };
    f.with_context(|| format!("creating {}", p.display()))
}

/// Joins an archive path onto `dest`, rejecting anything that could escape it.
pub(crate) fn safe_join(dest: &Path, rel: &str) -> Result<PathBuf> {
    let rp = Path::new(rel);
    ensure!(!rel.is_empty() && !rel.contains('\0'), "invalid entry path");
    for c in rp.components() {
        match c {
            Component::Normal(_) => {}
            _ => bail!("unsafe entry path {rel:?}"),
        }
    }
    Ok(dest.join(rp))
}

/// `<sys/clonefile.h>`: don't follow a symlink at `from`.
const CLONE_NOFOLLOW: u32 = 0x0001;

/// Duplicates at least this large are cloned; smaller ones are written again.
const CLONE_MIN: usize = 64 << 10;

fn clone_or_copy(from: &Path, to: &Path) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let _gate = Creating::enter();
    let a = std::ffi::CString::new(from.as_os_str().as_bytes())?;
    let b = std::ffi::CString::new(to.as_os_str().as_bytes())?;
    // SAFETY: both are valid NUL-terminated paths; clonefile does not retain them.
    let rc = unsafe { libc::clonefile(a.as_ptr(), b.as_ptr(), CLONE_NOFOLLOW) };
    if rc != 0 {
        std::fs::copy(from, to)?;
    }
    Ok(())
}

/// Attributes for `setattrlist`: modification and access time, then the mode,
/// in the order of their bits.
#[repr(C, packed(4))]
struct ModeTimes {
    modified: libc::timespec,
    accessed: libc::timespec,
    mode: u32,
}

fn mode_times(e: &Entry) -> (libc::attrlist, ModeTimes) {
    let list = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: libc::ATTR_CMN_MODTIME | libc::ATTR_CMN_ACCTIME | libc::ATTR_CMN_ACCESSMASK,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    let t = libc::timespec { tv_sec: e.mtime.0 as libc::time_t, tv_nsec: e.mtime.1 as libc::c_long };
    (list, ModeTimes { modified: t, accessed: t, mode: e.mode & 0o7777 })
}

pub(crate) const QUARANTINE: &str = "com.apple.quarantine";

/// Metadata through an open descriptor: no path lookups, no symlink races.
/// Mode and times go in one call (a chmod and a utimes cost two). `quarantine`,
/// when given, is set too.
pub(crate) fn apply_metadata_fd(f: &File, e: &Entry, quarantine: Option<&[u8]>) -> Result<()> {
    use std::os::fd::AsRawFd;
    use xattr::FileExt as _;
    for (k, v) in &e.xattrs {
        let _ = f.set_xattr(k, v);
    }
    if let Some(q) = quarantine {
        let _ = f.set_xattr(QUARANTINE, q);
    }
    let (mut list, mut buf) = mode_times(e);
    // SAFETY: `list` describes exactly the attributes `buf` holds, in order.
    let rc = unsafe {
        libc::fsetattrlist(
            f.as_raw_fd(),
            (&mut list as *mut libc::attrlist).cast(),
            (&mut buf as *mut ModeTimes).cast(),
            std::mem::size_of::<ModeTimes>(),
            0,
        )
    };
    if rc != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

/// A directory's mode and times by path, in one call.
fn set_dir_mode_times(p: &Path, e: &Entry) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(p.as_os_str().as_bytes())?;
    let (mut list, mut buf) = mode_times(e);
    // SAFETY: as in `apply_metadata_fd`; `c` is a NUL-terminated path.
    let rc = unsafe {
        libc::setattrlist(
            c.as_ptr(),
            (&mut list as *mut libc::attrlist).cast(),
            (&mut buf as *mut ModeTimes).cast(),
            std::mem::size_of::<ModeTimes>(),
            libc::FSOPT_NOFOLLOW,
        )
    };
    if rc != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

pub(crate) fn apply_metadata(p: &Path, e: &Entry, quarantine: Option<&[u8]>) -> Result<()> {
    let is_link = matches!(e.kind, EntryKind::Symlink { .. });
    // A clone inherits its source's attributes; keep only this entry's own.
    if matches!(e.kind, EntryKind::File { .. }) {
        if let Ok(names) = xattr::list(p) {
            for name in names {
                let own = e.xattrs.iter().any(|(k, _)| std::ffi::OsStr::new(k) == name)
                    || (quarantine.is_some() && name == QUARANTINE);
                if !own {
                    let _ = xattr::remove(p, &name);
                }
            }
        }
    }
    for (k, v) in &e.xattrs {
        // Best effort: some attributes are not settable on every filesystem.
        let _ = xattr::set(p, k, v);
    }
    if let Some(q) = quarantine {
        // On a link itself, not what it points at.
        let _ = xattr::set(p, QUARANTINE, q);
    }
    if matches!(e.kind, EntryKind::Dir) {
        return set_dir_mode_times(p, e);
    }
    let mtime = filetime::FileTime::from_unix_time(e.mtime.0, e.mtime.1);
    if is_link {
        filetime::set_symlink_file_times(p, mtime, mtime)?;
    } else {
        if !matches!(e.kind, EntryKind::HardLink { .. }) {
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(e.mode))?;
        }
        filetime::set_file_times(p, mtime, mtime)?;
    }
    Ok(())
}
