//! Writing ZIP archives on every core.
//!
//! Every file is compressed (zlib-rs Deflate) on whichever core is free, up to a
//! bounded distance ahead of a writer that lays the entries down in a fixed
//! order, so the same input always gives the same archive. A file that Deflate
//! doesn't shrink is stored. Entries carry Unix permissions, an exact
//! modification time (extended-timestamp field) next to the DOS one, UTF-8
//! names, and links as links; ZIP64 records appear only when sizes, offsets or
//! the entry count need them. Like the 7-Zip path it replaces, extended
//! attributes are not stored.

use crate::format::{Entry, EntryKind};
use crate::hooks::Hooks;
use crate::scan;
use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct ZipWriteOptions {
    /// Deflate level, 1 (fastest) to 9 (smallest); 0 stores everything.
    pub level: u32,
    pub threads: usize,
    /// Leave out `.DS_Store`, `._*` AppleDouble files, `__MACOSX` and the
    /// volume-level folders macOS keeps (what the app's toggle promises).
    pub leave_out_mac_files: bool,
    pub hooks: Hooks,
}

impl Default for ZipWriteOptions {
    fn default() -> Self {
        ZipWriteOptions { level: 6, threads: 0, leave_out_mac_files: true, hooks: Hooks::default() }
    }
}

#[derive(Debug, Default, Clone)]
pub struct ZipWriteStats {
    pub files: u64,
    pub dirs: u64,
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub seconds: f64,
}

const MAC_ONLY: &[&str] = &[".DS_Store", "__MACOSX", ".Spotlight-V100", ".fseventsd", ".Trashes"];
/// How far (input bytes) compression may run ahead of the writer.
const LOOKAHEAD: u64 = 256 << 20;

fn left_out(path: &str) -> bool {
    path.split('/').any(|part| MAC_ONLY.contains(&part) || (part.starts_with("._") && part.len() > 2))
}

/// A file's bytes: read whole when small, mapped when large (so storing a huge
/// file that doesn't compress never copies it).
enum Bytes {
    Owned(Vec<u8>),
    Mapped(memmap2::Mmap),
}

impl std::ops::Deref for Bytes {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            Bytes::Owned(v) => v,
            Bytes::Mapped(m) => m,
        }
    }
}

/// One entry, compressed (or not) and ready to write.
struct Ready {
    method: u16,
    crc: u32,
    size: u64,
    data: Bytes,
}

/// DOS date and time (local, 2-second steps) for Unix seconds.
fn dos_time(secs: i64) -> u32 {
    // SAFETY: localtime_r fills `tm` from a valid time_t.
    unsafe {
        let t = secs as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() || tm.tm_year < 80 {
            return (1 << 21) | (1 << 16); // 1980-01-01, the earliest DOS date
        }
        let date = ((tm.tm_year - 80) as u32) << 9 | ((tm.tm_mon + 1) as u32) << 5 | tm.tm_mday as u32;
        let time = (tm.tm_hour as u32) << 11 | (tm.tm_min as u32) << 5 | (tm.tm_sec as u32 / 2);
        date << 16 | time
    }
}

fn compress(raw: Bytes, level: u32) -> Ready {
    let crc = crc32fast::hash(&raw);
    if level > 0 && !raw.is_empty() {
        let mut c = flate2::Compress::new(flate2::Compression::new(level), false);
        let mut out = Vec::with_capacity(raw.len() / 2 + 64);
        loop {
            let status = c.compress_vec(&raw[c.total_in() as usize..], &mut out, flate2::FlushCompress::Finish);
            match status {
                Ok(flate2::Status::StreamEnd) => break,
                Ok(_) => {
                    if out.len() >= raw.len() {
                        break; // already no smaller than storing
                    }
                    out.reserve(raw.len() / 4 + 64);
                }
                Err(_) => break,
            }
        }
        if c.total_in() as usize == raw.len() && out.len() < raw.len() {
            return Ready { method: 8, crc, size: raw.len() as u64, data: Bytes::Owned(out) };
        }
    }
    Ready { method: 0, crc, size: raw.len() as u64, data: raw }
}

fn read_all(path: &Path, size: u64) -> Result<Bytes> {
    let mut f = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let bytes = if size >= 1 << 20 {
        // SAFETY: only read; a file changed underneath gets a CRC of what was read.
        Bytes::Mapped(unsafe { memmap2::Mmap::map(&f) }.with_context(|| format!("mapping {}", path.display()))?)
    } else {
        let mut v = Vec::with_capacity(size as usize);
        f.read_to_end(&mut v).with_context(|| format!("reading {}", path.display()))?;
        Bytes::Owned(v)
    };
    anyhow::ensure!(bytes.len() as u64 == size, "{} changed size while archiving", path.display());
    Ok(bytes)
}

struct Record {
    name: Vec<u8>,
    method: u16,
    crc: u32,
    compressed: u64,
    size: u64,
    offset: u64,
    dos: u32,
    mtime: i64,
    external: u32,
}

/// Writes `inputs` as a ZIP archive at `output` (via a temporary name).
pub fn write_zip(inputs: &[PathBuf], output: &Path, opts: &ZipWriteOptions) -> Result<ZipWriteStats> {
    let mut tmp_name = output.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".partial");
    let tmp = output.with_file_name(tmp_name);
    let result = write_to(inputs, output, &tmp, opts);
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

fn write_to(inputs: &[PathBuf], output: &Path, tmp: &Path, opts: &ZipWriteOptions) -> Result<ZipWriteStats> {
    let start = Instant::now();
    let threads = if opts.threads == 0 { std::thread::available_parallelism().map_or(4, |n| n.get()) } else { opts.threads };
    let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build()?;
    let mut scanned = pool.install(|| scan::scan(inputs))?;
    if opts.leave_out_mac_files {
        scanned.retain(|s| !left_out(&s.entry.path));
    }
    // Hard links have no ZIP form: each comes out as its own copy of the file.
    let sources: Vec<Option<PathBuf>> = scanned
        .iter()
        .map(|s| match (&s.entry.kind, &s.source) {
            (EntryKind::HardLink { target }, _) => scanned.get(*target as usize).and_then(|t| t.source.clone()),
            (_, src) => src.clone(),
        })
        .collect();
    let entries: Vec<&Entry> = scanned.iter().map(|s| &s.entry).collect();
    let size_of = |i: usize| -> u64 {
        match &entries[i].kind {
            EntryKind::File { size, .. } => *size,
            EntryKind::HardLink { target } => match &scanned[*target as usize].entry.kind {
                EntryKind::File { size, .. } => *size,
                _ => 0,
            },
            _ => 0,
        }
    };
    let total: u64 = (0..entries.len()).map(size_of).sum();

    let mut out = BufWriter::with_capacity(4 << 20, File::create(tmp).with_context(|| format!("creating {}", tmp.display()))?);
    let mut records: Vec<Record> = Vec::with_capacity(entries.len());
    let mut offset = 0u64;
    let mut stats = ZipWriteStats::default();

    // Files compress ahead of the writer, which takes them strictly in order.
    let n = entries.len();
    let slots: Vec<Mutex<Option<Result<Ready>>>> = (0..n).map(|_| Mutex::new(None)).collect();
    let (ready_tx, ready_rx) = crossbeam_channel::unbounded::<()>();
    let level = opts.level.min(9);
    let job = |i: usize| -> Result<Ready> {
        opts.hooks.check()?;
        match &entries[i].kind {
            EntryKind::Symlink { target } => Ok(compress(Bytes::Owned(target.clone()), 0)),
            EntryKind::File { .. } | EntryKind::HardLink { .. } => {
                let src = sources[i].as_ref().context("file without a source")?;
                Ok(compress(read_all(src, size_of(i))?, level))
            }
            EntryKind::Dir => Ok(Ready { method: 0, crc: 0, size: 0, data: Bytes::Owned(Vec::new()) }),
        }
    };
    let (slots, job) = (&slots, &job);
    pool.in_place_scope(|scope| -> Result<()> {
        let (mut spawned, mut ahead, mut done) = (0usize, 0u64, 0u64);
        for k in 0..n {
            while spawned < n && (spawned == k || ahead + size_of(spawned) <= LOOKAHEAD) {
                let i = spawned;
                ahead += size_of(i);
                spawned += 1;
                let tx = ready_tx.clone();
                scope.spawn(move |_| {
                    *slots[i].lock().unwrap() = Some(job(i));
                    let _ = tx.send(());
                });
            }
            let ready = loop {
                if let Some(r) = slots[k].lock().unwrap().take() {
                    break r?;
                }
                ready_rx.recv().context("compression stalled")?;
            };
            ahead -= size_of(k);
            let e = entries[k];
            let dir = matches!(e.kind, EntryKind::Dir);
            let mut name = e.path.clone().into_bytes();
            if dir {
                name.push(b'/');
                stats.dirs += 1;
            } else if !matches!(e.kind, EntryKind::Symlink { .. }) {
                stats.files += 1;
            }
            let kind_bits: u32 = match e.kind {
                EntryKind::Dir => 0o040000,
                EntryKind::Symlink { .. } => 0o120000,
                _ => 0o100000,
            };
            let external = ((kind_bits | (e.mode & 0o7777)) << 16) | if dir { 0x10 } else { 0 };
            let rec = Record {
                name,
                method: ready.method,
                crc: ready.crc,
                compressed: ready.data.len() as u64,
                size: ready.size,
                offset,
                dos: dos_time(e.mtime.0),
                mtime: e.mtime.0,
                external,
            };
            offset += write_local(&mut out, &rec)?;
            out.write_all(&ready.data)?;
            offset += ready.data.len() as u64;
            done += ready.size;
            stats.input_bytes += ready.size;
            opts.hooks.report(done, total);
            records.push(rec);
        }
        Ok(())
    })?;

    let cd_start = offset;
    for r in &records {
        offset += write_central(&mut out, r)?;
    }
    write_end(&mut out, records.len() as u64, cd_start, offset - cd_start)?;
    let file = out.into_inner().map_err(|e| e.into_error())?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(tmp, output)?;
    stats.output_bytes = std::fs::metadata(output)?.len();
    stats.seconds = start.elapsed().as_secs_f64();
    Ok(stats)
}

fn needs_zip64(r: &Record) -> bool {
    r.size >= 0xFFFF_FFFF || r.compressed >= 0xFFFF_FFFF || r.offset >= 0xFFFF_FFFF
}

fn flags(r: &Record) -> u16 {
    // Bit 11: the name is UTF-8 (only needed, and only set, when it isn't ASCII).
    if r.name.is_ascii() { 0 } else { 1 << 11 }
}

/// Extended timestamp: modification time only.
fn ut_extra(r: &Record) -> [u8; 9] {
    let mut x = [0u8; 9];
    x[0..2].copy_from_slice(&0x5455u16.to_le_bytes());
    x[2..4].copy_from_slice(&5u16.to_le_bytes());
    x[4] = 1;
    x[5..9].copy_from_slice(&(r.mtime.clamp(i32::MIN as i64, i32::MAX as i64) as i32).to_le_bytes());
    x
}

fn write_local(w: &mut impl Write, r: &Record) -> Result<u64> {
    let z64 = needs_zip64(r);
    let mut extra = ut_extra(r).to_vec();
    if z64 {
        extra.extend_from_slice(&1u16.to_le_bytes());
        extra.extend_from_slice(&16u16.to_le_bytes());
        extra.extend_from_slice(&r.size.to_le_bytes());
        extra.extend_from_slice(&r.compressed.to_le_bytes());
    }
    let mut h = Vec::with_capacity(30 + r.name.len() + extra.len());
    h.extend_from_slice(b"PK\x03\x04");
    h.extend_from_slice(&(if z64 { 45u16 } else { 20 }).to_le_bytes());
    h.extend_from_slice(&flags(r).to_le_bytes());
    h.extend_from_slice(&r.method.to_le_bytes());
    h.extend_from_slice(&r.dos.to_le_bytes());
    h.extend_from_slice(&r.crc.to_le_bytes());
    let (c, u) = if z64 { (0xFFFF_FFFFu32, 0xFFFF_FFFFu32) } else { (r.compressed as u32, r.size as u32) };
    h.extend_from_slice(&c.to_le_bytes());
    h.extend_from_slice(&u.to_le_bytes());
    h.extend_from_slice(&(r.name.len() as u16).to_le_bytes());
    h.extend_from_slice(&(extra.len() as u16).to_le_bytes());
    h.extend_from_slice(&r.name);
    h.extend_from_slice(&extra);
    w.write_all(&h)?;
    Ok(h.len() as u64)
}

fn write_central(w: &mut impl Write, r: &Record) -> Result<u64> {
    let z64 = needs_zip64(r);
    let mut extra = ut_extra(r).to_vec();
    if z64 {
        let mut fields = Vec::new();
        if r.size >= 0xFFFF_FFFF {
            fields.extend_from_slice(&r.size.to_le_bytes());
        }
        if r.compressed >= 0xFFFF_FFFF {
            fields.extend_from_slice(&r.compressed.to_le_bytes());
        }
        if r.offset >= 0xFFFF_FFFF {
            fields.extend_from_slice(&r.offset.to_le_bytes());
        }
        extra.extend_from_slice(&1u16.to_le_bytes());
        extra.extend_from_slice(&(fields.len() as u16).to_le_bytes());
        extra.extend_from_slice(&fields);
    }
    let sat = |v: u64| if v >= 0xFFFF_FFFF { 0xFFFF_FFFFu32 } else { v as u32 };
    let mut h = Vec::with_capacity(46 + r.name.len() + extra.len());
    h.extend_from_slice(b"PK\x01\x02");
    // Made by Unix (3), spec 6.3, so readers take the mode from the high half.
    h.extend_from_slice(&(3u16 << 8 | 63).to_le_bytes());
    h.extend_from_slice(&(if z64 { 45u16 } else { 20 }).to_le_bytes());
    h.extend_from_slice(&flags(r).to_le_bytes());
    h.extend_from_slice(&r.method.to_le_bytes());
    h.extend_from_slice(&r.dos.to_le_bytes());
    h.extend_from_slice(&r.crc.to_le_bytes());
    h.extend_from_slice(&sat(r.compressed).to_le_bytes());
    h.extend_from_slice(&sat(r.size).to_le_bytes());
    h.extend_from_slice(&(r.name.len() as u16).to_le_bytes());
    h.extend_from_slice(&(extra.len() as u16).to_le_bytes());
    h.extend_from_slice(&0u16.to_le_bytes()); // comment
    h.extend_from_slice(&0u16.to_le_bytes()); // disk
    h.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
    h.extend_from_slice(&r.external.to_le_bytes());
    h.extend_from_slice(&sat(r.offset).to_le_bytes());
    h.extend_from_slice(&r.name);
    h.extend_from_slice(&extra);
    w.write_all(&h)?;
    Ok(h.len() as u64)
}

fn write_end(w: &mut impl Write, count: u64, cd_start: u64, cd_len: u64) -> Result<()> {
    if count >= 0xFFFF || cd_start >= 0xFFFF_FFFF || cd_len >= 0xFFFF_FFFF {
        let z64_at = cd_start + cd_len;
        let mut e = Vec::new();
        e.extend_from_slice(b"PK\x06\x06");
        e.extend_from_slice(&44u64.to_le_bytes());
        e.extend_from_slice(&(3u16 << 8 | 63).to_le_bytes());
        e.extend_from_slice(&45u16.to_le_bytes());
        e.extend_from_slice(&0u32.to_le_bytes());
        e.extend_from_slice(&0u32.to_le_bytes());
        e.extend_from_slice(&count.to_le_bytes());
        e.extend_from_slice(&count.to_le_bytes());
        e.extend_from_slice(&cd_len.to_le_bytes());
        e.extend_from_slice(&cd_start.to_le_bytes());
        e.extend_from_slice(b"PK\x06\x07");
        e.extend_from_slice(&0u32.to_le_bytes());
        e.extend_from_slice(&z64_at.to_le_bytes());
        e.extend_from_slice(&1u32.to_le_bytes());
        w.write_all(&e)?;
    }
    let sat16 = |v: u64| if v >= 0xFFFF { 0xFFFFu16 } else { v as u16 };
    let sat32 = |v: u64| if v >= 0xFFFF_FFFF { 0xFFFF_FFFFu32 } else { v as u32 };
    let mut e = Vec::with_capacity(22);
    e.extend_from_slice(b"PK\x05\x06");
    e.extend_from_slice(&0u16.to_le_bytes());
    e.extend_from_slice(&0u16.to_le_bytes());
    e.extend_from_slice(&sat16(count).to_le_bytes());
    e.extend_from_slice(&sat16(count).to_le_bytes());
    e.extend_from_slice(&sat32(cd_len).to_le_bytes());
    e.extend_from_slice(&sat32(cd_start).to_le_bytes());
    e.extend_from_slice(&0u16.to_le_bytes());
    w.write_all(&e)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_only_names() {
        assert!(left_out("a/.DS_Store"));
        assert!(left_out("a/._x"));
        assert!(left_out("__MACOSX/a/x"));
        assert!(!left_out("a/._"));
        assert!(!left_out("a/x.DS_Store"));
    }
}
