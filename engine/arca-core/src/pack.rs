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
use crate::prof;
use crate::scan;
use crate::transform::{self, TransformOpts};
use anyhow::{bail, ensure, Context, Result};
use crossbeam_channel::{bounded, Receiver, Sender};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    pub image_codec: CodecConfig,
    /// Executables and libraries get their own lane with this codec. When
    /// false they share the binary lane (and its codec), as before.
    pub code_lane: bool,
    pub code_codec: CodecConfig,
    /// Block size for the code lane.
    pub code_block_size: usize,
    /// zstd level for the index. 19 costs ~0.3 s on one thread for a 10,000-file
    /// folder and saves ~7% of an index that is under 0.1% of the archive.
    pub index_level: i32,
    /// Memory the pack may use, in bytes; 0 means half of the Mac's memory.
    pub memory_budget: u64,
    /// Order files by their sniffed content class (one extra pass that opens
    /// every file) instead of by extension alone.
    pub sniff_order: bool,
    pub hooks: Hooks,
}

impl PackOptions {
    pub fn preset(level: Level) -> Self {
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        let zstd = |level| CodecConfig { codec: Codec::Zstd, level, extreme: false };
        let lzma2 = |level, extreme| CodecConfig { codec: Codec::Lzma2, level, extreme };
        // Per lane: text needs the expensive match finders (zstd's binary tree, xz's
        // extreme mode) to reach full ratio; binaries lose <0.4% with the cheaper
        // ones and gain it back from bigger blocks, at 3-4x the speed.
        let (text, binary, text_block, block, adaptive_below) = match level {
            Level::Fast => (zstd(3), zstd(3), 8 << 20, 8 << 20, u64::MAX),
            Level::Balanced => (zstd(15), zstd(12), 16 << 20, 64 << 20, 64 << 20),
            // Max is the fast high-ratio level: LZMA2 with 32-byte matches (half the
            // CPU of 273-byte ones for 1-6% more output) for binaries and pixels,
            // while text and code (the code lane below) get the longest matches in
            // 64 MiB blocks: what source trees and Silesia need to beat 7-Zip Ultra.
            Level::Max => (lzma2(9, true), lzma2(7, false), 64 << 20, 64 << 20, 0),
            // Ultra is the smallest: context modelling for text (it beats LZ on
            // prose and source code, at the cost of decode speed) and the longest
            // LZMA2 matches.
            Level::Ultra => (CodecConfig { codec: Codec::Ppmd, level: 16, extreme: false }, lzma2(9, true), 64 << 20, 64 << 20, 0),
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
                // A few photo-like PNGs from level-9 encoders need millions of
                // searches to re-create (one took 3 s); they stay as they are.
                deflate_steps_per_byte: 128,
                png_pixels: matches!(level, Level::Max | Level::Ultra)
                    || std::env::var_os("ARCA_EXP_PIXELS").is_some(),
            },
            dedup: true,
            probe: matches!(level, Level::Fast | Level::Balanced),
            text_codec: text,
            binary_codec: binary,
            image_codec: binary,
            // At Max and Ultra code and text get long matches: what makes source
            // trees and apps small (7-Zip Ultra is the bar there).
            code_lane: matches!(level, Level::Max | Level::Ultra),
            code_codec: if matches!(level, Level::Max | Level::Ultra) { lzma2(9, true) } else { binary },
            code_block_size: block,
            index_level: if matches!(level, Level::Fast | Level::Balanced) { 9 } else { 19 },
            memory_budget: 0,
            // Ordering by content is 0.15% smaller at Default and costs ~0.1 s; at
            // Fast (small blocks) it changes nothing and costs 15% of the time.
            sniff_order: level != Level::Fast,
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

struct Job {
    entry: usize,
    source: PathBuf,
    size: u64,
    /// Sort group guessed from the file name (see `name_group`), or from the
    /// content class when sniffing.
    group: u8,
    /// Content class, when sniffed before ordering.
    class: Option<Class>,
}

/// Classifies a file from its first bytes.
fn sniff(path: &Path) -> Class {
    let _p = prof::span("pack.sniff", 0);
    let Ok(mut f) = File::open(path) else { return Class::Binary };
    let mut head = [0u8; SNIFF_LEN];
    let mut n = 0;
    while n < head.len() {
        match f.read(&mut head[n..]) {
            Ok(0) | Err(_) => break,
            Ok(k) => n += k,
        }
    }
    if n == 0 {
        Class::Text
    } else {
        classify::classify(&head[..n])
    }
}

/// Sort group from a sniffed class: 0 text, 1 binary lanes, 2 mostly stored.
fn class_group(class: Class) -> u8 {
    match class {
        Class::Text => 0,
        Class::Compressed | Class::Jpeg => 2,
        _ => 1,
    }
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

/// Prints one block's queue, start and end times when dropped (debug timing).
struct Timeline {
    start: Option<Instant>,
    id: u32,
    lane: Lane,
    len: usize,
    queued: Option<f64>,
    began: Option<f64>,
}

impl Drop for Timeline {
    fn drop(&mut self) {
        if let (Some(s), Some(q), Some(b)) = (self.start, self.queued, self.began) {
            eprintln!(
                "  block {:>3} {:<6} {:>5.1} MiB  queued {:>6.2}s  start {:>6.2}s  end {:>6.2}s",
                self.id,
                format!("{:?}", self.lane),
                self.len as f64 / 1048576.0,
                q,
                b,
                s.elapsed().as_secs_f64()
            );
        }
    }
}

struct BlockSink {
    pool: Arc<rayon::ThreadPool>,
    slots_tx: Sender<()>,
    slots_rx: Receiver<()>,
    done_tx: Sender<Result<Done>>,
    open: [Option<OpenBlock>; 5],
    next_id: u32,
    block_sizes: [usize; 5],
    codecs: [CodecConfig; 5],
    lanes: Vec<Lane>,
    /// Pack start, for the block timeline `ARCA_DEBUG_TIMING` prints.
    start: Option<Instant>,
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
        // Research hook: keep each block's raw bytes for codec experiments.
        if let Some(dir) = std::env::var_os("ARCA_DUMP_BLOCKS") {
            let name = format!("{:03}-{:?}.bin", block.id, block.lane);
            let _ = std::fs::write(Path::new(&dir).join(name), &block.buf);
        }
        // Bound blocks in flight so memory stays flat on huge inputs.
        self.slots_rx.recv().expect("slot channel");
        let cfg = self.codecs[block.lane as usize];
        let (done_tx, slots_tx) = (self.done_tx.clone(), self.slots_tx.clone());
        let start = self.start;
        let queued = start.map(|s| s.elapsed().as_secs_f64());
        self.pool.spawn(move || {
            let began = start.map(|s| s.elapsed().as_secs_f64());
            let _timeline = Timeline { start, id: block.id, lane: block.lane, len: block.buf.len(), queued, began };
            let _p = prof::span(
                match (cfg.codec, block.lane) {
                    (Codec::Store, _) => "block.store",
                    (_, Lane::Text) => "block.text",
                    (_, Lane::Image) => "block.image",
                    (_, Lane::Code) => "block.code",
                    _ => "block.binary",
                },
                block.buf.len() as u64,
            );
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
        for lane in [Lane::Text, Lane::Binary, Lane::Image, Lane::Code, Lane::Store] {
            self.flush_lane(lane);
        }
    }
}

/// Sort group from the extension: 0 for text, 2 for media and other
/// already-compressed formats, 1 for everything else. Files are ordered by name
/// before anything is read, so reading, sniffing and packing all stream in one
/// pass (a separate sniffing pass opened every file twice). The lane a file
/// lands in still comes from its content.
fn name_group(ext: &str) -> u8 {
    const TEXT: &[&str] = &[
        "txt", "md", "markdown", "rst", "tex", "csv", "tsv", "log", "rs", "c", "h", "cc", "cpp", "cxx", "hpp", "hh",
        "m", "mm", "swift", "py", "rb", "pl", "php", "lua", "go", "java", "kt", "scala", "js", "mjs", "cjs", "ts",
        "tsx", "jsx", "json", "toml", "yaml", "yml", "xml", "html", "htm", "css", "scss", "sh", "bash", "zsh", "fish",
        "ini", "cfg", "conf", "lock", "sql", "s", "asm", "cmake", "mk", "am", "in", "ac", "svg", "proto", "gradle",
        "properties", "bat", "ps1", "vue", "el", "hs", "ml", "erl", "ex", "exs", "clj", "dart", "r", "jl", "nim",
        "zig", "v", "sv", "vhd", "d", "f90", "pas", "cs", "fs", "vb", "awk", "sed", "diff", "patch", "po", "pot",
        "orig", "cargo-ok", "gitignore", "gitattributes", "gitmodules", "stderr", "stdout", "m4", "rc", "inc", "pc",
        "options", "1", "2", "3", "4", "5", "6", "7", "8", "9", "xsd", "xsl", "dtd", "bib", "mdx", "ipynb",
        "editorconfig", "npmignore", "dockerignore", "env", "def", "mak", "tcl", "pyi", "pxd", "pyx",
    ];
    const MEDIA: &[&str] = &[
        "jpg", "jpeg", "jpe", "heic", "heif", "avif", "webp", "gif", "jxl", "mp4", "m4v", "mov", "m4a", "mp3", "aac",
        "flac", "ogg", "oga", "opus", "mkv", "webm", "avi", "zst", "xz", "bz2", "7z", "rar", "lz4", "lzma", "aar",
    ];
    if TEXT.contains(&ext) {
        0
    } else if MEDIA.contains(&ext) {
        2
    } else {
        1
    }
}

/// Order within a sort group. PNGs, the costliest files to transform, go last
/// so the blocks of everything before them fill, and start compressing, while
/// they are still being expanded. PDF and ZIP-family data stays next to the
/// executables: putting plain binaries between them cost 1 MiB at Max.
fn class_rank(class: Option<Class>) -> u8 {
    use crate::classify::DeflateKind;
    match class {
        None => 0,
        Some(Class::Text) => 1,
        Some(Class::MachO) => 2,
        Some(Class::Code) => 2,
        Some(Class::DeflateContainer(DeflateKind::Zip)) => 3,
        Some(Class::DeflateContainer(DeflateKind::Pdf)) => 4,
        Some(Class::DeflateContainer(DeflateKind::Gzip)) => 5,
        Some(Class::Binary) => 6,
        Some(Class::DeflateContainer(DeflateKind::Png)) => 7,
        Some(Class::Compressed) => 8,
        Some(Class::Jpeg) => 9,
    }
}

/// Text files that usually have no extension.
fn text_name(name: &str) -> bool {
    const NAMES: &[&str] = &[
        "LICENSE", "LICENCE", "COPYING", "README", "NOTICE", "AUTHORS", "CHANGELOG", "CHANGES", "CONTRIBUTING",
        "COPYRIGHT", "UNLICENSE", "INSTALL", "NEWS", "TODO", "THANKS", "HISTORY", "PATENTS", "VERSION", "Makefile",
        "makefile", "GNUmakefile", "Dockerfile", "Gemfile", "Rakefile", "Procfile", "Vagrantfile", "BUILD", "WORKSPACE",
        "CODEOWNERS", "Podfile", "Brewfile", "Justfile", "justfile",
    ];
    NAMES.iter().any(|n| name.starts_with(n))
}

fn order_key(job: &Job, entries: &[Entry]) -> (u8, u8, String, u64, String) {
    let path = &entries[job.entry].path;
    let name = path.rsplit('/').next().unwrap_or(path);
    let ext = extension(name);
    // Lepton output is stored raw, so JPEG order can't affect size: biggest first
    // keeps the slowest files from finishing alone at the end.
    let jpeg = job.class.map_or(matches!(ext.as_str(), "jpg" | "jpeg" | "jpe"), |c| c == Class::Jpeg);
    let size_first = if jpeg { u64::MAX - job.size } else { 0 };
    (job.group, class_rank(job.class), ext, size_first, name.to_owned())
}

fn extension(name: &str) -> String {
    name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default()
}

/// Packs `inputs` into a new archive at `output`. The archive is written to a
/// temporary name and renamed into place only when complete; on error or
/// cancellation nothing is left behind.
pub fn pack(inputs: &[PathBuf], output: &Path, opts: &PackOptions) -> Result<PackStats> {
    let mut tmp_name = output.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".partial");
    let tmp_path = output.with_file_name(tmp_name);
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
    let scanned = {
        let _p = prof::span("pack.scan", 0);
        pool.install(|| scan::scan(inputs))?
    };
    phase("scanned");
    let mut entries: Vec<Entry> = Vec::with_capacity(scanned.len());
    let mut jobs: Vec<Job> = Vec::new();
    let mut stats = PackStats::default();
    for (i, s) in scanned.into_iter().enumerate() {
        match (&s.entry.kind, s.source) {
            (EntryKind::File { size, .. }, Some(source)) => {
                stats.files += 1;
                stats.input_bytes += size;
                let name = s.entry.path.rsplit('/').next().unwrap_or(&s.entry.path);
                let ext = extension(name);
                let group = if ext.is_empty() && text_name(name) { 0 } else { name_group(&ext) };
                jobs.push(Job { entry: i, source, size: *size, group, class: None });
            }
            (EntryKind::Dir, _) => stats.dirs += 1,
            _ => {}
        }
        entries.push(s.entry);
    }

    if opts.sniff_order {
        pool.install(|| {
            use rayon::prelude::*;
            jobs.par_iter_mut().with_max_len(64).for_each(|j| {
                let class = if j.size == 0 { Class::Text } else { sniff(&j.source) };
                j.class = Some(class);
                j.group = class_group(class);
            });
        });
    }
    jobs.sort_by_cached_key(|j| order_key(j, &entries));
    phase("sorted");

    let (mut block_size, mut text_block_size) = (opts.block_size, opts.text_block_size);
    if stats.input_bytes < opts.adaptive_below {
        let per_core = (stats.input_bytes / opts.threads.max(1) as u64).max(1 << 20) as usize;
        block_size = block_size.min(per_core);
        text_block_size = text_block_size.min(per_core);
    }

    // Write to a temporary name and rename at the end: never leave a half archive.
    let mut out = File::create(tmp_path).with_context(|| format!("creating {}", tmp_path.display()))?;
    out.write_all(&format::header_bytes(0))?;

    // Memory plan. Block encoders are the big consumers (xz -9e: ~0.67 GB each,
    // plus the block and its output), so their number follows the memory
    // budget, not just the core count: ten at once peaked at 8-10 GB, which an
    // 8 GB Mac can't hold.
    let budget = if opts.memory_budget > 0 { opts.memory_budget } else { codec::physical_memory() / 2 };
    let lookahead = LOOKAHEAD_BYTES.min(budget / 16);
    // Each encoder holds its block and its output; every lane fills one open block;
    // file tasks hold their working copies (expanded Deflate, Lepton buffers).
    let per_encoder = [
        (opts.text_codec, text_block_size),
        (opts.binary_codec, block_size),
        (opts.image_codec, block_size),
        (opts.code_codec, block_size),
    ]
        .into_iter()
        .map(|(c, b)| codec::encoder_memory(c, b) + 2 * b as u64)
        .max()
        .unwrap_or(0)
        .max(1);
    let open_blocks = 4 * (block_size.max(text_block_size) as u64 + CDC_MAX as u64);
    let task_memory = opts.threads as u64 * (64 << 20);
    let spare = budget.saturating_sub(lookahead + open_blocks + task_memory);
    let mut block_threads = (spare / per_encoder).clamp(1, opts.threads.max(1) as u64) as usize;
    // LZMA2 encoders run two threads each (the match finder has its own), so
    // about half as many of them keep the cores busy without starving the
    // first blocks: six on 10 cores made Max 4-6% faster than ten.
    if [opts.text_codec, opts.binary_codec, opts.image_codec, opts.code_codec].iter().any(|c| c.codec == Codec::Lzma2) {
        block_threads = block_threads.min(opts.threads.max(1) / 2 + 1);
    }
    if let Some(cap) = std::env::var("ARCA_EXP_ENCODERS").ok().and_then(|v| v.parse::<usize>().ok()) {
        block_threads = block_threads.min(cap.max(1));
    }
    if timing {
        eprintln!(
            "memory budget {} MiB: {block_threads} block encoders of {} MiB, lookahead {} MiB",
            budget >> 20,
            per_encoder >> 20,
            lookahead >> 20
        );
    }
    // Running blocks plus two queued: enough to keep every encoder fed.
    let in_flight = block_threads + 2;
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
        pool: Arc::new(rayon::ThreadPoolBuilder::new().num_threads(block_threads).build()?),
        slots_tx,
        slots_rx,
        done_tx,
        open: [None, None, None, None, None],
        next_id: 0,
        block_sizes: [text_block_size, block_size, block_size, block_size, opts.code_block_size.min(block_size.max(1 << 20))],
        codecs: [opts.text_codec, opts.binary_codec, store, opts.image_codec, opts.code_codec],
        lanes: Vec::new(),
        start: timing.then_some(start),
    };

    let mut objects: Vec<Object> = Vec::new();
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut by_hash: HashMap<[u8; 32], u32> = HashMap::new();
    let mut by_chunk: HashMap<[u8; 32], u32> = HashMap::new();
    let mut tstats: HashMap<&'static str, TransformStat> = HashMap::new();
    let debug = std::env::var_os("ARCA_DEBUG_ROUTING").is_some();

    // Streaming pipeline. Every file is its own task (read, hash, transform,
    // chunk) running up to `lookahead` bytes ahead of the packer, which consumes
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
    // Bytes that finished tasks hold beyond their input size (an expanded PNG can
    // be 15x its file), so the lookahead bounds memory, not just input bytes.
    let grown = AtomicU64::new(0);
    let (ready_tx, ready_rx) = crossbeam_channel::unbounded::<()>();
    // Sort groups feed lanes in order, so each lane's last block can be sealed,
    // and start compressing, as soon as its group has gone by.
    let text_end = jobs.iter().position(|j| j.group > 0).unwrap_or(n);
    let binary_end = jobs.iter().position(|j| j.group > 1).unwrap_or(n);

    let jobs = &jobs;
    pool.in_place_scope(|scope| -> Result<()> {
        let (mut spawned, mut inflight, mut done_bytes) = (0usize, 0u64, 0u64);
        let take = |k: usize| -> Option<Result<Hashed>> { slots[k].lock().unwrap().take() };
        let wait_for = |k: usize| -> Result<Hashed> {
            let _p = prof::span("packer.wait", 0);
            loop {
                if let Some(r) = take(k) {
                    return r;
                }
                ready_rx.recv().context("pipeline stalled")?;
            }
        };
        let result = (|| -> Result<()> {
            for k in 0..n {
                while spawned < n
                    && (spawned == k || inflight + grown.load(Ordering::Relaxed) + jobs[spawned].size <= lookahead)
                {
                    let i = spawned;
                    inflight += jobs[i].size;
                    spawned += 1;
                    let (slots, claims, cancel, grown, tx) = (&slots, &claims, &cancel, &grown, ready_tx.clone());
                    scope.spawn(move |_| {
                        let r = if cancel.load(Ordering::Relaxed) {
                            Err(anyhow::anyhow!("cancelled"))
                        } else {
                            process_file(&jobs[i], i, claims, opts)
                        };
                        if let Ok(h) = &r {
                            grown.fetch_add(h.extra, Ordering::Relaxed);
                        }
                        *slots[i].lock().unwrap() = Some(r);
                        let _ = tx.send(());
                    });
                }

                let job = &jobs[k];
                let mine = wait_for(k)?;
                inflight -= job.size;
                grown.fetch_sub(mine.extra, Ordering::Relaxed);
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
                            eprintln!("{:?}\t{:?}\t{}\t{}\t{}\t{}", lane, mine.class, transform.name(), data.len(), stored.len(), entries[job.entry].path);
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
                    sink.flush_lane(Lane::Image);
                    sink.flush_lane(Lane::Code);
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

    let mut lane_stats = [("text", 0u64, 0u64), ("binary", 0, 0), ("store", 0, 0), ("image", 0, 0), ("code", 0, 0)];
    for (b, lane) in blocks.iter().zip(&lanes) {
        lane_stats[*lane as usize].1 += b.raw_len;
        lane_stats[*lane as usize].2 += b.comp_len;
    }
    stats.lanes = lane_stats.to_vec();
    stats.blocks = blocks.len() as u64;

    let created = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let index = Index { created, entries, objects, chunks, blocks };
    let raw = index.encode();
    if let Some(p) = std::env::var_os("ARCA_DUMP_INDEX") {
        let _ = std::fs::write(p, &raw);
    }
    let comp = {
        let _p = prof::span("index.compress", raw.len() as u64);
        zstd::bulk::compress(&raw, opts.index_level)?
    };
    let index_offset = index.blocks.iter().map(|b| b.file_offset + b.comp_len).max().unwrap_or(format::HEADER_LEN as u64);
    out.write_all(&comp)?;
    let footer = Footer {
        index_offset,
        index_comp_len: comp.len() as u64,
        index_raw_len: raw.len() as u64,
        index_hash: format::hash16(&comp),
    };
    out.write_all(&footer.to_bytes())?;
    {
        let _p = prof::span("pack.sync", 0);
        out.sync_all()?;
    }
    drop(out);
    std::fs::rename(tmp_path, output)?;
    phase("index written, synced");

    let mut t: Vec<_> = tstats.into_iter().collect();
    t.sort_by_key(|(k, _)| *k);
    stats.transforms = t;
    stats.index_bytes = comp.len() as u64;
    stats.output_bytes = std::fs::metadata(output)?.len();
    stats.seconds = start.elapsed().as_secs_f64();
    prof::report("pack");
    Ok(stats)
}

type Cut = (usize, usize, [u8; 32], Lane);
/// (transform, transformed bytes if any, lane, chunks, original bytes)
type Prepared = (Transform, Option<Vec<u8>>, Lane, Vec<Cut>, FileData);

struct Hashed {
    hash: [u8; 32],
    class: Class,
    /// Transformed bytes held beyond the input (for the lookahead's accounting).
    extra: u64,
    /// Present when this task claimed the content (first to hash it).
    prepared: Option<Prepared>,
}

/// Reads, hashes and, unless another task already claimed the same content,
/// transforms and chunks one file.
fn process_file(job: &Job, index: usize, claims: &Mutex<HashMap<[u8; 32], usize>>, opts: &PackOptions) -> Result<Hashed> {
    let data = {
        let _p = prof::span("file.read", job.size);
        read_file(&job.source, job.size)?
    };
    if data.len() as u64 != job.size {
        bail!("{} changed size while archiving", job.source.display());
    }
    let class = if data.is_empty() { Class::Text } else { classify::classify(&data[..data.len().min(SNIFF_LEN)]) };
    // Content identity. Files that will be transformed hash their original bytes
    // (the index keeps that hash to check the rebuilt file). Every other file is
    // only hashed once, chunk by chunk, and its identity comes from those hashes.
    let transformable = transform::applies(class, &opts.transforms);
    let (hash, plain_cuts) = if transformable || data.is_empty() {
        let _h = prof::span("file.hash", job.size);
        (*blake3::hash(&data).as_bytes(), None)
    } else {
        let cuts = {
            let _p = prof::span("file.chunk", data.len() as u64);
            cut_chunks(&data, opts.dedup)
        };
        (content_key(&cuts), Some(cuts))
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
        return Ok(Hashed { hash, class, extra: 0, prepared: None });
    }
    let (transform, owned, lane, store_from, cuts) = match plain_cuts {
        Some(cuts) => (Transform::None, None, transform::plain_lane(class), None, cuts),
        None => {
            let p = transform::forward(&data, class, &opts.transforms);
            let owned = match p.stored {
                std::borrow::Cow::Owned(v) => Some(v),
                std::borrow::Cow::Borrowed(_) => None,
            };
            let stored: &[u8] = owned.as_deref().unwrap_or(&data);
            let cuts = {
                let _p = prof::span("file.chunk", stored.len() as u64);
                match p.store_from {
                    // Chunk the compressible head and the coded tail separately,
                    // so each chunk belongs to one lane.
                    Some(k) => {
                        let mut c = cut_chunks(&stored[..k], opts.dedup);
                        c.extend(cut_chunks(&stored[k..], opts.dedup).into_iter().map(|(o, l, h)| (o + k, l, h)));
                        c
                    }
                    None => cut_chunks(stored, opts.dedup),
                }
            };
            (p.transform, owned, p.lane, p.store_from, cuts)
        }
    };
    let lane = if lane == Lane::Code && !opts.code_lane { Lane::Binary } else { lane };
    let stored: &[u8] = owned.as_deref().unwrap_or(&data);
    let cuts = cuts
        .into_iter()
        .map(|(off, len, hash)| {
            if store_from.is_some_and(|k| off >= k) {
                return (off, len, hash, Lane::Store);
            }
            let incompressible = opts.probe && matches!(lane, Lane::Binary | Lane::Image) && {
                let _p = prof::span("file.probe", len as u64);
                classify::probably_incompressible(&stored[off..off + len])
            };
            (off, len, hash, if incompressible { Lane::Store } else { lane })
        })
        .collect::<Vec<_>>();
    let extra = owned.as_ref().map_or(0, |v| v.len() as u64);
    Ok(Hashed { hash, class, extra, prepared: Some((transform, owned, lane, cuts, data)) })
}

/// Identity of untransformed content from its chunk hashes. One chunk is the
/// content's own BLAKE3, the same as for a transformed file; several chunks are
/// hashed as a list in a separate key domain, so no file's bytes can pose as
/// another file's chunk list.
fn content_key(cuts: &[(usize, usize, [u8; 32])]) -> [u8; 32] {
    match cuts {
        [] => *blake3::hash(&[]).as_bytes(),
        [(_, _, h)] => *h,
        _ => {
            let mut k = blake3::Hasher::new_derive_key("Arca 2026-10 content key from chunk hashes");
            for (_, len, h) in cuts {
                k.update(&(*len as u64).to_le_bytes());
                k.update(h);
            }
            *k.finalize().as_bytes()
        }
    }
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
