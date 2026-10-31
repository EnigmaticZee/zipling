//! Our JPEG coefficient coder (Default and up): the research coder of
//! `examples/jpegmodel/fast.rs`, in the engine.
//!
//! A baseline JPEG's scans are decoded to quantised coefficients and re-coded
//! with a static model per image: one symbol per value (zero, one, or magnitude
//! class with its top mantissa bit), the low bits and sign raw; the nonzero count
//! of each block from its neighbours' counts; a dense block that repeats an
//! earlier one copied by distance; DC from a LOCO-I prediction. Symbol frequencies
//! are quantised to half-octave levels, sent with an adaptive binary coder, and
//! drive four interleaved rANS states. About 2.5% smaller than Lepton, decoding
//! 2-3x as fast (see RESEARCH.md, "Photos", rounds 3-5).
//!
//! Stored layout (the prefix compresses with the binary lane, the rest is stored):
//!   prefix: uv header length, the file with its scan data removed,
//!           uv scan count, then per scan uv offset and uv length
//!   rest:   u32 side length, side (tables), u32 raw length, raw bits, rANS words
//!
//! `encode` only parses and codes; `transform::forward` decodes the result and
//! compares it with the original before accepting it, as for every transform.
//! `decode` must not panic on any input: a damaged archive is an error, never
//! undefined behaviour.

use std::collections::HashMap;

// ---------- JPEG layout and entropy decoding ----------

const ZZ: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43,
    36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];
const _: () = assert!(ZZ.len() == 64);

/// At most this many 8x8 blocks per image: 512 MB of coefficients, an 8192 x 8192
/// 4:2:0 wallpaper being 1.5M blocks. Lepton takes anything larger.
const MAX_BLOCKS: usize = 1 << 22;

#[derive(Clone)]
struct Huff {
    maxcode: [i32; 18],
    valptr: [i32; 17],
    mincode: [i32; 17],
    vals: Vec<u8>,
}

impl Huff {
    fn new(counts: &[u8; 16], vals: &[u8]) -> Huff {
        let mut h = Huff { maxcode: [0; 18], valptr: [0; 17], mincode: [0; 17], vals: vals.to_vec() };
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
    #[inline(always)]
    fn bit(&mut self) -> u32 {
        if self.n == 0 {
            self.fill();
        }
        let b = (self.acc >> 63) as u32;
        self.acc <<= 1;
        self.n -= 1;
        b
    }
    #[inline(always)]
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
    /// Drops buffered bits and moves past the next restart marker.
    fn restart(&mut self) -> bool {
        self.acc = 0;
        self.n = 0;
        self.hit_marker = false;
        while self.pos + 1 < self.d.len() {
            if self.d[self.pos] == 0xFF && (0xD0..=0xD7).contains(&self.d[self.pos + 1]) {
                self.pos += 2;
                return true;
            }
            self.pos += 1;
        }
        false
    }
}

#[inline(always)]
fn extend(v: u32, s: u32) -> i32 {
    if s == 0 {
        0
    } else if v < (1 << (s - 1)) {
        v as i32 - (1 << s) + 1
    } else {
        v as i32
    }
}

type Dht = ([u8; 16], Vec<u8>);

struct Comp {
    id: u8,
    h: usize,
    v: usize,
    /// Blocks per row and rows, padded to whole MCUs.
    bw: usize,
    bh: usize,
}

struct Scan {
    /// (component index, DC table, AC table) in scan order.
    comps: Vec<(usize, u8, u8)>,
    start: usize,
    end: usize,
}

struct Layout {
    comps: Vec<Comp>,
    scans: Vec<Scan>,
    dc: HashMap<u8, Dht>,
    ac: HashMap<u8, Dht>,
    dri: usize,
    width: usize,
    height: usize,
    hmax: usize,
    vmax: usize,
}

impl Layout {
    /// Markers of a baseline JPEG; each scan's entropy data is located, not decoded.
    fn parse(d: &[u8]) -> Option<Layout> {
        if d.len() < 4 || d[0] != 0xFF || d[1] != 0xD8 {
            return None;
        }
        let mut i = 2;
        let mut l = Layout { comps: vec![], scans: vec![], dc: HashMap::new(), ac: HashMap::new(), dri: 0, width: 0, height: 0, hmax: 1, vmax: 1 };
        while i + 4 <= d.len() {
            if d[i] != 0xFF {
                return None;
            }
            let m = d[i + 1];
            if m == 0xD9 {
                break;
            }
            if m == 0xFF {
                i += 1;
                continue;
            }
            if (0xD0..=0xD8).contains(&m) || m == 0x01 {
                i += 2; // standalone markers
                continue;
            }
            let ln = u16::from_be_bytes([d[i + 2], d[i + 3]]) as usize;
            if ln < 2 || i + 2 + ln > d.len() {
                return None;
            }
            let seg = &d[i + 4..i + 2 + ln];
            match m {
                0xC0 | 0xC1 => {
                    if !l.comps.is_empty() || seg.len() < 6 || seg[0] != 8 {
                        return None;
                    }
                    l.height = u16::from_be_bytes([seg[1], seg[2]]) as usize;
                    l.width = u16::from_be_bytes([seg[3], seg[4]]) as usize;
                    let nc = seg[5] as usize;
                    if l.width == 0 || l.height == 0 || nc == 0 || nc > 4 || seg.len() < 6 + 3 * nc {
                        return None;
                    }
                    for c in 0..nc {
                        let hv = seg[7 + 3 * c];
                        let (h, v) = ((hv >> 4) as usize, (hv & 15) as usize);
                        if !(1..=4).contains(&h) || !(1..=4).contains(&v) {
                            return None;
                        }
                        l.comps.push(Comp { id: seg[6 + 3 * c], h, v, bw: 0, bh: 0 });
                    }
                    l.hmax = l.comps.iter().map(|c| c.h).max()?;
                    l.vmax = l.comps.iter().map(|c| c.v).max()?;
                    let mcux = l.width.div_ceil(8 * l.hmax);
                    let mcuy = l.height.div_ceil(8 * l.vmax);
                    let mut blocks = 0usize;
                    for c in l.comps.iter_mut() {
                        c.bw = mcux * c.h;
                        c.bh = mcuy * c.v;
                        blocks += c.bw * c.bh;
                    }
                    if blocks > MAX_BLOCKS {
                        return None;
                    }
                }
                0xC2 | 0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => return None, // not baseline
                0xC4 => {
                    let mut p = 0;
                    while p + 17 <= seg.len() {
                        let tc = seg[p] >> 4;
                        let th = seg[p] & 15;
                        let mut counts = [0u8; 16];
                        counts.copy_from_slice(&seg[p + 1..p + 17]);
                        let n: usize = counts.iter().map(|&x| x as usize).sum();
                        if n > 256 || p + 17 + n > seg.len() || tc > 1 || th > 3 {
                            return None;
                        }
                        let raw = (counts, seg[p + 17..p + 17 + n].to_vec());
                        if tc == 0 {
                            l.dc.insert(th, raw);
                        } else {
                            l.ac.insert(th, raw);
                        }
                        p += 17 + n;
                    }
                    if p != seg.len() {
                        return None;
                    }
                }
                0xDD => {
                    if seg.len() < 2 {
                        return None;
                    }
                    l.dri = u16::from_be_bytes([seg[0], seg[1]]) as usize;
                }
                0xDC => return None, // DNL: the height is not in the frame header
                0xDA => {
                    if l.comps.is_empty() || seg.is_empty() {
                        return None;
                    }
                    let ns = seg[0] as usize;
                    if ns == 0 || ns > 4 || seg.len() < 1 + 2 * ns + 3 {
                        return None;
                    }
                    let mut sc = Vec::with_capacity(ns);
                    for k in 0..ns {
                        let cid = seg[1 + 2 * k];
                        let t = seg[2 + 2 * k];
                        let ci = l.comps.iter().position(|c| c.id == cid)?;
                        if sc.iter().any(|&(c, _, _)| c == ci) {
                            return None;
                        }
                        sc.push((ci, t >> 4, t & 15));
                    }
                    // baseline: full spectral range, no successive approximation
                    let (ss, se, ahal) = (seg[1 + 2 * ns], seg[2 + 2 * ns], seg[3 + 2 * ns]);
                    if ss != 0 || se != 63 || ahal != 0 {
                        return None;
                    }
                    let start = i + 2 + ln;
                    // the entropy data ends at the first marker that is not stuffing or a restart
                    let mut j = start;
                    while j + 1 < d.len() {
                        if d[j] == 0xFF && d[j + 1] != 0 && !(0xD0..=0xD7).contains(&d[j + 1]) {
                            break;
                        }
                        j += 1;
                    }
                    if j + 1 >= d.len() {
                        return None; // no marker after the scan
                    }
                    l.scans.push(Scan { comps: sc, start, end: j });
                    i = j;
                    continue;
                }
                _ => {}
            }
            i += 2 + ln;
        }
        if l.scans.is_empty() || l.comps.is_empty() {
            return None;
        }
        Some(l)
    }

    fn mcus(&self) -> (usize, usize) {
        (self.width.div_ceil(8 * self.hmax), self.height.div_ceil(8 * self.vmax))
    }

    /// Blocks of a component that a one-component scan codes (not padded to MCUs).
    fn scan_dims(&self, c: &Comp) -> (usize, usize) {
        ((self.width * c.h).div_ceil(8 * self.hmax), (self.height * c.v).div_ceil(8 * self.vmax))
    }

    /// Decodes every scan into zeroed coefficient planes (zigzag order, block raster).
    fn decode_scans(&self, d: &[u8]) -> Option<Vec<Vec<[i16; 64]>>> {
        let mut planes: Vec<Vec<[i16; 64]>> = self.comps.iter().map(|c| vec![[0i16; 64]; c.bw * c.bh]).collect();
        for sc in &self.scans {
            let tabs: Vec<(Huff, Huff)> = sc
                .comps
                .iter()
                .map(|&(_, td, ta)| {
                    let (dc, ac) = (self.dc.get(&td)?, self.ac.get(&ta)?);
                    Some((Huff::new(&dc.0, &dc.1), Huff::new(&ac.0, &ac.1)))
                })
                .collect::<Option<_>>()?;
            let mut bits = Bits { d: &d[..sc.end], pos: sc.start, acc: 0, n: 0, hit_marker: false };
            let mut pred = vec![0i32; self.comps.len()];
            let mut mcu = 0usize;
            let decode_block = |bits: &mut Bits, ci: usize, t: &(Huff, Huff), pred: &mut [i32], blk: &mut [i16; 64]| -> Option<()> {
                let s = bits.decode(&t.0)? as u32;
                if s > 11 {
                    return None;
                }
                pred[ci] += extend(bits.get(s), s);
                blk[0] = pred[ci] as i16;
                let mut k = 1;
                while k < 64 {
                    let rs = bits.decode(&t.1)? as u32;
                    let (r, s) = (rs >> 4, rs & 15);
                    if s == 0 {
                        if r == 15 {
                            k += 16;
                            continue;
                        }
                        break;
                    }
                    k += r as usize;
                    if k > 63 || s > 10 {
                        return None;
                    }
                    blk[k] = extend(bits.get(s), s) as i16;
                    k += 1;
                }
                Some(())
            };
            if sc.comps.len() == 1 {
                let (ci, _, _) = sc.comps[0];
                let c = &self.comps[ci];
                let (cw, ch) = self.scan_dims(c);
                let bw = c.bw;
                for by in 0..ch {
                    for bx in 0..cw {
                        if self.dri > 0 && mcu > 0 && mcu % self.dri == 0 {
                            if !bits.restart() {
                                return None;
                            }
                            pred.iter_mut().for_each(|p| *p = 0);
                        }
                        decode_block(&mut bits, ci, &tabs[0], &mut pred, &mut planes[ci][by * bw + bx])?;
                        mcu += 1;
                    }
                }
            } else {
                let (mcux, mcuy) = self.mcus();
                for my in 0..mcuy {
                    for mx in 0..mcux {
                        if self.dri > 0 && mcu > 0 && mcu % self.dri == 0 {
                            if !bits.restart() {
                                return None;
                            }
                            pred.iter_mut().for_each(|p| *p = 0);
                        }
                        for (si, &(ci, _, _)) in sc.comps.iter().enumerate() {
                            let (h, v, bw) = (self.comps[ci].h, self.comps[ci].v, self.comps[ci].bw);
                            for y in 0..v {
                                for x in 0..h {
                                    let idx = (my * v + y) * bw + mx * h + x;
                                    decode_block(&mut bits, ci, &tabs[si], &mut pred, &mut planes[ci][idx])?;
                                }
                            }
                        }
                        mcu += 1;
                    }
                }
            }
        }
        Some(planes)
    }
}

// ---------- JPEG scan writing ----------

fn codes(t: &Dht) -> [(u32, u32); 256] {
    let (counts, vals) = t;
    let mut out = [(0u32, 0u32); 256];
    let mut code = 0u32;
    let mut k = 0;
    for l in 1..=16u32 {
        for _ in 0..counts[l as usize - 1] {
            if let Some(&v) = vals.get(k) {
                out[v as usize] = (code, l);
            }
            code += 1;
            k += 1;
        }
        code <<= 1;
    }
    out
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
        self.acc = (self.acc << l) | v as u64;
        self.n += l;
        if self.n >= 32 {
            self.n -= 32;
            let w = (self.acc >> self.n) as u32;
            let inv = !w;
            if (inv.wrapping_sub(0x0101_0101) & !inv & 0x8080_8080) == 0 {
                self.out.extend_from_slice(&w.to_be_bytes());
            } else {
                for b in w.to_be_bytes() {
                    self.out.push(b);
                    if b == 0xFF {
                        self.out.push(0);
                    }
                }
            }
        }
    }
    /// Flushes whole bytes, then pads the last partial byte with ones.
    fn pad(&mut self) {
        while self.n >= 8 {
            self.n -= 8;
            let b = (self.acc >> self.n) as u8;
            self.out.push(b);
            if b == 0xFF {
                self.out.push(0);
            }
        }
        if self.n > 0 {
            let k = 8 - self.n;
            let b = (((self.acc << k) | ((1 << k) - 1)) & 0xFF) as u8;
            self.out.push(b);
            if b == 0xFF {
                self.out.push(0);
            }
            self.n = 0;
        }
        self.acc = 0;
    }
}

/// Nonzero AC positions of a block: four masks of 16 nibbles, 0xF where the
/// coefficient is nonzero (the DC lane cleared).
#[inline(always)]
fn nibble_masks(blk: &[i16; 64]) -> [u64; 4] {
    let mut masks = [0u64; 4];
    #[cfg(target_arch = "aarch64")]
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
    #[cfg(not(target_arch = "aarch64"))]
    for k in 0..64 {
        if blk[k] != 0 {
            masks[k / 16] |= 0xFu64 << (4 * (k % 16));
        }
    }
    masks[0] &= !0xF;
    masks
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
    let (c, l) = dct[(s as usize).min(255)];
    w.put((c << s) | b, l + s);
    let masks = nibble_masks(blk);
    let mut last = 0u32;
    for (q, &mask) in masks.iter().enumerate() {
        let mut m = mask;
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
            let (s, b) = mag(blk[k as usize] as i32);
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

/// Writes one scan's entropy data from the coefficient planes.
fn write_scan(l: &Layout, planes: &[Vec<[i16; 64]>], sc: &Scan, cap: usize) -> Option<Vec<u8>> {
    let mut w = BW { out: Vec::with_capacity(cap + 4096), acc: 0, n: 0 };
    let mut pred = vec![0i32; l.comps.len()];
    let tabs: Vec<_> = sc.comps.iter().map(|&(_, td, ta)| Some((codes(l.dc.get(&td)?), codes(l.ac.get(&ta)?)))).collect::<Option<_>>()?;
    let mut mcu = 0usize;
    let mut rst = 0u8;
    let mut restart = |w: &mut BW, pred: &mut Vec<i32>, mcu: usize| {
        if l.dri > 0 && mcu > 0 && mcu % l.dri == 0 {
            w.pad();
            w.out.push(0xFF);
            w.out.push(0xD0 + rst);
            rst = (rst + 1) & 7;
            pred.iter_mut().for_each(|p| *p = 0);
        }
    };
    if sc.comps.len() == 1 {
        let (ci, _, _) = sc.comps[0];
        let c = &l.comps[ci];
        let (cw, ch) = l.scan_dims(c);
        for by in 0..ch {
            for bx in 0..cw {
                restart(&mut w, &mut pred, mcu);
                enc_block(&mut w, &planes[ci][by * c.bw + bx], &mut pred[ci], &tabs[0].0, &tabs[0].1);
                mcu += 1;
            }
        }
    } else {
        let (mcux, mcuy) = l.mcus();
        for my in 0..mcuy {
            for mx in 0..mcux {
                restart(&mut w, &mut pred, mcu);
                for (si, &(ci, _, _)) in sc.comps.iter().enumerate() {
                    let c = &l.comps[ci];
                    for y in 0..c.v {
                        for x in 0..c.h {
                            enc_block(&mut w, &planes[ci][(my * c.v + y) * c.bw + mx * c.h + x], &mut pred[ci], &tabs[si].0, &tabs[si].1);
                        }
                    }
                }
                mcu += 1;
            }
        }
    }
    w.pad();
    Some(w.out)
}

// ---------- the coefficient model ----------

const PB: u32 = 12;
const M: u32 = 1 << PB;
const L: u32 = 1 << 16;

// symbol types: (context space, alphabet)
const T_N: usize = 0;
const T_MF: usize = 1;
const T_MD: usize = 2;
const T_AC: usize = 3;
const T_DC: usize = 6;
const TYPES: [(usize, usize); 8] = [(2 * 9, 64), (32, 2), (16, 32), (2 * 64 * 32 * 8, 32), (2 * 16 * 16, 2), (16, 2), (2 * 11 * 8, 32), (32, 2)];

/// One symbol per value: 0 = zero, 1 = magnitude 1, then (category e >= 2, top mantissa bit)
/// as 2 + 2 (e - 2) + top. Sign and the bits below the top one are raw.
#[inline(always)]
fn joint(a: u32) -> usize {
    if a == 0 {
        return 0;
    }
    let e = 32 - a.leading_zeros();
    if e == 1 {
        return 1;
    }
    2 + 2 * (e as usize - 2) + ((a >> (e - 2)) & 1) as usize
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

#[inline(always)]
fn bucket(x: u32) -> usize {
    if x < 3 {
        x as usize
    } else {
        1 + (32 - (x - 1).leading_zeros()) as usize
    }
}

const RECIP: [u32; 7] = [0, 65536, 32768, 21845, 16384, 13107, 10922];

trait Io {
    const ENC: bool;
    fn sym<const A: usize>(&mut self, t: usize, ctx: usize, s: usize) -> usize;
    fn raw(&mut self, v: u32, n: u32) -> u32;
    fn bad(&mut self);
}

/// Coefficients of one component in zigzag order, block raster.
struct Plane<'a> {
    coef: &'a mut [[i16; 64]],
    bw: usize,
    bh: usize,
}

#[inline(always)]
fn value<I: Io>(io: &mut I, ctx: usize, v: i32) -> i32 {
    // branch-free: the low bits and the sign come in one raw read
    let a = v.unsigned_abs();
    let sy = io.sym::<32>(T_AC, ctx, if I::ENC { joint(a) } else { 0 });
    let (hi, nlow) = SPLIT[sy & 31];
    let ns = (sy != 0) as u32;
    let enc = if I::ENC { (a & ((1u32 << nlow) - 1)) | (((v < 0) as u32) << nlow) } else { 0 };
    let bits = io.raw(enc, nlow + ns);
    let m = (hi | (bits & ((1u32 << nlow) - 1))) as i32;
    let sign = ((bits >> nlow) & ns) as i32;
    (m ^ -sign) + sign
}

/// Expected-magnitude level of every AC position from the neighbours' values, as
/// a context index: ((cc * 64 + k) * 32 + level) * 8 + lb.
#[inline(always)]
fn ac_contexts(ctxs: &mut [u32; 64], nb: [Option<&[i16; 64]>; 4], w: u32, cc: usize, lb: u32) {
    if w == 0 {
        for k in 1..64 {
            ctxs[k] = ((((cc * 64 + k) * 32 + 31) * 8) as u32) + lb;
        }
        return;
    }
    let zero = [0i16; 64];
    let [pu, pl, pul, pur] = nb.map(|p| p.unwrap_or(&zero));
    #[cfg(target_arch = "aarch64")]
    unsafe {
        use std::arch::aarch64::*;
        let cap = vdupq_n_u16(1023);
        let rc = vdupq_n_u32(RECIP[w as usize]);
        for j in 0..8 {
            let a = |q: &[i16; 64]| vminq_u16(vreinterpretq_u16_s16(vabsq_s16(vld1q_s16(q.as_ptr().add(8 * j)))), cap);
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
                let base = vshlq_n_u32::<5>(vaddq_u32(vdupq_n_u32((cc * 64) as u32), kv));
                let c = vaddq_u32(vshlq_n_u32::<3>(vaddq_u32(base, l)), vdupq_n_u32(lb));
                vst1q_u32(ctxs.as_mut_ptr().add(k0 as usize), c);
            }
        }
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        // The same arithmetic, one position at a time.
        let a = |q: &[i16; 64], k: usize| (q[k].unsigned_abs() as u32).min(1023);
        for k in 0..64 {
            let s16 = ((a(pu, k) + a(pl, k)) << 4) + ((a(pul, k) + a(pur, k)) << 3);
            let x8 = (s16 * RECIP[w as usize]) >> 16;
            let l = if x8 == 0 {
                0
            } else {
                let y = x8 + 8;
                let m = 31 - y.leading_zeros();
                let half = (y >> (m - 1)) & 1;
                ((2 * m + half) as i32 - 5).clamp(1, 30) as u32
            };
            ctxs[k] = (((cc * 64 + k) as u32 * 32 + l) * 8) + lb;
        }
    }
}

/// The model walk: codes (or decodes into) every block of every plane.
fn walk<I: Io>(io: &mut I, planes: &mut [Plane]) {
    for (ci, pl) in planes.iter_mut().enumerate() {
        let cc = ci.min(1);
        let (bw, bh) = (pl.bw, pl.bh);
        let coef = &mut *pl.coef;
        let mut nzs = vec![0u8; bw * bh]; // nonzero AC counts of coded blocks
        // block copy: per nonzero count, every dense block so far (block index), and on the
        // encoder a map from AC pattern to its latest position in that list
        let mut lists: Vec<Vec<u32>> = vec![Vec::new(); 64];
        let mut seen: HashMap<[i16; 63], u32> = HashMap::new();
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
                if has_up {
                    s += 2 * nzs[b - bw] as u32;
                    w += 2;
                }
                if has_lf {
                    s += 2 * nzs[b - 1] as u32;
                    w += 2;
                }
                if has_ul {
                    s += nzs[b - bw - 1] as u32;
                    w += 1;
                }
                if has_ur {
                    s += nzs[b - bw + 1] as u32;
                    w += 1;
                }
                let nctx = if w == 0 { 8 } else { bucket((2 * s + w) / (2 * w)).min(7) };
                let n_enc = if I::ENC { coef[b][1..].iter().filter(|&&v| v != 0).count() } else { 0 };
                let n = (io.sym::<64>(T_N, cc * 9 + nctx, n_enc) as u32).min(63);
                nzs[b] = n as u8;
                let mut copied = false;
                if n >= 6 {
                    let mut key = [0i16; 63];
                    if I::ENC {
                        key.copy_from_slice(&coef[b][1..]);
                    }
                    let hit = if I::ENC { seen.get(&key).copied() } else { None };
                    let fctx = cc * 16 + bucket(n).min(7) * 2 + prev_match;
                    let m = io.sym::<2>(T_MF, fctx, hit.is_some() as usize) == 1;
                    let list = &mut lists[n as usize];
                    if m {
                        let d_enc = if I::ENC { list.len() as u32 - hit.unwrap_or(0) } else { 0 };
                        let e = io.sym::<32>(T_MD, cc * 8 + bucket(n).min(7), if I::ENC { (32 - d_enc.leading_zeros()) as usize } else { 0 }) as u32;
                        if !(1..=24).contains(&e) {
                            io.bad();
                        } else {
                            let low = io.raw(d_enc & ((1u32 << (e - 1)) - 1), e - 1);
                            let d = ((1u32 << (e - 1)) | low) as usize;
                            if d == 0 || d > list.len() {
                                io.bad();
                            } else if !I::ENC {
                                let src = list[list.len() - d] as usize;
                                let (from, to) = (src.min(b), src.max(b));
                                let (lo, hi) = coef.split_at_mut(to);
                                let (src_blk, dst_blk) = if src < b { (&lo[from], &mut hi[0]) } else { (&hi[0], &mut lo[from]) };
                                dst_blk[1..].copy_from_slice(&src_blk[1..]);
                            }
                            copied = true;
                        }
                    }
                    if I::ENC {
                        seen.insert(key, list.len() as u32);
                    }
                    list.push(b as u32);
                    prev_match = m as usize;
                }
                if !copied && n > 0 {
                    let mut ctxs = [0u32; 64];
                    let w = 2 * has_up as u32 + 2 * has_lf as u32 + has_ul as u32 + has_ur as u32;
                    let lb = bucket(n).min(7) as u32;
                    let nb = [
                        has_up.then(|| &coef[b - bw]),
                        has_lf.then(|| &coef[b - 1]),
                        has_ul.then(|| &coef[b - bw - 1]),
                        has_ur.then(|| &coef[b - bw + 1]),
                    ];
                    ac_contexts(&mut ctxs, nb, w, cc, lb);
                    let mut left = n;
                    let mut k = 1;
                    while left > 0 && k < 64 {
                        let v = value(io, ctxs[k] as usize, coef[b][k] as i32);
                        coef[b][k] = v as i16;
                        left -= (v != 0) as u32;
                        k += 1;
                    }
                    if left > 0 {
                        io.bad();
                    }
                }
                // DC: LOCO-I from neighbour DCs
                let a = if has_lf { coef[b - 1][0] as i32 } else { 0 };
                let bb = if has_up { coef[b - bw][0] as i32 } else { 0 };
                let c = if has_ul { coef[b - bw - 1][0] as i32 } else { 0 };
                let pred = match (has_lf, has_up) {
                    (true, true) => {
                        if c >= a.max(bb) {
                            a.min(bb)
                        } else if c <= a.min(bb) {
                            a.max(bb)
                        } else {
                            a + bb - c
                        }
                    }
                    (true, false) => a,
                    (false, true) => bb,
                    _ => 0,
                };
                let act = bucket((a - c).unsigned_abs() + (bb - c).unsigned_abs()).min(10);
                let r_enc = if I::ENC { coef[b][0] as i32 - pred } else { 0 };
                let ra = r_enc.unsigned_abs();
                let sy = io.sym::<32>(T_DC, (cc * 11 + act) * 8 + bucket(n).min(7), if I::ENC { joint(ra) } else { 0 });
                let mut r = 0i32;
                if sy > 0 {
                    let (hi, nlow) = SPLIT[sy & 31];
                    let m = hi | io.raw(ra & ((1u32 << nlow) - 1), nlow);
                    let neg = io.raw((r_enc < 0) as u32, 1) == 1;
                    r = if neg { -(m as i32) } else { m as i32 };
                }
                coef[b][0] = (pred + r) as i16;
            }
        }
    }
}

// ---------- binary range coder for the tables ----------

trait Rc {
    const ENC: bool;
    fn bit(&mut self, p: &mut u16, b: bool) -> bool;
}

const SHIFT: u32 = 5;
const TOP: u32 = 1 << 24;

struct BEnc {
    low: u64,
    range: u32,
    cache: u8,
    cache_size: u64,
    out: Vec<u8>,
}

impl BEnc {
    fn new() -> BEnc {
        BEnc { low: 0, range: 0xFFFF_FFFF, cache: 0, cache_size: 1, out: Vec::new() }
    }
    #[inline]
    fn shift_low(&mut self) {
        if (self.low as u32) < 0xFF00_0000 || (self.low >> 32) != 0 {
            let carry = (self.low >> 32) as u8;
            let mut temp = self.cache;
            loop {
                self.out.push(temp.wrapping_add(carry));
                temp = 0xFF;
                self.cache_size -= 1;
                if self.cache_size == 0 {
                    break;
                }
            }
            self.cache = ((self.low >> 24) & 0xFF) as u8;
        }
        self.cache_size += 1;
        self.low = (self.low & 0x00FF_FFFF) << 8;
    }
    fn finish(mut self) -> Vec<u8> {
        for _ in 0..5 {
            self.shift_low();
        }
        self.out
    }
}

impl Rc for BEnc {
    const ENC: bool = true;
    #[inline(always)]
    fn bit(&mut self, p: &mut u16, b: bool) -> bool {
        let bound = (self.range >> 12) * (*p as u32);
        if !b {
            self.range = bound;
            *p += (4096 - *p) >> SHIFT;
        } else {
            self.low += bound as u64;
            self.range -= bound;
            *p -= *p >> SHIFT;
        }
        while self.range < TOP {
            self.range <<= 8;
            self.shift_low();
        }
        b
    }
}

struct BDec<'a> {
    range: u32,
    code: u32,
    d: &'a [u8],
    pos: usize,
}

impl<'a> BDec<'a> {
    fn new(d: &'a [u8]) -> BDec<'a> {
        let mut x = BDec { range: 0xFFFF_FFFF, code: 0, d, pos: 0 };
        for _ in 0..5 {
            x.code = (x.code << 8) | x.byte() as u32;
        }
        x
    }
    #[inline(always)]
    fn byte(&mut self) -> u8 {
        let b = *self.d.get(self.pos).unwrap_or(&0);
        self.pos += 1;
        b
    }
}

impl<'a> Rc for BDec<'a> {
    const ENC: bool = false;
    #[inline(always)]
    fn bit(&mut self, p: &mut u16, _b: bool) -> bool {
        let bound = (self.range >> 12) * (*p as u32);
        let r;
        if self.code < bound {
            self.range = bound;
            *p += (4096 - *p) >> SHIFT;
            r = false;
        } else {
            self.code -= bound;
            self.range -= bound;
            *p -= *p >> SHIFT;
            r = true;
        }
        if self.range < TOP {
            self.range <<= 8;
            self.code = (self.code << 8) | self.byte() as u32;
        }
        r
    }
}

// ---------- tables ----------

/// Approximate counts as half-octave levels: 0 absent, else 1 + floor(2 log2 c).
/// Encoder only: the decoder reads the levels from the side stream.
fn quant(c: u32) -> u8 {
    if c == 0 {
        0
    } else {
        (1 + ((c as f64).log2() * 2.0).floor() as u32).min(40) as u8
    }
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

/// Normalises approximate counts to frequencies summing to M (present symbols >= 1).
fn normalise(w: &[u32], freq: &mut [u32]) {
    let tot: u64 = w.iter().map(|&x| x as u64).sum();
    let mut sum = 0u32;
    let mut big = 0;
    for (i, &x) in w.iter().enumerate() {
        freq[i] = if x == 0 { 0 } else { ((x as u64 * M as u64) / tot).max(1) as u32 };
        sum += freq[i];
        if freq[i] > freq[big] {
            big = i;
        }
    }
    if sum > M {
        freq[big] = freq[big].saturating_sub(sum - M).max(1);
    } else {
        freq[big] += M - sum;
    }
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
            if !u {
                continue;
            }
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

// ---------- encoder ----------

struct BitW {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl BitW {
    #[inline(always)]
    fn put(&mut self, v: u32, n: u32) {
        if n == 0 {
            return;
        }
        self.acc |= (v as u64) << self.n;
        self.n += n;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }
    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push(self.acc as u8);
        }
        self.out
    }
}

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
    fn bad(&mut self) {}
}

/// Codes the planes: [u32 side len][side][u32 raw len][raw][rANS words].
fn encode_planes(planes: &mut [Plane]) -> Vec<u8> {
    let mut rec = Recorder { events: Vec::new(), counts: TYPES.iter().map(|&(s, a)| vec![0; s * a]).collect(), raw: BitW { out: Vec::new(), acc: 0, n: 0 } };
    walk(&mut rec, planes);
    // quantise counts, code the tables, then rebuild the exact frequencies the decoder will use
    let mut q: Vec<Vec<u8>> = rec.counts.iter().map(|v| v.iter().map(|&c| quant(c)).collect()).collect();
    let mut used: Vec<Vec<bool>> =
        TYPES.iter().enumerate().map(|(t, &(s, a))| (0..s).map(|c| rec.counts[t][c * a..(c + 1) * a].iter().any(|&x| x > 0)).collect()).collect();
    let mut be = BEnc::new();
    code_tables(&mut be, &mut q, &mut used);
    let side = be.finish();
    let (freqs, cums) = build(&q, &used);
    // rANS, backwards, four states
    let mut x = [L; 4];
    let mut words: Vec<u16> = Vec::with_capacity(rec.events.len() / 2 + 8);
    for (i, &(t, ctx, s)) in rec.events.iter().enumerate().rev() {
        let a = TYPES[t as usize].1;
        let f = freqs[t as usize][ctx as usize * a + s as usize];
        let st = cums[t as usize][ctx as usize * (a + 1) + s as usize] as u32;
        let xs = &mut x[i & 3];
        let xmax = (((L >> PB) << 16) as u64) * f as u64;
        if *xs as u64 >= xmax {
            words.push(*xs as u16);
            *xs >>= 16;
        }
        *xs = ((*xs / f) << PB) + (*xs % f) + st;
    }
    for xs in [x[3], x[2], x[1], x[0]] {
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
        let mut cu = vec![0u16; space * (a + 1)];
        let mut w = vec![0u32; a];
        for ctx in 0..space {
            if !used[t][ctx] {
                continue;
            }
            for s in 0..a {
                w[s] = DEQ[q[t][ctx * a + s] as usize & 63];
            }
            normalise(&w, &mut f[ctx * a..(ctx + 1) * a]);
            let mut acc = 0u32;
            for s in 0..a {
                cu[ctx * (a + 1) + s] = acc as u16;
                acc += f[ctx * a + s];
            }
            cu[ctx * (a + 1) + a] = acc as u16;
        }
        freqs.push(f);
        cums.push(cu);
    }
    (freqs, cums)
}

// ---------- decoder ----------

struct Decoder {
    map: Vec<Vec<u16>>,
    flat: Vec<u16>,
    base: [usize; 8],
    bk: Vec<u32>,
    bbase: [usize; 8],
    x0: u32,
    x1: u32,
    x2: u32,
    x3: u32,
    words: Vec<u8>,
    wp: usize,
    raw: Vec<u8>,
    rp: usize,
    acc: u64,
    n: u32,
    bad: bool,
}

impl Io for Decoder {
    const ENC: bool = false;
    #[inline(always)]
    fn sym<const A: usize>(&mut self, t: usize, ctx: usize, _s: usize) -> usize {
        let Some(&dense) = self.map[t].get(ctx) else {
            self.bad = true;
            return 0;
        };
        if dense == u16::MAX {
            self.bad = true;
            return 0;
        }
        let ctx = dense as usize;
        let base = self.base[t] + ctx * (A + 1);
        let xs = self.x0;
        let slot = (xs & (M - 1)) as u16;
        let mut s: usize;
        let p = &self.flat[base..base + A + 1];
        let st: u32;
        let f: u32;
        if A == 2 {
            s = (p[1] <= slot) as usize;
            st = p[s] as u32;
            f = p[s + 1] as u32 - st;
        } else {
            // bucket entry: first symbol of the 64-slot bucket with its start and last slot
            let e = self.bk[self.bbase[t] + ctx * 64 + (slot >> 6) as usize];
            let last = e >> 20;
            if slot as u32 <= last {
                s = (e & 0xFF) as usize;
                st = (e >> 8) & 0xFFF;
                f = last - st + 1;
            } else {
                s = (e & 0xFF) as usize + 1;
                while s + 1 < A && p[s + 1] <= slot {
                    s += 1;
                }
                st = p[s] as u32;
                f = p[s + 1] as u32 - st;
            }
        }
        let mut x = f * (xs >> PB) + slot as u32 - st;
        if x < L {
            let w = match self.words.get(self.wp..self.wp + 2) {
                Some(b) => u16::from_le_bytes([b[0], b[1]]),
                None => {
                    self.bad = true;
                    0
                }
            };
            self.wp += 2;
            x = (x << 16) | w as u32;
        }
        self.x0 = self.x1;
        self.x1 = self.x2;
        self.x2 = self.x3;
        self.x3 = x;
        s
    }
    #[inline(always)]
    fn raw(&mut self, _v: u32, n: u32) -> u32 {
        if self.n < 32 {
            match self.raw.get(self.rp..self.rp + 8) {
                Some(b) => {
                    self.acc |= u64::from_le_bytes(b.try_into().unwrap()) << self.n;
                    let take = (63 - self.n) >> 3;
                    self.rp += take as usize;
                    self.n += take << 3;
                }
                None => {
                    // past the end: zeros, and the result will not verify
                    self.bad = true;
                    self.n = 64;
                }
            }
        }
        let v = (self.acc & ((1u64 << n) - 1)) as u32;
        self.acc >>= n;
        self.n -= n;
        v
    }
    fn bad(&mut self) {
        self.bad = true;
    }
}

impl Decoder {
    fn new(data: &[u8]) -> Option<Decoder> {
        let sl = u32::from_le_bytes(data.get(0..4)?.try_into().ok()?) as usize;
        let side = data.get(4..4 + sl)?;
        let rl = u32::from_le_bytes(data.get(4 + sl..8 + sl)?.try_into().ok()?) as usize;
        let raw = data.get(8 + sl..8 + sl + rl)?;
        let words = data.get(8 + sl + rl..)?;
        if words.len() < 16 {
            return None;
        }
        let mut q: Vec<Vec<u8>> = TYPES.iter().map(|&(s, a)| vec![0u8; s * a]).collect();
        let mut used: Vec<Vec<bool>> = TYPES.iter().map(|&(s, _)| vec![false; s]).collect();
        let mut bd = BDec::new(side);
        code_tables(&mut bd, &mut q, &mut used);
        // dense tables for the used contexts only
        let mut map: Vec<Vec<u16>> = Vec::with_capacity(8);
        let mut base = [0usize; 8];
        let mut bbase = [0usize; 8];
        let mut flat: Vec<u16> = Vec::new();
        let mut bk: Vec<u32> = Vec::new();
        let mut w = [0u32; 64];
        let mut f = [0u32; 64];
        for t in 0..8 {
            let (space, a) = TYPES[t];
            base[t] = flat.len();
            bbase[t] = bk.len();
            let mut m = vec![u16::MAX; space];
            let mut dense = 0u16;
            for ctx in 0..space {
                if !used[t][ctx] {
                    continue;
                }
                m[ctx] = dense;
                dense += 1;
                for s in 0..a {
                    w[s] = DEQ[q[t][ctx * a + s] as usize & 63];
                }
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
                        while sym + 1 < a && c[sym + 1] <= lo {
                            sym += 1;
                        }
                        let st = c[sym] as u32;
                        let last = (c[sym + 1] as u32).saturating_sub(1).min(4095);
                        bk.push(sym as u32 | (st << 8) | (last << 20));
                    }
                }
            }
            map.push(m);
        }
        flat.extend_from_slice(&[u16::MAX; 16]);
        let rd = |p: usize| u16::from_le_bytes([words[p], words[p + 1]]) as u32;
        let x0 = rd(0) | (rd(2) << 16);
        let x1 = rd(4) | (rd(6) << 16);
        let x2 = rd(8) | (rd(10) << 16);
        let x3 = rd(12) | (rd(14) << 16);
        // The raw reader refills eight bytes at a time and takes up to seven of
        // them, so at the end it can stand seven bytes past the data and read
        // eight more: sixteen bytes of padding keep that in bounds, and reading
        // past the padding marks the stream bad.
        let mut words = words.to_vec();
        words.extend_from_slice(&[0; 8]);
        let mut raw = raw.to_vec();
        raw.extend_from_slice(&[0; 16]);
        Some(Decoder { map, flat, base, bk, bbase, x0, x1, x2, x3, words, wp: 16, raw, rp: 0, acc: 0, n: 0, bad: false })
    }
}

// ---------- the transform ----------

fn uv(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let b = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            break;
        }
        out.push(b | 0x80);
    }
}

fn read_uv(d: &[u8], p: &mut usize) -> Option<u64> {
    let mut v = 0u64;
    for i in 0..10 {
        let b = *d.get(*p)?;
        *p += 1;
        v |= ((b & 0x7F) as u64) << (7 * i);
        if b & 0x80 == 0 {
            return Some(v);
        }
    }
    None
}

/// Re-codes a baseline JPEG, or returns `None` for one this coder does not take.
/// The second value is where the stored (incompressible) part begins.
pub fn encode(d: &[u8]) -> Option<(Vec<u8>, usize)> {
    let l = Layout::parse(d)?;
    let mut coefs = l.decode_scans(d)?;
    let body = {
        let mut planes: Vec<Plane> = coefs.iter_mut().zip(&l.comps).map(|(cf, c)| Plane { coef: cf, bw: c.bw, bh: c.bh }).collect();
        encode_planes(&mut planes)
    };
    // the file without its scan data
    let mut hdr = Vec::with_capacity(4096);
    let mut at = 0;
    for sc in &l.scans {
        hdr.extend_from_slice(&d[at..sc.start]);
        at = sc.end;
    }
    hdr.extend_from_slice(&d[at..]);
    let mut out = Vec::with_capacity(hdr.len() + body.len() + 64);
    uv(&mut out, hdr.len() as u64);
    out.extend_from_slice(&hdr);
    uv(&mut out, l.scans.len() as u64);
    for sc in &l.scans {
        uv(&mut out, sc.start as u64);
        uv(&mut out, (sc.end - sc.start) as u64);
    }
    let store_from = out.len();
    out.extend_from_slice(&body);
    (out.len() < d.len()).then_some((out, store_from))
}

/// `encode`, then `decode`, keeping only an exact round trip: what the engine
/// stores. A photo that does not come back exactly goes to the next coder.
pub fn encode_verified(d: &[u8]) -> Option<(Vec<u8>, usize)> {
    let (out, from) = encode(d)?;
    match decode(&out, d.len() as u64) {
        Ok(back) if back == d => Some((out, from)),
        _ => None,
    }
}

/// Rebuilds the JPEG from `encode`'s output.
pub fn decode(stored: &[u8], size: u64) -> anyhow::Result<Vec<u8>> {
    let mut p = 0usize;
    let fail = |what: &'static str| anyhow::anyhow!("jpegrans: damaged stream ({what})");
    let hl = read_uv(stored, &mut p).ok_or_else(|| fail("header length"))? as usize;
    let hdr = stored.get(p..p + hl).ok_or_else(|| fail("header"))?;
    p += hl;
    let ns = read_uv(stored, &mut p).ok_or_else(|| fail("scan count"))? as usize;
    if ns == 0 || ns > 64 {
        return Err(fail("scan count"));
    }
    let mut spans = Vec::with_capacity(ns);
    for _ in 0..ns {
        let start = read_uv(stored, &mut p).ok_or_else(|| fail("scan offsets"))? as usize;
        let len = read_uv(stored, &mut p).ok_or_else(|| fail("scan offsets"))? as usize;
        spans.push((start, len));
    }
    let body = stored.get(p..).ok_or_else(|| fail("body"))?;
    // the header parses as a JPEG whose scans hold no data
    let l = Layout::parse(hdr).ok_or_else(|| fail("layout"))?;
    if l.scans.len() != ns {
        return Err(fail("scan count differs"));
    }
    let mut coefs: Vec<Vec<[i16; 64]>> = l.comps.iter().map(|c| vec![[0i16; 64]; c.bw * c.bh]).collect();
    let mut dec = Decoder::new(body).ok_or_else(|| fail("tables"))?;
    {
        let mut planes: Vec<Plane> = coefs.iter_mut().zip(&l.comps).map(|(cf, c)| Plane { coef: cf, bw: c.bw, bh: c.bh }).collect();
        walk(&mut dec, &mut planes);
    }
    if dec.bad {
        return Err(fail("symbols"));
    }
    let mut out = Vec::with_capacity(size as usize + 16);
    let mut at = 0usize; // position in hdr
    let mut removed = 0usize; // scan bytes before this point in the original
    for (sc, &(start, len)) in l.scans.iter().zip(&spans) {
        let hpos = start.checked_sub(removed).ok_or_else(|| fail("scan offsets"))?;
        if hpos < at || hpos > hdr.len() || hpos != sc.start {
            return Err(fail("scan position"));
        }
        out.extend_from_slice(&hdr[at..hpos]);
        let scan = write_scan(&l, &coefs, sc, len).ok_or_else(|| fail("scan tables"))?;
        out.extend_from_slice(&scan);
        at = hpos;
        removed += len;
    }
    out.extend_from_slice(&hdr[at..]);
    anyhow::ensure!(out.len() as u64 == size, "jpegrans: rebuilt {} bytes, expected {size}", out.len());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// A BMP with gradients, tiles and noise, so sips makes a JPEG with real structure.
    fn bmp(w: usize, h: usize, seed: u32) -> Vec<u8> {
        let row = (w * 3 + 3) & !3;
        let size = 54 + row * h;
        let mut d = Vec::with_capacity(size);
        d.extend_from_slice(b"BM");
        d.extend_from_slice(&(size as u32).to_le_bytes());
        d.extend_from_slice(&[0; 4]);
        d.extend_from_slice(&54u32.to_le_bytes());
        d.extend_from_slice(&40u32.to_le_bytes());
        d.extend_from_slice(&(w as i32).to_le_bytes());
        d.extend_from_slice(&(h as i32).to_le_bytes());
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&24u16.to_le_bytes());
        d.extend_from_slice(&[0; 24]);
        let mut s = seed;
        for y in 0..h {
            for x in 0..w {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                let tile = ((x / 16 + y / 16) % 3) as u32 * 60;
                let n = (s >> 26) as u32;
                d.push(((x * 255 / w) as u32 + n).min(255) as u8);
                d.push(((y * 255 / h) as u32 + tile).min(255) as u8);
                d.push((((x ^ y) & 0x3F) as u32 * 3 + n).min(255) as u8);
            }
            d.extend(std::iter::repeat(0).take(row - w * 3));
        }
        d
    }

    fn sips_jpeg(dir: &std::path::Path, name: &str, w: usize, h: usize, seed: u32, q: &str) -> Option<Vec<u8>> {
        let src = dir.join(format!("{name}.bmp"));
        let dst = dir.join(format!("{name}.jpg"));
        std::fs::write(&src, bmp(w, h, seed)).unwrap();
        let ok = Command::new("sips")
            .args(["-s", "format", "jpeg", "-s", "formatOptions", q])
            .arg(&src)
            .arg("--out")
            .arg(&dst)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        ok.then(|| std::fs::read(&dst).unwrap())
    }

    #[test]
    fn photos_from_sips_round_trip() {
        let dir = std::env::temp_dir().join(format!("arca-jpegrans-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut tried = 0;
        for (i, (w, h, q)) in [(64, 48, "40"), (333, 211, "85"), (800, 600, "100"), (17, 9, "60"), (1024, 768, "92")].iter().enumerate() {
            let Some(data) = sips_jpeg(&dir, &i.to_string(), *w, *h, i as u32 + 7, q) else { continue };
            tried += 1;
            let Some((stored, store_from)) = encode(&data) else {
                assert!(w * h < 10_000, "photo {i} ({w}x{h}) not encoded");
                continue;
            };
            assert!(store_from <= stored.len());
            let back = decode(&stored, data.len() as u64).unwrap();
            assert!(back == data, "photo {i} differs");
        }
        let _ = std::fs::remove_dir_all(&dir);
        if cfg!(target_os = "macos") {
            assert!(tried > 0);
        }
    }

    /// One random change: anywhere, in the headers and tables, or near the end.
    fn mutate(d: &mut Vec<u8>, s: &mut u64) {
        let mut r = |n: usize| {
            *s = s.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(0x1234_5);
            ((*s >> 33) as usize) % n.max(1)
        };
        let n = d.len();
        if n < 2 {
            return;
        }
        match r(7) {
            0 => {
                let i = r(n);
                d[i] ^= 1 << r(8);
            }
            1 => {
                let i = r(n);
                d[i] = 0xFF;
            }
            2 => {
                let (i, m) = (r(n), [0x00, 0xD0, 0xD3, 0xFF, 0xD9][r(5)]);
                d.splice(i..i, [0xFF, m]);
            }
            3 => {
                d.remove(r(n));
            }
            4 => {
                let i = r(n.min(1024));
                d[i] ^= 1 << r(8);
            }
            5 => {
                let i = n - 1 - r(n.min(12));
                d[i] ^= 1 << r(8);
            }
            _ => {
                let i = n - 1 - r(n.min(12));
                d.insert(i, r(256) as u8);
            }
        }
    }

    /// Damaged photos: whatever `encode` takes must either come back byte for byte
    /// or be told apart by `decode` (the engine compares before accepting). Nothing
    /// may panic. Damaged streams: `decode` must return an error or a wrong result,
    /// never panic. Returns (accepted, accepted but different).
    fn fuzz(photo: &[u8], rounds: usize, seed: u64) -> (usize, usize) {
        let (mut s, mut accepted, mut differ) = (seed, 0, 0);
        for i in 0..rounds {
            let mut d = photo.to_vec();
            for _ in 0..1 + (s >> 60) as usize % 3 {
                mutate(&mut d, &mut s);
            }
            if let Some((out, _)) = encode(&d) {
                accepted += 1;
                match decode(&out, d.len() as u64) {
                    Ok(back) if back == d => {}
                    _ => differ += 1,
                }
                if i % 4 == 0 {
                    let mut bad = out.clone();
                    for _ in 0..1 + (s >> 61) as usize % 4 {
                        mutate(&mut bad, &mut s);
                    }
                    let _ = decode(&bad, d.len() as u64);
                    let _ = decode(&bad, d.len() as u64 + 1);
                }
            }
        }
        (accepted, differ)
    }

    #[test]
    fn damaged_photos_and_streams_never_panic() {
        let dir = std::env::temp_dir().join(format!("arca-jpegrans-fuzz-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        if let Some(photo) = sips_jpeg(&dir, "a", 333, 211, 5, "85") {
            let (stored, _) = encode(&photo).expect("the clean photo encodes");
            assert_eq!(decode(&stored, photo.len() as u64).unwrap(), photo);
            let (accepted, differ) = fuzz(&photo, 3000, 1);
            assert!(accepted > 0);
            // a damaged photo that parses may still fail to come back exactly (padding
            // bits, restart numbering); the engine's verify step refuses those
            eprintln!("fuzz: {accepted} accepted, {differ} would be refused by verify");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Long run over real photos: `ARCA_FUZZ_DIR=<folder> ARCA_FUZZ_ROUNDS=<n per
    /// photo> cargo test --release -p arca-core jpegrans::tests::fuzz_folder -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn fuzz_folder() {
        let dir = std::env::var("ARCA_FUZZ_DIR").unwrap();
        let rounds: usize = std::env::var("ARCA_FUZZ_ROUNDS").ok().and_then(|v| v.parse().ok()).unwrap_or(200);
        let paths: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
        let (total, differ, exact, files): (usize, usize, usize, usize) = std::thread::scope(|sc| {
            let mut hs = Vec::new();
            for part in paths.chunks(paths.len().div_ceil(10)) {
                hs.push(sc.spawn(move || {
                    let (mut t, mut df, mut ex, mut n) = (0, 0, 0, 0);
                    for (i, p) in part.iter().enumerate() {
                        if let Ok(d) = std::fs::read(p) {
                            if let Some((out, _)) = encode(&d) {
                                n += 1;
                                match decode(&out, d.len() as u64) {
                                    Ok(b) if b == d => ex += 1,
                                    Ok(_) => eprintln!("  differs: {}", p.display()),
                                    Err(e) => eprintln!("  {e}: {}", p.display()),
                                }
                                // big photos take seconds a mutant: fewer rounds for them
                                let r = ((rounds as f64) * (300_000.0 / d.len() as f64).min(1.0)).ceil() as usize;
                                let (a, f) = fuzz(&d, r, i as u64 * 2 + 1);
                                t += a;
                                df += f;
                            } else if d.len() > 4 && d[0] == 0xFF && d[1] == 0xD8 {
                                let why = match Layout::parse(&d) {
                                    None => "layout".to_string(),
                                    Some(l) => match l.decode_scans(&d) {
                                        None => format!("scans (dri {}, {} scans, {} comps)", l.dri, l.scans.len(), l.comps.len()),
                                        Some(_) => "not smaller".to_string(),
                                    },
                                };
                                eprintln!("  declined ({why}): {}", p.display());
                            }
                        }
                    }
                    (t, df, ex, n)
                }));
            }
            hs.into_iter().map(|h| h.join().unwrap()).fold((0, 0, 0, 0), |a, b| (a.0 + b.0, a.1 + b.1, a.2 + b.2, a.3 + b.3))
        });
        eprintln!("{files} photos encoded, {exact} exact; {total} mutants accepted, {differ} of them would be refused by verify");
        assert_eq!(exact, files, "every clean photo must come back exactly");
    }
}
