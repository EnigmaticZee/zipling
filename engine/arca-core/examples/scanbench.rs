//! Research tool: the folder scan, single-threaded (as `scan::scan` does it) and
//! in parallel over directories. `scanbench DIR`
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn visit_par(dir: &Path) -> usize {
    let mut kids: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap()).collect();
    kids.sort_by_key(|e| e.file_name());
    kids.par_iter()
        .map(|c| {
            let p = c.path();
            let m = std::fs::symlink_metadata(&p).unwrap();
            let _ = xattr::list(&p).map(|l| l.count());
            1 + if m.is_dir() { visit_par(&p) } else { 0 }
        })
        .sum()
}

fn main() {
    let root = PathBuf::from(std::env::args().nth(1).unwrap());
    for _ in 0..2 {
        let t = Instant::now();
        let n = arca_core::scan::scan(&[root.clone()]).unwrap().len();
        println!("scan::scan        {n} entries {:.3} s", t.elapsed().as_secs_f64());
        for threads in [2, 4, 10] {
            let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
            let t = Instant::now();
            let n = pool.install(|| visit_par(&root));
            println!("parallel {threads:>2} thr  {n} entries {:.3} s", t.elapsed().as_secs_f64());
        }
    }
}
