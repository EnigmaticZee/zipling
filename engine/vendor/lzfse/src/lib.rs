//! Apple's lzfse 1.0 (the open-source reference), pinned so its output never
//! changes, with the one setting today's macOS encoder differs in.

use std::os::raw::c_int;

extern "C" {
    fn arca_lzfse_encode(dst: *mut u8, dst_size: usize, src: *const u8, src_size: usize, literal_runs: u32) -> usize;
    fn arca_lzfse_decode_exact(dst: *mut u8, dst_size: usize, src: *const u8, src_size: usize) -> c_int;
}

/// What macOS's libcompression uses (lzfse 1.0 used 8): how many 315-byte
/// literal runs may wait before a pending match is emitted.
pub const MACOS_LITERAL_RUNS: u8 = 3;

/// Decodes a whole LZFSE stream that must expand to exactly `raw_len` bytes.
pub fn decode(stream: &[u8], raw_len: usize) -> Option<Vec<u8>> {
    let mut out = vec![0u8; raw_len];
    (unsafe { arca_lzfse_decode_exact(out.as_mut_ptr(), raw_len, stream.as_ptr(), stream.len()) } == 1).then_some(out)
}

/// Encodes `raw`; `None` unless the stream is exactly `len` bytes.
pub fn encode(raw: &[u8], literal_runs: u8, len: usize) -> Option<Vec<u8>> {
    if literal_runs == 0 {
        return None;
    }
    // The encoder wants room past what it writes (it reserves a block's worst
    // case before coding it), and a longer stream must not fit either.
    let mut out = vec![0u8; raw.len().max(len) + (64 << 10)];
    let n = unsafe { arca_lzfse_encode(out.as_mut_ptr(), out.len(), raw.as_ptr(), raw.len(), literal_runs.into()) };
    (n == len).then(|| {
        out.truncate(len);
        out
    })
}

/// Encodes `raw` (output of any length; for making test data).
pub fn compress(raw: &[u8], literal_runs: u8) -> Vec<u8> {
    let mut out = vec![0u8; raw.len() + (64 << 10)];
    let n = unsafe { arca_lzfse_encode(out.as_mut_ptr(), out.len(), raw.as_ptr(), raw.len(), literal_runs.into()) };
    out.truncate(n);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_exact_lengths() {
        let raw: Vec<u8> = (0..500_000u32).map(|i| ((i * 7) % 251) as u8 ^ (i / 3000) as u8).collect();
        for runs in [3, 8] {
            let z = compress(&raw, runs);
            assert!(!z.is_empty() && z.len() < raw.len());
            assert_eq!(&z[..4], b"bvx2");
            assert_eq!(encode(&raw, runs, z.len()).as_deref(), Some(&z[..]));
            assert!(encode(&raw, runs, z.len() - 1).is_none());
            assert_eq!(decode(&z, raw.len()).as_deref(), Some(&raw[..]));
            assert!(decode(&z, raw.len() - 1).is_none());
            assert!(decode(&z, raw.len() + 1).is_none());
        }
        assert!(encode(&raw, 0, 100).is_none());
    }
}
