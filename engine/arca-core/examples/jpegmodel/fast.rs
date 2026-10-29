//! The fast JPEG coefficient coder: model G + block copy, static tables per
//! image, two interleaved rANS states (12-bit frequencies, 16-bit words),
//! raw bits in a separate stream. One model walk serves both directions.
//!
//! Layout: [u32 side len][side][u32 raw len][raw][rANS words]
//! The side stream holds the tables, coded with the adaptive binary coder.

use super::codec::{Dec as BDec, Enc as BEnc, Rc};

const PB: u32 = 12;
const M: u32 = 1 << PB;
const L: u32 = 1 << 16;

// symbol types: (context space, alphabet)
const T_N: usize = 0;
const T_MF: usize = 1;
const T_MD: usize = 2;
const T_AC: usize = 3;
const T_TOP: usize = 4;
const T_SIGN: usize = 5;
const T_DC: usize = 6;
const T_DCTOP: usize = 7;
const TYPES: [(usize, usize); 8] = [(2 * 9, 64), (32, 2), (16, 32), (2 * 64 * 32 * 8, 32), (2 * 16 * 16, 2), (16, 2), (2 * 11 * 8, 32), (32, 2)];

/// One symbol per value: 0 = zero, 1 = magnitude 1, then (category e >= 2, top mantissa bit)
/// as 2 + 2 (e - 2) + top. Sign and the bits below the top one are raw.
#[inline(always)]
fn joint(a: u32) -> (usize, u32) {
    if a == 0 { return (0, 0); }
    let e = 32 - a.leading_zeros();
    if e == 1 { return (1, 1); }
    (2 + 2 * (e as usize - 2) + ((a >> (e - 2)) & 1) as usize, e)
}

/// Inverse: (magnitude without the low bits, number of raw low bits).
const SPLIT: [(u32, u32); 32] = {
    let mut t = [(0u32, 0u32); 32];
    t[1] = (1, 0);
    let mut s = 2;
    while s < 32 {
        let e = (s - 2) / 2 + 2;
        let top = ((s - 2) % 2) as u32;
        t[s] = (((2 | top) << (e - 2)) as u32, (e - 2) as u32);
        s += 1;
    }
    t
};

const SIGN_RAW: bool = true;
const USE_N: bool = true;

#[inline(always)]
fn bucket(x: u32) -> usize {
    if x < 3 { x as usize } else { 1 + (32 - (x - 1).leading_zeros()) as usize }
}

static LEVELS: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
const RECIP: [u32; 7] = [0, 65536, 32768, 21845, 16384, 13107, 10922];

/// 1 + round(2 log2(x8 / 8 + 1)), capped at 30; from a table.
#[inline(always)]
fn level(s8: u32, w: u32, lv: &[u8]) -> usize {
    let x8 = ((s8 as u64 * RECIP[w as usize] as u64) >> 16) as usize;
    lv[x8.min((1 << 18) - 1)] as usize
}

pub fn levels() -> &'static [u8] {
    LEVELS.get_or_init(|| {
        (0..1u32 << 18).map(|x8| if x8 == 0 { 0 } else { 1 + ((((x8 as f64) / 8.0 + 1.0).log2() * 2.0).round() as u8).min(29) }).collect()
    })
}

pub trait Io {
    const ENC: bool;
    fn sym<const A: usize>(&mut self, t: usize, ctx: usize, s: usize) -> usize;
    fn raw(&mut self, v: u32, n: u32) -> u32;
}

/// Coefficients of one component in zigzag order, block raster.
pub struct Plane<'a> {
    pub coef: &'a mut [[i16; 64]],
    pub bw: usize,
    pub bh: usize,
}

#[inline(always)]
fn value<I: Io>(io: &mut I, ctx: usize, _cc: usize, _k: usize, v: i32, _sctx: usize) -> i32 {
    // branch-free: the low bits and the sign come in one raw read
    let a = v.unsigned_abs();
    let sy = io.sym::<32>(T_AC, ctx, if I::ENC { joint(a).0 } else { 0 });
    let (hi, nlow) = SPLIT[sy];
    let ns = (sy != 0) as u32;
    let enc = if I::ENC { (a & ((1u32 << nlow) - 1)) | (((v < 0) as u32) << nlow) } else { 0 };
    let bits = io.raw(enc, nlow + ns);
    let m = (hi | (bits & ((1u32 << nlow) - 1))) as i32;
    let sign = ((bits >> nlow) & ns) as i32;
    (m ^ -sign) + sign
}

/// The model walk: codes (or decodes into) every block of every plane.
pub fn walk<I: Io>(io: &mut I, planes: &mut [Plane]) {
    let lvt = levels();
    let fixed = std::env::var_os("FIXED_CTX").is_some();
    for (ci, pl) in planes.iter_mut().enumerate() {
        let cc = ci.min(1);
        let (bw, bh) = (pl.bw, pl.bh);
        let coef = &mut *pl.coef;
        let mut nzs = vec![0u8; bw * bh]; // nonzero AC counts of coded blocks
        // block copy: per nonzero count, every dense block so far (block index), and on the
        // encoder a map from AC pattern to its latest position in that list
        let mut lists: Vec<Vec<u32>> = vec![Vec::new(); 64];
        let mut seen: std::collections::HashMap<[i16; 63], u32> = Default::default();
        let mut prev_match = 0usize;
        for by in 0..bh {
            for bx in 0..bw {
                let b = by * bw + bx;
                let has_up = by > 0;
                let has_lf = bx > 0;
                let has_ul = has_up && has_lf;
                let has_ur = has_up && bx + 1 < bw;
                // nonzero count from weighted neighbour counts
                let (mut s, mut w) = (0u32, 0u32);
                if has_up { s += 2 * nzs[b - bw] as u32; w += 2; }
                if has_lf { s += 2 * nzs[b - 1] as u32; w += 2; }
                if has_ul { s += nzs[b - bw - 1] as u32; w += 1; }
                if has_ur { s += nzs[b - bw + 1] as u32; w += 1; }
                let nctx = if w == 0 { 8 } else { bucket((2 * s + w) / (2 * w)).min(7) };
                let n_enc = if I::ENC { coef[b][1..].iter().filter(|&&v| v != 0).count() } else { 0 };
                let n = io.sym::<64>(T_N, cc * 9 + nctx, n_enc) as u32;
                nzs[b] = n as u8;
                let mut blk = if I::ENC { coef[b] } else { [0i16; 64] };
                let mut copied = false;
                if n >= 6 {
                    let mut key = [0i16; 63];
                    if I::ENC {
                        key.copy_from_slice(&blk[1..]);
                    }
                    let hit = if I::ENC { seen.get(&key).copied() } else { None };
                    let fctx = cc * 16 + bucket(n).min(7) * 2 + prev_match;
                    let m = io.sym::<2>(T_MF, fctx, hit.is_some() as usize) == 1;
                    let list = &mut lists[n as usize];
                    if m {
                        let d_enc = if I::ENC { list.len() as u32 - hit.unwrap() } else { 0 };
                        let e = io.sym::<32>(T_MD, cc * 8 + bucket(n).min(7), if I::ENC { (32 - d_enc.leading_zeros()) as usize } else { 0 }) as u32;
                        let low = io.raw(d_enc & ((1u32 << (e - 1)) - 1), e - 1);
                        let d = (1u32 << (e - 1)) | low;
                        let src = list[list.len() - d as usize] as usize;
                        if !I::ENC {
                            let ac = coef[src];
                            blk[1..].copy_from_slice(&ac[1..]);
                        }
                        copied = true;
                    }
                    if I::ENC {
                        seen.insert(key, list.len() as u32);
                    }
                    list.push(b as u32);
                    prev_match = m as usize;
                }
                if !copied && n > 0 {
                    // expected-magnitude level for every position at once (NEON)
                    let mut ctxs = [0u32; 64];
                    unsafe {
                        use std::arch::aarch64::*;
                        let w = 2 * has_up as u32 + 2 * has_lf as u32 + has_ul as u32 + has_ur as u32;
                        let lb = bucket(n).min(7) as u32;
                        if w == 0 {
                            for k in 1..64 { ctxs[k] = ((((cc * 64 + k) * 32 + 31) * 8) as u32) + lb; }
                        } else {
                            let zero = [0i16; 64];
                            let pu = if has_up { coef[b - bw].as_ptr() } else { zero.as_ptr() };
                            let pl = if has_lf { coef[b - 1].as_ptr() } else { zero.as_ptr() };
                            let pul = if has_ul { coef[b - bw - 1].as_ptr() } else { zero.as_ptr() };
                            let pur = if has_ur { coef[b - bw + 1].as_ptr() } else { zero.as_ptr() };
                            let cap = vdupq_n_u16(1023);
                            let rc = vdupq_n_u32(RECIP[w as usize]);
                            for j in 0..8 {
                                let a = |q: *const i16| vminq_u16(vreinterpretq_u16_s16(vabsq_s16(vld1q_s16(q.add(8 * j)))), cap);
                                // 16|up| + 16|lf| + 8|ul| + 8|ur|, at most 48 * 1023
                                let s16 = vaddq_u16(vshlq_n_u16::<4>(vaddq_u16(a(pu), a(pl))), vshlq_n_u16::<3>(vaddq_u16(a(pul), a(pur))));
                                let lo = vshrq_n_u32::<16>(vmulq_u32(vmovl_u16(vget_low_u16(s16)), rc));
                                let hi = vshrq_n_u32::<16>(vmulq_u32(vmovl_high_u16(s16), rc));
                                // level: 0 for 0, else clamp(2m + half - 5, 1, 30) with m = floor(log2(x8 + 8))
                                for (h, x8) in [lo, hi].into_iter().enumerate() {
                                    let y = vaddq_u32(x8, vdupq_n_u32(8));
                                    let m = vsubq_u32(vdupq_n_u32(31), vclzq_u32(y));
                                    let sh = vnegq_s32(vreinterpretq_s32_u32(vsubq_u32(m, vdupq_n_u32(1))));
                                    let half = vandq_u32(vshlq_u32(y, sh), vdupq_n_u32(1));
                                    let l = vreinterpretq_s32_u32(vaddq_u32(vshlq_n_u32::<1>(m), half));
                                    let l = vminq_s32(vmaxq_s32(vsubq_s32(l, vdupq_n_s32(5)), vdupq_n_s32(1)), vdupq_n_s32(30));
                                    let l = vbslq_u32(vceqq_u32(x8, vdupq_n_u32(0)), vdupq_n_u32(0), vreinterpretq_u32_s32(l));
                                    let k0 = (8 * j + 4 * h) as u32;
                                    let kv = vaddq_u32(vdupq_n_u32(k0), vld1q_u32([0u32, 1, 2, 3].as_ptr()));
                                    // ((cc*64 + k)*32 + lv)*8 + lb
                                    let base = vshlq_n_u32::<5>(vaddq_u32(vdupq_n_u32((cc * 64) as u32), kv));
                                    let c = vaddq_u32(vshlq_n_u32::<3>(vaddq_u32(base, l)), vdupq_n_u32(lb));
                                    vst1q_u32(ctxs.as_mut_ptr().add(k0 as usize), c);
                                }
                            }
                        }
                    }
                    if fixed {
                        for k in 1..64 { ctxs[k] = ((cc * 64 + k) * 32 * 8) as u32; }
                    }
                    let mut left = n;
                    let mut k = 1;
                    while left > 0 {
                        let v = value(io, ctxs[k] as usize, cc, k, blk[k] as i32, 5);
                        blk[k] = v as i16;
                        left -= (v != 0) as u32;
                        k += 1;
                    }
                }
                // DC: LOCO-I from neighbour DCs
                let a = if has_lf { coef[b - 1][0] as i32 } else { 0 };
                let bb = if has_up { coef[b - bw][0] as i32 } else { 0 };
                let c = if has_ul { coef[b - bw - 1][0] as i32 } else { 0 };
                let pred = match (has_lf, has_up) {
                    (true, true) => if c >= a.max(bb) { a.min(bb) } else if c <= a.min(bb) { a.max(bb) } else { a + bb - c },
                    (true, false) => a,
                    (false, true) => bb,
                    _ => 0,
                };
                let act = bucket(((a - c).unsigned_abs() + (bb - c).unsigned_abs()) as u32).min(10);
                let r_enc = if I::ENC { blk[0] as i32 - pred } else { 0 };
                let ra = r_enc.unsigned_abs();
                let sy = io.sym::<32>(T_DC, (cc * 11 + act) * 8 + bucket(n).min(7), if I::ENC { joint(ra).0 } else { 0 });
                let mut r = 0i32;
                if sy > 0 {
                    let (hi, nlow) = SPLIT[sy];
                    let m = hi | io.raw(ra & ((1u32 << nlow) - 1), nlow);
                    let neg = io.raw((r_enc < 0) as u32, 1) == 1;
                    r = if neg { -(m as i32) } else { m as i32 };
                }
                blk[0] = (pred + r) as i16;
                if !I::ENC {
                    coef[b] = blk;
                }
            }
        }
    }
}

// ---------- encoder ----------

struct Recorder {
    events: Vec<(u8, u32, u8)>,
    counts: Vec<Vec<u32>>,
    raw: BitW,
}

impl Io for Recorder {
    const ENC: bool = true;
    #[inline(always)]
    fn sym<const A: usize>(&mut self, t: usize, ctx: usize, s: usize) -> usize {
        self.events.push((t as u8, ctx as u32, s as u8));
        self.counts[t][ctx * TYPES[t].1 + s] += 1;
        s
    }
    #[inline(always)]
    fn raw(&mut self, v: u32, n: u32) -> u32 {
        self.raw.put(v, n);
        v
    }
}

pub struct BitW {
    pub out: Vec<u8>,
    acc: u64,
    n: u32,
}
impl BitW {
    fn new() -> BitW { BitW { out: Vec::new(), acc: 0, n: 0 } }
    #[inline(always)]
    fn put(&mut self, v: u32, n: u32) {
        if n == 0 { return; }
        self.acc |= (v as u64) << self.n;
        self.n += n;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }
    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 { self.out.push(self.acc as u8); }
        self.out.extend_from_slice(&[0; 8]);
        self.out
    }
}

/// Approximate counts from half-octave levels: q = 0 absent, else 1 + floor(2 log2 c).
fn quant(c: u32) -> u32 {
    if c == 0 { 0 } else { 1 + ((c as f64).log2() * 2.0).floor() as u32 }.min(40)
}
const DEQ: [u32; 64] = {
    // round(2^((q - 1) / 2)): odd steps use 1.41421356 as 46341/32768
    let mut t = [0u32; 64];
    let mut q = 1;
    while q < 64 {
        let e = (q - 1) / 2;
        t[q] = if (q - 1) % 2 == 0 { 1u32 << e } else { (((46341u64 << e) + 16384) >> 15) as u32 };
        q += 1;
    }
    t
};
fn dequant(q: u32) -> u32 {
    DEQ[q as usize]
}

/// Normalises approximate counts to frequencies summing to M (present symbols >= 1).
fn normalise(w: &[u32], freq: &mut [u32]) {
    let tot: u64 = w.iter().map(|&x| x as u64).sum();
    let mut sum = 0u32;
    let mut big = 0;
    for (i, &x) in w.iter().enumerate() {
        freq[i] = if x == 0 { 0 } else { ((x as u64 * M as u64) / tot).max(1) as u32 };
        sum += freq[i];
        if freq[i] > freq[big] { big = i; }
    }
    if sum > M { freq[big] -= sum - M; } else { freq[big] += M - sum; }
}

/// Side stream: which contexts are used and their quantised counts.
fn code_tables<C: Rc>(c: &mut C, q: &mut [Vec<u8>], used: &mut [Vec<bool>]) {
    let mut p_used = vec![2048u16; 8 * 2];
    let mut p_max = vec![2048u16; 8 * 64];
    let mut p_q = vec![2048u16; 8 * 16 * 64];
    for t in 0..8 {
        let (space, a) = TYPES[t];
        let abits = (a as u32).trailing_zeros();
        let mut prev = 0usize;
        for ctx in 0..space {
            let u = c.bit(&mut p_used[t * 2 + prev], used[t][ctx]);
            used[t][ctx] = u;
            prev = u as usize;
            if !u { continue; }
            // largest present symbol, then each level up to it
            let amax_enc = if C::ENC { (0..a).rev().find(|&s| q[t][ctx * a + s] > 0).unwrap_or(0) } else { 0 };
            let mut node = 1usize;
            for i in (0..abits).rev() {
                let bit = c.bit(&mut p_max[t * 64 + node.min(63)], (amax_enc >> i) & 1 == 1);
                node = (node << 1) | bit as usize;
            }
            let amax = node - (1 << abits);
            for s in 0..=amax {
                let qe = if C::ENC { q[t][ctx * a + s] as u32 } else { 0 };
                let mut node = 1usize;
                for i in (0..6).rev() {
                    let bit = c.bit(&mut p_q[(t * 16 + s.min(15)) * 64 + node], (qe >> i) & 1 == 1);
                    node = (node << 1) | bit as usize;
                }
                q[t][ctx * a + s] = (node - 64) as u8;
            }
        }
    }
}

pub fn encode(planes: &mut [Plane]) -> Vec<u8> {
    let mut rec = Recorder { events: Vec::new(), counts: TYPES.iter().map(|&(s, a)| vec![0; s * a]).collect(), raw: BitW::new() };
    walk(&mut rec, planes);
    {
        let ac = rec.events.iter().filter(|e| e.0 as usize == T_AC).count() as u64;
        let z = rec.events.iter().filter(|e| e.0 as usize == T_AC && e.2 == 0).count() as u64;
        N_AC.fetch_add(ac, std::sync::atomic::Ordering::Relaxed);
        T_ZERO.fetch_add(z, std::sync::atomic::Ordering::Relaxed);
    }
    // quantise counts, code the tables, then rebuild the exact frequencies the decoder will use
    let mut q: Vec<Vec<u8>> = rec.counts.iter().map(|v| v.iter().map(|&c| quant(c) as u8).collect()).collect();
    let mut used: Vec<Vec<bool>> = TYPES.iter().enumerate().map(|(t, &(s, a))| (0..s).map(|c| rec.counts[t][c * a..(c + 1) * a].iter().any(|&x| x > 0)).collect()).collect();
    let mut be = BEnc::new();
    code_tables(&mut be, &mut q, &mut used);
    let side = be.finish();
    let (freqs, cums) = build(&q, &used);
    // rANS, backwards, two states
    let mut x = [L, L];
    let mut words: Vec<u16> = Vec::with_capacity(rec.events.len());
    for (i, &(t, ctx, s)) in rec.events.iter().enumerate().rev() {
        let a = TYPES[t as usize].1;
        let base = ctx as usize * a + s as usize;
        let f = freqs[t as usize][base];
        let st = cums[t as usize][ctx as usize * (a + 1) + s as usize] as u32;
        let xs = &mut x[i & 1];
        let xmax = (((L >> PB) << 16) as u64) * f as u64;
        if *xs as u64 >= xmax {
            words.push(*xs as u16);
            *xs >>= 16;
        }
        *xs = ((*xs / f) << PB) + (*xs % f) + st;
    }
    for xs in [x[1], x[0]] {
        words.push((xs >> 16) as u16);
        words.push(xs as u16);
    }
    words.reverse();
    let raw = rec.raw.finish();
    let mut out = Vec::with_capacity(side.len() + raw.len() + 2 * words.len() + 8);
    out.extend_from_slice(&(side.len() as u32).to_le_bytes());
    out.extend_from_slice(&side);
    out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    out.extend_from_slice(&raw);
    for w in words {
        out.extend_from_slice(&w.to_le_bytes());
    }
    out
}

/// Frequencies and cumulative tables from the quantised counts.
fn build(q: &[Vec<u8>], used: &[Vec<bool>]) -> (Vec<Vec<u32>>, Vec<Vec<u16>>) {
    let mut freqs = Vec::new();
    let mut cums = Vec::new();
    for t in 0..8 {
        let (space, a) = TYPES[t];
        let mut f = vec![0u32; space * a];
        // cum has a+1 entries per context; unused symbols get an empty range
        let mut cu = vec![0u16; space * (a + 1)];
        let mut w = vec![0u32; a];
        for ctx in 0..space {
            if !used[t][ctx] { continue; }
            for s in 0..a { w[s] = dequant(q[t][ctx * a + s] as u32); }
            normalise(&w, &mut f[ctx * a..(ctx + 1) * a]);
            let mut acc = 0u32;
            for s in 0..a {
                cu[ctx * (a + 1) + s] = acc as u16;
                acc += f[ctx * a + s];
            }
            // M itself doesn't fit u16 when acc = 4096; store 4096 (fits: 4096 < 65536)
            cu[ctx * (a + 1) + a] = acc as u16;
        }
        freqs.push(f);
        cums.push(cu);
    }
    (freqs, cums)
}

// ---------- decoder ----------

pub struct Decoder<'a> {
    map: Vec<Vec<u16>>,
    flat: Vec<u16>,
    base: [usize; 8],
    bk: Vec<u8>,
    bbase: [usize; 8],
    x0: u32,
    x1: u32,
    i: usize,
    words: &'a [u8],
    wp: usize,
    raw: &'a [u8],
    rp: usize,
    acc: u64,
    n: u32,
}

impl<'a> Io for Decoder<'a> {
    const ENC: bool = false;
    #[inline(always)]
    fn sym<const A: usize>(&mut self, t: usize, ctx: usize, _s: usize) -> usize {
        let ctx = unsafe { *self.map.get_unchecked(t).get_unchecked(ctx) } as usize;
        let base = self.base[t] + ctx * (A + 1);
        self.i += 1;
        let xs = self.x0;
        let slot = (xs & (M - 1)) as u16;
        let mut s: usize;
        let nx;
        unsafe {
            let p = self.flat.as_ptr().add(base);
            if A == 2 {
                s = (*p.add(1) <= slot) as usize;
            } else {
                // bucket table: first symbol of the 64-slot bucket, then step forward
                s = *self.bk.as_ptr().add(self.bbase[t] + ctx * 64 + (slot >> 6) as usize) as usize;
                while *p.add(s + 1) <= slot {
                    s += 1;
                }
            }
            let st = *p.add(s) as u32;
            let f = *p.add(s + 1) as u32 - st;
            let mut x = f * (xs >> PB) + slot as u32 - st;
            if x < L {
                let w = u16::from_le_bytes([*self.words.get_unchecked(self.wp), *self.words.get_unchecked(self.wp + 1)]);
                self.wp += 2;
                x = (x << 16) | w as u32;
            }
            nx = x;
        }
        self.x0 = self.x1;
        self.x1 = nx;
        s
    }
    #[inline(always)]
    fn raw(&mut self, _v: u32, n: u32) -> u32 {
        // branchless refill (the raw stream is padded with 8 zero bytes)
        if self.n < 32 {
            let w = unsafe { std::ptr::read_unaligned(self.raw.as_ptr().add(self.rp) as *const u64) };
            self.acc |= u64::from_le(w) << self.n;
            let take = (63 - self.n) >> 3;
            self.rp += take as usize;
            self.n += take << 3;
        }
        let v = (self.acc & ((1u64 << n) - 1)) as u32;
        self.acc >>= n;
        self.n -= n;
        v
    }
}

pub static T_TABLES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static T_ZERO: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static N_AC: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static T_SYMS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn decode(data: &[u8], planes: &mut [Plane]) {
    let t0 = std::time::Instant::now();
    let sl = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    let side = &data[4..4 + sl];
    let rl = u32::from_le_bytes(data[4 + sl..8 + sl].try_into().unwrap()) as usize;
    let raw = &data[8 + sl..8 + sl + rl];
    let words = &data[8 + sl + rl..];
    let mut q: Vec<Vec<u8>> = TYPES.iter().map(|&(s, a)| vec![0u8; s * a]).collect();
    let mut used: Vec<Vec<bool>> = TYPES.iter().map(|&(s, _)| vec![false; s]).collect();
    let mut bd = BDec::new(side);
    code_tables(&mut bd, &mut q, &mut used);
    // dense tables for the used contexts only
    let mut map: Vec<Vec<u16>> = Vec::with_capacity(8);
    let mut base = [0usize; 8];
    let mut bbase = [0usize; 8];
    let mut flat: Vec<u16> = Vec::new();
    let mut bk: Vec<u8> = Vec::new();
    let mut w = [0u32; 64];
    let mut f = [0u32; 64];
    for t in 0..8 {
        let (space, a) = TYPES[t];
        base[t] = flat.len();
        bbase[t] = bk.len();
        let mut m = vec![u16::MAX; space];
        let mut dense = 0u16;
        for ctx in 0..space {
            if !used[t][ctx] { continue; }
            m[ctx] = dense;
            dense += 1;
            for s in 0..a { w[s] = dequant(q[t][ctx * a + s] as u32); }
            normalise(&w[..a], &mut f[..a]);
            let start = flat.len();
            let mut acc = 0u32;
            for s in 0..a {
                flat.push(acc as u16);
                acc += f[s];
            }
            flat.push(acc as u16);
            if a > 2 {
                let c = &flat[start..start + a + 1];
                let mut sym = 0usize;
                for b in 0..64 {
                    let lo = (b * 64) as u16;
                    while c[sym + 1] <= lo { sym += 1; }
                    bk.push(sym as u8);
                }
            }
        }
        map.push(m);
    }
    flat.extend_from_slice(&[u16::MAX; 16]);
    let rd = |p: usize| (u16::from_le_bytes([words[p], words[p + 1]]) as u32);
    let x0 = rd(0) | (rd(2) << 16);
    let x1 = rd(4) | (rd(6) << 16);
    let mut words = words.to_vec();
    words.extend_from_slice(&[0; 8]);
    let words: &[u8] = &words;
    let mut d = Decoder { map, flat, base, bk, bbase, x0, x1, i: 0, words, wp: 8, raw, rp: 0, acc: 0, n: 0 };
    T_TABLES.fetch_add(t0.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
    walk(&mut d, planes);
    T_SYMS.fetch_add(d.i as u64, std::sync::atomic::Ordering::Relaxed);
}

pub fn sizes(data: &[u8]) -> (usize, usize, usize) {
    let sl = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    let rl = u32::from_le_bytes(data[4 + sl..8 + sl].try_into().unwrap()) as usize;
    (sl, rl, data.len() - 8 - sl - rl)
}
