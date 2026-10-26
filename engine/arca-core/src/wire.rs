//! Minimal little-endian / LEB128 binary encoding used by the archive index.
//!
//! The index format is defined entirely by this module plus `format.rs`, so the
//! on-disk layout stays explicit and independent of any serde crate's choices.

use anyhow::{bail, Result};

#[derive(Default)]
pub struct Writer {
    pub buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn uv(&mut self, mut v: u64) {
        loop {
            let byte = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                self.buf.push(byte);
                return;
            }
            self.buf.push(byte | 0x80);
        }
    }

    /// Zig-zag encoded signed varint.
    pub fn iv(&mut self, v: i64) {
        self.uv(((v << 1) ^ (v >> 63)) as u64);
    }

    pub fn bytes(&mut self, b: &[u8]) {
        self.uv(b.len() as u64);
        self.buf.extend_from_slice(b);
    }

    pub fn raw(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }

    pub fn str(&mut self, s: &str) {
        self.bytes(s.as_bytes());
    }
}

pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }

    pub fn u8(&mut self) -> Result<u8> {
        let Some(&b) = self.data.get(self.pos) else {
            bail!("index truncated")
        };
        self.pos += 1;
        Ok(b)
    }

    pub fn uv(&mut self) -> Result<u64> {
        let mut v: u64 = 0;
        let mut shift = 0;
        loop {
            let b = self.u8()?;
            if shift >= 64 {
                bail!("varint overflow");
            }
            v |= ((b & 0x7f) as u64) << shift;
            if b & 0x80 == 0 {
                return Ok(v);
            }
            shift += 7;
        }
    }

    /// Reads a varint that is used as an in-memory length or index.
    pub fn len(&mut self) -> Result<usize> {
        let v = self.uv()?;
        usize::try_from(v).map_err(|_| anyhow::anyhow!("length out of range"))
    }

    pub fn iv(&mut self) -> Result<i64> {
        let u = self.uv()?;
        Ok(((u >> 1) as i64) ^ -((u & 1) as i64))
    }

    pub fn raw(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.data.len());
        let Some(end) = end else { bail!("index truncated") };
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    pub fn bytes(&mut self) -> Result<&'a [u8]> {
        let n = self.len()?;
        self.raw(n)
    }

    pub fn str(&mut self) -> Result<String> {
        Ok(std::str::from_utf8(self.bytes()?)?.to_owned())
    }

    pub fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.raw(N)?.try_into().unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints_roundtrip() {
        let mut w = Writer::new();
        let vals = [0u64, 1, 127, 128, 300, u32::MAX as u64, u64::MAX];
        let ivals = [0i64, -1, 1, -64, 64, i64::MIN, i64::MAX];
        for v in vals {
            w.uv(v);
        }
        for v in ivals {
            w.iv(v);
        }
        let mut r = Reader::new(&w.buf);
        for v in vals {
            assert_eq!(r.uv().unwrap(), v);
        }
        for v in ivals {
            assert_eq!(r.iv().unwrap(), v);
        }
        assert!(r.is_empty());
    }
}
