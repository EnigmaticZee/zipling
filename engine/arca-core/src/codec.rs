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
