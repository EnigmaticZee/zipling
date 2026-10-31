//! LZMA's parse coded with context models that are tables, not bit models.
//!
//! Every context keeps counts of the symbols it has coded and a decode table
//! built from them. A symbol decodes with one table lookup, a multiply and an
//! add; the table is rebuilt only after the context has coded a growing number
//! of symbols (32, 64, ... up to `CAP`), the counts halving at each rebuild from
//! then on, so the model follows each context at its own pace for a few cycles a
//! symbol. A symbol the table has not seen goes through an escape symbol and raw
//! bits. Nothing is sent ahead: decoder and encoder rebuild from the same counts.
//!
//! The contexts are those of `lzs`, which the estimator in `examples/lztok`
//! (column A+) puts 3.9% behind LZMA and 4.6% ahead of zstd -19 on Silesia:
//! literals by the previous byte's top bits and the position's low bits, the
//! first literal after a match by the predicted byte, the token's kind and length
//! code as one symbol by the previous kind and the position, distance buckets by
//! length class, and the low four bits of far distances.
//!
//! Two interleaved rANS states; raw bits have a stream of their own.
//!
//! Stream: `u32 coded bytes | x0 x1 | rANS words | raw bits`.

use crate::lzs::{code, uncode, Token};
use anyhow::{bail, ensure, Result};

/// Probability bits: 11 for the alphabets of 256 and more, 10 for the small ones.
const BIG_BITS: u32 = 11;
const SMALL_BITS: u32 = 10;
const RANS_L: u32 = 1 << 16;

/// A context's first table rebuild comes after this many symbols, the next
/// after twice as many, up to `CAP`, where counts start halving at each rebuild.
const START: u32 = 32;
const CAP: u32 = 4096;

// Contexts, flat.
const LIT0: usize = 0;
const MATCHED0: usize = 32;
const LL0: usize = 96;
const FUSED0: usize = 103;
const OF0: usize = 131;
const ALIGN: usize = 134;
/// Extra bits of length codes 16-21 (lengths 18-129), run codes 16-21, and the
/// bits below the top one of a distance under 64: modelled, as LZMA does.
const LEX0: usize = 135;
const RLEX0: usize = 141;
const DLOW0: usize = 147;
const N_CTX: usize = 152;

/// Length codes (`code` of anything below 2^32 is under 72); a fused symbol is
/// `kind * ML_CODES + length code`.
const ML_CODES: usize = 72;

const KIND_MATCH: usize = 0;
const KIND_SHORT: usize = 5;
const KIND_START: usize = 6;

/// Symbols of a context, the escape not counted: the escape is symbol `alphabet`.
const fn alphabet(ctx: usize) -> usize {
    match ctx {
        0..=95 => 256,
        96..=102 => ML_CODES,
        103..=130 => 6 * ML_CODES,
        131..=133 => 32,
        134 => 16,
        135..=146 => 8 << ((ctx - LEX0) % 6 / 2),
        _ => 2 << (ctx - DLOW0),
    }
}

/// Raw bits an escaped symbol of the context takes.
const fn sym_bits(ctx: usize) -> u32 {
    match ctx {
        0..=95 => 8,
        96..=102 => 7,
        103..=130 => 9,
        131..=133 => 5,
        134 => 4,
        135..=146 => 3 + ((ctx - LEX0) % 6 / 2) as u32,
        _ => 1 + (ctx - DLOW0) as u32,
    }
}

const fn bits_of(ctx: usize) -> u32 {
    if ctx < 96 || (ctx >= FUSED0 && ctx < OF0) {
        BIG_BITS
    } else {
        SMALL_BITS
    }
}

const fn bases() -> ([usize; N_CTX + 1], [usize; N_CTX + 1]) {
    let (mut tab, mut cnt) = ([0usize; N_CTX + 1], [0usize; N_CTX + 1]);
    let mut i = 0;
    while i < N_CTX {
        tab[i + 1] = tab[i] + (1 << bits_of(i));
        cnt[i + 1] = cnt[i] + alphabet(i) + 1;
        i += 1;
    }
    (tab, cnt)
}

/// Where each context's decode table and count row start.
const TABLE_BASE: [usize; N_CTX + 1] = bases().0;
const COUNT_BASE: [usize; N_CTX + 1] = bases().1;
const TABLE_ENTRIES: usize = TABLE_BASE[N_CTX];
const COUNT_ENTRIES: usize = COUNT_BASE[N_CTX];

/// Walks the parse in coding order: `sym(context, symbol)` for every modelled
/// symbol, `raw(value, bits, category)` for the raw bits (0 run extra, 1 length
/// extra, 2 near distance, 3 far distance). Encoder and decoder agree through it.
fn walk(data: &[u8], tokens: &[Token], mut sym: impl FnMut(usize, u32), mut raw: impl FnMut(u32, u32, usize)) {
    let mut reps = [1u32; 4];
    let (mut pos, mut prev_kind, mut after_match) = (0usize, KIND_START, false);
    let mut ti = 0;
    loop {
        // A short repeat whose distance the replay doesn't agree with can't be a
        // match (matches are two bytes or more): its byte is a literal.
        while tokens.get(ti).is_some_and(|t| t.kind as usize == KIND_SHORT && reps[0] != t.dist) {
            ti += 1;
        }
        let next = tokens.get(ti).map_or(data.len(), |t| t.pos as usize);
        let run = (next - pos) as u32;
        let (c, nb, extra) = code(run);
        sym(LL0 + prev_kind, c);
        if (16..22).contains(&c) {
            sym(RLEX0 + (c - 16) as usize, extra);
        } else {
            raw(extra, nb, 0);
        }
        for i in 0..run as usize {
            let p = pos + i;
            let prev = if p > 0 { (data[p - 1] >> 5) as usize } else { 0 };
            if after_match && i == 0 {
                let mb = if p >= reps[0] as usize { data[p - reps[0] as usize] } else { 0 };
                sym(MATCHED0 + (mb >> 5) as usize * 8 + prev, (data[p] ^ mb) as u32);
            } else {
                sym(LIT0 + prev + 8 * (p & 3), data[p] as u32);
            }
        }
        pos = next;
        let Some(&t) = tokens.get(ti) else { break };
        ti += 1;
        // A repeat is one only while the replayed distances agree with the
        // parser's (the LZMA2 encoder restores its state after a chunk it stores
        // raw); otherwise it is a match.
        let mut kind = t.kind as usize;
        if kind != KIND_MATCH {
            let r = if kind == KIND_SHORT { 0 } else { kind - 1 };
            if reps[r] != t.dist {
                kind = KIND_MATCH;
            }
        }
        let fused = FUSED0 + prev_kind * 4 + (pos & 3);
        if kind == KIND_SHORT {
            sym(fused, (KIND_SHORT * ML_CODES) as u32);
        } else {
            let (c, nb, extra) = code(t.len - 2);
            sym(fused, (kind * ML_CODES) as u32 + c);
            if (16..22).contains(&c) {
                sym(LEX0 + (c - 16) as usize, extra);
            } else {
                raw(extra, nb, 1);
            }
        }
        let d;
        if kind == KIND_MATCH {
            d = t.dist;
            let nb = 31 - d.leading_zeros();
            let class = (t.len > 4) as usize + (t.len > 16) as usize;
            sym(OF0 + class, nb);
            if nb >= 6 {
                raw((d >> 4) & ((1 << (nb - 4)) - 1), nb - 4, 3);
                sym(ALIGN, d & 15);
            } else if nb > 0 {
                sym(DLOW0 + nb as usize - 1, d & ((1 << nb) - 1));
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
}

/// Counts to frequencies summing to `1 << bits`, every used symbol at least 1,
/// into `freq`.
fn normalize(counts: &[u32], bits: u32, freq: &mut [u16]) {
    let total: u64 = counts.iter().map(|&c| c as u64).sum();
    let target = 1u64 << bits;
    freq.fill(0);
    debug_assert!(total > 0);
    let mut sum = 0u64;
    let mut top = 0;
    for (i, &c) in counts.iter().enumerate() {
        if c == 0 {
            continue;
        }
        let f = ((c as u64 * target + total / 2) / total).max(1);
        freq[i] = f as u16;
        sum += f;
        if freq[i] > freq[top] {
            top = i;
        }
    }
    if sum > target {
        // Trim the biggest, never below 1.
        let mut over = sum - target;
        while over > 0 {
            let i = (0..freq.len()).max_by_key(|&i| freq[i]).unwrap();
            let take = ((freq[i] - 1) as u64).min(over);
            if take == 0 {
                break;
            }
            freq[i] -= take as u16;
            over -= take;
        }
    } else {
        freq[top] += (target - sum) as u16;
    }
    debug_assert_eq!(freq.iter().map(|&f| f as u64).sum::<u64>(), target);
}

/// Bits, MSB-first.
struct Bits {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    fn new(cap: usize) -> Bits {
        Bits { out: Vec::with_capacity(cap), acc: 0, n: 0 }
    }
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

/// The models: counts, when each context rebuilds next, and the frequencies
/// and cumulative frequencies its table holds (the escape last).
struct Models {
    counts: Vec<u32>,
    since: Vec<u32>,
    interval: Vec<u32>,
    freq: Vec<u16>,
    cum: Vec<u16>,
    /// `CAP`, or `LZT_CAP` for research.
    cap: u32,
}

impl Models {
    fn new() -> Models {
        let mut m = Models {
            counts: vec![0; COUNT_ENTRIES],
            since: vec![0; N_CTX],
            interval: vec![START; N_CTX],
            freq: vec![0; COUNT_ENTRIES],
            cum: vec![0; COUNT_ENTRIES + N_CTX],
            cap: std::env::var("LZT_CAP").ok().and_then(|v| v.parse().ok()).unwrap_or(CAP),
        };
        for ctx in 0..N_CTX {
            m.rebuild(ctx, None);
        }
        m
    }

    #[inline(always)]
    fn row(ctx: usize) -> std::ops::Range<usize> {
        COUNT_BASE[ctx]..COUNT_BASE[ctx + 1]
    }

    /// `cum` has one more entry per context than `freq`.
    #[inline(always)]
    fn cum_row(ctx: usize) -> std::ops::Range<usize> {
        COUNT_BASE[ctx] + ctx..COUNT_BASE[ctx + 1] + ctx + 1
    }

    #[inline(always)]
    fn freq_cum(&self, ctx: usize, s: usize) -> (u32, u32) {
        (self.freq[COUNT_BASE[ctx] + s] as u32, self.cum[COUNT_BASE[ctx] + ctx + s] as u32)
    }

    /// Rebuilds a context's frequencies from its counts (the escape counting
    /// once), fills its decode table when given one, and sets the next rebuild.
    /// Once the interval is at its cap the counts halve, so the model keeps
    /// following the data.
    fn rebuild(&mut self, ctx: usize, tab: Option<&mut [u32]>) {
        let row = Self::row(ctx);
        // A symbol too rare for a table entry of its own (it would round to
        // nothing) goes through the escape, whose count is theirs plus one, so
        // the table's mass stays with the symbols that recur.
        let (bits, counts) = (bits_of(ctx), &self.counts[row.clone()]);
        let total: u64 = counts[..counts.len() - 1].iter().map(|&c| c as u64).sum();
        let mut tmp = Vec::with_capacity(counts.len());
        let mut esc = 1u64;
        for &c in &counts[..counts.len() - 1] {
            if c > 0 && (c as u64) << (bits + 1) < total {
                esc += c as u64;
                tmp.push(0);
            } else {
                tmp.push(c);
            }
        }
        tmp.push(esc.min(u32::MAX as u64) as u32);
        normalize(&tmp, bits, &mut self.freq[row.clone()]);
        let cum = &mut self.cum[Self::cum_row(ctx)];
        let mut acc = 0u32;
        for (i, &f) in self.freq[row.clone()].iter().enumerate() {
            cum[i] = acc as u16;
            acc += f as u32;
        }
        cum[row.len()] = acc as u16;
        if let Some(t) = tab {
            let base = TABLE_BASE[ctx];
            let (fr, cu) = (&self.freq[row.clone()], &self.cum[Self::cum_row(ctx)]);
            for (s, &f) in fr.iter().enumerate() {
                let c = cu[s] as usize;
                // symbol (9 bits), frequency (12), slot - cum (11)
                for slot in c..c + f as usize {
                    t[base + slot] = s as u32 | (f as u32) << 9 | ((slot - c) as u32) << 21;
                }
            }
        }
        self.since[ctx] = 0;
        if self.interval[ctx] >= self.cap {
            for c in &mut self.counts[row] {
                *c >>= 1;
            }
        } else {
            self.interval[ctx] *= 2;
        }
    }

    /// Counts a coded symbol; true when the context's table is due a rebuild.
    #[inline(always)]
    fn count(&mut self, ctx: usize, s: usize) -> bool {
        // SAFETY of indices: s < alphabet(ctx) <= row length - 1, ctx < N_CTX.
        unsafe {
            *self.counts.get_unchecked_mut(COUNT_BASE[ctx] + s) += 1;
            let since = self.since.get_unchecked_mut(ctx);
            *since += 1;
            *since >= *self.interval.get_unchecked(ctx)
        }
    }
}

/// Encodes `data` given its parse. The tokens must cover the data in order.
pub fn encode(data: &[u8], tokens: &[Token]) -> Vec<u8> {
    let mut m = Models::new();
    // Every symbol with the frequency and cumulative frequency it is coded with
    // (escapes included) and its context's probability bits, for the backward
    // rANS pass; raw bits as they come.
    let mut coded_f: Vec<u16> = Vec::with_capacity(data.len() / 2);
    let mut coded_c: Vec<u16> = Vec::with_capacity(data.len() / 2);
    let mut coded_b: Vec<u8> = Vec::with_capacity(data.len() / 2);
    // both callbacks write raw bits: escapes, and the lengths' and distances' own
    let raw = std::cell::RefCell::new(Bits::new(data.len() / 8));
    // Research: bits per category (LZT_STATS).
    let stats = std::env::var_os("LZT_STATS").is_some();
    let spent = std::cell::RefCell::new([0f64; 11]);
    let group = |ctx: usize| match ctx {
        0..=31 => 0,
        32..=95 => 1,
        96..=102 | 141..=146 => 2,
        103..=130 | 135..=140 => 3,
        131..=133 | 147..=151 => 4,
        _ => 5,
    };
    walk(
        data,
        tokens,
        |ctx, s| {
            let s = s as usize;
            let (f, c) = m.freq_cum(ctx, s);
            if f == 0 {
                let (f, c) = m.freq_cum(ctx, alphabet(ctx));
                debug_assert!(f > 0, "escape missing in context {ctx}");
                coded_f.push(f as u16);
                coded_c.push(c as u16);
                raw.borrow_mut().put(s as u32, sym_bits(ctx));
                if stats {
                    spent.borrow_mut()[6] += bits_of(ctx) as f64 - (f as f64).log2() + sym_bits(ctx) as f64;
                }
            } else {
                coded_f.push(f as u16);
                coded_c.push(c as u16);
                if stats {
                    spent.borrow_mut()[group(ctx)] += bits_of(ctx) as f64 - (f as f64).log2();
                }
            }
            coded_b.push(bits_of(ctx) as u8);
            if m.count(ctx, s) {
                m.rebuild(ctx, None);
            }
        },
        |v, bits, cat| {
            raw.borrow_mut().put(v, bits);
            if stats {
                spent.borrow_mut()[7 + cat] += bits as f64;
            }
        },
    );
    let raw = raw.into_inner();
    if stats {
        let b = spent.borrow();
        let total: f64 = b.iter().sum();
        eprintln!("lzt bits: total {:.0} KB", total / 8192.0);
        let names = ["literal", "matched", "run", "kind+len", "dist", "align", "escape", "raw run", "raw len", "raw d<64", "raw d far"];
        for (name, v) in names.iter().zip(b.iter()) {
            eprintln!("  {name:9} {:8.0} KB {:5.1}%", v / 8192.0, 100.0 * v / total);
        }
    }

    // rANS, backwards, two states alternating by symbol index.
    let n = coded_f.len();
    let mut words: Vec<u16> = Vec::with_capacity(n / 2 + 8);
    let mut x = [RANS_L; 2];
    for i in (0..n).rev() {
        let (f, c, bits) = (coded_f[i] as u32, coded_c[i] as u32, coded_b[i] as u32);
        let st = &mut x[i & 1];
        // in 64 bits: a context with one symbol has f == 2^bits
        if (*st as u64) >= (f as u64) << (32 - bits) {
            words.push(*st as u16);
            *st >>= 16;
        }
        *st = ((*st / f) << bits) + (*st % f) + c;
    }
    let raw = raw.finish();
    let mut out = Vec::with_capacity(words.len() * 2 + raw.len() + 16);
    out.extend_from_slice(&((words.len() * 2 + 8) as u32).to_le_bytes());
    out.extend_from_slice(&x[0].to_le_bytes());
    out.extend_from_slice(&x[1].to_le_bytes());
    for w in words.iter().rev() {
        out.extend_from_slice(&w.to_le_bytes());
    }
    out.extend_from_slice(&raw);
    out
}

/// Reads rANS words forward; two states, the one in `a` next, swapped after
/// every symbol.
struct Rans {
    a: u32,
    b: u32,
    p: *const u8,
    end: *const u8,
}

impl Rans {
    /// Decodes one symbol from the table at `tab` with `bits` probability bits.
    #[inline(always)]
    unsafe fn sym(&mut self, tab: *const u32, bits: u32) -> u32 {
        let x = self.a;
        let slot = x & ((1 << bits) - 1);
        let e = *tab.add(slot as usize);
        let nx = (x >> bits) * ((e >> 9) & 0xfff) + (e >> 21);
        // Renormalize without a branch on the state: the word is read either way
        // (zero past the end, which the final check catches) and the pointer
        // moves only when the word was used.
        let w = if self.p < self.end { u16::from_le_bytes([*self.p, *self.p.add(1)]) as u32 } else { 0 };
        let need = nx < RANS_L;
        self.a = self.b;
        self.b = if need { (nx << 16) | w } else { nx };
        self.p = self.p.add(if need { 2 } else { 0 });
        e & 511
    }
}

/// Raw bits, MSB-first, from a buffer padded with eight zero bytes.
struct RawReader {
    p: *const u8,
    acc: u64,
    n: u32,
}

impl RawReader {
    #[inline(always)]
    unsafe fn get(&mut self, bits: u32) -> u32 {
        if bits == 0 {
            return 0;
        }
        if self.n < bits {
            while self.n <= 56 {
                self.acc = (self.acc << 8) | *self.p as u64;
                self.p = self.p.add(1);
                self.n += 8;
            }
        }
        self.n -= bits;
        ((self.acc >> self.n) & ((1u64 << bits) - 1)) as u32
    }
}

/// The decoder: coder, raw bits, decode tables and models.
struct Dec {
    r: Rans,
    raw: RawReader,
    tab: Vec<u32>,
    m: Models,
}

impl Dec {
    /// Decodes one symbol of `ctx`: escape resolved, counted, table rebuilt when due.
    #[inline(always)]
    unsafe fn sym(&mut self, ctx: usize) -> usize {
        let mut s = self.r.sym(self.tab.as_ptr().add(TABLE_BASE[ctx]), bits_of(ctx)) as usize;
        if s == alphabet(ctx) {
            s = self.raw.get(sym_bits(ctx)) as usize;
        }
        if self.m.count(ctx, s) {
            self.m.rebuild(ctx, Some(&mut self.tab));
        }
        s
    }
}

/// Bytes a match copy may write past its end when there is room: the output
/// buffer has this much spare capacity.
const COPY_SLACK: usize = 32;

/// Copies a match of `len` bytes from `d` bytes back. With room to overshoot it
/// goes 16 bytes at a time; a distance under 16 repeats a pattern instead of
/// running a byte loop (half of x-ray's matches are that short).
///
/// # Safety
/// `src` and `dst` must be valid for `len` bytes, plus `COPY_SLACK` past `dst`
/// when `room`.
#[inline(always)]
unsafe fn copy_match(src: *const u8, dst: *mut u8, d: usize, len: usize, room: bool) {
    if !room {
        for i in 0..len {
            *dst.add(i) = *src.add(i);
        }
    } else if d >= 16 {
        let mut i = 0;
        while i < len {
            std::ptr::copy_nonoverlapping(src.add(i), dst.add(i), 16);
            i += 16;
        }
    } else {
        // a 16-byte window of the period-d pattern, advanced by whole periods
        let mut pat = [0u8; 16];
        for (i, p) in pat.iter_mut().enumerate() {
            *p = *src.add(i % d);
        }
        let step = (16 / d) * d;
        let mut i = 0;
        while i < len {
            std::ptr::copy_nonoverlapping(pat.as_ptr(), dst.add(i), 16);
            i += step;
        }
    }
}

/// Decodes a run of `run` literals at `pos`; on its own so the loop's few live
/// values stay in registers.
///
/// # Safety
/// `out` must have room for `pos + run` bytes, and `pos - rep0` must be in it
/// when `after_match` and `pos >= rep0`.
#[inline(never)]
unsafe fn literals(d: &mut Dec, out: *mut u8, pos: usize, run: usize, after_match: bool, rep0: usize) {
    let mut p = pos;
    let end = pos + run;
    let mut prev = if p > 0 { (*out.add(p - 1) >> 5) as usize } else { 0 };
    if after_match && run > 0 {
        let mb = if p >= rep0 { *out.add(p - rep0) } else { 0 };
        let b = d.sym(MATCHED0 + (mb >> 5) as usize * 8 + prev) as u8 ^ mb;
        *out.add(p) = b;
        prev = (b >> 5) as usize;
        p += 1;
    }
    while p < end {
        let b = d.sym(LIT0 + prev + 8 * (p & 3)) as u8;
        *out.add(p) = b;
        prev = (b >> 5) as usize;
        p += 1;
    }
}

/// Decodes `size` bytes.
pub fn decode(stored: &[u8], size: usize) -> Result<Vec<u8>> {
    ensure!(stored.len() >= 12, "lzt stream too short");
    let coded_len = u32::from_le_bytes(stored[..4].try_into().unwrap()) as usize;
    ensure!(coded_len >= 8 && 4 + coded_len <= stored.len(), "lzt coded length");
    let x0 = u32::from_le_bytes(stored[4..8].try_into().unwrap());
    let x1 = u32::from_le_bytes(stored[8..12].try_into().unwrap());
    let words = &stored[12..4 + coded_len];
    let mut rawbuf = stored[4 + coded_len..].to_vec();
    let raw_len = rawbuf.len();
    rawbuf.extend_from_slice(&[0u8; 8]);
    let mut dec = Dec {
        r: Rans { a: x0, b: x1, p: words.as_ptr(), end: unsafe { words.as_ptr().add(words.len()) } },
        raw: RawReader { p: rawbuf.as_ptr(), acc: 0, n: 0 },
        tab: vec![0u32; TABLE_ENTRIES],
        m: Models::new(),
    };
    // the fresh models' tables: escape only, until each context's first rebuild
    for ctx in 0..N_CTX {
        let base = TABLE_BASE[ctx];
        let esc = alphabet(ctx) as u32;
        for (i, e) in dec.tab[base..base + (1 << bits_of(ctx))].iter_mut().enumerate() {
            *e = esc | (1u32 << bits_of(ctx)) << 9 | (i as u32) << 21;
        }
    }
    let mut out: Vec<u8> = Vec::with_capacity(size + COPY_SLACK);
    let mut reps = [1u32; 4];
    let (mut prev_kind, mut after_match) = (KIND_START, false);
    // SAFETY: the rANS reader checks its end on every read, the raw reader's
    // buffer is padded, and output writes stay within the capacity reserved
    // above, checked per token.
    unsafe {
        loop {
            let pos = out.len();
            let c = dec.sym(LL0 + prev_kind) as u32;
            let nb = if c < 16 { 0 } else { (c - 16) / 2 + 3 };
            let extra = if (16..22).contains(&c) { dec.sym(RLEX0 + (c - 16) as usize) as u32 } else { dec.raw.get(nb) };
            let run = uncode(c, extra) as usize;
            ensure!(pos + run <= size, "literal run past the end");
            let op = out.as_mut_ptr();
            literals(&mut dec, op, pos, run, after_match, reps[0] as usize);
            out.set_len(pos + run);
            let pos = pos + run;
            if pos >= size {
                break;
            }
            let v = dec.sym(FUSED0 + prev_kind * 4 + (pos & 3));
            let (kind, c) = (v / ML_CODES, (v % ML_CODES) as u32);
            ensure!(kind <= KIND_SHORT, "bad token kind");
            let len = if kind == KIND_SHORT {
                1
            } else {
                let nb = if c < 16 { 0 } else { (c - 16) / 2 + 3 };
                let extra = if (16..22).contains(&c) { dec.sym(LEX0 + (c - 16) as usize) as u32 } else { dec.raw.get(nb) };
                uncode(c, extra) as usize + 2
            };
            let d = if kind == KIND_MATCH {
                let class = (len > 4) as usize + (len > 16) as usize;
                let nb = dec.sym(OF0 + class) as u32;
                ensure!(nb < 32, "bad distance bucket");
                let d = if nb >= 6 {
                    let high = dec.raw.get(nb - 4);
                    let low = dec.sym(ALIGN) as u32;
                    (1 << nb) | (high << 4) | low
                } else if nb > 0 {
                    (1 << nb) | dec.sym(DLOW0 + nb as usize - 1) as u32
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
            let (src, dst) = (op.add(pos - d), op.add(pos));
            copy_match(src, dst, d, len, pos + len + COPY_SLACK <= size);
            out.set_len(pos + len);
            prev_kind = kind;
            after_match = true;
        }
        ensure!(dec.r.p <= dec.r.end.add(2), "rANS stream overrun");
        ensure!(dec.raw.p as usize - rawbuf.as_ptr() as usize <= raw_len + 8, "raw bits overrun");
    }
    if out.len() != size {
        bail!("lzt decoded {} bytes, expected {size}", out.len());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_sums_and_keeps_used_symbols() {
        let mut f = vec![0u16; 6];
        normalize(&[1, 0, 1_000_000, 3, 0, 7], 10, &mut f);
        assert_eq!(f.iter().map(|&x| x as u32).sum::<u32>(), 1024);
        assert!(f[0] >= 1 && f[3] >= 1 && f[5] >= 1 && f[1] == 0 && f[4] == 0);
        let mut one = vec![0u16; 3];
        normalize(&[0, 5, 0], 11, &mut one);
        assert_eq!(one, vec![0, 2048, 0]);
    }

    #[test]
    fn literals_only_round_trip() {
        let data: Vec<u8> = (0..10_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let out = encode(&data, &[]);
        assert_eq!(decode(&out, data.len()).unwrap(), data);
    }

    #[test]
    fn matches_round_trip() {
        let mut data = b"hello hello hello hello, said the parrot. ".to_vec();
        let unit = data.clone();
        for _ in 0..50 {
            data.extend_from_slice(&unit);
        }
        let n = data.len() as u32;
        let tokens = [
            Token { pos: 6, len: 5, kind: 0, dist: 6 },
            Token { pos: 11, len: 12, kind: 1, dist: 6 },
            Token { pos: unit.len() as u32, len: n - unit.len() as u32, kind: 0, dist: unit.len() as u32 },
        ];
        let out = encode(&data, &tokens);
        assert_eq!(decode(&out, data.len()).unwrap(), data);
    }

    #[test]
    fn engine_codec_round_trip() {
        // Through the block codec: LZMA's parse from the token tap, then lzt.
        use crate::codec::{compress, decompress, CodecConfig};
        use crate::format::Codec;
        let mut data = Vec::with_capacity(3 << 20);
        let mut s = 5u64;
        while data.len() < 3 << 20 {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let word = [b"alpha ", b"beta  ", b"gamma ", b"delta "][(s >> 62) as usize];
            data.extend_from_slice(word);
            data.push(b'a' + (s >> 55) as u8 % 26);
        }
        for extreme in [false, true] {
            let cfg = CodecConfig { codec: Codec::Lzt, level: 7, extreme, long: false, workers: 1 };
            let comp = compress(&data, cfg).unwrap();
            assert!(comp.len() < data.len() / 4, "lzt did not compress: {}", comp.len());
            assert_eq!(decompress(&comp, Codec::Lzt, data.len() as u64).unwrap(), data);
        }
    }

    #[test]
    fn changing_alphabets_round_trip() {
        // Literals whose alphabet changes every few hundred KB, so tables meet
        // symbols they have never seen long after their first rebuilds.
        let mut data = Vec::with_capacity(3 << 20);
        let mut s = 1u64;
        for seg in 0..12u8 {
            for _ in 0..200_000 {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                data.push(seg.wrapping_mul(21) ^ ((s >> 58) as u8));
            }
        }
        let out = encode(&data, &[]);
        assert_eq!(decode(&out, data.len()).unwrap(), data);
    }
}
