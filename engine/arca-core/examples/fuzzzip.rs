//! Robustness tool for the ZIP reader: mutates ZIP files thousands of ways and
//! checks that opening, verifying and extracting never panic or hang, and that
//! nothing is ever written outside the destination. `fuzzzip SEED ITERATIONS WORKDIR`.
use arca_core::zip::{ZipArchive, ZipExtractOptions};
use arca_core::zipwrite::{write_zip, ZipWriteOptions};
use std::fs;
use std::io::Write;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::Command;
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
    fs::create_dir_all(root.join("src/deep/er")).unwrap();
    fs::create_dir_all(root.join("bin")).unwrap();
    for i in 0..20 {
        let text: String = (0..800).map(|j| format!("fn f_{i}_{j}() -> u32 {{ {j} }}\n")).collect();
        fs::write(root.join(format!("src/m_{i}.rs")), text).unwrap();
    }
    fs::write(root.join("src/deep/er/leaf.txt"), "leaf").unwrap();
    for i in 0..3 {
        let blob: Vec<u8> = (0..200_000).map(|_| rng.next() as u8).collect();
        fs::write(root.join(format!("bin/b_{i}.bin")), blob).unwrap();
    }
    fs::write(root.join("empty.txt"), "").unwrap();
    let mut f = fs::File::create(root.join("notes.txt")).unwrap();
    for i in 0..5000 {
        writeln!(f, "line {i}: the quick brown fox jumps over the lazy dog").unwrap();
    }
    let _ = std::os::unix::fs::symlink("notes.txt", root.join("link"));
}

#[derive(Debug)]
#[allow(dead_code)]
enum Mutation {
    Flips(u32),
    Run { at: u64, len: usize },
    Truncate(u64),
    Insert { at: u64, len: usize },
    Delete { at: u64, len: usize },
    Directory { from_end: u64, len: usize },
    Field { at: u64, value: u32 },
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
            let len = 1 + rng.below(2048) as usize;
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
            let len = 1 + rng.below(3000) as usize;
            let at = rng.below(n) as usize;
            let junk: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
            bytes.splice(at..at, junk);
            Mutation::Insert { at: at as u64, len }
        }
        4 => {
            let len = 1 + rng.below(3000) as usize;
            let at = rng.below(n) as usize;
            let end = (at + len).min(bytes.len());
            bytes.drain(at..end);
            Mutation::Delete { at: at as u64, len: end - at }
        }
        5 => {
            // The central directory and end record live in the last few KiB.
            let from_end = rng.below((4096).min(n));
            let len = 1 + rng.below(64) as usize;
            let at = n.saturating_sub(from_end) as usize;
            for i in at..(at + len).min(bytes.len()) {
                bytes[i] = rng.next() as u8;
            }
            Mutation::Directory { from_end, len }
        }
        _ => {
            // A 32-bit field somewhere, set to a lie: huge, zero, or near the file size.
            let at = rng.below(n.saturating_sub(4));
            let value = match rng.below(4) {
                0 => u32::MAX,
                1 => 0,
                2 => rng.next() as u32,
                _ => (n as u32).wrapping_sub(rng.below(64) as u32),
            };
            bytes[at as usize..at as usize + 4].copy_from_slice(&value.to_le_bytes());
            Mutation::Field { at, value }
        }
    }
}

fn exercise(path: &Path, parent: &Path) -> Result<(), String> {
    let _ = fs::remove_dir_all(parent);
    fs::create_dir_all(parent).unwrap();
    let dest = parent.join("dest");
    fs::create_dir_all(&dest).unwrap();
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let z = ZipArchive::open(path)?;
        let _ = z.verify(2);
        z.extract(&dest, &ZipExtractOptions { threads: 2, overwrite: true, ..Default::default() })
    }));
    match outcome {
        Err(_) => return Err("PANIC in open/verify/extract".into()),
        Ok(_) => {}
    }
    // Nothing may appear beside the destination: an entry that escaped it would.
    let beside: Vec<String> = fs::read_dir(parent)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != "dest")
        .collect();
    if !beside.is_empty() {
        return Err(format!("ESCAPED the destination: {beside:?}"));
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let iterations: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(500);
    let work = PathBuf::from(args.get(3).cloned().unwrap_or_else(|| "/tmp/fuzzzip".into()));
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).unwrap();
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let src = work.join("tree");
    tree(&src, &mut rng);
    let mut bases: Vec<Vec<u8>> = Vec::new();
    for (name, level) in [("deflate", 6u32), ("stored", 0)] {
        let p = work.join(format!("{name}.zip"));
        write_zip(std::slice::from_ref(&src), &p, &ZipWriteOptions { level, threads: 2, leave_out_mac_files: false, ..Default::default() }).unwrap();
        bases.push(fs::read(&p).unwrap());
    }
    // A Mac-made ZIP, with its __MACOSX twins, when ditto is there.
    let ditto = work.join("ditto.zip");
    if Command::new("ditto").args(["-c", "-k", "--sequesterRsrc", "--keepParent"]).arg(&src).arg(&ditto).status().map(|s| s.success()).unwrap_or(false) {
        bases.push(fs::read(&ditto).unwrap());
    }
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
    let case_path = work.join("case.zip");
    let parent = work.join("x-parent");
    let mut problems = 0;
    let begin = Instant::now();
    for i in 0..iterations {
        let b = rng.below(bases.len() as u64) as usize;
        let mut bytes = bases[b].clone();
        let m = mutate(&mut bytes, &mut rng);
        current.store(i, Ordering::Relaxed);
        *started.lock().unwrap() = Instant::now();
        println!("case {i} base {b} {m:?}");
        std::io::stdout().flush().unwrap();
        fs::write(&case_path, &bytes).unwrap();
        if let Err(e) = exercise(&case_path, &parent) {
            problems += 1;
            println!("PROBLEM case {i} seed {seed} base {b} {m:?}: {e}");
            let _ = fs::copy(&case_path, work.join(format!("problem-{seed}-{i}.zip")));
        }
    }
    println!("done: {iterations} cases over {} bases, {problems} problems, {:.1} s", bases.len(), begin.elapsed().as_secs_f64());
    if problems > 0 {
        std::process::exit(1);
    }
}
