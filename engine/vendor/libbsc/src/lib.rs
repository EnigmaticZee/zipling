//! Rust side of the vendored libbsc: one call to compress a buffer as one bsc
//! block, one to decompress it.
use std::os::raw::c_int;

extern "C" {
    fn bsc_init(features: c_int) -> c_int;
    fn bsc_compress(input: *const u8, output: *mut u8, n: c_int, lzp_hash: c_int, lzp_min: c_int, sorter: c_int, coder: c_int, features: c_int) -> c_int;
    fn arca_bsc_decompress_mt(input: *const u8, input_size: c_int, output: *mut u8, output_size: c_int, threads: c_int) -> c_int;
}

/// Bytes bsc adds in front of a block.
pub const HEADER_SIZE: usize = 28;
pub const SORTER_BWT: i32 = 1;
pub const CODER_QLFC_STATIC: i32 = 1;
pub const CODER_QLFC_ADAPTIVE: i32 = 2;
pub const CODER_QLFC_FAST: i32 = 3;
const FEATURE_FASTMODE: i32 = 1;

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// LZP hash size in bits (10..=28), or 0 for no LZP.
    pub lzp_hash: i32,
    /// LZP minimum match length (4..=255), or 0.
    pub lzp_min: i32,
    pub sorter: i32,
    pub coder: i32,
}

/// bsc's command-line defaults: LZP 15/72 bits, BWT, static QLFC.
pub const DEFAULT: Settings = Settings { lzp_hash: 15, lzp_min: 72, sorter: SORTER_BWT, coder: CODER_QLFC_STATIC };

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Error(pub i32);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "libbsc error {}", self.0)
    }
}

impl std::error::Error for Error {}

fn init() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    // SAFETY: bsc_init only fills libbsc's own tables.
    ONCE.call_once(|| unsafe {
        bsc_init(FEATURE_FASTMODE);
    });
}

/// Compresses `src` (under 2 GiB) as one bsc block.
pub fn compress(src: &[u8], s: Settings) -> Result<Vec<u8>, Error> {
    init();
    let n = c_int::try_from(src.len()).map_err(|_| Error(-1))?;
    let mut out = vec![0u8; src.len() + HEADER_SIZE];
    // SAFETY: `out` holds n + HEADER_SIZE bytes, as bsc_compress requires.
    let r = unsafe { bsc_compress(src.as_ptr(), out.as_mut_ptr(), n, s.lzp_hash, s.lzp_min, s.sorter, s.coder, FEATURE_FASTMODE) };
    if r < 0 {
        return Err(Error(r));
    }
    out.truncate(r as usize);
    Ok(out)
}

/// Decompresses one bsc block into exactly `raw_len` bytes. A block's entropy
/// coded sub-blocks and the segments of its inverse BWT decode on up to eight
/// threads (`src/arca_bsc.cpp`): one 64 MiB text block takes ~0.3 s instead of
/// ~1.6 s on one core.
pub fn decompress(src: &[u8], raw_len: usize) -> Result<Vec<u8>, Error> {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(8) as c_int;
    decompress_threads(src, raw_len, threads)
}

/// As `decompress`, on at most `threads` threads.
pub fn decompress_threads(src: &[u8], raw_len: usize, threads: c_int) -> Result<Vec<u8>, Error> {
    init();
    let mut out = vec![0u8; raw_len];
    let (n, m) = (c_int::try_from(src.len()).map_err(|_| Error(-1))?, c_int::try_from(raw_len).map_err(|_| Error(-1))?);
    // SAFETY: both buffers are valid for their lengths.
    let r = unsafe { arca_bsc_decompress_mt(src.as_ptr(), n, out.as_mut_ptr(), m, threads) };
    if r != 0 {
        return Err(Error(r));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(n: usize) -> Vec<u8> {
        let words = ["alpha ", "beta ", "gamma ", "delta\n", "epsilon ", "zeta "];
        let mut s = 7u64;
        (0..n)
            .flat_map(|_| {
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                words[(s % 6) as usize].bytes()
            })
            .collect()
    }

    #[test]
    fn round_trips() {
        for n in [1usize, 100, 70_000] {
            let d = data(n);
            for s in [DEFAULT, Settings { lzp_hash: 24, lzp_min: 64, sorter: SORTER_BWT, coder: CODER_QLFC_ADAPTIVE }] {
                match compress(&d, s) {
                    Ok(c) => assert_eq!(decompress(&c, d.len()).unwrap(), d),
                    Err(Error(-3)) => {} // too small to compress
                    Err(e) => panic!("{e}"),
                }
            }
        }
    }

    #[test]
    fn damaged_input_is_an_error() {
        let d = data(50_000);
        let mut c = compress(&d, DEFAULT).unwrap();
        let mid = c.len() / 2;
        c[mid] ^= 0x5A;
        assert!(decompress(&c, d.len()).map_or(true, |b| b != d));
        assert!(decompress(&c[..10], d.len()).is_err());
    }
}
