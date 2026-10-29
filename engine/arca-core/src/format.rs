//! On-disk layout of an `.arca` archive.
//!
//! ```text
//! +----------------------+  HEADER_LEN bytes: magic, version, flags
//! | header               |
//! +----------------------+
//! | block 0              |  independently compressed solid blocks, written in
//! | block 1              |  completion order (the block table records offsets)
//! | ...                  |
//! +----------------------+
//! | index (zstd)         |  entries, objects, chunks, block table
//! +----------------------+
//! | footer               |  FOOTER_LEN bytes: index location + checksum, magic
//! +----------------------+
//! ```
//!
//! Content model:
//! * an **entry** is a path in the archive (file, dir, symlink, hard link);
//! * a file entry points at an **object** — unique content, stored once no matter
//!   how many entries share it (whole-file dedup);
//! * an object was reversibly **transformed** (e.g. JPEG → Lepton) into a stored
//!   stream, which is cut into content-defined **chunks**;
//! * chunks are deduplicated globally and packed into **blocks**, each compressed
//!   with its own codec.
//!
//! Integrity: every block carries BLAKE3-128 of its compressed and of its decoded
//! bytes; transformed objects also carry BLAKE3-128 of their original content.

use crate::wire::{Reader, Writer};
use anyhow::{bail, ensure, Result};

pub const MAGIC: &[u8; 8] = b"ARCA\x00\r\n\x1a";
pub const FOOTER_MAGIC: &[u8; 8] = b"ARCAEND\x00";
/// Version 2 stores the index's entries column by column (front-coded paths,
/// delta-coded times and objects). Version 1 archives still open.
pub const VERSION: u16 = 2;
pub const HEADER_LEN: usize = 32;
pub const FOOTER_LEN: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    Store,
    Zstd,
    Xz,
    /// PPMd var. H; the payload starts with `order: u8, mem_size: u32le`.
    Ppmd,
    /// Raw LZMA2 from the LZMA SDK; the payload starts with its dictionary
    /// property byte. (Xz blocks carry an xz container and a CRC-64 that the
    /// block's own BLAKE3 hashes make redundant.)
    Lzma2,
    /// One libbsc block (BWT via libsais, then QLFC); its own header carries
    /// everything the decoder needs.
    Bsc,
}

impl Codec {
    fn tag(self) -> u8 {
        match self {
            Codec::Store => 0,
            Codec::Zstd => 1,
            Codec::Xz => 2,
            Codec::Ppmd => 3,
            Codec::Lzma2 => 4,
            Codec::Bsc => 5,
        }
    }

    fn from_tag(t: u8) -> Result<Self> {
        Ok(match t {
            0 => Codec::Store,
            1 => Codec::Zstd,
            2 => Codec::Xz,
            3 => Codec::Ppmd,
            4 => Codec::Lzma2,
            5 => Codec::Bsc,
            _ => bail!("unknown codec {t}"),
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Codec::Store => "store",
            Codec::Zstd => "zstd",
            Codec::Xz => "xz",
            Codec::Ppmd => "ppmd",
            Codec::Lzma2 => "lzma2",
            Codec::Bsc => "bsc",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transform {
    /// Stored as-is.
    None,
    /// Mach-O branch filters applied in place to these (offset, len) ranges:
    /// AArch64 BL/ADRP on `ranges`, x86 CALL/JMP (XZ's BCJ) on `x86`.
    Arm64 { ranges: Vec<(u64, u64)>, x86: Vec<(u64, u64)> },
    /// JPEG losslessly re-encoded with Lepton.
    Lepton,
    /// Deflate streams expanded with preflate (ZIP/Office/PNG/PDF/gzip).
    Deflate,
}

impl Transform {
    pub fn name(&self) -> &'static str {
        match self {
            Transform::None => "none",
            Transform::Arm64 { .. } => "arm64",
            Transform::Lepton => "lepton",
            Transform::Deflate => "deflate",
        }
    }
}

#[derive(Debug, Clone)]
pub enum EntryKind {
    File { size: u64, object: u32 },
    Dir,
    Symlink { target: Vec<u8> },
    HardLink { target: u32 },
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub path: String,
    pub kind: EntryKind,
    pub mode: u32,
    pub mtime: (i64, u32),
    pub xattrs: Vec<(String, Vec<u8>)>,
}

#[derive(Debug, Clone)]
pub struct Object {
    /// BLAKE3-128 of the original content. Only transformed objects carry one:
    /// plain objects are covered by their blocks' content hashes.
    pub hash: Option<[u8; 16]>,
    pub size: u64,
    pub transform: Transform,
    pub stored_len: u64,
    pub chunks: Vec<u32>,
}

#[derive(Debug, Clone, Copy)]
pub struct Chunk {
    pub block: u32,
    pub offset: u64,
    pub len: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct BlockInfo {
    pub file_offset: u64,
    pub comp_len: u64,
    pub raw_len: u64,
    pub codec: Codec,
    /// BLAKE3-128 of the compressed bytes (checked before decoding).
    pub checksum: [u8; 16],
    /// BLAKE3-128 of the decoded bytes (checked after decoding).
    pub raw_hash: [u8; 16],
}

#[derive(Debug, Clone, Default)]
pub struct Index {
    pub created: i64,
    pub entries: Vec<Entry>,
    pub objects: Vec<Object>,
    pub chunks: Vec<Chunk>,
    pub blocks: Vec<BlockInfo>,
}

pub fn header_bytes(flags: u16) -> [u8; HEADER_LEN] {
    let mut h = [0u8; HEADER_LEN];
    h[..8].copy_from_slice(MAGIC);
    h[8..10].copy_from_slice(&VERSION.to_le_bytes());
    h[10..12].copy_from_slice(&flags.to_le_bytes());
    h
}

pub fn check_header(h: &[u8]) -> Result<u16> {
    ensure!(h.len() >= HEADER_LEN && &h[..8] == MAGIC, "not an Arca archive");
    let version = u16::from_le_bytes([h[8], h[9]]);
    ensure!(version == 1 || version == VERSION, "unsupported archive version {version}");
    Ok(u16::from_le_bytes([h[10], h[11]]))
}

#[derive(Debug, Clone, Copy)]
pub struct Footer {
    pub index_offset: u64,
    pub index_comp_len: u64,
    pub index_raw_len: u64,
    pub index_hash: [u8; 16],
}

impl Footer {
    pub fn to_bytes(&self) -> [u8; FOOTER_LEN] {
        let mut f = [0u8; FOOTER_LEN];
        f[0..8].copy_from_slice(&self.index_offset.to_le_bytes());
        f[8..16].copy_from_slice(&self.index_comp_len.to_le_bytes());
        f[16..24].copy_from_slice(&self.index_raw_len.to_le_bytes());
        f[24..40].copy_from_slice(&self.index_hash);
        f[56..64].copy_from_slice(FOOTER_MAGIC);
        f
    }

    pub fn parse(f: &[u8]) -> Result<Self> {
        ensure!(f.len() == FOOTER_LEN && &f[56..64] == FOOTER_MAGIC, "archive footer missing (truncated archive?)");
        let u = |r: std::ops::Range<usize>| u64::from_le_bytes(f[r].try_into().unwrap());
        Ok(Footer {
            index_offset: u(0..8),
            index_comp_len: u(8..16),
            index_raw_len: u(16..24),
            index_hash: f[24..40].try_into().unwrap(),
        })
    }
}

pub fn hash16(data: &[u8]) -> [u8; 16] {
    blake3::hash(data).as_bytes()[..16].try_into().unwrap()
}

impl Index {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.uv(VERSION as u64);
        w.iv(self.created);

        // Entries column by column, so zstd sees like next to like: paths
        // front-coded against the previous one (a folder walk shares long
        // prefixes), times and object numbers as differences.
        let es = &self.entries;
        w.uv(es.len() as u64);
        let mut prev: &[u8] = &[];
        for e in es {
            let p = e.path.as_bytes();
            let shared = p.iter().zip(prev).take_while(|(a, b)| a == b).count();
            w.uv(shared as u64);
            w.bytes(&p[shared..]);
            prev = p;
        }
        for e in es {
            w.u8(match e.kind {
                EntryKind::File { .. } => 0,
                EntryKind::Dir => 1,
                EntryKind::Symlink { .. } => 2,
                EntryKind::HardLink { .. } => 3,
            });
        }
        let mut prev_object = 0i64;
        for e in es {
            if let EntryKind::File { size, object } = e.kind {
                w.uv(size);
                w.iv(object as i64 - prev_object);
                prev_object = object as i64;
            }
        }
        for e in es {
            match &e.kind {
                EntryKind::Symlink { target } => w.bytes(target),
                EntryKind::HardLink { target } => w.uv(*target as u64),
                _ => {}
            }
        }
        for e in es {
            w.uv(e.mode as u64);
        }
        let mut prev_time = 0i64;
        for e in es {
            w.iv(e.mtime.0 - prev_time);
            prev_time = e.mtime.0;
        }
        for e in es {
            w.uv(e.mtime.1 as u64);
        }
        for e in es {
            w.uv(e.xattrs.len() as u64);
            for (k, v) in &e.xattrs {
                w.str(k);
                w.bytes(v);
            }
        }

        w.uv(self.objects.len() as u64);
        for o in &self.objects {
            w.uv(o.size);
            match &o.transform {
                Transform::None => w.u8(0),
                Transform::Arm64 { ranges, x86 } => {
                    // Tag 4 adds the x86 ranges; arm64-only files keep tag 1.
                    w.u8(if x86.is_empty() { 1 } else { 4 });
                    let lists = [ranges, x86];
                    for list in &lists[..if x86.is_empty() { 1 } else { 2 }] {
                        w.uv(list.len() as u64);
                        for &(off, len) in list.iter() {
                            w.uv(off);
                            w.uv(len);
                        }
                    }
                }
                Transform::Lepton => w.u8(2),
                Transform::Deflate => w.u8(3),
            }
            if o.transform != Transform::None {
                w.raw(&o.hash.expect("transformed objects carry a hash"));
            }
            w.uv(o.stored_len);
            w.uv(o.chunks.len() as u64);
            let mut prev = 0i64;
            for &c in &o.chunks {
                w.iv(c as i64 - prev);
                prev = c as i64;
            }
        }

        w.uv(self.chunks.len() as u64);
        let (mut pb, mut pend) = (0u32, 0u64);
        for c in &self.chunks {
            // Chunks are usually contiguous within a block: encode the block delta and
            // the gap from the previous chunk's end, both almost always zero.
            w.iv(c.block as i64 - pb as i64);
            let base = if c.block == pb { pend } else { 0 };
            w.iv(c.offset as i64 - base as i64);
            w.uv(c.len);
            pb = c.block;
            pend = c.offset + c.len;
        }

        w.uv(self.blocks.len() as u64);
        for b in &self.blocks {
            w.uv(b.file_offset);
            w.uv(b.comp_len);
            w.uv(b.raw_len);
            w.u8(b.codec.tag());
            w.raw(&b.checksum);
            w.raw(&b.raw_hash);
        }
        w.buf
    }

    fn entries_v1(r: &mut Reader) -> Result<Vec<Entry>> {
        let n = r.len()?;
        let mut entries = Vec::with_capacity(n.min(1 << 20));
        for _ in 0..n {
            let path = r.str()?;
            let kind = match r.u8()? {
                0 => EntryKind::File { size: r.uv()?, object: r.uv()? as u32 },
                1 => EntryKind::Dir,
                2 => EntryKind::Symlink { target: r.bytes()?.to_vec() },
                3 => EntryKind::HardLink { target: r.uv()? as u32 },
                t => bail!("unknown entry kind {t}"),
            };
            let mode = r.uv()? as u32;
            let mtime = (r.iv()?, r.uv()? as u32);
            let nx = r.len()?;
            let mut xattrs = Vec::with_capacity(nx.min(64));
            for _ in 0..nx {
                xattrs.push((r.str()?, r.bytes()?.to_vec()));
            }
            entries.push(Entry { path, kind, mode, mtime, xattrs });
        }
        Ok(entries)
    }

    fn entries_v2(r: &mut Reader) -> Result<Vec<Entry>> {
        let n = r.len()?;
        let mut paths: Vec<String> = Vec::with_capacity(n.min(1 << 20));
        let mut prev: Vec<u8> = Vec::new();
        for _ in 0..n {
            let shared = r.len()?;
            ensure!(shared <= prev.len(), "bad path prefix in index");
            prev.truncate(shared);
            prev.extend_from_slice(r.bytes()?);
            paths.push(String::from_utf8(prev.clone()).map_err(|_| anyhow::anyhow!("path in index is not UTF-8"))?);
        }
        let kinds: Vec<u8> = (0..n).map(|_| r.u8()).collect::<Result<_>>()?;
        let mut entries: Vec<Entry> = Vec::with_capacity(n.min(1 << 20));
        let mut prev_object = 0i64;
        for (path, &k) in paths.into_iter().zip(&kinds) {
            let kind = match k {
                0 => {
                    let size = r.uv()?;
                    prev_object += r.iv()?;
                    EntryKind::File { size, object: u32::try_from(prev_object)? }
                }
                1 => EntryKind::Dir,
                2 => EntryKind::Symlink { target: Vec::new() },
                3 => EntryKind::HardLink { target: 0 },
                t => bail!("unknown entry kind {t}"),
            };
            entries.push(Entry { path, kind, mode: 0, mtime: (0, 0), xattrs: Vec::new() });
        }
        for e in &mut entries {
            match &mut e.kind {
                EntryKind::Symlink { target } => *target = r.bytes()?.to_vec(),
                EntryKind::HardLink { target } => *target = r.uv()? as u32,
                _ => {}
            }
        }
        for e in &mut entries {
            e.mode = r.uv()? as u32;
        }
        let mut prev_time = 0i64;
        for e in &mut entries {
            prev_time = prev_time.checked_add(r.iv()?).ok_or_else(|| anyhow::anyhow!("bad time in index"))?;
            e.mtime.0 = prev_time;
        }
        for e in &mut entries {
            e.mtime.1 = r.uv()? as u32;
        }
        for e in &mut entries {
            let nx = r.len()?;
            e.xattrs.reserve(nx.min(64));
            for _ in 0..nx {
                e.xattrs.push((r.str()?, r.bytes()?.to_vec()));
            }
        }
        Ok(entries)
    }

    pub fn decode(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let version = r.uv()?;
        ensure!(version == 1 || version == VERSION as u64, "unsupported index version {version}");
        let created = r.iv()?;
        let entries = if version == 1 { Self::entries_v1(&mut r)? } else { Self::entries_v2(&mut r)? };

        let n = r.len()?;
        let mut objects = Vec::with_capacity(n.min(1 << 20));
        for _ in 0..n {
            let size = r.uv()?;
            let transform = match r.u8()? {
                0 => Transform::None,
                t @ (1 | 4) => {
                    let mut list = || -> Result<Vec<(u64, u64)>> {
                        let nr = r.len()?;
                        let mut ranges = Vec::with_capacity(nr.min(64));
                        for _ in 0..nr {
                            ranges.push((r.uv()?, r.uv()?));
                        }
                        Ok(ranges)
                    };
                    let ranges = list()?;
                    let x86 = if t == 4 { list()? } else { Vec::new() };
                    Transform::Arm64 { ranges, x86 }
                }
                2 => Transform::Lepton,
                3 => Transform::Deflate,
                t => bail!("unknown transform {t}"),
            };
            let hash = if transform == Transform::None { None } else { Some(r.array::<16>()?) };
            let stored_len = r.uv()?;
            let nc = r.len()?;
            let mut chunks = Vec::with_capacity(nc.min(1 << 16));
            let mut prev = 0i64;
            for _ in 0..nc {
                prev += r.iv()?;
                chunks.push(u32::try_from(prev)?);
            }
            objects.push(Object { hash, size, transform, stored_len, chunks });
        }

        let n = r.len()?;
        let mut chunks = Vec::with_capacity(n.min(1 << 20));
        let (mut pb, mut pend) = (0u32, 0u64);
        for _ in 0..n {
            let block = u32::try_from(pb as i64 + r.iv()?)?;
            let base = if block == pb { pend } else { 0 };
            let offset = u64::try_from(base as i64 + r.iv()?)?;
            let len = r.uv()?;
            chunks.push(Chunk { block, offset, len });
            pb = block;
            pend = offset + len;
        }

        let n = r.len()?;
        let mut blocks = Vec::with_capacity(n.min(1 << 20));
        for _ in 0..n {
            blocks.push(BlockInfo {
                file_offset: r.uv()?,
                comp_len: r.uv()?,
                raw_len: r.uv()?,
                codec: Codec::from_tag(r.u8()?)?,
                checksum: r.array::<16>()?,
                raw_hash: r.array::<16>()?,
            });
        }

        let idx = Index { created, entries, objects, chunks, blocks };
        idx.validate()?;
        Ok(idx)
    }

    /// Approximate compressed bytes attributable to each object, by object index.
    ///
    /// Blocks are compressed as a whole, so an object's share has to be estimated.
    /// Splitting a block by raw length alone is badly wrong where different kinds
    /// of data meet in one block: unpacked PNG pixels compress far better than app
    /// code or PDF text, so the images would be blamed for the rest. Files are
    /// packed grouped by type, so most blocks hold one kind of data (transform and
    /// file type, within a codec), and those blocks give that kind's ratio. In a
    /// block where kinds meet, the kinds with a known ratio take their share at
    /// it, and what's left goes to the others by length, weighed by how sure each
    /// side is: a few bytes of another kind beside a big file don't absorb the
    /// noise in that file's ratio, they compress like the block. Unpacked Deflate files
    /// are also weighed by their original size, which tells a smooth screenshot
    /// from a detailed icon even when both are PNGs. Every block is accounted for
    /// exactly. Shared (deduplicated) chunks count fully for every object using them.
    pub fn packed_estimates(&self) -> Vec<u64> {
        // A file's type is its extension, from the first file that uses the object.
        let mut extension = vec![None::<String>; self.objects.len()];
        for e in &self.entries {
            if let EntryKind::File { object, .. } = e.kind {
                let ext = &mut extension[object as usize];
                if ext.is_none() {
                    let name = e.path.rsplit('/').next().unwrap_or(&e.path);
                    *ext = Some(name.rsplit_once('.').map(|(_, x)| x.to_ascii_lowercase()).unwrap_or_default());
                }
            }
        }
        // The kind and object of each chunk, from the first object that uses it.
        let mut chunk_kind = vec![usize::MAX; self.chunks.len()];
        let mut chunk_object = vec![usize::MAX; self.chunks.len()];
        let mut kinds: std::collections::HashMap<(u8, u8, &str), usize> = std::collections::HashMap::new();
        for (i, o) in self.objects.iter().enumerate() {
            let transform = match o.transform {
                Transform::None => 0,
                Transform::Arm64 { .. } => 1,
                Transform::Lepton => 2,
                Transform::Deflate => 3,
            };
            let ext = extension[i].as_deref().unwrap_or("");
            for &c in &o.chunks {
                let c = c as usize;
                if chunk_kind[c] == usize::MAX {
                    let key = (self.blocks[self.chunks[c].block as usize].codec.tag(), transform, ext);
                    let next = kinds.len();
                    chunk_kind[c] = *kinds.entry(key).or_insert(next);
                    chunk_object[c] = i;
                }
            }
        }

        // Ratios from blocks that one kind (nearly) fills.
        let mut fill: Vec<Vec<(usize, f64)>> = vec![Vec::new(); self.blocks.len()];
        let mut members: Vec<Vec<usize>> = vec![Vec::new(); self.blocks.len()];
        for (c, ch) in self.chunks.iter().enumerate() {
            let kind = chunk_kind[c];
            if kind == usize::MAX {
                continue;
            }
            let b = ch.block as usize;
            members[b].push(c);
            match fill[b].iter_mut().find(|(k, _)| *k == kind) {
                Some((_, raw)) => *raw += ch.len as f64,
                None => fill[b].push((kind, ch.len as f64)),
            }
        }
        // How well an unpacked Deflate file's data compresses varies a lot even
        // within one type (a smooth screenshot against a detailed icon), and its
        // original Deflate size says by how much: the codecs here typically land
        // near 0.6 of it. Blocks one kind fills calibrate that per kind.
        let prior = |c: usize| -> Option<f64> {
            let o = &self.objects[chunk_object[c]];
            (matches!(o.transform, Transform::Deflate) && o.stored_len > 0)
                .then(|| 0.6 * o.size as f64 / o.stored_len as f64)
        };
        let mut seen = vec![(0f64, 0f64, 0f64); kinds.len()];
        for (b, (row, block)) in fill.iter().zip(&self.blocks).enumerate() {
            let total: f64 = row.iter().map(|(_, r)| r).sum();
            if let Some(&(kind, raw)) = row.iter().find(|(_, r)| *r >= 0.95 * total) {
                if total > 0.0 {
                    let comp = block.comp_len as f64 * raw / total;
                    let expected: f64 = members[b]
                        .iter()
                        .filter(|&&c| chunk_kind[c] == kind)
                        .map(|&c| prior(c).unwrap_or(0.0) * self.chunks[c].len as f64)
                        .sum();
                    seen[kind].0 += comp;
                    seen[kind].1 += raw;
                    seen[kind].2 += expected;
                }
            }
        }
        // A chunk's ratio: from its kind's own blocks, scaled per file by the prior
        // where there is one; or the prior alone for a kind that never fills a block.
        let rate = |c: usize| -> Option<f64> {
            let (comp, raw, expected) = seen[chunk_kind[c]];
            match prior(c) {
                Some(p) if expected > 0.0 => Some(p * comp / expected),
                Some(p) => Some(p),
                None => (raw > 0.0).then(|| comp / raw),
            }
        };

        let mut share = vec![0f64; self.chunks.len()];
        for (b, block) in self.blocks.iter().enumerate() {
            let comp = block.comp_len as f64;
            let total: f64 = members[b].iter().map(|&c| self.chunks[c].len as f64).sum();
            if total <= 0.0 {
                continue;
            }
            let fixed: f64 = members[b].iter().filter_map(|&c| rate(c).map(|q| q * self.chunks[c].len as f64)).sum();
            let unknown: f64 = members[b].iter().filter(|&&c| rate(c).is_none()).map(|&c| self.chunks[c].len as f64).sum();
            // The rest, guessed at the block's own ratio, and what's left over
            // once both are counted. A learned ratio is good to about 2%, a guess
            // to about 50%, so the leftover goes to whichever side is less sure:
            // the rest, when it's a real part of the block; the known kinds, when
            // it's a few bytes beside them and the leftover is their noise.
            let guess = unknown * comp / total;
            let (sure, unsure) = ((0.02 * fixed).powi(2), (0.5 * guess).powi(2));
            let weight = if sure + unsure > 0.0 { unsure / (sure + unsure) } else { 0.0 };
            let mut rest = guess + (comp - fixed - guess) * weight;
            let mut known = comp - rest;
            if rest < 0.0 || known < 0.0 {
                // The ratios don't fit this block: scale everything to it.
                let scale = comp / (fixed + guess).max(f64::MIN_POSITIVE);
                rest = guess * scale;
                known = fixed * scale;
            }
            for &c in &members[b] {
                let len = self.chunks[c].len as f64;
                share[c] = match rate(c) {
                    Some(q) => q * len * known / fixed.max(f64::MIN_POSITIVE),
                    None => rest * len / unknown,
                };
            }
        }
        self.objects.iter().map(|o| o.chunks.iter().map(|&c| share[c as usize]).sum::<f64>().round() as u64).collect()
    }

    /// Structural checks so later stages can index without bounds surprises.
    fn validate(&self) -> Result<()> {
        for e in &self.entries {
            match e.kind {
                EntryKind::File { object, .. } => {
                    ensure!((object as usize) < self.objects.len(), "entry references missing object")
                }
                EntryKind::HardLink { target } => {
                    ensure!((target as usize) < self.entries.len(), "hard link to missing entry")
                }
                _ => {}
            }
        }
        for o in &self.objects {
            let mut total = 0u64;
            for &c in &o.chunks {
                let c = self.chunks.get(c as usize).ok_or_else(|| anyhow::anyhow!("missing chunk"))?;
                total += c.len;
            }
            ensure!(total == o.stored_len, "object chunk lengths do not add up");
        }
        for c in &self.chunks {
            let b = self.blocks.get(c.block as usize).ok_or_else(|| anyhow::anyhow!("chunk in missing block"))?;
            ensure!(c.offset + c.len <= b.raw_len, "chunk outside its block");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(transform: Transform, chunks: Vec<u32>, stored_len: u64) -> Object {
        Object { hash: None, size: stored_len, transform, stored_len, chunks }
    }

    fn block(raw_len: u64, comp_len: u64) -> BlockInfo {
        BlockInfo { file_offset: 0, comp_len, raw_len, codec: Codec::Zstd, checksum: [0; 16], raw_hash: [0; 16] }
    }

    fn file(path: &str, object: u32, size: u64) -> Entry {
        Entry { path: path.into(), kind: EntryKind::File { size, object }, mode: 0o644, mtime: (0, 0), xattrs: vec![] }
    }

    fn chunk(block: u32, offset: u64, len: u64) -> Chunk {
        Chunk { block, offset, len }
    }

    /// Branch-filter ranges come back from the index as written: arm64-only
    /// objects keep tag 1, objects with x86_64 code use tag 4.
    #[test]
    fn branch_filter_ranges_round_trip() {
        let hashed = |transform, chunks, len| Object { hash: Some([7; 16]), ..object(transform, chunks, len) };
        let both = Transform::Arm64 { ranges: vec![(16, 4096), (8192, 64)], x86: vec![(20_000, 3001)] };
        let arm_only = Transform::Arm64 { ranges: vec![(0, 400)], x86: vec![] };
        let idx = Index {
            created: 0,
            entries: vec![file("Universal", 0, 30_000), file("Arm", 1, 400)],
            objects: vec![hashed(both.clone(), vec![0], 30_000), hashed(arm_only.clone(), vec![1], 400)],
            chunks: vec![chunk(0, 0, 30_000), chunk(0, 30_000, 400)],
            blocks: vec![block(30_400, 9000)],
        };
        let back = Index::decode(&idx.encode()).unwrap();
        assert_eq!(back.objects[0].transform, both);
        assert_eq!(back.objects[1].transform, arm_only);
    }

    /// Block 0 holds only app code (compresses to half), block 1 only PNG pixels
    /// (to a tenth), block 2 a little of both. The mixed block must be split by
    /// how each kind compresses, not by length.
    #[test]
    fn mixed_blocks_are_split_by_how_each_kind_compresses() {
        let arm64 = || Transform::Arm64 { ranges: vec![], x86: vec![] };
        let idx = Index {
            created: 0,
            entries: vec![file("Tool", 0, 1000), file("a.png", 1, 1000), file("Tool2", 2, 500), file("b.png", 3, 500)],
            objects: vec![
                object(arm64(), vec![0], 1000),
                object(Transform::Deflate, vec![1], 1000),
                object(arm64(), vec![2], 500),
                object(Transform::Deflate, vec![3], 500),
            ],
            chunks: vec![chunk(0, 0, 1000), chunk(1, 0, 1000), chunk(2, 0, 500), chunk(2, 500, 500)],
            // The mixed block: 500 × 0.5 + 500 × 0.1 = 300.
            blocks: vec![block(1000, 500), block(1000, 100), block(1000, 300)],
        };
        let est = idx.packed_estimates();
        assert_eq!(&est[..2], &[500, 100], "single-kind blocks are exact");
        assert_eq!(&est[2..], &[250, 50], "the mixed block is split by each kind's ratio");
        assert_eq!(est.iter().sum::<u64>(), 900, "every compressed byte is accounted for once");
    }

    /// An asset catalog shares a block with PNG pixels and never fills one itself:
    /// it gets what the pixels, at their known ratio, leave over.
    #[test]
    fn a_kind_without_its_own_block_gets_what_is_left() {
        let idx = Index {
            created: 0,
            entries: vec![file("big.png", 0, 2000), file("Assets.car", 1, 400)],
            objects: vec![object(Transform::Deflate, vec![0, 1], 2000), object(Transform::None, vec![2], 400)],
            chunks: vec![chunk(0, 0, 1000), chunk(1, 0, 1000), chunk(1, 1000, 400)],
            // Pixels compress to 5%, the catalog to 50%: 1000 × 0.05 + 400 × 0.5 = 250.
            blocks: vec![block(1000, 50), block(1400, 250)],
        };
        assert_eq!(idx.packed_estimates(), vec![100, 200]);
    }

    /// A note of a few bytes shares a block with a big text file, whose ratio
    /// differs a little from block to block. What's left after the text's share
    /// is that difference, not the note: the note compresses like the block
    /// instead of coming out larger than it went in.
    #[test]
    fn a_few_bytes_beside_a_big_file_do_not_take_its_noise() {
        let idx = Index {
            created: 0,
            entries: vec![file("Word list.txt", 0, 2000), file("Read me.md", 1, 10)],
            objects: vec![object(Transform::None, vec![0, 1], 2000), object(Transform::None, vec![2], 10)],
            chunks: vec![chunk(0, 0, 1000), chunk(1, 0, 1000), chunk(1, 1000, 10)],
            // The text compresses to 30% in one block and 32% in the other.
            blocks: vec![block(1000, 300), block(1010, 320)],
        };
        let est = idx.packed_estimates();
        assert!(est[1] > 0 && est[1] < 10, "the note's estimate {} should be below its 10 bytes", est[1]);
        assert_eq!(est.iter().sum::<u64>(), 620, "every compressed byte is accounted for once");
    }

    /// Two kinds that never fill a block on their own: the unpacked PDF is judged
    /// by its original Deflate size, and the rest goes to the catalog beside it.
    #[test]
    fn unpacked_files_fall_back_to_their_original_size() {
        let mut pdf = object(Transform::Deflate, vec![0], 800);
        pdf.size = 300;
        let idx = Index {
            created: 0,
            entries: vec![file("notes.pdf", 0, 300), file("Assets.car", 1, 400)],
            objects: vec![pdf, object(Transform::None, vec![1], 400)],
            chunks: vec![chunk(0, 0, 800), chunk(0, 800, 400)],
            // 0.6 × 300 = 180 for the PDF, the other 200 for the catalog.
            blocks: vec![block(1200, 380)],
        };
        assert_eq!(idx.packed_estimates(), vec![180, 200]);
    }
}
