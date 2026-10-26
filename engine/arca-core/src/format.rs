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
pub const VERSION: u16 = 1;
pub const HEADER_LEN: usize = 32;
pub const FOOTER_LEN: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    Store,
    Zstd,
    Xz,
    /// PPMd var. H; the payload starts with `order: u8, mem_size: u32le`.
    Ppmd,
}

impl Codec {
    fn tag(self) -> u8 {
        match self {
            Codec::Store => 0,
            Codec::Zstd => 1,
            Codec::Xz => 2,
            Codec::Ppmd => 3,
        }
    }

    fn from_tag(t: u8) -> Result<Self> {
        Ok(match t {
            0 => Codec::Store,
            1 => Codec::Zstd,
            2 => Codec::Xz,
            3 => Codec::Ppmd,
            _ => bail!("unknown codec {t}"),
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Codec::Store => "store",
            Codec::Zstd => "zstd",
            Codec::Xz => "xz",
            Codec::Ppmd => "ppmd",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transform {
    /// Stored as-is.
    None,
    /// AArch64 branch/ADRP filter applied in place to these (offset, len) ranges.
    Arm64 { ranges: Vec<(u64, u64)> },
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
    ensure!(version == VERSION, "unsupported archive version {version}");
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

        w.uv(self.entries.len() as u64);
        for e in &self.entries {
            w.str(&e.path);
            match &e.kind {
                EntryKind::File { size, object } => {
                    w.u8(0);
                    w.uv(*size);
                    w.uv(*object as u64);
                }
                EntryKind::Dir => w.u8(1),
                EntryKind::Symlink { target } => {
                    w.u8(2);
                    w.bytes(target);
                }
                EntryKind::HardLink { target } => {
                    w.u8(3);
                    w.uv(*target as u64);
                }
            }
            w.uv(e.mode as u64);
            w.iv(e.mtime.0);
            w.uv(e.mtime.1 as u64);
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
                Transform::Arm64 { ranges } => {
                    w.u8(1);
                    w.uv(ranges.len() as u64);
                    for &(off, len) in ranges {
                        w.uv(off);
                        w.uv(len);
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

    pub fn decode(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let version = r.uv()?;
        ensure!(version == VERSION as u64, "unsupported index version {version}");
        let created = r.iv()?;

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

        let n = r.len()?;
        let mut objects = Vec::with_capacity(n.min(1 << 20));
        for _ in 0..n {
            let size = r.uv()?;
            let transform = match r.u8()? {
                0 => Transform::None,
                1 => {
                    let nr = r.len()?;
                    let mut ranges = Vec::with_capacity(nr.min(64));
                    for _ in 0..nr {
                        ranges.push((r.uv()?, r.uv()?));
                    }
                    Transform::Arm64 { ranges }
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

    /// Approximate compressed bytes attributable to an object: each chunk's share
    /// of its block's compressed size. Shared (deduplicated) chunks count fully
    /// for every object that uses them.
    pub fn packed_estimate(&self, object: u32) -> u64 {
        let o = &self.objects[object as usize];
        o.chunks
            .iter()
            .map(|&c| {
                let ch = &self.chunks[c as usize];
                let b = &self.blocks[ch.block as usize];
                (ch.len as u128 * b.comp_len as u128 / b.raw_len.max(1) as u128) as u64
            })
            .sum()
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
