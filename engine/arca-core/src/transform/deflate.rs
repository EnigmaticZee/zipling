//! Bit-exact "un-deflating" of container formats (ZIP/Office/JAR, PNG, PDF, gzip).
//!
//! Deflate is a weak, 32 KiB-window codec, and a huge share of real files wrap
//! their payload in it. We locate each Deflate stream, expand it with preflate
//! (which records the tiny diff needed to reproduce the *exact* original
//! compressed bytes), and let the archive's stronger codecs see the plain data.
//!
//! The stored form is a small layout program plus the literal (non-Deflate)
//! bytes, the corrections, and the plain texts:
//!
//! ```text
//! u8 version | streams: (corr_len, plain_len)* | ops* | literal_len
//! [corrections...][literal][plain texts...]
//! ```

use crate::classify::DeflateKind;
use crate::wire::{Reader, Writer};
use anyhow::{ensure, Result};
use memchr::memmem;
use preflate_rs::{preflate_whole_deflate_stream, recreate_whole_deflate_stream, PreflateConfig};
use rayon::prelude::*;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicIsize, Ordering};

const MIN_STREAM: usize = 64;
const MAX_PLAIN_TOTAL: usize = 512 << 20;

struct StreamRec {
    corrections: Vec<u8>,
    plain: Vec<u8>,
    /// File ranges (offset, len) covered by this stream's compressed bytes, in order.
    spans: Vec<(usize, usize)>,
}

enum Op {
    Lit(u64),
    Stream { idx: u32, len: u64 },
}

struct Scanner<'a> {
    data: &'a [u8],
    config: PreflateConfig,
    /// Plain-text bytes still allowed for this file (bounds memory on Deflate bombs).
    budget: AtomicIsize,
}

/// A Deflate stream starting `skip` bytes into the virtual stream made of `pieces`.
struct Candidate {
    pieces: Vec<(usize, usize)>,
    skip: usize,
}

impl Scanner<'_> {
    /// Expands one candidate stream. Pure, so candidates run in parallel.
    fn analyze(&self, c: &Candidate) -> Option<StreamRec> {
        if self.budget.load(Ordering::Relaxed) <= 0 {
            return None;
        }
        let (pieces, skip) = (&c.pieces, c.skip);
        let owned;
        let virt: &[u8] = if pieces.len() == 1 {
            &self.data[pieces[0].0..pieces[0].0 + pieces[0].1]
        } else {
            owned = pieces.iter().flat_map(|&(o, l)| &self.data[o..o + l]).copied().collect::<Vec<u8>>();
            &owned
        };
        let input = virt.get(skip..)?;
        if input.len() < MIN_STREAM {
            return None;
        }
        let config = self.config;
        let res = catch_unwind(AssertUnwindSafe(|| preflate_whole_deflate_stream(input, &config))).ok()?.ok()?;
        let (chunk, plain) = res;
        let consumed = chunk.compressed_size;
        // A stream that needs many corrections came from an exotic encoder;
        // expanding it would cost more than it gains.
        if consumed < MIN_STREAM || chunk.corrections.len() * 4 > consumed {
            return None;
        }
        let plain = plain.text().to_vec();
        if self.budget.fetch_sub(plain.len() as isize, Ordering::Relaxed) < plain.len() as isize {
            return None;
        }

        // Map virtual range [skip, skip + consumed) back onto file spans.
        let (mut vstart, vend) = (skip, skip + consumed);
        let mut spans = Vec::new();
        let mut vpos = 0usize;
        for &(off, len) in pieces {
            let (ps, pe) = (vpos, vpos + len);
            let s = vstart.max(ps);
            let e = vend.min(pe);
            if s < e {
                spans.push((off + (s - ps), e - s));
                vstart = e;
            }
            vpos = pe;
            if vpos >= vend {
                break;
            }
        }
        Some(StreamRec { corrections: chunk.corrections, plain, spans })
    }

    /// Expands independent candidates in parallel, keeping file order and
    /// dropping any stream that overlaps an earlier one.
    fn analyze_all(&self, candidates: Vec<Candidate>) -> Vec<StreamRec> {
        let found: Vec<Option<StreamRec>> =
            candidates.par_iter().with_max_len(1).map(|c| self.analyze(c)).collect();
        let mut out: Vec<StreamRec> = Vec::new();
        let mut next_free = 0usize;
        for rec in found.into_iter().flatten() {
            let (first, _) = rec.spans[0];
            let &(last, len) = rec.spans.last().unwrap();
            if first < next_free {
                continue;
            }
            next_free = last + len;
            out.push(rec);
        }
        out
    }

    fn scan_zip(&self) -> Vec<StreamRec> {
        let d = self.data;
        let candidates = zip_deflate_entries(d)
            .into_iter()
            .filter(|&(start, csize)| start.checked_add(csize).is_some_and(|e| e <= d.len()))
            .map(|(start, csize)| Candidate { pieces: vec![(start, csize)], skip: 0 })
            .collect();
        self.analyze_all(candidates)
    }

    /// gzip members are found one after another, so this one stays sequential.
    fn scan_gzip(&self) -> Vec<StreamRec> {
        let d = self.data;
        let mut out = Vec::new();
        let mut pos = 0usize;
        while d.len() >= pos + 18 && d[pos..pos + 3] == [0x1F, 0x8B, 0x08] {
            let flg = d[pos + 3];
            let mut p = pos + 10;
            if flg & 4 != 0 {
                let Some(x) = d.get(p..p + 2) else { break };
                p += 2 + u16::from_le_bytes([x[0], x[1]]) as usize;
            }
            for bit in [8u8, 16] {
                if flg & bit != 0 {
                    let Some(z) = d.get(p..).and_then(|s| memchr::memchr(0, s)) else { return out };
                    p += z + 1;
                }
            }
            if flg & 2 != 0 {
                p += 2;
            }
            if p >= d.len() {
                break;
            }
            let Some(rec) = self.analyze(&Candidate { pieces: vec![(p, d.len() - p)], skip: 0 }) else { break };
            let consumed: usize = rec.spans.iter().map(|s| s.1).sum();
            out.push(rec);
            pos = p + consumed + 8;
        }
        out
    }

    fn scan_png(&self) -> Vec<StreamRec> {
        let d = self.data;
        let mut pos = 8usize;
        let mut idat = Vec::new();
        while pos + 12 <= d.len() {
            let len = u32::from_be_bytes(d[pos..pos + 4].try_into().unwrap()) as usize;
            let ty = &d[pos + 4..pos + 8];
            let Some(end) = (pos + 12).checked_add(len).filter(|&e| e <= d.len()) else { break };
            if ty == b"IDAT" {
                idat.push((pos + 8, len));
            } else if ty == b"IEND" {
                break;
            }
            pos = end;
        }
        if idat.is_empty() {
            return Vec::new();
        }
        // zlib: 2-byte header, raw Deflate, 4-byte Adler-32 (left as literal).
        self.analyze(&Candidate { pieces: idat, skip: 2 }).into_iter().collect()
    }

    fn scan_pdf(&self) -> Vec<StreamRec> {
        let d = self.data;
        let end_finder = memmem::Finder::new(b"endstream");
        let mut candidates = Vec::new();
        for i in memmem::find_iter(d, b"stream") {
            if i >= 3 && &d[i - 3..i] == b"end" {
                continue;
            }
            let mut s = i + 6;
            match d.get(s..s + 2) {
                Some(b"\r\n") => s += 2,
                Some([b'\n', _]) | Some([b'\r', _]) => s += 1,
                _ => continue,
            }
            let Some(h) = d.get(s..s + 2) else { continue };
            if h[0] & 0x0F != 8 || (u16::from(h[0]) << 8 | u16::from(h[1])) % 31 != 0 {
                continue;
            }
            let bound = end_finder.find(&d[s..]).map_or(d.len(), |e| s + e);
            candidates.push(Candidate { pieces: vec![(s, bound - s)], skip: 2 });
        }
        self.analyze_all(candidates)
    }
}

/// (data offset, compressed size) of every Deflate entry, from the central directory.
fn zip_deflate_entries(d: &[u8]) -> Vec<(usize, usize)> {
    let le16 = |o: usize| d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as u64);
    let le32 = |o: usize| d.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as u64);
    let le64 = |o: usize| d.get(o..o + 8).map(|b| u64::from_le_bytes(b.try_into().unwrap()));

    let tail_start = d.len().saturating_sub(65_557);
    let Some(eocd) = memmem::rfind(&d[tail_start..], b"PK\x05\x06").map(|p| p + tail_start) else {
        return Vec::new();
    };
    let (Some(mut count), Some(mut cd_off)) = (le16(eocd + 10), le32(eocd + 16)) else {
        return Vec::new();
    };
    if (count == 0xFFFF || cd_off == 0xFFFF_FFFF) && eocd >= 20 && le32(eocd - 20) == Some(0x0706_4b50) {
        if let Some(z) = le64(eocd - 12) {
            let z = z as usize;
            if le32(z) == Some(0x0606_4b50) {
                count = le64(z + 32).unwrap_or(0);
                cd_off = le64(z + 48).unwrap_or(0);
            }
        }
    }

    let mut out = Vec::new();
    let mut p = cd_off as usize;
    for _ in 0..count.min(1 << 20) {
        if le32(p) != Some(0x0201_4b50) {
            break;
        }
        let (Some(flags), Some(method), Some(mut csize), Some(mut usize_), Some(nlen), Some(xlen), Some(clen), Some(mut lho)) = (
            le16(p + 8),
            le16(p + 10),
            le32(p + 20),
            le32(p + 24),
            le16(p + 28),
            le16(p + 30),
            le16(p + 32),
            le32(p + 42),
        ) else {
            break;
        };
        // Zip64 extra field carries the real values for saturated fields, in order.
        let mut x = p + 46 + nlen as usize;
        let xend = x + xlen as usize;
        while x + 4 <= xend {
            let (Some(id), Some(sz)) = (le16(x), le16(x + 2)) else { break };
            if id == 1 {
                let mut q = x + 4;
                for field in [&mut usize_, &mut csize, &mut lho] {
                    if *field == 0xFFFF_FFFF {
                        *field = le64(q).unwrap_or(0);
                        q += 8;
                    }
                }
            }
            x += 4 + sz as usize;
        }
        p = xend + clen as usize;

        if method != 8 || flags & 1 != 0 || csize == 0 {
            continue;
        }
        let lho = lho as usize;
        if le32(lho) != Some(0x0403_4b50) {
            continue;
        }
        let (Some(ln), Some(lx)) = (le16(lho + 26), le16(lho + 28)) else { continue };
        out.push((lho + 30 + ln as usize + lx as usize, csize as usize));
    }
    out.sort_unstable();
    out.dedup_by(|b, a| b.0 < a.0 + a.1);
    out
}

/// Expands the Deflate streams in `data`. `None` when nothing worth expanding was found.
///
/// `self_check` asks preflate to re-create each stream as it goes. Callers that
/// verify the whole transformed file afterwards (the default) turn it off: one
/// round trip proves the same thing at a third less work.
pub fn encode(data: &[u8], kind: DeflateKind, max_chain: u32, self_check: bool) -> Option<Vec<u8>> {
    let sc = Scanner {
        data,
        config: PreflateConfig {
            plain_text_limit: 256 << 20,
            verify_compression: self_check,
            max_chain_length: max_chain,
        },
        budget: AtomicIsize::new(MAX_PLAIN_TOTAL as isize),
    };
    let streams = match kind {
        DeflateKind::Zip => sc.scan_zip(),
        DeflateKind::Gzip => sc.scan_gzip(),
        DeflateKind::Png => sc.scan_png(),
        DeflateKind::Pdf => sc.scan_pdf(),
    };
    if streams.is_empty() {
        return None;
    }

    // Layout: interleave literal gaps with stream spans in file order.
    let mut spans: Vec<(usize, usize, u32)> = Vec::new();
    for (i, s) in streams.iter().enumerate() {
        spans.extend(s.spans.iter().map(|&(o, l)| (o, l, i as u32)));
    }
    spans.sort_unstable();
    let mut ops = Vec::new();
    let mut literal = Vec::new();
    let mut pos = 0usize;
    for (off, len, idx) in spans {
        if off < pos {
            return None; // overlapping streams would not be reproducible
        }
        if off > pos {
            ops.push(Op::Lit((off - pos) as u64));
            literal.extend_from_slice(&data[pos..off]);
        }
        ops.push(Op::Stream { idx, len: len as u64 });
        pos = off + len;
    }
    if pos < data.len() {
        ops.push(Op::Lit((data.len() - pos) as u64));
        literal.extend_from_slice(&data[pos..]);
    }

    let mut w = Writer::new();
    w.u8(1);
    w.uv(streams.len() as u64);
    for s in &streams {
        w.uv(s.corrections.len() as u64);
        w.uv(s.plain.len() as u64);
    }
    w.uv(ops.len() as u64);
    for op in &ops {
        match *op {
            Op::Lit(n) => w.uv(n << 1),
            Op::Stream { idx, len } => {
                w.uv(len << 1 | 1);
                w.uv(idx as u64);
            }
        }
    }
    w.uv(literal.len() as u64);
    for s in &streams {
        w.raw(&s.corrections);
    }
    w.raw(&literal);
    for s in &streams {
        w.raw(&s.plain);
    }
    Some(w.buf)
}

pub fn decode(stored: &[u8], size: u64) -> Result<Vec<u8>> {
    let mut r = Reader::new(stored);
    ensure!(r.u8()? == 1, "unknown deflate layout version");
    let n = r.len()?;
    let mut lens = Vec::with_capacity(n.min(1 << 16));
    for _ in 0..n {
        lens.push((r.len()?, r.len()?));
    }
    let nops = r.len()?;
    let mut ops = Vec::with_capacity(nops.min(1 << 16));
    for _ in 0..nops {
        let v = r.uv()?;
        ops.push(if v & 1 == 0 {
            Op::Lit(v >> 1)
        } else {
            let idx = r.uv()?;
            ensure!((idx as usize) < n, "bad stream index");
            Op::Stream { idx: idx as u32, len: v >> 1 }
        });
    }
    let lit_len = r.len()?;
    let corrections: Vec<&[u8]> = lens.iter().map(|&(c, _)| r.raw(c)).collect::<Result<_>>()?;
    let literal = r.raw(lit_len)?;
    let plains: Vec<&[u8]> = lens.iter().map(|&(_, p)| r.raw(p)).collect::<Result<_>>()?;
    ensure!(r.is_empty(), "trailing bytes in deflate layout");

    // Streams are independent: re-create them on all cores.
    let recreated: Vec<Vec<u8>> = corrections
        .par_iter()
        .zip(plains.par_iter())
        .with_max_len(1)
        .map(|(c, p)| {
            catch_unwind(AssertUnwindSafe(|| recreate_whole_deflate_stream(p, c)))
                .map_err(|_| anyhow::anyhow!("preflate panicked"))?
                .map_err(|e| anyhow::anyhow!("deflate recreate failed: {e:?}"))
        })
        .collect::<Result<_>>()?;

    let mut out = Vec::with_capacity(size as usize);
    let mut lit_pos = 0usize;
    let mut stream_pos = vec![0usize; n];
    for op in ops {
        match op {
            Op::Lit(len) => {
                let end = lit_pos + len as usize;
                ensure!(end <= literal.len(), "literal overrun");
                out.extend_from_slice(&literal[lit_pos..end]);
                lit_pos = end;
            }
            Op::Stream { idx, len } => {
                let (s, p) = (&recreated[idx as usize], &mut stream_pos[idx as usize]);
                let end = *p + len as usize;
                ensure!(end <= s.len(), "stream overrun");
                out.extend_from_slice(&s[*p..end]);
                *p = end;
            }
        }
    }
    ensure!(out.len() as u64 == size, "deflate layout produced wrong size");
    Ok(out)
}
