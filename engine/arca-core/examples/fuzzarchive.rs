//! Robustness tool: mutates archives thousands of ways and checks that the reader,
//! the verifier and the recovery record never panic, never hang, and never hand
//! back a repaired copy that does not verify. `fuzzarchive SEED ITERATIONS WORKDIR`.
//!
//! Every case prints its number and mutation before it runs, so a crash the
//! process cannot catch (an aborted allocation) still names the case.
use arca_core::recovery;
use arca_core::{pack, Archive, Hooks, Level, PackOptions};
use std::fs;
use std::io::Write;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next() % n }
    }
}

fn tree(root: &Path, rng: &mut Rng) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("bin")).unwrap();
    for i in 0..30 {
        let text: String = (0..1500).map(|j| format!("fn f_{i}_{j}() -> u32 {{ {j} }}\n")).collect();
        fs::write(root.join(format!("src/m_{i}.rs")), text).unwrap();
    }
    for i in 0..4 {
        let blob: Vec<u8> = (0..300_000).map(|_| rng.next() as u8).collect();
        fs::write(root.join(format!("bin/b_{i}.bin")), blob).unwrap();
    }
    fs::write(root.join("bin/b_copy.bin"), fs::read(root.join("bin/b_0.bin")).unwrap()).unwrap();
    let mut f = fs::File::create(root.join("notes.txt")).unwrap();
    for i in 0..8000 {
        writeln!(f, "line {i}: the quick brown fox jumps over the lazy dog").unwrap();
    }
}

#[derive(Debug)]
#[allow(dead_code)] // the fields are for the printed record of each case
enum Mutation {
    Flips(u32),
    Run { at: u64, len: usize },
    Truncate(u64),
    Insert { at: u64, len: usize },
    Delete { at: u64, len: usize },
    Footer { field: usize, value: u64 },
    Tail { from_end: u64, len: usize },
}

fn mutate(bytes: &mut Vec<u8>, rng: &mut Rng) -> Mutation {
    let n = bytes.len() as u64;
    match rng.below(7) {
        0 => {
            let k = 1 + rng.below(16) as u32;
            for _ in 0..k {
                let i = rng.below(n) as usize;
                bytes[i] ^= 1 << rng.below(8);
            }
            Mutation::Flips(k)
        }
        1 => {
            let len = 1 + rng.below(4096) as usize;
            let at = rng.below(n);
            for i in at as usize..(at as usize + len).min(bytes.len()) {
                bytes[i] = rng.next() as u8;
            }
            Mutation::Run { at, len }
        }
        2 => {
            let to = rng.below(n);
            bytes.truncate(to as usize);
            Mutation::Truncate(to)
        }
        3 => {
            let len = 1 + rng.below(5000) as usize;
            let at = rng.below(n) as usize;
            let junk: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
            bytes.splice(at..at, junk);
            Mutation::Insert { at: at as u64, len }
        }
        4 => {
            let len = 1 + rng.below(5000) as usize;
            let at = rng.below(n) as usize;
            let end = (at + len).min(bytes.len());
            bytes.drain(at..end);
            Mutation::Delete { at: at as u64, len: end - at }
        }
        5 => {
            // A footer field set to something absurd or to a near-miss.
            let field = rng.below(7) as usize; // 7 u64 fields: offsets, lengths, record offset and length
            let value = match rng.below(4) {
                0 => u64::MAX,
                1 => rng.next(),
                2 => n.saturating_sub(rng.below(200)),
                _ => rng.below(1 << 40),
            };
            if bytes.len() >= 64 {
                let base = bytes.len() - 64;
                let off = [0usize, 8, 16, 40, 48, 8, 16][field];
                bytes[base + off..base + off + 8].copy_from_slice(&value.to_le_bytes());
            }
            Mutation::Footer { field, value }
        }
        _ => {
            // Damage in the last 64 KiB: the record's second header, the index, the footer.
            let from_end = rng.below((64 << 10).min(n));
            let len = 1 + rng.below(512) as usize;
            let at = n.saturating_sub(from_end) as usize;
            for i in at..(at + len).min(bytes.len()) {
                bytes[i] = rng.next() as u8;
            }
            Mutation::Tail { from_end, len }
        }
    }
}

fn exercise(path: &Path, out: &Path) -> Result<(), String> {
    let hooks = Hooks::default();
    // Open and verify: Ok or Err, never a panic.
    let open = catch_unwind(AssertUnwindSafe(|| Archive::open(path).and_then(|a| a.verify(2))));
    match open {
        Err(_) => return Err("PANIC in open/verify".into()),
        Ok(_) => {}
    }
    // The record: check, then repair; a repairable report must give a copy that verifies.
    let check = catch_unwind(AssertUnwindSafe(|| recovery::check(path, &hooks)));
    let report = match check {
        Err(_) => return Err("PANIC in recovery::check".into()),
        Ok(Err(_)) => return Ok(()),
        Ok(Ok(r)) => r,
    };
    if report.record.is_none() {
        return Ok(());
    }
    let repair = catch_unwind(AssertUnwindSafe(|| recovery::repair(path, out, &hooks)));
    match repair {
        Err(_) => Err("PANIC in recovery::repair".into()),
        Ok(Err(_)) => Ok(()),
        Ok(Ok(after)) => {
            if after.repairable {
                match catch_unwind(AssertUnwindSafe(|| Archive::open(out).and_then(|a| a.verify(2)))) {
                    Err(_) => Err("PANIC verifying the repaired copy".into()),
                    Ok(Err(e)) => Err(format!("repair said repairable but the copy fails: {e}")),
                    Ok(Ok(_)) => Ok(()),
                }
            } else {
                Ok(())
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let iterations: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(500);
    let work = PathBuf::from(args.get(3).cloned().unwrap_or_else(|| "/tmp/fuzzarchive".into()));
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).unwrap();
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let src = work.join("tree");
    tree(&src, &mut rng);
    let mut bases = Vec::new();
    for (name, level, rec) in [("fast", Level::Fast, 0u16), ("balanced", Level::Balanced, 5), ("max", Level::Max, 3), ("plain", Level::Balanced, 0)] {
        let p = work.join(format!("{name}.zlg"));
        let mut o = PackOptions::preset(level);
        o.recovery_percent = rec;
        pack(std::slice::from_ref(&src), &p, &o).unwrap();
        bases.push(fs::read(&p).unwrap());
    }
    // A watchdog: a case that runs past 60 s is a hang, and the process says so and exits.
    let current = Arc::new(AtomicU64::new(0));
    let started = Arc::new(std::sync::Mutex::new(Instant::now()));
    {
        let (current, started) = (current.clone(), started.clone());
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(5));
            if started.lock().unwrap().elapsed() > Duration::from_secs(60) {
                eprintln!("HANG at case {}", current.load(Ordering::Relaxed));
                std::process::exit(3);
            }
        });
    }
    let case_path = work.join("case.zlg");
    let out_path = work.join("case-repaired.zlg");
    let mut problems = 0;
    let begin = Instant::now();
    for i in 0..iterations {
        let b = rng.below(bases.len() as u64) as usize;
        let mut bytes = bases[b].clone();
        let m = mutate(&mut bytes, &mut rng);
        current.store(i, Ordering::Relaxed);
        *started.lock().unwrap() = Instant::now();
        // Printed first, so an abort still names the case.
        println!("case {i} base {b} {m:?}");
        std::io::stdout().flush().unwrap();
        fs::write(&case_path, &bytes).unwrap();
        let _ = fs::remove_file(&out_path);
        if let Err(e) = exercise(&case_path, &out_path) {
            problems += 1;
            println!("PROBLEM case {i} seed {seed} base {b} {m:?}: {e}");
            let keep = work.join(format!("problem-{seed}-{i}.zlg"));
            let _ = fs::copy(&case_path, &keep);
        }
    }
    println!("done: {iterations} cases, {problems} problems, {:.1} s", begin.elapsed().as_secs_f64());
    if problems > 0 {
        std::process::exit(1);
    }
}
