//! Filesystem walk: builds archive entries with macOS metadata (mode, mtime,
//! extended attributes such as resource forks and Finder info, hard links).
//!
//! Directories are read in parallel (each directory's children on the pool),
//! then flattened in the same depth-first, name-sorted order a sequential walk
//! would produce, so archives stay deterministic.

use crate::format::{Entry, EntryKind};
use anyhow::{Context, Result};
use rayon::prelude::*;
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

/// One walked item, before hard links are resolved.
struct Node {
    entry: Entry,
    path: PathBuf,
    /// (device, inode) for regular files with more than one link.
    link_key: Option<(u64, u64)>,
    children: Vec<Node>,
}

/// Walks `inputs`. Runs on the current rayon pool (callers install theirs).
pub fn scan(inputs: &[PathBuf]) -> Result<Vec<Scanned>> {
    let mut roots = Vec::with_capacity(inputs.len());
    for input in inputs {
        let meta = std::fs::symlink_metadata(input).with_context(|| format!("reading {}", input.display()))?;
        let name = input
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "root".to_owned());
        roots.push(visit(input.clone(), name, &meta)?);
    }
    let mut out = Vec::new();
    let mut inodes: HashMap<(u64, u64), u32> = HashMap::new();
    for root in roots.into_iter().flatten() {
        flatten(root, &mut out, &mut inodes);
    }
    Ok(out)
}

/// Pre-order, children in name order; the first path of a hard-linked inode
/// holds the content and later ones link to it.
fn flatten(node: Node, out: &mut Vec<Scanned>, inodes: &mut HashMap<(u64, u64), u32>) {
    let Node { mut entry, path, link_key, children } = node;
    let mut source = matches!(entry.kind, EntryKind::File { .. }).then_some(path);
    if let Some(key) = link_key {
        match inodes.get(&key) {
            Some(&first) => {
                entry.kind = EntryKind::HardLink { target: first };
                source = None;
            }
            None => {
                inodes.insert(key, out.len() as u32);
            }
        }
    }
    out.push(Scanned { entry, source });
    for c in children {
        flatten(c, out, inodes);
    }
}

/// Builds the node for `path` (and, for a directory, everything beneath it).
/// `None` for sockets, FIFOs and devices, which are skipped like Archive Utility does.
fn visit(path: PathBuf, rel: String, meta: &std::fs::Metadata) -> Result<Option<Node>> {
    let ft = meta.file_type();
    let mtime = (meta.mtime(), meta.mtime_nsec() as u32);
    let mode = meta.mode() & 0o7777;
    let xattrs = read_xattrs(&path);
    let mk = |kind| Entry { path: rel.clone(), kind, mode, mtime, xattrs: xattrs.clone() };

    let node = if ft.is_symlink() {
        let target = std::fs::read_link(&path)?;
        use std::os::unix::ffi::OsStrExt;
        let kind = EntryKind::Symlink { target: target.as_os_str().as_bytes().to_vec() };
        Node { entry: mk(kind), path, link_key: None, children: Vec::new() }
    } else if ft.is_dir() {
        let mut kids: Vec<_> = std::fs::read_dir(&path)?.collect::<std::io::Result<_>>()?;
        kids.sort_by_key(|c| c.file_name());
        let children = kids
            .par_iter()
            .with_max_len(1)
            .map(|c| -> Result<Option<Node>> {
                let name = c.file_name().to_string_lossy().into_owned();
                let child = c.path();
                let child_meta = std::fs::symlink_metadata(&child)?;
                visit(child, format!("{rel}/{name}"), &child_meta)
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect();
        Node { entry: mk(EntryKind::Dir), path, link_key: None, children }
    } else if ft.is_file() {
        let link_key = (meta.nlink() > 1).then(|| (meta.dev(), meta.ino()));
        Node { entry: mk(EntryKind::File { size: meta.len(), object: 0 }), path, link_key, children: Vec::new() }
    } else {
        return Ok(None);
    };
    Ok(Some(node))
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
