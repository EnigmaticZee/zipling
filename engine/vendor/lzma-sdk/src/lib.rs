//! Igor Pavlov's LZMA SDK (public domain, from 7-Zip 26.05) for Arca's LZMA2
//! blocks: its encoder can run its match finder in a second thread, which on a
//! 64 MiB block is 1.7x faster and uses 17% less CPU (the output differs from
//! one thread's by ~0.06% but is the same on every run); its decoder has a
//! hand-written ARM64 core.

use std::os::raw::{c_int, c_uchar, c_uint};

extern "C" {
    fn arca_lzma2_encode(
        src: *const c_uchar,
        src_len: usize,
        dst: *mut c_uchar,
        dst_len: *mut usize,
        level: c_int,
        dict_size: c_uint,
        fb: c_int,
        mc: c_uint,
        threads: c_int,
        prop: *mut c_uchar,
    ) -> c_int;
    fn arca_lzma2_decode(src: *const c_uchar, src_len: usize, dst: *mut c_uchar, dst_len: usize, prop: c_uchar) -> c_int;
}

/// LZMA2 settings. 7-Zip's `-mx9` is level 9, a 64 MiB dictionary and `fb` 64;
/// `fb` 273 is its "ultra" with the longest matches.
#[derive(Debug, Clone, Copy)]
pub struct Settings {
    pub level: i32,
    pub dict_size: u32,
    pub fb: i32,
    /// Match-finder cycles; 0 picks the SDK's default for `fb`.
    pub mc: u32,
    /// 1, or 2 to run the match finder in its own thread (deterministic output,
    /// a little different from one thread's).
    pub threads: i32,
}

/// SDK error code (`SZ_ERROR_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Error(pub i32);

/// The SDK's "output buffer full" code: the data didn't shrink enough to fit.
pub const ERROR_OUTPUT_EOF: i32 = 7;

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LZMA SDK error {}", self.0)
    }
}

impl std::error::Error for Error {}

/// Encodes `src` as one solid LZMA2 stream of at most `max_out` bytes. Returns
/// the stream and its dictionary property byte.
pub fn encode(src: &[u8], s: Settings, max_out: usize) -> Result<(Vec<u8>, u8), Error> {
    let mut out = vec![0u8; max_out];
    let mut len = max_out;
    let mut prop = 0u8;
    // SAFETY: valid buffers with their lengths; the SDK writes at most `len` bytes.
    let rc = unsafe {
        arca_lzma2_encode(src.as_ptr(), src.len(), out.as_mut_ptr(), &mut len, s.level, s.dict_size, s.fb, s.mc, s.threads, &mut prop)
    };
    if rc != 0 {
        return Err(Error(rc));
    }
    out.truncate(len);
    Ok((out, prop))
}

/// Decodes a whole LZMA2 stream into exactly `raw_len` bytes.
pub fn decode(src: &[u8], prop: u8, raw_len: usize) -> Result<Vec<u8>, Error> {
    let mut out = vec![0u8; raw_len];
    // SAFETY: valid buffers with their lengths; the SDK writes at most `raw_len` bytes.
    let rc = unsafe { arca_lzma2_decode(src.as_ptr(), src.len(), out.as_mut_ptr(), raw_len, prop) };
    if rc != 0 {
        return Err(Error(rc));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let data: Vec<u8> = (0..300_000u32).map(|i| ((i * 7) % 251) as u8 ^ (i >> 9) as u8).collect();
        for threads in [1, 2] {
            let s = Settings { level: 9, dict_size: 1 << 20, fb: 273, mc: 0, threads };
            let (c, prop) = encode(&data, s, data.len() + 1024).unwrap();
            assert!(c.len() < data.len() / 4);
            assert_eq!(decode(&c, prop, data.len()).unwrap(), data);
        }
    }

    #[test]
    fn threaded_output_is_deterministic() {
        let data: Vec<u8> = (0..500_000u32).map(|i| (i.wrapping_mul(2654435761) >> 27) as u8 + (i % 13) as u8).collect();
        let s = Settings { level: 9, dict_size: 1 << 20, fb: 64, mc: 0, threads: 2 };
        let a = encode(&data, s, data.len() + 1024).unwrap();
        for _ in 0..3 {
            assert_eq!(encode(&data, s, data.len() + 1024).unwrap(), a);
        }
    }

    #[test]
    fn damaged_input_is_an_error() {
        let data = vec![7u8; 100_000];
        let (mut c, prop) = encode(&data, Settings { level: 9, dict_size: 1 << 20, fb: 64, mc: 0, threads: 1 }, 200_000).unwrap();
        let n = c.len();
        c.truncate(n / 2);
        assert!(decode(&c, prop, data.len()).is_err());
    }
}

/// A parse decision of the encoder, as `set_token_hook`'s hook receives it:
/// `dist` is `u32::MAX` for a literal, 0-3 for a repeat (0 with `len` 1 is a
/// short repeat), else the match distance plus 4; `reps` are the four repeat
/// distances before this token, as the encoder keeps them.
pub type TokenHook = unsafe extern "C" fn(pos: u32, len: u32, dist: u32, reps: *const u32, state: u32);

#[cfg(feature = "token-hook")]
extern "C" {
    fn arca_lzma_set_token_hook(hook: Option<TokenHook>);
}

/// Research: have the encoder report every parse decision (feature `token-hook`).
#[cfg(feature = "token-hook")]
pub fn set_token_hook(hook: Option<TokenHook>) {
    // SAFETY: a plain pointer store the encoder reads on its own thread.
    unsafe { arca_lzma_set_token_hook(hook) }
}
