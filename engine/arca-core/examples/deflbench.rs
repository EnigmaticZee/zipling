//! Research tool: expands one Deflate container with Arca's transform, then
//! times its inverse. `deflbench FILE [chain] [reps]`
use arca_core::classify::{classify, Class};
use arca_core::transform::{deflate, inverse};
use arca_core::Transform;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&args[1]).unwrap();
    let chain: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4096);
    let reps: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1);
    let Class::DeflateContainer(kind) = classify(&data[..data.len().min(4096)]) else { panic!("not a deflate container") };
    let t = Instant::now();
    let out = deflate::encode(&data, kind, chain, false);
    let fwd = t.elapsed().as_secs_f64();
    let Some(out) = out else {
        println!("{} -> not expanded; forward {fwd:.3} s", data.len());
        return;
    };
    let t = Instant::now();
    for _ in 0..reps {
        let back = inverse(out.clone(), &Transform::Deflate, data.len() as u64).unwrap();
        assert_eq!(back, data);
    }
    let inv = t.elapsed().as_secs_f64() / reps as f64;
    println!("{} -> {} bytes; forward {fwd:.3} s, inverse {inv:.3} s", data.len(), out.len());
}
