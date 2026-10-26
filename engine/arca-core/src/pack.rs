//! Compression pipeline.
//!
//! ```text
//! scan ─► sniff (parallel) ─► order by similarity ─► per window:
//!   hash files (parallel) ─► whole-file dedup ─► transform + CDC chunk (parallel)
//!   ─► chunk dedup + lane routing ─► blocks ─► compress (parallel) ─► writer
//! ```
//! Windows bound memory; block compression of one window overlaps with the
//! transforms of the next.

use crate::classify::{self, Class, Lane, SNIFF_LEN};
use crate::codec::{self, CodecConfig};
use crate::format::{self, BlockInfo, Chunk, Codec, Entry, EntryKind, Footer, Index, Object, Transform};
use crate::hooks::Hooks;
use crate::scan;
use crate::transform::{self, TransformOpts};
use anyhow::{bail, ensure, Context, Result};
use crossbeam_channel::{bounded, Receiver, Sender};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Mutex;
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Fast,
    Balanced,
    /// Smallest archives that still extract at LZMA speed.
    Max,
    /// Smallest archives, period: PPMd on text, whose decoder is as slow as its encoder.
    Ultra,
}

#[derive(Debug, Clone)]
pub struct PackOptions {
    pub level: Level,
    pub threads: usize,
    pub block_size: usize,
    /// Block size for the text lane (smaller blocks decode on more cores).
    pub text_block_size: usize,
    /// Inputs smaller than this get blocks shrunk to one per core, so small jobs
    /// still use every core. 0 disables shrinking.
    pub adaptive_below: u64,
    pub transforms: TransformOpts,
    pub dedup: bool,
    /// Probe binary chunks and store incompressible ones raw (saves CPU, may
    /// cost a little ratio on data with long-range redundancy).
    pub probe: bool,
    pub text_codec: CodecConfig,
    pub binary_codec: CodecConfig,
    pub hooks: Hooks,
}

impl PackOptions {
    pub fn preset(level: Level) -> Self {
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        let zstd = |level| CodecConfig { codec: Codec::Zstd, level, extreme: false };
        let xz = |extreme| CodecConfig { codec: Codec::Xz, level: 9, extreme };
        // Per lane: text needs the expensive match finders (zstd's binary tree, xz's
        // extreme mode) to reach full ratio; binaries lose <0.4% with the cheaper
        // ones and gain it back from bigger blocks, at 3-4x the speed.
        let (text, binary, text_block, block, adaptive_below) = match level {
            Level::Fast => (zstd(3), zstd(3), 8 << 20, 8 << 20, u64::MAX),
            Level::Balanced => (zstd(15), zstd(12), 16 << 20, 64 << 20, 64 << 20),
            Level::Max => (xz(true), xz(true), 64 << 20, 64 << 20, 0),
            // Context modelling beats LZ on prose and source code, at the cost of decode speed.
            Level::Ultra => (CodecConfig { codec: Codec::Ppmd, level: 16, extreme: false }, xz(true), 64 << 20, 64 << 20, 0),
        };
        let heavy = level != Level::Fast;
        PackOptions {
            level,
            threads,
            block_size: block,
            text_block_size: text_block,
            adaptive_below,
            transforms: TransformOpts {
                jpeg: heavy,
                deflate: heavy,
                arm64: true,
                verify: true,
                // Most real-world Deflate comes from zlib levels 1-7 (search depth
                // <= 256); deeper search only helps the rare level-8/9 streams.
                deflate_chain: if level == Level::Balanced { 256 } else { 4096 },
            },
            dedup: true,
            probe: matches!(level, Level::Fast | Level::Balanced),
            text_codec: text,
            binary_codec: binary,
            hooks: Hooks::default(),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct TransformStat {
    pub files: u64,
    pub input: u64,
    pub stored: u64,
}

#[derive(Debug, Default, Clone)]
pub struct PackStats {
    pub files: u64,
    pub dirs: u64,
    pub input_bytes: u64,
    pub dup_file_bytes: u64,
    pub dup_chunk_bytes: u64,
    pub transforms: Vec<(&'static str, TransformStat)>,
    /// (lane name, raw bytes, compressed bytes)
    pub lanes: Vec<(&'static str, u64, u64)>,
    pub blocks: u64,
    pub index_bytes: u64,
    pub output_bytes: u64,
    pub seconds: f64,
}

const CDC_MIN_FILE: usize = 1 << 20;
const CDC_MIN: u32 = 128 << 10;
const CDC_AVG: u32 = 512 << 10;
const CDC_MAX: u32 = 2 << 20;
/// How far (in input bytes) file tasks may run ahead of the packer.
const LOOKAHEAD_BYTES: u64 = 512 << 20;
const MMAP_THRESHOLD: u64 = 256 << 10;

enum FileData {
    Mem(Vec<u8>),
    Map(memmap2::Mmap),
}

impl std::ops::Deref for FileData {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            FileData::Mem(v) => v,
            FileData::Map(m) => m,
        }
    }
}

fn read_file(path: &Path, size: u64) -> Result<FileData> {
    let mut f = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    if size >= MMAP_THRESHOLD {
        // SAFETY: the archive records the hash of what was read; a file modified
        // while mapped yields a consistent archive of whatever bytes we saw.
        let map = unsafe { memmap2::Mmap::map(&f) }.with_context(|| format!("mapping {}", path.display()))?;
        Ok(FileData::Map(map))
    } else {
        let mut v = Vec::with_capacity(size as usize);
        f.read_to_end(&mut v).with_context(|| format!("reading {}", path.display()))?;
        Ok(FileData::Mem(v))
    }
}

/// Small files are read whole while sniffing, so each is opened only once.
const SMALL_FILE: u64 = 64 << 10;
/// Cap on small-file contents held between sniffing and packing.
const SMALL_CACHE_BYTES: u64 = 256 << 20;

/// Classifies a file from its first bytes; returns its whole content when small.
fn sniff(path: &Path, size: u64, cache_left: &AtomicI64) -> (Class, Option<Vec<u8>>) {
    let Ok(mut f) = File::open(path) else { return (Class::Binary, None) };
    if size <= SMALL_FILE && cache_left.fetch_sub(size as i64, Ordering::Relaxed) >= size as i64 {
        let mut v = Vec::with_capacity(size as usize);
        if f.read_to_end(&mut v).is_ok() {
            return (classify::classify(&v[..v.len().min(SNIFF_LEN)]), Some(v));
        }
        return (Class::Binary, None);
    }
    let mut head = [0u8; SNIFF_LEN];
    let mut n = 0;
    while n < head.len() {
        match f.read(&mut head[n..]) {
            Ok(0) | Err(_) => break,
            Ok(k) => n += k,
        }
    }
    (classify::classify(&head[..n]), None)
}

struct Job {
    entry: usize,
    source: PathBuf,
    size: u64,
    class: Class,
    /// Content read while sniffing (small files only); taken by the file task.
    content: Mutex<Option<Vec<u8>>>,
}

struct OpenBlock {
    id: u32,
    lane: Lane,
    buf: Vec<u8>,
}

struct Done {
    id: u32,
    comp: Vec<u8>,
    raw_len: u64,
    raw_hash: [u8; 16],
    codec: Codec,
}

struct BlockSink {
    pool: Arc<rayon::ThreadPool>,
    slots_tx: Sender<()>,
    slots_rx: Receiver<()>,
    done_tx: Sender<Result<Done>>,
    open: [Option<OpenBlock>; 3],
    next_id: u32,
    block_sizes: [usize; 3],
    codecs: [CodecConfig; 3],
    lanes: Vec<Lane>,
}

impl BlockSink {
    fn append(&mut self, lane: Lane, bytes: &[u8]) -> (u32, u64) {
        let limit = self.block_sizes[lane as usize];
        let slot = &mut self.open[lane as usize];
        if slot.is_none() {
            *slot = Some(OpenBlock { id: self.next_id, lane, buf: Vec::with_capacity(limit + CDC_MAX as usize) });
            self.lanes.push(lane);
            self.next_id += 1;
        }
        let b = slot.as_mut().unwrap();
        let at = (b.id, b.buf.len() as u64);
        b.buf.extend_from_slice(bytes);
        if b.buf.len() >= limit {
            let full = slot.take().unwrap();
            self.close(full);
        }
        at
    }

    fn close(&mut self, block: OpenBlock) {
        // Bound blocks in flight so memory stays flat on huge inputs.
        self.slots_rx.recv().expect("slot channel");
        let cfg = self.codecs[block.lane as usize];
        let (done_tx, slots_tx) = (self.done_tx.clone(), self.slots_tx.clone());
        self.pool.spawn(move || {
            let res = codec::compress(&block.buf, cfg).map(|comp| {
                // Never let a codec expand data: fall back to storing the block.
                let (comp, codec) =
                    if comp.len() >= block.buf.len() { (block.buf.clone(), Codec::Store) } else { (comp, cfg.codec) };
                Done { id: block.id, comp, raw_len: block.buf.len() as u64, raw_hash: format::hash16(&block.buf), codec }
            });
            let _ = done_tx.send(res);
            let _ = slots_tx.send(());
        });
    }

    fn flush_lane(&mut self, lane: Lane) {
        if let Some(b) = self.open[lane as usize].take() {
            self.close(b);
        }
    }

    fn flush(&mut self) {
        for lane in [Lane::Text, Lane::Binary, Lane::Store] {
            self.flush_lane(lane);
        }
    }
}

/// Sort group: 0 feeds the text lane, 1 the binary lane, 2 mostly the store lane.
fn order_group(class: Class) -> u8 {
    match class {
        Class::Text => 0,
        Class::Compressed | Class::Jpeg => 2,
        _ => 1,
    }
}

fn order_key(job: &Job, entries: &[Entry]) -> (u8, Class, String, u64, String) {
    let path = &entries[job.entry].path;
    let name = path.rsplit('/').next().unwrap_or(path);
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    // Lepton output is stored raw, so JPEG order can't affect size: biggest first
    // keeps the slowest files from finishing alone at the end.
    let size_first = if job.class == Class::Jpeg { u64::MAX - job.size } else { 0 };
    (order_group(job.class), job.class, ext, size_first, name.to_owned())
}

/// Packs `inputs` into a new archive at `output`. The archive is written to a
/// temporary name and renamed into place only when complete; on error or
/// cancellation nothing is left behind.
pub fn pack(inputs: &[PathBuf], output: &Path, opts: &PackOptions) -> Result<PackStats> {
    let tmp_path = output.with_extension("arca.partial");
    let result = pack_to(inputs, output, &tmp_path, opts);
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    }
    result
}

fn pack_to(inputs: &[PathBuf], output: &Path, tmp_path: &Path, opts: &PackOptions) -> Result<PackStats> {
    let start = Instant::now();
    let pool = rayon::ThreadPoolBuilder::new().num_threads(opts.threads.max(1)).build()?;

    let timing = std::env::var_os("ARCA_DEBUG_TIMING").is_some();
    let phase = |name: &str| {
        if timing {
            eprintln!("[{:>7.3}s] {name}", start.elapsed().as_secs_f64());
        }
    };
    let scanned = scan::scan(inputs)?;
    phase("scanned");
    let mut entries: Vec<Entry> = Vec::with_capacity(scanned.len());
    let mut jobs: Vec<Job> = Vec::new();
    let mut stats = PackStats::default();
    for (i, s) in scanned.into_iter().enumerate() {
        match (&s.entry.kind, s.source) {
            (EntryKind::File { size, .. }, Some(source)) => {
                stats.files += 1;
                stats.input_bytes += size;
                jobs.push(Job { entry: i, source, size: *size, class: Class::Binary, content: Mutex::new(None) });
            }
            (EntryKind::Dir, _) => stats.dirs += 1,
            _ => {}
        }
        entries.push(s.entry);
    }

    pool.install(|| {
        use rayon::prelude::*;
        let cache_left = AtomicI64::new(SMALL_CACHE_BYTES as i64);
        jobs.par_iter_mut().with_max_len(64).for_each(|j| {
            if j.size == 0 {
                j.class = Class::Text;
            } else {
                let (class, content) = sniff(&j.source, j.size, &cache_left);
                j.class = class;
                *j.content.get_mut().unwrap() = content;
            }
        });
    });
    jobs.sort_by_cached_key(|j| order_key(j, &entries));
    phase("sniffed and sorted");

    let (mut block_size, mut text_block_size) = (opts.block_size, opts.text_block_size);
    if stats.input_bytes < opts.adaptive_below {
        let per_core = (stats.input_bytes / opts.threads.max(1) as u64).max(1 << 20) as usize;
        block_size = block_size.min(per_core);
        text_block_size = text_block_size.min(per_core);
    }

    // Write to a temporary name and rename at the end: never leave a half archive.
    let mut out = File::create(tmp_path).with_context(|| format!("creating {}", tmp_path.display()))?;
    out.write_all(&format::header_bytes(0))?;

    let in_flight = opts.threads.max(1) * 2;
    let (slots_tx, slots_rx) = bounded::<()>(in_flight);
    for _ in 0..in_flight {
        slots_tx.send(()).unwrap();
    }
    let (done_tx, done_rx) = bounded::<Result<Done>>(in_flight);
    let writer = std::thread::spawn(move || -> Result<(File, Vec<(u32, BlockInfo)>)> {
        let mut pos = format::HEADER_LEN as u64;
        let mut infos = Vec::new();
        for done in done_rx {
            let d = done?;
            out.write_all(&d.comp)?;
            infos.push((
                d.id,
                BlockInfo {
                    file_offset: pos,
                    comp_len: d.comp.len() as u64,
                    raw_len: d.raw_len,
                    codec: d.codec,
                    checksum: format::hash16(&d.comp),
                    raw_hash: d.raw_hash,
                },
            ));
            pos += d.comp.len() as u64;
        }
        Ok((out, infos))
    });

    let store = CodecConfig { codec: Codec::Store, level: 0, extreme: false };
    let mut sink = BlockSink {
        // Its own pool: blocks must not queue behind file tasks, or compression
        // trails at the end instead of overlapping with the transforms.
        pool: Arc::new(rayon::ThreadPoolBuilder::new().num_threads(opts.threads.max(1)).build()?),
        slots_tx,
        slots_rx,
        done_tx,
        open: [None, None, None],
        next_id: 0,
        block_sizes: [text_block_size, block_size, block_size],
        codecs: [opts.text_codec, opts.binary_codec, store],
        lanes: Vec::new(),
    };

    let mut objects: Vec<Object> = Vec::new();
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut by_hash: HashMap<[u8; 32], u32> = HashMap::new();
    let mut by_chunk: HashMap<[u8; 32], u32> = HashMap::new();
    let mut tstats: HashMap<&'static str, TransformStat> = HashMap::new();
    let debug = std::env::var_os("ARCA_DEBUG_ROUTING").is_some();

    // Streaming pipeline. Every file is its own task (read, hash, transform,
    // chunk) running up to LOOKAHEAD_BYTES ahead of the packer, which consumes
    // results strictly in sort order. Blocks therefore fill, and start
    // compressing, continuously; no batch waits for its slowest file; and the
    // archive layout stays deterministic.
    //
    // Duplicates: the first task to hash some content claims it and transforms
    // it; later tasks with the same hash skip the work. The packer creates objects
    // in order, taking the claimer's result when it reaches the first occurrence.
    let n = jobs.len();
    let slots: Vec<Mutex<Option<Result<Hashed>>>> = (0..n).map(|_| Mutex::new(None)).collect();
    let claims: Mutex<HashMap<[u8; 32], usize>> = Mutex::new(HashMap::new());
    let cancel = AtomicBool::new(false);
    let (ready_tx, ready_rx) = crossbeam_channel::unbounded::<()>();
    // Sort groups feed lanes in order, so each lane's last block can be sealed,
    // and start compressing, as soon as its group has gone by.
    let text_end = jobs.iter().position(|j| order_group(j.class) > 0).unwrap_or(n);
    let binary_end = jobs.iter().position(|j| order_group(j.class) > 1).unwrap_or(n);

    let jobs = &jobs;
    pool.in_place_scope(|scope| -> Result<()> {
        let (mut spawned, mut inflight, mut done_bytes) = (0usize, 0u64, 0u64);
        let take = |k: usize| -> Option<Result<Hashed>> { slots[k].lock().unwrap().take() };
        let wait_for = |k: usize| -> Result<Hashed> {
            loop {
                if let Some(r) = take(k) {
                    return r;
                }
                ready_rx.recv().context("pipeline stalled")?;
            }
        };
        let result = (|| -> Result<()> {
            for k in 0..n {
                while spawned < n && (spawned == k || inflight + jobs[spawned].size <= LOOKAHEAD_BYTES) {
                    let i = spawned;
                    inflight += jobs[i].size;
                    spawned += 1;
                    let (slots, claims, cancel, tx) = (&slots, &claims, &cancel, ready_tx.clone());
                    scope.spawn(move |_| {
                        let r = if cancel.load(Ordering::Relaxed) {
                            Err(anyhow::anyhow!("cancelled"))
                        } else {
                            process_file(&jobs[i], i, claims, opts)
                        };
                        *slots[i].lock().unwrap() = Some(r);
                        let _ = tx.send(());
                    });
                }

                let job = &jobs[k];
                let mine = wait_for(k)?;
                inflight -= job.size;
                let obj = match by_hash.get(&mine.hash) {
                    Some(&o) if opts.dedup => {
                        stats.dup_file_bytes += job.size;
                        o
                    }
                    _ => {
                        // First occurrence in order: the content was transformed by
                        // whichever task claimed it (this one, or a later duplicate).
                        let claimer = claims.lock().unwrap()[&mine.hash];
                        let prepared = match mine.prepared {
                            Some(p) => p,
                            None => {
                                ensure!(claimer > k, "dedup claim out of order");
                                let mut theirs = wait_for(claimer)?;
                                let p = theirs.prepared.take().context("claimed content missing")?;
                                // Put the (now content-less) result back for its own turn.
                                *slots[claimer].lock().unwrap() = Some(Ok(theirs));
                                p
                            }
                        };
                        let o = objects.len() as u32;
                        let (transform, owned, lane, cuts, data) = prepared;
                        let stored: &[u8] = owned.as_deref().unwrap_or(&data);
                        if debug {
                            eprintln!("{:?}\t{:?}\t{}\t{}\t{}", lane, job.class, transform.name(), data.len(), entries[job.entry].path);
                        }
                        let t = tstats.entry(transform.name()).or_default();
                        t.files += 1;
                        t.input += data.len() as u64;
                        t.stored += stored.len() as u64;

                        let mut ids = Vec::with_capacity(cuts.len());
                        for (off, len, hash, lane) in cuts {
                            if opts.dedup {
                                if let Some(&c) = by_chunk.get(&hash) {
                                    if chunks[c as usize].len == len as u64 {
                                        stats.dup_chunk_bytes += len as u64;
                                        ids.push(c);
                                        continue;
                                    }
                                }
                            }
                            let (block, offset) = sink.append(lane, &stored[off..off + len]);
                            let c = chunks.len() as u32;
                            chunks.push(Chunk { block, offset, len: len as u64 });
                            by_chunk.insert(hash, c);
                            ids.push(c);
                        }
                        let hash = (transform != Transform::None).then(|| mine.hash[..16].try_into().unwrap());
                        objects.push(Object { hash, size: job.size, transform, stored_len: stored.len() as u64, chunks: ids });
                        by_hash.insert(mine.hash, o);
                        o
                    }
                };
                if let EntryKind::File { object, .. } = &mut entries[job.entry].kind {
                    *object = obj;
                }
                done_bytes += job.size;
                opts.hooks.report(done_bytes, stats.input_bytes);
                opts.hooks.check()?;
                if k + 1 == text_end {
                    sink.flush_lane(Lane::Text);
                }
                if k + 1 == binary_end {
                    sink.flush_lane(Lane::Binary);
                }
            }
            Ok(())
        })();
        if result.is_err() {
            cancel.store(true, Ordering::Relaxed);
        }
        result
    })?;
    phase("all files transformed");
    sink.flush();
    let lanes = std::mem::take(&mut sink.lanes);
    drop(sink);

    let (mut out, infos) = writer.join().map_err(|_| anyhow::anyhow!("writer thread panicked"))??;
    phase("all blocks compressed and written");
    let mut blocks: Vec<Option<BlockInfo>> = vec![None; lanes.len()];
    for (id, info) in infos {
        blocks[id as usize] = Some(info);
    }
    let blocks: Vec<BlockInfo> = blocks.into_iter().collect::<Option<_>>().context("block missing from writer")?;

    let mut lane_stats = [("text", 0u64, 0u64), ("binary", 0, 0), ("store", 0, 0)];
    for (b, lane) in blocks.iter().zip(&lanes) {
        lane_stats[*lane as usize].1 += b.raw_len;
        lane_stats[*lane as usize].2 += b.comp_len;
    }
    stats.lanes = lane_stats.to_vec();
    stats.blocks = blocks.len() as u64;

    let created = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let index = Index { created, entries, objects, chunks, blocks };
    let raw = index.encode();
    let comp = zstd::bulk::compress(&raw, 19)?;
    let index_offset = index.blocks.iter().map(|b| b.file_offset + b.comp_len).max().unwrap_or(format::HEADER_LEN as u64);
    out.write_all(&comp)?;
    let footer = Footer {
        index_offset,
        index_comp_len: comp.len() as u64,
        index_raw_len: raw.len() as u64,
        index_hash: format::hash16(&comp),
    };
    out.write_all(&footer.to_bytes())?;
    out.sync_all()?;
    drop(out);
    std::fs::rename(tmp_path, output)?;
    phase("index written, synced");

    let mut t: Vec<_> = tstats.into_iter().collect();
    t.sort_by_key(|(k, _)| *k);
    stats.transforms = t;
    stats.index_bytes = comp.len() as u64;
    stats.output_bytes = std::fs::metadata(output)?.len();
    stats.seconds = start.elapsed().as_secs_f64();
    Ok(stats)
}

type Cut = (usize, usize, [u8; 32], Lane);
/// (transform, transformed bytes if any, lane, chunks, original bytes)
type Prepared = (Transform, Option<Vec<u8>>, Lane, Vec<Cut>, FileData);

struct Hashed {
    hash: [u8; 32],
    /// Present when this task claimed the content (first to hash it).
    prepared: Option<Prepared>,
}

/// Reads, hashes and, unless another task already claimed the same content,
/// transforms and chunks one file.
fn process_file(job: &Job, index: usize, claims: &Mutex<HashMap<[u8; 32], usize>>, opts: &PackOptions) -> Result<Hashed> {
    let cached = job.content.lock().unwrap().take();
    let data = match cached {
        Some(v) => FileData::Mem(v),
        None => read_file(&job.source, job.size)?,
    };
    if data.len() as u64 != job.size {
        bail!("{} changed size while archiving", job.source.display());
    }
    let hash = if data.len() > 1 << 20 {
        *blake3::Hasher::new().update_rayon(&data).finalize().as_bytes()
    } else {
        *blake3::hash(&data).as_bytes()
    };
    let claimed = {
        let mut c = claims.lock().unwrap();
        match c.get(&hash) {
            // Without dedup every file keeps its own content.
            Some(_) if opts.dedup => false,
            _ => {
                c.entry(hash).or_insert(index);
                true
            }
        }
    };
    if !claimed {
        return Ok(Hashed { hash, prepared: None });
    }
    let p = transform::forward(&data, job.class, &opts.transforms);
    let (transform, lane) = (p.transform, p.lane);
    let owned = match p.stored {
        std::borrow::Cow::Owned(v) => Some(v),
        std::borrow::Cow::Borrowed(_) => None,
    };
    let stored: &[u8] = owned.as_deref().unwrap_or(&data);
    let cuts = cut_chunks(stored, opts.dedup)
        .into_iter()
        .map(|(off, len, hash)| {
            let incompressible =
                opts.probe && lane == Lane::Binary && classify::probably_incompressible(&stored[off..off + len]);
            (off, len, hash, if incompressible { Lane::Store } else { lane })
        })
        .collect::<Vec<_>>();
    Ok(Hashed { hash, prepared: Some((transform, owned, lane, cuts, data)) })
}

/// Chunk boundaries with full 256-bit hashes (the dedup key; the index keeps 128 bits).
fn cut_chunks(stored: &[u8], dedup: bool) -> Vec<(usize, usize, [u8; 32])> {
    let hash = |s: &[u8]| *blake3::hash(s).as_bytes();
    let mut cuts = Vec::new();
    if stored.is_empty() {
        return cuts;
    }
    if dedup && stored.len() >= CDC_MIN_FILE {
        for c in fastcdc::v2020::FastCDC::new(stored, CDC_MIN, CDC_AVG, CDC_MAX) {
            cuts.push((c.offset, c.length, hash(&stored[c.offset..c.offset + c.length])));
        }
    } else {
        for off in (0..stored.len()).step_by(CDC_MAX as usize) {
            let len = (stored.len() - off).min(CDC_MAX as usize);
            cuts.push((off, len, hash(&stored[off..off + len])));
        }
    }
    cuts
}
