//! Research tool: one LZMA SDK encode of a block. `lzone BLOCK [FB] [MC] [THREADS]`
//! Prints size, CPU, wall, and checks decoding.
fn cpu() -> f64 {
    let mut r: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut r) };
    r.ru_utime.tv_sec as f64 + r.ru_utime.tv_usec as f64 / 1e6
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let d = std::fs::read(&a[1]).unwrap();
    let fb: i32 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(273);
    let mc: u32 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    let threads: i32 = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(1);
    let s = lzma_sdk::Settings { level: 9, dict_size: (d.len() as u32).clamp(1 << 16, 1 << 26), fb, mc, threads };
    let (t, w) = (cpu(), std::time::Instant::now());
    let (c, prop) = lzma_sdk::encode(&d, s, d.len() + d.len() / 2 + 4096).unwrap();
    let (tc, wc) = (cpu() - t, w.elapsed().as_secs_f64());
    let w = std::time::Instant::now();
    let back = lzma_sdk::decode(&c, prop, d.len()).unwrap();
    let wd = w.elapsed().as_secs_f64();
    assert!(back == d);
    println!(
        "{} fb{fb} mc{mc} t{threads}: {:.3} MB, {tc:.2} s CPU, {wc:.2} s wall, decode {wd:.2} s",
        a[1].rsplit('/').next().unwrap(),
        c.len() as f64 / 1e6
    );
}
