//! Lossless coding of 8-bit pixels, for the rows inside PNG files.
//!
//! LZMA treats a PNG's filtered rows as bytes; a pixel model sees an image. On
//! photo-like pictures this codec is 15-20% smaller than LZMA at -9 and several
//! times faster both ways; on flat graphics with repeated areas LZMA still wins,
//! so the Deflate transform picks per image (see `deflate.rs`).
//!
//! Model: each channel is predicted with MED (the LOCO-I median edge
//! detector) from its west, north and north-west neighbours. Green is coded
//! first, red and blue as differences from green's residual, then alpha. A
//! whole-pixel "nothing changed" flag makes flat areas nearly free. Each
//! channel's residual becomes one of 16 tokens (zero, or sign and size class),
//! coded with adaptive 16-symbol tables chosen by local activity, the channel's
//! previous error and green's residual. The top remainder bit is modelled, the
//! rest are written raw. Tokens and flags share one rANS stream, so decoding a
//! symbol is a multiply and a 15-way compare instead of a chain of binary steps.
//!
//! Stream: `u32 rans_len | two rANS states (u32 each) and 16-bit words |
//! remainder bits`. Symbols alternate between the two states, so decoding runs
//! two independent chains at once.

use anyhow::{ensure, Result};

/// Activity buckets.
const ACT: usize = 12;
/// Second context: previous-error class (3) x green-residual class (4).
const C2: usize = 12;
const SCALE_BITS: u32 = 15;
const SCALE: u32 = 1 << SCALE_BITS;
const RANS_L: u32 = 1 << 16;
/// Final adaptation rate of the token tables (after the warm-up).
const RATE: u32 = 7;
/// Adaptation rate by the bit length of the symbols seen so far (see `rate`).
const RATE_BY_LEN: [u8; 17] = {
    let r = RATE as u8;
    [2, 2, 2, 3, 3, 4, 4, 5, 5, r, r, r, r, r, r, r, r]
};

/// Where a table moves after symbol `s`: entry i heads for i up to `s` and for
/// SCALE - 16 + i above it, offset by -2^14 to fit in i16.
static TARGETS: [[i16; 16]; 16] = {
    let mut t = [[0i16; 16]; 16];
    let mut s = 0;
    while s < 16 {
        let mut i = 0;
        while i < 16 {
            t[s][i] = (if i > s { SCALE as i32 - 16 + i as i32 } else { i as i32 } - 16384) as i16;
            i += 1;
        }
        s += 1;
    }
    t
};

/// Adaptive distribution over 16 symbols as a cumulative table (LZNA style):
/// after each symbol the table moves 1/2^rate of the way toward one that puts
/// almost everything on that symbol, so every symbol keeps a frequency >= 1.
#[derive(Clone)]
struct Cdf16 {
    c: [u16; 16],
    /// Symbols seen, for the warm-up: adapt fast at first, then settle.
    n: u16,
}

impl Cdf16 {
    fn new() -> Self {
        let mut c = [0u16; 16];
        for (i, v) in c.iter_mut().enumerate() {
            *v = (i as u32 * SCALE / 16) as u16;
        }
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

    /// The symbol whose interval holds `slot`: how many of entries 1..=15 are at
    /// or below it.
    #[inline(always)]
    fn find(&self, slot: u32) -> usize {
        #[cfg(target_arch = "aarch64")]
        // SAFETY: NEON is part of every aarch64 target; `c` is 16 u16s.
        unsafe {
            use std::arch::aarch64::*;
            let v = vdupq_n_u16(slot as u16);
            let a = vcleq_u16(vld1q_u16(self.c.as_ptr()), v);
            let b = vcleq_u16(vld1q_u16(self.c.as_ptr().add(8)), v);
            // Entry 0 is 0, always <= slot: subtract it.
            ((vaddvq_u16(vshrq_n_u16::<15>(a)) + vaddvq_u16(vshrq_n_u16::<15>(b))) - 1) as usize
        }
        #[cfg(not(target_arch = "aarch64"))]
        {
            let slot = slot as u16;
            (1..16).map(|i| (self.c[i] <= slot) as usize).sum()
        }
    }

    /// 2 for the first 4 symbols, 3 up to 16, 4 up to 64, 5 up to 256, then RATE.
    #[inline(always)]
    fn rate(&mut self) -> u32 {
        // Bit length of max(n, 3); n is a u16, so at most 16.
        let len = 32 - (self.n as u32 | 3).leading_zeros();
        self.n = self.n.saturating_add(1);
        RATE_BY_LEN[len as usize] as u32
    }

    /// Moves the table toward one with almost everything on `s`: targets are i
    /// up to `s` and SCALE - 16 + i above it, so every frequency stays >= 1.
    #[inline(always)]
    fn update(&mut self, s: usize) {
        let rate = self.rate();
        #[cfg(target_arch = "aarch64")]
        // SAFETY: NEON is part of every aarch64 target; `c` and each row of
        // TARGETS are 16 values. Values are offset by 2^14 so the arithmetic
        // fits in i16.
        unsafe {
            use std::arch::aarch64::*;
            let off = vdupq_n_s16(16384);
            let shift = vdupq_n_s16(-(rate as i16));
            let t = TARGETS[s & 15].as_ptr();
            for half in 0..2 {
                let target = vld1q_s16(t.add(half * 8));
                let ptr = self.c.as_mut_ptr().add(half * 8);
                let cur = vsubq_s16(vreinterpretq_s16_u16(vld1q_u16(ptr)), off);
                let next = vaddq_s16(cur, vshlq_s16(vsubq_s16(target, cur), shift));
                vst1q_u16(ptr, vreinterpretq_u16_s16(vaddq_s16(next, off)));
            }
        }
        #[cfg(not(target_arch = "aarch64"))]
        for i in 1..16 {
            let target = if i > s { (SCALE as i32 - 16 + i as i32 - 16384) as i16 } else { i as i16 - 16384 };
            let cur = (self.c[i] as i16).wrapping_sub(16384);
            self.c[i] = (cur + ((target - cur) >> rate)).wrapping_add(16384) as u16;
        }
    }
}

/// An adaptive binary probability (frequency of 0 out of SCALE). The update
/// keeps it within 31..=32737, so it is always a valid frequency.
#[derive(Clone, Copy)]
struct Bin(u16);

const HALF: Bin = Bin(SCALE as u16 / 2);

impl Bin {
    #[inline]
    fn f0(self) -> u32 {
        self.0 as u32
    }

    #[inline]
    fn update(&mut self, bit: bool) {
        let p = self.0 as i32;
        self.0 = if bit { (p - (p >> 5)) as u16 } else { (p + ((SCALE as i32 - p) >> 5)) as u16 };
    }
}

struct Models {
    tok: Box<[Cdf16; 4 * ACT * C2]>,
    zero: [Bin; 8],
    /// Top remainder bit, by (channel, size class, activity).
    top: Box<[Bin; 4 * 8 * ACT]>,
}

impl Models {
    fn new() -> Self {
        let tok: Vec<Cdf16> = vec![Cdf16::new(); 4 * ACT * C2];
        let top: Vec<Bin> = vec![HALF; 4 * 8 * ACT];
        Models { tok: tok.try_into().ok().unwrap(), zero: [HALF; 8], top: top.try_into().ok().unwrap() }
    }

    /// Token table for channel `c` (< 4), activity `act` (< ACT) and second
    /// context `c2` (< C2).
    #[inline(always)]
    fn tok(&mut self, c: usize, act: usize, c2: usize) -> &mut Cdf16 {
        &mut self.tok[tok_index(c, act, c2)]
    }

    #[inline(always)]
    fn top(&mut self, c: usize, k: u32, act: usize) -> &mut Bin {
        &mut self.top[top_index(c, k, act)]
    }
}

/// Size class of each token (see `tokenize`).
const CLASS: [u32; 16] = [0, 0, 1, 2, 3, 4, 5, 6, 0, 1, 2, 3, 4, 5, 6, 7];

/// Token for a residual in -128..=127: 0 zero, 1..=7 positive with
/// floor(log2 r) = token - 1, 8..=15 negative with floor(log2 -r) = token - 8.
/// Returns (token, size class k, remainder below the leading bit).
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
fn untokenize(tok: usize, rem: u32) -> i32 {
    let mag = (((tok != 0) as i32) << CLASS[tok & 15]) + rem as i32;
    if tok >= 8 { -mag } else { mag }
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

fn bucket(v: i32) -> u8 {
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

/// Class of green's residual: 0, 1..=2, 3..=8 or more.
#[inline]
fn gclass(g: i32) -> usize {
    let a = g.unsigned_abs();
    (a > 0) as usize + (a > 2) as usize + (a > 8) as usize
}

/// Coding order of the channels: green, red, blue, alpha (or gray, alpha).
#[inline(always)]
fn order<const CH: usize>() -> [usize; CH] {
    std::array::from_fn(|k| if CH >= 3 { [1, 0, 2, 3][k] } else { k })
}

/// The image with one zero row above and one zero column on each side, so the
/// neighbours of any pixel are plain reads.
struct Padded<const CH: usize> {
    buf: Vec<u8>,
    stride: usize,
}

impl<const CH: usize> Padded<CH> {
    fn new(w: usize, h: usize) -> Self {
        let stride = (w + 2) * CH;
        Padded { buf: vec![0; (h + 1) * stride], stride }
    }

    #[inline(always)]
    fn at(&self, x: usize, y: usize) -> usize {
        (y + 1) * self.stride + (x + 1) * CH
    }

    /// West, north, north-west and north-east neighbours of channel `c` at `i`.
    #[inline(always)]
    fn around(&self, i: usize, c: usize) -> (i32, i32, i32, i32) {
        let (b, s) = (&self.buf, self.stride);
        // `i` is an interior pixel of the padded image (from `at`), so its west,
        // north, north-west and north-east neighbours are all inside `buf`.
        debug_assert!(i >= s + CH && i + CH + c < b.len() && c < CH);
        // SAFETY: see above.
        unsafe {
            (
                *b.get_unchecked(i - CH + c) as i32,
                *b.get_unchecked(i - s + c) as i32,
                *b.get_unchecked(i - s - CH + c) as i32,
                *b.get_unchecked(i - s + CH + c) as i32,
            )
        }
    }

    /// MED prediction, activity bucket and previous-error class for one channel.
    #[inline(always)]
    fn context(&self, i: usize, c: usize, prev_err: i32, lut: &[u8; 1024]) -> (i32, usize, usize) {
        let (wv, nv, nw, ne) = self.around(i, c);
        let act = (wv - nw).abs() + (nv - nw).abs() + (nv - ne).abs() + prev_err;
        let ctx2 = (prev_err != 0) as usize + (prev_err >= 8) as usize;
        // act <= 3 * 255 + 64 < 1024.
        (med(wv, nv, nw), lut[act as usize & 1023] as usize, ctx2)
    }
}

fn act_lut() -> [u8; 1024] {
    let mut t = [0u8; 1024];
    for (i, v) in t.iter_mut().enumerate() {
        *v = bucket(i as i32);
    }
    t
}

#[inline]
fn tok_index(c: usize, act: usize, c2: usize) -> usize {
    (c * ACT + act) * C2 + c2
}

#[inline]
fn top_index(c: usize, k: u32, act: usize) -> usize {
    (c * 8 + k as usize) * ACT + act
}

/// Codes `px` (rows of `w` pixels with `ch` interleaved 8-bit channels, 1 to 4).
pub fn encode(px: &[u8], w: usize, h: usize, ch: usize) -> Vec<u8> {
    assert!((1..=4).contains(&ch) && px.len() == w * h * ch);
    match ch {
        1 => encode_ch::<1>(px, w, h),
        2 => encode_ch::<2>(px, w, h),
        3 => encode_ch::<3>(px, w, h),
        _ => encode_ch::<4>(px, w, h),
    }
}

fn encode_ch<const CH: usize>(px: &[u8], w: usize, h: usize) -> Vec<u8> {
    let lut = act_lut();
    let mut p = Padded::<CH>::new(w, h);
    for y in 0..h {
        let i = p.at(0, y);
        p.buf[i..i + w * CH].copy_from_slice(&px[y * w * CH..(y + 1) * w * CH]);
    }
    let mut m = Models::new();
    // Model every symbol in order, remembering its (start, frequency); rANS
    // then codes them last to first.
    let mut syms: Vec<(u16, u16)> = Vec::with_capacity(w * h + px.len() / 2);
    let mut bits = BitWriter::default();
    let mut prev = [0i32; 4];
    let mut zrow = vec![false; w + 2];
    let mut ctxs = [(0usize, 0usize); 4];
    let mut res = [0i32; 4];
    let bin = |f0: u32, bit: bool| if bit { (f0 as u16, (SCALE - f0) as u16) } else { (0, f0 as u16) };
    for y in 0..h {
        let mut zleft = false;
        for x in 0..w {
            let i = p.at(x, y);
            let mut g_res = 0;
            let mut all_zero = true;
            for c in order::<CH>() {
                let (pred, act, c2) = p.context(i, c, prev[c], &lut);
                let mut r = (p.buf[i + c] as i32 - pred) as i8 as i32;
                if CH >= 3 && c == 1 {
                    g_res = r;
                } else if CH >= 3 && (c == 0 || c == 2) {
                    r = (r - g_res) as i8 as i32;
                }
                ctxs[c] = (act, c2 + 3 * if CH >= 3 && c != 1 { gclass(g_res) } else { 0 });
                res[c] = r;
                all_zero &= r == 0;
            }
            let z = &mut m.zero[zleft as usize + 2 * zrow[x + 1] as usize + 4 * zrow[x + 2] as usize];
            syms.push(bin(z.f0(), !all_zero));
            z.update(!all_zero);
            zleft = all_zero;
            zrow[x + 1] = all_zero;
            for c in order::<CH>() {
                if !all_zero {
                    let (act, c2) = ctxs[c];
                    let model = m.tok(c, act, c2);
                    let (tok, k, rem) = tokenize(res[c]);
                    syms.push((model.lo(tok) as u16, (model.hi(tok) - model.lo(tok)) as u16));
                    model.update(tok);
                    // -128 (token 15 with k = 7) has no remainder.
                    if k > 0 && tok != 15 {
                        let top = (rem >> (k - 1)) & 1 == 1;
                        let b = m.top(c, k, act);
                        syms.push(bin(b.f0(), top));
                        b.update(top);
                        if k > 1 {
                            bits.put(rem & ((1 << (k - 1)) - 1), k - 1);
                        }
                    }
                }
                prev[c] = res[c].abs().min(64);
            }
        }
    }
    let mut words: Vec<u16> = Vec::with_capacity(syms.len() / 2);
    let mut xs: [u32; 2] = [RANS_L; 2];
    for (i, &(start, freq)) in syms.iter().enumerate().rev() {
        let x = &mut xs[i & 1];
        let (start, freq) = (start as u32, freq as u32);
        let x_max = ((RANS_L >> SCALE_BITS) << 16) * freq;
        while *x >= x_max {
            words.push(*x as u16);
            *x >>= 16;
        }
        *x = ((*x / freq) << SCALE_BITS) + (*x % freq) + start;
    }
    let rans_len = 8 + 2 * words.len();
    let rem = bits.finish();
    let mut out = Vec::with_capacity(4 + rans_len + rem.len());
    out.extend_from_slice(&(rans_len as u32).to_le_bytes());
    out.extend_from_slice(&xs[0].to_le_bytes());
    out.extend_from_slice(&xs[1].to_le_bytes());
    for wd in words.iter().rev() {
        out.extend_from_slice(&wd.to_le_bytes());
    }
    out.extend_from_slice(&rem);
    out
}

/// Refills a state that fell below RANS_L with the next 16-bit word. No branch:
/// decoding is bound by instruction count, and whether a refill is due is a
/// coin toss. Past the end of the data the words read as 0.
#[inline(always)]
fn renorm(x: &mut u32, rans: &[u8], rp: &mut usize) {
    let need = (*x < RANS_L) as u32;
    let w = match rans.get(*rp..*rp + 2) {
        Some(b) => u16::from_le_bytes([b[0], b[1]]) as u32,
        None => 0,
    };
    *x = (*x << (need * 16)) | (w & need.wrapping_neg());
    *rp += 2 * need as usize;
}

#[inline(always)]
fn decode_bin(x: &mut u32, rans: &[u8], rp: &mut usize, b: &mut Bin) -> bool {
    let f0 = b.f0();
    let slot = *x & (SCALE - 1);
    let bit = slot >= f0;
    *x = if bit { (SCALE - f0) * (*x >> SCALE_BITS) + slot - f0 } else { f0 * (*x >> SCALE_BITS) + slot };
    renorm(x, rans, rp);
    b.update(bit);
    bit
}

/// Inverse of `encode`. Malformed input gives an error or wrong pixels (which
/// the caller's hash check rejects), never a panic.
pub fn decode(src: &[u8], w: usize, h: usize, ch: usize) -> Result<Vec<u8>> {
    match ch {
        1 => decode_ch::<1>(src, w, h),
        2 => decode_ch::<2>(src, w, h),
        3 => decode_ch::<3>(src, w, h),
        4 => decode_ch::<4>(src, w, h),
        _ => anyhow::bail!("pixel data has {ch} channels"),
    }
}

/// One decoder per channel count, so the channel loops unroll: decoding speed
/// is set by the number of instructions per pixel.
fn decode_ch<const CH: usize>(src: &[u8], w: usize, h: usize) -> Result<Vec<u8>> {
    ensure!(src.len() >= 8, "pixel data truncated");
    let rans_len = u32::from_le_bytes(src[0..4].try_into().unwrap()) as usize;
    ensure!(rans_len >= 8 && 4 + rans_len <= src.len(), "pixel data truncated");
    let rans = &src[4..4 + rans_len];
    let mut bits = BitReader { src: &src[4 + rans_len..], pos: 0, acc: 0, n: 0 };
    let mut xs = [u32::from_le_bytes(rans[0..4].try_into().unwrap()), u32::from_le_bytes(rans[4..8].try_into().unwrap())];
    // Which state the next symbol uses.
    let mut si = 0usize;
    let mut rp = 8usize;
    let lut = act_lut();
    let mut m = Models::new();
    let mut p = Padded::<CH>::new(w, h);
    let mut prev = [0i32; 4];
    let mut zrow = vec![false; w + 2];
    for y in 0..h {
        let mut zleft = false;
        for x0 in 0..w {
            let i = p.at(x0, y);
            let z = &mut m.zero[zleft as usize + 2 * zrow[x0 + 1] as usize + 4 * zrow[x0 + 2] as usize];
            let any = decode_bin(&mut xs[si & 1], rans, &mut rp, z);
            si += 1;
            zleft = !any;
            zrow[x0 + 1] = !any;
            if !any {
                // Every residual is zero: each channel is its prediction.
                for c in order::<CH>() {
                    let (wv, nv, nw, _) = p.around(i, c);
                    p.buf[i + c] = med(wv, nv, nw) as u8;
                }
                prev = [0; 4];
                continue;
            }
            let mut g_res = 0;
            for c in order::<CH>() {
                let (pred, act, c2) = p.context(i, c, prev[c], &lut);
                let c2 = c2 + 3 * if CH >= 3 && c != 1 { gclass(g_res) } else { 0 };
                let model = m.tok(c, act, c2);
                let x = &mut xs[si & 1];
                si += 1;
                let slot = *x & (SCALE - 1);
                let tok = model.find(slot);
                let (lo, hi) = (model.lo(tok), model.hi(tok));
                *x = (hi - lo) * (*x >> SCALE_BITS) + slot - lo;
                renorm(x, rans, &mut rp);
                model.update(tok);
                let k = CLASS[tok & 15];
                let rem = if k > 0 && tok != 15 {
                    let top = decode_bin(&mut xs[si & 1], rans, &mut rp, m.top(c, k, act));
                    si += 1;
                    ((top as u32) << (k - 1)) | if k > 1 { bits.get(k - 1) } else { 0 }
                } else {
                    0
                };
                let coded = untokenize(tok, rem);
                prev[c] = (coded as i8 as i32).abs().min(64);
                let r = if CH >= 3 && (c == 0 || c == 2) { (coded + g_res) as i8 as i32 } else { coded };
                if CH >= 3 && c == 1 {
                    g_res = coded;
                }
                p.buf[i + c] = (pred + r) as u8;
            }
        }
    }
    let mut px = vec![0u8; w * h * CH];
    for y in 0..h {
        let i = p.at(0, y);
        px[y * w * CH..(y + 1) * w * CH].copy_from_slice(&p.buf[i..i + w * CH]);
    }
    Ok(px)
}

#[derive(Default)]
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
        if self.n < bits {
            if let Some(b) = self.src.get(self.pos..self.pos + 4) {
                self.acc |= (u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as u64) << self.n;
                self.pos += 4;
                self.n += 32;
            } else {
                while self.n < bits {
                    self.acc |= (self.src.get(self.pos).copied().unwrap_or(0) as u64) << self.n;
                    self.pos += 1;
                    self.n += 8;
                }
            }
        }
        let v = (self.acc & ((1u64 << bits) - 1)) as u32;
        self.acc >>= bits;
        self.n -= bits;
        v
    }
}

// ---------- PNG row filters ----------

#[inline(always)]
fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i16 + b as i16 - c as i16;
    let (pa, pb, pc) = ((p - a as i16).abs(), (p - b as i16).abs(), (p - c as i16).abs());
    // Written as selects so the loops below vectorise.
    let bc = if pb <= pc { b } else { c };
    if pa <= pb && pa <= pc { a } else { bc }
}

/// Splits PNG filtered rows (`h` rows of a type byte plus `w * ch` bytes) into
/// the row filter types and the pixels. `None` if a filter type is invalid.
pub fn unfilter(filtered: &[u8], w: usize, h: usize, ch: usize) -> Option<(Vec<u8>, Vec<u8>)> {
    let stride = w * ch;
    if filtered.len() != h * (stride + 1) {
        return None;
    }
    let mut px = vec![0u8; h * stride];
    let mut types = Vec::with_capacity(h);
    let zero = vec![0u8; stride];
    for y in 0..h {
        let src = &filtered[y * (stride + 1)..(y + 1) * (stride + 1)];
        let (t, src) = (src[0], &src[1..]);
        types.push(t);
        let (done, rest) = px.split_at_mut(y * stride);
        let up = if y > 0 { &done[(y - 1) * stride..] } else { &zero[..] };
        let row = &mut rest[..stride];
        // One loop per filter type; the first pixel has no left neighbour.
        let first = ch.min(stride);
        match t {
            0 => row.copy_from_slice(src),
            1 => {
                row[..first].copy_from_slice(&src[..first]);
                for x in ch..stride {
                    row[x] = src[x].wrapping_add(row[x - ch]);
                }
            }
            2 => {
                for x in 0..stride {
                    row[x] = src[x].wrapping_add(up[x]);
                }
            }
            3 => {
                for x in 0..first {
                    row[x] = src[x].wrapping_add(up[x] / 2);
                }
                for x in ch..stride {
                    row[x] = src[x].wrapping_add(((row[x - ch] as u16 + up[x] as u16) / 2) as u8);
                }
            }
            4 => {
                for x in 0..first {
                    row[x] = src[x].wrapping_add(up[x]);
                }
                for x in ch..stride {
                    row[x] = src[x].wrapping_add(paeth(row[x - ch], up[x], up[x - ch]));
                }
            }
            _ => return None,
        }
    }
    Some((types, px))
}

/// Inverse of `unfilter`: the exact filtered rows the PNG held.
pub fn refilter(types: &[u8], px: &[u8], w: usize, h: usize, ch: usize) -> Result<Vec<u8>> {
    let stride = w * ch;
    ensure!(types.len() == h && px.len() == h * stride, "row filter data does not match the image");
    let mut out = vec![0u8; h * (stride + 1)];
    let zero = vec![0u8; stride];
    for y in 0..h {
        let t = types[y];
        ensure!(t <= 4, "invalid PNG row filter {t}");
        let o = &mut out[y * (stride + 1)..(y + 1) * (stride + 1)];
        o[0] = t;
        let o = &mut o[1..];
        let row = &px[y * stride..(y + 1) * stride];
        let up = if y > 0 { &px[(y - 1) * stride..y * stride] } else { &zero[..] };
        let first = ch.min(stride);
        match t {
            0 => o.copy_from_slice(row),
            1 => {
                o[..first].copy_from_slice(&row[..first]);
                for x in ch..stride {
                    o[x] = row[x].wrapping_sub(row[x - ch]);
                }
            }
            2 => {
                for x in 0..stride {
                    o[x] = row[x].wrapping_sub(up[x]);
                }
            }
            3 => {
                for x in 0..first {
                    o[x] = row[x].wrapping_sub(up[x] / 2);
                }
                for x in ch..stride {
                    o[x] = row[x].wrapping_sub(((row[x - ch] as u16 + up[x] as u16) / 2) as u8);
                }
            }
            _ => {
                for x in 0..first {
                    o[x] = row[x].wrapping_sub(up[x]);
                }
                for x in ch..stride {
                    o[x] = row[x].wrapping_sub(paeth(row[x - ch], up[x], up[x - ch]));
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(w: usize, h: usize, ch: usize, seed: u64) -> Vec<u8> {
        let mut s = seed;
        (0..w * h * ch)
            .map(|i| {
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                // Smooth gradient plus a little noise, with flat stretches.
                let base = ((i / ch) % w + (i / ch) / w) as u64;
                if (i / ch / 7) % 5 == 0 { 200 } else { (base + (s & 7)) as u8 }
            })
            .collect()
    }

    #[test]
    fn round_trips() {
        for &(w, h, ch) in &[(1, 1, 1), (3, 2, 2), (17, 9, 3), (64, 33, 4), (5, 1, 4), (1, 7, 3)] {
            let px = image(w, h, ch, (w * 31 + h * 7 + ch) as u64);
            let coded = encode(&px, w, h, ch);
            assert_eq!(decode(&coded, w, h, ch).unwrap(), px, "{w}x{h}x{ch}");
        }
    }

    #[test]
    fn filters_round_trip() {
        let (w, h, ch) = (13, 6, 4);
        let px = image(w, h, ch, 9);
        let types: Vec<u8> = (0..h as u8).map(|y| y % 5).collect();
        let filtered = refilter(&types, &px, w, h, ch).unwrap();
        assert_eq!(unfilter(&filtered, w, h, ch).unwrap(), (types, px));
    }

    #[test]
    fn malformed_input_is_an_error_or_garbage_not_a_panic() {
        let px = image(20, 10, 4, 3);
        let coded = encode(&px, 20, 10, 4);
        for cut in [0, 3, 7, coded.len() / 2] {
            let _ = decode(&coded[..cut], 20, 10, 4);
        }
        let mut bad = coded.clone();
        bad[0] = 0xFF;
        assert!(decode(&bad, 20, 10, 4).is_err());
    }
}
