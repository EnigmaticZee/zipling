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
    /// the dictionary is always the block.
    pub level: i32,
    pub extreme: bool,
}

pub fn compress(raw: &[u8], cfg: CodecConfig) -> Result<Vec<u8>> {
    match cfg.codec {
        Codec::Store => Ok(raw.to_vec()),
        Codec::Zstd => {
            let mut c = zstd::bulk::Compressor::new(cfg.level)?;
            // Window spans the whole block so long-range repeats inside it are found.
            let wlog = (usize::BITS - raw.len().max(1 << 20).leading_zeros()).clamp(20, 30);
            c.set_parameter(CParameter::WindowLog(wlog))?;
            c.set_parameter(CParameter::EnableLongDistanceMatching(true))?;
            c.set_parameter(CParameter::ChecksumFlag(true))?;
            Ok(c.compress(raw)?)
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
