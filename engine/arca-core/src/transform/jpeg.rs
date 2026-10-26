//! Lossless JPEG recompression via Lepton (Microsoft's Rust port).
//!
//! Lepton decodes the Huffman layer and re-codes DCT coefficients with an
//! adaptive arithmetic coder, saving ~20% while reproducing the original file
//! bit for bit. The encoder verifies the round trip before we accept it.

use lepton_jpeg::{decode_lepton, encode_lepton_verify, EnabledFeatures, SingleThreadPool};
use std::io::Cursor;
use std::panic::{catch_unwind, AssertUnwindSafe};

/// Re-encodes `data`, or returns `None` when Lepton can't (or shouldn't) take it.
pub fn encode(data: &[u8]) -> Option<Vec<u8>> {
    let mut features = EnabledFeatures::compat_lepton_vector_write();
    features.max_processor_threads = 1;
    if data.len() as u64 >= features.max_jpeg_file_size as u64 {
        return None;
    }
    // Parallelism comes from compressing many files at once, so stay single-threaded here.
    let pool = SingleThreadPool::default();
    let res = catch_unwind(AssertUnwindSafe(|| encode_lepton_verify(data, &features, &pool)));
    match res {
        Ok(Ok((out, _))) if out.len() < data.len() => Some(out),
        _ => None,
    }
}

pub fn decode(data: &[u8], size_hint: u64) -> anyhow::Result<Vec<u8>> {
    let mut features = EnabledFeatures::compat_lepton_vector_read();
    features.max_processor_threads = 1;
    let pool = SingleThreadPool::default();
    let mut out = Vec::with_capacity(size_hint as usize);
    let res = catch_unwind(AssertUnwindSafe(|| {
        decode_lepton(&mut Cursor::new(data), &mut out, &features, &pool)
    }));
    match res {
        Ok(Ok(_)) => Ok(out),
        Ok(Err(e)) => anyhow::bail!("lepton decode failed: {e:?}"),
        Err(_) => anyhow::bail!("lepton decoder panicked"),
    }
}
