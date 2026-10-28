//! Research tool: compares two extracted trees file by file: type, contents,
//! permissions, modification time (seconds), BSD flags, extended attributes and
//! link targets. `treecmp A B [--no-mtime]` prints differences, then a summary.
use std::collections::BTreeMap;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

fn walk(root: &Path, rel: &str, out: &mut BTreeMap<String, String>) {
    let p = if rel.is_empty() { root.to_path_buf() } else { root.join(rel) };
    let m = std::fs::symlink_metadata(&p).unwrap();
    let mut xa: Vec<(String, Vec<u8>)> = xattr::list(&p)
        .map(|l| l.filter_map(|n| xattr::get(&p, &n).ok().flatten().map(|v| (n.to_string_lossy().into_owned(), v))).collect())
        .unwrap_or_default();
    xa.retain(|(n, _)| n != "com.apple.provenance" && n != "com.apple.quarantine");
    xa.sort();
    let xs: Vec<String> = xa.iter().map(|(n, v)| format!("{n}={:x}", md5ish(v))).collect();
    let kind = if m.file_type().is_symlink() {
        format!("link->{}", std::fs::read_link(&p).unwrap().display())
    } else if m.is_dir() {
        "dir".into()
    } else {
        format!("file {} {:x}", m.len(), md5ish(&std::fs::read(&p).unwrap()))
    };
    let mtime = if std::env::args().any(|a| a == "--no-mtime") || m.is_dir() { 0 } else { m.mtime() };
    out.insert(rel.to_owned(), format!("{kind} mode {:o} mtime {mtime} flags {:x} xattrs [{}]", m.mode() & 0o7777, std::os::macos::fs::MetadataExt::st_flags(&m), xs.join(" ")));
    if m.is_dir() && !m.file_type().is_symlink() {
        for c in std::fs::read_dir(&p).unwrap() {
            let n = c.unwrap().file_name().to_string_lossy().into_owned();
            walk(root, &if rel.is_empty() { n } else { format!("{rel}/{n}") }, out);
        }
    }
}

fn md5ish(v: &[u8]) -> u64 {
    // FNV-1a: enough to spot differences.
    v.iter().fold(0xcbf29ce484222325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

fn main() {
    let a: Vec<String> = std::env::args().filter(|a| !a.starts_with("--")).collect();
    let (mut x, mut y) = (BTreeMap::new(), BTreeMap::new());
    walk(Path::new(&a[1]), "", &mut x);
    walk(Path::new(&a[2]), "", &mut y);
    let mut diffs = 0;
    for (k, v) in &x {
        match y.get(k) {
            None => {
                diffs += 1;
                if diffs <= 15 {
                    println!("only in A: {k}");
                }
            }
            Some(w) if w != v => {
                diffs += 1;
                if diffs <= 15 {
                    println!("differs: {k}\n  A {v}\n  B {w}");
                }
            }
            _ => {}
        }
    }
    for k in y.keys() {
        if !x.contains_key(k) {
            diffs += 1;
            if diffs <= 15 {
                println!("only in B: {k}");
            }
        }
    }
    println!("{} items in A, {} in B, {diffs} differences", x.len(), y.len());
}
