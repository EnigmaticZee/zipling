//! Research tool: where extraction's kernel time goes. `writebench SRC_TREE OUT MODE`
//! Loads every file of SRC_TREE into memory, then recreates them under OUT on
//! all cores. MODE: empty | data | meta (data + fchmod + futimes) | dirs (mkdir only)
use rayon::prelude::*;
use std::os::unix::fs::{FileExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

fn walk(root: &Path, d: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>, dirs: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(d).unwrap().flatten() {
        let p = e.path();
        let ft = e.file_type().unwrap();
        if ft.is_dir() {
            dirs.push(p.strip_prefix(root).unwrap().to_owned());
            walk(root, &p, out, dirs);
        } else if ft.is_file() {
            out.push((p.strip_prefix(root).unwrap().to_owned(), std::fs::read(&p).unwrap()));
        }
    }
}

fn times() -> (f64, f64) {
    let mut r: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut r) };
    let f = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 / 1e6;
    (f(r.ru_utime), f(r.ru_stime))
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (src, out, mode) = (PathBuf::from(&a[1]), PathBuf::from(&a[2]), a[3].as_str());
    let (mut files, mut dirs) = (Vec::new(), Vec::new());
    walk(&src, &src, &mut files, &mut dirs);
    let bytes: usize = files.iter().map(|f| f.1.len()).sum();
    let (u0, s0) = times();
    let t = std::time::Instant::now();
    std::fs::create_dir_all(&out).unwrap();
    for d in &dirs {
        std::fs::create_dir_all(out.join(d)).unwrap();
    }
    let td = t.elapsed().as_secs_f64();
    let (u1, s1) = times();
    if mode == "openat" {
        // Files grouped by directory; each group opens its directory once.
        let mut groups: std::collections::BTreeMap<PathBuf, Vec<usize>> = Default::default();
        for (i, (p, _)) in files.iter().enumerate() {
            groups.entry(p.parent().unwrap().to_owned()).or_default().push(i);
        }
        let groups: Vec<_> = groups.into_iter().collect();
        groups.par_iter().for_each(|(dir, idx)| {
            use std::os::unix::ffi::OsStrExt;
            let dp = std::ffi::CString::new(out.join(dir).as_os_str().as_bytes()).unwrap();
            let dfd = unsafe { libc::open(dp.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY) };
            assert!(dfd >= 0);
            for &i in idx {
                let (p, data) = &files[i];
                let name = std::ffi::CString::new(p.file_name().unwrap().as_bytes()).unwrap();
                let fd = unsafe { libc::openat(dfd, name.as_ptr(), libc::O_WRONLY | libc::O_CREAT | libc::O_NOFOLLOW, 0o644) };
                assert!(fd >= 0);
                let f = unsafe { <std::fs::File as std::os::fd::FromRawFd>::from_raw_fd(fd) };
                f.write_all_at(data, 0).unwrap();
            }
            unsafe { libc::close(dfd) };
        });
    } else if mode.starts_with("gate") {
        // gate-open / gate-close / gate-all: at most GATE threads in that part.
        let cap: usize = std::env::var("GATE").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
        let gate = (std::sync::Mutex::new(0usize), std::sync::Condvar::new());
        let enter = || {
            let mut n = gate.0.lock().unwrap();
            while *n >= cap {
                n = gate.1.wait(n).unwrap();
            }
            *n += 1;
        };
        let leave = || {
            *gate.0.lock().unwrap() -= 1;
            gate.1.notify_one();
        };
        files.par_iter().for_each(|(p, data)| {
            let g_open = mode == "gate-open" || mode == "gate-all";
            let g_close = mode == "gate-close" || mode == "gate-all";
            if g_open {
                enter();
            }
            let f = std::fs::OpenOptions::new().write(true).create(true).custom_flags(libc::O_NOFOLLOW).open(out.join(p)).unwrap();
            if g_open && !g_close {
                leave();
            }
            f.write_all_at(data, 0).unwrap();
            if g_close && !g_open {
                enter();
            }
            drop(f);
            if g_close {
                leave();
            }
        });
    } else if mode != "dirs" {
        files.par_iter().for_each(|(p, data)| {
            let f = std::fs::OpenOptions::new().write(true).create(true).custom_flags(libc::O_NOFOLLOW).open(out.join(p)).unwrap();
            if mode != "empty" {
                f.write_all_at(data, 0).unwrap();
            }
            if mode == "meta" {
                f.set_permissions(std::fs::Permissions::from_mode(0o644)).unwrap();
                let t = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
                f.set_times(std::fs::FileTimes::new().set_accessed(t).set_modified(t)).unwrap();
            }
        });
    }
    let tf = t.elapsed().as_secs_f64() - td;
    let (u2, s2) = times();
    println!(
        "{mode}: {} files ({:.0} MB), {} dirs | mkdir {td:.2} s (sys {:.2}) | files {tf:.2} s (user {:.2}, sys {:.2})",
        files.len(),
        bytes as f64 / 1e6,
        dirs.len(),
        s1 - s0,
        u2 - u1,
        s2 - s1
    );
    let _ = u0;
}
