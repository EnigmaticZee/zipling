//! Round-trips files through the lzs codec on LZMA's parse and times decoding.
//!
//!     cargo run --release --example lzsbench -- <file> ... [--fb N]

use arca_core::lzs::{self, Token};
use std::cell::RefCell;
use std::time::Instant;

thread_local! {
    static TOKENS: RefCell<Vec<Token>> = const { RefCell::new(Vec::new()) };
}

unsafe extern "C" fn hook(pos: u32, len: u32, dist: u32, reps: *const u32, _state: u32) {
    if dist == u32::MAX {
        return;
    }
    let (kind, d) = if dist < 4 {
        (if dist == 0 && len == 1 { 5u8 } else { dist as u8 + 1 }, unsafe { *reps.add(dist as usize) })
    } else {
        (0, dist - 4 + 1)
    };
    TOKENS.with(|t| {
        let mut t = t.borrow_mut();
        // The LZMA2 encoder re-runs a chunk it decides to store raw: drop what
        // the first run reported from that position on.
        while t.last().is_some_and(|l| l.pos >= pos) {
            t.pop();
        }
        t.push(Token { pos, len, kind, dist: d });
    });
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut fb = 273i32;
    let mut files = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--fb" => fb = it.next().unwrap().parse().unwrap(),
            f => files.push(f.to_string()),
        }
    }
    if files.first().map(|f| f.as_str()) == Some("--decode") {
        // --decode <stream.lzs> <original>: decode repeatedly, for profiling
        let (st, orig) = (std::fs::read(&files[1]).unwrap(), std::fs::read(&files[2]).unwrap());
        let mut best = f64::MAX;
        for _ in 0..10 {
            let t = Instant::now();
            let back = lzs::decode(&st, orig.len()).unwrap();
            best = best.min(t.elapsed().as_secs_f64());
            assert!(back == orig);
        }
        println!("decode {:.0} MB/s", orig.len() as f64 / 1e6 / best);
        return;
    }
    let save = std::env::var("LZS_SAVE").ok();
    lzma_sdk::set_token_hook(Some(hook));
    println!("{:<12} {:>7} | {:>8} {:>8} {:>7} | {:>8} {:>8}", "file", "MB", "lzma", "lzs", "vs", "enc s", "dec MB/s");
    let (mut tl, mut ts) = (0usize, 0usize);
    for f in &files {
        let data = std::fs::read(f).unwrap();
        TOKENS.with(|t| t.borrow_mut().clear());
        let s = lzma_sdk::Settings { level: 9, dict_size: (data.len() as u32).clamp(1 << 16, 1 << 27), fb, mc: 0, threads: 1 };
        let t0 = Instant::now();
        let (lz, prop) = lzma_sdk::encode(&data, s, data.len() + 1024).unwrap();
        let toks = TOKENS.with(|t| t.borrow().clone());
        for w in toks.windows(2) {
            assert!(w[1].pos >= w[0].pos + w[0].len, "tokens overlap at {}", w[1].pos);
        }
        for t in &toks {
            let (p, l, d) = (t.pos as usize, t.len as usize, t.dist as usize);
            assert!(d >= 1 && d <= p && (0..l).all(|i| data[p + i] == data[p - d + i]), "token does not match the data: {t:?}");
        }
        let out = lzs::encode(&data, &toks);
        let enc = t0.elapsed().as_secs_f64();
        if let Some(dir) = &save {
            let name = std::path::Path::new(f).file_name().unwrap().to_string_lossy().into_owned();
            std::fs::write(format!("{dir}/{name}.lzs"), &out).unwrap();
        }
        let mut best = f64::MAX;
        let mut back = Vec::new();
        for _ in 0..3 {
            let t = Instant::now();
            back = lzs::decode(&out, data.len()).unwrap();
            best = best.min(t.elapsed().as_secs_f64());
        }
        assert!(back == data, "{f}: round trip differs");
        let mut lzbest = f64::MAX;
        for _ in 0..2 {
            let t = Instant::now();
            let d = lzma_sdk::decode(&lz, prop, data.len()).unwrap();
            assert!(d == data);
            lzbest = lzbest.min(t.elapsed().as_secs_f64());
        }
        tl += lz.len();
        ts += out.len();
        let name = std::path::Path::new(f).file_name().unwrap().to_string_lossy();
        println!(
            "{:<12} {:>7.2} | {:>8.3} {:>8.3} {:>+6.1}% | {:>8.1} {:>8.0}  (lzma dec {:.0} MB/s)",
            &name[..name.len().min(12)],
            data.len() as f64 / 1e6,
            lz.len() as f64 / 1e6,
            out.len() as f64 / 1e6,
            (out.len() as f64 / lz.len() as f64 - 1.0) * 100.0,
            enc,
            data.len() as f64 / 1e6 / best,
            data.len() as f64 / 1e6 / lzbest
        );
    }
    if files.len() > 1 {
        println!("all: lzma {:.3} MB, lzs {:.3} MB ({:+.1}%)", tl as f64 / 1e6, ts as f64 / 1e6, (ts as f64 / tl as f64 - 1.0) * 100.0);
    }
}
