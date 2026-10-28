//! Research tool: times the inverse transform of every transformed object in an
//! archive and prints the costliest. `objtime ARCHIVE [N]`
use arca_core::{transform, Archive, EntryKind, Transform};
use rayon::prelude::*;
use std::collections::HashMap;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let a = Archive::open(std::path::Path::new(&args[1]))?;
    let top: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30);
    let idx = &a.index;
    let blocks: Vec<Vec<u8>> = (0..idx.blocks.len() as u32).into_par_iter().map(|b| a.read_block(b).unwrap()).collect();
    let mut name: HashMap<u32, &str> = HashMap::new();
    for e in &idx.entries {
        if let EntryKind::File { object, .. } = e.kind {
            name.entry(object).or_insert(&e.path);
        }
    }
    let mut rows: Vec<(f64, u64, u64, &str, &'static str)> = (0..idx.objects.len() as u32)
        .into_par_iter()
        .with_max_len(1)
        .filter(|&o| idx.objects[o as usize].transform != Transform::None)
        .map(|o| {
            let obj = &idx.objects[o as usize];
            let mut stored = Vec::new();
            for &c in &obj.chunks {
                let ch = &idx.chunks[c as usize];
                stored.extend_from_slice(&blocks[ch.block as usize][ch.offset as usize..(ch.offset + ch.len) as usize]);
            }
            let t = Instant::now();
            transform::inverse(stored, &obj.transform, obj.size).unwrap();
            (t.elapsed().as_secs_f64(), obj.size, obj.stored_len, name.get(&o).copied().unwrap_or("?"), obj.transform.name())
        })
        .collect();
    rows.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    let total: f64 = rows.iter().map(|r| r.0).sum();
    println!("total inverse CPU {total:.2} s over {} objects", rows.len());
    for (t, size, stored, path, kind) in rows.iter().take(top) {
        println!("{t:>7.3} s  {kind:<7} {size:>10} -> {stored:>11}  {path}");
    }
    Ok(())
}
