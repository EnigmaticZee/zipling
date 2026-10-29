//! Research tool: preflate's forward and inverse CPU per file at Default's settings.
//! `preftime LIST [chain]` where LIST has one `path<TAB>group` per line; prints totals per group.
use arca_core::classify::{classify, Class};
use arca_core::transform::{deflate, inverse};
use arca_core::Transform;
use std::collections::BTreeMap;

fn cpu() -> f64 {
    let mut t = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut t) };
    t.tv_sec as f64 + t.tv_nsec as f64 / 1e9
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let chain: u32 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(256);
    let mut g: BTreeMap<String, (usize, usize, usize, f64, f64)> = BTreeMap::new();
    for line in std::fs::read_to_string(&a[1]).unwrap().lines() {
        let (path, group) = line.split_once('\t').unwrap();
        let data = std::fs::read(path).unwrap();
        let Class::DeflateContainer(kind) = classify(&data[..data.len().min(4096)]) else { continue };
        let t = cpu();
        let out = deflate::encode(&data, kind, chain, 128, false, None).map(|e| e.stored);
        let tf = cpu() - t;
        let e = g.entry(group.to_owned()).or_default();
        e.0 += 1;
        e.3 += tf;
        if let Some(out) = out {
            let t = cpu();
            let back = inverse(out, &Transform::Deflate, data.len() as u64).unwrap();
            e.4 += cpu() - t;
            assert_eq!(back, data);
            e.1 += 1;
            e.2 += data.len();
        }
    }
    for (k, (n, ok, bytes, tf, ti)) in g {
        println!("{k}: {n} files, {ok} expanded ({:.1} MB), forward {tf:.2} s, inverse {ti:.2} s", bytes as f64 / 1e6);
    }
}
