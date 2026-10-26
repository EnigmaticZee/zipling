//! Filesystem walk: builds archive entries with macOS metadata (mode, mtime,
//! extended attributes such as resource forks and Finder info, hard links).

use crate::format::{Entry, EntryKind};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// Attributes that describe this machine rather than the content.
const SKIPPED_XATTRS: &[&str] = &["com.apple.quarantine", "com.apple.provenance", "com.apple.lastuseddate#PS"];

pub struct Scanned {
    pub entry: Entry,
    /// Source path for regular files that need their content read.
    pub source: Option<PathBuf>,
}

pub fn scan(inputs: &[PathBuf]) -> Result<Vec<Scanned>> {
    let mut out = Vec::new();
    let mut inodes: HashMap<(u64, u64), u32> = HashMap::new();
    for input in inputs {
        let meta = std::fs::symlink_metadata(input).with_context(|| format!("reading {}", input.display()))?;
        let name = input
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "root".to_owned());
        visit(input, &name, &meta, &mut out, &mut inodes)?;
    }
    Ok(out)
}

fn visit(
    path: &Path,
    rel: &str,
    meta: &std::fs::Metadata,
    out: &mut Vec<Scanned>,
    inodes: &mut HashMap<(u64, u64), u32>,
) -> Result<()> {
    let ft = meta.file_type();
    let mtime = (meta.mtime(), meta.mtime_nsec() as u32);
    let mode = meta.mode() & 0o7777;
    let xattrs = read_xattrs(path);
    let mk = |kind| Entry { path: rel.to_owned(), kind, mode, mtime, xattrs: xattrs.clone() };

    if ft.is_symlink() {
        let target = std::fs::read_link(path)?;
        use std::os::unix::ffi::OsStrExt;
        out.push(Scanned { entry: mk(EntryKind::Symlink { target: target.as_os_str().as_bytes().to_vec() }), source: None });
    } else if ft.is_dir() {
        out.push(Scanned { entry: mk(EntryKind::Dir), source: None });
        let mut children: Vec<_> = std::fs::read_dir(path)?.collect::<std::io::Result<_>>()?;
        children.sort_by_key(|c| c.file_name());
        for c in children {
            let name = c.file_name().to_string_lossy().into_owned();
            let child_meta = std::fs::symlink_metadata(c.path())?;
            visit(&c.path(), &format!("{rel}/{name}"), &child_meta, out, inodes)?;
        }
    } else if ft.is_file() {
        if meta.nlink() > 1 {
            let key = (meta.dev(), meta.ino());
            if let Some(&first) = inodes.get(&key) {
                out.push(Scanned { entry: mk(EntryKind::HardLink { target: first }), source: None });
                return Ok(());
            }
            inodes.insert(key, out.len() as u32);
        }
        out.push(Scanned { entry: mk(EntryKind::File { size: meta.len(), object: 0 }), source: Some(path.to_owned()) });
    }
    // Sockets, FIFOs and devices are skipped, like Archive Utility does.
    Ok(())
}

fn read_xattrs(path: &Path) -> Vec<(String, Vec<u8>)> {
    let Ok(names) = xattr::list(path) else { return Vec::new() };
    let mut v = Vec::new();
    for name in names {
        let key = name.to_string_lossy().into_owned();
        if SKIPPED_XATTRS.contains(&key.as_str()) {
            continue;
        }
        if let Ok(Some(val)) = xattr::get(path, &name) {
            v.push((key, val));
        }
    }
    v.sort();
    v
}
