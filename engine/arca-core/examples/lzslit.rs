//! Times the lzs literal path alone: a stream with no matches, bytes drawn from a
//! skewed distribution so the models have real work.
//!
//!     cargo run --release --example lzslit -- [MiB]

use arca_core::lzs;
use std::time::Instant;

fn main() {
    let mib: usize = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(32);
    let n = mib << 20;
    let mut s = 0x9E3779B97F4A7C15u64;
    let mut data = Vec::with_capacity(n);
    let mut prev = 0u8;
    while data.len() < n {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        // text-like: a byte depends a little on the previous one, order-0 skewed
        let r = (s >> 32) as u32;
        let b = if r & 3 == 0 { prev.wrapping_add((r >> 8) as u8 & 7) } else { 97 + ((r >> 8) % 26) as u8 };
        data.push(b);
        prev = b;
    }
    let enc = lzs::encode(&data, &[]);
    let t = Instant::now();
    let rounds = 3;
    let mut out = Vec::new();
    for _ in 0..rounds {
        out = lzs::decode(&enc, data.len()).unwrap();
    }
    let dt = t.elapsed().as_secs_f64() / rounds as f64;
    assert_eq!(out, data);
    let ghz = 3.5;
    println!(
        "{} MiB literals: {:.3} -> {:.3} MB ({:.2} bits/byte), decode {:.0} MB/s = {:.1} cycles/byte at {ghz} GHz",
        mib,
        n as f64 / 1e6,
        enc.len() as f64 / 1e6,
        enc.len() as f64 * 8.0 / n as f64,
        n as f64 / 1e6 / dt,
        ghz * 1e9 * dt / n as f64
    );
}
