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

const PBITS: u32 = 12;
const PONE: u16 = 1 << PBITS;
const SHIFT: u32 = 4;

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
    fn bit(&mut self, p: &mut u16, b: bool) {
        let bound = (self.range >> PBITS) * *p as u32;
        if !b {
            self.range = bound;
            *p += (PONE - *p) >> SHIFT;
        } else {
            self.low += bound as u64;
            self.range -= bound;
            *p -= *p >> SHIFT;
        }
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
    fn bit(&mut self, p: &mut u16) -> bool {
        let bound = (self.range >> PBITS) * *p as u32;
        let b = if self.code < bound {
            self.range = bound;
            *p += (PONE - *p) >> SHIFT;
            false
        } else {
            self.code -= bound;
            self.range -= bound;
            *p -= *p >> SHIFT;
            true
        };
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
struct Model {
    zero: [[u16; 3]; ACT],
    sign: [[u16; 3]; ACT],
    expo: [[[u16; 8]; 3]; ACT],
    mant: [[[u16; 8]; 8]; ACT],
}

impl Model {
    fn new() -> Box<Self> {
        let h = PONE / 2;
        Box::new(Model { zero: [[h; 3]; ACT], sign: [[h; 3]; ACT], expo: [[[h; 8]; 3]; ACT], mant: [[[h; 8]; 8]; ACT] })
    }
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
    let mut prev = vec![0i32; ch];
    for y in 0..h {
        for x in 0..w {
            let mut g_res = 0;
            for &c in order(ch) {
                let (pred, act, c2) = predict(px, w, ch, x, y, c, prev[c]);
                let actual = px[(y * w + x) * ch + c] as i32;
                let mut r = (actual - pred) as i8 as i32;
                if c == 1 && ch >= 3 {
                    g_res = r;
                } else if (c == 0 || c == 2) && ch >= 3 {
                    r = (r - g_res) as i8 as i32;
                }
                encode_residual(&mut e, &mut models[c], act, c2, r);
                prev[c] = r.abs().min(64);
            }
        }
    }
    e.finish()
}

fn decode(src: &[u8], w: usize, h: usize, ch: usize) -> Vec<u8> {
    let mut d = Dec::new(src);
    let mut models: Vec<Box<Model>> = (0..ch).map(|_| Model::new()).collect();
    let mut px = vec![0u8; w * h * ch];
    let mut prev = vec![0i32; ch];
    for y in 0..h {
        for x in 0..w {
            let mut g_res = 0;
            for &c in order(ch) {
                let (pred, act, c2) = predict(&px, w, ch, x, y, c, prev[c]);
                let mut r = decode_residual(&mut d, &mut models[c], act, c2);
                prev[c] = (r as i8 as i32).abs().min(64);
                if c == 1 && ch >= 3 {
                    g_res = r;
                } else if (c == 0 || c == 2) && ch >= 3 {
                    r = (r + g_res) as i8 as i32;
                }
                px[(y * w + x) * ch + c] = (pred + r) as u8;
            }
        }
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
            let ours = encode(&px, png.w, png.h, png.ch);
            let te = t.elapsed().as_secs_f64();
            let t = Instant::now();
            let back = decode(&ours, png.w, png.h, png.ch);
            let td = t.elapsed().as_secs_f64();
            assert!(back == px, "round trip failed: {}", f.display());
            let t = Instant::now();
            let mut xz = liblzma::write::XzEncoder::new(Vec::new(), 9);
            std::io::Write::write_all(&mut xz, &png.filtered).unwrap();
            let xz = xz.finish().unwrap().len();
            let tx = t.elapsed().as_secs_f64();
            Some((f.file_name().unwrap().to_string_lossy().into_owned(), d.len(), px.len(), ours.len(), xz, te, td, tx))
        })
        .collect();
    let mut tot = (0usize, 0usize, 0usize, 0usize, 0f64, 0f64, 0f64);
    for (name, size, raw, ours, xz, te, td, tx) in &rows {
        if std::env::var_os("PIX_VERBOSE").is_some() {
            println!("{name}\t{size}\t{raw}\t{ours}\t{xz}\t{:.3}\t{:.3}\t{:.3}", te, td, tx);
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
