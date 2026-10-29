//! zlib 1.3.2's raw Deflate encoder, pinned so its output never changes.

use std::os::raw::c_int;

extern "C" {
    fn arca_zlib_matches(plain: *const u8, n: usize, level: c_int, wbits: c_int, mem: c_int, strategy: c_int, target: *const u8, len: usize) -> c_int;
    fn arca_zlib_deflate(plain: *const u8, n: usize, level: c_int, wbits: c_int, mem: c_int, strategy: c_int, out: *mut u8, cap: usize) -> i64;
}

/// The settings `deflateInit2` takes (raw Deflate: no zlib or gzip wrapper).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Params {
    /// 1-9.
    pub level: u8,
    /// 9-15 (a window of 2^bits bytes).
    pub window_bits: u8,
    /// 1-9.
    pub mem_level: u8,
    /// 0 default, 1 filtered, 2 Huffman only, 3 RLE, 4 fixed.
    pub strategy: u8,
}

impl Params {
    pub fn valid(&self) -> bool {
        (1..=9).contains(&self.level) && (9..=15).contains(&self.window_bits) && (1..=9).contains(&self.mem_level) && self.strategy <= 4
    }
}

/// Whether Deflate with `p` turns `plain` into exactly `target`. Stops at the
/// first byte that differs, so a wrong guess usually costs little.
pub fn matches(plain: &[u8], p: Params, target: &[u8]) -> bool {
    p.valid()
        && unsafe {
            arca_zlib_matches(plain.as_ptr(), plain.len(), p.level.into(), p.window_bits.into(), p.mem_level.into(), p.strategy.into(), target.as_ptr(), target.len())
        } == 1
}

/// Deflates `plain` with `p`; `None` unless the output is exactly `len` bytes.
pub fn deflate(plain: &[u8], p: Params, len: usize) -> Option<Vec<u8>> {
    if !p.valid() {
        return None;
    }
    // One spare byte: zlib may need room past the end to report it has finished.
    let mut out = vec![0u8; len + 1];
    let n = unsafe {
        arca_zlib_deflate(plain.as_ptr(), plain.len(), p.level.into(), p.window_bits.into(), p.mem_level.into(), p.strategy.into(), out.as_mut_ptr(), out.len())
    };
    (n == len as i64).then(|| {
        out.truncate(len);
        out
    })
}

/// Deflates `plain` with `p` (output of any length; for making test data).
pub fn compress(plain: &[u8], p: Params) -> Option<Vec<u8>> {
    if !p.valid() {
        return None;
    }
    let mut out = vec![0u8; plain.len() + plain.len() / 8 + (64 << 10)];
    let n = unsafe {
        arca_zlib_deflate(plain.as_ptr(), plain.len(), p.level.into(), p.window_bits.into(), p.mem_level.into(), p.strategy.into(), out.as_mut_ptr(), out.len())
    };
    (n >= 0).then(|| {
        out.truncate(n as usize);
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deflate_then_match() {
        let plain: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8 ^ (i / 1000) as u8).collect();
        let p = Params { level: 1, window_bits: 15, mem_level: 8, strategy: 0 };
        let big = compress(&plain, p).unwrap();
        let z = deflate(&plain, p, big.len()).unwrap();
        assert_eq!(z, big);
        assert!(matches(&plain, p, &z));
        assert!(!matches(&plain, Params { level: 6, ..p }, &z));
        assert!(!matches(&plain, p, &z[..z.len() - 1]));
        assert!(deflate(&plain, p, z.len() - 1).is_none());
        assert!(!matches(&plain, Params { level: 0, ..p }, &z));
    }
}
