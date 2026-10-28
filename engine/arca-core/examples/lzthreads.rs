//! Research tool: wall and process CPU time of one LZMA SDK encode with one or
//! two match-finder threads. `lzthreads BLOCK FB`
fn proc_cpu() -> f64 {
    let mut r: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut r) };
    r.ru_utime.tv_sec as f64 + r.ru_utime.tv_usec as f64 / 1e6 + r.ru_stime.tv_sec as f64 + r.ru_stime.tv_usec as f64 / 1e6
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).unwrap();
    let fb: i32 = a[2].parse().unwrap();
    for threads in [1, 2, 1, 2] {
        let (c0, t0) = (proc_cpu(), std::time::Instant::now());
        let (c, _) = lzma_sdk::encode(&data, lzma_sdk::Settings { level: 9, dict_size: 1 << 26, fb, mc: 0, threads }, data.len()).unwrap();
        println!("threads {threads}: {} bytes, wall {:.2} s, CPU {:.2} s", c.len(), t0.elapsed().as_secs_f64(), proc_cpu() - c0);
    }
}
