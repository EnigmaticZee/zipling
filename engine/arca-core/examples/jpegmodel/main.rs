//! Size estimate for a light JPEG coefficient coder.
//! Decodes baseline JPEGs to quantised coefficients, then charges every binary
//! decision of a candidate model to an adaptive probability (LZMA-style, 12-bit),
//! so the totals are what a real adaptive binary coder would write.
//!
//!   jpegest <dir>

mod codec;
mod modelf;
mod modelg;
mod modelm;
mod fast;
mod modelh;
mod modelr;
mod lite;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

// ---------- JPEG reading ----------

#[derive(Clone, Default)]
struct Huff {
    // canonical decode: for each length, (first code, first index, count)
    maxcode: [i32; 18],
    valptr: [i32; 17],
    mincode: [i32; 17],
    vals: Vec<u8>,
}

impl Huff {
    fn new(counts: &[u8; 16], vals: &[u8]) -> Huff {
        let mut h = Huff { vals: vals.to_vec(), ..Default::default() };
        let mut code = 0i32;
        let mut k = 0i32;
        for l in 1..=16 {
            let n = counts[l - 1] as i32;
            if n > 0 {
                h.valptr[l] = k;
                h.mincode[l] = code;
                code += n;
                k += n;
                h.maxcode[l] = code - 1;
            } else {
                h.maxcode[l] = -1;
            }
            code <<= 1;
        }
        h.maxcode[17] = i32::MAX;
        h
    }
}

struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
    acc: u64,
    n: u32,
    hit_marker: bool,
}

impl<'a> Bits<'a> {
    fn fill(&mut self) {
        while self.n <= 56 {
            let mut b = 0u8;
            if !self.hit_marker && self.pos < self.d.len() {
                b = self.d[self.pos];
                if b == 0xFF {
                    let nx = *self.d.get(self.pos + 1).unwrap_or(&0xD9);
                    if nx == 0 {
                        self.pos += 2;
                    } else {
                        self.hit_marker = true;
                        b = 0;
                    }
                } else {
                    self.pos += 1;
                }
            }
            self.acc |= (b as u64) << (56 - self.n);
            self.n += 8;
        }
    }
    fn bit(&mut self) -> u32 {
        if self.n == 0 {
            self.fill();
        }
        let b = (self.acc >> 63) as u32;
        self.acc <<= 1;
        self.n -= 1;
        b
    }
    fn get(&mut self, k: u32) -> u32 {
        if k == 0 {
            return 0;
        }
        if self.n < k {
            self.fill();
        }
        let v = (self.acc >> (64 - k)) as u32;
        self.acc <<= k;
        self.n -= k;
        v
    }
    fn decode(&mut self, h: &Huff) -> Option<u8> {
        let mut code = 0i32;
        for l in 1..=16 {
            code = (code << 1) | self.bit() as i32;
            if code <= h.maxcode[l] {
                return h.vals.get((h.valptr[l] + code - h.mincode[l]) as usize).copied();
            }
        }
        None
    }
    fn restart(&mut self) {
        // drop buffered bits, skip to the RST marker and past it
        self.acc = 0;
        self.n = 0;
        if self.hit_marker {
            self.hit_marker = false;
        }
        while self.pos + 1 < self.d.len() {
            if self.d[self.pos] == 0xFF && (0xD0..=0xD7).contains(&self.d[self.pos + 1]) {
                self.pos += 2;
                return;
            }
            self.pos += 1;
        }
    }
}

fn extend(v: u32, s: u32) -> i32 {
    if s == 0 {
        0
    } else if v < (1 << (s - 1)) {
        v as i32 - (1 << s) + 1
    } else {
        v as i32
    }
}

pub struct Comp {
    pub id: u8,
    pub tq: u8,
    pub h: usize,
    pub v: usize,
    pub bw: usize, // blocks per row (padded to MCU)
    pub bh: usize,
    pub coef: Vec<[i16; 64]>, // zigzag order
}

pub struct Scan {
    pub comps: Vec<(usize, u8, u8)>,
    pub start: usize,
    pub end: usize,
}

pub struct Image {
    pub comps: Vec<Comp>,
    pub scans: Vec<Scan>,
    pub dc: HashMap<u8, ([u8; 16], Vec<u8>)>,
    pub ac: HashMap<u8, ([u8; 16], Vec<u8>)>,
    pub dri: usize,
    pub width: usize,
    pub height: usize,
    pub hmax: usize,
    pub vmax: usize,
    pub qt: HashMap<u8, [u16; 64]>,
}

fn read_jpeg(d: &[u8]) -> Result<Image, String> {
    if d.len() < 4 || d[0] != 0xFF || d[1] != 0xD8 {
        return Err("not jpeg".into());
    }
    let mut i = 2;
    let mut dc: HashMap<u8, Huff> = HashMap::new();
    let mut ac: HashMap<u8, Huff> = HashMap::new();
    let mut comps: Vec<Comp> = Vec::new();
    let (mut hmax, mut vmax, mut width, mut height) = (1, 1, 0usize, 0usize);
    let mut dri = 0usize;
    let mut scans = 0;
    let mut scan_list: Vec<Scan> = vec![];
    let mut qt: HashMap<u8, [u16; 64]> = HashMap::new();
    let mut dc_raw: HashMap<u8, ([u8; 16], Vec<u8>)> = HashMap::new();
    let mut ac_raw: HashMap<u8, ([u8; 16], Vec<u8>)> = HashMap::new();
    while i + 4 <= d.len() {
        if d[i] != 0xFF {
            return Err(format!("bad marker at {i}"));
        }
        let m = d[i + 1];
        if m == 0xD9 {
            break;
        }
        if m == 0xFF {
            i += 1;
            continue;
        }
        let ln = u16::from_be_bytes([d[i + 2], d[i + 3]]) as usize;
        let seg = &d[i + 4..(i + 2 + ln).min(d.len())];
        match m {
            0xC0 | 0xC1 => {
                height = u16::from_be_bytes([seg[1], seg[2]]) as usize;
                width = u16::from_be_bytes([seg[3], seg[4]]) as usize;
                let nc = seg[5] as usize;
                for c in 0..nc {
                    let hv = seg[7 + 3 * c];
                    comps.push(Comp { id: seg[6 + 3 * c], tq: seg[8 + 3 * c], h: (hv >> 4) as usize, v: (hv & 15) as usize, bw: 0, bh: 0, coef: vec![] });
                }
                hmax = comps.iter().map(|c| c.h).max().unwrap();
                vmax = comps.iter().map(|c| c.v).max().unwrap();
                let mcux = (width + 8 * hmax - 1) / (8 * hmax);
                let mcuy = (height + 8 * vmax - 1) / (8 * vmax);
                for c in comps.iter_mut() {
                    c.bw = mcux * c.h;
                    c.bh = mcuy * c.v;
                    c.coef = vec![[0i16; 64]; c.bw * c.bh];
                }
            }
            0xC2 | 0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => return Err("not baseline".into()),
            0xC4 => {
                let mut p = 0;
                while p + 17 <= seg.len() {
                    let tc = seg[p] >> 4;
                    let th = seg[p] & 15;
                    let mut counts = [0u8; 16];
                    counts.copy_from_slice(&seg[p + 1..p + 17]);
                    let n: usize = counts.iter().map(|&x| x as usize).sum();
                    let h = Huff::new(&counts, &seg[p + 17..p + 17 + n]);
                    let raw = (counts, seg[p + 17..p + 17 + n].to_vec());
                    if tc == 0 { dc.insert(th, h); dc_raw.insert(th, raw); } else { ac.insert(th, h); ac_raw.insert(th, raw); }
                    p += 17 + n;
                }
            }
            0xDB => {
                let mut p = 0;
                while p < seg.len() {
                    let pq = seg[p] >> 4;
                    let tq = seg[p] & 15;
                    let mut t = [0u16; 64];
                    for k in 0..64 {
                        let v = if pq == 0 { seg[p + 1 + k] as u16 } else { u16::from_be_bytes([seg[p + 1 + 2 * k], seg[p + 2 + 2 * k]]) };
                        t[modelf::ZZ[k]] = v;
                    }
                    qt.insert(tq, t);
                    p += 1 + 64 * (1 + pq as usize);
                }
            }
            0xDD => dri = u16::from_be_bytes([seg[0], seg[1]]) as usize,
            0xDA => {
                scans += 1;
                let ns = seg[0] as usize;
                let mut sc = vec![];
                for k in 0..ns {
                    let cid = seg[1 + 2 * k];
                    let t = seg[2 + 2 * k];
                    let ci = comps.iter().position(|c| c.id == cid).ok_or("bad comp")?;
                    sc.push((ci, t >> 4, t & 15));
                }
                let start = i + 2 + ln;
                let mut bits = Bits { d, pos: start, acc: 0, n: 0, hit_marker: false };
                let mut pred = vec![0i32; comps.len()];
                let decode_block = |bits: &mut Bits, ci: usize, td: u8, ta: u8, pred: &mut Vec<i32>, blk: &mut [i16; 64]| -> Result<(), String> {
                    let s = bits.decode(dc.get(&td).ok_or("no dc table")?).ok_or("bad dc")? as u32;
                    let diff = extend(bits.get(s), s);
                    pred[ci] += diff;
                    blk[0] = pred[ci] as i16;
                    let t = ac.get(&ta).ok_or("no ac table")?;
                    let mut k = 1;
                    while k < 64 {
                        let rs = bits.decode(t).ok_or("bad ac")? as u32;
                        let (r, s) = (rs >> 4, rs & 15);
                        if s == 0 {
                            if r == 15 { k += 16; continue; }
                            break;
                        }
                        k += r as usize;
                        if k > 63 { return Err("ac overrun".into()); }
                        blk[k] = extend(bits.get(s), s) as i16;
                        k += 1;
                    }
                    Ok(())
                };
                let mut mcu_count = 0usize;
                if ns == 1 {
                    let (ci, td, ta) = sc[0];
                    let c = &comps[ci];
                    let cw = (width * c.h + 8 * hmax - 1) / (8 * hmax);
                    let ch = (height * c.v + 8 * vmax - 1) / (8 * vmax);
                    let bw = c.bw;
                    let mut coef = std::mem::take(&mut comps[ci].coef);
                    for by in 0..ch {
                        for bx in 0..cw {
                            if dri > 0 && mcu_count > 0 && mcu_count % dri == 0 {
                                bits.restart();
                                pred.iter_mut().for_each(|p| *p = 0);
                            }
                            decode_block(&mut bits, ci, td, ta, &mut pred, &mut coef[by * bw + bx])?;
                            mcu_count += 1;
                        }
                    }
                    comps[ci].coef = coef;
                } else {
                    let mcux = (width + 8 * hmax - 1) / (8 * hmax);
                    let mcuy = (height + 8 * vmax - 1) / (8 * vmax);
                    let mut coefs: Vec<Vec<[i16; 64]>> = comps.iter_mut().map(|c| std::mem::take(&mut c.coef)).collect();
                    for my in 0..mcuy {
                        for mx in 0..mcux {
                            if dri > 0 && mcu_count > 0 && mcu_count % dri == 0 {
                                bits.restart();
                                pred.iter_mut().for_each(|p| *p = 0);
                            }
                            for &(ci, td, ta) in &sc {
                                let (h, v, bw) = (comps[ci].h, comps[ci].v, comps[ci].bw);
                                for y in 0..v {
                                    for x in 0..h {
                                        let idx = (my * v + y) * bw + mx * h + x;
                                        decode_block(&mut bits, ci, td, ta, &mut pred, &mut coefs[ci][idx])?;
                                    }
                                }
                            }
                            mcu_count += 1;
                        }
                    }
                    for (c, v) in comps.iter_mut().zip(coefs) {
                        c.coef = v;
                    }
                }
                // continue after the scan: find the next marker that is not RST/stuffing
                let mut j = bits.pos;
                while j + 1 < d.len() {
                    if d[j] == 0xFF && d[j + 1] != 0 && !(0xD0..=0xD7).contains(&d[j + 1]) {
                        break;
                    }
                    j += 1;
                }
                scan_list.push(Scan { comps: sc.clone(), start, end: j });
                i = j;
                continue;
            }
            _ => {}
        }
        i += 2 + ln;
    }
    if scans == 0 || comps.is_empty() {
        return Err("no scan".into());
    }
    Ok(Image { comps, scans: scan_list, dc: dc_raw, ac: ac_raw, dri, width, height, hmax, vmax, qt })
}

// ---------- cost model ----------

#[derive(Clone, Copy)]
struct P(u16);
impl Default for P {
    fn default() -> Self {
        P(2048)
    }
}

/// Adaptive binary probability, charged in bits (-log2 p), LZMA-like update.
struct Coder {
    bits: f64,
    decisions: u64,
    lg: Vec<f64>,
}
impl Coder {
    fn new() -> Coder {
        let lg = (0..4097).map(|i| if i == 0 { 12.0 } else { -((i as f64) / 4096.0).log2() }).collect();
        Coder { bits: 0.0, decisions: 0, lg }
    }
    #[inline]
    fn code(&mut self, p: &mut P, bit: bool, shift: u32) {
        let p0 = p.0 as usize; // probability of 0
        self.decisions += 1;
        if bit {
            self.bits += self.lg[4096 - p0];
            p.0 -= p.0 >> shift;
        } else {
            self.bits += self.lg[p0];
            p.0 += (4096 - p.0) >> shift;
        }
        p.0 = p.0.clamp(31, 4065);
    }
}

/// Contexts are plain hash maps keyed by small tuples: slow, but this is an estimate.
#[derive(Default)]
struct Ctx {
    m: HashMap<u64, P>,
}
impl Ctx {
    #[inline]
    fn get(&mut self, k: u64) -> &mut P {
        self.m.entry(k).or_default()
    }
}

fn bucket(x: u32) -> u64 {
    // 0,1,2,3-4,5-8,9-16,...
    if x < 3 { x as u64 } else { 1 + (32 - (x - 1).leading_zeros()) as u64 }
}

/// Codes a signed value as zero flag, sign, exponent (unary), mantissa; contexts by `k`.
fn code_signed(c: &mut Coder, ctx: &mut Ctx, base: u64, v: i32, sign_ctx: u64) {
    let z = v == 0;
    c.code(ctx.get(base << 8), !z, 5);
    if z {
        return;
    }
    let a = v.unsigned_abs();
    let e = 32 - a.leading_zeros(); // 1..
    for i in 1..e {
        c.code(ctx.get((base << 8) | i as u64), true, 5);
    }
    if e < 16 {
        c.code(ctx.get((base << 8) | e as u64), false, 5);
    }
    // mantissa: top bit below the leading one has a context, the rest are ~1 bit
    for b in (0..e - 1).rev() {
        let bit = (a >> b) & 1 == 1;
        if b == e - 2 {
            c.code(ctx.get((base << 8) | 0x80 | e as u64), bit, 5);
        } else {
            c.bits += 1.0;
            c.decisions += 1;
        }
    }
    c.code(ctx.get((base << 8) | 0xF0 | sign_ctx), v < 0, 5);
}

#[derive(Default)]
struct Totals {
    raw: [f64; 8],
    bits: [f64; 8],
    decisions: [u64; 8],
}

/// Model A: no neighbours (close to JPEG's own arithmetic coding).
/// Model B: neighbour contexts (above/left nonzero counts and magnitudes), DC by LOCO-I prediction.
fn estimate(img: &Image, t: &mut Totals) {
    for (ci, comp) in img.comps.iter().enumerate() {
        let cc = (ci.min(1)) as u64; // luma vs chroma
        let (bw, bh) = (comp.bw, comp.bh);
        // ---- model A ----
        let mut ca = Coder::new();
        let mut xa = Ctx::default();
        let mut prev_dc = 0i32;
        for b in 0..bw * bh {
            let blk = &comp.coef[b];
            let dc = blk[0] as i32;
            code_signed(&mut ca, &mut xa, (cc << 16) | 1, dc - prev_dc, 0);
            prev_dc = dc;
            let last = (1..64).rev().find(|&k| blk[k] != 0).unwrap_or(0);
            for k in 1..64 {
                // end-of-block flag before each coefficient (as T.81)
                let eob = k > last;
                c_eob(&mut ca, &mut xa, cc, k, eob);
                if eob {
                    break;
                }
                code_signed(&mut ca, &mut xa, (cc << 16) | (2 << 8) | k as u64, blk[k] as i32, 0);
            }
        }
        t.bits[0] += ca.bits;
        t.decisions[0] += ca.decisions;

        // ---- model B ----
        let mut cb = Coder::new();
        let mut xb = Ctx::default();
        let nz = |blk: &[i16; 64]| blk[1..].iter().filter(|&&v| v != 0).count() as u32;
        for by in 0..bh {
            for bx in 0..bw {
                let b = by * bw + bx;
                let blk = &comp.coef[b];
                let up = if by > 0 { Some(&comp.coef[b - bw]) } else { None };
                let lf = if bx > 0 { Some(&comp.coef[b - 1]) } else { None };
                let ul = if bx > 0 && by > 0 { Some(&comp.coef[b - bw - 1]) } else { None };
                // nonzero count, context from neighbours' counts
                let nu = up.map(nz);
                let nl = lf.map(nz);
                let nctx = match (nu, nl) {
                    (Some(a), Some(b)) => bucket((a + b + 1) / 2),
                    (Some(a), None) | (None, Some(a)) => bucket(a),
                    _ => 15,
                };
                let n = nz(blk);
                // 6-bit binary tree
                let mut node = 1u64;
                for i in (0..6).rev() {
                    let bit = (n >> i) & 1 == 1;
                    cb.code(xb.get((cc << 40) | (3 << 32) | (nctx << 8) | node), bit, 5);
                    node = (node << 1) | bit as u64;
                }
                // AC in zigzag order until all nonzeros are coded
                let mut left = n;
                for k in 1..64 {
                    if left == 0 {
                        break;
                    }
                    let nb = up.map_or(0, |u| u[k].unsigned_abs() as u32) + lf.map_or(0, |l| l[k].unsigned_abs() as u32);
                    let nbb = bucket(nb).min(7);
                    let lb = bucket(left).min(7);
                    let kb = k as u64;
                    let v = blk[k] as i32;
                    let sctx = match (up.map(|u| u[k].signum()), lf.map(|l| l[k].signum())) {
                        (Some(a), Some(b)) => ((a + b + 2) as u64).min(4),
                        _ => 5,
                    };
                    code_signed(&mut cb, &mut xb, (cc << 40) | (4 << 32) | (kb << 16) | (nbb << 4) | lb, v, sctx);
                    if v != 0 {
                        left -= 1;
                    }
                }
                // DC: LOCO-I median prediction from neighbours' DC
                let dc = blk[0] as i32;
                let (a, bb, c) = (lf.map_or(0, |l| l[0] as i32), up.map_or(0, |u| u[0] as i32), ul.map_or(0, |u| u[0] as i32));
                let pred = match (lf.is_some(), up.is_some()) {
                    (true, true) => {
                        if c >= a.max(bb) { a.min(bb) } else if c <= a.min(bb) { a.max(bb) } else { a + bb - c }
                    }
                    (true, false) => a,
                    (false, true) => bb,
                    _ => 0,
                };
                let act = bucket(((a - c).unsigned_abs() + (bb - c).unsigned_abs()) as u32).min(10);
                code_signed(&mut cb, &mut xb, (cc << 40) | (5 << 32) | (act << 4) | bucket(n).min(7), dc - pred, 0);
            }
        }
        t.bits[1] += cb.bits;
        t.decisions[1] += cb.decisions;

        // ---- model D: multi-symbol adaptive counts (for a fast rANS coder) ----
        let mut md = Multi::default();
        let kgroup = |k: usize| -> u64 { [0u64, 0, 1, 2, 3, 4, 5, 5, 6, 6, 6, 7, 7, 7, 7, 8, 8, 8, 8, 8, 8, 9, 9, 9, 9, 9, 9, 9, 10, 10, 10, 10, 10, 10, 10, 10,
            11, 11, 11, 11, 11, 11, 11, 11, 11, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12][k] };
        for by in 0..bh {
            for bx in 0..bw {
                let b = by * bw + bx;
                let blk = &comp.coef[b];
                let up = if by > 0 { Some(&comp.coef[b - bw]) } else { None };
                let lf = if bx > 0 { Some(&comp.coef[b - 1]) } else { None };
                let ul = if bx > 0 && by > 0 { Some(&comp.coef[b - bw - 1]) } else { None };
                let nu = up.map(nz);
                let nl = lf.map(nz);
                let nctx = match (nu, nl) {
                    (Some(a), Some(b)) => bucket((a + b + 1) / 2),
                    (Some(a), None) | (None, Some(a)) => bucket(a),
                    _ => 15,
                };
                let n = nz(blk);
                md.sym((cc << 40) | (3 << 32) | nctx, n as usize, 64);
                let mut left = n;
                for k in 1..64 {
                    if left == 0 { break; }
                    let nb = up.map_or(0, |u| u[k].unsigned_abs() as u32) + lf.map_or(0, |l| l[k].unsigned_abs() as u32);
                    let v = blk[k] as i32;
                    let e = 32 - v.unsigned_abs().leading_zeros();
                    let ctx = (cc << 40) | (4 << 32) | (kgroup(k) << 16) | (bucket(nb).min(7) << 4) | bucket(left).min(7);
                    md.sym(ctx, e as usize, 16);
                    if e > 0 {
                        md.mantissa_sign(cc, kgroup(k).min(4), e, v.unsigned_abs());
                        left -= 1;
                    }
                }
                let dc = blk[0] as i32;
                let (a, bb, c) = (lf.map_or(0, |l| l[0] as i32), up.map_or(0, |u| u[0] as i32), ul.map_or(0, |u| u[0] as i32));
                let pred = match (lf.is_some(), up.is_some()) {
                    (true, true) => { if c >= a.max(bb) { a.min(bb) } else if c <= a.min(bb) { a.max(bb) } else { a + bb - c } }
                    (true, false) => a,
                    (false, true) => bb,
                    _ => 0,
                };
                let act = bucket(((a - c).unsigned_abs() + (bb - c).unsigned_abs()) as u32).min(10);
                let r = dc - pred;
                let e = 32 - r.unsigned_abs().leading_zeros();
                md.sym((cc << 40) | (5 << 32) | (act << 4) | bucket(n).min(7), e as usize, 16);
                if e > 0 { md.mantissa_sign(cc, 15, e, r.unsigned_abs()); }
            }
        }
        t.bits[2] += md.bits;
        t.decisions[2] += md.symbols;
        t.raw[2] += md.raw;
        // static per-image tables: ideal entropy of each context's counts, plus a table
        // cost of 5 bits per used symbol and 8 bits per used context
        let mut st = md.raw;
        let mut side = 0.0;
        for cnt in md.stat.values() {
            let tot: u32 = cnt.iter().sum();
            if tot == 0 { continue; }
            side += 8.0;
            for &c in cnt {
                if c > 0 { st += c as f64 * (tot as f64 / c as f64).log2(); side += 5.0; }
            }
        }
        t.bits[3] += st + side;
        t.raw[3] += side;
    }
}

/// Multi-symbol adaptive frequencies: what a rANS coder with periodically rebuilt
/// tables approaches. Raw bits (mantissa below the top bit, signs) are counted apart.
#[derive(Default)]
struct Multi {
    stat: HashMap<u64, Vec<u32>>,
    m: HashMap<u64, Vec<u32>>,
    bits: f64,
    raw: f64,
    symbols: u64,
}
impl Multi {
    fn sym(&mut self, ctx: u64, s: usize, n: usize) {
        let cnt = self.stat.entry(ctx).or_insert_with(|| vec![0; n]);
        cnt[s] += 1;
        let f = self.m.entry(ctx).or_insert_with(|| vec![1; n + 1]);
        let tot = f[n];
        self.bits += -((f[s] as f64) / (tot as f64)).log2();
        self.symbols += 1;
        f[s] += 24;
        f[n] += 24;
        if f[n] > 1 << 13 {
            let mut t = 0;
            for x in f[..n].iter_mut() { *x = (*x + 1) / 2; t += *x; }
            f[n] = t;
        }
    }
    fn mantissa_sign(&mut self, cc: u64, kg: u64, e: u32, a: u32) {
        if e >= 2 {
            let top = ((a >> (e - 2)) & 1) as usize;
            self.sym((cc << 40) | (6 << 32) | (kg << 8) | e as u64, top, 2);
            self.bits += (e - 2) as f64;
            self.raw += (e - 2) as f64;
        }
        self.bits += 1.0;
        self.raw += 1.0;
    }
}

fn c_eob(c: &mut Coder, x: &mut Ctx, cc: u64, k: usize, eob: bool) {
    c.code(x.get((cc << 16) | (9 << 8) | k as u64), eob, 5);
}

// ---------- re-encoding (exactness check) ----------

fn codes(counts: &[u8; 16], vals: &[u8]) -> [(u32, u32); 256] {
    let mut t = [(0u32, 0u32); 256];
    let mut code = 0u32;
    let mut k = 0;
    for l in 1..=16u32 {
        for _ in 0..counts[l as usize - 1] {
            t[vals[k] as usize] = (code, l);
            code += 1;
            k += 1;
        }
        code <<= 1;
    }
    t
}

struct BW {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}
impl BW {
    /// Appends `l` bits of `v`; whole bytes leave 32 bits at a time, 0xFF bytes stuffed.
    #[inline(always)]
    fn put(&mut self, v: u32, l: u32) {
        // callers pass exactly `l` bits
        self.acc = (self.acc << l) | v as u64;
        self.n += l;
        if self.n >= 32 {
            self.n -= 32;
            let w = (self.acc >> self.n) as u32;
            let bytes = w.to_be_bytes();
            // any 0xFF byte? (haszero trick on the inverted word)
            let inv = !w;
            if self.out.capacity() - self.out.len() < 8 {
                self.out.reserve(1 << 16);
            }
            if (inv.wrapping_sub(0x0101_0101) & !inv & 0x8080_8080) == 0 {
                unsafe {
                    let l = self.out.len();
                    std::ptr::write_unaligned(self.out.as_mut_ptr().add(l) as *mut u32, w.to_be());
                    self.out.set_len(l + 4);
                }
            } else {
                for b in bytes {
                    self.out.push(b);
                    if b == 0xFF { self.out.push(0); }
                }
            }
        }
    }
    fn pad(&mut self) {
        // flush whole bytes, then pad the last partial byte with ones
        while self.n >= 8 {
            self.n -= 8;
            let b = (self.acc >> self.n) as u8;
            self.out.push(b);
            if b == 0xFF { self.out.push(0); }
        }
        if self.n > 0 {
            let k = 8 - self.n;
            let b = (((self.acc << k) | ((1 << k) - 1)) & 0xFF) as u8;
            self.out.push(b);
            if b == 0xFF { self.out.push(0); }
            self.n = 0;
        }
        self.acc = 0;
    }
}

#[inline(always)]
fn enc_block(w: &mut BW, blk: &[i16; 64], pred: &mut i32, dct: &[(u32, u32); 256], act: &[(u32, u32); 256]) {
    #[inline(always)]
    fn mag(v: i32) -> (u32, u32) {
        let a = v.unsigned_abs();
        let s = 32 - a.leading_zeros();
        // negative values send v - 1 in s bits
        (s, (v + (v >> 31)) as u32 & ((1u32 << s) - 1))
    }
    let diff = blk[0] as i32 - *pred;
    *pred = blk[0] as i32;
    let (s, b) = mag(diff);
    let (c, l) = dct[s as usize];
    w.put((c << s) | b, l + s);
    // nonzero positions as 4-bit lanes, 16 coefficients per mask (NEON)
    let mut masks = [0u64; 4];
    unsafe {
        use std::arch::aarch64::*;
        let p = blk.as_ptr();
        for q in 0..4 {
            let a = vtstq_s16(vld1q_s16(p.add(16 * q)), vld1q_s16(p.add(16 * q)));
            let b = vtstq_s16(vld1q_s16(p.add(16 * q + 8)), vld1q_s16(p.add(16 * q + 8)));
            let bytes = vcombine_u8(vmovn_u16(a), vmovn_u16(b));
            masks[q] = vget_lane_u64::<0>(vreinterpret_u64_u8(vshrn_n_u16::<4>(vreinterpretq_u16_u8(bytes))));
        }
    }
    masks[0] &= !0xF; // the DC lane
    let mut last = 0u32;
    for q in 0..4 {
        let mut m = masks[q];
        while m != 0 {
            let tz = m.trailing_zeros();
            m &= !(0xFu64 << (tz & !3));
            let k = 16 * q as u32 + tz / 4;
            let mut run = k - last - 1;
            while run > 15 {
                let (c, l) = act[0xF0];
                w.put(c, l);
                run -= 16;
            }
            let (s, b) = mag(unsafe { *blk.get_unchecked(k as usize) } as i32);
            let (c, l) = act[((run << 4) | s) as usize];
            w.put((c << s) | b, l + s);
            last = k;
        }
    }
    if last < 63 {
        let (c, l) = act[0];
        w.put(c, l);
    }
}

fn reencode(img: &Image, sc: &Scan) -> Vec<u8> {
    let mut w = BW { out: Vec::with_capacity(sc.end - sc.start + 4096), acc: 0, n: 0 };
    let mut pred = vec![0i32; img.comps.len()];
    let tabs: Vec<_> = sc.comps.iter().map(|&(_, td, ta)| (codes(&img.dc[&td].0, &img.dc[&td].1), codes(&img.ac[&ta].0, &img.ac[&ta].1))).collect();
    let mut mcu = 0usize;
    let mut rst = 0u8;
    let mut restart = |w: &mut BW, pred: &mut Vec<i32>, mcu: usize| {
        if img.dri > 0 && mcu > 0 && mcu % img.dri == 0 {
            w.pad();
            w.out.push(0xFF);
            w.out.push(0xD0 + rst);
            rst = (rst + 1) & 7;
            pred.iter_mut().for_each(|p| *p = 0);
        }
    };
    if sc.comps.len() == 1 {
        let (ci, _, _) = sc.comps[0];
        let c = &img.comps[ci];
        let cw = (img.width * c.h + 8 * img.hmax - 1) / (8 * img.hmax);
        let ch = (img.height * c.v + 8 * img.vmax - 1) / (8 * img.vmax);
        for by in 0..ch {
            for bx in 0..cw {
                restart(&mut w, &mut pred, mcu);
                enc_block(&mut w, &c.coef[by * c.bw + bx], &mut pred[ci], &tabs[0].0, &tabs[0].1);
                mcu += 1;
            }
        }
    } else {
        let mcux = (img.width + 8 * img.hmax - 1) / (8 * img.hmax);
        let mcuy = (img.height + 8 * img.vmax - 1) / (8 * img.vmax);
        for my in 0..mcuy {
            for mx in 0..mcux {
                restart(&mut w, &mut pred, mcu);
                for (si, &(ci, _, _)) in sc.comps.iter().enumerate() {
                    let c = &img.comps[ci];
                    for y in 0..c.v {
                        for x in 0..c.h {
                            enc_block(&mut w, &c.coef[(my * c.v + y) * c.bw + mx * c.h + x], &mut pred[ci], &tabs[si].0, &tabs[si].1);
                        }
                    }
                }
                mcu += 1;
            }
        }
    }
    w.pad();
    w.out
}

fn repeats(files: &[std::path::PathBuf]) {
    // blocks whose 63 AC values equal an earlier block of the same component
    let (mut blocks, mut nzb, mut rep, mut nz_all, mut nz_rep, mut rep_dc) = (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    for f in files {
        let d = fs::read(f).unwrap();
        let Ok(img) = read_jpeg(&d) else { continue };
        for c in &img.comps {
            let mut seen: HashMap<[i16; 63], i16> = HashMap::new();
            for b in &c.coef {
                blocks += 1;
                let nz = b[1..].iter().filter(|&&v| v != 0).count() as u64;
                nz_all += nz;
                if nz == 0 { continue; }
                nzb += 1;
                let mut key = [0i16; 63];
                key.copy_from_slice(&b[1..]);
                match seen.get(&key) {
                    Some(&dc) => { rep += 1; nz_rep += nz; if dc == b[0] { rep_dc += 1; } }
                    None => { seen.insert(key, b[0]); }
                }
            }
        }
    }
    // dense repeats and how far back they are (in distinct-block order)
    let (mut dense_rep, mut dense_nz, mut near) = (0u64, 0u64, [0u64; 4]);
    for f in files {
        let d = fs::read(f).unwrap();
        let Ok(img) = read_jpeg(&d) else { continue };
        for c in &img.comps {
            let mut last: HashMap<[i16; 63], usize> = HashMap::new();
            let mut order = 0usize;
            for (bi, b) in c.coef.iter().enumerate() {
                let nz = b[1..].iter().filter(|&&v| v != 0).count() as u64;
                if nz < 6 { continue; }
                let mut key = [0i16; 63];
                key.copy_from_slice(&b[1..]);
                if let Some(&o) = last.get(&key) {
                    dense_rep += 1;
                    dense_nz += nz;
                    let dist = order - o;
                    let same_nb = (bi >= 1 && c.coef[bi - 1][1..] == b[1..]) || (bi >= c.bw && c.coef[bi - c.bw][1..] == b[1..]);
                    if same_nb { near[0] += 1 } else if dist <= 16 { near[1] += 1 } else if dist <= 256 { near[2] += 1 } else { near[3] += 1 }
                }
                last.insert(key, order);
                order += 1;
            }
        }
    }
    println!("dense (6+ nonzeros) repeats: {dense_rep}, their nonzeros {:.1}% of all; same as left/up {}, within 16 {}, within 256 {}, further {}",
        100.0 * dense_nz as f64 / nz_all as f64, near[0], near[1], near[2], near[3]);
    println!("blocks {blocks}, with AC {nzb}; AC repeats an earlier block: {rep} ({:.1}% of AC blocks), DC equal too {rep_dc}; their nonzeros {:.1}% of all", 100.0 * rep as f64 / nzb as f64, 100.0 * nz_rep as f64 / nz_all as f64);
}

fn run_lepton(files: &[std::path::PathBuf]) {
    use std::time::Instant;
    let (mut n, mut inb, mut lep, mut te, mut td) = (0, 0u64, 0u64, 0f64, 0f64);
    let (mut hdr_files, mut hdr_raw) = (0u64, Vec::new());
    for f in files {
        let d = fs::read(f).unwrap();
        let Ok(img) = read_jpeg(&d) else { continue };
        n += 1;
        inb += d.len() as u64;
        let mut feat = lepton_jpeg::EnabledFeatures::compat_lepton_vector_write();
        feat.max_processor_threads = 1;
        let pool = lepton_jpeg::SingleThreadPool::default();
        let t = Instant::now();
        let mut out = Vec::new();
        lepton_jpeg::encode_lepton(&mut std::io::Cursor::new(&d), &mut std::io::Cursor::new(&mut out), &feat, &pool).unwrap();
        te += t.elapsed().as_secs_f64();
        lep += out.len() as u64;
        let mut rf = lepton_jpeg::EnabledFeatures::compat_lepton_vector_read();
        rf.max_processor_threads = 1;
        let t = Instant::now();
        let mut back = Vec::new();
        lepton_jpeg::decode_lepton(&mut std::io::Cursor::new(&out), &mut back, &rf, &pool).unwrap();
        td += t.elapsed().as_secs_f64();
        assert!(back == d);
        // the header bytes (everything but scan data), for our side of the comparison
        let mut h = Vec::new();
        let mut at = 0;
        for sc in &img.scans { h.extend_from_slice(&d[at..sc.start]); at = sc.end; }
        h.extend_from_slice(&d[at..]);
        hdr_files += zstd::bulk::compress(&h, 19).unwrap().len() as u64;
        hdr_raw.extend_from_slice(&h);
    }
    let solid = zstd::bulk::compress(&hdr_raw, 19).unwrap().len();
    println!("lepton: {n} baseline files, {:.2} MB -> {:.2} MB; one core encode {:.2} s ({:.0} MB/s), decode {:.2} s ({:.0} MB/s)",
        inb as f64 / 1e6, lep as f64 / 1e6, te, inb as f64 / 1e6 / te, td, inb as f64 / 1e6 / td);
    println!("  headers: {:.2} MB raw, zstd-19 per file {:.2} MB, solid {:.2} MB", hdr_raw.len() as f64 / 1e6, hdr_files as f64 / 1e6, solid as f64 / 1e6);
}

fn run_lite(files: &[std::path::PathBuf], threads: usize) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;
    let mut jobs = Vec::new();
    for f in files {
        let d = fs::read(f).unwrap();
        let Ok(img) = read_jpeg(&d) else { continue };
        jobs.push((d, img));
    }
    jobs.sort_by_key(|j| std::cmp::Reverse(j.0.len()));
    let total: u64 = jobs.iter().map(|j| j.0.len() as u64).sum();
    let mut packed: Vec<Option<Vec<u8>>> = vec![None; jobs.len()];
    for &t in &[1usize, threads] {
        // pack
        let next = AtomicUsize::new(0);
        let start = Instant::now();
        let results: Vec<std::sync::Mutex<Option<Vec<u8>>>> = (0..jobs.len()).map(|_| std::sync::Mutex::new(None)).collect();
        std::thread::scope(|sc| {
            for _ in 0..t {
                sc.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= jobs.len() { break; }
                    let (d, img) = &jobs[i];
                    *results[i].lock().unwrap() = lite::pack_tight(d, img);
                });
            }
        });
        let tp = start.elapsed().as_secs_f64();
        packed = results.into_iter().map(|m| m.into_inner().unwrap()).collect();
        // unpack (whole files: headers copied, scans re-written)
        let next = AtomicUsize::new(0);
        let bad = AtomicUsize::new(0);
        let start = Instant::now();
        std::thread::scope(|sc| {
            for _ in 0..t {
                sc.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= jobs.len() { break; }
                    let (d, img) = &jobs[i];
                    let Some(p) = &packed[i] else { bad.fetch_add(1, Ordering::Relaxed); continue };
                    let scans = lite::unpack_tight(p, img);
                    let mut out = Vec::with_capacity(d.len());
                    let mut at = 0;
                    for (s, sd) in img.scans.iter().zip(&scans) {
                        out.extend_from_slice(&d[at..s.start]);
                        out.extend_from_slice(sd);
                        at = s.end;
                    }
                    out.extend_from_slice(&d[at..]);
                    if out != *d { bad.fetch_add(1, Ordering::Relaxed); }
                });
            }
        });
        let tu = start.elapsed().as_secs_f64();
        println!("lite on {t} thread(s): pack {:.3} s ({:.0} MB/s), unpack {:.3} s ({:.0} MB/s), not exact {}",
            tp, total as f64 / 1e6 / tp, tu, total as f64 / 1e6 / tu, bad.load(Ordering::Relaxed));
    }
    if std::env::var_os("LOOP_UNPACK").is_some() {
        let t = Instant::now();
        for _ in 0..8 {
            for (i, (_, img)) in jobs.iter().enumerate() {
                std::hint::black_box(lite::unpack(packed[i].as_ref().unwrap(), img));
            }
        }
        let el = t.elapsed().as_secs_f64();
        let n = lite::SYMS.load(std::sync::atomic::Ordering::Relaxed);
        println!("unpack x8 one thread: {:.2} s; AC symbols counted {} ({:.2} ns each)", el, n, el * 1e9 / n as f64 * 9.0 / 8.0);
    }
    println!("pack phases (all runs, all threads): parse {:.2} s, tables {:.2} s, tables + write {:.2} s",
        lite::T_PARSE.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e9, lite::T_TAB.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e9, lite::T_WRITE.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e9);
    for (d, img) in &jobs { std::hint::black_box(lite::read_only(d, img)); }
    println!("read only, one thread: {:.2} s", lite::T_READ.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e9);
    let size: u64 = packed.iter().map(|p| p.as_ref().map_or(0, |v| v.len() as u64)).sum();
    let scans: u64 = jobs.iter().map(|(_, img)| img.scans.iter().map(|s| (s.end - s.start) as u64).sum::<u64>()).sum();
    let mut hdr = Vec::new();
    for (d, img) in &jobs {
        let mut at = 0;
        for s in &img.scans { hdr.extend_from_slice(&d[at..s.start]); at = s.end; }
        hdr.extend_from_slice(&d[at..]);
    }
    let h = zstd::bulk::compress(&hdr, 19).unwrap().len() as u64;
    println!("lite: {} files {:.2} MB, scans {:.2} MB -> {:.2} MB ({:.1}% smaller); with headers (solid) {:.2} MB",
        jobs.len(), total as f64 / 1e6, scans as f64 / 1e6, size as f64 / 1e6, 100.0 * (1.0 - size as f64 / scans as f64), (size + h) as f64 / 1e6);
}

fn run_parallel(files: &[std::path::PathBuf], threads: usize) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;
    // prepare: every image encoded (one thread, not timed as a parallel figure)
    let mut jobs = Vec::new();
    for f in files {
        let d = fs::read(f).unwrap();
        let Ok(img) = read_jpeg(&d) else { continue };
        let mut coefs: Vec<Vec<[i16; 64]>> = img.comps.iter().map(|c| c.coef.clone()).collect();
        let bytes = {
            let mut planes: Vec<fast::Plane> = coefs.iter_mut().zip(&img.comps).map(|(cf, c)| fast::Plane { coef: cf, bw: c.bw, bh: c.bh }).collect();
            fast::encode(&mut planes)
        };
        jobs.push((d, img, bytes));
    }
    // biggest first, as a scheduler would
    jobs.sort_by_key(|j| std::cmp::Reverse(j.0.len()));
    let total: u64 = jobs.iter().map(|j| j.0.len() as u64).sum();
    for &t in &[1usize, threads] {
        let next = AtomicUsize::new(0);
        let bad = AtomicUsize::new(0);
        let start = Instant::now();
        std::thread::scope(|sc| {
            for _ in 0..t {
                sc.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= jobs.len() { break; }
                    let (d, img, bytes) = &jobs[i];
                    let mut dec: Vec<Vec<[i16; 64]>> = img.comps.iter().map(|c| vec![[0i16; 64]; c.coef.len()]).collect();
                    {
                        let mut planes: Vec<fast::Plane> = dec.iter_mut().zip(&img.comps).map(|(cf, c)| fast::Plane { coef: cf, bw: c.bw, bh: c.bh }).collect();
                        fast::decode(bytes, &mut planes);
                    }
                    let img2 = Image { comps: img.comps.iter().zip(dec).map(|(c, cf)| Comp { id: c.id, tq: c.tq, h: c.h, v: c.v, bw: c.bw, bh: c.bh, coef: cf }).collect(),
                        scans: vec![], dc: img.dc.clone(), ac: img.ac.clone(), dri: img.dri, width: img.width, height: img.height, hmax: img.hmax, vmax: img.vmax, qt: img.qt.clone() };
                    // the whole file: headers as they were, scans re-written
                    let mut out = Vec::with_capacity(d.len());
                    let mut at = 0;
                    for s in &img.scans {
                        out.extend_from_slice(&d[at..s.start]);
                        out.extend_from_slice(&reencode(&img2, s));
                        at = s.end;
                    }
                    out.extend_from_slice(&d[at..]);
                    if out != *d { bad.fetch_add(1, Ordering::Relaxed); }
                });
            }
        });
        let el = start.elapsed().as_secs_f64();
        println!("unpack {} files on {t} thread(s): {:.3} s ({:.0} MB/s), mismatches {}", jobs.len(), el, total as f64 / 1e6 / el, bad.load(Ordering::Relaxed));
    }
}

fn run_fast(files: &[std::path::PathBuf]) {
    use std::time::Instant;
    let (mut inb, mut outb, mut side, mut raw, mut ra) = (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut hdr_raw: Vec<u8> = Vec::new();
    let (mut te, mut td, mut th, mut ok, mut n) = (0f64, 0f64, 0f64, 0, 0);
    for f in files {
        let d = fs::read(f).unwrap();
        let Ok(img) = read_jpeg(&d) else { continue };
        n += 1;
        inb += img.scans.iter().map(|s| (s.end - s.start) as u64).sum::<u64>();
        {
            let mut at = 0;
            for sc in &img.scans { hdr_raw.extend_from_slice(&d[at..sc.start]); at = sc.end; }
            hdr_raw.extend_from_slice(&d[at..]);
        }
        let mut coefs: Vec<Vec<[i16; 64]>> = img.comps.iter().map(|c| c.coef.clone()).collect();
        let t = Instant::now();
        let bytes = {
            let mut planes: Vec<fast::Plane> = coefs.iter_mut().zip(&img.comps).map(|(cf, c)| fast::Plane { coef: cf, bw: c.bw, bh: c.bh }).collect();
            fast::encode(&mut planes)
        };
        te += t.elapsed().as_secs_f64();
        outb += bytes.len() as u64;
        let (a, b, c) = fast::sizes(&bytes);
        side += a as u64; raw += b as u64; ra += c as u64;
        let mut dec: Vec<Vec<[i16; 64]>> = img.comps.iter().map(|c| vec![[0i16; 64]; c.coef.len()]).collect();
        let t = Instant::now();
        let reps = if std::env::var_os("NO_DECODE").is_some() { 0 } else if std::env::var_os("DECODE3").is_some() { 3 } else { 1 };
        for _ in 1..reps {
            let mut planes: Vec<fast::Plane> = dec.iter_mut().zip(&img.comps).map(|(cf, c)| fast::Plane { coef: cf, bw: c.bw, bh: c.bh }).collect();
            fast::decode(&bytes, &mut planes);
        }
        if reps > 0 {
            let mut planes: Vec<fast::Plane> = dec.iter_mut().zip(&img.comps).map(|(cf, c)| fast::Plane { coef: cf, bw: c.bw, bh: c.bh }).collect();
            fast::decode(&bytes, &mut planes);
        }
        td += t.elapsed().as_secs_f64();
        if std::env::var_os("REPLAY").is_some() {
            let mut cf2: Vec<Vec<[i16; 64]>> = img.comps.iter().map(|c| c.coef.clone()).collect();
            let mut planes: Vec<fast::Plane> = cf2.iter_mut().zip(&img.comps).map(|(cf, c)| fast::Plane { coef: cf, bw: c.bw, bh: c.bh }).collect();
            let ev = fast::events_of(&mut planes);
            std::hint::black_box(fast::replay(&bytes, &ev));
        }
        let t = Instant::now();
        let img2 = Image { comps: img.comps.iter().zip(dec).map(|(c, cf)| Comp { id: c.id, tq: c.tq, h: c.h, v: c.v, bw: c.bw, bh: c.bh, coef: cf }).collect(),
            scans: vec![], dc: img.dc.clone(), ac: img.ac.clone(), dri: img.dri, width: img.width, height: img.height, hmax: img.hmax, vmax: img.vmax, qt: img.qt.clone() };
        let mut same = true;
        for sc in &img.scans {
            let r = reencode(&img2, sc);
            same &= r[..] == d[sc.start..sc.end];
        }
        th += t.elapsed().as_secs_f64();
        if same { ok += 1; } else { eprintln!("{}: round trip differs", f.display()); }
    }
    let solid = zstd::bulk::compress(&hdr_raw, 19).unwrap().len() as u64;
    println!("fast total with headers (solid zstd-19 {:.2} MB): {:.2} MB", solid as f64 / 1e6, (outb + solid) as f64 / 1e6);
    println!("fast: {n} images, scans {:.2} MB -> {:.2} MB ({:.1}% smaller): tables {:.2} MB, raw bits {:.2} MB, rANS {:.2} MB; identical {ok}/{n}",
        inb as f64 / 1e6, outb as f64 / 1e6, 100.0 * (1.0 - outb as f64 / inb as f64), side as f64 / 1e6, raw as f64 / 1e6, ra as f64 / 1e6);
    println!("  one core: encode {:.2} s ({:.0} MB/s), decode {:.2} s ({:.0} MB/s), Huffman re-encode {:.2} s ({:.0} MB/s)",
        te, inb as f64 / 1e6 / te, td, inb as f64 / 1e6 / td, th, inb as f64 / 1e6 / th);
    println!("  AC symbols {}, zeros among them {:.1}%", fast::N_AC.load(std::sync::atomic::Ordering::Relaxed),
        100.0 * fast::T_ZERO.load(std::sync::atomic::Ordering::Relaxed) as f64 / fast::N_AC.load(std::sync::atomic::Ordering::Relaxed) as f64);
    println!("  replay (symbols only, contexts given): {:.2} s", fast::REPLAY_NS.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e9);
    println!("  decode: tables {:.2} s, symbols {} ({:.2} per input byte)", fast::T_TABLES.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e9,
        fast::T_SYMS.load(std::sync::atomic::Ordering::Relaxed), fast::T_SYMS.load(std::sync::atomic::Ordering::Relaxed) as f64 / inb as f64);
}

fn run_codec(files: &[std::path::PathBuf]) {
    use std::time::Instant;
    let (mut inb, mut outb, mut te, mut td, mut th, mut ok, mut n) = (0u64, 0u64, 0f64, 0f64, 0f64, 0, 0);
    for f in files {
        let d = fs::read(f).unwrap();
        let Ok(img) = read_jpeg(&d) else { continue };
        n += 1;
        inb += img.scans.iter().map(|s| (s.end - s.start) as u64).sum::<u64>();
        // encode
        let t = Instant::now();
        let mut md = codec::Model::new();
        let mut e = codec::Enc::new();
        for (ci, c) in img.comps.iter().enumerate() {
            let mut coef = c.coef.clone();
            codec::code_component(&mut e, &mut md, ci.min(1), &mut coef, c.bw, c.bh);
        }
        let bytes = e.finish();
        te += t.elapsed().as_secs_f64();
        outb += bytes.len() as u64;
        // decode
        let t = Instant::now();
        let mut md = codec::Model::new();
        let mut dd = codec::Dec::new(&bytes);
        let mut img2 = Image { comps: vec![], scans: vec![], dc: img.dc.clone(), ac: img.ac.clone(), dri: img.dri, width: img.width, height: img.height, hmax: img.hmax, vmax: img.vmax, qt: img.qt.clone() };
        for (ci, c) in img.comps.iter().enumerate() {
            let mut coef = vec![[0i16; 64]; c.coef.len()];
            codec::code_component(&mut dd, &mut md, ci.min(1), &mut coef, c.bw, c.bh);
            img2.comps.push(Comp { id: c.id, tq: c.tq, h: c.h, v: c.v, bw: c.bw, bh: c.bh, coef });
        }
        td += t.elapsed().as_secs_f64();
        let t = Instant::now();
        let mut same = true;
        for sc in &img.scans {
            let r = reencode(&img2, sc);
            same &= r[..] == d[sc.start..sc.end];
        }
        th += t.elapsed().as_secs_f64();
        if same { ok += 1; } else { eprintln!("{}: round trip differs", f.display()); }
    }
    println!("codec: {n} images, scans {:.2} MB -> {:.2} MB ({:.1}% smaller); round trips identical {ok}/{n}", inb as f64 / 1e6, outb as f64 / 1e6, 100.0 * (1.0 - outb as f64 / inb as f64));
    println!("  one core: model encode {:.2} s ({:.0} MB/s), model decode {:.2} s ({:.0} MB/s), Huffman re-encode {:.2} s ({:.0} MB/s)",
        te, inb as f64 / 1e6 / te, td, inb as f64 / 1e6 / td, th, inb as f64 / 1e6 / th);
}

fn main() {
    let dir = std::env::args().nth(1).expect("dir");
    let mut files = vec![];
    fn uniq(files: Vec<std::path::PathBuf>) -> Vec<std::path::PathBuf> {
        use std::hash::{Hash, Hasher};
        let mut seen = std::collections::HashSet::new();
        files.into_iter().filter(|f| {
            let d = fs::read(f).unwrap();
            let mut h = std::collections::hash_map::DefaultHasher::new();
            d.hash(&mut h);
            seen.insert((h.finish(), d.len()))
        }).collect()
    }
    fn walk(p: &Path, out: &mut Vec<std::path::PathBuf>) {
        for e in fs::read_dir(p).unwrap() {
            let e = e.unwrap().path();
            if e.is_dir() { walk(&e, out) } else { out.push(e) }
        }
    }
    walk(Path::new(&dir), &mut files);
    files.sort();
    if std::env::var_os("UNIQUE").is_some() {
        files = uniq(files);
    }
    if std::env::args().any(|a| a == "--lite") {
        run_lite(&files, 10);
        return;
    }
    if std::env::args().any(|a| a == "--par") {
        run_parallel(&files, 10);
        return;
    }
    if std::env::args().any(|a| a == "--lepton") {
        run_lepton(&files);
        return;
    }
    if std::env::args().any(|a| a == "--fast") {
        run_fast(&files);
        return;
    }
    if std::env::args().any(|a| a == "--repeats") {
        repeats(&files);
        return;
    }
    if std::env::args().any(|a| a == "--codec") {
        run_codec(&files);
        return;
    }
    let (mut total, mut scan_ok, mut other) = (0u64, 0u64, 0u64);
    let mut t = Totals::default();
    let mut failed = 0;
    let mut exact = 0;
    let t0 = std::time::Instant::now();
    let mut dec_time = 0.0;
    for f in &files {
        let d = fs::read(f).unwrap();
        total += d.len() as u64;
        let s = std::time::Instant::now();
        match read_jpeg(&d) {
            Ok(img) => {
                dec_time += s.elapsed().as_secs_f64();
                let mut same = true;
                for sc in &img.scans {
                    let r = reencode(&img, sc);
                    if r[..] != d[sc.start..sc.end] {
                        same = false;
                        let at = r.iter().zip(&d[sc.start..sc.end]).position(|(a, b)| a != b);
                        eprintln!("{}: re-encode differs (ours {} vs {} bytes, first diff {:?})", f.display(), r.len(), sc.end - sc.start, at);
                    }
                }
                if same { exact += 1; }
                scan_ok += d.len() as u64;
                if std::env::var("ONLY_F").is_err() { estimate(&img, &mut t); }
                if std::env::var("ONLY_G").is_err() {
                    let (b, dcs) = modelf::estimate_f(&img);
                    t.bits[4] += b;
                    t.decisions[4] += dcs;
                }
                if std::env::var("ONLY_M").is_err() {
                    let (b, dcs) = modelg::estimate_g(&img);
                    t.bits[5] += b;
                    t.decisions[5] += dcs;
                }
                if std::env::var("WITH_R").is_ok() {
                    let (b, side) = modelr::estimate_r(&img);
                    t.bits[3] += b;
                    t.raw[3] += side;
                }
                if std::env::var("WITH_H").is_ok() {
                    t.bits[7] += modelh::estimate_h(&img);
                }
                if std::env::var("WITH_M").is_ok() {
                    let (b, dcs) = modelm::estimate_m(&img);
                    t.bits[6] += b;
                    t.decisions[6] += dcs;
                }
            }
            Err(e) => {
                failed += 1;
                other += d.len() as u64;
                eprintln!("{}: {}", f.display(), e);
            }
        }
    }
    println!("{} files, {:.2} MB; baseline decoded {:.2} MB ({} not), Huffman decode {:.2} s, total {:.1} s",
        files.len(), total as f64 / 1e6, scan_ok as f64 / 1e6, failed, dec_time, t0.elapsed().as_secs_f64());
    for (i, name) in ["A: no neighbours (T.81-like)", "B: neighbour contexts + LOCO-I DC", "D: B's contexts, multi-symbol (rANS-able)", "E: D with static per-image tables (raw = table bits)", "F: B + edge-continuity predictions", "G: fine expected-magnitude contexts", "M: three models mixed + APM", "H: light Fast mode, static Huffman per small context"].iter().enumerate() {
        let mb = t.bits[i] / 8e6;
        println!("  {name}: {:.2} MB of coefficients ({:.1}% of the decoded files), {:.1} decisions per input byte, {:.2} raw bits per input byte",
            mb, 100.0 * (1.0 - mb / (scan_ok as f64 / 1e6)), t.decisions[i] as f64 / scan_ok as f64, t.raw[i] / scan_ok as f64);
    }
    {
        let g = modelf::CAT.lock().unwrap();
        println!("  F by part (MB): count {:.2}, interior {:.2}, top row {:.2}, left column {:.2}, DC {:.2}", g[0] / 8e6, g[1] / 8e6, g[2] / 8e6, g[3] / 8e6, g[4] / 8e6);
    }
    {
        let g = modelg::IDEAL.lock().unwrap();
        println!("  G as static per-image tables: ideal {:.2} MB, + every used histogram {:.2} MB", g[0] / 8e6, (g[0] + g[1]) / 8e6);
    }
    {
        // trained tables, two folds: train on the other half's counts, charge this half
        let imgs = modelg::PER_IMAGE.lock().unwrap();
        let mut tot_global = 0.0;
        let mut tot_hybrid = 0.0;
        for fold in 0..2 {
            let mut train: HashMap<u64, Vec<f64>> = HashMap::new();
            for (i, (st, _)) in imgs.iter().enumerate() {
                if i % 2 == fold { continue; }
                for (k, v) in st {
                    let t = train.entry(*k).or_insert_with(|| vec![0.0; v.len()]);
                    for (a, b) in t.iter_mut().zip(v) { *a += *b as f64; }
                }
            }
            for (i, (st, raw)) in imgs.iter().enumerate() {
                if i % 2 != fold { continue; }
                tot_global += raw;
                tot_hybrid += raw;
                for (k, v) in st {
                    let n = v.len() as f64;
                    let tot: u32 = v.iter().sum();
                    // global table, smoothed
                    let (tr, trt) = match train.get(k) {
                        Some(t) => (t.clone(), t.iter().sum::<f64>()),
                        None => (vec![0.0; v.len()], 0.0),
                    };
                    let mut g = 0.0;
                    for (s, &c) in v.iter().enumerate() {
                        if c > 0 { g += c as f64 * -((tr[s] + 0.02) / (trt + 0.02 * n)).log2(); }
                    }
                    // own histogram: entropy + 6 bits per used symbol + 4 per context
                    let mut own = 4.0;
                    for &c in v.iter() { if c > 0 { own += c as f64 * (tot as f64 / c as f64).log2() + 6.0; } }
                    tot_global += g + 1.0 / 8.0; // a flag per used context, roughly
                    tot_hybrid += g.min(own) + 1.0;
                }
            }
        }
        println!("  G with trained tables (2-fold): global only {:.2} MB, global or own per context {:.2} MB", tot_global / 8e6, tot_hybrid / 8e6);
    }
    println!("  re-encoded byte for byte: {exact} of {}", files.len() - failed);
    let _ = other;
}
