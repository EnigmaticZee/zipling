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
use crate::hash;
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
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
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
    /// Block size cap for inputs of `SPLIT_BELOW` or more; 0 for none.
    pub large_input_block: usize,
    pub transforms: TransformOpts,
    pub dedup: bool,
    /// Probe binary chunks, so a block of mostly incompressible-looking ones can
    /// try a quick level before its slow one. Levels already quick don't need it.
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
    /// Send disk images to the code lane without `code_lane`, when it runs zstd.
    /// Default's lane there is zstd -5: an image is hundreds of megabytes of mostly
    /// app bundles, and five DMGs took 5.4 s at level 12 against 2.7 s, for 1.8%.
    pub quick_images: bool,
    /// Send filtered binaries and ZIP, gzip or asset catalog files to the code lane
    /// without `code_lane`. Fast's lane there is zstd -1: they are most of a big app's
    /// bytes and hold the least for level 3 to find. Third-party apps 1.05 → 0.57 s,
    /// for 10% in size, which still leaves them under zstd -3's.
    pub quick_binaries: bool,
    /// Send photos no coder touched to the image lane instead of the store lane,
    /// when it runs zstd. Fast's lane there is zstd -1: a camera JPEG holds under
    /// 1% for it but a wallpaper or gradient 10-85%, and the 400-photo set goes
    /// 98.4 → 89 MB in the time copying takes, where re-coding the symbols took
    /// 4x as long for 78 MB (see RESEARCH.md, "Photos at Fast").
    pub quick_photos: bool,
    /// Block size for the image lane (PNG rows).
    pub image_block_size: usize,
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
        let zstd = |level| CodecConfig { codec: Codec::Zstd, level, extreme: false, long: true, workers: 1, job: 0 };
        let lzma2 = |level, extreme| CodecConfig { codec: Codec::Lzma2, level, extreme, long: false, workers: 1, job: 0 };
        // Per lane: text needs the expensive match finders (zstd's binary tree, xz's
        // extreme mode) to reach full ratio; binaries lose <0.4% with the cheaper
        // ones and gain it back from bigger blocks, at 3-4x the speed.
        let (text, binary, text_block, block, adaptive_below) = match level {
            // Long-distance matching stays on for text, where far repeats are 5% of
            // a source tree; on binaries bigger tables are smaller and faster.
            Level::Fast => (zstd(3), CodecConfig { long: false, ..zstd(3) }, 8 << 20, 8 << 20, u64::MAX),
            Level::Balanced => (zstd(15), zstd(12), 16 << 20, 64 << 20, 64 << 20),
            // Max is the fast high-ratio level: LZMA2 with 32-byte matches (half the
            // CPU of 273-byte ones for 1-6% more output) for binaries and pixels,
            // while text and code (the code lane below) get the longest matches in
            // 64 MiB blocks: what source trees and Silesia need to beat 7-Zip Ultra.
            // Research: ARCA_EXP_LZT puts the table coder on Max's lanes in
            // LZMA2's place, same parse.
            Level::Max if std::env::var_os("ARCA_EXP_LZT").is_some() => (
                CodecConfig { codec: Codec::Lzt, ..lzma2(9, true) },
                CodecConfig { codec: Codec::Lzt, ..lzma2(7, false) },
                64 << 20,
                64 << 20,
                0,
            ),
            Level::Max => (lzma2(9, true), lzma2(7, false), 64 << 20, 64 << 20, 0),
            // Ultra is the smallest: context modelling for text (it beats LZ on
            // prose and source code, at the cost of decode speed) and the longest
            // LZMA2 matches.
            Level::Ultra => (CodecConfig { codec: Codec::Ppmd, level: 16, extreme: false, long: false, workers: 1, job: 0 }, lzma2(9, true), 64 << 20, 64 << 20, 0),
        };
        let heavy = level != Level::Fast;
        PackOptions {
            level,
            threads,
            block_size: block,
            text_block_size: text_block,
            adaptive_below,
            // At Max, 32 MiB blocks: LZMA spends less on each byte and six encoders
            // get through more. Third-party apps 51 → 43 s, saved folder 31 → 25 s,
            // for 0.9-3% in size (see RESEARCH.md).
            large_input_block: if level == Level::Max { 32 << 20 } else { 0 },
            transforms: TransformOpts {
                jpeg: heavy,
                // Research: ARCA_EXP_JPEGLITE puts the light coder back on Fast's
                // photos: 12% smaller than storing them, at 4x the time (see
                // `quick_photos`).
                jpeg_lite: level == Level::Fast && std::env::var_os("ARCA_EXP_JPEGLITE").is_some(),
                // Our coefficient coder ahead of Lepton from Default up; ARCA_EXP_LEPTON=1
                // leaves photos to Lepton alone, for comparison.
                jpeg_rans: heavy && std::env::var_os("ARCA_EXP_LEPTON").is_none(),
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
                // Experiment for Default: keep the codec for the images where it
                // saves the most per pixel decoded (see RESEARCH.md).
                png_pixels_per_byte_saved: match level {
                    Level::Balanced => std::env::var("ARCA_EXP_PIXEL_GAIN").ok().and_then(|v| v.parse().ok()).unwrap_or(50),
                    _ => 0,
                },
                asset_catalogs: std::env::var_os("ARCA_EXP_CAR").is_some(),
                disk_images: level != Level::Fast,
                verify_filters: level != Level::Fast,
            },
            dedup: true,
            probe: level == Level::Balanced,
            text_codec: text,
            binary_codec: binary,
            // Fast's image lane holds stored photos (see `quick_photos`).
            image_codec: if level == Level::Fast { CodecConfig { long: false, ..zstd(1) } } else { binary },
            // At Max and Ultra code and text get long matches: what makes source
            // trees and apps small (7-Zip Ultra is the bar there).
            code_lane: matches!(level, Level::Max | Level::Ultra),
            code_codec: match level {
                Level::Max | Level::Ultra => lzma2(9, true),
                Level::Fast => CodecConfig { long: false, ..zstd(1) },
                // Disk images only: see `quick_images`.
                Level::Balanced => zstd(5),
            },
            quick_binaries: level == Level::Fast,
            quick_photos: level == Level::Fast,
            quick_images: level == Level::Balanced,
            code_block_size: block,
            image_block_size: block,
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
/// Bytes of a stream one task cuts into chunks. Nothing can be compressed until a
/// file is cut, so a 50 MB file on one thread held every core idle for 30% of
/// Silesia's time at Fast.
const CDC_SPAN: usize = 8 << 20;
/// A seam between spans lands where the content says, within this many bytes of
/// its place. Seams at fixed offsets split chunks differently in two copies of
/// the same data, which cost 0.25% at Default, where Firefox's expanded `omni.ja`
/// files dedupe against each other.
const SEAM_REACH: usize = 4 << 20;
/// How far (in input bytes) file tasks may run ahead of the packer.
const LOOKAHEAD_BYTES: u64 = 512 << 20;
const MMAP_THRESHOLD: u64 = 256 << 10;
const OPENS_AT_ONCE: usize = 3;

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

/// Opens a file with at most `OPENS_AT_ONCE` opens running. `open` takes a lock
/// in the kernel that doesn't scale: on an M1 Pro one open costs 18 us of CPU
/// alone, 25 us on three threads and 120-140 us on eight to ten, where threads
/// spin, and three or four threads also finish a folder soonest. The rest of a
/// file's read scales, so only this call waits.
fn open_file(path: &Path) -> std::io::Result<File> {
    static GATE: OnceLock<(Sender<()>, Receiver<()>)> = OnceLock::new();
    let (tx, rx) = GATE.get_or_init(|| {
        let (tx, rx) = bounded(OPENS_AT_ONCE);
        for _ in 0..OPENS_AT_ONCE {
            tx.send(()).unwrap();
        }
        (tx, rx)
    });
    rx.recv().expect("gate holds its own sender");
    let f = File::open(path);
    tx.send(()).expect("gate holds its own receiver");
    f
}

fn read_file(path: &Path, size: u64) -> Result<FileData> {
    let mut f = open_file(path).with_context(|| format!("opening {}", path.display()))?;
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
    let Ok(mut f) = open_file(path) else { return Class::Binary };
    let mut head = [0u8; SNIFF_LEN];
    let mut n = 0;
    while n < head.len() {
        match f.read(&mut head[n..]) {
            Ok(0) | Err(_) => break,
            Ok(k) => n += k,
        }
    }
    if n == 0 {
        return Class::Text;
    }
    let class = classify::classify(&head[..n]);
    if named_dmg(path) {
        let mut tail = [0u8; 512];
        let len = f.metadata().map_or(0, |m| m.len());
        if len >= 1024 && std::os::unix::fs::FileExt::read_exact_at(&f, &mut tail, len - 512).is_ok() && classify::is_udif_trailer(&tail) {
            return Class::DeflateContainer(classify::DeflateKind::Dmg);
        }
    }
    class
}

/// Disk images start with the image's own bytes; a `.dmg` name sends the
/// classifier to the trailer, which says what the file is.
fn named_dmg(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("dmg"))
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
    /// Bytes from chunks that look incompressible.
    hard: usize,
    /// Where each chunk starts, for splitting between chunks.
    starts: Vec<u32>,
}

/// Inputs from this size on have blocks enough for every core.
const SPLIT_BELOW: u64 = 512 << 20;
/// Inputs below this size can't fill enough full blocks to run short of memory.
const FEW_BLOCKS_BELOW: u64 = 256 << 20;
/// Blocks at least this long may split in two, and the halves likewise. 16 MiB
/// parts, where the quick pass allows them, let a block's decoding spread over
/// four cores: Silesia extracts in 0.22 s against 0.36 s with 32 MiB parts.
const SPLIT_MIN: usize = 16 << 20;
/// A split is made when the quick pass says it costs less than 1 part in this
/// many: Silesia's blocks measure 0.11-0.46%, Apple's apps 0.72-1.04%, where LZMA
/// loses more (1.7%), a source tree 1.1-8.7%, PDFs 2.5%.
const SPLIT_COST: usize = 200;

/// The chunk boundary nearest the middle of a block, if splitting there loses
/// little. A quick pass measures that directly, the whole block against its two
/// halves, because nothing cheaper tells: Silesia at Max loses 0.5% in two for
/// half the time, and a source tree 5%, its repeats spanning the lane.
fn split_point(buf: &[u8], starts: &[u32]) -> Option<usize> {
    let middle = (buf.len() / 2) as u32;
    let at = *starts.iter().min_by_key(|&&s| s.abs_diff(middle))? as usize;
    if at < buf.len() / 4 || at > buf.len() / 4 * 3 {
        return None;
    }
    let quick = CodecConfig { codec: Codec::Zstd, level: 1, extreme: false, long: true, workers: 1, job: 0 };
    let (a, (b, whole)) = rayon::join(
        || codec::compress(&buf[..at], quick),
        || rayon::join(|| codec::compress(&buf[at..], quick), || codec::compress(buf, quick)),
    );
    let (parts, whole) = (a.ok()?.len() + b.ok()?.len(), whole.ok()?.len());
    (parts < whole + whole / SPLIT_COST).then_some(at)
}

struct Done {
    id: u32,
    comp: Vec<u8>,
    raw_len: u64,
    raw_hash: [u8; 16],
    checksum: [u8; 16],
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
                "  block {:>3} {:<6} {:>5.1} MiB  queued {:>6.3}s  start {:>6.3}s  end {:>6.3}s",
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

/// A block, or part of one, waiting for an encoder. Ordered by `weight`, the time
/// it will take as far as can be told.
struct Part {
    weight: usize,
    id: u32,
    buf: Vec<u8>,
    hard: bool,
    cfg: CodecConfig,
}

impl PartialEq for Part {
    fn eq(&self, other: &Self) -> bool {
        (self.weight, self.id) == (other.weight, other.id)
    }
}

impl Eq for Part {}

impl PartialOrd for Part {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Part {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.weight, self.id).cmp(&(other.weight, other.id))
    }
}

/// Numbers a new block in `lane`.
fn new_block(lanes: &Mutex<Vec<Lane>>, lane: Lane) -> u32 {
    let mut l = lanes.lock().unwrap();
    l.push(lane);
    l.len() as u32 - 1
}

struct BlockSink {
    pool: Arc<rayon::ThreadPool>,
    slots_tx: Sender<()>,
    slots_rx: Receiver<()>,
    done_tx: Sender<Result<Done>>,
    open: [Option<OpenBlock>; 5],
    block_sizes: [usize; 5],
    codecs: [CodecConfig; 5],
    /// The lane of each block number.
    lanes: Arc<Mutex<Vec<Lane>>>,
    /// Blocks split as they closed: (block, the second half's number, where).
    splits: Arc<Mutex<Vec<(u32, u32, u64)>>>,
    /// Whether blocks may split, for levels whose decoders run one per block.
    split: bool,
    /// Parts waiting for an encoder, longest first.
    queue: Arc<Mutex<std::collections::BinaryHeap<Part>>>,
    /// Pack start, for the block timeline `ARCA_DEBUG_TIMING` prints.
    start: Option<Instant>,
    key: Arc<hash::Key>,
    /// File tasks and blocks under way or waiting, and the cores they share.
    busy: Arc<AtomicUsize>,
    cores: usize,
    /// A small input: the zstd job for its blocks, a core's share of it (0: a
    /// block is one job, two workers with cores to spare).
    small_job: usize,
}

impl BlockSink {
    fn append(&mut self, lane: Lane, bytes: &[u8], hard: bool) -> (u32, u64) {
        let limit = self.block_sizes[lane as usize];
        let slot = &mut self.open[lane as usize];
        if slot.is_none() {
            let id = new_block(&self.lanes, lane);
            *slot = Some(OpenBlock { id, lane, buf: Vec::with_capacity(limit + CDC_MAX as usize), hard: 0, starts: Vec::new() });
        }
        let b = slot.as_mut().unwrap();
        let at = (b.id, b.buf.len() as u64);
        b.starts.push(b.buf.len() as u32);
        b.buf.extend_from_slice(bytes);
        b.hard += if hard { bytes.len() } else { 0 };
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
        let (start, key, busy, cores) = (self.start, self.key.clone(), self.busy.clone(), self.cores);
        let small_job = self.small_job;
        let queued = start.map(|s| s.elapsed().as_secs_f64());
        let (lanes, splits, queue) = (self.lanes.clone(), self.splits.clone(), self.queue.clone());
        let may_split = self.split && matches!(cfg.codec, Codec::Lzma2 | Codec::Bsc | Codec::Lzt) && block.buf.len() >= SPLIT_MIN;
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
            // With cores to spare, a block takes two; a small input's block is cut
            // into a core's share of the input, on every core.
            let at_once = busy.fetch_add(1, Ordering::Relaxed) + 1;
            let (workers, job) = if small_job > 0 { (cores as u32, small_job) } else if 2 * at_once <= cores { (2, 0) } else { (1, 0) };
            let cfg = CodecConfig { workers, job, ..cfg };
            let finish = Arc::new(move |Part { id, buf, hard, cfg, .. }: Part| {
                let raw_len = buf.len() as u64;
                let raw_hash = key.hash(&buf);
                let res = if cfg.codec == Codec::Store {
                    Ok((Vec::new(), Codec::Store))
                } else if hard {
                    codec::compress_hard(&buf, cfg)
                } else {
                    codec::compress_auto(&buf, cfg)
                }
                .map(|(comp, codec)| {
                    // Never let a codec expand data: fall back to storing the block.
                    let stored = codec == Codec::Store || comp.len() >= buf.len();
                    let (comp, codec, checksum) = if stored {
                        (buf, Codec::Store, raw_hash)
                    } else {
                        let checksum = key.hash(&comp);
                        (comp, codec, checksum)
                    };
                    Done { id, comp, raw_len, raw_hash, checksum, codec }
                });
                let _ = done_tx.send(res);
            });
            let OpenBlock { id, lane, buf, hard, starts } = block;
            let hard = hard * 2 > buf.len();
            let mut parts = vec![(id, buf, starts)];
            let mut i = 0;
            while i < parts.len() {
                let (id, buf, starts) = &mut parts[i];
                match (may_split && buf.len() >= SPLIT_MIN).then(|| split_point(buf, starts)).flatten() {
                    Some(at) => {
                        let tail = buf.split_off(at);
                        let from = starts.partition_point(|&s| (s as usize) < at);
                        let tail_starts = starts.split_off(from).iter().map(|&s| s - at as u32).collect();
                        let second = new_block(&lanes, lane);
                        splits.lock().unwrap().push((*id, second, at as u64));
                        parts.push((second, tail, tail_starts));
                    }
                    None => i += 1,
                }
            }
            // A block that split has no far repeats to speak of, so long matches
            // buy little in it: Silesia 15% less CPU for 0.3%.
            let long = matches!(cfg.codec, Codec::Lzma2 | Codec::Lzt) && cfg.extreme;
            let cfg = if parts.len() > 1 && long { CodecConfig { extreme: false, level: 9, ..cfg } } else { cfg };
            // The longest first: with a fixed amount of work, what decides the time
            // is whether a long part starts last. Silesia at Max took 10.5 or 13.3 s
            // from one run to the next as the parts happened to fall.
            let mut more = parts.len();
            for (id, buf, _) in parts {
                let weight = buf.len() * if cfg.level >= 9 { 2 } else { 1 };
                queue.lock().unwrap().push(Part { weight, id, buf, hard, cfg });
            }
            while more > 1 {
                more -= 1;
                let (queue, finish) = (queue.clone(), finish.clone());
                rayon::spawn_fifo(move || {
                    let part = queue.lock().unwrap().pop();
                    if let Some(part) = part {
                        finish(part);
                    }
                });
            }
            let part = queue.lock().unwrap().pop();
            if let Some(part) = part {
                finish(part);
            }
            busy.fetch_sub(1, Ordering::Relaxed);
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
        Some(Class::DeflateContainer(DeflateKind::Zip | DeflateKind::Dmg)) => 3,
        Some(Class::DeflateContainer(DeflateKind::Pdf)) => 4,
        Some(Class::DeflateContainer(DeflateKind::Gzip)) => 5,
        Some(Class::Binary | Class::DeflateContainer(DeflateKind::AssetCatalog)) => 6,
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
    if timing {
        let _ = TIMING_START.set(start);
    }
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
    if opts.large_input_block > 0 && stats.input_bytes >= SPLIT_BELOW {
        block_size = block_size.min(opts.large_input_block);
    }
    // A bsc block is slow to decode, so more of them use more cores. Source trees
    // at Max: half the block, 33% less time to make and 15% less to extract, for
    // 2.2%. Silesia at Default: 12 MiB, 0.47 to 0.43 s, for 0.25%.
    if opts.text_codec.codec == Codec::Bsc {
        match opts.level {
            Level::Max => text_block_size = text_block_size.min(32 << 20),
            Level::Balanced => text_block_size = text_block_size.min(12 << 20),
            _ => {}
        }
    }
    // A small input keeps whole blocks; zstd cuts each into a core's share of
    // the input, jobs that overlap, instead: blocks cut to that share cost
    // 5-12% on a 7-41 MB input (far repeats across them lost) where the jobs
    // cost 0-4% at the same speed.
    let small_job =
        if stats.input_bytes < opts.adaptive_below { (stats.input_bytes / opts.threads.max(1) as u64).max(1 << 20) as usize } else { 0 };

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
    // first blocks: six on 10 cores made Max 4-6% faster than ten. A small input
    // has only a few full blocks, so its memory stays bounded, and it runs eight:
    // Silesia 11% and a source tree 11% faster than six, and ten is no better.
    if [opts.text_codec, opts.binary_codec, opts.image_codec, opts.code_codec].iter().any(|c| matches!(c.codec, Codec::Lzma2 | Codec::Lzt)) {
        block_threads = block_threads.min(match stats.input_bytes {
            n if n < FEW_BLOCKS_BELOW => (opts.threads * 4 / 5).max(1),
            _ => opts.threads.max(1) / 2 + 1,
        });
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
    // Running blocks plus two queued: enough to keep every encoder fed. (A queue
    // forty deep let Fast read all of Silesia by 0.05 s instead of 0.11 and
    // changed nothing: the encoders are the bound, not the feed.)
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
                    checksum: d.checksum,
                    raw_hash: d.raw_hash,
                },
            ));
            pos += d.comp.len() as u64;
        }
        Ok((out, infos))
    });

    let store = CodecConfig { codec: Codec::Store, level: 0, extreme: false, long: false, workers: 1, job: 0 };
    let busy = Arc::new(AtomicUsize::new(0));
    // Drawn after the files exist: see `hash` for why that is what matters.
    let key = Arc::new(hash::Key::random());
    let mut sink = BlockSink {
        // Its own pool: blocks must not queue behind file tasks, or compression
        // trails at the end instead of overlapping with the transforms.
        pool: Arc::new(rayon::ThreadPoolBuilder::new().num_threads(block_threads).build()?),
        slots_tx,
        slots_rx,
        done_tx,
        open: [None, None, None, None, None],
        block_sizes: [
            text_block_size,
            block_size,
            block_size,
            opts.image_block_size.clamp(1 << 20, block_size.max(1 << 20)),
            opts.code_block_size.min(block_size.max(1 << 20)),
        ],
        codecs: [opts.text_codec, opts.binary_codec, store, opts.image_codec, opts.code_codec],
        lanes: Arc::new(Mutex::new(Vec::new())),
        splits: Arc::new(Mutex::new(Vec::new())),
        split: opts.level == Level::Max && stats.input_bytes < SPLIT_BELOW,
        queue: Arc::new(Mutex::new(std::collections::BinaryHeap::new())),
        start: timing.then_some(start),
        key: key.clone(),
        busy: busy.clone(),
        cores: opts.threads.max(1),
        small_job,
    };

    let mut objects: Vec<Object> = Vec::new();
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut by_hash: HashMap<[u8; 16], u32> = HashMap::new();
    let mut by_chunk: HashMap<[u8; 16], u32> = HashMap::new();
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
    let claims: Mutex<HashMap<[u8; 16], usize>> = Mutex::new(HashMap::new());
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
                    let (slots, claims, cancel, grown, key, tx) = (&slots, &claims, &cancel, &grown, &key, ready_tx.clone());
                    busy.fetch_add(1, Ordering::Relaxed);
                    let busy = &busy;
                    scope.spawn(move |_| {
                        let r = if cancel.load(Ordering::Relaxed) {
                            Err(anyhow::anyhow!("cancelled"))
                        } else {
                            process_file(&jobs[i], i, claims, key, opts)
                        };
                        if let Ok(h) = &r {
                            grown.fetch_add(h.extra, Ordering::Relaxed);
                        }
                        *slots[i].lock().unwrap() = Some(r);
                        busy.fetch_sub(1, Ordering::Relaxed);
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
                        for (off, len, hash, lane, hard) in cuts {
                            if opts.dedup {
                                if let Some(&c) = by_chunk.get(&hash) {
                                    if chunks[c as usize].len == len as u64 {
                                        stats.dup_chunk_bytes += len as u64;
                                        ids.push(c);
                                        continue;
                                    }
                                }
                            }
                            let _a = prof::span("pack.append", len as u64);
                            let (block, offset) = sink.append(lane, &stored[off..off + len], hard);
                            drop(_a);
                            let c = chunks.len() as u32;
                            chunks.push(Chunk { block, offset, len: len as u64 });
                            by_chunk.insert(hash, c);
                            ids.push(c);
                        }
                        let hash = (transform != Transform::None).then_some(mine.hash);
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
    let (lanes, splits) = (sink.lanes.clone(), sink.splits.clone());
    drop(sink);

    let (mut out, infos) = writer.join().map_err(|_| anyhow::anyhow!("writer thread panicked"))??;
    phase("all blocks compressed and written");
    let lanes = std::mem::take(&mut *lanes.lock().unwrap());
    // In the order they were made: a block's first half can split again.
    for &(id, second, at) in splits.lock().unwrap().iter() {
        for c in chunks.iter_mut().filter(|c| c.block == id && c.offset >= at) {
            c.block = second;
            c.offset -= at;
        }
    }
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
    let index = Index { created, key: Some(key.bytes()), entries, objects, chunks, blocks };
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
        // The data before the rename, in order, which is what keeps a half archive
        // from appearing under its name. A full flush of the drive's cache
        // (`sync_all`, F_FULLFSYNC) adds no such guarantee and took 0.17 s of 0.31 s
        // on a 620 MB archive, so it is only the fallback for volumes without barriers.
        use std::os::fd::AsRawFd;
        // SAFETY: a valid descriptor and a command that takes no argument.
        if unsafe { libc::fcntl(out.as_raw_fd(), libc::F_BARRIERFSYNC) } == -1 {
            out.sync_all()?;
        }
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

/// (offset, length, hash, lane, looks incompressible)
type Cut = (usize, usize, [u8; 16], Lane, bool);
/// (transform, transformed bytes if any, lane, chunks, original bytes)
type Prepared = (Transform, Option<Vec<u8>>, Lane, Vec<Cut>, FileData);

struct Hashed {
    hash: [u8; 16],
    class: Class,
    /// Transformed bytes held beyond the input (for the lookahead's accounting).
    extra: u64,
    /// Present when this task claimed the content (first to hash it).
    prepared: Option<Prepared>,
}

/// The first pack's start, for the file timeline `ARCA_DEBUG_TIMING` prints.
static TIMING_START: OnceLock<Instant> = OnceLock::new();

/// Reads, hashes and, unless another task already claimed the same content,
/// transforms and chunks one file.
fn process_file(
    job: &Job,
    index: usize,
    claims: &Mutex<HashMap<[u8; 16], usize>>,
    key: &hash::Key,
    opts: &PackOptions,
) -> Result<Hashed> {
    let began = TIMING_START.get().map(|s| s.elapsed().as_secs_f64());
    let r = process_file_inner(job, index, claims, key, opts);
    if let Some(s) = TIMING_START.get() {
        eprintln!(
            "  file {:>5.1} MiB  start {:>6.3}s  end {:>6.3}s  {}",
            job.size as f64 / 1048576.0,
            began.unwrap_or(0.0),
            s.elapsed().as_secs_f64(),
            job.source.display()
        );
    }
    r
}

fn process_file_inner(
    job: &Job,
    index: usize,
    claims: &Mutex<HashMap<[u8; 16], usize>>,
    key: &hash::Key,
    opts: &PackOptions,
) -> Result<Hashed> {
    let data = {
        let _p = prof::span("file.read", job.size);
        read_file(&job.source, job.size)?
    };
    if data.len() as u64 != job.size {
        bail!("{} changed size while archiving", job.source.display());
    }
    let class = match data.len() {
        0 => Class::Text,
        n if n >= 1024 && named_dmg(&job.source) && classify::is_udif_trailer(&data[n - 512..]) => {
            Class::DeflateContainer(classify::DeflateKind::Dmg)
        }
        n => classify::classify(&data[..n.min(SNIFF_LEN)]),
    };
    // Content identity. Files that will be transformed hash their original bytes
    // (the index keeps that hash to check the rebuilt file). Every other file is
    // only hashed once, chunk by chunk, and its identity comes from those hashes.
    let transformable = transform::applies(class, &opts.transforms);
    let (hash, plain_cuts) = if transformable || data.is_empty() {
        let _h = prof::span("file.hash", job.size);
        (key.hash(&data), None)
    } else {
        let cuts = {
            let _p = prof::span("file.chunk", data.len() as u64);
            cut_chunks(&data, key, opts.dedup)
        };
        (content_key(&cuts, key), Some(cuts))
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
                        let mut c = cut_chunks(&stored[..k], key, opts.dedup);
                        // The coded tail of a photo shares nothing with another's
                        // short of being the same file, so fixed cuts will do.
                        c.extend(cut_chunks(&stored[k..], key, false).into_iter().map(|(o, l, h)| (o + k, l, h)));
                        c
                    }
                    None => cut_chunks(stored, key, opts.dedup),
                }
            };
            (p.transform, owned, p.lane, p.store_from, cuts)
        }
    };
    let lane = if lane == Lane::Code && !opts.code_lane { Lane::Binary } else { lane };
    use classify::DeflateKind as Kind;
    let packed = matches!(class, Class::DeflateContainer(Kind::Zip | Kind::Gzip | Kind::AssetCatalog));
    let lane = if opts.quick_binaries && (matches!(transform, Transform::Arm64 { .. }) || packed) { Lane::Code } else { lane };
    let image = matches!(class, Class::DeflateContainer(Kind::Dmg));
    let lane = if image && opts.quick_images && opts.code_codec.codec == Codec::Zstd { Lane::Code } else { lane };
    let photo = matches!(class, Class::Jpeg) && matches!(transform, Transform::None);
    let lane = if photo && opts.quick_photos && opts.image_codec.codec == Codec::Zstd { Lane::Image } else { lane };
    let stored: &[u8] = owned.as_deref().unwrap_or(&data);
    let route = |(off, len, hash): (usize, usize, [u8; 16])| {
        if store_from.is_some_and(|k| off >= k) {
            return (off, len, hash, Lane::Store, false);
        }
        let hard = opts.probe && matches!(lane, Lane::Binary | Lane::Image) && {
            let _p = prof::span("file.probe", len as u64);
            classify::probably_incompressible(&stored[off..off + len])
        };
        (off, len, hash, lane, hard)
    };
    let cuts: Vec<Cut> = {
        use rayon::prelude::*;
        if cuts.len() > 4 { cuts.into_par_iter().map(route).collect() } else { cuts.into_iter().map(route).collect() }
    };
    let extra = owned.as_ref().map_or(0, |v| v.len() as u64);
    Ok(Hashed { hash, class, extra, prepared: Some((transform, owned, lane, cuts, data)) })
}

/// Identity of untransformed content from its chunk hashes. One chunk is the
/// content's own hash, the same as for a transformed file; several chunks are
/// hashed as a list with BLAKE3, so no file's bytes can pose as another file's
/// chunk list.
fn content_key(cuts: &[(usize, usize, [u8; 16])], key: &hash::Key) -> [u8; 16] {
    match cuts {
        [] => key.hash(&[]),
        [(_, _, h)] => *h,
        _ => {
            let mut k = blake3::Hasher::new_derive_key("Arca 2026-10 content key from chunk hashes");
            for (_, len, h) in cuts {
                k.update(&(*len as u64).to_le_bytes());
                k.update(h);
            }
            k.finalize().as_bytes()[..16].try_into().unwrap()
        }
    }
}

/// The first position at or past `from` where FastCDC itself would cut, or
/// `from + SEAM_REACH` if there is none. A span's first chunk then starts where
/// the same data elsewhere gets a chunk of its own. FastCDC's two-byte loop is
/// the plain gear hash below, tested at every byte; its harder mask covers the
/// easier one's bits, so this holds on either side of its average chunk size.
fn seam_after(stored: &[u8], from: usize) -> usize {
    static GEAR: OnceLock<Box<[u64; 256]>> = OnceLock::new();
    let gear = GEAR.get_or_init(|| fastcdc::v2020::get_gear_with_seed(0).0);
    let mask = fastcdc::v2020::MASKS[CDC_AVG.trailing_zeros() as usize + 1];
    let end = (from + SEAM_REACH).min(stored.len());
    let mut h = 0u64;
    for (i, &b) in stored[from..end].iter().enumerate() {
        h = (h << 1).wrapping_add(gear[b as usize]);
        if i >= 64 && h & mask == 0 {
            return from + i;
        }
    }
    end
}

/// Chunk boundaries with the hashes that deduplicate them. A long stream is cut
/// span by span on every core.
fn cut_chunks(stored: &[u8], key: &hash::Key, dedup: bool) -> Vec<(usize, usize, [u8; 16])> {
    use rayon::prelude::*;
    if !dedup || stored.len() <= 2 * CDC_SPAN {
        return cut_span(stored, key, dedup);
    }
    let mut seams: Vec<usize> = (1..stored.len() / CDC_SPAN).into_par_iter().map(|k| seam_after(stored, k * CDC_SPAN)).collect();
    seams.insert(0, 0);
    seams.push(stored.len());
    let spans: Vec<(usize, usize)> = seams.windows(2).filter(|w| w[1] > w[0]).map(|w| (w[0], w[1])).collect();
    let parts: Vec<_> = spans
        .par_iter()
        .map(|&(a, b)| cut_span(&stored[a..b], key, dedup).into_iter().map(|(o, l, h)| (o + a, l, h)).collect::<Vec<_>>())
        .collect();
    parts.into_iter().flatten().collect()
}

fn cut_span(stored: &[u8], key: &hash::Key, dedup: bool) -> Vec<(usize, usize, [u8; 16])> {
    let hash = |s: &[u8]| key.hash(s);
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
