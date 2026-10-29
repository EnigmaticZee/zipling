//! Model R: JPEG's own symbols (DC size; AC run/size, EOB, ZRL) re-coded with
//! static Huffman tables per image and per context. Raw magnitude bits are
//! JPEG's own, so unpacking is a table swap. Contexts: component class, the
//! band of the next position, nonzeros so far, neighbour nonzero counts.

use super::{bucket, Image};
use std::collections::HashMap;

fn lengths_limited(cnt: &[u32], maxlen: u32) -> Vec<u32> {
    let mut c: Vec<u64> = cnt.iter().map(|&x| x as u64).collect();
    loop {
        let len = huff(&c);
        if len.iter().all(|&l| l <= maxlen) {
            return len;
        }
        for x in c.iter_mut() {
            if *x > 0 { *x = (*x + 1) / 2; }
        }
    }
}

fn huff(cnt: &[u64]) -> Vec<u32> {
    let n = cnt.len();
    let mut nodes: Vec<(i32, i32)> = Vec::new();
    let mut heap = std::collections::BinaryHeap::new();
    for (i, &c) in cnt.iter().enumerate() {
        if c > 0 {
            nodes.push((-1, i as i32));
            heap.push(std::cmp::Reverse((c, nodes.len() - 1)));
        }
    }
    let mut len = vec![0u32; n];
    if nodes.len() == 1 {
        len[nodes[0].1 as usize] = 1;
        return len;
    }
    while heap.len() > 1 {
        let std::cmp::Reverse((w1, a)) = heap.pop().unwrap();
        let std::cmp::Reverse((w2, b)) = heap.pop().unwrap();
        nodes.push((a as i32, b as i32));
        heap.push(std::cmp::Reverse((w1 + w2, nodes.len() - 1)));
    }
    let root = heap.pop().unwrap().0 .1;
    let mut stack = vec![(root, 0u32)];
    while let Some((i, d)) = stack.pop() {
        let (l, r) = nodes[i];
        if l == -1 { len[r as usize] = d.max(1); } else { stack.push((l as usize, d + 1)); stack.push((r as usize, d + 1)); }
    }
    len
}

pub fn band(k: usize) -> u64 {
    match k { 0..=2 => 0, 3..=5 => 1, 6..=9 => 2, 10..=14 => 3, 15..=20 => 4, 21..=27 => 5, 28..=35 => 6, _ => 7 }
}

/// Returns (bits with tables, bits of JPEG's own coding of the same symbols' raw part).
pub fn estimate_r(img: &Image) -> (f64, f64) {
    let mode = std::env::var("R_MODE").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
    let mut st: HashMap<u64, Vec<u32>> = HashMap::new();
    let mut raw = 0.0f64;
    let mut add = |ctx: u64, s: usize| { st.entry(ctx).or_insert_with(|| vec![0; 256])[s] += 1; };
    for (ci, comp) in img.comps.iter().enumerate() {
        let cc = ci.min(1) as u64;
        let (bw, bh) = (comp.bw, comp.bh);
        let nz = |b: &[i16; 64]| b[1..].iter().filter(|&&v| v != 0).count() as u32;
        let mut prev_dc = 0i32;
        for by in 0..bh {
            for bx in 0..bw {
                let b = by * bw + bx;
                let blk = &comp.coef[b];
                let nu = if by > 0 { Some(nz(&comp.coef[b - bw])) } else { None };
                let nl = if bx > 0 { Some(nz(&comp.coef[b - 1])) } else { None };
                let nctx = match (nu, nl) { (Some(a), Some(b)) => bucket((a + b + 1) / 2), (Some(a), None) | (None, Some(a)) => bucket(a), _ => 8 } as u64;
                // DC size
                let d = blk[0] as i32 - prev_dc;
                prev_dc = blk[0] as i32;
                let s = 32 - d.unsigned_abs().leading_zeros();
                add((cc << 40) | (1 << 32) | nctx, s as usize);
                raw += s as f64;
                // AC run/size
                let mut run = 0u32;
                let mut k0 = 1usize; // position the current symbol starts at
                let mut seen = 0u32;
                let last = (1..64).rev().find(|&k| blk[k] != 0).unwrap_or(0);
                let ctx_at = |k0: usize, seen: u32| -> u64 {
                    let sb = if mode == 1 { 0 } else { bucket(seen).min(5) as u64 };
                    (cc << 40) | (2 << 32) | (band(k0) << 12) | (nctx << 4) | sb
                };
                for k in 1..64 {
                    if k > last { break; }
                    let v = blk[k] as i32;
                    if v == 0 { run += 1; continue; }
                    while run > 15 {
                        add(ctx_at(k0, seen), 0xF0);
                        run -= 16;
                        k0 += 16;
                    }
                    let s = 32 - v.unsigned_abs().leading_zeros();
                    add(ctx_at(k0, seen), ((run << 4) | s) as usize);
                    raw += s as f64;
                    seen += 1;
                    run = 0;
                    k0 = k + 1;
                }
                if last < 63 {
                    add(ctx_at(k0, seen), 0x00);
                }
            }
        }
    }
    let mut bits = raw;
    let mut side = 0.0;
    for cnt in st.values() {
        let len = lengths_limited(cnt, 16);
        for (c, l) in cnt.iter().zip(&len) { bits += *c as f64 * *l as f64; }
        // DHT-style table: 16 counts + one byte per used symbol
        side += 8.0 * (16.0 + cnt.iter().filter(|&&c| c > 0).count() as f64);
    }
    (bits + side, side)
}
