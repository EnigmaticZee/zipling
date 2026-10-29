//! Model H: a light mode for Fast. Static Huffman codes per image and per small
//! context (frequency band x neighbour nonzero bucket), joint symbols as in the
//! fast coder; cost = sum of code lengths + raw bits + a table cost.

use super::{bucket, Image};
use std::collections::HashMap;

/// Huffman code lengths (limited to 15 bits by flattening), for the counts given.
fn lengths(cnt: &[u32]) -> Vec<u32> {
    let n = cnt.len();
    let mut nodes: Vec<(u64, i32, i32)> = Vec::new(); // (weight, left, right); leaves have -1
    let mut heap = std::collections::BinaryHeap::new();
    for (i, &c) in cnt.iter().enumerate() {
        if c > 0 {
            nodes.push((c as u64, -1, i as i32));
            heap.push(std::cmp::Reverse((c as u64, nodes.len() - 1)));
        }
    }
    let mut len = vec![0u32; n];
    if nodes.len() == 1 {
        len[nodes[0].2 as usize] = 1;
        return len;
    }
    while heap.len() > 1 {
        let std::cmp::Reverse((w1, a)) = heap.pop().unwrap();
        let std::cmp::Reverse((w2, b)) = heap.pop().unwrap();
        nodes.push((w1 + w2, a as i32, b as i32));
        heap.push(std::cmp::Reverse((w1 + w2, nodes.len() - 1)));
    }
    let root = heap.pop().unwrap().0 .1;
    let mut stack = vec![(root, 0u32)];
    while let Some((i, d)) = stack.pop() {
        let (_, l, r) = nodes[i];
        if l == -1 {
            len[r as usize] = d.max(1).min(15);
        } else {
            stack.push((l as usize, d + 1));
            stack.push((r as usize, d + 1));
        }
    }
    len
}

pub fn estimate_h(img: &Image) -> f64 {
    let mut st: HashMap<u64, Vec<u32>> = HashMap::new();
    let mut raw = 0.0f64;
    let mut add = |ctx: u64, s: usize, n: usize| { st.entry(ctx).or_insert_with(|| vec![0; n])[s] += 1; };
    let band = |k: usize| -> u64 { [0u64, 0, 0, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 3, 4, 4, 4, 4, 4, 4, 5, 5, 5, 5, 5, 5, 5, 6, 6, 6, 6, 6, 6, 6, 6,
        7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7][k] };
    let joint = |a: u32| -> (usize, u32) {
        if a == 0 { return (0, 0); }
        let e = 32 - a.leading_zeros();
        if e == 1 { return (1, 0); }
        (2 + 2 * (e as usize - 2) + ((a >> (e - 2)) & 1) as usize, e - 2)
    };
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
                let n = nz(blk);
                add((cc << 40) | (1 << 32) | nctx, n as usize, 64);
                let nb = bucket(n).min(7) as u64;
                let mut left = n;
                let mut k = 1;
                while left > 0 {
                    let v = blk[k] as i32;
                    let (sy, nlow) = joint(v.unsigned_abs());
                    add((cc << 40) | (2 << 32) | (band(k) << 8) | nb, sy, 32);
                    if sy > 0 { raw += nlow as f64 + 1.0; left -= 1; }
                    k += 1;
                }
                let r = blk[0] as i32 - prev_dc;
                prev_dc = blk[0] as i32;
                let (sy, nlow) = joint(r.unsigned_abs());
                add((cc << 40) | (3 << 32) | nb, sy, 32);
                if sy > 0 { raw += nlow as f64 + 1.0; }
            }
        }
    }
    let mut bits = raw;
    for cnt in st.values() {
        let len = lengths(cnt);
        for (c, l) in cnt.iter().zip(&len) {
            bits += *c as f64 * *l as f64;
        }
        bits += 4.0 * cnt.len() as f64; // a code length per symbol
    }
    bits
}
