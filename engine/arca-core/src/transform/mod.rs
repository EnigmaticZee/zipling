//! Reversible, format-aware transforms applied to whole objects before chunking.
//!
//! Every forward transform is checked by running its inverse before the result
//! is accepted (when `verify` is on), so a transform bug can cost ratio but
//! never data.

pub mod arm64;
pub mod deflate;
pub mod jpeg;
pub mod pixels;

use crate::classify::{Class, Lane};
use crate::format::Transform;
use crate::prof;
use std::borrow::Cow;

#[derive(Debug, Clone, Copy)]
pub struct TransformOpts {
    pub jpeg: bool,
    pub deflate: bool,
    pub arm64: bool,
    pub verify: bool,
    /// Preflate's match-search depth: lower is faster, but streams from encoders
    /// that searched deeper need more corrections.
    pub deflate_chain: u32,
    /// Most hash-chain steps per compressed byte a Deflate stream may need to be
    /// re-created (0: no limit). Re-creating costs ~3.1 ns a step on an M1, so 128
    /// keeps every expanded stream above ~2.5 MB/s per core on extraction.
    pub deflate_steps_per_byte: u32,
    /// Code PNG pixels with the pixel codec where it beats compressing the rows.
    pub png_pixels: bool,
}

pub struct Prepared<'a> {
    pub transform: Transform,
    pub stored: Cow<'a, [u8]>,
    pub lane: Lane,
    /// Bytes from here on are already entropy coded and go to the store lane.
    pub store_from: Option<usize>,
}

/// Whether `forward` will attempt a transform for this class.
pub fn applies(class: Class, opts: &TransformOpts) -> bool {
    match class {
        Class::Jpeg => opts.jpeg,
        Class::DeflateContainer(_) => opts.deflate,
        Class::MachO => opts.arm64,
        _ => false,
    }
}

/// Picks the lane for bytes that are stored without a transform. Binary data is
/// refined per chunk later (see `pack`), so it is not probed here.
pub fn plain_lane(class: Class) -> Lane {
    match class {
        Class::Compressed | Class::Jpeg => Lane::Store,
        Class::Text => Lane::Text,
        _ => Lane::Binary,
    }
}

pub fn forward<'a>(data: &'a [u8], class: Class, opts: &TransformOpts) -> Prepared<'a> {
    let fwd = prof::span(
        match class {
            Class::Jpeg if opts.jpeg => "fwd.lepton(+verify)",
            Class::DeflateContainer(crate::classify::DeflateKind::Png) if opts.deflate => "fwd.deflate.png",
            Class::DeflateContainer(crate::classify::DeflateKind::Pdf) if opts.deflate => "fwd.deflate.pdf",
            Class::DeflateContainer(_) if opts.deflate => "fwd.deflate.other",
            Class::MachO if opts.arm64 => "fwd.arm64",
            _ => "fwd.none",
        },
        data.len() as u64,
    );
    let mut store_from = None;
    let attempt: Option<(Transform, Vec<u8>, Lane)> = match class {
        Class::Jpeg if opts.jpeg => jpeg::encode(data).map(|out| (Transform::Lepton, out, Lane::Store)),
        Class::DeflateContainer(kind) if opts.deflate => {
            let png = kind == crate::classify::DeflateKind::Png;
            deflate::encode(data, kind, opts.deflate_chain, opts.deflate_steps_per_byte, !opts.verify, opts.png_pixels).map(
                |e| {
                    store_from = e.store_from;
                    // Coded pixels leave only layout and corrections to compress.
                    let lane = if png && e.store_from.is_none() { Lane::Image } else { Lane::Binary };
                    (Transform::Deflate, e.stored, lane)
                },
            )
        }
        Class::MachO if opts.arm64 => {
            let ranges = arm64::code_ranges(data);
            if ranges.is_empty() {
                None
            } else {
                let mut out = data.to_vec();
                arm64::filter(&mut out, &ranges, true).ok().map(|_| (Transform::Arm64 { ranges }, out, Lane::Binary))
            }
        }
        _ => None,
    };

    drop(fwd);
    if let Some((transform, out, lane)) = attempt {
        let _v = prof::span(
            match (transform == Transform::Deflate, class) {
                (true, Class::DeflateContainer(crate::classify::DeflateKind::Png)) => "verify.deflate.png",
                (true, _) => "verify.deflate.other",
                _ => "verify.other",
            },
            data.len() as u64,
        );
        // Lepton verifies its own round trip internally.
        let ok = !opts.verify
            || transform == Transform::Lepton
            || verify(&out, &transform, data);
        if ok {
            return Prepared { transform, stored: Cow::Owned(out), lane, store_from };
        }
    }
    Prepared { transform: Transform::None, stored: Cow::Borrowed(data), lane: plain_lane(class), store_from: None }
}

/// Checks that `out` inverts back to exactly `original`, copying only where the
/// inverse must work in place.
fn verify(out: &[u8], transform: &Transform, original: &[u8]) -> bool {
    match transform {
        Transform::Deflate => deflate::decode(out, original.len() as u64).is_ok_and(|back| back == original),
        _ => inverse(out.to_vec(), transform, original.len() as u64).is_ok_and(|back| back == original),
    }
}

pub fn inverse(stored: Vec<u8>, transform: &Transform, size: u64) -> anyhow::Result<Vec<u8>> {
    let out = match transform {
        Transform::None => stored,
        Transform::Arm64 { ranges } => {
            let mut v = stored;
            arm64::filter(&mut v, ranges, false)?;
            v
        }
        Transform::Lepton => jpeg::decode(&stored, size)?,
        Transform::Deflate => deflate::decode(&stored, size)?,
    };
    anyhow::ensure!(out.len() as u64 == size, "transform produced {} bytes, expected {size}", out.len());
    Ok(out)
}
