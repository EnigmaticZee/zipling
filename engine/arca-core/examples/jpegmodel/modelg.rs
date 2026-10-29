//! Model G: a continuous "expected magnitude" per coefficient from four
//! neighbours (above, left, above-left, above-right) at the same frequency,
//! quantised finely (quarter octaves), as the main context. Coded adaptively
//! here to measure the modelling; the fast coder would use static tables.

use super::modelf::ZZ;
use super::{bucket, Coder, Ctx, Image};

fn var(name: &str) -> i32 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(0)
}

fn signed(c: &mut Coder, x: &mut Ctx, base: u64, v: i32, sctx: u64) {
    c.code(x.get(base << 8), v != 0, 5);
    if v == 0 {
        return;
    }
    let a = v.unsigned_abs();
    let e = 32 - a.leading_zeros();
    for i in 1..e {
        c.code(x.get((base << 8) | i as u64), true, 5);
    }
    if e < 16 {
        c.code(x.get((base << 8) | e as u64), false, 5);
    }
    for b in (0..e - 1).rev() {
        let bit = (a >> b) & 1 == 1;
        if b == e - 2 {
            c.code(x.get((base << 8) | 0x80 | e as u64), bit, 5);
        } else {
            c.bits += 1.0;
            c.decisions += 1;
        }
    }
    c.code(x.get((base << 8) | 0xF0 | sctx), v < 0, 5);
}

/// Quarter-octave level of a non-negative weighted magnitude (scaled by 8).
fn level(x8: u32, steps: u32) -> u64 {
    if x8 == 0 {
        return 0;
    }
    let l = ((x8 as f64 / 8.0 + 1.0).log2() * steps as f64).round() as u64;
    1 + l.min(40)
}

pub static IDEAL: std::sync::Mutex<[f64; 4]> = std::sync::Mutex::new([0.0; 4]);
pub static PER_IMAGE: std::sync::Mutex<Vec<(std::collections::HashMap<u64, Vec<u32>>, f64)>> = std::sync::Mutex::new(Vec::new());

/// Per-image static entropy of the counts, with no table cost; and the table cost
/// if every used context sent its histogram (6 bits per used symbol + 4 per context).
fn static_cost(st: &std::collections::HashMap<u64, Vec<u32>>) -> (f64, f64) {
    let (mut h, mut side) = (0.0, 0.0);
    for cnt in st.values() {
        let tot: u32 = cnt.iter().sum();
        if tot == 0 { continue; }
        side += 4.0;
        for &c in cnt { if c > 0 { h += c as f64 * (tot as f64 / c as f64).log2(); side += 6.0; } }
    }
    (h, side)
}

pub fn estimate_g(img: &Image) -> (f64, u64) {
    let mut st: std::collections::HashMap<u64, Vec<u32>> = Default::default();
    let mut raw = 0.0f64;
    let mut add = |ctx: u64, s: usize, n: usize| { st.entry(ctx).or_insert_with(|| vec![0; n])[s] += 1; };
    let steps = if var("G_STEPS") > 0 { var("G_STEPS") as u32 } else { 4 };
    let use_left = var("G_NOLEFT") == 0;
    let intra = var("G_INTRA"); // 1: add the previous coded coefficient's magnitude as context
    let mt = var("G_MATCH") as u32; // > 0: blocks with at least this many nonzeros may copy an earlier block
    let mut c = Coder::new();
    let (mut bits, mut dec) = (0.0, 0);
    for (ci, comp) in img.comps.iter().enumerate() {
        let cc = ci.min(1) as u64;
        let (bw, bh) = (comp.bw, comp.bh);
        let nat: Vec<[i32; 64]> = comp.coef.iter().map(|b| { let mut n = [0i32; 64]; for k in 0..64 { n[ZZ[k]] = b[k] as i32; } n }).collect();
        let nzc = |n: &[i32; 64]| n[1..].iter().filter(|&&v| v != 0).count() as u32;
        let mut x = Ctx::default();
        c.bits = 0.0;
        c.decisions = 0;
        // per nonzero count: distinct earlier blocks, and where each AC pattern was last seen
        let mut seen: std::collections::HashMap<[i32; 63], usize> = Default::default();
        let mut order = vec![0usize; 64];
        let mut prev_match = false;
        for by in 0..bh {
            for bx in 0..bw {
                let b = by * bw + bx;
                let cur = &nat[b];
                let up = if by > 0 { Some(&nat[b - bw]) } else { None };
                let lf = if bx > 0 { Some(&nat[b - 1]) } else { None };
                let ul = if by > 0 && bx > 0 { Some(&nat[b - bw - 1]) } else { None };
                let ur = if by > 0 && bx + 1 < bw { Some(&nat[b - bw + 1]) } else { None };
                // count: weighted neighbour counts
                let (mut s, mut w) = (0u32, 0u32);
                for (nb, wt) in [(up, 2), (lf, 2), (ul, 1), (ur, 1)] {
                    if let Some(n) = nb { s += wt * nzc(n); w += wt; }
                }
                let nctx = if w == 0 { 63 } else { ((2 * s + w) / (2 * w)) as u64 };
                let n = nzc(cur);
                add((cc << 40) | (3 << 32) | nctx.min(63), n as usize, 64);
                let mut node = 1u64;
                for i in (0..6).rev() {
                    let bit = (n >> i) & 1 == 1;
                    c.code(x.get((cc << 40) | (3 << 32) | (nctx.min(63) << 8) | node), bit, 5);
                    node = (node << 1) | bit as u64;
                }
                let mut left = n;
                let mut prev = 0u32;
                let mut matched = false;
                if mt > 0 && n >= mt {
                    let mut key = [0i32; 63];
                    for k in 1..64 { key[k - 1] = cur[ZZ[k]]; }
                    let hit = seen.get(&key).copied();
                    let fctx = (cc << 40) | (10 << 32) | (bucket(n).min(7) << 1) | prev_match as u64;
                    c.code(x.get(fctx), hit.is_some(), 5);
                    add(fctx, hit.is_some() as usize, 2);
                    if let Some(pos) = hit {
                        // distance among distinct earlier blocks with the same count
                        let d = (order[n as usize] - pos) as i32;
                        signed(&mut c, &mut x, (cc << 40) | (11 << 32) | bucket(n).min(7), d, 0);
                        c.bits -= 1.0; // the sign is known
                        let e = (32 - (d as u32).leading_zeros()) as usize;
                        add((cc << 40) | (11 << 32) | bucket(n).min(7), e, 32);
                        raw += (e - 1) as f64;
                        matched = true;
                        left = 0;
                    }
                    if hit.is_none() { seen.insert(key, order[n as usize]); order[n as usize] += 1; } else { seen.insert(key, order[n as usize]); order[n as usize] += 1; }
                    prev_match = matched;
                }
                for k in 1..64 {
                    if left == 0 { break; }
                    let p = ZZ[k];
                    let (mut s8, mut w) = (0u32, 0u32);
                    for (nb, wt) in [(up, 2), (lf, 2), (ul, 1), (ur, 1)] {
                        if let Some(n) = nb { s8 += 8 * wt * n[p].unsigned_abs(); w += wt; }
                    }
                    let lv = if w == 0 { 63 } else { level(s8 / w, steps) };
                    let lb = if use_left { bucket(left).min(7) } else { 0 };
                    let ib = if intra == 1 { bucket(prev).min(3) } else { 0 };
                    let kg = k as u64;
                    let v = cur[p];
                    let sctx = match (up, lf) {
                        (Some(a), Some(b)) => ((a[p].signum() + b[p].signum() + 2) as u64).min(4),
                        _ => 5,
                    };
                    signed(&mut c, &mut x, (cc << 40) | (4 << 32) | (kg << 20) | (lv << 8) | (lb << 3) | ib, v, sctx);
                    {
                        let a = v.unsigned_abs();
                        let e = (32 - a.leading_zeros()) as usize;
                        add((cc << 40) | (4 << 32) | (kg << 20) | (lv << 8) | (lb << 3) | ib, e, 16);
                        if e >= 2 {
                            add((cc << 40) | (6 << 32) | ((k as u64).min(15) << 8) | e as u64, ((a >> (e - 2)) & 1) as usize, 2);
                            raw += (e - 2) as f64;
                        }
                        if e > 0 { add((cc << 40) | (7 << 32) | sctx, (v < 0) as usize, 2); }
                    }
                    if v != 0 { left -= 1; }
                    prev = v.unsigned_abs();
                }
                // DC: LOCO-I as in B
                let a = lf.map_or(0, |l| l[0]);
                let bb = up.map_or(0, |u| u[0]);
                let cdc = ul.map_or(0, |u| u[0]);
                let pr = match (lf.is_some(), up.is_some()) {
                    (true, true) => if cdc >= a.max(bb) { a.min(bb) } else if cdc <= a.min(bb) { a.max(bb) } else { a + bb - cdc },
                    (true, false) => a,
                    (false, true) => bb,
                    _ => 0,
                };
                let act = bucket(((a - cdc).unsigned_abs() + (bb - cdc).unsigned_abs()) as u32).min(10);
                signed(&mut c, &mut x, (cc << 40) | (5 << 32) | (act << 4) | bucket(n).min(7), cur[0] - pr, 0);
                {
                    let r = cur[0] - pr;
                    let a = r.unsigned_abs();
                    let e = (32 - a.leading_zeros()) as usize;
                    add((cc << 40) | (5 << 32) | (act << 4) | bucket(n).min(7), e, 16);
                    if e >= 2 { add((cc << 40) | (8 << 32) | e as u64, ((a >> (e - 2)) & 1) as usize, 2); raw += (e - 2) as f64; }
                    if e > 0 { raw += 1.0; }
                }
            }
        }
        bits += c.bits;
        dec += c.decisions;
    }
    let (h, side) = static_cost(&st);
    PER_IMAGE.lock().unwrap().push((st.clone(), raw));
    {
        let mut g = IDEAL.lock().unwrap();
        g[0] += h + raw;
        g[1] += side;
    }
    (bits, dec)
}
