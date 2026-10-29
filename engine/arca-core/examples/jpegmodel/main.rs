//! Size estimate for a light JPEG coefficient coder (research prototype, 2026-10-29).
//! Decodes baseline JPEGs to quantised coefficients, then charges every binary
//! decision of a candidate model to an adaptive probability (LZMA-style, 12-bit),
//! so the totals are what a real adaptive binary coder would write.
//!
//!   cargo run --release --example jpegmodel -- <dir> [--codec]

mod codec;
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
                    comps.push(Comp { id: seg[6 + 3 * c], h: (hv >> 4) as usize, v: (hv & 15) as usize, bw: 0, bh: 0, coef: vec![] });
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
    Ok(Image { comps, scans: scan_list, dc: dc_raw, ac: ac_raw, dri, width, height, hmax, vmax })
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
    raw: [f64; 4],
    bits: [f64; 4],
    decisions: [u64; 4],
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
    fn put(&mut self, v: u32, l: u32) {
        if l == 0 { return; }
        self.acc = (self.acc << l) | (v as u64 & ((1u64 << l) - 1));
        self.n += l;
        while self.n >= 8 {
            let b = (self.acc >> (self.n - 8)) as u8;
            self.out.push(b);
            if b == 0xFF { self.out.push(0); }
            self.n -= 8;
        }
    }
    fn pad(&mut self) {
        if self.n > 0 { let k = 8 - self.n; self.put((1 << k) - 1, k); }
    }
}

fn enc_block(w: &mut BW, blk: &[i16; 64], pred: &mut i32, dct: &[(u32, u32); 256], act: &[(u32, u32); 256]) {
    let mag = |v: i32| -> (u32, u32) {
        let a = v.unsigned_abs();
        let s = 32 - a.leading_zeros();
        let bits = if v < 0 { (v - 1) as u32 } else { v as u32 };
        (s, bits & ((1u32 << s) - 1).max(0))
    };
    let diff = blk[0] as i32 - *pred;
    *pred = blk[0] as i32;
    let (s, b) = mag(diff);
    let (c, l) = dct[s as usize];
    w.put(c, l);
    w.put(b, s);
    let mut run = 0;
    for k in 1..64 {
        let v = blk[k] as i32;
        if v == 0 { run += 1; continue; }
        while run > 15 { let (c, l) = act[0xF0]; w.put(c, l); run -= 16; }
        let (s, b) = mag(v);
        let (c, l) = act[((run << 4) | s) as usize];
        w.put(c, l);
        w.put(b, s);
        run = 0;
    }
    if run > 0 { let (c, l) = act[0]; w.put(c, l); }
}

fn reencode(img: &Image, sc: &Scan) -> Vec<u8> {
    let mut w = BW { out: vec![], acc: 0, n: 0 };
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
        let mut img2 = Image { comps: vec![], scans: vec![], dc: img.dc.clone(), ac: img.ac.clone(), dri: img.dri, width: img.width, height: img.height, hmax: img.hmax, vmax: img.vmax };
        for (ci, c) in img.comps.iter().enumerate() {
            let mut coef = vec![[0i16; 64]; c.coef.len()];
            codec::code_component(&mut dd, &mut md, ci.min(1), &mut coef, c.bw, c.bh);
            img2.comps.push(Comp { id: c.id, h: c.h, v: c.v, bw: c.bw, bh: c.bh, coef });
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
    fn walk(p: &Path, out: &mut Vec<std::path::PathBuf>) {
        for e in fs::read_dir(p).unwrap() {
            let e = e.unwrap().path();
            if e.is_dir() { walk(&e, out) } else { out.push(e) }
        }
    }
    walk(Path::new(&dir), &mut files);
    files.sort();
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
                estimate(&img, &mut t);
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
    for (i, name) in ["A: no neighbours (T.81-like)", "B: neighbour contexts + LOCO-I DC", "D: B's contexts, multi-symbol (rANS-able)", "E: D with static per-image tables (raw = table bits)"].iter().enumerate() {
        let mb = t.bits[i] / 8e6;
        println!("  {name}: {:.2} MB of coefficients ({:.1}% of the decoded files), {:.1} decisions per input byte, {:.2} raw bits per input byte",
            mb, 100.0 * (1.0 - mb / (scan_ok as f64 / 1e6)), t.decisions[i] as f64 / scan_ok as f64, t.raw[i] / scan_ok as f64);
    }
    println!("  re-encoded byte for byte: {exact} of {}", files.len() - failed);
    let _ = other;
}
