//! LZ with a symbol coder: LZMA's parse, coded with adaptive 16-symbol models
//! and interleaved rANS instead of LZMA's bit-by-bit range coder, so decoding
//! runs at a symbol coder's speed.
//!
//! The parse comes from the LZMA SDK's optimal parser through its token tap.
//! Tokens are coded as LZMA would see them: a literal run, then a match, a
//! repeat (one of four recent distances) or a short repeat (one byte at the last
//! distance). Contexts follow what LZMA models and the estimator in
//! `examples/lztok` found worth 4%: literals by the previous byte's top bits and
//! the position's low bits, the first literal after a match by the predicted
//! byte, kinds and lengths by the previous kind and the position, the low four
//! bits of far distances (alignment) by their own model.
//!
//! Every alphabet is a nibble or two: a 16-symbol adaptive model whose
//! probabilities sum to 2^15, updated by moving a thirty-second of every symbol's
//! mass to the one coded, so a decode is one compare over 16 lanes and the update
//! two vector operations. Symbols alternate between two rANS states, so two
//! chains overlap. Raw bits (the rest of lengths and distances) have a stream of
//! their own.
//!
//! Stream: `u32 coded length | rANS words (u16) | raw bits`.

use anyhow::{bail, ensure, Result};

/// Probability bits of every model.
const PROB_BITS: u32 = 15;
const PROB_TOTAL: u32 = 1 << PROB_BITS;
/// Adaptation: a thirty-second of each symbol's mass per coded symbol.
const RATE: u32 = 5;
/// rANS state bounds: 32-bit state, 16-bit words.
const RANS_L: u32 = 1 << 16;

/// Adaptive mass below the units reserved: one unit per symbol keeps every
/// range non-empty however the adaptive parts move, so each entry can be updated
/// on its own (AV1's coder works this way). `cum[i] = adaptive[i] + i`.
const ADAPT_TOP: u32 = PROB_TOTAL - 16;

/// One adaptive 16-symbol model: cumulative frequencies with `cum[16] == 2^15`.
#[derive(Clone, Copy)]
#[repr(align(32))]
struct Model {
    cum: [u16; 17],
}

impl Model {
    const fn new() -> Model {
        let mut cum = [0u16; 17];
        let mut i = 0;
        while i <= 16 {
            cum[i] = (i * (ADAPT_TOP as usize / 16) + i) as u16;
            i += 1;
        }
        Model { cum }
    }

    #[inline(always)]
    fn freq(&self, s: usize) -> u32 {
        // SAFETY: callers pass s <= 15.
        unsafe { (*self.cum.get_unchecked(s + 1) - *self.cum.get_unchecked(s)) as u32 }
    }

    /// Moves about a thirty-second of every other symbol's adaptive mass to `s`:
    /// entries below `s` shrink towards 0, entries above grow towards the top,
    /// each on its own. The reserved units keep the entries strictly increasing.
    #[inline(always)]
    fn update(&mut self, s: usize) {
        #[cfg(target_arch = "aarch64")]
        // SAFETY: NEON is in the Apple Silicon baseline; `cum` is 17 lanes.
        unsafe {
            self.update_neon(s)
        }
        #[cfg(not(target_arch = "aarch64"))]
        self.update_scalar(s)
    }

    #[inline(always)]
    #[allow(dead_code)]
    fn update_scalar(&mut self, s: usize) {
        for i in 1..16 {
            let a = self.cum[i] as u32 - i as u32;
            let a = if i > s { a + ((ADAPT_TOP - a) >> RATE) } else { a - (a >> RATE) };
            self.cum[i] = (a + i as u32) as u16;
        }
    }

    /// The symbol whose range holds `slot`.
    #[inline(always)]
    fn find(&self, slot: u32) -> usize {
        #[cfg(target_arch = "aarch64")]
        // SAFETY: as for `update`.
        unsafe {
            self.find_neon(slot)
        }
        #[cfg(not(target_arch = "aarch64"))]
        self.find_scalar(slot)
    }

    #[inline(always)]
    #[allow(dead_code)]
    fn find_scalar(&self, slot: u32) -> usize {
        let mut s = 0;
        for i in 1..16 {
            s += (self.cum[i] as u32 <= slot) as usize;
        }
        s
    }

    /// `find` over 16 lanes: how many of cum[1..=16] are at or below the slot
    /// (cum[16] never is).
    #[cfg(target_arch = "aarch64")]
    #[inline(always)]
    unsafe fn find_neon(&self, slot: u32) -> usize {
        use std::arch::aarch64::*;
        let p = self.cum.as_ptr().add(1);
        let v = vdupq_n_u16(slot as u16);
        let a = vcleq_u16(vld1q_u16(p), v);
        let b = vcleq_u16(vld1q_u16(p.add(8)), v);
        // each true lane is 0xFFFF: sum as signed and negate
        let n = vaddvq_s16(vaddq_s16(vreinterpretq_s16_u16(a), vreinterpretq_s16_u16(b)));
        (-(n as i32)) as usize
    }

    /// `update` over 16 lanes, each on its own.
    #[cfg(target_arch = "aarch64")]
    #[inline(always)]
    unsafe fn update_neon(&mut self, s: usize) {
        use std::arch::aarch64::*;
        const I0: [u16; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
        const I1: [u16; 8] = [9, 10, 11, 12, 13, 14, 15, 16];
        let p = self.cum.as_mut_ptr().add(1);
        let (i0, i1) = (vld1q_u16(I0.as_ptr()), vld1q_u16(I1.as_ptr()));
        let (e0, e1) = (vld1q_u16(p), vld1q_u16(p.add(8)));
        let (a0, a1) = (vsubq_u16(e0, i0), vsubq_u16(e1, i1));
        let sv = vdupq_n_u16(s as u16);
        let top = vdupq_n_u16(ADAPT_TOP as u16);
        let (g0, g1) = (vcgtq_u16(i0, sv), vcgtq_u16(i1, sv));
        let up0 = vaddq_u16(a0, vshrq_n_u16::<{ RATE as i32 }>(vsubq_u16(top, a0)));
        let up1 = vaddq_u16(a1, vshrq_n_u16::<{ RATE as i32 }>(vsubq_u16(top, a1)));
        let dn0 = vsubq_u16(a0, vshrq_n_u16::<{ RATE as i32 }>(a0));
        let dn1 = vsubq_u16(a1, vshrq_n_u16::<{ RATE as i32 }>(a1));
        vst1q_u16(p, vaddq_u16(vbslq_u16(g0, up0, dn0), i0));
        vst1q_u16(p.add(8), vaddq_u16(vbslq_u16(g1, up1, dn1), i1));
    }
}

/// A model for an alphabet of up to 256: a high nibble, then a low one in the
/// high nibble's context.
#[derive(Clone, Copy)]
struct Wide {
    hi: Model,
    lo: [Model; 16],
}

impl Wide {
    const fn new() -> Wide {
        Wide { hi: Model::new(), lo: [Model::new(); 16] }
    }
}

/// Kinds of token, as coded: 0 match, 1-4 repeats 0-3, 5 short repeat; 6 is the
/// state before any token.
const KIND_MATCH: usize = 0;
const KIND_SHORT: usize = 5;
const KIND_START: usize = 6;

/// zstd-style code for a count: direct below 16, then two codes per octave with
/// the rest raw. Returns (code, extra bits, extra value).
#[inline(always)]
fn code(v: u32) -> (u32, u32, u32) {
    if v < 16 {
        (v, 0, 0)
    } else {
        let nb = 31 - v.leading_zeros();
        let c = 16 + (nb - 4) * 2 + ((v >> (nb - 1)) & 1);
        (c, nb - 1, v & ((1 << (nb - 1)) - 1))
    }
}

#[inline(always)]
fn uncode(c: u32, extra: u32) -> u32 {
    if c < 16 {
        c
    } else {
        let nb = (c - 16) / 2 + 4;
        (1 << nb) | (((c - 16) & 1) << (nb - 1)) | extra
    }
}

/// Every model, laid out so the hot ones sit together.
struct Models {
    /// literal by (prev byte >> 5) + 8 * (pos & 3)
    lit: [Wide; 32],
    /// literal after a match, as byte ^ predicted, by (predicted >> 5) * 8 + (prev >> 5)
    matched: [Wide; 64],
    /// literal-run code by previous kind
    ll: [Wide; 7],
    /// kind by previous kind * 4 + (pos & 3)
    kind: [Model; 28],
    /// match-length code by (is match) + 2 * (pos & 3)
    ml: [Wide; 8],
    /// distance bucket by length class
    of: [Wide; 3],
    /// low four bits of a far distance
    align: Model,
}

impl Models {
    fn new() -> Box<Models> {
        Box::new(Models {
            lit: [Wide::new(); 32],
            matched: [Wide::new(); 64],
            ll: [Wide::new(); 7],
            kind: [Model::new(); 28],
            ml: [Wide::new(); 8],
            of: [Wide::new(); 3],
            align: Model::new(),
        })
    }
}

/// A parse token: a literal run is implicit between matches.
#[derive(Clone, Copy, Debug)]
pub struct Token {
    pub pos: u32,
    pub len: u32,
    /// 0 match, 1-4 repeat 0-3, 5 short repeat
    pub kind: u8,
    /// distance in bytes, for a match
    pub dist: u32,
}

/// Symbols in coding order, with the model state each was coded under, for the
/// backward rANS pass.
struct Coded {
    cum: Vec<u16>,
    freq: Vec<u16>,
}

impl Coded {
    #[inline(always)]
    fn push(&mut self, m: &mut Model, s: usize) {
        self.cum.push(m.cum[s]);
        self.freq.push(m.freq(s) as u16);
        m.update(s);
    }
    fn push_wide(&mut self, w: &mut Wide, v: u32) {
        let (hi, lo) = ((v >> 4) as usize, (v & 15) as usize);
        self.push(&mut w.hi, hi);
        self.push(&mut w.lo[hi], lo);
    }
}

/// Raw bits, written MSB-first into bytes.
struct RawWriter {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl RawWriter {
    #[inline(always)]
    fn put(&mut self, v: u32, bits: u32) {
        if bits == 0 {
            return;
        }
        self.acc = (self.acc << bits) | v as u64;
        self.n += bits;
        while self.n >= 8 {
            self.n -= 8;
            self.out.push((self.acc >> self.n) as u8);
        }
    }
    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push((self.acc << (8 - self.n)) as u8);
        }
        self.out
    }
}

/// Encodes `data` given its parse. The tokens must cover the data in order.
pub fn encode(data: &[u8], tokens: &[Token]) -> Vec<u8> {
    let mut m = Models::new();
    let mut coded = Coded { cum: Vec::with_capacity(tokens.len() * 4), freq: Vec::with_capacity(tokens.len() * 4) };
    let mut raw = RawWriter { out: Vec::with_capacity(data.len() / 8), acc: 0, n: 0 };
    let mut reps = [1u32; 4];
    let (mut pos, mut prev_kind, mut after_match) = (0usize, KIND_START, false);
    let mut ti = 0;
    loop {
        // A short repeat whose distance the replay doesn't agree with (see below)
        // can't be a match, matches being two bytes or more: its byte is a literal.
        while tokens.get(ti).is_some_and(|t| t.kind as usize == KIND_SHORT && reps[0] != t.dist) {
            ti += 1;
        }
        // the literal run up to the next token, or the end
        let next = tokens.get(ti).map_or(data.len(), |t| t.pos as usize);
        let run = (next - pos) as u32;
        let (c, nb, extra) = code(run);
        coded.push_wide(&mut m.ll[prev_kind], c);
        raw.put(extra, nb);
        for i in 0..run as usize {
            let p = pos + i;
            let prev = if p > 0 { (data[p - 1] >> 5) as usize } else { 0 };
            if after_match && i == 0 {
                let mb = if p >= reps[0] as usize { data[p - reps[0] as usize] } else { 0 };
                coded.push_wide(&mut m.matched[(mb >> 5) as usize * 8 + prev], (data[p] ^ mb) as u32);
            } else {
                coded.push_wide(&mut m.lit[prev + 8 * (p & 3)], data[p] as u32);
            }
        }
        pos = next;
        let Some(&t) = tokens.get(ti) else { break };
        ti += 1;
        // A repeat is coded as one only while the replayed distances agree with
        // the parser's: the LZMA2 encoder restores its state after a chunk it
        // stores raw, and the two drift apart there. Otherwise it is a match.
        let mut kind = t.kind as usize;
        if kind != KIND_MATCH {
            let r = if kind == KIND_SHORT { 0 } else { kind - 1 };
            if reps[r] != t.dist {
                kind = KIND_MATCH;
            }
        }
        coded.push(&mut m.kind[prev_kind * 4 + (pos & 3)], kind);
        if kind != KIND_SHORT {
            let (c, nb, extra) = code(t.len - 2);
            coded.push_wide(&mut m.ml[(kind == KIND_MATCH) as usize + 2 * (pos & 3)], c);
            raw.put(extra, nb);
        }
        let d;
        if kind == KIND_MATCH {
            d = t.dist;
            let nb = 31 - d.leading_zeros();
            let class = (t.len > 4) as usize + (t.len > 16) as usize;
            coded.push_wide(&mut m.of[class], nb);
            if nb >= 6 {
                // bits below the top one: the low four modelled, the rest raw
                raw.put((d >> 4) & ((1 << (nb - 4)) - 1), nb - 4);
                coded.push(&mut m.align, (d & 15) as usize);
            } else if nb > 0 {
                raw.put(d & ((1 << nb) - 1), nb);
            }
            reps = [d, reps[0], reps[1], reps[2]];
        } else {
            let r = if kind == KIND_SHORT { 0 } else { kind - 1 };
            d = reps[r];
            if r > 0 {
                reps.copy_within(0..r, 1);
                reps[0] = d;
            }
        }
        debug_assert!(d as usize <= pos && d > 0);
        pos += t.len as usize;
        prev_kind = kind;
        after_match = true;
    }
    debug_assert_eq!(pos, data.len());

    // rANS, backwards, two states alternating by symbol index
    let n = coded.cum.len();
    let mut words: Vec<u16> = Vec::with_capacity(n / 2 + 8);
    let mut x = [RANS_L; 2];
    for i in (0..n).rev() {
        let (c, f) = (coded.cum[i] as u32, coded.freq[i] as u32);
        let s = &mut x[i & 1];
        // renormalize so the decoder's renormalization mirrors it
        let max = f << (32 - PROB_BITS);
        if *s >= max {
            words.push(*s as u16);
            *s >>= 16;
        }
        *s = (*s / f) * PROB_TOTAL + (*s % f) + c;
    }
    let mut out = Vec::with_capacity(words.len() * 2 + raw.out.len() + 16);
    out.extend_from_slice(&((words.len() * 2 + 8) as u32).to_le_bytes());
    out.extend_from_slice(&x[0].to_le_bytes());
    out.extend_from_slice(&x[1].to_le_bytes());
    for w in words.iter().rev() {
        out.extend_from_slice(&w.to_le_bytes());
    }
    out.extend_from_slice(&raw.finish());
    out
}

/// Reads rANS words forward. Two states; the one in `a` is next, and they swap
/// after every symbol, so both stay in registers.
struct Rans {
    a: u32,
    b: u32,
    p: *const u8,
    end: *const u8,
}

impl Rans {
    /// Decodes one symbol of `m`.
    #[inline(always)]
    unsafe fn sym(&mut self, m: &mut Model) -> usize {
        let x = self.a;
        let slot = x & (PROB_TOTAL - 1);
        let s = m.find(slot);
        let c = *m.cum.get_unchecked(s) as u32;
        let f = m.freq(s);
        let nx = f * (x >> PROB_BITS) + slot - c;
        // Renormalize without a branch on the state: the word is read either way
        // (past the end it reads as zero, which the length check catches) and the
        // pointer moves only when the word was used.
        let w = if self.p < self.end { u16::from_le_bytes([*self.p, *self.p.add(1)]) as u32 } else { 0 };
        let need = nx < RANS_L;
        let renormed = (nx << 16) | w;
        self.a = self.b;
        self.b = if need { renormed } else { nx };
        self.p = self.p.add(if need { 2 } else { 0 });
        m.update(s);
        s
    }
}

struct RawReader<'a> {
    d: &'a [u8],
    bit: usize,
}

impl RawReader<'_> {
    #[inline(always)]
    fn get(&mut self, bits: u32) -> Result<u32> {
        if bits == 0 {
            return Ok(0);
        }
        ensure!(self.bit + bits as usize <= self.d.len() * 8, "raw bits overrun");
        let byte = self.bit >> 3;
        let mut w = [0u8; 8];
        let avail = self.d.len().saturating_sub(byte).min(8);
        w[..avail].copy_from_slice(&self.d[byte..byte + avail]);
        let v = u64::from_be_bytes(w);
        let shift = 64 - (self.bit & 7) as u32 - bits;
        self.bit += bits as usize;
        Ok(((v >> shift) & ((1u64 << bits) - 1)) as u32)
    }
}

/// Decodes a run of `run` literals at `pos`. On its own so that the loop's few
/// live values stay in registers: inside `decode` the compiler reloaded a spilled
/// pointer on every byte.
///
/// # Safety
/// `out` must have room for `pos + run` bytes, and `pos - rep0` must be in it
/// when `after_match` and `pos >= rep0`.
#[inline(never)]
unsafe fn literals(r: &mut Rans, m: &mut Models, out: *mut u8, pos: usize, run: usize, after_match: bool, rep0: usize) {
    let mut p = pos;
    let end = pos + run;
    let mut prev = if p > 0 { (*out.add(p - 1) >> 5) as usize } else { 0 };
    if after_match && run > 0 {
        let mb = if p >= rep0 { *out.add(p - rep0) } else { 0 };
        let w = m.matched.get_unchecked_mut((mb >> 5) as usize * 8 + prev);
        let hi = r.sym(&mut w.hi);
        let lo = r.sym(w.lo.get_unchecked_mut(hi));
        let b = (((hi << 4) | lo) as u8) ^ mb;
        *out.add(p) = b;
        prev = (b >> 5) as usize;
        p += 1;
    }
    while p < end {
        let w = m.lit.get_unchecked_mut(prev + 8 * (p & 3));
        let hi = r.sym(&mut w.hi);
        let lo = r.sym(w.lo.get_unchecked_mut(hi));
        let b = ((hi << 4) | lo) as u8;
        *out.add(p) = b;
        prev = (b >> 5) as usize;
        p += 1;
    }
}

/// Decodes `size` bytes.
pub fn decode(stored: &[u8], size: usize) -> Result<Vec<u8>> {
    ensure!(stored.len() >= 12, "lzs stream too short");
    let coded_len = u32::from_le_bytes(stored[..4].try_into().unwrap()) as usize;
    ensure!(coded_len >= 8 && 4 + coded_len <= stored.len(), "lzs coded length");
    let x0 = u32::from_le_bytes(stored[4..8].try_into().unwrap());
    let x1 = u32::from_le_bytes(stored[8..12].try_into().unwrap());
    let words = &stored[12..4 + coded_len];
    let mut raw = RawReader { d: &stored[4 + coded_len..], bit: 0 };
    let mut m = Models::new();
    let mut out: Vec<u8> = Vec::with_capacity(size + 32);
    let mut reps = [1u32; 4];
    let (mut prev_kind, mut after_match) = (KIND_START, false);
    // SAFETY: the rANS reader checks its end on every read; output writes stay
    // within the capacity reserved above, checked per token.
    unsafe {
        let mut r = Rans { a: x0, b: x1, p: words.as_ptr(), end: words.as_ptr().add(words.len()) };
        macro_rules! sym {
            ($m:expr) => {{
                r.sym($m)
            }};
        }
        macro_rules! wide {
            ($w:expr) => {{
                let w: &mut Wide = $w;
                let hi = sym!(&mut w.hi);
                let lo = sym!(w.lo.get_unchecked_mut(hi));
                ((hi << 4) | lo) as u32
            }};
        }
        loop {
            let pos = out.len();
            let c = wide!(&mut m.ll[prev_kind]);
            let nb = if c < 16 { 0 } else { (c - 16) / 2 + 3 };
            let run = uncode(c, raw.get(nb)?) as usize;
            ensure!(pos + run <= size, "literal run past the end");
            let op = out.as_mut_ptr();
            literals(&mut r, &mut m, op, pos, run, after_match, reps[0] as usize);
            out.set_len(pos + run);
            let pos = pos + run;
            if pos >= size {
                break;
            }
            let kind = sym!(&mut m.kind[prev_kind * 4 + (pos & 3)]);
            ensure!(kind <= KIND_SHORT, "bad token kind");
            let len = if kind == KIND_SHORT {
                1
            } else {
                let c = wide!(&mut m.ml[(kind == KIND_MATCH) as usize + 2 * (pos & 3)]);
                let nb = if c < 16 { 0 } else { (c - 16) / 2 + 3 };
                uncode(c, raw.get(nb)?) as usize + 2
            };
            let d = if kind == KIND_MATCH {
                let class = (len > 4) as usize + (len > 16) as usize;
                let nb = wide!(&mut m.of[class]);
                ensure!(nb < 32, "bad distance bucket");
                let d = if nb >= 6 {
                    let high = raw.get(nb - 4)?;
                    let low = sym!(&mut m.align) as u32;
                    (1 << nb) | (high << 4) | low
                } else if nb > 0 {
                    (1 << nb) | raw.get(nb)?
                } else {
                    1
                };
                reps = [d, reps[0], reps[1], reps[2]];
                d
            } else {
                let rr = if kind == KIND_SHORT { 0 } else { kind - 1 };
                let d = reps[rr];
                if rr > 0 {
                    reps.copy_within(0..rr, 1);
                    reps[0] = d;
                }
                d
            } as usize;
            ensure!(d >= 1 && d <= pos && pos + len <= size, "bad match");
            // copy, byte by byte where the match overlaps itself
            let (src, dst) = (op.add(pos - d), op.add(pos));
            if d >= len {
                std::ptr::copy_nonoverlapping(src, dst, len);
            } else {
                for i in 0..len {
                    *dst.add(i) = *src.add(i);
                }
            }
            out.set_len(pos + len);
            prev_kind = kind;
            after_match = true;
        }
        ensure!(r.p <= r.end.add(2), "rANS stream overrun");
    }
    if out.len() != size {
        bail!("lzs decoded {} bytes, expected {size}", out.len());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_keeps_its_total_and_order() {
        let mut m = Model::new();
        let mut s = 7u64;
        for _ in 0..100_000 {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let sym = ((s >> 60) as usize) % 16;
            m.update(if sym < 12 { sym % 3 } else { sym });
            assert_eq!(m.cum[16] as u32, PROB_TOTAL);
            assert_eq!(m.cum[0], 0);
            for i in 0..16 {
                assert!(m.cum[i] < m.cum[i + 1], "{:?}", m.cum);
            }
        }
        for slot in 0..PROB_TOTAL {
            let f = m.find(slot);
            assert!(m.cum[f] as u32 <= slot && slot < m.cum[f + 1] as u32);
        }
    }

    #[test]
    fn neon_and_scalar_agree() {
        let (mut a, mut b) = (Model::new(), Model::new());
        let mut s = 99u64;
        for _ in 0..200_000 {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let sym = ((s >> 59) as usize) % 16;
            let sym = if sym < 10 { sym % 4 } else { sym };
            let slot = (s >> 20) as u32 & (PROB_TOTAL - 1);
            assert_eq!(a.find(slot), b.find_scalar(slot));
            a.update(sym);
            b.update_scalar(sym);
            assert_eq!(a.cum, b.cum);
        }
    }

    #[test]
    fn codes_round_trip() {
        for v in (0..5000).chain([65535, 1 << 20, (1 << 31) - 1]) {
            let (c, nb, extra) = code(v);
            assert_eq!(uncode(c, extra), v);
            assert_eq!(nb, if c < 16 { 0 } else { (c - 16) / 2 + 3 });
        }
    }

    #[test]
    fn literals_only_round_trip() {
        let data: Vec<u8> = (0..10_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let out = encode(&data, &[]);
        assert_eq!(decode(&out, data.len()).unwrap(), data);
    }
}
