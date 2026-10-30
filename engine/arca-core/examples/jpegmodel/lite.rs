//! Light JPEG mode: JPEG's own symbols (DC size; AC run/size, EOB, ZRL) and raw
//! bits, re-coded with static Huffman tables per image and per context. No
//! coefficients are rebuilt: unpacking reads a symbol with our table and writes
//! the photo's original code for it, raw bits copied across.
//!
//! Contexts: component class x band of the next position x neighbour nonzero
//! counts (AC); component class x neighbour counts (DC).

use super::{Image, Scan};

const MAXLEN: u32 = 11;
const NCTX: usize = 9;
const CTX_DC: usize = 2 * NCTX;
const CTX_AC: usize = 2 * 8 * NCTX;

#[inline(always)]
fn bucket(x: u32) -> usize {
    if x < 3 { x as usize } else { (1 + (32 - (x - 1).leading_zeros()) as usize).min(7) }
}

const BAND: [u8; 80] = {
    let mut t = [7u8; 80];
    let mut k = 0;
    while k < 80 {
        t[k] = match k { 0..=2 => 0, 3..=5 => 1, 6..=9 => 2, 10..=14 => 3, 15..=20 => 4, 21..=27 => 5, 28..=35 => 6, _ => 7 };
        k += 1;
    }
    t
};

#[inline(always)]
fn band(k: usize) -> usize {
    if CLZ_BANDS {
        // 1, 2-3, 4-7, 8-15, 16-31, 32-63 (and 64+ for ZRL overruns): one instruction
        (63 - (k as u64 | 1).leading_zeros() as usize).min(7)
    } else {
        unsafe { *BAND.get_unchecked(k) as usize }
    }
}
const CLZ_BANDS: bool = false;

// ---------- JPEG side ----------

/// Canonical JPEG Huffman codes: (code, length) per symbol.
fn jpeg_codes(counts: &[u8; 16], vals: &[u8]) -> [(u32, u32); 256] {
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

/// MSB-first reader of entropy-coded JPEG data (stuffing removed), with a 9-bit lookahead table.
struct JReader<'a> {
    d: &'a [u8],
    pos: usize,
    acc: u64,
    n: u32,
}

struct JTable {
    fast: Vec<u16>, // 512 entries: (len << 8) | sym, len 0 = slow path
    maxcode: [i32; 18],
    valptr: [i32; 17],
    mincode: [i32; 17],
    vals: Vec<u8>,
}

impl JTable {
    fn new(counts: &[u8; 16], vals: &[u8]) -> JTable {
        let mut t = JTable { fast: vec![0; 4096], maxcode: [-1; 18], valptr: [0; 17], mincode: [0; 17], vals: vals.to_vec() };
        let mut code = 0i32;
        let mut k = 0i32;
        for l in 1..=16usize {
            let n = counts[l - 1] as i32;
            if n > 0 {
                t.valptr[l] = k;
                t.mincode[l] = code;
                for i in 0..n {
                    if l <= 12 {
                        let c = ((code + i) as usize) << (12 - l);
                        for j in 0..(1usize << (12 - l)) {
                            t.fast[c | j] = ((l as u16) << 8) | vals[(k + i) as usize] as u16;
                        }
                    }
                }
                code += n;
                k += n;
                t.maxcode[l] = code - 1;
            }
            code <<= 1;
        }
        t.maxcode[17] = i32::MAX;
        t
    }
}

impl<'a> JReader<'a> {
    #[inline(always)]
    fn fill(&mut self) {
        if self.pos + 8 <= self.d.len() {
            let w = u64::from_be_bytes(self.d[self.pos..self.pos + 8].try_into().unwrap());
            // no 0xFF byte among the 8: take as many whole bytes as fit
            let inv = !w;
            if (inv.wrapping_sub(0x0101_0101_0101_0101) & !inv & 0x8080_8080_8080_8080) == 0 {
                let take = (64 - self.n) >> 3;
                let wm = if take == 8 { w } else { w & !(u64::MAX >> (8 * take)) };
                self.acc |= wm >> self.n;
                self.pos += take as usize;
                self.n += 8 * take;
                return;
            }
        }
        while self.n <= 56 {
            let mut b = 0u8;
            if self.pos < self.d.len() {
                b = self.d[self.pos];
                if b == 0xFF {
                    if self.d.get(self.pos + 1) == Some(&0) {
                        self.pos += 2;
                    } else {
                        b = 0; // a marker: feed zeros
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
    fn bits(&mut self, k: u32) -> u32 {
        if k == 0 { return 0; }
        if self.n < k { self.fill(); }
        let v = (self.acc >> (64 - k)) as u32;
        self.acc <<= k;
        self.n -= k;
        v
    }
    #[inline(always)]
    fn sym(&mut self, t: &JTable) -> Option<u8> {
        if self.n < 16 { self.fill(); }
        let e = unsafe { *t.fast.get_unchecked((self.acc >> 52) as usize) };
        if e != 0 {
            let l = (e >> 8) as u32;
            self.acc <<= l;
            self.n -= l;
            return Some(e as u8);
        }
        let mut code = 0i32;
        for l in 1..=16usize {
            code = (code << 1) | (self.acc >> 63) as i32;
            self.acc <<= 1;
            self.n -= 1;
            if code <= t.maxcode[l] {
                return t.vals.get((t.valptr[l] + code - t.mincode[l]) as usize).copied();
            }
        }
        None
    }
    /// A symbol and its raw bits in one step: (symbol, raw value). `ac` takes the
    /// size from the low 4 bits of the symbol, DC from the whole symbol.
    #[inline(always)]
    fn sym_raw(&mut self, t: &JTable, ac: bool) -> Option<(u8, u32)> {
        if self.n < 32 { self.fill(); }
        let e = unsafe { *t.fast.get_unchecked((self.acc >> 52) as usize) };
        if e != 0 {
            let l = (e >> 8) as u32;
            let sym = e as u8;
            let s = if ac { (sym & 15) as u32 } else { sym as u32 };
            let v = self.acc << l;
            let raw = (((v >> 1) >> (63 - s)) as u32) & ((1u32 << s) - 1);
            self.acc = v << s;
            self.n -= l + s;
            return Some((sym, raw));
        }
        let sym = self.sym(t)?;
        let s = if ac { (sym & 15) as u32 } else { sym as u32 };
        if s > 16 { return None; }
        Some((sym, self.bits(s)))
    }
    /// Skips to just after the next RST marker (bits buffered are dropped).
    fn restart(&mut self) {
        self.acc = 0;
        self.n = 0;
        while self.pos + 1 < self.d.len() {
            if self.d[self.pos] == 0xFF && (0xD0..=0xD7).contains(&self.d[self.pos + 1]) {
                self.pos += 2;
                return;
            }
            self.pos += 1;
        }
    }
}

/// MSB-first JPEG writer with byte stuffing.
pub struct JWriter {
    pub out: Vec<u8>,
    acc: u64,
    n: u32,
}
impl JWriter {
    #[inline(always)]
    fn put(&mut self, v: u32, l: u32) {
        self.acc = (self.acc << l) | v as u64;
        self.n += l;
        let full = (self.n >> 5) & 1; // 1 when 32+ bits wait
        let w = (self.acc >> (self.n - 32 * full)) as u32;
        let inv = !w;
        let has_ff = (inv.wrapping_sub(0x0101_0101) & !inv & 0x8080_8080) != 0;
        if full == 1 && has_ff {
            self.n -= 32;
            for b in w.to_be_bytes() {
                self.out.push(b);
                if b == 0xFF { self.out.push(0); }
            }
            return;
        }
        if self.out.capacity() - self.out.len() < 8 { self.out.reserve(1 << 16); }
        unsafe {
            let l = self.out.len();
            std::ptr::write_unaligned(self.out.as_mut_ptr().add(l) as *mut u32, w.to_be());
            self.out.set_len(l + 4 * full as usize);
        }
        self.n -= 32 * full;
    }
    fn pad(&mut self) {
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

// ---------- our side ----------

/// LSB-first bit writer for the light stream: stores 8 bytes every time and
/// advances by the whole bytes filled (the buffer is sized up front).
struct LWriter {
    out: Vec<u8>,
    len: usize,
    acc: u64,
    n: u32,
}
impl LWriter {
    fn with_capacity(c: usize) -> LWriter {
        LWriter { out: vec![0; c + 16], len: 0, acc: 0, n: 0 }
    }
    #[inline(always)]
    fn put(&mut self, v: u32, l: u32) {
        self.acc |= (v as u64) << self.n;
        self.n += l;
        unsafe { std::ptr::write_unaligned(self.out.as_mut_ptr().add(self.len) as *mut u64, self.acc.to_le()) };
        let bytes = self.n >> 3;
        self.len += bytes as usize;
        self.acc = if bytes == 8 { 0 } else { self.acc >> (8 * bytes) };
        self.n &= 7;
    }
    fn finish(mut self) -> Vec<u8> {
        unsafe { std::ptr::write_unaligned(self.out.as_mut_ptr().add(self.len) as *mut u64, self.acc.to_le()) };
        self.len += ((self.n + 7) >> 3) as usize;
        self.out.truncate(self.len);
        self.out.extend_from_slice(&[0; 16]);
        self.out
    }
}

/// Length-limited Huffman code lengths: sort, two-queue Huffman (no heap), then
/// JPEG's Annex K.3 adjustment to cap the lengths at MAXLEN.
fn lengths(cnt: &[u32; 256]) -> [u8; 256] {
    let mut len = [0u8; 256];
    let mut syms = [0u64; 256];
    let mut m = 0usize;
    for (s, &c) in cnt.iter().enumerate() {
        if c > 0 {
            syms[m] = ((c as u64) << 8) | s as u64;
            m += 1;
        }
    }
    if m == 0 {
        return len;
    }
    if m == 1 {
        len[(syms[0] & 0xFF) as usize] = 1;
        return len;
    }
    let syms = &mut syms[..m];
    syms.sort_unstable();
    // two queues: leaves (sorted) and internal nodes (created in order)
    let mut w = [0u64; 512];
    let mut parent = [0u16; 512];
    for i in 0..m {
        w[i] = syms[i] >> 8;
    }
    let (mut li, mut ni, mut nn) = (0usize, m, m);
    let mut pick = |li: &mut usize, ni: &mut usize, nn: usize, w: &[u64; 512]| -> usize {
        if *li < m && (*ni >= nn || w[*li] <= w[*ni]) {
            *li += 1;
            *li - 1
        } else {
            *ni += 1;
            *ni - 1
        }
    };
    while nn < 2 * m - 1 {
        let a = pick(&mut li, &mut ni, nn, &w);
        let b = pick(&mut li, &mut ni, nn, &w);
        w[nn] = w[a] + w[b];
        parent[a] = nn as u16;
        parent[b] = nn as u16;
        nn += 1;
    }
    // depths from the root down (parents are created after children)
    let mut depth = [0u8; 512];
    for i in (0..nn - 1).rev() {
        depth[i] = depth[parent[i] as usize] + 1;
    }
    // number of codes per length, capped (Annex K.3 style)
    let mut bits = [0u32; 64];
    for i in 0..m {
        bits[depth[i] as usize] += 1;
    }
    let mut i = 63;
    while i > MAXLEN as usize {
        while bits[i] > 0 {
            let mut j = i - 2;
            while bits[j] == 0 {
                j -= 1;
            }
            bits[i] -= 2;
            bits[i - 1] += 1;
            bits[j + 1] += 2;
            bits[j] -= 1;
        }
        i -= 1;
    }
    // shortest codes to the most frequent symbols (syms is ascending)
    let mut k = m;
    for l in 1..=MAXLEN as usize {
        for _ in 0..bits[l] {
            k -= 1;
            len[(syms[k] & 0xFF) as usize] = l as u8;
        }
    }
    len
}

/// Canonical codes from lengths, bit-reversed for LSB-first writing (no decode table).
fn canon_codes(len: &[u8; 256]) -> [(u16, u8); 256] {
    let mut codes = [(0u16, 0u8); 256];
    // symbols by length, then canonical order
    let mut order: Vec<(u8, u8)> = (0..256).filter(|&s| len[s] > 0).map(|s| (len[s], s as u8)).collect();
    order.sort_unstable();
    let mut code = 0u32;
    let mut prev = order.first().map_or(0, |x| x.0);
    for &(l, s) in &order {
        code <<= (l - prev) as u32;
        prev = l;
        codes[s as usize] = ((code.reverse_bits() >> (32 - l as u32)) as u16, l);
        code += 1;
    }
    codes
}

/// Canonical codes from lengths, bit-reversed for LSB-first writing; and the decode table.
fn canon(len: &[u8; 256]) -> ([(u16, u8); 256], Vec<u16>) {
    let mut codes = [(0u16, 0u8); 256];
    let mut lut = vec![0u16; 1 << MAXLEN];
    let mut code = 0u32;
    for l in 1..=MAXLEN as u8 {
        for s in 0..256 {
            if len[s] == l {
                let rev = (code.reverse_bits() >> (32 - l as u32)) as u16;
                codes[s] = (rev, l);
                let step = 1usize << l;
                let mut j = rev as usize;
                while j < (1 << MAXLEN) {
                    lut[j] = ((l as u16) << 8) | s as u16;
                    j += step;
                }
                code += 1;
            }
        }
        code <<= 1;
    }
    (codes, lut)
}

struct Ev {
    ctx: u16,
    sym: u8,
    raw: u16,
    rl: u8,
}

/// Blocks of a scan in order: (component, index in scan, block index, last of MCU, up, left).
struct Blocks<'a> {
    img: &'a Image,
    sc: &'a Scan,
    single: bool,
    mcux: usize,
    mcuy: usize,
    cw: usize,
    ch: usize,
    mx: usize,
    my: usize,
    si: usize,
    x: usize,
    y: usize,
    done: bool,
}

impl<'a> Blocks<'a> {
    fn new(img: &'a Image, sc: &'a Scan) -> Blocks<'a> {
        let single = sc.comps.len() == 1;
        let c = &img.comps[sc.comps[0].0];
        Blocks {
            img, sc, single,
            mcux: (img.width + 8 * img.hmax - 1) / (8 * img.hmax),
            mcuy: (img.height + 8 * img.vmax - 1) / (8 * img.vmax),
            cw: (img.width * c.h + 8 * img.hmax - 1) / (8 * img.hmax),
            ch: (img.height * c.v + 8 * img.vmax - 1) / (8 * img.vmax),
            mx: 0, my: 0, si: 0, x: 0, y: 0, done: false,
        }
    }
}

impl<'a> Iterator for Blocks<'a> {
    type Item = (usize, usize, usize, bool, bool, bool);
    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.done { return None; }
        if self.single {
            let ci = self.sc.comps[0].0;
            let c = &self.img.comps[ci];
            let (bx, by) = (self.x, self.y);
            let item = (ci, 0, by * c.bw + bx, true, by > 0, bx > 0);
            self.x += 1;
            if self.x == self.cw { self.x = 0; self.y += 1; if self.y == self.ch { self.done = true; } }
            return Some(item);
        }
        let ci = self.sc.comps[self.si].0;
        let c = &self.img.comps[ci];
        let (bx, by) = (self.mx * c.h + self.x, self.my * c.v + self.y);
        let last = self.si + 1 == self.sc.comps.len() && self.y + 1 == c.v && self.x + 1 == c.h;
        let item = (ci, self.si, by * c.bw + bx, last, by > 0, bx > 0);
        self.x += 1;
        if self.x == c.h {
            self.x = 0;
            self.y += 1;
            if self.y == c.v {
                self.y = 0;
                self.si += 1;
                if self.si == self.sc.comps.len() {
                    self.si = 0;
                    self.mx += 1;
                    if self.mx == self.mcux {
                        self.mx = 0;
                        self.my += 1;
                        if self.my == self.mcuy { self.done = true; }
                    }
                }
            }
        }
        Some(item)
    }
}

/// Calls `f(component, index in scan, block index, last block of its MCU)` in scan
/// order; stops early when `f` returns false.
#[inline(always)]
fn for_blocks(img: &Image, sc: &Scan, mut f: impl FnMut(usize, usize, usize, bool, bool, bool) -> bool) -> bool {
    if sc.comps.len() == 1 {
        let (ci, _, _) = sc.comps[0];
        let c = &img.comps[ci];
        let cw = (img.width * c.h + 8 * img.hmax - 1) / (8 * img.hmax);
        let ch = (img.height * c.v + 8 * img.vmax - 1) / (8 * img.vmax);
        for by in 0..ch {
            for bx in 0..cw {
                if !f(ci, 0, by * c.bw + bx, true, by > 0, bx > 0) { return false; }
            }
        }
    } else {
        let mcux = (img.width + 8 * img.hmax - 1) / (8 * img.hmax);
        let mcuy = (img.height + 8 * img.vmax - 1) / (8 * img.vmax);
        let nsc = sc.comps.len();
        for my in 0..mcuy {
            for mx in 0..mcux {
                for (si, &(ci, _, _)) in sc.comps.iter().enumerate() {
                    let c = &img.comps[ci];
                    for y in 0..c.v {
                        for x in 0..c.h {
                            let last = si + 1 == nsc && y + 1 == c.v && x + 1 == c.h;
                            if !f(ci, si, (my * c.v + y) * c.bw + mx * c.h + x, last, my * c.v + y > 0, mx * c.h + x > 0) { return false; }
                        }
                    }
                }
            }
        }
    }
    true
}

#[inline(always)]
fn nctx(cnt: &[u8], bw: usize, b: usize, up: bool, lf: bool) -> usize {
    match (up, lf) {
        (true, true) => bucket((cnt[b - bw] as u32 + cnt[b - 1] as u32 + 1) / 2),
        (true, false) => bucket(cnt[b - bw] as u32),
        (false, true) => bucket(cnt[b - 1] as u32),
        _ => 8,
    }
}

/// Packs the scans of a baseline JPEG; `None` if a scan doesn't parse or re-create exactly.
pub static T_PARSE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static T_TAB: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static T_WRITE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub static T_READ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Reads every symbol and raw bits of the scans and nothing else (a floor for parsing).
pub fn read_only(d: &[u8], img: &Image) -> u64 {
    let t0 = std::time::Instant::now();
    let mut sum = 0u64;
    for sc in &img.scans {
        let tabs: Vec<(JTable, JTable)> = sc.comps.iter().map(|&(_, td, ta)| {
            let (dc, ac) = (&img.dc[&td], &img.ac[&ta]);
            (JTable::new(&dc.0, &dc.1), JTable::new(&ac.0, &ac.1))
        }).collect();
        let mut r = JReader { d: &d[..sc.end], pos: sc.start, acc: 0, n: 0 };
        let mut mcu = 0usize;
        let mut first = true;
        for_blocks(img, sc, |_ci, si, _b, end_mcu, _up, _lf| {
            if img.dri > 0 && first && mcu > 0 && mcu % img.dri == 0 { r.restart(); }
            first = false;
            let Some((_s, raw)) = r.sym_raw(&tabs[si].0, false) else { return false };
            sum += raw as u64;
            let mut k = 1usize;
            while k < 64 {
                let Some((rs, raw)) = r.sym_raw(&tabs[si].1, true) else { return false };
                sum += raw as u64;
                if rs & 15 == 0 { if rs == 0xF0 { k += 16; continue; } break; }
                k += (rs >> 4) as usize + 1;
            }
            if end_mcu { mcu += 1; first = true; }
            true
        });
    }
    T_READ.fetch_add(t0.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
    sum
}

pub fn pack(d: &[u8], img: &Image) -> Option<Vec<u8>> {
    let t0 = std::time::Instant::now();
    let mut evs: Vec<u16> = Vec::with_capacity(2 * d.len());
    let mut rw = LWriter::with_capacity(d.len() + 64);
    let mut cnt_ac = vec![[0u32; 256]; CTX_AC];
    let mut cnt_dc = vec![[0u32; 256]; CTX_DC];
    let mut counts: Vec<Vec<u8>> = img.comps.iter().map(|c| vec![0u8; c.bw * c.bh]).collect();
    for sc in &img.scans {
        let tabs: Vec<(JTable, JTable)> = sc.comps.iter().map(|&(_, td, ta)| {
            let (dc, ac) = (&img.dc[&td], &img.ac[&ta]);
            (JTable::new(&dc.0, &dc.1), JTable::new(&ac.0, &ac.1))
        }).collect();
        let mut r = JReader { d: &d[..sc.end], pos: sc.start, acc: 0, n: 0 };
        let mut mcu = 0usize;
        let mut first = true;
        for (ci, si, b, end_mcu, up, lf) in Blocks::new(img, sc) {
            if img.dri > 0 && first && mcu > 0 && mcu % img.dri == 0 {
                r.restart();
            }
            first = false;
            let cc = ci.min(1);
            let bw = img.comps[ci].bw;
            let nc = nctx(&counts[ci], bw, b, up, lf);
            // DC
            let Some((s, raw)) = r.sym_raw(&tabs[si].0, false) else { return None };
            let s = s as u32;
            if s > 15 { return None; }
            let ctx = cc * NCTX + nc;
            unsafe { *cnt_dc.get_unchecked_mut(ctx).get_unchecked_mut(s as usize) += 1; }
            evs.push(((ctx as u16) << 8) | s as u16);
            rw.put(raw, s);
            // AC
            let mut k = 1usize;
            let mut n = 0u32;
            while k < 64 {
                let Some((rs, raw)) = r.sym_raw(&tabs[si].1, true) else { return None };
                let rs = rs as u32;
                let ctx = CTX_DC + (cc * 8 + band(k)) * NCTX + nc;
                let (run, sz) = (rs >> 4, rs & 15);
                unsafe { *cnt_ac.get_unchecked_mut(ctx - CTX_DC).get_unchecked_mut(rs as usize) += 1; }
                evs.push(((ctx as u16) << 8) | rs as u16);
                rw.put(raw, sz);
                if sz == 0 {
                    if run == 15 { k += 16; continue; }
                    break; // EOB
                }
                k += run as usize + 1;
                n += 1;
            }
            if k > 64 { return None; }
            counts[ci][b] = n.min(63) as u8;
            if end_mcu {
                mcu += 1;
                first = true;
            }
        }
    }
    T_PARSE.fetch_add(t0.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
    let t1 = std::time::Instant::now();
    finish_pack(&evs, &cnt_dc, &cnt_ac, rw.finish(), t1)
}

fn finish_pack(evs: &[u16], cnt_dc: &[[u32; 256]], cnt_ac: &[[u32; 256]], raw: Vec<u8>, t1: std::time::Instant) -> Option<Vec<u8>> {
    // tables
    let mut out = Vec::new();
    let mut codes: Vec<[(u16, u8); 256]> = Vec::with_capacity(CTX_DC + CTX_AC);
    for c in cnt_dc.iter().chain(cnt_ac.iter()) {
        let used = c.iter().any(|&x| x > 0);
        out.push(used as u8);
        if !used {
            codes.push([(0, 0); 256]);
            continue;
        }
        let len = lengths(c);
        // DHT-style: counts per length (1..=11), then symbols in length order
        let mut per = [0u8; MAXLEN as usize];
        for &l in len.iter() { if l > 0 { per[l as usize - 1] += 1; } }
        out.extend_from_slice(&per);
        for l in 1..=MAXLEN as u8 {
            for s in 0..256 { if len[s] == l { out.push(s as u8); } }
        }
        codes.push(canon_codes(&len));
    }
    let tlen = out.len();
    T_TAB.fetch_add(t1.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
    // codes only: at most 11 bits per event
    let mut w = LWriter::with_capacity(evs.len() * 2 + 64);
    for &e in evs {
        let (c, l) = unsafe { *codes.get_unchecked((e >> 8) as usize).get_unchecked((e & 0xFF) as usize) };
        w.put(c as u32, l as u32);
    }
    let body = w.finish();
    T_WRITE.fetch_add(t1.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
    let mut res = Vec::with_capacity(8 + tlen + body.len() + raw.len());
    res.extend_from_slice(&(tlen as u32).to_le_bytes());
    res.extend_from_slice(&out);
    res.extend_from_slice(&(body.len() as u32).to_le_bytes());
    res.extend_from_slice(&body);
    res.extend_from_slice(&raw);
    Some(res)
}

/// Re-creates the scan data of every scan (concatenated in scan order).
pub static SYMS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static NOWRITE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

pub fn unpack(data: &[u8], img: &Image) -> Vec<Vec<u8>> {

    let tlen = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    let mut p = 4;
    let t = &data[..4 + tlen];
    let mut luts: Vec<Vec<u16>> = Vec::with_capacity(CTX_DC + CTX_AC);
    let mut hot: Vec<u16> = vec![0; (CTX_DC + CTX_AC) * 256];
    for _ in 0..CTX_DC + CTX_AC {
        let used = t[p];
        p += 1;
        if used == 0 {
            luts.push(Vec::new());
            continue;
        }
        let per = &t[p..p + MAXLEN as usize];
        p += MAXLEN as usize;
        let mut len = [0u8; 256];
        for l in 1..=MAXLEN as usize {
            for _ in 0..per[l - 1] {
                len[t[p] as usize] = l as u8;
                p += 1;
            }
        }
        let full = canon(&len).1;
        let ci = luts.len();
        for i in 0..256 {
            if (full[i] >> 8) <= 8 { hot[ci * 256 + i] = full[i]; }
        }
        luts.push(full);
    }
    let bl = u32::from_le_bytes(data[4 + tlen..8 + tlen].try_into().unwrap()) as usize;
    let body = &data[8 + tlen..8 + tlen + bl];
    let rawb = &data[8 + tlen + bl..];
    let (mut acc, mut n, mut bp) = (0u64, 0u32, 0usize);
    let (mut racc, mut rn, mut rp) = (0u64, 0u32, 0usize);
    let mut counts: Vec<Vec<u8>> = img.comps.iter().map(|c| vec![0u8; c.bw * c.bh]).collect();
    let mut outs = Vec::new();
    for sc in &img.scans {
        let jt: Vec<([(u32, u32); 256], [(u32, u32); 256])> = sc.comps.iter().map(|&(_, td, ta)| {
            let (dc, ac) = (&img.dc[&td], &img.ac[&ta]);
            (jpeg_codes(&dc.0, &dc.1), jpeg_codes(&ac.0, &ac.1))
        }).collect();
        let mut w = JWriter { out: Vec::with_capacity(sc.end - sc.start + 64), acc: 0, n: 0 };
        let mut mcu = 0usize;
        let mut rst = 0u8;
        let mut first = true;
        for (ci, si, b, end_mcu, up, lf) in Blocks::new(img, sc) {
            if img.dri > 0 && first && mcu > 0 && mcu % img.dri == 0 {
                w.pad();
                w.out.push(0xFF);
                w.out.push(0xD0 + rst);
                rst = (rst + 1) & 7;
            }
            first = false;
            let cc = ci.min(1);
            let bw = img.comps[ci].bw;
            let nc = nctx(&counts[ci], bw, b, up, lf);
            let (jdc, jac) = (&jt[si].0, &jt[si].1);
            // DC
            {
                let v = unsafe { std::ptr::read_unaligned(body.as_ptr().add(bp) as *const u64) };
                acc |= u64::from_le(v) << n;
                let take = (63 - n) >> 3;
                bp += take as usize;
                n += take << 3;
            }
            let ctx = cc * NCTX + nc;
            let mut e = hot[ctx * 256 + (acc & 255) as usize];
            if e == 0 { e = luts[ctx][(acc & ((1 << MAXLEN) - 1)) as usize]; }
            let l = (e >> 8) as u32;
            let s = (e & 0xFF) as u32;
            acc >>= l;
            n -= l;
            {
                let v = unsafe { std::ptr::read_unaligned(rawb.as_ptr().add(rp) as *const u64) };
                racc |= u64::from_le(v) << rn;
                let take = (63 - rn) >> 3;
                rp += take as usize;
                rn += take << 3;
            }
            let raw = (racc & ((1u64 << s) - 1)) as u32;
            racc >>= s;
            rn -= s;
            let (c, cl) = jdc[s as usize];
            w.put((c << s) | raw, cl + s);
            // AC
            let mut k = 1usize;
            let mut nz = 0u32;
            let base = CTX_DC + cc * 8 * NCTX + nc;
            while k < 64 {
                {
                    // branchless refill: top up to 56-63 bits every symbol
                    let v = unsafe { std::ptr::read_unaligned(body.as_ptr().add(bp) as *const u64) };
                    acc |= u64::from_le(v) << n;
                    let take = (63 - n) >> 3;
                    bp += take as usize;
                    n += take << 3;
                }
                let ctx = base + band(k) * NCTX;
                let mut e = unsafe { *hot.get_unchecked(ctx * 256 + (acc & 255) as usize) };
                if e == 0 {
                    e = unsafe { *luts.get_unchecked(ctx).get_unchecked((acc & ((1 << MAXLEN) - 1)) as usize) };
                }
                let l = (e >> 8) as u32;
                let rs = (e & 0xFF) as u32;
                acc >>= l;
                n -= l;
                let sz = rs & 15;
                if rn < 16 {
                    let v = unsafe { std::ptr::read_unaligned(rawb.as_ptr().add(rp) as *const u64) };
                    racc |= u64::from_le(v) << rn;
                    let take = (63 - rn) >> 3;
                    rp += take as usize;
                    rn += take << 3;
                }
                let raw = (racc & ((1u64 << sz) - 1)) as u32;
                racc >>= sz;
                rn -= sz;
                let (c, cl) = unsafe { *jac.get_unchecked(rs as usize) };
                w.put((c << sz) | raw, cl + sz);
                if sz == 0 {
                    if rs == 0xF0 { k += 16; continue; }
                    break;
                }
                k += (rs >> 4) as usize + 1;
                nz += 1;
            }
            counts[ci][b] = nz.min(63) as u8;
            if end_mcu {
                mcu += 1;
                first = true;
            }
        }
        w.pad();
        outs.push(w.out);
    }
    outs
}

// ---------- tight unpack ----------

/// Flushes `w` (32 bits) with byte stuffing; the rare path.
#[cold]
#[inline(never)]
unsafe fn put_stuffed(w: u32, wp: &mut *mut u8) {
    for b in w.to_be_bytes() {
        **wp = b;
        *wp = wp.add(1);
        if b == 0xFF {
            **wp = 0;
            *wp = wp.add(1);
        }
    }
}

/// The same as `unpack`, written as one loop over locals.
pub fn unpack_tight(data: &[u8], img: &Image) -> Vec<Vec<u8>> {
    let tlen = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    let t = &data[..4 + tlen];
    let mut p = 4;
    let mut luts: Vec<Vec<u16>> = Vec::with_capacity(CTX_DC + CTX_AC);
    let mut hot: Vec<u16> = vec![0; (CTX_DC + CTX_AC) * 256];
    for ci in 0..CTX_DC + CTX_AC {
        let used = t[p];
        p += 1;
        if used == 0 {
            luts.push(Vec::new());
            continue;
        }
        let per = &t[p..p + MAXLEN as usize];
        p += MAXLEN as usize;
        let mut len = [0u8; 256];
        for l in 1..=MAXLEN as usize {
            for _ in 0..per[l - 1] {
                len[t[p] as usize] = l as u8;
                p += 1;
            }
        }
        let full = canon(&len).1;
        for i in 0..256 {
            if (full[i] >> 8) <= 8 { hot[ci * 256 + i] = full[i]; }
        }
        luts.push(full);
    }
    let bl = u32::from_le_bytes(data[4 + tlen..8 + tlen].try_into().unwrap()) as usize;
    let body = &data[8 + tlen..8 + tlen + bl];
    let rawb = &data[8 + tlen + bl..];
    let mut counts: Vec<Vec<u8>> = img.comps.iter().map(|c| vec![0u8; c.bw * c.bh]).collect();
    let mut outs = Vec::new();
    unsafe {
        let (mut cacc, mut cn, mut cp) = (0u64, 0u32, body.as_ptr());
        let (mut racc, mut rn, mut rp) = (0u64, 0u32, rawb.as_ptr());
        let hotp = hot.as_ptr();
        for sc in &img.scans {
            let jt: Vec<([(u32, u32); 256], [(u32, u32); 256])> = sc.comps.iter().map(|&(_, td, ta)| {
                let (dc, ac) = (&img.dc[&td], &img.ac[&ta]);
                (jpeg_codes(&dc.0, &dc.1), jpeg_codes(&ac.0, &ac.1))
            }).collect();
            // output: the scan's size is known to the caller; leave room for growth
            let cap = (sc.end - sc.start) * 2 + 4096;
            let mut out: Vec<u8> = Vec::with_capacity(cap);
            let base_out = out.as_mut_ptr();
            let mut wp = base_out;
            let (mut wacc, mut wn) = (0u64, 0u32);
            macro_rules! put {
                ($v:expr, $l:expr) => {{
                    wacc = (wacc << $l) | $v as u64;
                    wn += $l;
                    if wn >= 32 {
                        wn -= 32;
                        let w = (wacc >> wn) as u32;
                        let inv = !w;
                        if (inv.wrapping_sub(0x0101_0101) & !inv & 0x8080_8080) == 0 {
                            std::ptr::write_unaligned(wp as *mut u32, w.to_be());
                            wp = wp.add(4);
                        } else {
                            put_stuffed(w, &mut wp);
                        }
                    }
                }};
            }
            macro_rules! refill_c {
                () => {{
                    let v = std::ptr::read_unaligned(cp as *const u64);
                    cacc |= u64::from_le(v) << cn;
                    let take = (63 - cn) >> 3;
                    cp = cp.add(take as usize);
                    cn += take << 3;
                }};
            }
            macro_rules! refill_r {
                () => {{
                    let v = std::ptr::read_unaligned(rp as *const u64);
                    racc |= u64::from_le(v) << rn;
                    let take = (63 - rn) >> 3;
                    rp = rp.add(take as usize);
                    rn += take << 3;
                }};
            }
            let mut mcu = 0usize;
            let mut rst = 0u8;
            let mut first = true;
            for (ci, si, b, end_mcu, up, lf) in Blocks::new(img, sc) {
                if img.dri > 0 && first && mcu > 0 && mcu % img.dri == 0 {
                    // pad with ones, then the marker
                    while wn >= 8 {
                        wn -= 8;
                        let by = (wacc >> wn) as u8;
                        *wp = by; wp = wp.add(1);
                        if by == 0xFF { *wp = 0; wp = wp.add(1); }
                    }
                    if wn > 0 {
                        let k = 8 - wn;
                        let by = (((wacc << k) | ((1 << k) - 1)) & 0xFF) as u8;
                        *wp = by; wp = wp.add(1);
                        if by == 0xFF { *wp = 0; wp = wp.add(1); }
                        wn = 0;
                    }
                    wacc = 0;
                    *wp = 0xFF; wp = wp.add(1);
                    *wp = 0xD0 + rst; wp = wp.add(1);
                    rst = (rst + 1) & 7;
                }
                first = false;
                let cc = ci.min(1);
                let bw = img.comps[ci].bw;
                let cnt = counts.get_unchecked_mut(ci);
                let nc = nctx(cnt, bw, b, up, lf);
                let (jdc, jac) = (&jt.get_unchecked(si).0, &jt.get_unchecked(si).1);
                // DC
                refill_c!();
                let ctx = cc * NCTX + nc;
                let mut e = *hotp.add(ctx * 256 + (cacc & 255) as usize);
                if e == 0 { e = *luts.get_unchecked(ctx).get_unchecked((cacc & ((1 << MAXLEN) - 1)) as usize); }
                let l = (e >> 8) as u32;
                let s = (e & 0xFF) as u32;
                cacc >>= l;
                cn -= l;
                refill_r!();
                let raw = (racc & ((1u64 << s) - 1)) as u32;
                racc >>= s;
                rn -= s;
                let (c, cl) = *jdc.get_unchecked(s as usize);
                put!((c << s) | raw, cl + s);
                // AC
                let row = CTX_DC + cc * 8 * NCTX + nc;
                let mut k = 1usize;
                let mut nz = 0u32;
                while k < 64 {
                    refill_c!();
                    let ctx = row + band(k) * NCTX;
                    let mut e = *hotp.add(ctx * 256 + (cacc & 255) as usize);
                    if e == 0 { e = *luts.get_unchecked(ctx).get_unchecked((cacc & ((1 << MAXLEN) - 1)) as usize); }
                    let l = (e >> 8) as u32;
                    let rs = (e & 0xFF) as u32;
                    cacc >>= l;
                    cn -= l;
                    let sz = rs & 15;
                    if rn < 16 { refill_r!(); }
                    let raw = (racc & ((1u64 << sz) - 1)) as u32;
                    racc >>= sz;
                    rn -= sz;
                    let (c, cl) = *jac.get_unchecked(rs as usize);
                    put!((c << sz) | raw, cl + sz);
                    if sz == 0 {
                        if rs == 0xF0 { k += 16; continue; }
                        break;
                    }
                    k += (rs >> 4) as usize + 1;
                    nz += 1;
                }
                *cnt.get_unchecked_mut(b) = nz.min(63) as u8;
                if end_mcu {
                    mcu += 1;
                    first = true;
                }
            }
            // final pad
            while wn >= 8 {
                wn -= 8;
                let by = (wacc >> wn) as u8;
                *wp = by; wp = wp.add(1);
                if by == 0xFF { *wp = 0; wp = wp.add(1); }
            }
            if wn > 0 {
                let k = 8 - wn;
                let by = (((wacc << k) | ((1 << k) - 1)) & 0xFF) as u8;
                *wp = by; wp = wp.add(1);
                if by == 0xFF { *wp = 0; wp = wp.add(1); }
            }
            let n = wp.offset_from(base_out) as usize;
            assert!(n <= cap);
            out.set_len(n);
            outs.push(out);
        }
    }
    outs
}


// ---------- tight pack ----------

/// Slow path of the JPEG reader: a code longer than the 12-bit table, bit by bit.
#[cold]
#[inline(never)]
fn slow_sym(t: &JTable, acc: &mut u64, n: &mut u32) -> Option<u8> {
    let mut code = 0i32;
    for l in 1..=16usize {
        code = (code << 1) | (*acc >> 63) as i32;
        *acc <<= 1;
        *n -= 1;
        if code <= t.maxcode[l] {
            return t.vals.get((t.valptr[l] + code - t.mincode[l]) as usize).copied();
        }
    }
    None
}

/// Refills the JPEG reader byte by byte (stuffing, markers); the rare path.
#[cold]
#[inline(never)]
fn slow_fill(d: &[u8], pos: &mut usize, acc: &mut u64, n: &mut u32) {
    while *n <= 56 {
        let mut b = 0u8;
        if *pos < d.len() {
            b = d[*pos];
            if b == 0xFF {
                if d.get(*pos + 1) == Some(&0) { *pos += 2; } else { b = 0; }
            } else {
                *pos += 1;
            }
        }
        *acc |= (b as u64) << (56 - *n);
        *n += 8;
    }
}

const ALWAYS_FILL: bool = false;

/// `pack` with the parse written as one loop over locals.
pub fn pack_tight(d: &[u8], img: &Image) -> Option<Vec<u8>> {
    let t0 = std::time::Instant::now();
    let mut evs: Vec<u16> = Vec::with_capacity(2 * d.len() + 64);
    let mut rawbuf: Vec<u8> = vec![0; d.len() + 64];
    let mut cnt_ac = vec![[0u32; 256]; CTX_AC];
    let mut cnt_dc = vec![[0u32; 256]; CTX_DC];
    let mut counts: Vec<Vec<u8>> = img.comps.iter().map(|c| vec![0u8; c.bw * c.bh]).collect();
    let (mut racc, mut rn, mut rlen) = (0u64, 0u32, 0usize);
    let mut nev = 0usize;
    unsafe {
        let mut evp = evs.as_mut_ptr();
        let rbp = rawbuf.as_mut_ptr();
        for sc in &img.scans {
            let tabs: Vec<(JTable, JTable)> = sc.comps.iter().map(|&(_, td, ta)| {
                let (dc, ac) = (&img.dc[&td], &img.ac[&ta]);
                (JTable::new(&dc.0, &dc.1), JTable::new(&ac.0, &ac.1))
            }).collect();
            let dd = &d[..sc.end];
            let (mut acc, mut n, mut pos) = (0u64, 0u32, sc.start);
            macro_rules! fill {
                () => {{
                    let mut done = false;
                    if pos + 8 <= dd.len() {
                        let w = u64::from_be_bytes(std::ptr::read_unaligned(dd.as_ptr().add(pos) as *const [u8; 8]));
                        let inv = !w;
                        if (inv.wrapping_sub(0x0101_0101_0101_0101) & !inv & 0x8080_8080_8080_8080) == 0 {
                            let take = (64 - n) >> 3;
                            let wm = if take == 8 { w } else { w & !(u64::MAX >> (8 * take)) };
                            acc |= wm >> n;
                            pos += take as usize;
                            n += 8 * take;
                            done = true;
                        }
                    }
                    if !done { slow_fill(dd, &mut pos, &mut acc, &mut n); }
                }};
            }
            macro_rules! sym_raw {
                ($t:expr, $ac:expr) => {{
                    if ALWAYS_FILL { fill!(); } else if n < 32 { fill!(); }
                    let e = *$t.fast.as_ptr().add((acc >> 52) as usize);
                    if e != 0 {
                        let l = (e >> 8) as u32;
                        let sym = e as u8;
                        let s = if $ac { (sym & 15) as u32 } else { sym as u32 };
                        let v = acc << l;
                        let raw = (((v >> 1) >> (63 - s)) as u32) & ((1u32 << s) - 1);
                        acc = v << s;
                        n -= l + s;
                        Some((sym, raw))
                    } else {
                        match slow_sym($t, &mut acc, &mut n) {
                            None => None,
                            Some(sym) => {
                                let s = if $ac { (sym & 15) as u32 } else { sym as u32 };
                                if s > 16 { None } else {
                                    if n < 32 { fill!(); }
                                    let raw = if s == 0 { 0 } else { (acc >> (64 - s)) as u32 };
                                    acc <<= s;
                                    n -= s;
                                    Some((sym, raw))
                                }
                            }
                        }
                    }
                }};
            }
            macro_rules! rput {
                ($v:expr, $l:expr) => {{
                    racc |= ($v as u64) << rn;
                    rn += $l;
                    std::ptr::write_unaligned(rbp.add(rlen) as *mut u64, racc.to_le());
                    let bytes = rn >> 3;
                    rlen += bytes as usize;
                    racc = if bytes == 8 { 0 } else { racc >> (8 * bytes) };
                    rn &= 7;
                }};
            }
            let mut mcu = 0usize;
            let mut first = true;
            for (ci, si, b, end_mcu, up, lf) in Blocks::new(img, sc) {
                if img.dri > 0 && first && mcu > 0 && mcu % img.dri == 0 {
                    acc = 0;
                    n = 0;
                    while pos + 1 < dd.len() {
                        if dd[pos] == 0xFF && (0xD0..=0xD7).contains(&dd[pos + 1]) { pos += 2; break; }
                        pos += 1;
                    }
                }
                first = false;
                // room for a whole block's events (at most 1 + 63 + a few ZRL)
                if nev + 80 > evs.capacity() {
                    evs.set_len(nev);
                    evs.reserve(evs.capacity());
                    evp = evs.as_mut_ptr();
                }
                let cc = ci.min(1);
                let bw = img.comps[ci].bw;
                let cnt = counts.get_unchecked_mut(ci);
                let nc = nctx(cnt, bw, b, up, lf);
                let (tdc, tac) = (&tabs.get_unchecked(si).0, &tabs.get_unchecked(si).1);
                // DC
                let (s, raw) = sym_raw!(tdc, false)?;
                let s = s as u32;
                if s > 15 { return None; }
                let ctx = cc * NCTX + nc;
                *cnt_dc.get_unchecked_mut(ctx).get_unchecked_mut(s as usize) += 1;
                *evp.add(nev) = ((ctx as u16) << 8) | s as u16;
                nev += 1;
                rput!(raw, s);
                // AC
                let row = cc * 8 * NCTX + nc;
                let mut k = 1usize;
                let mut nz = 0u32;
                while k < 64 {
                    let (rs, raw) = sym_raw!(tac, true)?;
                    let rs = rs as u32;
                    let actx = row + band(k) * NCTX;
                    let (run, sz) = (rs >> 4, rs & 15);
                    *cnt_ac.get_unchecked_mut(actx).get_unchecked_mut(rs as usize) += 1;
                    *evp.add(nev) = (((CTX_DC + actx) as u16) << 8) | rs as u16;
                    nev += 1;
                    rput!(raw, sz);
                    if sz == 0 {
                        if run == 15 { k += 16; continue; }
                        break;
                    }
                    k += run as usize + 1;
                    nz += 1;
                }
                if k > 64 { return None; }
                *cnt.get_unchecked_mut(b) = nz.min(63) as u8;
                if end_mcu { mcu += 1; first = true; }
            }
        }
        std::ptr::write_unaligned(rbp.add(rlen) as *mut u64, racc.to_le());
        rlen += ((rn + 7) >> 3) as usize;
        evs.set_len(nev);
    }
    rawbuf.truncate(rlen);
    rawbuf.extend_from_slice(&[0; 16]);
    T_PARSE.fetch_add(t0.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
    let t1 = std::time::Instant::now();
    finish_pack(&evs, &cnt_dc, &cnt_ac, rawbuf, t1)
}
