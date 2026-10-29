//! A real coder for model B: adaptive binary range coding (LZMA-style), one
//! function drives both directions so encoder and decoder can't drift apart.

pub trait Rc {
    const ENC: bool;
    fn bit(&mut self, p: &mut u16, b: bool) -> bool;
    fn direct(&mut self, v: u32, n: u32) -> u32;
}

const SHIFT: u32 = 5;
const TOP: u32 = 1 << 24;

pub struct Enc {
    low: u64,
    range: u32,
    cache: u8,
    cache_size: u64,
    pub out: Vec<u8>,
}

impl Enc {
    pub fn new() -> Enc {
        Enc { low: 0, range: 0xFFFF_FFFF, cache: 0, cache_size: 1, out: Vec::new() }
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
    pub fn finish(mut self) -> Vec<u8> {
        for _ in 0..5 {
            self.shift_low();
        }
        self.out
    }
}

impl Rc for Enc {
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
    #[inline(always)]
    fn direct(&mut self, v: u32, n: u32) -> u32 {
        for i in (0..n).rev() {
            self.range >>= 1;
            if (v >> i) & 1 == 1 {
                self.low += self.range as u64;
            }
            while self.range < TOP {
                self.range <<= 8;
                self.shift_low();
            }
        }
        v
    }
}

pub struct Dec<'a> {
    range: u32,
    code: u32,
    d: &'a [u8],
    pos: usize,
}

impl<'a> Dec<'a> {
    pub fn new(d: &'a [u8]) -> Dec<'a> {
        let mut x = Dec { range: 0xFFFF_FFFF, code: 0, d, pos: 0 };
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

impl<'a> Rc for Dec<'a> {
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
    #[inline(always)]
    fn direct(&mut self, _v: u32, n: u32) -> u32 {
        let mut v = 0;
        for _ in 0..n {
            self.range >>= 1;
            let b = (self.code >= self.range) as u32;
            self.code -= self.range & 0u32.wrapping_sub(b);
            v = (v << 1) | b;
            if self.range < TOP {
                self.range <<= 8;
                self.code = (self.code << 8) | self.byte() as u32;
            }
        }
        v
    }
}

// ---------- the model ----------

const NB: usize = 8; // neighbour magnitude buckets
const LB: usize = 8; // remaining-nonzero buckets

/// Probabilities for one signed value: zero flag, 15 exponent steps, 16 top-mantissa, 6 signs.
#[derive(Clone)]
struct Sv([u16; 38]);
impl Default for Sv {
    fn default() -> Self {
        Sv([2048; 38])
    }
}

pub struct Model {
    nz: Vec<u16>,  // [cc][nctx 16][node 64]
    ac: Vec<Sv>,   // [cc][k 64][nb][lb]
    dc: Vec<Sv>,   // [cc][act 11][nbk 8]
}

impl Model {
    pub fn new() -> Model {
        Model { nz: vec![2048; 2 * 16 * 64], ac: vec![Sv::default(); 2 * 64 * NB * LB], dc: vec![Sv::default(); 2 * 11 * 8] }
    }
}

#[inline(always)]
fn bucket(x: u32) -> usize {
    if x < 3 { x as usize } else { 1 + (32 - (x - 1).leading_zeros()) as usize }
}

#[inline(always)]
fn signed<C: Rc>(c: &mut C, s: &mut Sv, v: i32, sign_ctx: usize) -> i32 {
    let nz = c.bit(&mut s.0[0], v != 0);
    if !nz {
        return 0;
    }
    let a = v.unsigned_abs();
    let e_enc = if C::ENC { 32 - a.leading_zeros() } else { 0 };
    // exponent, unary from 1
    let mut e = 1u32;
    while e < 16 {
        if !c.bit(&mut s.0[e as usize], C::ENC && e < e_enc) {
            break;
        }
        e += 1;
    }
    let mut m = 1u32;
    if e >= 2 {
        let top = c.bit(&mut s.0[15 + e as usize], C::ENC && (a >> (e - 2)) & 1 == 1) as u32;
        m = (m << 1) | top;
        if e >= 3 {
            let rest = c.direct(a & ((1 << (e - 2)) - 1), e - 2);
            m = (m << (e - 2)) | rest;
        }
    }
    let neg = c.bit(&mut s.0[32 + sign_ctx], v < 0);
    if neg { -(m as i32) } else { m as i32 }
}

/// Codes (or decodes into) one component's blocks, in raster order.
pub fn code_component<C: Rc>(c: &mut C, md: &mut Model, cc: usize, coef: &mut [[i16; 64]], bw: usize, bh: usize) {
    let nzc = |blk: &[i16; 64]| -> u32 {
        let mut n = 0;
        for &v in &blk[1..] {
            n += (v != 0) as u32;
        }
        n
    };
    // nonzero counts of the previous row and of the left block, kept to avoid recounting
    let mut row_n = vec![0u32; bw];
    for by in 0..bh {
        let mut left_n = 0u32;
        for bx in 0..bw {
            let b = by * bw + bx;
            let has_up = by > 0;
            let has_lf = bx > 0;
            let nctx = match (has_up, has_lf) {
                (true, true) => bucket((row_n[bx] + left_n + 1) / 2),
                (true, false) => bucket(row_n[bx]),
                (false, true) => bucket(left_n),
                _ => 15,
            };
            let n_enc = if C::ENC { nzc(&coef[b]) } else { 0 };
            let base = (cc * 16 + nctx) * 64;
            let mut node = 1usize;
            for i in (0..6).rev() {
                let bit = c.bit(&mut md.nz[base + node], (n_enc >> i) & 1 == 1);
                node = (node << 1) | bit as usize;
            }
            let n = (node - 64) as u32;
            let (up, lf): ([i16; 64], [i16; 64]) = (
                if has_up { coef[b - bw] } else { [0; 64] },
                if has_lf { coef[b - 1] } else { [0; 64] },
            );
            let mut blk = if C::ENC { coef[b] } else { [0i16; 64] };
            let mut left = n;
            let mut k = 1;
            while left > 0 && k < 64 {
                let nb = (up[k].unsigned_abs() as u32 + lf[k].unsigned_abs() as u32) as u32;
                let nbb = bucket(nb).min(NB - 1);
                let lb = bucket(left).min(LB - 1);
                let sctx = if has_up && has_lf { ((up[k].signum() + lf[k].signum() + 2) as usize).min(4) } else { 5 };
                let s = &mut md.ac[((cc * 64 + k) * NB + nbb) * LB + lb];
                let v = signed(c, s, blk[k] as i32, sctx);
                blk[k] = v as i16;
                if v != 0 {
                    left -= 1;
                }
                k += 1;
            }
            // DC from neighbours (LOCO-I)
            let a = lf[0] as i32;
            let bb = up[0] as i32;
            let cdc = if has_up && has_lf { coef[b - bw - 1][0] as i32 } else { 0 };
            let pred = match (has_lf, has_up) {
                (true, true) => {
                    if cdc >= a.max(bb) { a.min(bb) } else if cdc <= a.min(bb) { a.max(bb) } else { a + bb - cdc }
                }
                (true, false) => a,
                (false, true) => bb,
                _ => 0,
            };
            let act = bucket(((a - cdc).unsigned_abs() + (bb - cdc).unsigned_abs()) as u32).min(10);
            let s = &mut md.dc[(cc * 11 + act) * 8 + bucket(n).min(7)];
            let r = signed(c, s, blk[0] as i32 - pred, 0);
            blk[0] = (pred + r) as i16;
            if !C::ENC {
                coef[b] = blk;
            }
            row_n[bx] = n;
            left_n = n;
        }
    }
}
