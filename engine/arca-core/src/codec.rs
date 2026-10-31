//! Block codecs. Every block is independent, so blocks compress and decompress
//! in parallel across all cores.

use crate::format::Codec;
use anyhow::{ensure, Context, Result};
use std::io::{Read, Write};
use zstd::zstd_safe::{CParameter, DParameter};

#[derive(Debug, Clone, Copy)]
pub struct CodecConfig {
    pub codec: Codec,
    /// zstd: 1..=22, xz: 0..=9 (with `extreme`), ppmd: model order 2..=64.
    /// lzma2: 7, 8 or 9 for match lengths (`fb`) 32, 48 or 64, `extreme` for 273;
    /// the dictionary is always the block. bsc: 1 for text (LZP 18/128), 2 for
    /// code (LZP 24/64), both with the adaptive QLFC coder; `extreme` falls back
    /// to LZMA2 fb 273 when bsc gets under 16:1 (see `compress_auto`).
    pub level: i32,
    pub extreme: bool,
    /// zstd: search the whole block for long repeats (long-distance matching).
    /// Off, the lane gets bigger tables instead (`FAST_TABLES`), which is smaller
    /// and faster on all but text and blocks full of far repeats, which try again
    /// with the search on (`LONG_RETRY_RATIO`).
    pub long: bool,
    /// zstd: threads for one block, when the block is at least two `ZSTD_JOB`s
    /// (or two `job`s).
    pub workers: u32,
    /// zstd, a block under two `ZSTD_JOB`s: cut it into jobs this big, each
    /// overlapping the one before by `SMALL_OVERLAP_LOG`. The packer sets a
    /// core's share of a small input; 0 keeps the block one job.
    pub job: usize,
}

/// How much of the block before it a small input's zstd job sees, as zstd
/// counts it (9: the whole window, 8: half, 7: a quarter, 6: an eighth; with
/// long-distance matching on, of a window the size of a job). Research knob.
fn small_overlap_log() -> u32 {
    std::env::var("ARCA_EXP_OVLOG").ok().and_then(|v| v.parse().ok()).unwrap_or(6)
}

thread_local! {
    /// One zstd context per encoder thread. A fresh context for every block
    /// allocates, page-faults and zeroes its tables again: with 2 MiB blocks on
    /// Silesia that was 85 contexts, a quarter of a gigabyte of fresh memory.
    static ZSTD_CCTX: std::cell::RefCell<Option<zstd::bulk::Compressor<'static>>> = const { std::cell::RefCell::new(None) };
}

/// Bytes one zstd worker takes at a time, each seeing the whole window before it.
/// With blocks this size a machine can hold only a few at once, so at Default one
/// 64 MiB block on one core was what the rest waited for: PDFs and screenshots
/// spent 1.2 of 1.8 s so. Two workers: 21% and 38% less time on PDFs and Apple's
/// apps, for 0.05% and 0.3% in size.
const ZSTD_JOB: usize = 32 << 20;

/// zstd's hash and chain table sizes (log2) at level 3 without long-distance
/// matching: 2x zstd's own. Against long-distance matching with zstd's tables,
/// 0.5-0.75% smaller and 9-23% less CPU on binaries, photos' leftovers and Silesia,
/// one thread at a time. On ten threads they cost 33% more CPU than zstd's own
/// (17, 16): the cores' shared cache. zstd's own would make Silesia in 0.133 s
/// instead of 0.146 at Fast, 0.9% bigger (apps +0.2%, mixed +0.4%, docs +0.5%),
/// and wins no row, so the size stays (see RESEARCH.md, "Fast on Silesia").
const FAST_TABLES: (u32, u32) = (18, 17);

/// Research knob: `ARCA_EXP_FAST_TABLES=h,c` in `FAST_TABLES`' place.
fn fast_tables() -> (u32, u32) {
    std::env::var("ARCA_EXP_FAST_TABLES")
        .ok()
        .and_then(|v| {
            let (h, c) = v.split_once(',')?;
            Some((h.parse().ok()?, c.parse().ok()?))
        })
        .unwrap_or(FAST_TABLES)
}

/// zstd's hash table (log2) at level 1 and below. Tables the size of level 3's
/// made level 1 no faster than level 3: most of its time went on cache misses.
const QUICK_HASH: u32 = 15;

/// A block that shrinks by more than this without long-distance matching is
/// compressed again with it. Static libraries are the case: at 22:1 the search
/// finds object files repeated across libraries, 10% of the lane's output.
const LONG_RETRY_RATIO: usize = 8;

/// The same after two workers, higher because their block took the slow route.
/// PDFs at 9:1 give nothing back for doing it over; static libraries at 22:1 do.
const SPLIT_RETRY_RATIO: usize = 20;

/// Below this ratio, data in a bsc lane with the LZMA2 fallback is mostly machine
/// code without long repeats, where LZMA2's literal and match model wins (an x86
/// tar: bsc +19%); above it, BWT wins (static libraries at 46:1: bsc -1%, 54x
/// faster).
const BSC_FALLBACK_RATIO: usize = 16;

/// As `compress`, also saying which codec the output is in: a bsc lane with the
/// fallback may hand a block to LZMA2.
pub fn compress_auto(raw: &[u8], cfg: CodecConfig) -> Result<(Vec<u8>, Codec)> {
    let lzma = CodecConfig { codec: Codec::Lzma2, level: 9, extreme: true, long: false, workers: 1, job: 0 };
    if cfg.codec == Codec::Bsc && cfg.extreme {
        // A sample decides up front for large blocks, so LZMA2's blocks don't pay
        // for a whole bsc pass first (the sample takes ~0.1 s).
        if raw.len() > 2 * BSC_SAMPLE {
            let sample = &raw[..BSC_SAMPLE];
            if compress(sample, cfg)?.len() * BSC_FALLBACK_RATIO > sample.len() {
                return Ok((compress(raw, lzma)?, Codec::Lzma2));
            }
        }
        let out = compress(raw, cfg)?;
        if out.len() * BSC_FALLBACK_RATIO > raw.len() {
            let lz = compress(raw, lzma)?;
            if lz.len() < out.len() {
                return Ok((lz, Codec::Lzma2));
            }
        }
        return Ok((out, cfg.codec));
    }
    Ok((compress(raw, cfg)?, cfg.codec))
}

/// A block that zstd -3 can't shrink by more than this stays as zstd -3 left it.
const HARD_RATIO: f64 = 0.95;

/// `compress_auto` for a block made mostly of chunks that look incompressible.
/// Looks can deceive: such a chunk can repeat earlier data, which a sample of it
/// alone can't show (Apple's apps come out 10% smaller at Default once compressed
/// with everything else). So a quick pass decides, and data that really is
/// incompressible doesn't sit through a slow level: DMGs at level 12 took 6x as long.
pub fn compress_hard(raw: &[u8], cfg: CodecConfig) -> Result<(Vec<u8>, Codec)> {
    if cfg.codec == Codec::Zstd && cfg.level > 3 {
        // Without long-distance matching: with it the pass found 0.3% more in
        // Apple's apps for 38% more time at Default (see RESEARCH.md, DMGs).
        let quick = compress(raw, CodecConfig { codec: Codec::Zstd, level: 3, extreme: false, long: false, workers: 1, job: 0 })?;
        if quick.len() as f64 > raw.len() as f64 * HARD_RATIO {
            return Ok((quick, Codec::Zstd));
        }
    }
    compress_auto(raw, cfg)
}

/// Bytes of a block bsc tries first when the block may go to LZMA2 instead.
const BSC_SAMPLE: usize = 4 << 20;

pub fn compress(raw: &[u8], cfg: CodecConfig) -> Result<Vec<u8>> {
    match cfg.codec {
        Codec::Bsc => {
            let s = libbsc::Settings {
                lzp_hash: if cfg.level >= 2 { 24 } else { 18 },
                lzp_min: if cfg.level >= 2 { 64 } else { 128 },
                sorter: libbsc::SORTER_BWT,
                coder: libbsc::CODER_QLFC_ADAPTIVE,
            };
            match libbsc::compress(raw, s) {
                Ok(c) => Ok(c),
                // Incompressible: the caller stores the block.
                Err(libbsc::Error(-3)) => Ok(raw.to_vec()),
                Err(e) => Err(e.into()),
            }
        }
        Codec::Store => Ok(raw.to_vec()),
        Codec::Zstd => {
            // Big blocks: 32 MiB jobs, each seeing the whole window before it.
            // A small input's blocks (`cfg.job`, a core's share of the input):
            // jobs that size, each seeing part of the one before, where blocks
            // cut to that share lost 5-12% (see RESEARCH.md).
            let (job, overlap_log) = if raw.len() >= 2 * ZSTD_JOB { (ZSTD_JOB, 9) } else { (cfg.job, small_overlap_log()) };
            let split = cfg.workers > 1 && job > 0 && raw.len() / 2 >= job;
            let zstd = |long: bool, split: bool| -> Result<Vec<u8>> {
                ZSTD_CCTX.with(|cell| {
                    let mut slot = cell.borrow_mut();
                    let c = match slot.as_mut() {
                        Some(c) => c,
                        None => slot.insert(zstd::bulk::Compressor::new(0)?),
                    };
                    // Every parameter is set each time: the context carries the
                    // previous block's over.
                    c.set_parameter(CParameter::CompressionLevel(cfg.level))?;
                    // Window spans the whole block so long-range repeats inside it are found.
                    let wlog = (usize::BITS - raw.len().max(1 << 20).leading_zeros()).clamp(20, 30);
                    c.set_parameter(CParameter::WindowLog(wlog))?;
                    c.set_parameter(CParameter::EnableLongDistanceMatching(long))?;
                    c.set_parameter(CParameter::NbWorkers(if split { cfg.workers } else { 0 }))?;
                    c.set_parameter(CParameter::JobSize(if split { job as u32 } else { 0 }))?;
                    c.set_parameter(CParameter::OverlapSizeLog(if split { overlap_log } else { 0 }))?;
                    // 0 is zstd's own size for the level.
                    let (hash, chain) = if !cfg.long && cfg.level == 3 {
                        let tables = fast_tables();
                        (tables.0.min(wlog), tables.1.min(wlog))
                    } else if !cfg.long && cfg.level < 3 {
                        (QUICK_HASH, 0)
                    } else {
                        (0, 0)
                    };
                    c.set_parameter(CParameter::HashLog(hash))?;
                    c.set_parameter(CParameter::ChainLog(chain))?;
                    Ok(c.compress(raw)?)
                })
            };
            let first = zstd(cfg.long, split)?;
            // Far repeats are what make such a block: worth a second look, and quick
            // to compress. Without long-distance matching some went unseen, and
            // workers can't see each other's.
            let retry = if split { SPLIT_RETRY_RATIO } else if !cfg.long { LONG_RETRY_RATIO } else { usize::MAX };
            if first.len().saturating_mul(retry) < raw.len() {
                let second = zstd(true, false)?;
                if second.len() < first.len() {
                    return Ok(second);
                }
            }
            Ok(first)
        }
        Codec::Xz => {
            let preset = cfg.level.clamp(0, 9) as u32 | if cfg.extreme { liblzma::stream::PRESET_EXTREME } else { 0 };
            let stream = liblzma::stream::Stream::new_easy_encoder(preset, liblzma::stream::Check::Crc64)?;
            let mut enc = liblzma::write::XzEncoder::new_stream(Vec::with_capacity(raw.len() / 3), stream);
            enc.write_all(raw)?;
            Ok(enc.finish()?)
        }
        Codec::Lzma2 => {
            // 7-Zip's binary-tree encoder with the block as dictionary and the
            // match finder in its own thread: 1.7x faster per block and 17% less
            // CPU than one thread, with deterministic output.
            // Longer matches (fb) cost CPU: 273 against 32 is 1.5-2.3x the time
            // for 1-6% less output.
            let dict = (raw.len() as u32).clamp(1 << 16, 1 << 26);
            let fb = match (cfg.extreme, cfg.level) {
                (true, _) => 273,
                (false, l) if l >= 9 => 64,
                (false, 8) => 48,
                _ => 32,
            };
            let s = lzma_sdk::Settings {
                level: 9,
                dict_size: dict,
                fb: std::env::var("ARCA_EXP_FB").ok().and_then(|v| v.parse().ok()).unwrap_or(fb),
                mc: 0,
                threads: 2,
            };
            // Anything that doesn't shrink is stored by the caller anyway.
            match lzma_sdk::encode(raw, s, raw.len()) {
                Ok((c, prop)) => {
                    let mut out = Vec::with_capacity(c.len() + 1);
                    out.push(prop);
                    out.extend_from_slice(&c);
                    Ok(out)
                }
                Err(e) if e.0 == lzma_sdk::ERROR_OUTPUT_EOF => Ok(raw.to_vec()),
                Err(e) => Err(e.into()),
            }
        }
        Codec::Lzt => lzt_compress(raw, cfg),
        Codec::Ppmd => {
            let order = cfg.level.clamp(2, 64) as u32;
            // Model memory: enough for the block, capped so parallel blocks stay affordable.
            let mem = (raw.len() as u64 * 3).clamp(16 << 20, PPMD_MAX_MEM as u64) as u32;
            let mut out = Vec::with_capacity(raw.len() / 4 + 8);
            out.push(order as u8);
            out.extend_from_slice(&mem.to_le_bytes());
            let mut enc = ppmd_rust::Ppmd7Encoder::new(out, order, mem).map_err(|e| anyhow::anyhow!("ppmd: {e:?}"))?;
            enc.write_all(raw)?;
            Ok(enc.finish(false)?)
        }
    }
}

thread_local! {
    /// The tokens the LZMA encoder reports while `LZT_CAPTURE` is set on this thread.
    static LZT_TOKENS: std::cell::RefCell<Vec<crate::lzs::Token>> = const { std::cell::RefCell::new(Vec::new()) };
    static LZT_CAPTURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The LZMA SDK's token tap: one call per decision, on the encoding thread.
unsafe extern "C" fn lzt_hook(pos: u32, len: u32, dist: u32, reps: *const u32, _state: u32) {
    if dist == u32::MAX || !LZT_CAPTURE.with(|c| c.get()) {
        return;
    }
    let (kind, d) = if dist < 4 {
        (if dist == 0 && len == 1 { 5u8 } else { dist as u8 + 1 }, unsafe { *reps.add(dist as usize) })
    } else {
        (0, dist - 4 + 1)
    };
    LZT_TOKENS.with(|t| {
        let mut t = t.borrow_mut();
        // The LZMA2 encoder re-runs a chunk it decides to store raw: drop what
        // the first run reported from that position on.
        while t.last().is_some_and(|l| l.pos >= pos) {
            t.pop();
        }
        t.push(crate::lzs::Token { pos, len, kind, dist: d });
    });
}

/// `lzt`: LZMA's parse through the token tap, then the table coder. The LZMA
/// output itself is dropped; the parse is the cost either way.
fn lzt_compress(raw: &[u8], cfg: CodecConfig) -> Result<Vec<u8>> {
    static HOOK: std::sync::Once = std::sync::Once::new();
    HOOK.call_once(|| lzma_sdk::set_token_hook(Some(lzt_hook)));
    let dict = (raw.len() as u32).clamp(1 << 16, 1 << 26);
    let fb = match (cfg.extreme, cfg.level) {
        (true, _) => 273,
        (false, l) if l >= 9 => 64,
        (false, 8) => 48,
        _ => 32,
    };
    let s = lzma_sdk::Settings { level: 9, dict_size: dict, fb, mc: 0, threads: 2 };
    LZT_TOKENS.with(|t| t.borrow_mut().clear());
    LZT_CAPTURE.with(|c| c.set(true));
    // room enough that the parse always completes; what doesn't shrink is
    // stored by the caller
    let res = lzma_sdk::encode(raw, s, raw.len() + raw.len() / 4 + 4096);
    LZT_CAPTURE.with(|c| c.set(false));
    let tokens = LZT_TOKENS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    match res {
        Ok(_) => Ok(crate::lzt::encode(raw, &tokens)),
        Err(e) if e.0 == lzma_sdk::ERROR_OUTPUT_EOF => Ok(raw.to_vec()),
        Err(e) => Err(e.into()),
    }
}

/// Memory one encoder of this codec needs for a block of `block` bytes,
/// excluding the block itself.
pub fn encoder_memory(cfg: CodecConfig, block: usize) -> u64 {
    match cfg.codec {
        Codec::Store => 0,
        // Match-finder tables plus long-distance-matching state; the window is
        // the block, which is already in memory.
        Codec::Zstd => {
            if cfg.level >= 12 {
                160 << 20
            } else {
                48 << 20
            }
        }
        Codec::Xz => {
            let preset = cfg.level.clamp(0, 9) as u32 | if cfg.extreme { liblzma::stream::PRESET_EXTREME } else { 0 };
            // SAFETY: a pure function of the preset.
            let m = unsafe { liblzma_sys::lzma_easy_encoder_memusage(preset) };
            if m == u64::MAX { 1 << 30 } else { m }
        }
        Codec::Ppmd => (block as u64 * 3).clamp(16 << 20, PPMD_MAX_MEM as u64) + (block as u64 / 2),
        // Binary tree of the dictionary (~10x it) plus the threaded match
        // finder's buffers, as 7-Zip estimates for -mx9.
        Codec::Lzma2 => 11 * (block as u64).clamp(1 << 16, 1 << 26) + (64 << 20),
        // LZMA's encoder, plus the tokens and the coded symbols kept for the
        // backward pass (about six bytes a byte at worst).
        Codec::Lzt => 11 * (block as u64).clamp(1 << 16, 1 << 26) + (64 << 20) + 6 * block as u64,
        // Suffix array (4 bytes a byte), the BWT output and LZP's buffer; with the
        // fallback, LZMA2's tables once bsc is done.
        Codec::Bsc => {
            let bsc = 6 * block as u64;
            if cfg.extreme { bsc.max(11 * (block as u64).clamp(1 << 16, 1 << 26) + (64 << 20)) } else { bsc }
        }
    }
}

/// Physical memory of this Mac (`ARCA_MEM_BYTES` overrides it, for testing how
/// the engine behaves on smaller machines).
pub fn physical_memory() -> u64 {
    if let Some(v) = std::env::var("ARCA_MEM_BYTES").ok().and_then(|v| v.parse().ok()) {
        return v;
    }
    let mut mem: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    // SAFETY: hw.memsize is a u64; `len` holds the buffer size.
    let rc = unsafe {
        libc::sysctlbyname(c"hw.memsize".as_ptr(), (&mut mem as *mut u64).cast(), &mut len, std::ptr::null_mut(), 0)
    };
    if rc == 0 && mem > 0 { mem } else { 8 << 30 }
}

/// Largest PPMd model a block may request (the decoder allocates this much).
pub const PPMD_MAX_MEM: u32 = 256 << 20;

pub fn decompress(comp: &[u8], codec: Codec, raw_len: u64) -> Result<Vec<u8>> {
    let raw_len = usize::try_from(raw_len)?;
    let out = match codec {
        Codec::Store => comp.to_vec(),
        Codec::Zstd => {
            let mut d = zstd::bulk::Decompressor::new()?;
            d.set_parameter(DParameter::WindowLogMax(31))?;
            d.decompress(comp, raw_len).context("zstd block")?
        }
        Codec::Xz => {
            let mut out = Vec::with_capacity(raw_len);
            liblzma::read::XzDecoder::new(comp).take(raw_len as u64 + 1).read_to_end(&mut out).context("xz block")?;
            out
        }
        Codec::Lzma2 => {
            ensure!(!comp.is_empty(), "lzma2 block header missing");
            lzma_sdk::decode(&comp[1..], comp[0], raw_len).context("lzma2 block")?
        }
        Codec::Lzt => crate::lzt::decode(comp, raw_len).context("lzt block")?,
        // One thread per 8 MiB: small blocks come in numbers, and five of 16 MiB on
        // 8 threads each fought over ten cores, taking 0.60 s to their 0.47 s on 3.
        Codec::Bsc => libbsc::decompress_threads(comp, raw_len, (raw_len >> 23).clamp(2, 8) as i32).context("bsc block")?,
        Codec::Ppmd => {
            ensure!(comp.len() >= 5, "ppmd block header truncated");
            let order = comp[0] as u32;
            let mem = u32::from_le_bytes(comp[1..5].try_into().unwrap());
            ensure!(mem <= PPMD_MAX_MEM, "ppmd block asks for {mem} bytes of model memory");
            let mut dec =
                ppmd_rust::Ppmd7Decoder::new(&comp[5..], order, mem).map_err(|e| anyhow::anyhow!("ppmd: {e:?}"))?;
            let mut out = vec![0u8; raw_len];
            dec.read_exact(&mut out).context("ppmd block")?;
            out
        }
    };
    ensure!(out.len() == raw_len, "block decompressed to {} bytes, expected {raw_len}", out.len());
    Ok(out)
}
