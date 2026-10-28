//! Research tool: how well does a small pixel codec do on PNG pixels, against
//! LZMA on the same filtered rows? `pixbench FILE_OR_DIR...`
//!
//! For each 8-bit, non-interlaced PNG: inflate the IDAT data (the filtered rows
//! Arca's Deflate transform stores today), undo the row filters, then code the
//! pixels with a predictor (MED) and an adaptive binary range coder, decode them
//! again and check. Prints per-file and total sizes and times.

use std::io::Read;
use std::time::Instant;

// ---------- PNG ----------

struct Png {
    w: usize,
    h: usize,
    ch: usize,
    filtered: Vec<u8>,
}

fn parse_png(d: &[u8]) -> Option<Png> {
    if d.len() < 8 || &d[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let (mut pos, mut idat, mut hdr) = (8usize, Vec::new(), None);
    while pos + 12 <= d.len() {
        let n = u32::from_be_bytes(d[pos..pos + 4].try_into().ok()?) as usize;
        let t = &d[pos + 4..pos + 8];
        let body = d.get(pos + 8..pos + 8 + n)?;
        match t {
            b"IHDR" => hdr = Some((u32::from_be_bytes(body[0..4].try_into().ok()?), u32::from_be_bytes(body[4..8].try_into().ok()?), body[8], body[9], body[12])),
            b"IDAT" => idat.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        pos += 12 + n;
    }
    let (w, h, depth, ctype, interlace) = hdr?;
    let ch = match ctype {
        0 => 1,
        2 => 3,
        4 => 2,
        6 => 4,
        _ => return None,
    };
    if depth != 8 || interlace != 0 {
        return None;
    }
    let mut filtered = Vec::new();
    flate2::read::ZlibDecoder::new(&idat[..]).read_to_end(&mut filtered).ok()?;
    if filtered.len() != h as usize * (1 + w as usize * ch) {
        return None;
    }
    Some(Png { w: w as usize, h: h as usize, ch, filtered })
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i16 + b as i16 - c as i16;
    let (pa, pb, pc) = ((p - a as i16).abs(), (p - b as i16).abs(), (p - c as i16).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// Undoes PNG row filters: returns (filter type per row, raw pixels).
fn unfilter(p: &Png) -> (Vec<u8>, Vec<u8>) {
    let stride = p.w * p.ch;
    let mut px = vec![0u8; p.h * stride];
    let mut types = Vec::with_capacity(p.h);
    for y in 0..p.h {
        let src = &p.filtered[y * (stride + 1)..(y + 1) * (stride + 1)];
        types.push(src[0]);
        for x in 0..stride {
            let a = if x >= p.ch { px[y * stride + x - p.ch] } else { 0 };
            let b = if y > 0 { px[(y - 1) * stride + x] } else { 0 };
            let c = if y > 0 && x >= p.ch { px[(y - 1) * stride + x - p.ch] } else { 0 };
            let v = src[1 + x];
            px[y * stride + x] = match src[0] {
                0 => v,
                1 => v.wrapping_add(a),
                2 => v.wrapping_add(b),
                3 => v.wrapping_add(((a as u16 + b as u16) / 2) as u8),
                _ => v.wrapping_add(paeth(a, b, c)),
            };
        }
    }
    (types, px)
}

// ---------- binary range coder (LZMA style, 12-bit probabilities) ----------

/// A probability (of a 0 bit, out of 65536) tracked at two speeds and averaged.
#[derive(Clone, Copy)]
struct P {
    fast: u16,
    slow: u16,
}

const HALF: P = P { fast: 1 << 15, slow: 1 << 15 };

impl P {
    #[inline]
    fn get(self) -> u32 {
        ((self.fast as u32 + self.slow as u32) >> 1).clamp(32, 65536 - 32)
    }
    #[inline]
    fn update(&mut self, bit: bool) {
        if bit {
            self.fast -= self.fast >> 4;
            self.slow -= self.slow >> 7;
        } else {
            self.fast += (65535 - self.fast) >> 4;
            self.slow += (65535 - self.slow) >> 7;
        }
    }
}

struct Enc {
    low: u64,
    range: u32,
    cache: u8,
    cache_size: u64,
    out: Vec<u8>,
}

impl Enc {
    fn new() -> Self {
        Enc { low: 0, range: u32::MAX, cache: 0, cache_size: 1, out: Vec::new() }
    }
    fn shift_low(&mut self) {
        if (self.low as u32) < 0xFF00_0000 || (self.low >> 32) != 0 {
            let carry = (self.low >> 32) as u8;
            let mut c = self.cache;
            loop {
                self.out.push(c.wrapping_add(carry));
                c = 0xFF;
                self.cache_size -= 1;
                if self.cache_size == 0 {
                    break;
                }
            }
            self.cache = (self.low >> 24) as u8;
        }
        self.cache_size += 1;
        self.low = (self.low & 0x00FF_FFFF) << 8;
    }
    #[inline]
    fn bit(&mut self, p: &mut P, b: bool) {
        let bound = (self.range >> 16) * p.get();
        if !b {
            self.range = bound;
        } else {
            self.low += bound as u64;
            self.range -= bound;
        }
        p.update(b);
        while self.range < (1 << 24) {
            self.range <<= 8;
            self.shift_low();
        }
    }
    fn finish(mut self) -> Vec<u8> {
        for _ in 0..5 {
            self.shift_low();
        }
        self.out
    }
}

struct Dec<'a> {
    code: u32,
    range: u32,
    src: &'a [u8],
    pos: usize,
}

impl<'a> Dec<'a> {
    fn new(src: &'a [u8]) -> Self {
        let mut d = Dec { code: 0, range: u32::MAX, src, pos: 1 };
        for _ in 0..4 {
            d.code = (d.code << 8) | d.next() as u32;
        }
        d
    }
    #[inline]
    fn next(&mut self) -> u8 {
        let b = self.src.get(self.pos).copied().unwrap_or(0);
        self.pos += 1;
        b
    }
    #[inline]
    fn bit(&mut self, p: &mut P) -> bool {
        let bound = (self.range >> 16) * p.get();
        let b = if self.code < bound {
            self.range = bound;
            false
        } else {
            self.code -= bound;
            self.range -= bound;
            true
        };
        p.update(b);
        while self.range < (1 << 24) {
            self.range <<= 8;
            self.code = (self.code << 8) | self.next() as u32;
        }
        b
    }
}

// ---------- pixel model ----------

const ACT: usize = 12;

/// Adaptive probabilities for one channel's residuals.
/// Second context: 0..3 from this channel's previous error, times 4 classes of
/// the size of green's residual in the same pixel (0 for green itself).
const C2: usize = 12;

struct Model {
    zero: [[P; C2]; ACT],
    sign: [[P; C2]; ACT],
    expo: [[[P; 8]; C2]; ACT],
    mant: [[[P; 8]; 8]; ACT],
}

impl Model {
    fn new() -> Box<Self> {
        let h = HALF;
        Box::new(Model { zero: [[h; C2]; ACT], sign: [[h; C2]; ACT], expo: [[[h; 8]; C2]; ACT], mant: [[[h; 8]; 8]; ACT] })
    }
}

#[inline]
fn gclass(g: i32) -> usize {
    match g.unsigned_abs() {
        0 => 0,
        1..=2 => 1,
        3..=8 => 2,
        _ => 3,
    }
}

/// Context for the whole-pixel "no residual at all" flag.
#[inline]
fn pixel_ctx(px: &[u8], w: usize, ch: usize, x: usize, y: usize, prev_zero: bool) -> usize {
    let s = w * ch;
    let same = |a: usize, b: usize| px[a..a + ch] == px[b..b + ch];
    let here = y * s + x * ch;
    let flat_w = x > 0 && y > 0 && same(here - ch, here - s - ch);
    let flat_n = y > 0 && x + 1 < w && x > 0 && same(here - s, here - s - ch);
    prev_zero as usize + 2 * flat_w as usize + 4 * flat_n as usize
}

#[inline]
fn med(a: i32, b: i32, c: i32) -> i32 {
    if c >= a.max(b) {
        a.min(b)
    } else if c <= a.min(b) {
        a.max(b)
    } else {
        a + b - c
    }
}

#[inline]
fn bucket(v: i32) -> usize {
    match v {
        0 => 0,
        1..=2 => 1,
        3..=4 => 2,
        5..=8 => 3,
        9..=14 => 4,
        15..=22 => 5,
        23..=34 => 6,
        35..=50 => 7,
        51..=80 => 8,
        81..=130 => 9,
        131..=220 => 10,
        _ => 11,
    }
}

/// Blended prediction: several simple predictors, each weighted by how small
/// its errors were at the neighbouring pixels (W, N, NW, NE) of the same channel.
const K: usize = 6;

struct Blend {
    w: usize,
    ch: usize,
    /// errs[(row & 1)][(x * ch + c) * K + k]
    errs: [Vec<u16>; 2],
    last: [[i32; K]; 4],
    med_only: bool,
}

impl Blend {
    fn new(w: usize, ch: usize) -> Self {
        Blend {
            w,
            ch,
            errs: [vec![0; w * ch * K], vec![0; w * ch * K]],
            last: [[0; K]; 4],
            med_only: std::env::var_os("PIX_MED").is_some(),
        }
    }

    #[inline]
    fn predict(&mut self, px: &[u8], x: usize, y: usize, c: usize, prev_err: i32) -> (i32, usize, usize) {
        if self.med_only {
            return predict(px, self.w, self.ch, x, y, c, prev_err);
        }
        let (w, ch) = (self.w, self.ch);
        let s = w * ch;
        let at = |xx: usize, yy: usize| px[yy * s + xx * ch + c] as i32;
        let wv = if x > 0 { at(x - 1, y) } else if y > 0 { at(x, y - 1) } else { 0 };
        let nv = if y > 0 { at(x, y - 1) } else { wv };
        let nw = if x > 0 && y > 0 { at(x - 1, y - 1) } else { nv };
        let ne = if y > 0 && x + 1 < w { at(x + 1, y - 1) } else { nv };
        let p = [wv, nv, ne, (wv + nv - nw).clamp(0, 255), med(wv, nv, nw), (wv + ne + 1) >> 1];
        let cur = &self.errs[y & 1];
        let up = &self.errs[(y + 1) & 1];
        let e = |row: &Vec<u16>, xx: usize, k: usize| row[(xx * ch + c) * K + k] as u32;
        let mut num: u64 = 0;
        let mut den: u64 = 0;
        let mut exp: u64 = 0;
        for k in 0..K {
            let mut cost = 0u32;
            if x > 0 {
                cost += e(cur, x - 1, k);
            }
            if y > 0 {
                cost += e(up, x, k);
                if x > 0 {
                    cost += e(up, x - 1, k);
                }
                if x + 1 < w {
                    cost += e(up, x + 1, k);
                }
            }
            let wt = (1u64 << 32) / ((1 + cost as u64) * (1 + cost as u64));
            num += wt * p[k] as u64;
            den += wt;
            exp += wt * cost as u64;
        }
        self.last[c] = p;
        let pred = ((num + den / 2) / den) as i32;
        let act = (exp / den) as i32 + prev_err;
        let ctx2 = if prev_err == 0 { 0 } else if prev_err < 8 { 1 } else { 2 };
        (pred, bucket(act), ctx2)
    }

    /// Records each predictor's error once the true value is known.
    #[inline]
    fn update(&mut self, x: usize, y: usize, c: usize, actual: i32) {
        if self.med_only {
            return;
        }
        let p = self.last[c];
        let row = &mut self.errs[y & 1];
        for k in 0..K {
            row[(x * self.ch + c) * K + k] = (p[k] - actual).unsigned_abs().min(255) as u16;
        }
    }
}

/// Prediction and context for channel `c` of pixel (x, y), given decoded pixels.
#[inline]
fn predict(px: &[u8], w: usize, ch: usize, x: usize, y: usize, c: usize, prev_err: i32) -> (i32, usize, usize) {
    let s = w * ch;
    let at = |xx: usize, yy: usize| px[yy * s + xx * ch + c] as i32;
    let wv = if x > 0 { at(x - 1, y) } else if y > 0 { at(x, y - 1) } else { 0 };
    let nv = if y > 0 { at(x, y - 1) } else { wv };
    let nw = if x > 0 && y > 0 { at(x - 1, y - 1) } else { nv };
    let ne = if y > 0 && x + 1 < w { at(x + 1, y - 1) } else { nv };
    let act = (wv - nw).abs() + (nv - nw).abs() + (nv - ne).abs() + prev_err;
    let ctx2 = if prev_err == 0 { 0 } else if prev_err < 8 { 1 } else { 2 };
    (med(wv, nv, nw), bucket(act), ctx2)
}

fn encode_residual(e: &mut Enc, m: &mut Model, act: usize, c2: usize, r: i32) {
    // r in -255..=255 after inter-channel differencing; wrap to a signed byte.
    let r = r as i8 as i32;
    e.bit(&mut m.zero[act][c2], r != 0);
    if r == 0 {
        return;
    }
    e.bit(&mut m.sign[act][c2], r < 0);
    let u = r.unsigned_abs() - 1; // 0..=127 (128 for -128)
    let k = 32 - (u + 1).leading_zeros() - 1; // floor(log2(u+1)), 0..=7
    for i in 0..k {
        e.bit(&mut m.expo[act][c2][i as usize], true);
    }
    if k < 7 {
        e.bit(&mut m.expo[act][c2][k as usize], false);
    }
    let v = u + 1 - (1 << k);
    for i in (0..k).rev() {
        e.bit(&mut m.mant[act][k as usize][i as usize], (v >> i) & 1 == 1);
    }
}

fn decode_residual(d: &mut Dec, m: &mut Model, act: usize, c2: usize) -> i32 {
    if !d.bit(&mut m.zero[act][c2]) {
        return 0;
    }
    let neg = d.bit(&mut m.sign[act][c2]);
    let mut k = 0u32;
    while k < 7 && d.bit(&mut m.expo[act][c2][k as usize]) {
        k += 1;
    }
    let mut v = 0u32;
    for i in (0..k).rev() {
        v |= (d.bit(&mut m.mant[act][k as usize][i as usize]) as u32) << i;
    }
    let u = (v + (1 << k) - 1) as i32 + 1;
    if neg { -u } else { u }
}

/// Channel order: green first, then red and blue as differences from green's
/// residual, then alpha.
fn order(ch: usize) -> &'static [usize] {
    match ch {
        3 | 4 => &[1, 0, 2, 3][..ch],
        _ => &[0, 1][..ch],
    }
}

fn encode(px: &[u8], w: usize, h: usize, ch: usize) -> Vec<u8> {
    let mut e = Enc::new();
    let mut models: Vec<Box<Model>> = (0..ch).map(|_| Model::new()).collect();
    let mut pz = [HALF; 8];
    let mut prev = vec![0i32; ch];
    let mut prev_zero = false;
    let mut preds = [(0i32, 0usize, 0usize); 4];
    let mut res = [0i32; 4];
    let mut blend = Blend::new(w, ch);
    for y in 0..h {
        for x in 0..w {
            let mut g_res = 0;
            let mut all_zero = true;
            for &c in order(ch) {
                let (pred, act, c2) = blend.predict(px, x, y, c, prev[c]);
                let actual = px[(y * w + x) * ch + c] as i32;
                blend.update(x, y, c, actual);
                let mut r = (actual - pred) as i8 as i32;
                if c == 1 && ch >= 3 {
                    g_res = r;
                } else if (c == 0 || c == 2) && ch >= 3 {
                    r = (r - g_res) as i8 as i32;
                }
                preds[c] = (pred, act, c2 + 3 * if c == 1 { 0 } else { gclass(g_res) });
                res[c] = r;
                all_zero &= r == 0;
            }
            e.bit(&mut pz[pixel_ctx(px, w, ch, x, y, prev_zero)], !all_zero);
            prev_zero = all_zero;
            for &c in order(ch) {
                if !all_zero {
                    let (_, act, c2) = preds[c];
                    encode_residual(&mut e, &mut models[c], act, c2, res[c]);
                }
                prev[c] = res[c].abs().min(64);
            }
        }
    }
    e.finish()
}

fn decode(src: &[u8], w: usize, h: usize, ch: usize) -> Vec<u8> {
    let mut d = Dec::new(src);
    let mut models: Vec<Box<Model>> = (0..ch).map(|_| Model::new()).collect();
    let mut pz = [HALF; 8];
    let mut px = vec![0u8; w * h * ch];
    let mut prev = vec![0i32; ch];
    let mut prev_zero = false;
    let mut blend = Blend::new(w, ch);
    for y in 0..h {
        for x in 0..w {
            let any = d.bit(&mut pz[pixel_ctx(&px, w, ch, x, y, prev_zero)]);
            prev_zero = !any;
            let mut g_res = 0;
            for &c in order(ch) {
                let (pred, act, c2) = blend.predict(&px, x, y, c, prev[c]);
                let c2 = c2 + 3 * if c == 1 { 0 } else { gclass(g_res) };
                let coded = if any { decode_residual(&mut d, &mut models[c], act, c2) } else { 0 };
                prev[c] = (coded as i8 as i32).abs().min(64);
                let mut r = coded;
                if c == 1 && ch >= 3 {
                    g_res = coded;
                } else if (c == 0 || c == 2) && ch >= 3 {
                    r = (coded + g_res) as i8 as i32;
                }
                px[(y * w + x) * ch + c] = (pred + r) as u8;
                blend.update(x, y, c, px[(y * w + x) * ch + c] as i32);
            }
        }
    }
    px
}


// ---------- v4: adaptive 16-symbol models + rANS (fast decoding) ----------

const SCALE_BITS: u32 = 15;
const SCALE: u32 = 1 << SCALE_BITS;
const RANS_L: u32 = 1 << 16;

/// Adaptive distribution over 16 symbols as a cumulative table (LZNA style):
/// after each symbol the table moves 1/2^RATE of the way toward one that puts
/// almost everything on that symbol. Every symbol keeps a frequency of at least 1.
#[derive(Clone)]
struct Cdf16 {
    c: [u16; 17],
    /// Symbols seen, for the warm-up: adapt fast at first, then settle.
    n: u16,
}

const RATE: u32 = 7;

impl Cdf16 {
    fn new() -> Self {
        let mut c = [0u16; 17];
        for (i, v) in c.iter_mut().enumerate() {
            *v = (i as u32 * SCALE / 16) as u16;
        }
        c[16] = SCALE as u16 - 1; // stored as SCALE-1 to fit; see `hi`
        Cdf16 { c, n: 0 }
    }
    #[inline]
    fn lo(&self, s: usize) -> u32 {
        self.c[s] as u32
    }
    #[inline]
    fn hi(&self, s: usize) -> u32 {
        if s == 15 { SCALE } else { self.c[s + 1] as u32 }
    }
    /// Branch-free: counts the entries 1..=15 at or below `slot` (vectorises).
    #[inline]
    fn find(&self, slot: u32) -> usize {
        let slot = slot as u16;
        let mut n = 0u16;
        for i in 1..16 {
            n += (self.c[i] <= slot) as u16;
        }
        n as usize
    }
    #[inline]
    fn update(&mut self, s: usize) {
        let rate = match self.n {
            0..=3 => 2,
            4..=15 => 3,
            16..=63 => 4,
            64..=255 => 5,
            _ => RATE,
        };
        self.n = self.n.saturating_add(1);
        let s = s as i16;
        let mut c: [i16; 16] = [0; 16];
        for i in 0..16 {
            c[i] = self.c[i] as i16; // SCALE <= 32768 fits once offset below
        }
        for i in 1..16 {
            let ii = i as i16;
            // Targets: i for i <= s, SCALE - 16 + i above s. Work relative to
            // 16384 so everything fits in i16.
            let target = if ii > s { (SCALE as i32 - 16 + i as i32 - 16384) as i16 } else { ii - 16384 };
            let cur = c[i].wrapping_sub(16384);
            c[i] = (cur + ((target - cur) >> rate)).wrapping_add(16384);
        }
        for i in 1..16 {
            self.c[i] = c[i] as u16;
        }
    }
}

/// A binary probability for rANS (frequency of 0 out of SCALE).
#[derive(Clone, Copy)]
struct Bin(u16);

impl Bin {
    #[inline]
    fn f0(self) -> u32 {
        (self.0 as u32).clamp(1, SCALE - 1)
    }
    #[inline]
    fn update(&mut self, bit: bool) {
        let p = self.0 as i32;
        self.0 = if bit { (p - (p >> 5)) as u16 } else { (p + ((SCALE as i32 - p) >> 5)) as u16 };
    }
}

/// Token for a residual in -128..=127: 0 zero, 1..=7 positive with
/// floor(log2 r) = token-1, 8..=15 negative with floor(log2 -r) = token-8.
#[inline]
fn tokenize(r: i32) -> (usize, u32, u32) {
    if r == 0 {
        return (0, 0, 0);
    }
    let a = r.unsigned_abs();
    let k = 31 - a.leading_zeros();
    let tok = if r > 0 { 1 + k as usize } else { 8 + k as usize };
    (tok, k, a - (1 << k))
}

#[inline]
fn untokenize(tok: usize, mant: u32) -> i32 {
    if tok == 0 {
        0
    } else if tok < 8 {
        ((1u32 << (tok - 1)) + mant) as i32
    } else {
        -(((1u32 << (tok - 8)) + mant) as i32)
    }
}

struct Models4 {
    tok: Vec<Cdf16>,
    zero: [Bin; 8],
    /// Top remainder bit, by (channel, size class, activity).
    top: Vec<Bin>,
}

impl Models4 {
    fn new(ch: usize) -> Self {
        Models4 { tok: vec![Cdf16::new(); ch * ACT * C2], zero: [Bin(SCALE as u16 / 2); 8], top: vec![Bin(SCALE as u16 / 2); ch * 8 * ACT] }
    }
}

#[inline]
fn tctx(c: usize, act: usize, c2: usize) -> usize {
    (c * ACT + act) * C2 + c2
}

struct BitWriter {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl BitWriter {
    #[inline]
    fn put(&mut self, v: u32, bits: u32) {
        self.acc |= (v as u64) << self.n;
        self.n += bits;
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

struct BitReader<'a> {
    src: &'a [u8],
    pos: usize,
    acc: u64,
    n: u32,
}

impl BitReader<'_> {
    #[inline]
    fn get(&mut self, bits: u32) -> u32 {
        while self.n < bits {
            self.acc |= (self.src.get(self.pos).copied().unwrap_or(0) as u64) << self.n;
            self.pos += 1;
            self.n += 8;
        }
        let v = (self.acc & ((1u64 << bits) - 1)) as u32;
        self.acc >>= bits;
        self.n -= bits;
        v
    }
}

/// Activity bucket for every activity value the coder can see.
fn act_lut() -> Vec<u8> {
    (0..1024).map(|v| bucket(v) as u8).collect()
}

/// Image with one zero row above and one zero column on each side, so the four
/// neighbours of any pixel are plain reads.
struct Padded {
    buf: Vec<u8>,
    /// Bytes per padded row.
    stride: usize,
    ch: usize,
}

impl Padded {
    fn new(w: usize, h: usize, ch: usize) -> Self {
        let stride = (w + 2) * ch;
        Padded { buf: vec![0; (h + 1) * stride], stride, ch }
    }
    #[inline(always)]
    fn at(&self, x: usize, y: usize) -> usize {
        (y + 1) * self.stride + (x + 1) * self.ch
    }
}

/// MED prediction, activity bucket and previous-error class for one channel.
#[inline(always)]
fn ctx4(p: &Padded, i: usize, c: usize, prev_err: i32, lut: &[u8]) -> (i32, usize, usize) {
    let b = &p.buf;
    let (s, ch) = (p.stride, p.ch);
    let wv = b[i - ch + c] as i32;
    let nv = b[i - s + c] as i32;
    let nw = b[i - s - ch + c] as i32;
    let ne = b[i - s + ch + c] as i32;
    let act = (wv - nw).abs() + (nv - nw).abs() + (nv - ne).abs() + prev_err;
    let ctx2 = if prev_err == 0 { 0 } else if prev_err < 8 { 1 } else { 2 };
    (med(wv, nv, nw), lut[act as usize] as usize, ctx2)
}

fn encode4(px: &[u8], w: usize, h: usize, ch: usize) -> Vec<u8> {
    let lut = act_lut();
    let mut p = Padded::new(w, h, ch);
    for y in 0..h {
        let i = p.at(0, y);
        p.buf[i..i + w * ch].copy_from_slice(&px[y * w * ch..(y + 1) * w * ch]);
    }
    let mut m = Models4::new(ch);
    // Forward pass: model the symbols, remembering each one's (start, freq).
    let mut syms: Vec<(u16, u16)> = Vec::with_capacity(w * h * (ch + 1));
    let mut bits = BitWriter { out: Vec::new(), acc: 0, n: 0 };
    let mut prev = [0i32; 4];
    let mut zrow = vec![false; w + 2];
    let mut ctxs = [(0usize, 0usize); 4];
    let mut res = [0i32; 4];
    for y in 0..h {
        let mut zleft = false;
        for x in 0..w {
            let i = p.at(x, y);
            let mut g_res = 0;
            let mut all_zero = true;
            for &c in order(ch) {
                let (pred, act, c2) = ctx4(&p, i, c, prev[c], &lut);
                let mut r = (p.buf[i + c] as i32 - pred) as i8 as i32;
                if c == 1 && ch >= 3 {
                    g_res = r;
                } else if (c == 0 || c == 2) && ch >= 3 {
                    r = (r - g_res) as i8 as i32;
                }
                ctxs[c] = (act, c2 + 3 * if c == 1 { 0 } else { gclass(g_res) });
                res[c] = r;
                all_zero &= r == 0;
            }
            let zc = zleft as usize + 2 * zrow[x + 1] as usize + 4 * zrow[x + 2] as usize;
            let z = &mut m.zero[zc];
            let f0 = z.f0();
            syms.push(if all_zero { (0, f0 as u16) } else { (f0 as u16, (SCALE - f0) as u16) });
            z.update(!all_zero);
            zleft = all_zero;
            zrow[x + 1] = all_zero;
            for &c in order(ch) {
                if !all_zero {
                    let (act, c2) = ctxs[c];
                    let model = &mut m.tok[tctx(c, act, c2)];
                    let (tok, k, mant) = tokenize(res[c]);
                    syms.push((model.lo(tok) as u16, (model.hi(tok) - model.lo(tok)) as u16));
                    model.update(tok);
                    if k > 0 && tok != 15 {
                        let topbit = (mant >> (k - 1)) & 1 == 1;
                        let b = &mut m.top[(c * 8 + k as usize) * ACT + act];
                        let f0 = b.f0();
                        syms.push(if topbit { (f0 as u16, (SCALE - f0) as u16) } else { (0, f0 as u16) });
                        b.update(topbit);
                        if k > 1 {
                            bits.put(mant & ((1 << (k - 1)) - 1), k - 1);
                        }
                    }
                }
                prev[c] = res[c].abs().min(64);
            }
        }
    }
    // rANS, last symbol first.
    let mut words: Vec<u16> = Vec::with_capacity(syms.len() / 2);
    let mut x: u32 = RANS_L;
    for &(start, freq) in syms.iter().rev() {
        let (start, freq) = (start as u32, freq as u32);
        let x_max = ((RANS_L >> SCALE_BITS) << 16) * freq;
        while x >= x_max {
            words.push(x as u16);
            x >>= 16;
        }
        x = ((x / freq) << SCALE_BITS) + (x % freq) + start;
    }
    let mut out = Vec::with_capacity(8 + words.len() * 2);
    out.extend_from_slice(&x.to_le_bytes());
    for wd in words.iter().rev() {
        out.extend_from_slice(&wd.to_le_bytes());
    }
    let mant = bits.finish();
    let mut framed = (out.len() as u32).to_le_bytes().to_vec();
    framed.extend_from_slice(&out);
    framed.extend_from_slice(&mant);
    framed
}

fn decode4(src: &[u8], w: usize, h: usize, ch: usize) -> Vec<u8> {
    let lut = act_lut();
    let rlen = u32::from_le_bytes(src[0..4].try_into().unwrap()) as usize;
    let rans = &src[4..4 + rlen];
    let mut bits = BitReader { src: &src[4 + rlen..], pos: 0, acc: 0, n: 0 };
    let mut x = u32::from_le_bytes(rans[0..4].try_into().unwrap());
    let mut rp = 4usize;
    let mut m = Models4::new(ch);
    let mut p = Padded::new(w, h, ch);
    let mut prev = [0i32; 4];
    let mut zrow = vec![false; w + 2];
    macro_rules! renorm {
        () => {
            if x < RANS_L {
                let wd = u16::from_le_bytes([rans.get(rp).copied().unwrap_or(0), rans.get(rp + 1).copied().unwrap_or(0)]);
                rp += 2;
                x = (x << 16) | wd as u32;
            }
        };
    }
    for y in 0..h {
        let mut zleft = false;
        for x0 in 0..w {
            let i = p.at(x0, y);
            let zc = zleft as usize + 2 * zrow[x0 + 1] as usize + 4 * zrow[x0 + 2] as usize;
            let z = &mut m.zero[zc];
            let f0 = z.f0();
            let slot = x & (SCALE - 1);
            let any = slot >= f0;
            x = if any { (SCALE - f0) * (x >> SCALE_BITS) + slot - f0 } else { f0 * (x >> SCALE_BITS) + slot };
            renorm!();
            z.update(any);
            zleft = !any;
            zrow[x0 + 1] = !any;
            let mut g_res = 0;
            for &c in order(ch) {
                let (pred, act, c2) = ctx4(&p, i, c, prev[c], &lut);
                let c2 = c2 + 3 * if c == 1 { 0 } else { gclass(g_res) };
                let coded = if any {
                    let model = &mut m.tok[tctx(c, act, c2)];
                    let slot = x & (SCALE - 1);
                    let tok = model.find(slot);
                    let (lo, hi) = (model.lo(tok), model.hi(tok));
                    x = (hi - lo) * (x >> SCALE_BITS) + slot - lo;
                    renorm!();
                    model.update(tok);
                    let k = if tok == 0 { 0 } else if tok < 8 { tok as u32 - 1 } else { tok as u32 - 8 };
                    let mant = if k > 0 && tok != 15 {
                        let b = &mut m.top[(c * 8 + k as usize) * ACT + act];
                        let f0 = b.f0();
                        let slot = x & (SCALE - 1);
                        let topbit = slot >= f0;
                        x = if topbit { (SCALE - f0) * (x >> SCALE_BITS) + slot - f0 } else { f0 * (x >> SCALE_BITS) + slot };
                        renorm!();
                        b.update(topbit);
                        ((topbit as u32) << (k - 1)) | if k > 1 { bits.get(k - 1) } else { 0 }
                    } else {
                        0
                    };
                    untokenize(tok, mant)
                } else {
                    0
                };
                prev[c] = (coded as i8 as i32).abs().min(64);
                let mut r = coded;
                if c == 1 && ch >= 3 {
                    g_res = coded;
                } else if (c == 0 || c == 2) && ch >= 3 {
                    r = (coded + g_res) as i8 as i32;
                }
                p.buf[i + c] = (pred + r) as u8;
            }
        }
    }
    let mut px = vec![0u8; w * h * ch];
    for y in 0..h {
        let i = p.at(0, y);
        px[y * w * ch..(y + 1) * w * ch].copy_from_slice(&p.buf[i..i + w * ch]);
    }
    px
}

fn main() {
    let mut files = Vec::new();
    for a in std::env::args().skip(1) {
        let p = std::path::PathBuf::from(&a);
        if p.is_dir() {
            for e in std::fs::read_dir(&p).unwrap() {
                files.push(e.unwrap().path());
            }
        } else {
            files.push(p);
        }
    }
    files.sort();
    use rayon::prelude::*;
    let rows: Vec<_> = files
        .par_iter()
        .filter_map(|f| {
            let d = std::fs::read(f).ok()?;
            let png = parse_png(&d)?;
            let (_types, px) = unfilter(&png);
            let t = Instant::now();
            let v4 = std::env::var_os("PIX_V4").is_some();
            let engine = std::env::var_os("PIX_ENGINE").is_some();
            let ours = if engine {
                arca_core::transform::pixels::encode(&px, png.w, png.h, png.ch)
            } else if v4 {
                encode4(&px, png.w, png.h, png.ch)
            } else {
                encode(&px, png.w, png.h, png.ch)
            };
            let te = t.elapsed().as_secs_f64();
            let t = Instant::now();
            let reps: usize = std::env::var("PIX_DEC_REPS").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
            let mut back = Vec::new();
            for _ in 0..reps {
                back = if engine {
                    arca_core::transform::pixels::decode(&ours, png.w, png.h, png.ch).unwrap()
                } else if v4 {
                    decode4(&ours, png.w, png.h, png.ch)
                } else {
                    decode(&ours, png.w, png.h, png.ch)
                };
            }
            let td = t.elapsed().as_secs_f64();
            assert!(back == px, "round trip failed: {}", f.display());
            let t = Instant::now();
            let xz = if std::env::var_os("PIX_NOXZ").is_some() {
                0
            } else {
                let mut xz = liblzma::write::XzEncoder::new(Vec::new(), 9);
                std::io::Write::write_all(&mut xz, &png.filtered).unwrap();
                xz.finish().unwrap().len()
            };
            let tx = t.elapsed().as_secs_f64();
            let z1 = zstd::bulk::compress(&png.filtered, 1).unwrap().len();
            Some((f.file_name().unwrap().to_string_lossy().into_owned(), d.len(), px.len(), ours.len(), xz, te, td, tx, z1))
        })
        .collect();
    let mut tot = (0usize, 0usize, 0usize, 0usize, 0f64, 0f64, 0f64);
    for (name, size, raw, ours, xz, te, td, tx, z1) in &rows {
        if std::env::var_os("PIX_VERBOSE").is_some() {
            println!("{name}\t{size}\t{raw}\t{ours}\t{xz}\t{:.3}\t{:.3}\t{:.3}\t{z1}", te, td, tx);
        }
        tot.0 += size;
        tot.1 += raw;
        tot.2 += ours;
        tot.3 += xz;
        tot.4 += te;
        tot.5 += td;
        tot.6 += tx;
    }
    println!(
        "{} PNGs: file {:.1} MB, pixels {:.1} MB | pixel codec {:.2} MB (enc {:.1} s, dec {:.1} s) | xz-9 on filtered rows {:.2} MB ({:.1} s)",
        rows.len(),
        tot.0 as f64 / 1e6,
        tot.1 as f64 / 1e6,
        tot.2 as f64 / 1e6,
        tot.4,
        tot.5,
        tot.3 as f64 / 1e6,
        tot.6
    );
}
