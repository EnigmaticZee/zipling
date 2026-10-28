//! Reading ZIP archives on every core.
//!
//! ZIP stores every file as its own compressed stream, so extraction is
//! naturally parallel: each entry is inflated (zlib-rs) and written on whichever
//! core is free, with its CRC-32 checked before anything is kept. A ZIP made on
//! a Mac carries a `._` AppleDouble twin for every file (in `__MACOSX/` or
//! beside it); those are decoded in memory and applied as extended attributes,
//! Finder info and resource forks while the real files are written, instead of
//! being written out and merged one by one. As in the app, quarantine and ACLs
//! from AppleDouble are never restored.
//!
//! Anything this reader does not handle (encryption, methods other than store
//! and Deflate, names in a legacy encoding, split archives) is reported as an
//! error starting with "unsupported:", so the caller can fall back to 7-Zip.

use crate::format::{Entry, EntryKind};
use crate::hooks::Hooks;
use crate::unpack::{apply_metadata, apply_metadata_fd, create_file, safe_join};
use anyhow::{bail, ensure, Context, Result};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// One entry of the central directory.
#[derive(Debug, Clone)]
pub struct ZipEntry {
    pub name: String,
    pub method: u16,
    pub flags: u16,
    pub crc: u32,
    pub compressed: u64,
    pub size: u64,
    /// Offset of the entry's local header.
    pub local: u64,
    pub made_by: u16,
    pub external: u32,
    pub dos_time: u32,
    /// Modification time from the extended-timestamp extra field, if present.
    pub unix_mtime: Option<i64>,
}

impl ZipEntry {
    fn unix_mode(&self) -> Option<u32> {
        // Made on Unix (3) or macOS (19): the high half is st_mode.
        matches!(self.made_by >> 8, 3 | 19).then_some(self.external >> 16).filter(|&m| m != 0)
    }

    pub fn is_dir(&self) -> bool {
        self.name.ends_with('/') || self.unix_mode().map_or(self.external & 0x10 != 0, |m| m & 0o170000 == 0o040000)
    }

    pub fn is_symlink(&self) -> bool {
        self.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
    }

    fn mode(&self) -> u32 {
        match self.unix_mode() {
            Some(m) => m & 0o7777,
            None if self.is_dir() => 0o755,
            None => 0o644,
        }
    }

    fn mtime(&self) -> (i64, u32) {
        (self.unix_mtime.unwrap_or_else(|| dos_to_unix(self.dos_time)), 0)
    }
}

/// DOS date and time (local time, 2-second steps) as Unix seconds.
fn dos_to_unix(t: u32) -> i64 {
    let (date, time) = (t >> 16, t & 0xFFFF);
    // SAFETY: a zeroed tm is valid; mktime only reads and normalises it.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        tm.tm_year = ((date >> 9) & 0x7F) as i32 + 80;
        tm.tm_mon = ((date >> 5) & 0x0F) as i32 - 1;
        tm.tm_mday = (date & 0x1F) as i32;
        tm.tm_hour = ((time >> 11) & 0x1F) as i32;
        tm.tm_min = ((time >> 5) & 0x3F) as i32;
        tm.tm_sec = ((time & 0x1F) * 2) as i32;
        tm.tm_isdst = -1;
        libc::mktime(&mut tm) as i64
    }
}

pub struct ZipArchive {
    map: memmap2::Mmap,
    pub entries: Vec<ZipEntry>,
}

fn unsupported(why: impl std::fmt::Display) -> anyhow::Error {
    anyhow::anyhow!("unsupported: {why}")
}

impl ZipArchive {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
        // SAFETY: the archive is only read; a file changed underneath fails its CRCs.
        let map = unsafe { memmap2::Mmap::map(&file) }.with_context(|| format!("mapping {}", path.display()))?;
        let entries = central_directory(&map)?;
        Ok(ZipArchive { map, entries })
    }

    /// Where an entry's compressed bytes are.
    fn data(&self, e: &ZipEntry) -> Result<&[u8]> {
        let d = &self.map[..];
        let at = usize::try_from(e.local)?;
        ensure!(le32(d, at) == Some(0x0403_4b50), "{}: local header missing", e.name);
        let (n, x) = (le16(d, at + 26).context("truncated")? as usize, le16(d, at + 28).context("truncated")? as usize);
        let start = at + 30 + n + x;
        let end = start.checked_add(usize::try_from(e.compressed)?).context("bad size")?;
        d.get(start..end).with_context(|| format!("{}: data runs past the end of the archive", e.name))
    }
}

fn le16(d: &[u8], at: usize) -> Option<u16> {
    d.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn le32(d: &[u8], at: usize) -> Option<u32> {
    d.get(at..at + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()))
}

fn le64(d: &[u8], at: usize) -> Option<u64> {
    d.get(at..at + 8).map(|b| u64::from_le_bytes(b.try_into().unwrap()))
}

fn central_directory(d: &[u8]) -> Result<Vec<ZipEntry>> {
    let tail = d.len().saturating_sub(65_557);
    let eocd = memchr::memmem::rfind(&d[tail..], b"PK\x05\x06").map(|p| p + tail).context("not a ZIP archive")?;
    let disk = le16(d, eocd + 4).context("truncated")?;
    let cd_disk = le16(d, eocd + 6).context("truncated")?;
    let mut count = le16(d, eocd + 10).context("truncated")? as u64;
    let mut cd_off = le32(d, eocd + 16).context("truncated")? as u64;
    if disk != 0 && disk != 0xFFFF || cd_disk != 0 && cd_disk != 0xFFFF {
        return Err(unsupported("split archive"));
    }
    if (count == 0xFFFF || cd_off == 0xFFFF_FFFF) && eocd >= 20 && le32(d, eocd - 20) == Some(0x0706_4b50) {
        let z = le64(d, eocd - 12).context("truncated")? as usize;
        ensure!(le32(d, z) == Some(0x0606_4b50), "ZIP64 end record missing");
        count = le64(d, z + 32).context("truncated")?;
        cd_off = le64(d, z + 48).context("truncated")?;
    }
    let mut out = Vec::with_capacity(count.min(1 << 20) as usize);
    let mut p = usize::try_from(cd_off)?;
    for _ in 0..count {
        ensure!(le32(d, p) == Some(0x0201_4b50), "central directory is damaged");
        let f = |o| le16(d, p + o).context("truncated");
        let g = |o| le32(d, p + o).context("truncated");
        let (made_by, flags, method, dos_time, crc) = (f(4)?, f(8)?, f(10)?, g(12)?, g(16)?);
        let (mut compressed, mut size) = (g(20)? as u64, g(24)? as u64);
        let (nlen, xlen, clen) = (f(28)? as usize, f(30)? as usize, f(32)? as usize);
        let start_disk = f(34)?;
        let external = g(38)?;
        let mut local = g(42)? as u64;
        let raw_name = d.get(p + 46..p + 46 + nlen).context("truncated")?;
        let extra = d.get(p + 46 + nlen..p + 46 + nlen + xlen).context("truncated")?;
        let mut unix_mtime = None;
        let mut x = 0;
        while x + 4 <= extra.len() {
            let (id, len) = (le16(extra, x).unwrap(), le16(extra, x + 2).unwrap() as usize);
            let body = extra.get(x + 4..x + 4 + len).unwrap_or(&[]);
            match id {
                // ZIP64: the real values of saturated fields, in this order.
                0x0001 => {
                    let mut q = 0;
                    for field in [&mut size, &mut compressed, &mut local] {
                        if *field == 0xFFFF_FFFF {
                            *field = le64(body, q).context("ZIP64 field missing")?;
                            q += 8;
                        }
                    }
                }
                // Extended timestamp: flags, then the modification time first.
                0x5455 if body.len() >= 5 && body[0] & 1 != 0 => {
                    unix_mtime = Some(i32::from_le_bytes(body[1..5].try_into().unwrap()) as i64);
                }
                // Info-ZIP's older Unix field, which ditto writes: access time,
                // then modification time (then owner, in local headers).
                0x5855 if body.len() >= 8 && unix_mtime.is_none() => {
                    unix_mtime = Some(i32::from_le_bytes(body[4..8].try_into().unwrap()) as i64);
                }
                _ => {}
            }
            x += 4 + len;
        }
        p += 46 + nlen + xlen + clen;
        if start_disk != 0 && start_disk != 0xFFFF {
            return Err(unsupported("split archive"));
        }
        // Bit 11 marks UTF-8; Mac and most modern tools write UTF-8 either way.
        // Other encodings (old Windows code pages) go to a reader that detects them.
        let name = match std::str::from_utf8(raw_name) {
            Ok(n) => n.to_owned(),
            Err(_) => return Err(unsupported("file names in a legacy encoding")),
        };
        out.push(ZipEntry { name, method, flags, crc, compressed, size, local, made_by, external, dos_time, unix_mtime });
    }
    Ok(out)
}

#[derive(Debug, Default, Clone)]
pub struct ZipExtractOptions {
    pub threads: usize,
    pub overwrite: bool,
    /// Extract only entries whose path equals or lies under one of these.
    pub only: Vec<String>,
    /// Set as `com.apple.quarantine` on everything extracted (the archive's own).
    pub quarantine: Option<Vec<u8>>,
    pub hooks: Hooks,
}

#[derive(Debug, Default, Clone)]
pub struct ZipExtractStats {
    pub files: u64,
    pub bytes: u64,
    pub seconds: f64,
    /// Entries left out for safety: paths that would land outside the destination
    /// or beneath a link the archive makes, and links that point outside.
    pub skipped_links: Vec<String>,
}

const QUARANTINE: &str = "com.apple.quarantine";
const FINDER_INFO: &str = "com.apple.FinderInfo";
const ACL: &str = "com.apple.acl.text";
/// Entries larger than this are inflated and written piece by piece.
const STREAM_ABOVE: u64 = 64 << 20;

/// What an entry becomes.
enum Role {
    Dir,
    File,
    Symlink,
    /// Mac metadata for the entry at this path.
    AppleDouble(String),
    Skip,
}

fn role(e: &ZipEntry) -> Role {
    let path = e.name.trim_end_matches('/');
    let sequestered = path == "__MACOSX" || path.starts_with("__MACOSX/");
    let (dir, name) = path.rsplit_once('/').map_or(("", path), |(d, n)| (d, n));
    if !e.is_dir() && name.len() > 2 && name.starts_with("._") {
        let dir = if sequestered { dir.strip_prefix("__MACOSX").unwrap_or(dir).trim_start_matches('/') } else { dir };
        let target = if dir.is_empty() { name[2..].to_owned() } else { format!("{dir}/{}", &name[2..]) };
        return Role::AppleDouble(target);
    }
    if sequestered {
        Role::Skip
    } else if e.is_dir() {
        Role::Dir
    } else if e.is_symlink() {
        Role::Symlink
    } else {
        Role::File
    }
}

/// Extended attributes in one AppleDouble file, as `copyfile` would set them,
/// with where each value lies. `None` for anything but version 2 laid out the
/// way macOS writes it.
pub fn apple_double(b: &[u8]) -> Option<Vec<(String, std::ops::Range<usize>)>> {
    let u16 = |i: usize| b.get(i..i + 2).map(|x| u16::from_be_bytes([x[0], x[1]]) as usize);
    let u32 = |i: usize| b.get(i..i + 4).map(|x| u32::from_be_bytes(x.try_into().unwrap()) as usize);
    if u32(0)? != 0x0005_1607 || u32(4)? != 0x0002_0000 {
        return None;
    }
    let mut out = Vec::new();
    for entry in 0..u16(24)? {
        let at = 26 + entry * 12;
        let (id, offset, length) = (u32(at)?, u32(at + 4)?, u32(at + 8)?);
        if offset.checked_add(length)? > b.len() {
            return None;
        }
        if id == 2 && length > 0 {
            out.push(("com.apple.ResourceFork".to_owned(), offset..offset + length));
        } else if id == 9 {
            if length < 32 {
                return None;
            }
            if b[offset..offset + 32].iter().any(|&x| x != 0) {
                out.push((FINDER_INFO.to_owned(), offset..offset + 32));
            }
            if length == 32 {
                continue;
            }
            // Two bytes of padding, then "ATTR", four words, three reserved
            // words, flags and the attribute count.
            let header = offset + 34;
            if u32(header)? != 0x4154_5452 {
                return None;
            }
            let mut at = header + 36;
            for _ in 0..u16(header + 34)? {
                let (start, size) = (u32(at)?, u32(at + 4)?);
                let name_len = *b.get(at + 10)? as usize; // with its terminating NUL
                if name_len < 2 || at + 11 + name_len > b.len() || start.checked_add(size)? > b.len() {
                    return None;
                }
                let name = String::from_utf8_lossy(&b[at + 11..at + 10 + name_len]).into_owned();
                out.push((name, start..start + size));
                at = (at + 11 + name_len + 3) & !3;
            }
        }
    }
    Some(out)
}

/// Attributes to set on one item: (name, value), and whether Finder hides it.
type Meta = (Vec<(String, Vec<u8>)>, bool);

fn meta_of(ad: &[u8]) -> Option<Meta> {
    let attrs = apple_double(ad)?;
    let mut hidden = false;
    let mut out = Vec::with_capacity(attrs.len());
    for (name, range) in attrs {
        if name == QUARANTINE || name == ACL {
            continue;
        }
        let value = ad[range].to_vec();
        // Like copyfile, an item Finder hides is hidden for everything else too.
        if name == FINDER_INFO && value.len() >= 10 && value[8] & 0x40 != 0 {
            hidden = true;
        }
        out.push((name, value));
    }
    Some((out, hidden))
}

/// True when a link's text, resolved from its folder, stays inside the extracted items.
fn link_stays_inside(link_path: &str, target: &[u8]) -> bool {
    let Ok(target) = std::str::from_utf8(target) else { return false };
    if target.starts_with('/') {
        return false;
    }
    let mut depth: i64 = link_path.matches('/').count() as i64;
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => depth += 1,
        }
    }
    true
}

fn inflate_into(data: &[u8], method: u16, out: &mut Vec<u8>, size: usize) -> Result<()> {
    match method {
        0 => out.extend_from_slice(data),
        8 => {
            out.reserve_exact(size);
            let mut d = flate2::Decompress::new(false);
            let status = d.decompress_vec(data, out, flate2::FlushDecompress::Finish)?;
            ensure!(status == flate2::Status::StreamEnd, "Deflate stream ends early");
        }
        _ => unreachable!(),
    }
    Ok(())
}

impl ZipArchive {
    /// Extracts the archive (or the `only` paths) into `dest`, checking every CRC.
    pub fn extract(&self, dest: &Path, opts: &ZipExtractOptions) -> Result<ZipExtractStats> {
        let start = Instant::now();
        std::fs::create_dir_all(dest)?;
        let threads = if opts.threads == 0 { std::thread::available_parallelism().map_or(4, |n| n.get()) } else { opts.threads };
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build()?;

        let wanted = |path: &str| {
            opts.only.is_empty()
                || opts.only.iter().any(|p| {
                    let p = p.trim_end_matches('/');
                    path == p || path.starts_with(&format!("{p}/"))
                })
        };

        // Sort every entry into what it becomes, refusing what this reader can't do.
        // Entries that would land outside the destination are left out and
        // reported, as 7-Zip does, so the rest of the archive still comes out.
        let (mut dirs, mut files, mut links, mut doubles) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut skipped: Vec<String> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        for e in &self.entries {
            if e.flags & 1 != 0 {
                return Err(unsupported("encrypted entries"));
            }
            if e.method != 0 && e.method != 8 {
                return Err(unsupported(format!("compression method {}", e.method)));
            }
            if e.name.contains('\\') {
                return Err(unsupported("backslashes in names"));
            }
            match role(e) {
                Role::Skip => {}
                Role::AppleDouble(target) => {
                    if wanted(&target) && safe_join(dest, &target).is_ok() && safe_join(dest, &e.name).is_ok() {
                        doubles.push((e, target));
                    }
                }
                r => {
                    let path = e.name.trim_end_matches('/');
                    if path.is_empty() || !wanted(path) {
                        continue;
                    }
                    if safe_join(dest, path).is_err() {
                        skipped.push(e.name.clone());
                        continue;
                    }
                    // Two entries for one path would leave it to chance which wins.
                    if !seen.insert(path) && !matches!(r, Role::Dir) {
                        return Err(unsupported("two entries with the same name"));
                    }
                    match r {
                        Role::Dir => dirs.push(e),
                        Role::File => files.push(e),
                        Role::Symlink => links.push(e),
                        _ => {}
                    }
                }
            }
        }

        // Nothing goes beneath a link the archive makes.
        let link_paths: HashSet<String> = links.iter().map(|e| e.name.trim_end_matches('/').to_owned()).collect();
        let under_link = |name: &str| {
            let mut parent = name.trim_end_matches('/');
            while let Some((head, _)) = parent.rsplit_once('/') {
                if link_paths.contains(head) {
                    return true;
                }
                parent = head;
            }
            false
        };
        for list in [&mut files, &mut links, &mut dirs] {
            list.retain(|e| {
                let keep = !under_link(&e.name);
                if !keep {
                    skipped.push(e.name.clone());
                }
                keep
            });
        }
        doubles.retain(|(e, target)| !under_link(target) && !under_link(&e.name));
        // Paths: safe (checked above), nothing overwritten unasked.
        let target_of = |name: &str| -> Result<PathBuf> { safe_join(dest, name.trim_end_matches('/')) };
        let targets: Vec<PathBuf> = files.iter().chain(links.iter()).map(|e| target_of(&e.name)).collect::<Result<_>>()?;
        for p in &targets {
            if p.symlink_metadata().is_ok() {
                if opts.overwrite {
                    std::fs::remove_file(p).or_else(|_| std::fs::remove_dir_all(p))?;
                } else {
                    bail!("{} already exists", p.display());
                }
            }
        }

        // Mac metadata, decoded in memory. A twin this reader can't parse is
        // written out as it is, for the caller's own merging to handle.
        let total: u64 = files.iter().map(|e| e.size).sum();
        let decoded: Vec<(String, Option<Meta>, &ZipEntry)> = pool.install(|| {
            doubles
                .par_iter()
                .map(|(e, target)| -> Result<_> {
                    let mut raw = Vec::new();
                    inflate_into(self.data(e)?, e.method, &mut raw, e.size as usize)?;
                    ensure!(crc32fast::hash(&raw) == e.crc, "{}: CRC mismatch", e.name);
                    Ok((target.clone(), meta_of(&raw), *e))
                })
                .collect::<Result<_>>()
        })?;
        let mut meta: HashMap<String, Meta> = HashMap::new();
        let mut loose: Vec<&ZipEntry> = Vec::new();
        for (target, m, e) in decoded {
            match m {
                Some(m) => {
                    meta.insert(target, m);
                }
                None => loose.push(e),
            }
        }

        let done = AtomicU64::new(0);
        let quarantine = opts.quarantine.as_deref();
        let entry_for = |e: &ZipEntry, kind: EntryKind| -> Entry {
            let path = e.name.trim_end_matches('/');
            let mut xattrs: Vec<(String, Vec<u8>)> = meta.get(path).map(|m| m.0.clone()).unwrap_or_default();
            if let Some(q) = quarantine {
                xattrs.push((QUARANTINE.to_owned(), q.to_vec()));
            }
            Entry { path: path.to_owned(), kind, mode: e.mode(), mtime: e.mtime(), xattrs }
        };
        let hide = |f: &File| {
            use std::os::fd::AsRawFd;
            // SAFETY: a valid descriptor; sets the user "hidden" flag, as copyfile does.
            unsafe {
                let mut st: libc::stat = std::mem::zeroed();
                if libc::fstat(f.as_raw_fd(), &mut st) == 0 {
                    libc::fchflags(f.as_raw_fd(), st.st_flags | libc::UF_HIDDEN);
                }
            }
        };

        // Files: inflated, checked and written on every core, largest stream first
        // within each run so no big file starts last.
        let mut order: Vec<usize> = (0..files.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(files[i].compressed));
        let all_files: Vec<&ZipEntry> = files.iter().copied().chain(loose.iter().copied()).collect();
        let loose_targets: Vec<PathBuf> = loose.iter().map(|e| target_of(&e.name)).collect::<Result<_>>()?;
        order.extend(files.len()..files.len() + loose.len());
        pool.install(|| {
            order.par_iter().with_max_len(1).try_for_each(|&i| -> Result<()> {
                opts.hooks.check()?;
                let e = all_files[i];
                let p = if i < files.len() { &targets[i] } else { &loose_targets[i - files.len()] };
                let data = self.data(e)?;
                let f = create_file(p)?;
                let mut crc = crc32fast::Hasher::new();
                if e.method == 8 && e.size > STREAM_ABOVE {
                    // Piece by piece, so a huge file never sits in memory whole.
                    let mut d = flate2::Decompress::new(false);
                    let mut buf = Vec::with_capacity(8 << 20);
                    let mut at = 0u64;
                    loop {
                        buf.clear();
                        let before = d.total_in() as usize;
                        let status = d.decompress_vec(&data[before..], &mut buf, flate2::FlushDecompress::None)?;
                        crc.update(&buf);
                        f.write_all_at(&buf, at)?;
                        at += buf.len() as u64;
                        ensure!(at <= e.size, "{}: larger than declared", e.name);
                        if status == flate2::Status::StreamEnd {
                            break;
                        }
                        ensure!(!buf.is_empty() || (d.total_in() as usize) > before, "{}: Deflate stream ends early", e.name);
                    }
                    ensure!(at == e.size, "{}: size mismatch", e.name);
                } else {
                    let mut raw = Vec::new();
                    inflate_into(data, e.method, &mut raw, e.size as usize)?;
                    ensure!(raw.len() as u64 == e.size, "{}: size mismatch", e.name);
                    crc.update(&raw);
                    f.write_all_at(&raw, 0)?;
                }
                ensure!(crc.finalize() == e.crc, "{}: CRC mismatch, the archive is damaged", e.name);
                let entry = entry_for(e, EntryKind::File { size: e.size, object: 0 });
                apply_metadata_fd(&f, &entry)?;
                if meta.get(&entry.path).is_some_and(|m| m.1) {
                    hide(&f);
                }
                let n = done.fetch_add(e.size, Ordering::Relaxed) + e.size;
                opts.hooks.report(n, total);
                Ok(())
            })
        })?;

        // Folders, including ones only implied by the paths beneath them.
        let mut dir_paths: Vec<(String, Option<&ZipEntry>)> = dirs.iter().map(|e| (e.name.trim_end_matches('/').to_owned(), Some(*e))).collect();
        for p in targets.iter().chain(loose_targets.iter()) {
            std::fs::create_dir_all(p.parent().unwrap_or(dest))?;
        }
        for (d, _) in &dir_paths {
            std::fs::create_dir_all(safe_join(dest, d)?)?;
        }
        // Symlinks last, and only those that stay inside.
        let mut skipped_links = skipped;
        for (e, p) in links.iter().zip(&targets[files.len()..]) {
            let path = e.name.trim_end_matches('/');
            let mut target = Vec::new();
            inflate_into(self.data(e)?, e.method, &mut target, e.size as usize)?;
            ensure!(crc32fast::hash(&target) == e.crc, "{}: CRC mismatch", e.name);
            if !link_stays_inside(path, &target) {
                skipped_links.push(path.to_owned());
                continue;
            }
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent)?;
            }
            use std::os::unix::ffi::OsStrExt;
            std::os::unix::fs::symlink(std::ffi::OsStr::from_bytes(&target), p)?;
            apply_metadata(p, &entry_for(e, EntryKind::Symlink { target }))?;
        }
        // Folder metadata deepest first, so writing inside doesn't change it later.
        dir_paths.sort_by_key(|(d, _)| std::cmp::Reverse(d.matches('/').count()));
        for (d, e) in &dir_paths {
            if let Some(e) = e {
                let p = safe_join(dest, d)?;
                apply_metadata(&p, &entry_for(e, EntryKind::Dir))?;
            }
        }
        if let Some(q) = quarantine {
            // Folders the archive doesn't list still came out of it.
            let listed: HashSet<&str> = dir_paths.iter().map(|(d, _)| d.as_str()).collect();
            let mut implied: HashSet<PathBuf> = HashSet::new();
            for e in files.iter().chain(links.iter()) {
                let mut path = e.name.trim_end_matches('/');
                while let Some((head, _)) = path.rsplit_once('/') {
                    if !listed.contains(head) {
                        implied.insert(safe_join(dest, head)?);
                    }
                    path = head;
                }
            }
            for p in implied {
                let _ = xattr::set(&p, QUARANTINE, q);
            }
        }

        Ok(ZipExtractStats {
            files: (files.len() + loose.len()) as u64,
            bytes: done.load(Ordering::Relaxed),
            seconds: start.elapsed().as_secs_f64(),
            skipped_links,
        })
    }

    /// Inflates every entry and checks its CRC without writing anything.
    pub fn verify(&self, threads: usize) -> Result<u64> {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads.max(1)).build()?;
        pool.install(|| {
            self.entries.par_iter().filter(|e| !e.is_dir()).try_fold(|| 0u64, |n, e| -> Result<u64> {
                if e.flags & 1 != 0 || (e.method != 0 && e.method != 8) {
                    return Err(unsupported("entries this reader can't check"));
                }
                let mut raw = Vec::new();
                inflate_into(self.data(e)?, e.method, &mut raw, e.size as usize)?;
                ensure!(crc32fast::hash(&raw) == e.crc && raw.len() as u64 == e.size, "{}: CRC mismatch", e.name);
                Ok(n + e.size)
            }).try_reduce(|| 0, |a, b| Ok(a + b))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_escape_rules() {
        assert!(link_stays_inside("a/b/link", b"../c"));
        assert!(link_stays_inside("a/link", b"../x"));
        assert!(!link_stays_inside("a/link", b"../../x"));
        assert!(!link_stays_inside("link", b"/etc/passwd"));
        assert!(link_stays_inside("a/link", b"./b/../c"));
    }

    #[test]
    fn apple_double_targets() {
        let e = |name: &str| ZipEntry {
            name: name.into(),
            method: 0,
            flags: 0,
            crc: 0,
            compressed: 0,
            size: 0,
            local: 0,
            made_by: 19 << 8,
            external: 0o100644 << 16,
            dos_time: 0,
            unix_mtime: None,
        };
        assert!(matches!(role(&e("__MACOSX/a/b/._x")), Role::AppleDouble(t) if t == "a/b/x"));
        assert!(matches!(role(&e("__MACOSX/._top")), Role::AppleDouble(t) if t == "top"));
        assert!(matches!(role(&e("a/._x")), Role::AppleDouble(t) if t == "a/x"));
        assert!(matches!(role(&e("a/._")), Role::File));
        assert!(matches!(role(&e("__MACOSX/a/")), Role::Skip));
    }
}
