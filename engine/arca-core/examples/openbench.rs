//! Research tool: how fast can we open and read the head of every file?
//! `openbench DIR` tries full paths and directory-relative opens at several
//! thread counts.
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn walk(dir: &Path, files: &mut Vec<(PathBuf, usize)>, dirs: &mut Vec<PathBuf>) {
    let d = dirs.len();
    dirs.push(dir.to_owned());
    let mut v: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap()).collect();
    v.sort_by_key(|e| e.file_name());
    for e in v {
        let ft = e.file_type().unwrap();
        if ft.is_dir() {
            walk(&e.path(), files, dirs);
        } else if ft.is_file() {
            files.push((e.path(), d));
        }
    }
}

fn main() {
    let root = PathBuf::from(std::env::args().nth(1).unwrap());
    let (mut files, mut dirs) = (Vec::new(), Vec::new());
    walk(&root, &mut files, &mut dirs);
    println!("{} files in {} dirs", files.len(), dirs.len());
    for threads in [1, 2, 4, 8, 10] {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        // Full paths.
        let t = Instant::now();
        pool.install(|| {
            use rayon::prelude::*;
            files.par_iter().with_max_len(64).for_each(|(p, _)| {
                let c = CString::new(p.as_os_str().as_bytes()).unwrap();
                let mut buf = [0u8; 4096];
                unsafe {
                    let fd = libc::open(c.as_ptr(), libc::O_RDONLY);
                    libc::read(fd, buf.as_mut_ptr().cast(), 4096);
                    libc::close(fd);
                }
            });
        });
        let full = t.elapsed().as_secs_f64();
        // Relative to an open directory, one directory per task.
        let t = Instant::now();
        let mut by_dir: Vec<Vec<&Path>> = vec![Vec::new(); dirs.len()];
        for (p, d) in &files {
            by_dir[*d].push(p);
        }
        pool.install(|| {
            use rayon::prelude::*;
            by_dir.par_iter().enumerate().for_each(|(d, ps)| {
                if ps.is_empty() {
                    return;
                }
                let dc = CString::new(dirs[d].as_os_str().as_bytes()).unwrap();
                let dfd = unsafe { libc::open(dc.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY) };
                let mut buf = [0u8; 4096];
                for p in ps {
                    let c = CString::new(p.file_name().unwrap().as_bytes()).unwrap();
                    unsafe {
                        let fd = libc::openat(dfd, c.as_ptr(), libc::O_RDONLY);
                        libc::read(fd, buf.as_mut_ptr().cast(), 4096);
                        libc::close(fd);
                    }
                }
                unsafe { libc::close(dfd) };
            });
        });
        let rel = t.elapsed().as_secs_f64();
        println!("{threads:>2} threads: full path {full:.3} s, dir-relative {rel:.3} s");
    }
}
