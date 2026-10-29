//! Model M: three context models per binary decision, mixed by a small online
//! logistic mixer (weights chosen by decision kind), then an adaptive
//! probability map. Measures how far mixing takes the size.

use super::modelf::ZZ;
use super::{bucket, Image};
use std::collections::HashMap;

fn var(name: &str) -> i32 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(0)
}

#[inline]
fn stretch(p: f64) -> f64 { (p / (1.0 - p)).ln() }
#[inline]
fn squash(x: f64) -> f64 { 1.0 / (1.0 + (-x).exp()) }

struct Mix {
    probs: Vec<HashMap<u64, (f64, f64)>>, // per model: fast and slow estimates of P(1)
    w: HashMap<u64, Vec<f64>>,
    apm: HashMap<u64, Vec<f64>>,
    bits: f64,
    n: u64,
    lr: f64,
}

impl Mix {
    fn new(m: usize) -> Mix {
        Mix { probs: (0..m).map(|_| HashMap::new()).collect(), w: HashMap::new(), apm: HashMap::new(), bits: 0.0, n: 0, lr: 0.02 }
    }
    /// One binary decision; `keys[i]` is model i's context, `wk` picks the weight set.
    fn code(&mut self, keys: &[u64], wk: u64, bit: bool) {
        let m = keys.len();
        let mut st = Vec::with_capacity(2 * m + 1);
        for (i, &k) in keys.iter().enumerate() {
            let (f, s) = *self.probs[i].entry(k).or_insert((0.5, 0.5));
            st.push(stretch(f.clamp(1e-4, 1.0 - 1e-4)));
            st.push(stretch(s.clamp(1e-4, 1.0 - 1e-4)));
        }
        st.push(0.3); // bias
        let w = self.w.entry(wk).or_insert_with(|| vec![0.3; 2 * m + 1]);
        let dot: f64 = w.iter().zip(&st).map(|(a, b)| a * b).sum();
        let pm = squash(dot).clamp(1e-5, 1.0 - 1e-5);
        // APM on the mixer output, 24 buckets by stretch, interpolated
        let a = self.apm.entry(wk).or_insert_with(|| (0..25).map(|j| squash((j as f64 - 12.0) / 2.0)).collect());
        let sx = (stretch(pm) * 2.0 + 12.0).clamp(0.0, 23.999);
        let j = sx.floor() as usize;
        let fr = sx - j as f64;
        let pa = (a[j] * (1.0 - fr) + a[j + 1] * fr).clamp(1e-5, 1.0 - 1e-5);
        let p = (pm + 3.0 * pa) / 4.0;
        let y = bit as u8 as f64;
        self.bits += -(if bit { p } else { 1.0 - p }).log2();
        self.n += 1;
        // updates
        let err = y - pm;
        for (wi, si) in w.iter_mut().zip(&st) { *wi += self.lr * err * si; }
        a[j] += (y - a[j]) * 0.02 * (1.0 - fr);
        a[j + 1] += (y - a[j + 1]) * 0.02 * fr;
        for (i, &k) in keys.iter().enumerate() {
            let e = self.probs[i].get_mut(&k).unwrap();
            e.0 += (y - e.0) / 16.0;
            e.1 += (y - e.1) / 128.0;
        }
    }
    /// Signed value with every model's base context; decision index is appended.
    fn signed(&mut self, bases: &[u64], kind: u64, v: i32) {
        let ks = |d: u64| -> Vec<u64> { bases.iter().map(|b| (b << 8) | d).collect() };
        self.code(&ks(0), (kind << 8) | 0, v != 0);
        if v == 0 { return; }
        let a = v.unsigned_abs();
        let e = 32 - a.leading_zeros();
        for i in 1..e { self.code(&ks(i as u64), (kind << 8) | i.min(6) as u64, true); }
        if e < 16 { self.code(&ks(e as u64), (kind << 8) | e.min(6) as u64, false); }
        for b in (0..e - 1).rev() {
            let bit = (a >> b) & 1 == 1;
            if b == e - 2 { self.code(&ks(0x80 | e as u64), (kind << 8) | 0x40 | e as u64, bit); } else { self.bits += 1.0; }
        }
        self.code(&ks(0xF0), (kind << 8) | 0xF0, v < 0);
    }
}

fn level(x8: u32) -> u64 {
    if x8 == 0 { return 0; }
    1 + (((x8 as f64 / 8.0 + 1.0).log2() * 2.0).round() as u64).min(30)
}

pub fn estimate_m(img: &Image) -> (f64, u64) {
    let _ = var("M_X");
    let (mut bits, mut n_dec) = (0.0, 0);
    for (ci, comp) in img.comps.iter().enumerate() {
        let cc = ci.min(1) as u64;
        let (bw, bh) = (comp.bw, comp.bh);
        let nat: Vec<[i32; 64]> = comp.coef.iter().map(|b| { let mut n = [0i32; 64]; for k in 0..64 { n[ZZ[k]] = b[k] as i32; } n }).collect();
        let nzc = |n: &[i32; 64]| n[1..].iter().filter(|&&v| v != 0).count() as u32;
        let mut mx = Mix::new(3);
        for by in 0..bh {
            for bx in 0..bw {
                let b = by * bw + bx;
                let cur = &nat[b];
                let up = if by > 0 { Some(&nat[b - bw]) } else { None };
                let lf = if bx > 0 { Some(&nat[b - 1]) } else { None };
                let ul = if by > 0 && bx > 0 { Some(&nat[b - bw - 1]) } else { None };
                let ur = if by > 0 && bx + 1 < bw { Some(&nat[b - bw + 1]) } else { None };
                let nu = up.map(nzc);
                let nl = lf.map(nzc);
                let nctx_b = match (nu, nl) { (Some(a), Some(b)) => bucket((a + b + 1) / 2), (Some(a), None) | (None, Some(a)) => bucket(a), _ => 15 };
                let (mut s, mut w) = (0u32, 0u32);
                for (nb, wt) in [(up, 2), (lf, 2), (ul, 1), (ur, 1)] { if let Some(n) = nb { s += wt * nzc(n); w += wt; } }
                let nctx_g = if w == 0 { 63 } else { ((2 * s + w) / (2 * w)) as u64 };
                let n = nzc(cur);
                let mut node = 1u64;
                for i in (0..6).rev() {
                    let bit = (n >> i) & 1 == 1;
                    let keys = [(cc << 40) | (3 << 32) | (nctx_b << 8) | node, (cc << 40) | (13 << 32) | (nctx_g.min(63) << 8) | node,
                        (cc << 40) | (23 << 32) | ((nu.unwrap_or(64) as u64) << 16) | ((nl.unwrap_or(64) as u64).min(63) << 8) | node];
                    mx.code(&keys, (cc << 20) | (1 << 16) | node, bit);
                    node = (node << 1) | bit as u64;
                }
                let mut left = n;
                let mut prev = 0u32;
                for k in 1..64 {
                    if left == 0 { break; }
                    let p = ZZ[k];
                    let nb = up.map_or(0, |u| u[p].unsigned_abs()) + lf.map_or(0, |l| l[p].unsigned_abs());
                    let (mut s8, mut w) = (0u32, 0u32);
                    for (nbk, wt) in [(up, 2), (lf, 2), (ul, 1), (ur, 1)] { if let Some(n) = nbk { s8 += 8 * wt * n[p].unsigned_abs(); w += wt; } }
                    let lv = if w == 0 { 63 } else { level(s8 / w) };
                    let lb = bucket(left).min(7);
                    let kk = k as u64;
                    let v = cur[p];
                    let bases = [
                        (cc << 40) | (4 << 32) | (kk << 16) | (bucket(nb).min(7) << 4) | lb,
                        (cc << 40) | (14 << 32) | (kk << 16) | (lv << 4) | lb,
                        (cc << 40) | (24 << 32) | (kk << 16) | (bucket(prev).min(7) << 8) | (bucket(nb).min(7) << 4) | bucket(n).min(7),
                    ];
                    mx.signed(&bases, (cc << 12) | (2 << 8) | (k as u64).min(15), v);
                    if v != 0 { left -= 1; }
                    prev = v.unsigned_abs();
                }
                let a = lf.map_or(0, |l| l[0]);
                let bb = up.map_or(0, |u| u[0]);
                let cdc = ul.map_or(0, |u| u[0]);
                let pr = match (lf.is_some(), up.is_some()) {
                    (true, true) => if cdc >= a.max(bb) { a.min(bb) } else if cdc <= a.min(bb) { a.max(bb) } else { a + bb - cdc },
                    (true, false) => a, (false, true) => bb, _ => 0,
                };
                let act = bucket(((a - cdc).unsigned_abs() + (bb - cdc).unsigned_abs()) as u32).min(10);
                let bases = [(cc << 40) | (5 << 32) | (act << 4) | bucket(n).min(7), (cc << 40) | (15 << 32) | act, (cc << 40) | (25 << 32) | bucket(n).min(7)];
                mx.signed(&bases, (cc << 12) | (3 << 8), cur[0] - pr);
            }
        }
        bits += mx.bits;
        n_dec += mx.n;
    }
    (bits, n_dec)
}
