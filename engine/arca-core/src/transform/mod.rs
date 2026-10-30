//! Reversible, format-aware transforms applied to whole objects before chunking.
//!
//! Every forward transform is checked by running its inverse before the result
//! is accepted (when `verify` is on), so a transform bug can cost ratio but
//! never data.

pub mod arm64;
pub mod deflate;
pub mod jpeg;
pub mod jpeglite;
pub mod pixels;

use crate::classify::{Class, Lane};
use crate::format::Transform;
use crate::prof;
use std::borrow::Cow;

#[derive(Debug, Clone, Copy)]
pub struct TransformOpts {
    pub jpeg: bool,
    /// JPEG's own symbols re-coded with per-image context tables (Fast): ~16%
    /// smaller at Huffman speed; `jpeg` (Lepton) wins when both are on.
    pub jpeg_lite: bool,
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
    /// With `png_pixels`, also require the codec to save at least one byte per
    /// this many pixel bytes (0: any saving). Decoding pixels costs extraction
    /// time, so Default keeps the codec for the images where it pays most.
    pub png_pixels_per_byte_saved: u32,
    /// Expand the LZFSE streams of asset catalogs (`Assets.car`). Their decoded
    /// images are ~17x the stored bytes: smaller with LZMA or bsc, but LZMA
    /// spends ~9x the CPU on them and zstd does worse than LZFSE (see RESEARCH.md).
    pub asset_catalogs: bool,
    /// Expand the zlib and LZFSE chunks of disk images (`.dmg`). 10-26% smaller
    /// DMGs, but the expanded data takes 4-10x the time to compress; on at Max
    /// and Ultra (see RESEARCH.md), and a switch for any level.
    pub disk_images: bool,
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
        Class::Jpeg => opts.jpeg || opts.jpeg_lite,
        Class::DeflateContainer(crate::classify::DeflateKind::AssetCatalog) => opts.deflate && opts.asset_catalogs,
        Class::DeflateContainer(crate::classify::DeflateKind::Dmg) => opts.disk_images,
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
        // Mac app code is arm64-filtered and gains little from long matches on
        // real apps (212.61 → 212.55 MiB on the saved folder for +15 s), so it
        // stays in the binary lane; libraries, ELF/PE executables and tar
        // bundles gain a lot (source trees: 9.3 → 8.5 MB).
        Class::Code => Lane::Code,
        _ => Lane::Binary,
    }
}

pub fn forward<'a>(data: &'a [u8], class: Class, opts: &TransformOpts) -> Prepared<'a> {
    let fwd = prof::span(
        match class {
            Class::Jpeg if opts.jpeg => "fwd.lepton(+verify)",
            Class::Jpeg if opts.jpeg_lite => "fwd.jpeglite",
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
        // Headers and tables compress with other files' in the binary lane; the
        // coded scans are stored.
        Class::Jpeg if opts.jpeg_lite => jpeglite::encode(data).map(|(out, from)| {
            store_from = Some(from);
            (Transform::JpegLite, out, Lane::Binary)
        }),
        Class::DeflateContainer(kind) if applies(class, opts) => {
            let png = kind == crate::classify::DeflateKind::Png;
            deflate::encode(data, kind, opts.deflate_chain, opts.deflate_steps_per_byte, !opts.verify, opts.png_pixels.then_some(opts.png_pixels_per_byte_saved)).map(
                |e| {
                    store_from = e.store_from;
                    // Coded pixels leave only layout and corrections to compress.
                    let lane = if png && e.store_from.is_none() { Lane::Image } else { Lane::Binary };
                    (Transform::Deflate, e.stored, lane)
                },
            )
        }
        Class::MachO if opts.arm64 => {
            let x86 = if std::env::var_os("ARCA_EXP_NO_X86").is_some() { Vec::new() } else { arm64::x86_code_ranges(data) };
            let ranges = arm64::code_ranges(data);
            if ranges.is_empty() && x86.is_empty() {
                None
            } else {
                let mut out = data.to_vec();
                (arm64::filter(&mut out, &ranges, true).is_ok() && arm64::filter_x86(&mut out, &x86, true).is_ok())
                    .then(|| (Transform::Arm64 { ranges, x86 }, out, Lane::Binary))
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
        // Lepton verifies its own round trip internally; the light JPEG coder
        // accounts for every bit of every scan while encoding (`jpeglite::encode`).
        let ok = !opts.verify
            || transform == Transform::Lepton
            || transform == Transform::JpegLite
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
        Transform::Arm64 { ranges, x86 } => {
            let mut v = stored;
            arm64::filter(&mut v, ranges, false)?;
            arm64::filter_x86(&mut v, x86, false)?;
            v
        }
        Transform::Lepton => jpeg::decode(&stored, size)?,
        Transform::JpegLite => jpeglite::decode(&stored, size)?,
        Transform::Deflate => deflate::decode(&stored, size)?,
    };
    anyhow::ensure!(out.len() as u64 == size, "transform produced {} bytes, expected {size}", out.len());
    Ok(out)
}
