//! Research tool: Lepton's encode and decode CPU over a folder of JPEGs.
//! `leptbench DIR` (files run in parallel, one thread each)
use lepton_jpeg::{decode_lepton, encode_lepton, EnabledFeatures, SingleThreadPool};
use rayon::prelude::*;
use std::io::Cursor;

fn cpu() -> f64 {
    let mut t = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut t) };
    t.tv_sec as f64 + t.tv_nsec as f64 / 1e9
}

fn walk(d: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(d).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("jpg") || x.eq_ignore_ascii_case("jpeg")) {
            out.push(p);
        }
    }
}

fn main() {
    let mut files = Vec::new();
    walk(std::path::Path::new(&std::env::args().nth(1).unwrap()), &mut files);
    let r: Vec<(usize, usize, f64, f64)> = files
        .par_iter()
        .filter_map(|f| {
            let data = std::fs::read(f).ok()?;
            let pool = SingleThreadPool::default();
            let mut feat = EnabledFeatures::compat_lepton_vector_write();
            feat.max_processor_threads = 1;
            let t = cpu();
            let mut enc = Vec::new();
            encode_lepton(&mut Cursor::new(&data), &mut Cursor::new(&mut enc), &feat, &pool).ok()?;
            let te = cpu() - t;
            let mut feat = EnabledFeatures::compat_lepton_vector_read();
            feat.max_processor_threads = 1;
            let t = cpu();
            let mut out = Vec::with_capacity(data.len());
            decode_lepton(&mut Cursor::new(&enc), &mut out, &feat, &pool).ok()?;
            let td = cpu() - t;
            assert_eq!(out, data);
            Some((data.len(), enc.len(), te, td))
        })
        .collect();
    let (a, b, te, td) = r.iter().fold((0, 0, 0.0, 0.0), |s, x| (s.0 + x.0, s.1 + x.1, s.2 + x.2, s.3 + x.3));
    println!("{} files {:.1} MB -> {:.1} MB, encode {te:.2} s ({:.1} MB/s), decode {td:.2} s ({:.1} MB/s)", r.len(), a as f64 / 1e6, b as f64 / 1e6, a as f64 / 1e6 / te, a as f64 / 1e6 / td);
}
