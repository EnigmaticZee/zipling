//! Research estimator: LZMA's parse, coded by something faster to decode.
//!
//! Runs the SDK's encoder on each file with the token tap on, verifies every
//! token against the data, then counts the bits three decodable coders would
//! spend on the same tokens, against LZMA's own output and zstd -19 --long:
//!
//! * S: zstd's way. Semi-static tables per chunk: literals order-0; literal-run,
//!   match-length and distance codes with raw extra bits; the token kind.
//! * C: S with LZMA's contexts. Literals by the previous byte's top 3 bits, the
//!   first literal after a match as (byte ^ byte at rep0); kind by the previous
//!   kind; length codes by kind; distance codes by length.
//! * A: C's contexts with adaptive counts, what a nibble-based rANS decoder
//!   could do with no tables at all.
//!
//!     cargo run --release --features lztok --example lztok -- <file> ... [--fb N] [--chunk KiB]

use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Clone, Copy)]
struct Tok {
    pos: u32,
    len: u32,
    /// 0 literal, 1 match, 2..=5 repeat 0-3, 6 short repeat
    kind: u8,
    /// bytes back, for matches and repeats
    dist: u32,
}

thread_local! {
    static TOKENS: RefCell<Vec<(u32, u32, u32, [u32; 4])>> = const { RefCell::new(Vec::new()) };
}

unsafe extern "C" fn hook(pos: u32, len: u32, dist: u32, reps: *const u32, _state: u32) {
    let r = unsafe { [*reps, *reps.add(1), *reps.add(2), *reps.add(3)] };
    TOKENS.with(|t| t.borrow_mut().push((pos, len, dist, r)));
}

/// Tokens as (pos, len, kind, distance back), checked against the data under
/// the convention that verifies.
fn tokens(data: &[u8], raw: &[(u32, u32, u32, [u32; 4])]) -> Vec<Tok> {
    // The encoder keeps reps as distances in bytes (1 = one byte back), and a
    // match's `dist` is its 0-based distance plus 4.
    let mut out = Vec::with_capacity(raw.len());
    let mut bad = 0usize;
    for &(pos, len, dist, reps) in raw {
        let (kind, d) = if dist == u32::MAX {
            (0u8, 0u32)
        } else if dist < 4 {
            (if dist == 0 && len == 1 { 6 } else { 2 + dist as u8 }, reps[dist as usize])
        } else {
            (1, dist - 4 + 1)
        };
        if kind != 0 {
            let (p, l, d) = (pos as usize, len as usize, d as usize);
            let ok = d >= 1 && d <= p && p + l <= data.len() && (0..l).all(|i| data[p + i] == data[p - d + i]);
            if !ok {
                bad += 1;
            }
        }
        out.push(Tok { pos, len, kind, dist: d });
    }
    assert!(bad == 0, "{bad} tokens don't match the data: the convention is wrong");
    out
}

/// Bits for a symbol stream under semi-static coding, plus a table cost.
fn semi_static(counts: &HashMap<u32, u64>) -> f64 {
    let total: u64 = counts.values().sum();
    if total == 0 {
        return 0.0;
    }
    let bits: f64 = counts.values().map(|&c| c as f64 * ((total as f64) / c as f64).log2()).sum();
    bits + counts.len() as f64 * 6.0 + 16.0
}

/// Adaptive frequency model over an alphabet, as a decoder could keep it.
struct Adaptive {
    f: Vec<u32>,
    total: u32,
}

impl Adaptive {
    fn new(n: usize) -> Self {
        Adaptive { f: vec![1; n], total: n as u32 }
    }
    fn cost(&mut self, s: usize) -> f64 {
        let b = (self.total as f64 / self.f[s] as f64).log2();
        self.f[s] += 24;
        self.total += 24;
        if self.total > 1 << 15 {
            self.total = 0;
            for x in &mut self.f {
                *x = (*x + 1) / 2;
                self.total += *x;
            }
        }
        b
    }
}

/// zstd-style code and extra bits for a count: direct below 16, then two
/// codes per octave with the rest raw.
fn code(v: u32) -> (u32, u32) {
    if v < 16 {
        (v, 0)
    } else {
        let nb = 31 - v.leading_zeros();
        (16 + (nb - 4) * 2 + ((v >> (nb - 1)) & 1), nb - 1)
    }
}

fn dist_code(d: u32) -> (u32, u32) {
    let nb = 31 - d.leading_zeros();
    (nb, nb)
}

struct Streams {
    lit: HashMap<u32, HashMap<u32, u64>>,
    kind: HashMap<u32, HashMap<u32, u64>>,
    ll: HashMap<u32, HashMap<u32, u64>>,
    ml: HashMap<u32, HashMap<u32, u64>>,
    of: HashMap<u32, HashMap<u32, u64>>,
    extra: u64,
}

impl Streams {
    fn new() -> Self {
        Streams { lit: HashMap::new(), kind: HashMap::new(), ll: HashMap::new(), ml: HashMap::new(), of: HashMap::new(), extra: 0 }
    }
    fn bits(&self) -> f64 {
        [&self.lit, &self.kind, &self.ll, &self.ml, &self.of].iter().flat_map(|m| m.values()).map(semi_static).sum::<f64>() + self.extra as f64
    }
}

fn bump(m: &mut HashMap<u32, HashMap<u32, u64>>, ctx: u32, sym: u32) {
    *m.entry(ctx).or_default().entry(sym).or_default() += 1;
}

/// Model bits for the tokens, in chunks of `chunk` input bytes. `ctx` switches
/// on C's contexts; `adaptive` on A's counts (no chunks then).
fn model(data: &[u8], toks: &[Tok], chunk: usize, ctx: bool, adaptive: bool, rich: bool) -> f64 {
    let mut total = 0.0;
    let mut s = Streams::new();
    let mut ad: HashMap<(u8, u32), Adaptive> = HashMap::new();
    let mut acost = 0.0;
    let mut chunk_end = chunk;
    let mut prev_kind = 0u32;
    let mut after_match = false;
    let mut run = 0u32;
    let mut i = 0;
    let n = toks.len();
    let sizes = |which: u8| -> usize { match which { 0 => 256, 1 => 256, 2 => 7, 3 => 64, 4 => 64, 6 => 16, _ => 32 } };
    macro_rules! emit {
        ($which:expr, $ctxv:expr, $sym:expr) => {{
            let (which, c, sym): (u8, u32, u32) = ($which, if ctx { $ctxv } else { 0 }, $sym);
            if adaptive {
                acost += ad.entry((which, c)).or_insert_with(|| Adaptive::new(sizes(which))).cost(sym as usize);
            } else {
                match which {
                    0 | 1 => bump(&mut s.lit, c + if which == 1 { 1000 } else { 0 }, sym),
                    2 => bump(&mut s.kind, c, sym),
                    3 => bump(&mut s.ll, c, sym),
                    4 => bump(&mut s.ml, c, sym),
                    6 => bump(&mut s.of, 5000 + c, sym),
                    _ => bump(&mut s.of, c, sym),
                }
            }
        }};
    }
    while i < n {
        let t = toks[i];
        if !adaptive && t.pos as usize >= chunk_end {
            total += s.bits();
            s = Streams::new();
            chunk_end += chunk;
        }
        if t.kind == 0 {
            let p = t.pos as usize;
            let prev = if p > 0 { data[p - 1] as u32 >> 5 } else { 0 };
            let ps = if rich { (p as u32) & 3 } else { 0 };
            if after_match && ctx {
                // the repeat distance before this token is in the raw tokens; the
                // byte it predicts is what LZMA codes against
                let r0 = REP0.with(|r| r.borrow()[i]) as usize;
                let mb = if p >= r0 { data[p - r0] } else { 0 };
                // rich: the match byte's top bits as context, as LZMA's bitwise
                // matched coding effectively has
                emit!(1, if rich { (mb as u32 >> 5) * 8 + prev } else { 0 }, (data[p] ^ mb) as u32);
            } else {
                emit!(0, prev + 8 * ps, data[p] as u32);
            }
            run += 1;
            after_match = false;
            i += 1;
            continue;
        }
        // a sequence: the literal run before it, then the token
        let (c, e) = code(run);
        emit!(3, prev_kind, c);
        s.extra += e as u64;
        if adaptive {
            acost += e as f64;
        }
        run = 0;
        let ps = if rich { t.pos & 3 } else { 0 };
        emit!(2, prev_kind + 8 * ps, t.kind as u32 - 1);
        if t.kind != 6 {
            let (c, e) = code(t.len - 1);
            emit!(4, (t.kind == 1) as u32 + 2 * ps, c);
            s.extra += e as u64;
            if adaptive {
                acost += e as f64;
            }
        }
        if t.kind == 1 {
            let (c, mut e) = dist_code(t.dist);
            emit!(5, (t.len > 4) as u32 + 2 * (t.len > 16) as u32, c);
            if rich && t.dist >= 64 {
                // LZMA's align bits: the low 4 bits of a far distance, modelled
                e -= 4;
                emit!(6, 0, t.dist & 15);
            }
            s.extra += e as u64;
            if adaptive {
                acost += e as f64;
            }
        }
        prev_kind = t.kind as u32;
        after_match = true;
        i += 1;
    }
    if !adaptive {
        let (c, e) = code(run);
        emit!(3, prev_kind, c);
        s.extra += e as u64;
        total += s.bits();
    }
    if adaptive { acost } else { total }
}

thread_local! {
    static REP0: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut fb = 273i32;
    let mut chunk = 256usize << 10;
    let mut files = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--fb" => fb = it.next().unwrap().parse().unwrap(),
            "--chunk" => chunk = it.next().unwrap().parse::<usize>().unwrap() << 10,
            f => files.push(f.to_string()),
        }
    }
    lzma_sdk::set_token_hook(Some(hook));
    println!("{:<14} {:>8} | {:>8} {:>8} | {:>8} {:>8} {:>8} {:>8} | tokens  lit%", "file", "MB", "lzma", "zstd19", "S", "C", "A", "A+");
    for f in &files {
        let data = std::fs::read(f).unwrap();
        TOKENS.with(|t| t.borrow_mut().clear());
        let s = lzma_sdk::Settings { level: 9, dict_size: (data.len() as u32).clamp(1 << 16, 1 << 27), fb, mc: 0, threads: 1 };
        let (lz, _) = lzma_sdk::encode(&data, s, data.len() + 1024).unwrap();
        let raw = TOKENS.with(|t| t.borrow().clone());
        let toks = tokens(&data, &raw);
        REP0.with(|r| *r.borrow_mut() = raw.iter().map(|x| x.3[0]).collect());
        let z = {
            let mut c = zstd::bulk::Compressor::new(19).unwrap();
            c.set_parameter(zstd::zstd_safe::CParameter::WindowLog(27)).unwrap();
            c.set_parameter(zstd::zstd_safe::CParameter::EnableLongDistanceMatching(true)).unwrap();
            c.compress(&data).unwrap().len()
        };
        let lits = toks.iter().filter(|t| t.kind == 0).count();
        let mb = |bits: f64| bits / 8.0 / 1e6;
        let (ms, mc, ma, mr) = (
            model(&data, &toks, chunk, false, false, false),
            model(&data, &toks, chunk, true, false, false),
            model(&data, &toks, chunk, true, true, false),
            model(&data, &toks, chunk, true, true, true),
        );
        let name = std::path::Path::new(f).file_name().unwrap().to_string_lossy();
        println!(
            "{:<14} {:>8.2} | {:>8.3} {:>8.3} | {:>8.3} {:>8.3} {:>8.3} {:>8.3} | {:>7} {:>4.0}",
            &name[..name.len().min(14)],
            data.len() as f64 / 1e6,
            lz.len() as f64 / 1e6,
            z as f64 / 1e6,
            mb(ms),
            mb(mc),
            mb(ma),
            mb(mr),
            toks.len(),
            100.0 * lits as f64 / toks.len() as f64
        );
    }
}
