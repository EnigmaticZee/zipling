//! The recovery record: Reed-Solomon parity inside the archive, so a damaged
//! `.zlg` can repair itself.
//!
//! ```text
//! header | blocks | record | index | footer
//!                  ^ parity over [0, record)      ^ copies of both in the record
//! ```
//!
//! The record sits between the data blocks and the index. It holds a copy of the
//! index and of the footer, a hash of every slice, and Reed-Solomon parity over
//! the bytes before it, computed region by region (`REGION` bytes, each its own
//! code) in slices of `slice` bytes. A record of X% repairs any X% of damage in
//! each region, wherever it falls; a truncated tail that took the index and the
//! footer is rebuilt from the copies and the parity regenerated from the intact
//! data. Damage is located by the slice hashes, which need no key. Repair writes
//! a corrected copy; the damaged file is never changed.
//!
//! Byte layout of the record (all integers little-endian):
//!
//! ```text
//! MAGIC[8] version u16 flags u16 percent u16 reserved u16
//! data_len u64  slice u32  regions u32
//! (k u32, m u32) per region
//! footer_copy[FOOTER_LEN]
//! index_copy_len u64, index_copy (the compressed index as written)
//! slice hashes: per region k data hashes then m parity hashes, 16 bytes each
//! anchors: the first 8 bytes of every data slice, so a slice that moved (bytes
//!          inserted or dropped before it) is found again by searching for them
//! header_hash[16]  (BLAKE3-128 of everything above)
//! parity: per region m slices of `slice` bytes
//! the header again, byte for byte (so one hit cannot take the only copy)
//! record_len u64  header_len u64  TRAILER_MAGIC[8]
//! ```

use crate::format::{self, Footer, FOOTER_LEN, HEADER_LEN};
use crate::hooks::Hooks;
use anyhow::{ensure, Context, Result};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::os::unix::fs::FileExt;
use std::path::Path;

pub const MAGIC: &[u8; 8] = b"ARCARECV";
pub const TRAILER_MAGIC: &[u8; 8] = b"ARCARECE";
const RECORD_VERSION: u16 = 2;
/// Bytes of archive one Reed-Solomon code covers. Repair of a region needs its
/// intact slices in memory, so this bounds the memory a repair takes.
pub const REGION: u64 = 128 << 20;
/// Data slices per full region: 64 KiB slices, so one bad sector costs 64 KiB of
/// the budget, not megabytes.
const SLICES_PER_REGION: u64 = 2048;
const MIN_SLICE: u64 = 4096;
pub const MIN_PERCENT: u16 = 1;
pub const MAX_PERCENT: u16 = 50;
const FIXED_HEADER: usize = 32;
const TRAILER: usize = 24;

/// What a record protects and how, as read from the archive.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RecordInfo {
    pub percent: u16,
    /// Bytes of archive the parity covers: the header and every data block.
    pub covered_bytes: u64,
    pub parity_bytes: u64,
    pub slice: u32,
    pub regions: u32,
    /// Damage repairable in any one region, in bytes (the smallest region's parity).
    pub repairs_up_to: u64,
}

/// The record's header, parsed.
#[derive(Debug, Clone)]
struct Record {
    percent: u16,
    data_len: u64,
    slice: u32,
    regions: Vec<(u32, u32)>,
    footer_copy: [u8; FOOTER_LEN],
    index_copy: Vec<u8>,
    hashes: Vec<[u8; 16]>,
    /// The first 8 bytes of every data slice, region after region.
    anchors: Vec<[u8; 8]>,
    /// The header as written (both copies are this), for putting it back.
    head: Vec<u8>,
    /// Where the parity starts in the file as it is, and the record's whole length.
    parity_offset: u64,
    len: u64,
}

impl Record {
    fn info(&self) -> RecordInfo {
        let parity: u64 = self.regions.iter().map(|&(_, m)| m as u64 * self.slice as u64).sum();
        let least = self.regions.iter().map(|&(_, m)| m as u64 * self.slice as u64).min().unwrap_or(0);
        RecordInfo {
            percent: self.percent,
            covered_bytes: self.data_len,
            parity_bytes: parity,
            slice: self.slice,
            regions: self.regions.len() as u32,
            repairs_up_to: least,
        }
    }

    fn region_len(&self, r: usize) -> u64 {
        (self.data_len - r as u64 * REGION).min(REGION)
    }

    /// First hash index of a region's slices (data, then parity).
    fn hash_base(&self, r: usize) -> usize {
        self.regions[..r].iter().map(|&(k, m)| (k + m) as usize).sum()
    }

    /// First anchor index of a region's data slices.
    fn anchor_base(&self, r: usize) -> usize {
        self.regions[..r].iter().map(|&(k, _)| k as usize).sum()
    }

    fn parity_base(&self, r: usize) -> u64 {
        self.parity_offset + self.regions[..r].iter().map(|&(_, m)| m as u64 * self.slice as u64).sum::<u64>()
    }
}

/// Slice size and (k, m) per region for `data_len` bytes at `percent`.
fn layout(data_len: u64, percent: u16) -> (u32, Vec<(u32, u32)>) {
    let full = data_len.min(REGION);
    let slice = (full.div_ceil(SLICES_PER_REGION)).max(MIN_SLICE).div_ceil(64) * 64;
    let n = data_len.div_ceil(REGION).max(1);
    let regions = (0..n)
        .map(|r| {
            let len = (data_len - r * REGION).min(REGION).max(1);
            let k = len.div_ceil(slice) as u32;
            let m = ((k as u64 * percent as u64).div_ceil(100) as u32).max(1);
            (k, m)
        })
        .collect();
    (slice as u32, regions)
}

fn header_len(regions: usize, index_copy: usize, hashes: usize, data_slices: usize) -> usize {
    FIXED_HEADER + 8 * regions + FOOTER_LEN + 8 + index_copy + 16 * hashes + 8 * data_slices + 16
}

fn anchor_of(shard: &[u8]) -> [u8; 8] {
    shard[..8].try_into().unwrap()
}

/// Reads one slice of the file at `at`, zero-padded past `limit` (the end of
/// its region) and past the end of the file. Returns None when nothing of it is
/// in the file at all (a truncated tail).
fn read_slice(file: &File, file_len: u64, at: u64, slice: usize, limit: u64) -> Option<Vec<u8>> {
    if at >= file_len {
        return None;
    }
    let want = (limit.saturating_sub(at)).min(slice as u64) as usize;
    let have = (file_len - at).min(want as u64) as usize;
    let mut buf = vec![0u8; slice];
    if have > 0 && file.read_exact_at(&mut buf[..have], at).is_err() {
        return None;
    }
    if have < want {
        // The tail of the slice is missing: not what was written.
        return None;
    }
    Some(buf)
}

/// Writes the archive's tail: the record (when `percent` is not 0), the index
/// and the footer, and returns the footer. `out` holds `data_len` bytes of
/// header and blocks and must be open for reading too.
pub fn write_tail(out: &mut File, data_len: u64, index_comp: &[u8], index_raw_len: u64, percent: u16, hooks: &Hooks) -> Result<Footer> {
    let index_hash = format::hash16(index_comp);
    if percent == 0 {
        out.write_all(index_comp)?;
        let footer = Footer { index_offset: data_len, index_comp_len: index_comp.len() as u64, index_raw_len, index_hash, recovery_offset: 0, recovery_len: 0 };
        out.write_all(&footer.to_bytes())?;
        return Ok(footer);
    }
    ensure!((MIN_PERCENT..=MAX_PERCENT).contains(&percent), "recovery record of {percent}%: choose 1 to 50");
    let (slice, regions) = layout(data_len, percent);
    let hashes_n: usize = regions.iter().map(|&(k, m)| (k + m) as usize).sum();
    let data_n: usize = regions.iter().map(|&(k, _)| k as usize).sum();
    let head_len = header_len(regions.len(), index_comp.len(), hashes_n, data_n);
    let parity_len: u64 = regions.iter().map(|&(_, m)| m as u64 * slice as u64).sum();
    let record_len = 2 * head_len as u64 + parity_len + TRAILER as u64;
    let footer = Footer {
        index_offset: data_len + record_len,
        index_comp_len: index_comp.len() as u64,
        index_raw_len,
        index_hash,
        recovery_offset: data_len,
        recovery_len: record_len,
    };

    // Parity and hashes, region by region, reading the data back from the file.
    let mut hashes: Vec<[u8; 16]> = Vec::with_capacity(hashes_n);
    let mut anchors: Vec<[u8; 8]> = Vec::with_capacity(data_n);
    let mut parity: Vec<u8> = Vec::with_capacity(parity_len as usize);
    for (r, &(k, m)) in regions.iter().enumerate() {
        hooks.check()?;
        let start = r as u64 * REGION;
        let len = (data_len - start).min(REGION) as usize;
        let mut buf = vec![0u8; k as usize * slice as usize];
        out.read_exact_at(&mut buf[..len], start).context("reading the archive back for its recovery record")?;
        let shards: Vec<&[u8]> = buf.chunks(slice as usize).collect();
        for s in &shards {
            hashes.push(format::hash16(s));
            anchors.push(anchor_of(s));
        }
        let rec = reed_solomon_simd::encode(k as usize, m as usize, shards).map_err(|e| anyhow::anyhow!("recovery coding: {e}"))?;
        for s in &rec {
            hashes.push(format::hash16(s));
            parity.extend_from_slice(s);
        }
        hooks.report(start + len as u64, data_len);
    }

    let mut head = Vec::with_capacity(head_len);
    head.extend_from_slice(MAGIC);
    head.extend_from_slice(&RECORD_VERSION.to_le_bytes());
    head.extend_from_slice(&0u16.to_le_bytes());
    head.extend_from_slice(&percent.to_le_bytes());
    head.extend_from_slice(&0u16.to_le_bytes());
    head.extend_from_slice(&data_len.to_le_bytes());
    head.extend_from_slice(&slice.to_le_bytes());
    head.extend_from_slice(&(regions.len() as u32).to_le_bytes());
    for &(k, m) in &regions {
        head.extend_from_slice(&k.to_le_bytes());
        head.extend_from_slice(&m.to_le_bytes());
    }
    head.extend_from_slice(&footer.to_bytes());
    head.extend_from_slice(&(index_comp.len() as u64).to_le_bytes());
    head.extend_from_slice(index_comp);
    for h in &hashes {
        head.extend_from_slice(h);
    }
    for a in &anchors {
        head.extend_from_slice(a);
    }
    let hh = format::hash16(&head);
    head.extend_from_slice(&hh);
    debug_assert_eq!(head.len(), head_len);

    out.write_all(&head)?;
    out.write_all(&parity)?;
    out.write_all(&head)?;
    out.write_all(&record_len.to_le_bytes())?;
    out.write_all(&(head_len as u64).to_le_bytes())?;
    out.write_all(TRAILER_MAGIC)?;
    out.write_all(index_comp)?;
    out.write_all(&footer.to_bytes())?;
    Ok(footer)
}

/// Parses a record starting at `offset` from its first header copy.
fn parse(file: &File, file_len: u64, offset: u64) -> Result<Record> {
    parse_at(file, file_len, offset, offset)
}

/// Parses a record starting at `offset` from a header copy at `at` (the first
/// copy sits at `offset`, the second after the parity). Fails when that copy is
/// damaged (its hash does not match).
fn parse_at(file: &File, file_len: u64, at: u64, offset: u64) -> Result<Record> {
    ensure!(at + FIXED_HEADER as u64 <= file_len, "recovery record cut off");
    let mut fixed = [0u8; FIXED_HEADER];
    file.read_exact_at(&mut fixed, at)?;
    ensure!(&fixed[..8] == MAGIC, "no recovery record at that offset");
    let u16_at = |i: usize| u16::from_le_bytes([fixed[i], fixed[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes(fixed[i..i + 4].try_into().unwrap());
    let version = u16_at(8);
    ensure!(version == RECORD_VERSION, "recovery record version {version} is newer than this engine");
    let percent = u16_at(12);
    let data_len = u64::from_le_bytes(fixed[16..24].try_into().unwrap());
    let slice = u32_at(24);
    let n = u32_at(28) as usize;
    ensure!(slice >= MIN_SLICE as u32 && slice % 64 == 0 && n >= 1 && n <= 1 << 20, "recovery record header is damaged");
    let mut regs = vec![0u8; 8 * n];
    file.read_exact_at(&mut regs, at + FIXED_HEADER as u64)?;
    let regions: Vec<(u32, u32)> = regs.chunks(8).map(|c| (u32::from_le_bytes(c[..4].try_into().unwrap()), u32::from_le_bytes(c[4..].try_into().unwrap()))).collect();
    let hashes_n: usize = regions.iter().map(|&(k, m)| (k + m) as usize).sum();
    let mut pos = at + FIXED_HEADER as u64 + 8 * n as u64;
    let mut footer_copy = [0u8; FOOTER_LEN];
    file.read_exact_at(&mut footer_copy, pos)?;
    pos += FOOTER_LEN as u64;
    let mut l = [0u8; 8];
    file.read_exact_at(&mut l, pos)?;
    pos += 8;
    let index_len = u64::from_le_bytes(l);
    ensure!(index_len <= 4 << 30, "recovery record header is damaged");
    let data_n: usize = regions.iter().map(|&(k, _)| k as usize).sum();
    let head_len = header_len(n, index_len as usize, hashes_n, data_n);
    ensure!(at + head_len as u64 <= file_len, "recovery record cut off");
    let mut index_copy = vec![0u8; index_len as usize];
    file.read_exact_at(&mut index_copy, pos)?;
    pos += index_len;
    let mut hb = vec![0u8; 16 * hashes_n];
    file.read_exact_at(&mut hb, pos)?;
    pos += hb.len() as u64;
    let mut ab = vec![0u8; 8 * data_n];
    file.read_exact_at(&mut ab, pos)?;
    pos += ab.len() as u64;
    let mut stored = [0u8; 16];
    file.read_exact_at(&mut stored, pos)?;
    // The header hash covers everything before it.
    let mut whole = vec![0u8; head_len - 16];
    file.read_exact_at(&mut whole, at)?;
    ensure!(format::hash16(&whole) == stored, "recovery record header is damaged");
    whole.extend_from_slice(&stored);
    let hashes = hb.chunks(16).map(|c| c.try_into().unwrap()).collect();
    let anchors = ab.chunks(8).map(|c| c.try_into().unwrap()).collect();
    let parity_len: u64 = regions.iter().map(|&(_, m)| m as u64 * slice as u64).sum();
    Ok(Record {
        percent,
        data_len,
        slice,
        regions,
        footer_copy,
        index_copy,
        hashes,
        anchors,
        head: whole,
        parity_offset: offset + head_len as u64,
        len: 2 * head_len as u64 + parity_len + TRAILER as u64,
    })
}

/// Parses a record from the second header copy, given where its trailer's magic
/// sits: the trailer says how long the record and its header are.
fn parse_from_trailer(file: &File, file_len: u64, magic_at: u64) -> Result<Record> {
    ensure!(magic_at >= 16, "no room for a recovery trailer");
    let mut t = [0u8; 16];
    file.read_exact_at(&mut t, magic_at - 16)?;
    let record_len = u64::from_le_bytes(t[..8].try_into().unwrap());
    let head_len = u64::from_le_bytes(t[8..].try_into().unwrap());
    let end = magic_at + 8;
    ensure!(record_len <= end && head_len + TRAILER as u64 <= record_len, "recovery trailer is damaged");
    let offset = end - record_len;
    let second = end - TRAILER as u64 - head_len;
    parse_at(file, file_len, second, offset).or_else(|_| parse(file, file_len, offset))
}

/// Finds the record: through the footer when it is intact, else through the
/// trailer before the index, else by scanning the file for its magic.
fn locate(file: &File, file_len: u64) -> Result<Option<Record>> {
    if file_len >= (HEADER_LEN + FOOTER_LEN) as u64 {
        let mut foot = [0u8; FOOTER_LEN];
        file.read_exact_at(&mut foot, file_len - FOOTER_LEN as u64)?;
        if let Ok(f) = Footer::parse(&foot) {
            if f.recovery_offset == 0 {
                return Ok(None);
            }
            if let Ok(r) = parse(file, file_len, f.recovery_offset) {
                return Ok(Some(r));
            }
            if f.recovery_len > TRAILER as u64 {
                if let Ok(r) = parse_from_trailer(file, file_len, f.recovery_offset + f.recovery_len - 8) {
                    return Ok(Some(r));
                }
            }
        }
    }
    // No usable footer: the trailer sits just before the index, near the end, so a
    // backward scan for its magic finds the second header copy.
    {
        const TAIL: u64 = 64 << 20;
        let from = file_len.saturating_sub(TAIL);
        let mut buf = vec![0u8; (file_len - from) as usize];
        file.read_exact_at(&mut buf, from)?;
        let mut end = buf.len();
        while let Some(hit) = memchr::memmem::rfind(&buf[..end], TRAILER_MAGIC) {
            if let Ok(r) = parse_from_trailer(file, file_len, from + hit as u64) {
                return Ok(Some(r));
            }
            end = hit;
        }
    }
    // A forward scan for the magic: the record starts right after the blocks, and
    // its header hash tells a real record from a chance match.
    const WINDOW: usize = 4 << 20;
    let mut buf = vec![0u8; WINDOW + 8];
    let mut pos = 0u64;
    while pos < file_len {
        let n = (file_len - pos).min(buf.len() as u64) as usize;
        file.read_exact_at(&mut buf[..n], pos)?;
        for hit in memchr::memmem::find_iter(&buf[..n], MAGIC) {
            if let Ok(r) = parse(file, file_len, pos + hit as u64) {
                return Ok(Some(r));
            }
        }
        if n < buf.len() {
            break;
        }
        pos += WINDOW as u64;
    }
    Ok(None)
}

/// What a check found.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct CheckReport {
    /// None when the archive has no recovery record (or it could not be found).
    pub record: Option<RecordInfo>,
    pub file_len: u64,
    /// The length the archive should have, from the record's footer copy.
    pub expected_len: u64,
    pub footer_ok: bool,
    pub index_ok: bool,
    pub damaged_data_slices: u64,
    pub damaged_data_bytes: u64,
    pub damaged_parity_slices: u64,
    /// Regions with more damage than their parity can repair.
    pub unrepairable_regions: Vec<u32>,
    /// Whether everything damaged can be put right.
    pub repairable: bool,
    /// Files whose data lies in unrepairable damage, when the index is readable.
    pub lost_files: Vec<String>,
    /// Whether bytes were found inserted or dropped: slices after the fault were
    /// located at a shifted position and are read from there.
    pub realigned: bool,
}

struct RegionDamage {
    bad_data: Vec<usize>,
    bad_parity: Vec<usize>,
    /// Where each data slice's bytes sit in the file relative to where they should:
    /// 0 for a file that only has overwritten bytes, else the running shift.
    shifts: Vec<i64>,
}

struct Damage {
    regions: Vec<RegionDamage>,
}

/// How far around a slice's expected place its anchor is searched for when the
/// slice is not there: bytes inserted or dropped by a bad copy, up to a few slices.
fn search_window(slice: usize) -> i64 {
    (4 * slice as i64).max(64 << 10)
}

/// Looks for slice `t` of region `r` near where the current shift puts it, by its
/// 8-byte anchor, and confirms a hit with the slice's hash. Returns the new shift.
fn find_shift(file: &File, file_len: u64, rec: &Record, r: usize, t: usize, shift: i64, want: usize) -> Option<i64> {
    let slice = rec.slice as usize;
    let pos = (r as u64 * REGION + t as u64 * slice as u64) as i64;
    let w = search_window(slice);
    let lo = (pos + shift - w).max(0);
    let hi = ((pos + shift + w + slice as i64) as u64).min(file_len) as i64;
    if hi <= lo + 8 {
        return None;
    }
    let mut buf = vec![0u8; (hi - lo) as usize];
    file.read_exact_at(&mut buf, lo as u64).ok()?;
    let anchor = &rec.anchors[rec.anchor_base(r) + t];
    let hash = &rec.hashes[rec.hash_base(r) + t];
    for (n, hit) in memchr::memmem::find_iter(&buf, anchor).enumerate() {
        if n > 256 {
            break;
        }
        let s = lo + hit as i64 - pos;
        if s != shift && matches(file, file_len, pos + s, slice, want, hash) {
            return Some(s);
        }
    }
    None
}

/// A slice read from a position that may be shifted (and so negative or past the end).
fn read_slice_at(file: &File, file_len: u64, pos: i64, slice: usize, want: usize) -> Option<Vec<u8>> {
    if pos < 0 {
        return None;
    }
    read_slice(file, file_len, pos as u64, slice, pos as u64 + want as u64)
}

fn matches(file: &File, file_len: u64, pos: i64, slice: usize, want: usize, hash: &[u8; 16]) -> bool {
    read_slice_at(file, file_len, pos, slice, want).is_some_and(|s| format::hash16(&s) == *hash)
}

fn survey(file: &File, file_len: u64, rec: &Record, hooks: &Hooks) -> Result<(CheckReport, Damage)> {
    let slice = rec.slice as usize;
    let mut report = CheckReport { record: Some(rec.info()), file_len, ..Default::default() };
    let footer = Footer::parse(&rec.footer_copy).context("the record's footer copy is unreadable")?;
    report.expected_len = footer.index_offset + footer.index_comp_len + FOOTER_LEN as u64;
    report.footer_ok = file_len == report.expected_len && {
        let mut f = [0u8; FOOTER_LEN];
        file.read_exact_at(&mut f, file_len - FOOTER_LEN as u64).is_ok() && f == rec.footer_copy
    };
    report.index_ok = footer.index_offset + footer.index_comp_len <= file_len && {
        let mut ix = vec![0u8; footer.index_comp_len as usize];
        file.read_exact_at(&mut ix, footer.index_offset).is_ok() && format::hash16(&ix) == footer.index_hash
    };
    let mut damage = Damage { regions: Vec::with_capacity(rec.regions.len()) };
    // The shift carries across regions: an insertion early in the file moves everything after it.
    let mut shift: i64 = 0;
    for (r, &(k, m)) in rec.regions.iter().enumerate() {
        hooks.check()?;
        let base = rec.hash_base(r);
        let start = r as u64 * REGION;
        let limit = start + rec.region_len(r);
        let want = |i: usize| ((limit - (start + i as u64 * slice as u64)).min(slice as u64)) as usize;
        let mut bad_data = Vec::new();
        let mut shifts = Vec::with_capacity(k as usize);
        let mut i = 0usize;
        while i < k as usize {
            let at = (start + i as u64 * slice as u64) as i64;
            if matches(file, file_len, at + shift, slice, want(i), &rec.hashes[base + i]) {
                shifts.push(shift);
                i += 1;
                continue;
            }
            // Not where it should be: was something inserted or dropped? Look for
            // the slice itself by its anchor, then for the next one.
            let _ = at;
            let found = find_shift(file, file_len, rec, r, i, shift, want(i))
                .map(|s| (s, false))
                .or_else(|| (i + 1 < k as usize).then(|| find_shift(file, file_len, rec, r, i + 1, shift, want(i + 1)).map(|s| (s, true))).flatten());
            match found {
                Some((s, this_one_damaged)) => {
                    shift = s;
                    report.realigned = true;
                    if this_one_damaged {
                        bad_data.push(i);
                    }
                    shifts.push(shift);
                }
                None => {
                    bad_data.push(i);
                    shifts.push(shift);
                }
            }
            i += 1;
        }
        let pbase = rec.parity_base(r);
        let mut bad_parity = Vec::new();
        for j in 0..m as usize {
            let at = pbase + j as u64 * slice as u64;
            let ok = read_slice(file, file_len, at, slice, at + slice as u64).is_some_and(|s| format::hash16(&s) == rec.hashes[base + k as usize + j]);
            if !ok {
                bad_parity.push(j);
            }
        }
        report.damaged_data_slices += bad_data.len() as u64;
        report.damaged_data_bytes += bad_data.iter().map(|&i| (limit - (start + i as u64 * slice as u64)).min(slice as u64)).sum::<u64>();
        report.damaged_parity_slices += bad_parity.len() as u64;
        if bad_data.len() + bad_parity.len() > m as usize {
            report.unrepairable_regions.push(r as u32);
        }
        hooks.report(limit, rec.data_len);
        damage.regions.push(RegionDamage { bad_data, bad_parity, shifts });
    }
    report.repairable = report.unrepairable_regions.is_empty();
    if !report.repairable {
        report.lost_files = lost_files(rec, &damage).unwrap_or_default();
    }
    Ok((report, damage))
}

/// The paths whose data lies in a region that cannot be repaired, from the
/// index copy: damaged slices → blocks → chunks → objects → entries.
fn lost_files(rec: &Record, damage: &Damage) -> Result<Vec<String>> {
    let footer = Footer::parse(&rec.footer_copy)?;
    let raw = zstd::bulk::decompress(&rec.index_copy, footer.index_raw_len as usize)?;
    let index = format::Index::decode(&raw)?;
    let slice = rec.slice as u64;
    let mut ranges: Vec<(u64, u64)> = Vec::new();
    for (r, d) in damage.regions.iter().enumerate() {
        let bad = &d.bad_data;
        if bad.len() + d.bad_parity.len() <= rec.regions[r].1 as usize {
            continue;
        }
        let start = r as u64 * REGION;
        for &i in bad {
            ranges.push((start + i as u64 * slice, start + (i as u64 + 1) * slice));
        }
    }
    let hit_block: Vec<bool> = index
        .blocks
        .iter()
        .map(|b| ranges.iter().any(|&(a, z)| a < b.file_offset + b.comp_len && b.file_offset < z))
        .collect();
    let hit_object: Vec<bool> = index.objects.iter().map(|o| o.chunks.iter().any(|&c| hit_block[index.chunks[c as usize].block as usize])).collect();
    let mut out: Vec<String> = index
        .entries
        .iter()
        .filter_map(|e| match &e.kind {
            format::EntryKind::File { object, .. } if hit_object[*object as usize] => Some(e.path.clone()),
            _ => None,
        })
        .collect();
    out.sort();
    Ok(out)
}

/// Looks for a record and reports what it finds, repairing nothing.
pub fn check(path: &Path, hooks: &Hooks) -> Result<CheckReport> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let file_len = file.metadata()?.len();
    let Some(rec) = locate(&file, file_len)? else {
        return Ok(CheckReport { file_len, ..Default::default() });
    };
    Ok(survey(&file, file_len, &rec, hooks)?.0)
}

/// Whether the archive carries a recovery record, and what it protects.
pub fn info(path: &Path) -> Result<Option<RecordInfo>> {
    let file = File::open(path)?;
    let file_len = file.metadata()?.len();
    Ok(locate(&file, file_len)?.map(|r| r.info()))
}

/// Writes a repaired copy of `path` to `out` and reports what was done. The
/// copy is complete when the report says `repairable`; otherwise the regions
/// that could be repaired are, and `lost_files` names what could not. Every
/// byte of the copy is written at its proper place, so a file with bytes
/// inserted or dropped comes back to its original length and layout.
pub fn repair(path: &Path, out: &Path, hooks: &Hooks) -> Result<CheckReport> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let file_len = file.metadata()?.len();
    let rec = locate(&file, file_len)?.context("no recovery record found in this archive")?;
    let (before, damage) = survey(&file, file_len, &rec, hooks)?;
    let footer = Footer::parse(&rec.footer_copy)?;
    let expected = before.expected_len;
    let slice = rec.slice as usize;
    let head_len = rec.head.len() as u64;
    // Where the record's parts belong in the original layout.
    let record_at = rec.data_len;
    let parity_at = record_at + head_len;
    let parity_len: u64 = rec.regions.iter().map(|&(_, m)| m as u64 * slice as u64).sum();

    let dst = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(out).with_context(|| format!("creating {}", out.display()))?;
    dst.set_len(expected)?;

    for (r, &(k, m)) in rec.regions.iter().enumerate() {
        hooks.check()?;
        let d = &damage.regions[r];
        let start = r as u64 * REGION;
        let limit = start + rec.region_len(r);
        let want = |i: usize| ((limit - (start + i as u64 * slice as u64)).min(slice as u64)) as usize;
        let repairable = d.bad_data.len() + d.bad_parity.len() <= m as usize;
        // Every intact data slice, from wherever it sits in the damaged file.
        let mut shards: Vec<Option<Vec<u8>>> = (0..k as usize)
            .map(|i| {
                if d.bad_data.contains(&i) {
                    return None;
                }
                let at = (start + i as u64 * slice as u64) as i64 + d.shifts[i];
                read_slice_at(&file, file_len, at, slice, want(i))
            })
            .collect();
        let pbase = rec.parity_base(r);
        let parity: Vec<Option<Vec<u8>>> = (0..m as usize)
            .map(|j| {
                if d.bad_parity.contains(&j) {
                    return None;
                }
                let at = pbase + j as u64 * slice as u64;
                read_slice(&file, file_len, at, slice, at + slice as u64)
            })
            .collect();
        if repairable && shards.iter().any(Option::is_none) {
            let originals: Vec<(usize, &[u8])> = shards.iter().enumerate().filter_map(|(i, s)| s.as_deref().map(|s| (i, s))).collect();
            let recovery: Vec<(usize, &[u8])> = parity.iter().enumerate().filter_map(|(j, s)| s.as_deref().map(|s| (j, s))).collect();
            let restored: BTreeMap<usize, Vec<u8>> =
                reed_solomon_simd::decode(k as usize, m as usize, originals, recovery).map_err(|e| anyhow::anyhow!("recovery decoding: {e}"))?;
            for (i, s) in restored {
                shards[i] = Some(s);
            }
        }
        // The data, at its original offsets; a slice nothing could bring back stays zero.
        for (i, s) in shards.iter().enumerate() {
            if let Some(s) = s {
                dst.write_all_at(&s[..want(i)], start + i as u64 * slice as u64)?;
            }
        }
        // The parity: intact slices as they were, the rest regenerated from complete data.
        let region_parity_at = parity_at + rec.regions[..r].iter().map(|&(_, m)| m as u64 * slice as u64).sum::<u64>();
        let regenerated = if repairable && parity.iter().any(Option::is_none) {
            let all: Vec<&[u8]> = shards.iter().map(|s| s.as_deref().expect("repairable region is complete")).collect();
            Some(reed_solomon_simd::encode(k as usize, m as usize, all).map_err(|e| anyhow::anyhow!("recovery coding: {e}"))?)
        } else {
            None
        };
        for j in 0..m as usize {
            let bytes: Option<&[u8]> = match (&parity[j], &regenerated) {
                (Some(p), _) => Some(p.as_slice()),
                (None, Some(rg)) => Some(rg[j].as_slice()),
                (None, None) => None,
            };
            if let Some(b) = bytes {
                dst.write_all_at(b, region_parity_at + j as u64 * slice as u64)?;
            }
        }
        hooks.report(limit, rec.data_len);
    }
    // The record's two headers and trailer, the index and the footer, from the copies.
    dst.write_all_at(&rec.head, record_at)?;
    dst.write_all_at(&rec.head, parity_at + parity_len)?;
    let trailer_at = parity_at + parity_len + head_len;
    dst.write_all_at(&rec.len.to_le_bytes(), trailer_at)?;
    dst.write_all_at(&head_len.to_le_bytes(), trailer_at + 8)?;
    dst.write_all_at(TRAILER_MAGIC, trailer_at + 16)?;
    dst.write_all_at(&rec.index_copy, footer.index_offset)?;
    dst.write_all_at(&rec.footer_copy, expected - FOOTER_LEN as u64)?;
    dst.sync_all()?;

    let mut after = survey(&dst, expected, &rec, hooks)?.0;
    if !before.repairable {
        after.repairable = false;
        after.unrepairable_regions = before.unrepairable_regions.clone();
        after.lost_files = before.lost_files.clone();
    }
    after.realigned = before.realigned;
    Ok(after)
}

/// Writes `out`: the archive at `path` with a recovery record of `percent`
/// (replacing any record it has).
pub fn protect(path: &Path, out: &Path, percent: u16, hooks: &Hooks) -> Result<RecordInfo> {
    ensure!((MIN_PERCENT..=MAX_PERCENT).contains(&percent), "recovery record of {percent}%: choose 1 to 50");
    let a = crate::unpack::Archive::open(path)?;
    let footer = a.footer();
    let data_len = if footer.recovery_offset > 0 { footer.recovery_offset } else { footer.index_offset };
    let src = File::open(path)?;
    let mut index_comp = vec![0u8; footer.index_comp_len as usize];
    src.read_exact_at(&mut index_comp, footer.index_offset)?;
    let mut dst = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(out).with_context(|| format!("creating {}", out.display()))?;
    let mut buf = vec![0u8; 8 << 20];
    let mut pos = 0u64;
    while pos < data_len {
        hooks.check()?;
        let n = (data_len - pos).min(buf.len() as u64) as usize;
        src.read_exact_at(&mut buf[..n], pos)?;
        dst.write_all(&buf[..n])?;
        pos += n as u64;
    }
    write_tail(&mut dst, data_len, &index_comp, footer.index_raw_len, percent, hooks)?;
    dst.sync_all()?;
    Ok(info(out)?.expect("the record was just written"))
}
