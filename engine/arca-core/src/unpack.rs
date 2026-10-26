//! Reading, verifying and extracting archives.
//!
//! Extraction decodes blocks in parallel waves. Plain objects are streamed
//! chunk-by-chunk straight into their files (so a 50 GB video never sits in
//! memory); transformed objects are rebuilt whole once all their blocks are in.
//! Every block is checked against BLAKE3 hashes of its compressed and decoded
//! bytes before anything is written, and every transformed object is checked
//! against the hash of its original content.

use crate::codec;
use crate::format::{self, BlockInfo, Entry, EntryKind, Footer, Index, Transform, FOOTER_LEN, HEADER_LEN};
use crate::transform;
use anyhow::{bail, ensure, Context, Result};
use rayon::prelude::*;
use std::collections::{BTreeSet, HashMap};
use std::fs::{File, OpenOptions};
use std::os::unix::fs::{FileExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub struct Archive {
    file: File,
    pub index: Index,
    pub archive_bytes: u64,
}

#[derive(Debug, Default, Clone)]
pub struct ExtractOptions {
    pub threads: usize,
    pub overwrite: bool,
    /// Extract only entries whose path equals or lies under one of these.
    pub only: Vec<String>,
}

#[derive(Debug, Default, Clone)]
pub struct ExtractStats {
    pub files: u64,
    pub bytes: u64,
    pub blocks: u64,
    pub seconds: f64,
}

impl Archive {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
        let len = file.metadata()?.len();
        ensure!(len >= (HEADER_LEN + FOOTER_LEN) as u64, "file too small to be an Arca archive");
        let mut head = [0u8; HEADER_LEN];
        file.read_exact_at(&mut head, 0)?;
        format::check_header(&head)?;
        let mut foot = [0u8; FOOTER_LEN];
        file.read_exact_at(&mut foot, len - FOOTER_LEN as u64)?;
        let footer = Footer::parse(&foot)?;
        ensure!(
            footer.index_offset.checked_add(footer.index_comp_len) == Some(len - FOOTER_LEN as u64),
            "index location is inconsistent"
        );
        ensure!(footer.index_raw_len <= 4 << 30, "index too large");
        let mut comp = vec![0u8; footer.index_comp_len as usize];
        file.read_exact_at(&mut comp, footer.index_offset)?;
        ensure!(format::hash16(&comp) == footer.index_hash, "index checksum mismatch: archive is damaged");
        let raw = zstd::bulk::decompress(&comp, footer.index_raw_len as usize)?;
        let index = Index::decode(&raw)?;
        for b in &index.blocks {
            ensure!(
                b.file_offset >= HEADER_LEN as u64 && b.file_offset + b.comp_len <= footer.index_offset,
                "block outside archive body"
            );
        }
        Ok(Archive { file, index, archive_bytes: len })
    }

    pub fn read_block(&self, id: u32) -> Result<Vec<u8>> {
        let b: &BlockInfo = &self.index.blocks[id as usize];
        let mut comp = vec![0u8; b.comp_len as usize];
        self.file.read_exact_at(&mut comp, b.file_offset)?;
        ensure!(format::hash16(&comp) == b.checksum, "block {id} is damaged (checksum mismatch)");
        let raw = codec::decompress(&comp, b.codec, b.raw_len).with_context(|| format!("block {id}"))?;
        ensure!(format::hash16(&raw) == b.raw_hash, "block {id} decoded to the wrong content");
        Ok(raw)
    }

    /// Decodes everything and checks every hash without writing anything.
    pub fn verify(&self, threads: usize) -> Result<ExtractStats> {
        self.run(None, &ExtractOptions { threads, ..Default::default() })
    }

    pub fn extract(&self, dest: &Path, opts: &ExtractOptions) -> Result<ExtractStats> {
        std::fs::create_dir_all(dest)?;
        self.run(Some(dest), opts)
    }

    fn selected(&self, opts: &ExtractOptions) -> Vec<bool> {
        let entries = &self.index.entries;
        let mut sel: Vec<bool> = entries
            .iter()
            .map(|e| {
                opts.only.is_empty()
                    || opts.only.iter().any(|p| {
                        let p = p.trim_end_matches('/');
                        e.path == p || e.path.starts_with(&format!("{p}/"))
                    })
            })
            .collect();
        // Hard links need their target's content.
        for (i, e) in entries.iter().enumerate() {
            if let (true, EntryKind::HardLink { target }) = (sel[i], &e.kind) {
                sel[*target as usize] = true;
            }
        }
        sel
    }

    fn run(&self, dest: Option<&Path>, opts: &ExtractOptions) -> Result<ExtractStats> {
        let start = Instant::now();
        let idx = &self.index;
        let threads = if opts.threads == 0 { std::thread::available_parallelism().map_or(4, |n| n.get()) } else { opts.threads };
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build()?;
        let sel = self.selected(opts);

        // Resolve and validate every output path before touching the disk.
        let mut targets: Vec<Option<PathBuf>> = vec![None; idx.entries.len()];
        if let Some(dest) = dest {
            let symlinks: std::collections::HashSet<&str> = idx
                .entries
                .iter()
                .filter(|e| matches!(e.kind, EntryKind::Symlink { .. }))
                .map(|e| e.path.as_str())
                .collect();
            for (i, e) in idx.entries.iter().enumerate() {
                if !sel[i] {
                    continue;
                }
                let p = safe_join(dest, &e.path)?;
                // Refuse to write through a symlink the archive itself creates.
                let mut parent = e.path.as_str();
                while let Some((head, _)) = parent.rsplit_once('/') {
                    ensure!(!symlinks.contains(head), "entry {} lies beneath a symlink", e.path);
                    parent = head;
                }
                if p.symlink_metadata().is_ok() && !matches!(e.kind, EntryKind::Dir) {
                    if opts.overwrite {
                        std::fs::remove_file(&p).or_else(|_| std::fs::remove_dir_all(&p))?;
                    } else {
                        bail!("{} already exists", p.display());
                    }
                }
                targets[i] = Some(p);
            }
            // Each directory once, parents before children.
            let mut dirs: BTreeSet<&Path> = BTreeSet::new();
            for (i, e) in idx.entries.iter().enumerate() {
                let Some(p) = &targets[i] else { continue };
                if matches!(e.kind, EntryKind::Dir) {
                    dirs.insert(p);
                } else if let Some(parent) = p.parent() {
                    dirs.insert(parent);
                }
            }
            for d in dirs {
                std::fs::create_dir_all(d)?;
            }
        }

        // Objects to rebuild, and the entries that receive each one.
        let mut receivers: HashMap<u32, Vec<usize>> = HashMap::new();
        for (i, e) in idx.entries.iter().enumerate() {
            if let (true, EntryKind::File { object, .. }) = (sel[i], &e.kind) {
                receivers.entry(*object).or_default().push(i);
            }
        }
        let mut objs: Vec<u32> = receivers.keys().copied().collect();
        objs.sort_unstable();

        // Work units per block, and how many units still need each block.
        let nblocks = idx.blocks.len();
        let mut refs = vec![0u32; nblocks];
        let mut stream_units: Vec<Vec<(u32, u64, u32)>> = vec![Vec::new(); nblocks]; // (object, file offset, chunk)
        let mut whole_ready: Vec<Vec<u32>> = vec![Vec::new(); nblocks];
        for &o in &objs {
            let obj = &idx.objects[o as usize];
            if obj.transform == Transform::None {
                let mut off = 0u64;
                for &c in &obj.chunks {
                    let ch = &idx.chunks[c as usize];
                    stream_units[ch.block as usize].push((o, off, c));
                    refs[ch.block as usize] += 1;
                    off += ch.len;
                }
            } else {
                let bs = blocks_of(idx, o);
                for &b in &bs {
                    refs[b as usize] += 1;
                }
                if let Some(&last) = bs.last() {
                    whole_ready[last as usize].push(o);
                }
            }
        }

        // Most files are a single chunk: they are created, written and given their
        // metadata through one file descriptor the moment their block decodes.
        // Only multi-chunk files are created up front for out-of-order writes.
        let primary = |o: u32| receivers[&o][0];
        let fd_done: Vec<AtomicBool> = (0..idx.entries.len()).map(|_| AtomicBool::new(false)).collect();
        let write_whole = |o: u32, content: &[u8]| -> Result<()> {
            let i = primary(o);
            let f = create_file(targets[i].as_ref().unwrap())?;
            f.write_all_at(content, 0)?;
            apply_metadata_fd(&f, &idx.entries[i])?;
            fd_done[i].store(true, Ordering::Relaxed);
            Ok(())
        };
        if dest.is_some() {
            pool.install(|| {
                objs.par_iter()
                    .filter(|&&o| idx.objects[o as usize].transform == Transform::None && idx.objects[o as usize].chunks.len() > 1)
                    .try_for_each(|&o| -> Result<()> {
                        let f = create_file(targets[primary(o)].as_ref().unwrap())?;
                        f.set_len(idx.objects[o as usize].size)?;
                        Ok(())
                    })
            })?;
        }

        let bytes = AtomicU64::new(0);
        let process = |wave: &[u32], loaded: &HashMap<u32, Arc<Vec<u8>>>| -> Result<()> {
            let units: Vec<(u32, u64, u32)> = wave.iter().flat_map(|&b| stream_units[b as usize].iter().copied()).collect();
            let wholes: Vec<u32> = wave.iter().flat_map(|&b| whole_ready[b as usize].iter().copied()).collect();
            units.par_iter().with_max_len(16).try_for_each(|&(o, off, c)| -> Result<()> {
                let ch = &idx.chunks[c as usize];
                let data = &loaded[&ch.block][ch.offset as usize..(ch.offset + ch.len) as usize];
                if dest.is_some() {
                    if idx.objects[o as usize].chunks.len() == 1 {
                        write_whole(o, data)?;
                    } else {
                        let p = targets[primary(o)].as_ref().unwrap();
                        let f = OpenOptions::new().write(true).custom_flags(libc::O_NOFOLLOW).open(p)?;
                        f.write_all_at(data, off)?;
                    }
                }
                bytes.fetch_add(ch.len, Ordering::Relaxed);
                Ok(())
            })?;
            wholes.par_iter().with_max_len(1).try_for_each(|&o| -> Result<()> {
                let obj = &idx.objects[o as usize];
                let mut stored = Vec::with_capacity(obj.stored_len as usize);
                for &c in &obj.chunks {
                    let ch = &idx.chunks[c as usize];
                    stored.extend_from_slice(&loaded[&ch.block][ch.offset as usize..(ch.offset + ch.len) as usize]);
                }
                let content = transform::inverse(stored, &obj.transform, obj.size)?;
                ensure!(Some(format::hash16(&content)) == obj.hash, "object {o} failed verification");
                if dest.is_some() {
                    write_whole(o, &content)?;
                }
                bytes.fetch_add(obj.size, Ordering::Relaxed);
                Ok(())
            })
        };
        let decode = |wave: &[u32]| -> Result<Vec<(u32, Vec<u8>)>> {
            wave.par_iter().with_max_len(1).map(|&b| Ok((b, self.read_block(b)?))).collect()
        };

        // Decode wave N+1 while wave N's files are rebuilt and written.
        let wanted: Vec<u32> = (0..nblocks as u32).filter(|&b| refs[b as usize] > 0).collect();
        let waves: Vec<&[u32]> = wanted.chunks(threads.max(1)).collect();
        let mut loaded: HashMap<u32, Arc<Vec<u8>>> = HashMap::new();
        let mut fresh = match waves.first() {
            Some(w) => pool.install(|| decode(w))?,
            None => Vec::new(),
        };
        for (wi, wave) in waves.iter().enumerate() {
            for (b, data) in fresh.drain(..) {
                loaded.insert(b, Arc::new(data));
            }
            let (next, done) = pool.install(|| {
                rayon::join(|| waves.get(wi + 1).map(|w| decode(w)).transpose(), || process(wave, &loaded))
            });
            done?;
            fresh = next?.unwrap_or_default();

            // Release blocks no remaining unit needs.
            for &b in wave.iter() {
                for &(_, _, c) in &stream_units[b as usize] {
                    refs[idx.chunks[c as usize].block as usize] -= 1;
                }
                for &o in &whole_ready[b as usize] {
                    for blk in blocks_of(idx, o) {
                        refs[blk as usize] -= 1;
                    }
                }
            }
            loaded.retain(|b, _| refs[*b as usize] > 0);
        }

        let mut stats = ExtractStats { files: 0, bytes: bytes.load(Ordering::Relaxed), blocks: wanted.len() as u64, seconds: 0.0 };
        if dest.is_some() {
            // Empty files have no chunks to trigger them.
            pool.install(|| {
                objs.par_iter().filter(|&&o| idx.objects[o as usize].chunks.is_empty()).try_for_each(|&o| write_whole(o, &[]))
            })?;
            // Duplicate content: APFS clones cost no space and no time.
            let copies: Vec<(usize, usize)> =
                receivers.values().flat_map(|r| r[1..].iter().map(move |&i| (r[0], i))).collect();
            pool.install(|| {
                copies.par_iter().try_for_each(|&(from, to)| {
                    clone_or_copy(targets[from].as_ref().unwrap(), targets[to].as_ref().unwrap())
                })
            })?;
            for (i, e) in idx.entries.iter().enumerate() {
                let Some(p) = &targets[i] else { continue };
                match &e.kind {
                    EntryKind::HardLink { target } => std::fs::hard_link(targets[*target as usize].as_ref().unwrap(), p)?,
                    EntryKind::Symlink { target } => {
                        use std::os::unix::ffi::OsStrExt;
                        std::os::unix::fs::symlink(std::ffi::OsStr::from_bytes(target), p)?
                    }
                    _ => {}
                }
            }
            // Remaining metadata by path; directories deepest-first so child writes
            // don't bump their mtime.
            pool.install(|| {
                idx.entries
                    .par_iter()
                    .enumerate()
                    .filter(|(i, e)| {
                        targets[*i].is_some() && !matches!(e.kind, EntryKind::Dir) && !fd_done[*i].load(Ordering::Relaxed)
                    })
                    .try_for_each(|(i, e)| apply_metadata(targets[i].as_ref().unwrap(), e))
            })?;
            let mut dirs: Vec<usize> =
                (0..idx.entries.len()).filter(|&i| targets[i].is_some() && matches!(idx.entries[i].kind, EntryKind::Dir)).collect();
            dirs.sort_by_key(|&i| std::cmp::Reverse(idx.entries[i].path.matches('/').count()));
            for i in dirs {
                apply_metadata(targets[i].as_ref().unwrap(), &idx.entries[i])?;
            }
        }
        stats.files = idx.entries.iter().enumerate().filter(|(i, e)| sel[*i] && matches!(e.kind, EntryKind::File { .. })).count() as u64;
        stats.seconds = start.elapsed().as_secs_f64();
        Ok(stats)
    }
}

/// Distinct blocks an object's chunks live in, ascending.
fn blocks_of(idx: &Index, o: u32) -> Vec<u32> {
    let mut bs: Vec<u32> = idx.objects[o as usize].chunks.iter().map(|&c| idx.chunks[c as usize].block).collect();
    bs.sort_unstable();
    bs.dedup();
    bs
}

fn create_file(p: &Path) -> Result<File> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .custom_flags(libc::O_NOFOLLOW)
        .open(p)
        .with_context(|| format!("creating {}", p.display()))
}

/// Joins an archive path onto `dest`, rejecting anything that could escape it.
fn safe_join(dest: &Path, rel: &str) -> Result<PathBuf> {
    let rp = Path::new(rel);
    ensure!(!rel.is_empty() && !rel.contains('\0'), "invalid entry path");
    for c in rp.components() {
        match c {
            Component::Normal(_) => {}
            _ => bail!("unsafe entry path {rel:?}"),
        }
    }
    Ok(dest.join(rp))
}

/// `<sys/clonefile.h>`: don't follow a symlink at `from`.
const CLONE_NOFOLLOW: u32 = 0x0001;

fn clone_or_copy(from: &Path, to: &Path) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let a = std::ffi::CString::new(from.as_os_str().as_bytes())?;
    let b = std::ffi::CString::new(to.as_os_str().as_bytes())?;
    // SAFETY: both are valid NUL-terminated paths; clonefile does not retain them.
    let rc = unsafe { libc::clonefile(a.as_ptr(), b.as_ptr(), CLONE_NOFOLLOW) };
    if rc != 0 {
        std::fs::copy(from, to)?;
    }
    Ok(())
}

fn mtime_of(e: &Entry) -> std::time::SystemTime {
    let (secs, nanos) = e.mtime;
    let base = std::time::UNIX_EPOCH;
    if secs >= 0 {
        base + std::time::Duration::new(secs as u64, nanos)
    } else {
        base - std::time::Duration::from_secs(secs.unsigned_abs()) + std::time::Duration::from_nanos(nanos as u64)
    }
}

/// Metadata through an open descriptor: no path lookups, no symlink races.
fn apply_metadata_fd(f: &File, e: &Entry) -> Result<()> {
    use xattr::FileExt as _;
    for (k, v) in &e.xattrs {
        let _ = f.set_xattr(k, v);
    }
    f.set_permissions(std::fs::Permissions::from_mode(e.mode))?;
    let t = mtime_of(e);
    f.set_times(std::fs::FileTimes::new().set_accessed(t).set_modified(t))?;
    Ok(())
}

fn apply_metadata(p: &Path, e: &Entry) -> Result<()> {
    let is_link = matches!(e.kind, EntryKind::Symlink { .. });
    // A clone inherits its source's attributes; keep only this entry's own.
    if matches!(e.kind, EntryKind::File { .. }) {
        if let Ok(names) = xattr::list(p) {
            for name in names {
                if !e.xattrs.iter().any(|(k, _)| std::ffi::OsStr::new(k) == name) {
                    let _ = xattr::remove(p, &name);
                }
            }
        }
    }
    for (k, v) in &e.xattrs {
        // Best effort: some attributes are not settable on every filesystem.
        let _ = xattr::set(p, k, v);
    }
    let mtime = filetime::FileTime::from_unix_time(e.mtime.0, e.mtime.1);
    if is_link {
        filetime::set_symlink_file_times(p, mtime, mtime)?;
    } else {
        if !matches!(e.kind, EntryKind::HardLink { .. }) {
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(e.mode))?;
        }
        filetime::set_file_times(p, mtime, mtime)?;
    }
    Ok(())
}
