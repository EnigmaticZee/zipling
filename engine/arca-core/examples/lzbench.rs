//! Research tool: size and CPU of LZMA SDK settings on dumped blocks.
//! `lzbench BLOCK...` (one thread per encoder; blocks run in parallel)
use rayon::prelude::*;

fn cpu() -> f64 {
    let mut t = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut t) };
    t.tv_sec as f64 + t.tv_nsec as f64 / 1e9
}

fn main() {
    let files: Vec<String> = std::env::args().skip(1).collect();
    let settings: &[(&str, i32, i32, u32)] = &[
        ("fb273", 9, 273, 0),
        ("fb128", 9, 128, 0),
        ("fb64", 9, 64, 0),
        ("fb48", 9, 48, 0),
        ("fb32", 9, 32, 0),
        ("fb64 mc16", 9, 64, 16),
        ("fb273 mc32", 9, 273, 32),
    ];
    let rows: Vec<String> = files
        .par_iter()
        .flat_map(|f| {
            let data = std::fs::read(f).unwrap();
            settings
                .par_iter()
                .map(|&(name, level, fb, mc)| {
                    let s = lzma_sdk::Settings { level, dict_size: 1 << 26, fb, mc, threads: 1 };
                    let t = cpu();
                    let (c, _) = lzma_sdk::encode(&data, s, data.len()).unwrap();
                    format!("{}\t{name}\t{}\t{:.2}", f.rsplit('/').next().unwrap(), c.len(), cpu() - t)
                })
                .collect::<Vec<_>>()
        })
        .collect();
    for r in rows {
        println!("{r}");
    }
}
