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
//!
//! Version 2 is a PNG whose pixels are coded with `pixels.rs` instead of being
//! stored as filtered rows (one stream; `plain_len` is the rows' length):
//!
//! ```text
//! version-1 fields up to literal_len | width | height | u8 channels | coded_len
//! [corrections][literal][row filter types: height bytes][coded pixels]
//! ```
//!
//! With bit `REPLAY` (0x10) set in the version byte, each stream's two lengths are
//! followed by how it is re-created: `u8 0` preflate (with its corrections, as
//! above); `u8 1 | level | window bits | memory level | strategy` for a stream
//! that stock zlib wrote, which running zlib 1.3.2 again re-creates byte for byte
//! (no corrections); `u8 2 | literal runs` for an LZFSE stream (disk images,
//! asset catalogs) that the pinned lzfse 1.0 re-creates with that setting. Most
//! PNGs made on a Mac are zlib streams (ImageIO writes level 1), and a re-run
//! costs less than preflate's analysis. A disk image's chunks share one
//! setting, found on the first chunk; level 8-9 images (most app installers)
//! re-run quickly because the pinned zlib searches matches faster there, with
//! the same output.
//!
//! Despite the name, the transform also takes containers whose streams are
//! LZFSE: UDIF disk images and asset catalogs (`Assets.car`).

use crate::classify::DeflateKind;
use crate::transform::pixels;
use crate::wire::{Reader, Writer};
use anyhow::{ensure, Result};
use memchr::memmem;
use preflate_rs::{preflate_whole_deflate_stream, recreate_whole_deflate_stream, PreflateConfig};
use rayon::prelude::*;
use zlib_pinned::Params as ZlibParams;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicIsize, Ordering};

const MIN_STREAM: usize = 64;
const MAX_PLAIN_TOTAL: usize = 512 << 20;
/// Version-byte flag: streams carry their re-creation method (see the module docs).
const REPLAY: u8 = 0x10;

/// How a stream is re-created at extraction.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Method {
    /// preflate, from the plain text and its corrections.
    Preflate,
    /// Running stock zlib again with these settings (no corrections).
    Zlib(ZlibParams),
    /// Running the pinned lzfse with this `literal_runs` setting.
    Lzfse(u8),
}

struct StreamRec {
    method: Method,
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

/// A Deflate stream starting `skip` bytes into the virtual stream made of `pieces`,
/// or (with `lzfse` set to its decoded length) a whole LZFSE stream.
struct Candidate {
    pieces: Vec<(usize, usize)>,
    skip: usize,
    lzfse: Option<usize>,
    /// zlib settings that re-created a sibling stream (a disk image's chunks all
    /// come from one encoder), tried for any level.
    zlib: Option<ZlibParams>,
}

impl Candidate {
    fn deflate(pieces: Vec<(usize, usize)>, skip: usize) -> Self {
        Candidate { pieces, skip, lzfse: None, zlib: None }
    }
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
        let replayed = match c.lzfse {
            Some(raw_len) => Some(self.lzfse(input, raw_len)?),
            None => self.replay(virt, skip, c.zlib),
        };
        let (method, corrections, plain, consumed) = match replayed {
            Some((method, plain, consumed)) => (method, Vec::new(), plain, consumed),
            None => self.preflate(input)?,
        };
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
        Some(StreamRec { method, corrections, plain, spans })
    }

    /// Expands a raw Deflate stream with preflate: (method, corrections, plain
    /// text, stream length).
    fn preflate(&self, input: &[u8]) -> Option<(Method, Vec<u8>, Vec<u8>, usize)> {
        let config = self.config;
        let (chunk, plain) = catch_unwind(AssertUnwindSafe(|| preflate_whole_deflate_stream(input, &config))).ok()?.ok()?;
        let consumed = chunk.compressed_size;
        if std::env::var_os("ARCA_DEBUG_DEFLATE").is_some() {
            eprintln!("deflate\t{consumed}\t{}\t{}\t{}", plain.text().len(), chunk.corrections.len(), chunk.steps);
        }
        // A stream that needs many corrections came from an exotic encoder;
        // expanding it would cost more than it gains.
        if consumed < MIN_STREAM || chunk.corrections.len() * 4 > consumed {
            return None;
        }
        Some((Method::Preflate, chunk.corrections, plain.text().to_vec(), consumed))
    }

    /// Decodes a whole LZFSE stream and checks that the pinned lzfse re-creates
    /// it: first with macOS's current setting, then with lzfse 1.0's. Streams
    /// under 4 KiB are skipped (both encoders use LZVN there, and theirs differ).
    fn lzfse(&self, stream: &[u8], raw_len: usize) -> Option<(Method, Vec<u8>, usize)> {
        let limit = self.config.plain_text_limit.min(self.budget.load(Ordering::Relaxed).max(0) as usize);
        if raw_len < 4096 || raw_len > limit || std::env::var_os("ARCA_EXP_NO_LZFSE").is_some() {
            return None;
        }
        let plain = lzfse_pinned::decode(stream, raw_len)?;
        for runs in [lzfse_pinned::MACOS_LITERAL_RUNS, 8] {
            if lzfse_pinned::encode(&plain, runs, stream.len()).as_deref() == Some(stream) {
                return Some((Method::Lzfse(runs), plain, stream.len()));
            }
        }
        None
    }

    /// Re-creates a zlib-wrapped stream (`skip` = 2: the header comes first) by
    /// running stock zlib again, when its header says level 1-5 or `known`
    /// settings are given. Returns the settings, the plain text and the Deflate
    /// stream's length. Other level 6-9 streams are left to preflate: level 6
    /// re-runs slower than preflate rebuilds, and level 8-9 PNGs (flat images,
    /// ~30x their Deflate bytes) cost far more than they save.
    fn replay(&self, virt: &[u8], skip: usize, known: Option<ZlibParams>) -> Option<(Method, Vec<u8>, usize)> {
        if skip != 2 || std::env::var_os("ARCA_EXP_NO_REPLAY").is_some() {
            return None;
        }
        let (flevel, window_bits) = zlib_header(virt)?;
        let tries = match known {
            Some(p) if p.window_bits == window_bits => vec![p],
            Some(_) => return None,
            None if flevel <= 1 => zlib_settings(flevel, window_bits),
            None => return None,
        };
        let (plain, consumed) = self.inflate(&virt[skip..])?;
        let target = &virt[skip..skip + consumed];
        let p = tries.into_iter().find(|&p| zlib_pinned::matches(&plain, p, target))?;
        Some((Method::Zlib(p), plain, consumed))
    }

    /// The zlib settings that re-create a zlib-wrapped stream, trying every
    /// setting its header's level allows.
    fn probe_zlib(&self, virt: &[u8]) -> Option<ZlibParams> {
        let (flevel, window_bits) = zlib_header(virt)?;
        let (plain, consumed) = self.inflate(&virt[2..])?;
        zlib_settings(flevel, window_bits).into_iter().find(|&p| zlib_pinned::matches(&plain, p, &virt[2..2 + consumed]))
    }

    /// Inflates a raw Deflate stream within the plain-text budget. Returns the
    /// plain text and the stream's length.
    fn inflate(&self, input: &[u8]) -> Option<(Vec<u8>, usize)> {
        let limit = self.config.plain_text_limit.min(self.budget.load(Ordering::Relaxed).max(0) as usize);
        let mut d = flate2::Decompress::new(false);
        let mut plain = Vec::with_capacity((input.len() * 4).clamp(1 << 16, limit.max(1 << 16)));
        loop {
            let at = d.total_in() as usize;
            let before = d.total_out();
            match d.decompress_vec(&input[at..], &mut plain, flate2::FlushDecompress::Finish).ok()? {
                flate2::Status::StreamEnd => break,
                _ if plain.len() >= limit => return None,
                _ if plain.len() == plain.capacity() => plain.reserve(plain.len().min(limit - plain.len()).max(1 << 16)),
                _ if d.total_out() == before && d.total_in() as usize == at => return None,
                _ => {}
            }
        }
        Some((plain, d.total_in() as usize))
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
            .map(|(start, csize)| Candidate::deflate(vec![(start, csize)], 0))
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
            let Some(rec) = self.analyze(&Candidate::deflate(vec![(p, d.len() - p)], 0)) else { break };
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
        // The zlib header's FLEVEL says 3 for levels 7-9. With a short search
        // limit (Default) preflate rejects nearly all of those after doing the
        // whole analysis (2.7 s of CPU on the saved folder's 92 level-9 PNGs).
        if self.config.max_chain_length < 1024 && d.get(idat[0].0 + 1).is_some_and(|&flg| flg >> 6 == 3) {
            return Vec::new();
        }
        // zlib: 2-byte header, raw Deflate, 4-byte Adler-32 (left as literal).
        self.analyze(&Candidate::deflate(idat, 2)).into_iter().collect()
    }

    /// A UDIF disk image: its `koly` trailer points at an XML property list whose
    /// `blkx` entries are base64 `mish` tables, one row per chunk of the image.
    /// zlib chunks (UDZO, `hdiutil`'s default, level 1) and LZFSE chunks (ULFO)
    /// are expanded.
    fn scan_dmg(&self) -> Vec<StreamRec> {
        let d = self.data;
        let be32 = |b: &[u8], o: usize| b.get(o..o + 4).map_or(0, |x| u32::from_be_bytes(x.try_into().unwrap())) as u64;
        let be64 = |b: &[u8], o: usize| b.get(o..o + 8).map_or(0, |x| u64::from_be_bytes(x.try_into().unwrap()));
        let Some(koly) = d.len().checked_sub(512).map(|t| &d[t..]) else { return Vec::new() };
        let (fork, xml_at, xml_len) = (be64(koly, 24), be64(koly, 216) as usize, be64(koly, 224) as usize);
        let Some(xml) = xml_at.checked_add(xml_len).and_then(|e| d.get(xml_at..e)) else { return Vec::new() };
        let mut candidates = Vec::new();
        let mut rest = xml;
        while let Some(s) = memmem::find(rest, b"<data>") {
            let body = &rest[s + 6..];
            let Some(e) = memmem::find(body, b"</data>") else { break };
            rest = &body[e..];
            let Some(mish) = base64(&body[..e]).filter(|m| m.starts_with(b"mish")) else { continue };
            let base = be64(&mish, 24);
            let n = be32(&mish, 200).min((mish.len().saturating_sub(204) / 40) as u64) as usize;
            for i in 0..n {
                let c = 204 + 40 * i;
                let (kind, sectors, off, len) = (be32(&mish, c), be64(&mish, c + 16), be64(&mish, c + 24), be64(&mish, c + 32));
                let at = fork.checked_add(base).and_then(|v| v.checked_add(off));
                let fits = at.and_then(|a| a.checked_add(len)).is_some_and(|e| e <= d.len() as u64);
                if !fits || (len as usize) < MIN_STREAM {
                    continue;
                }
                let pieces = vec![(at.unwrap() as usize, len as usize)];
                match kind {
                    0x8000_0005 => candidates.push(Candidate::deflate(pieces, 2)),
                    0x8000_0007 if sectors < 1 << 22 => {
                        candidates.push(Candidate { pieces, skip: 0, lzfse: Some(sectors as usize * 512), zlib: None })
                    }
                    _ => {}
                }
            }
        }
        candidates.sort_unstable_by_key(|c| c.pieces[0].0);
        // Chunks from zlib levels 7-9 are worth expanding only at the deep search
        // of Max and Ultra: Discord's cost five times the others' to rebuild
        // (19 CPU-s to unpack, against 3-4), which Default's extraction can't carry.
        if self.config.max_chain_length < 4096 {
            candidates.retain(|c| {
                let chunk = &d[c.pieces[0].0..c.pieces[0].0 + c.pieces[0].1];
                c.lzfse.is_some() || zlib_header(chunk).is_none_or(|(flevel, _)| flevel < 3)
            });
        }
        // One encoder wrote every chunk. If stock zlib at some level re-creates
        // the first level 6-9 chunk (`hdiutil -imagekey zlib-level=9`, as many
        // app installers are made), the rest are tried with the same setting.
        let span = |c: &Candidate| &d[c.pieces[0].0..c.pieces[0].0 + c.pieces[0].1];
        let deep = |c: &Candidate| c.lzfse.is_none() && zlib_header(span(c)).is_some_and(|(flevel, _)| flevel >= 2);
        let known = candidates.iter().find(|c| deep(c)).and_then(|first| self.probe_zlib(span(first)));
        for c in candidates.iter_mut().filter(|c| deep(c)) {
            c.zlib = known;
        }
        self.analyze_all(candidates)
    }

    /// An asset catalog (`Assets.car`, a BOM store): its renditions hold LZFSE
    /// streams, found by walking their block headers from `bvx` to `bvx$`.
    fn scan_car(&self) -> Vec<StreamRec> {
        let d = self.data;
        let mut candidates = Vec::new();
        let mut at = 0usize;
        while let Some(p) = memmem::find(&d[at..], b"bvx").map(|p| p + at) {
            match lzfse_extent(&d[p..]) {
                Some((len, raw_len)) => {
                    candidates.push(Candidate { pieces: vec![(p, len)], skip: 0, lzfse: Some(raw_len), zlib: None });
                    at = p + len;
                }
                None => at = p + 3,
            }
        }
        self.analyze_all(candidates)
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
            candidates.push(Candidate::deflate(vec![(s, bound - s)], 2));
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

/// A zlib header's FLEVEL and window bits; `None` unless it is a valid Deflate
/// header without a preset dictionary.
fn zlib_header(virt: &[u8]) -> Option<(u8, u8)> {
    let (&cmf, &flg) = (virt.first()?, virt.get(1)?);
    let window_bits = (cmf >> 4) + 8;
    let valid = cmf & 0x0F == 8 && flg & 0x20 == 0 && (u16::from(cmf) << 8 | u16::from(flg)) % 31 == 0 && (9..=15).contains(&window_bits);
    valid.then_some((flg >> 6, window_bits))
}

/// The zlib settings a header's FLEVEL allows, likeliest first. zlib writes
/// FLEVEL 0 for level 1 (or level 0 and the Huffman-only and RLE strategies,
/// which preflate handles), 1 for levels 2-5, 2 for level 6 and 3 for levels
/// 7-9. Levels 1-3 ignore the filtered strategy.
fn zlib_settings(flevel: u8, window_bits: u8) -> Vec<ZlibParams> {
    let levels: &[u8] = match flevel {
        0 => &[1],
        1 => &[2, 3, 4, 5],
        2 => &[6],
        _ => &[9, 8, 7],
    };
    let mut out = Vec::new();
    for &level in levels {
        for mem_level in [8, 9] {
            for &strategy in if level >= 4 { &[0u8, 1][..] } else { &[0u8][..] } {
                out.push(ZlibParams { level, window_bits, mem_level, strategy });
            }
        }
    }
    out
}

/// The length and decoded length of the LZFSE stream at the start of `d`, read
/// from its block headers (v2 LZFSE, LZVN and stored blocks, then `bvx$`).
fn lzfse_extent(d: &[u8]) -> Option<(usize, usize)> {
    let le32 = |o: usize| d.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize);
    let le64 = |o: usize| d.get(o..o + 8).map(|b| u64::from_le_bytes(b.try_into().unwrap()));
    let (mut p, mut raw) = (0usize, 0usize);
    loop {
        let magic = d.get(p..p + 4)?;
        let size = match magic {
            b"bvx$" => return (p > 0).then_some((p + 4, raw)),
            b"bvx2" => {
                let (v0, v1, v2) = (le64(p + 8)?, le64(p + 16)?, le64(p + 24)?);
                (v2 & 0xFFFF_FFFF) as usize + ((v0 >> 20) & 0xF_FFFF) as usize + ((v1 >> 40) & 0xF_FFFF) as usize
            }
            b"bvxn" => 12 + le32(p + 8)?,
            b"bvx-" => 8 + le32(p + 4)?,
            _ => return None,
        };
        raw = raw.checked_add(le32(p + 4)?)?;
        p = p.checked_add(size).filter(|&e| e <= d.len())?;
    }
}

/// Decodes standard base64, skipping whitespace; `None` on anything else.
fn base64(text: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for &c in text {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b' ' | b'\t' | b'\r' | b'\n' => continue,
            _ => return None,
        };
        acc = acc << 6 | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// The transformed form of a file.
pub struct Encoded {
    pub stored: Vec<u8>,
    /// Where already entropy-coded bytes (coded pixels) start, if any: they go to
    /// the store lane, the rest to a compressing lane.
    pub store_from: Option<usize>,
}

/// Width, height and channels of an 8-bit, non-interlaced PNG whose pixels the
/// pixel codec can take (grey, grey + alpha, RGB, RGBA).
fn png_geometry(d: &[u8]) -> Option<(usize, usize, usize)> {
    let h = d.get(8..33)?;
    if &h[4..8] != b"IHDR" || u32::from_be_bytes(h[0..4].try_into().ok()?) != 13 {
        return None;
    }
    let (w, ht) = (u32::from_be_bytes(h[8..12].try_into().ok()?), u32::from_be_bytes(h[12..16].try_into().ok()?));
    let (depth, ctype, interlace) = (h[16], h[17], h[20]);
    let ch = match ctype {
        0 => 1,
        2 => 3,
        4 => 2,
        6 => 4,
        _ => return None,
    };
    (depth == 8 && interlace == 0 && w > 0 && ht > 0).then_some((w as usize, ht as usize, ch))
}

/// Expands the Deflate streams in `data`. `None` when nothing worth expanding was found.
///
/// `self_check` asks preflate to re-create each stream as it goes. Callers that
/// verify the whole transformed file afterwards (the default) turn it off: one
/// round trip proves the same thing at a third less work. With `pixels`, a PNG's
/// pixels are coded with the pixel codec when that beats compressing its rows
/// (and, when the value is not 0, saves a byte per that many pixel bytes).
pub fn encode(
    data: &[u8],
    kind: DeflateKind,
    max_chain: u32,
    steps_per_byte: u32,
    self_check: bool,
    pixels: Option<u32>,
) -> Option<Encoded> {
    let sc = Scanner {
        data,
        config: PreflateConfig {
            plain_text_limit: 256 << 20,
            verify_compression: self_check,
            max_chain_length: max_chain,
            cap_max_chain: std::env::var_os("ARCA_EXP_CAP").is_some(),
            max_steps_per_byte: std::env::var("ARCA_EXP_STEPS").ok().and_then(|v| v.parse().ok()).unwrap_or(steps_per_byte),
        },
        budget: AtomicIsize::new(MAX_PLAIN_TOTAL as isize),
    };
    let streams = match kind {
        DeflateKind::Zip => sc.scan_zip(),
        DeflateKind::Gzip => sc.scan_gzip(),
        DeflateKind::Png => sc.scan_png(),
        DeflateKind::Pdf => sc.scan_pdf(),
        DeflateKind::Dmg => sc.scan_dmg(),
        DeflateKind::AssetCatalog => sc.scan_car(),
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

    // Pixel mode: when the pixel codec's output is under 0.9x what zstd-1 makes of
    // the rows, LZMA would not beat it either (on the saved folder this choice is
    // within 1% of picking the better of the codec and xz-9 per image). 0.9x
    // zstd-1 is also close to zstd-12 (median 0.88x), so it measures the saving.
    let mut coded_pixels = None;
    if let (Some(per_byte), DeflateKind::Png, 1) = (pixels, kind, streams.len()) {
        if let Some((pw, ph, ch)) = png_geometry(data) {
            let rows = &streams[0].plain;
            if let Some((types, px)) = pixels::unfilter(rows, pw, ph, ch) {
                let coded = {
                    let _p = crate::prof::span("fwd.pixels", px.len() as u64);
                    pixels::encode(&px, pw, ph, ch)
                };
                let lz = zstd::bulk::compress(rows, 1).map_or(usize::MAX, |c| c.len()) as i64;
                // Bytes saved against 0.9x zstd-1, in tenths.
                let saved10 = lz * 9 - coded.len() as i64 * 10;
                if saved10 > 0 && (per_byte == 0 || saved10 * per_byte as i64 >= px.len() as i64 * 10) {
                    coded_pixels = Some((pw, ph, ch, types, coded));
                }
            }
        }
    }

    let mut w = Writer::new();
    let replayed = streams.iter().any(|s| s.method != Method::Preflate);
    w.u8(if coded_pixels.is_some() { 2 } else { 1 } | if replayed { REPLAY } else { 0 });
    w.uv(streams.len() as u64);
    for s in &streams {
        w.uv(s.corrections.len() as u64);
        w.uv(s.plain.len() as u64);
        if replayed {
            match s.method {
                Method::Preflate => w.u8(0),
                Method::Zlib(p) => {
                    w.u8(1);
                    for v in [p.level, p.window_bits, p.mem_level, p.strategy] {
                        w.u8(v);
                    }
                }
                Method::Lzfse(runs) => {
                    w.u8(2);
                    w.u8(runs);
                }
            }
        }
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
    if let Some((pw, ph, ch, types, coded)) = &coded_pixels {
        w.uv(*pw as u64);
        w.uv(*ph as u64);
        w.u8(*ch as u8);
        w.uv(coded.len() as u64);
        w.raw(&streams[0].corrections);
        w.raw(&literal);
        w.raw(types);
        let store_from = w.buf.len();
        w.raw(coded);
        return Some(Encoded { stored: w.buf, store_from: Some(store_from) });
    }
    for s in &streams {
        w.raw(&s.corrections);
    }
    w.raw(&literal);
    for s in &streams {
        w.raw(&s.plain);
    }
    Some(Encoded { stored: w.buf, store_from: None })
}

/// Largest pixel image a stored PNG may ask the decoder to rebuild.
const MAX_PIXEL_ROWS: usize = 1 << 30;

pub fn decode(stored: &[u8], size: u64) -> Result<Vec<u8>> {
    let mut r = Reader::new(stored);
    let byte = r.u8()?;
    let (version, replayed) = (byte & !REPLAY, byte & REPLAY != 0);
    ensure!(version == 1 || version == 2, "unknown deflate layout version");
    let n = r.len()?;
    let mut lens = Vec::with_capacity(n.min(1 << 16));
    let mut methods = Vec::with_capacity(n.min(1 << 16));
    for _ in 0..n {
        let (c, p) = (r.len()?, r.len()?);
        lens.push((c, p));
        let method = match replayed.then(|| r.u8()).transpose()? {
            None | Some(0) => Method::Preflate,
            Some(1) => Method::Zlib(ZlibParams { level: r.u8()?, window_bits: r.u8()?, mem_level: r.u8()?, strategy: r.u8()? }),
            Some(2) => Method::Lzfse(r.u8()?),
            Some(_) => anyhow::bail!("unknown deflate stream method"),
        };
        ensure!(c == 0 || method == Method::Preflate, "a re-run stream has no corrections");
        methods.push(method);
    }
    let nops = r.len()?;
    let mut ops = Vec::with_capacity(nops.min(1 << 16));
    // Each stream's length: what a re-run must produce.
    let mut stream_len = vec![0u64; n];
    for _ in 0..nops {
        let v = r.uv()?;
        ops.push(if v & 1 == 0 {
            Op::Lit(v >> 1)
        } else {
            let idx = r.uv()?;
            ensure!((idx as usize) < n, "bad stream index");
            stream_len[idx as usize] = stream_len[idx as usize].saturating_add(v >> 1);
            Op::Stream { idx: idx as u32, len: v >> 1 }
        });
    }
    let lit_len = r.len()?;
    let rebuilt_rows;
    let (corrections, literal, plains): (Vec<&[u8]>, &[u8], Vec<&[u8]>) = if version == 2 {
        let (pw, ph, ch, coded_len) = (r.len()?, r.len()?, r.u8()? as usize, r.len()?);
        ensure!(n == 1 && (1..=4).contains(&ch), "bad pixel layout");
        let rows_len = pw.checked_mul(ch).and_then(|s| s.checked_add(1)).and_then(|s| s.checked_mul(ph));
        ensure!(rows_len == Some(lens[0].1) && lens[0].1 <= MAX_PIXEL_ROWS, "pixel layout does not match its rows");
        let corrections = vec![r.raw(lens[0].0)?];
        let literal = r.raw(lit_len)?;
        let types = r.raw(ph)?;
        let px = {
            let _p = crate::prof::span("inverse.pixels", (pw * ph * ch) as u64);
            pixels::decode(r.raw(coded_len)?, pw, ph, ch)?
        };
        rebuilt_rows = pixels::refilter(types, &px, pw, ph, ch)?;
        (corrections, literal, vec![&rebuilt_rows[..]])
    } else {
        let corrections = lens.iter().map(|&(c, _)| r.raw(c)).collect::<Result<_>>()?;
        let literal = r.raw(lit_len)?;
        (corrections, literal, lens.iter().map(|&(_, p)| r.raw(p)).collect::<Result<_>>()?)
    };
    ensure!(r.is_empty(), "trailing bytes in deflate layout");

    // Streams are independent: re-create them on all cores.
    let recreated: Vec<Vec<u8>> = (0..n)
        .into_par_iter()
        .with_max_len(1)
        .map(|i| match methods[i] {
            // Re-run lengths are bounded by the stored file's size, so a damaged
            // layout can't ask for more.
            Method::Zlib(params) => {
                ensure!(stream_len[i] <= size, "stream longer than its file");
                zlib_pinned::deflate(plains[i], params, stream_len[i] as usize).ok_or_else(|| anyhow::anyhow!("zlib re-run gave a different stream"))
            }
            Method::Lzfse(runs) => {
                ensure!(stream_len[i] <= size, "stream longer than its file");
                lzfse_pinned::encode(plains[i], runs, stream_len[i] as usize).ok_or_else(|| anyhow::anyhow!("lzfse re-run gave a different stream"))
            }
            Method::Preflate => catch_unwind(AssertUnwindSafe(|| recreate_whole_deflate_stream(plains[i], corrections[i])))
                .map_err(|_| anyhow::anyhow!("preflate panicked"))?
                .map_err(|e| anyhow::anyhow!("deflate recreate failed: {e:?}")),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// An RGB PNG whose IDAT stream stock zlib wrote at `level` (filtered
    /// strategy at 4 and up, as libpng does), in two IDAT chunks.
    fn png(level: u8) -> Vec<u8> {
        let (w, h) = (300usize, 200usize);
        let mut rows = Vec::new();
        for y in 0..h {
            rows.push((y % 3) as u8);
            rows.extend((0..w * 3).map(|i| ((i * 7 + y * 13) % 256) as u8 ^ ((i * y) % 7) as u8));
        }
        let strategy = u8::from(level >= 4);
        let body = zlib_pinned::compress(&rows, ZlibParams { level, window_bits: 15, mem_level: 8, strategy }).unwrap();
        let flevel = match level {
            0 | 1 => 0u8,
            2..=5 => 1,
            6 => 2,
            _ => 3,
        };
        let cmf = 0x78u8;
        let mut flg = flevel << 6;
        flg += (31 - ((u16::from(cmf) << 8 | u16::from(flg)) % 31) as u8) % 31;
        let (mut a, mut b) = (1u32, 0u32);
        for &x in &rows {
            a = (a + u32::from(x)) % 65521;
            b = (b + a) % 65521;
        }
        let mut z = vec![cmf, flg];
        z.extend_from_slice(&body);
        z.extend_from_slice(&(b << 16 | a).to_be_bytes());
        let chunk = |out: &mut Vec<u8>, ty: &[u8], data: &[u8]| {
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            out.extend_from_slice(ty);
            out.extend_from_slice(data);
            out.extend_from_slice(&crc32fast::hash(&[ty, data].concat()).to_be_bytes());
        };
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&(w as u32).to_be_bytes());
        ihdr.extend_from_slice(&(h as u32).to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        chunk(&mut out, b"IHDR", &ihdr);
        let half = z.len() / 2;
        chunk(&mut out, b"IDAT", &z[..half]);
        chunk(&mut out, b"IDAT", &z[half..]);
        chunk(&mut out, b"IEND", b"");
        out
    }

    #[test]
    fn stock_zlib_streams_are_re_run() {
        for (level, replayed) in [(1, true), (3, true), (5, true), (6, false), (9, false)] {
            let data = png(level);
            for pixels in [None, Some(0)] {
                let e = encode(&data, DeflateKind::Png, 4096, 128, false, pixels).unwrap();
                assert_eq!(e.stored[0] & REPLAY != 0, replayed, "level {level}");
                assert_eq!(decode(&e.stored, data.len() as u64).unwrap(), data, "level {level}");
            }
        }
    }

    /// A made-up asset catalog: LZFSE streams written with both settings between
    /// other bytes, one of them too short to bother with.
    #[test]
    fn lzfse_streams_are_re_run() {
        let image = |seed: u32, n: usize| -> Vec<u8> { (0..n as u32).map(|i| ((i / 7 + seed) % 13) as u8 * 17 ^ (i % 5) as u8).collect() };
        let mut data = b"BOMStore".to_vec();
        data.extend_from_slice(&[0x5A; 300]);
        for (runs, n) in [(3u8, 200_000), (8, 90_000), (3, 1000)] {
            let z = lzfse_pinned::compress(&image(n as u32, n), runs);
            assert!(z.starts_with(b"bvx"));
            data.extend_from_slice(&z);
            data.extend_from_slice(b"MLEC rendition header bytes");
        }
        let e = encode(&data, DeflateKind::AssetCatalog, 256, 128, false, None).unwrap();
        assert!(e.stored[0] & REPLAY != 0);
        assert!(e.stored.len() > 290_000, "both big streams expanded");
        assert_eq!(decode(&e.stored, data.len() as u64).unwrap(), data);
    }

    #[test]
    fn level_9_disk_image_chunks_are_re_run() {
        // A minimal UDIF image: three zlib level-9 chunks, a `mish` table in the
        // XML's <data>, and a `koly` trailer pointing at it.
        let chunk = |seed: u32| -> Vec<u8> {
            let plain: Vec<u8> = (0..100_000u32).flat_map(|i| format!("{} {}\n", i % (97 + seed), i / 13).into_bytes()).collect();
            let p = ZlibParams { level: 9, window_bits: 15, mem_level: 8, strategy: 0 };
            let (mut a, mut b) = (1u32, 0u32);
            for &x in &plain {
                a = (a + u32::from(x)) % 65521;
                b = (b + a) % 65521;
            }
            let mut z = vec![0x78, 0xDA];
            z.extend(zlib_pinned::compress(&plain, p).unwrap());
            z.extend((b << 16 | a).to_be_bytes());
            z
        };
        let mut data = Vec::new();
        let mut mish = b"mish".to_vec();
        mish.resize(204, 0);
        mish[200..204].copy_from_slice(&3u32.to_be_bytes());
        for seed in 0..3 {
            let z = chunk(seed);
            for v in [0x8000_0005u64 << 32, seed as u64 * 1000, 1000, data.len() as u64, z.len() as u64] {
                mish.extend(&v.to_be_bytes());
            }
            data.extend(z);
        }
        let b64 = |b: &[u8]| -> String {
            const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            b.chunks(3)
                .flat_map(|c| {
                    let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
                    (0..4).map(move |i| if i <= c.len() { A[(n >> (18 - 6 * i)) as usize & 63] as char } else { '=' })
                })
                .collect()
        };
        let xml = format!("<plist><dict><key>blkx</key><array><dict><key>Data</key><data>{}</data></dict></array></dict></plist>", b64(&mish));
        let xml_at = data.len() as u64;
        data.extend(xml.as_bytes());
        let mut koly = vec![0u8; 512];
        koly[..8].copy_from_slice(b"koly\0\0\0\x04");
        koly[216..224].copy_from_slice(&xml_at.to_be_bytes());
        koly[224..232].copy_from_slice(&(xml.len() as u64).to_be_bytes());
        data.extend(koly);
        // Max's search limit (the probe runs at Max and Ultra).
        let e = encode(&data, DeflateKind::Dmg, 4096, 128, false, None).unwrap();
        assert!(e.stored[0] & REPLAY != 0);
        assert!(e.stored.len() > 2_000_000, "all three chunks expanded");
        assert_eq!(decode(&e.stored, data.len() as u64).unwrap(), data);
    }

    #[test]
    fn a_wrong_zlib_setting_never_passes() {
        let data = png(1);
        let mut stored = encode(&data, DeflateKind::Png, 4096, 128, false, None).unwrap().stored;
        // Version, one stream, no corrections, the rows' length; then the method.
        let mut w = Writer::new();
        w.uv(1);
        w.uv(0);
        w.uv(200 * 901);
        let at = 1 + w.buf.len();
        assert_eq!(&stored[at..at + 2], &[1, 1], "method 1 (zlib), level 1");
        stored[at + 1] = 2;
        // Extraction checks every file's hash; here the stream must not come back.
        assert!(decode(&stored, data.len() as u64).map_or(true, |back| back != data));
    }
}
