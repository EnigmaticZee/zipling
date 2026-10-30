//! Light JPEG recompression for Fast: JPEG's own symbols (DC size; AC run/size,
//! EOB, ZRL) and raw bits, re-coded with static Huffman tables per image and per
//! context. No coefficients are rebuilt, so both directions run at Huffman speed;
//! unpacking reads a symbol with our table and writes the photo's own code for it.
//!
//! Contexts: luma or chroma x band of the next position x the neighbours' nonzero
//! counts (AC); luma or chroma x neighbour counts (DC). Baseline JPEGs only.
//!
//! Stored layout (the prefix compresses with the binary lane, the rest is stored):
//!   prefix: uv header length, the file with its scan data removed,
//!           uv scan count, then per scan uv offset and uv length,
//!           uv table length, the tables
//!   rest:   u32 code-stream length, code stream, raw-bit stream

use std::collections::HashMap;

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
    BAND[k.min(79)] as usize
}

// ---------- JPEG layout ----------

type Dht = ([u8; 16], Vec<u8>);

struct Comp {
    id: u8,
    h: usize,
    v: usize,
    bw: usize,
    bh: usize,
}

struct Scan {
    /// (component index, DC table, AC table) in scan order.
    comps: Vec<(usize, Dht, Dht)>,
    start: usize,
    end: usize,
}

struct Layout {
    comps: Vec<Comp>,
    scans: Vec<Scan>,
    dri: usize,
    width: usize,
    height: usize,
    hmax: usize,
    vmax: usize,
}

impl Layout {
    /// Markers and scans of a baseline JPEG. Scan data ends at the first marker
    /// that isn't stuffing or a restart.
    fn parse(d: &[u8]) -> Option<Layout> {
        if d.len() < 4 || d[0] != 0xFF || d[1] != 0xD8 {
            return None;
        }
        let mut i = 2;
        let mut dc: HashMap<u8, Dht> = HashMap::new();
        let mut ac: HashMap<u8, Dht> = HashMap::new();
        let mut l = Layout { comps: vec![], scans: vec![], dri: 0, width: 0, height: 0, hmax: 1, vmax: 1 };
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
                    let mcux = (l.width + 8 * l.hmax - 1) / (8 * l.hmax);
                    let mcuy = (l.height + 8 * l.vmax - 1) / (8 * l.vmax);
                    for c in l.comps.iter_mut() {
                        c.bw = mcux * c.h;
                        c.bh = mcuy * c.v;
                    }
                }
                0xC2 | 0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => return None,
                0xC4 => {
                    let mut p = 0;
                    while p + 17 <= seg.len() {
                        let (tc, th) = (seg[p] >> 4, seg[p] & 15);
                        let mut counts = [0u8; 16];
                        counts.copy_from_slice(&seg[p + 1..p + 17]);
                        let n: usize = counts.iter().map(|&x| x as usize).sum();
                        if p + 17 + n > seg.len() || n > 256 {
                            return None;
                        }
                        let t = (counts, seg[p + 17..p + 17 + n].to_vec());
                        if tc == 0 { dc.insert(th, t); } else { ac.insert(th, t); }
                        p += 17 + n;
                    }
                }
                0xDD => {
                    if seg.len() < 2 {
                        return None;
                    }
                    l.dri = u16::from_be_bytes([seg[0], seg[1]]) as usize;
                }
                0xDA => {
                    if l.comps.is_empty() || seg.is_empty() {
                        return None;
                    }
                    let ns = seg[0] as usize;
                    if ns == 0 || seg.len() < 1 + 2 * ns + 3 {
                        return None;
                    }
                    // baseline: Ss = 0, Se = 63, Ah = Al = 0
                    if seg[1 + 2 * ns] != 0 || seg[2 + 2 * ns] != 63 || seg[3 + 2 * ns] != 0 {
                        return None;
                    }
                    let mut comps = vec![];
                    for k in 0..ns {
                        let ci = l.comps.iter().position(|c| c.id == seg[1 + 2 * k])?;
                        let t = seg[2 + 2 * k];
                        comps.push((ci, dc.get(&(t >> 4))?.clone(), ac.get(&(t & 15))?.clone()));
                    }
                    let start = i + 2 + ln;
                    let mut j = start;
                    while j + 1 < d.len() {
                        // skip 8 bytes at a time while none is 0xFF
                        if j + 8 <= d.len() {
                            let w = !u64::from_le_bytes(d[j..j + 8].try_into().unwrap());
                            if (w.wrapping_sub(0x0101_0101_0101_0101) & !w & 0x8080_8080_8080_8080) == 0 {
                                j += 8;
                                continue;
                            }
                        }
                        if d[j] == 0xFF && d[j + 1] != 0 && !(0xD0..=0xD7).contains(&d[j + 1]) {
                            break;
                        }
                        j += 1;
                    }
                    if j + 1 >= d.len() {
                        return None;
                    }
                    l.scans.push(Scan { comps, start, end: j });
                    i = j;
                    continue;
                }
                _ => {}
            }
            i += 2 + ln;
        }
        if l.scans.is_empty() {
            return None;
        }
        Some(l)
    }
}

/// Blocks of a scan in order: (component, index in scan, block, last of MCU, up, left).
struct Blocks<'a> {
    l: &'a Layout,
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
    fn new(l: &'a Layout, sc: &'a Scan) -> Blocks<'a> {
        let c = &l.comps[sc.comps[0].0];
        Blocks {
            l,
            sc,
            single: sc.comps.len() == 1,
            mcux: (l.width + 8 * l.hmax - 1) / (8 * l.hmax),
            mcuy: (l.height + 8 * l.vmax - 1) / (8 * l.vmax),
            cw: (l.width * c.h + 8 * l.hmax - 1) / (8 * l.hmax),
            ch: (l.height * c.v + 8 * l.vmax - 1) / (8 * l.vmax),
            mx: 0,
            my: 0,
            si: 0,
            x: 0,
            y: 0,
            done: false,
        }
    }
}

impl Iterator for Blocks<'_> {
    type Item = (usize, usize, usize, bool, bool, bool);
    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        if self.single {
            let ci = self.sc.comps[0].0;
            let (bx, by) = (self.x, self.y);
            let item = (ci, 0, by * self.l.comps[ci].bw + bx, true, by > 0, bx > 0);
            self.x += 1;
            if self.x == self.cw {
                self.x = 0;
                self.y += 1;
                self.done = self.y == self.ch;
            }
            return Some(item);
        }
        let ci = self.sc.comps[self.si].0;
        let c = &self.l.comps[ci];
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
                        self.done = self.my == self.mcuy;
                    }
                }
            }
        }
        Some(item)
    }
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

// ---------- JPEG Huffman reading and writing ----------

/// A JPEG table for reading: a 12-bit lookahead table, then the canonical walk.
struct JTable {
    fast: Vec<u16>, // (len << 8) | symbol; 0 = longer than 12 bits
    maxcode: [i32; 18],
    valptr: [i32; 17],
    mincode: [i32; 17],
    vals: Vec<u8>,
}

impl JTable {
    fn new(t: &Dht) -> JTable {
        let (counts, vals) = (&t.0, &t.1);
        let mut j = JTable { fast: vec![0; 4096], maxcode: [-1; 18], valptr: [0; 17], mincode: [0; 17], vals: vals.clone() };
        let (mut code, mut k) = (0i32, 0i32);
        for l in 1..=16usize {
            let n = counts[l - 1] as i32;
            if n > 0 {
                j.valptr[l] = k;
                j.mincode[l] = code;
                if l <= 12 {
                    for i in 0..n {
                        let c = ((code + i) as usize) << (12 - l);
                        for x in 0..(1usize << (12 - l)) {
                            j.fast[c | x] = ((l as u16) << 8) | vals[(k + i) as usize] as u16;
                        }
                    }
                }
                code += n;
                k += n;
                j.maxcode[l] = code - 1;
            }
            code <<= 1;
        }
        j.maxcode[17] = i32::MAX;
        j
    }
}

/// True when the table is a prefix code with one code per symbol: then a parsed
/// symbol names the bits it was read from, and nothing else does.
fn one_code_each(t: &Dht) -> bool {
    let mut seen = [false; 256];
    let (mut code, mut k) = (0u32, 0);
    for l in 1..=16u32 {
        for _ in 0..t.0[l as usize - 1] {
            let Some(&s) = t.1.get(k) else { return false };
            if code >= 1 << l || seen[s as usize] {
                return false;
            }
            seen[s as usize] = true;
            code += 1;
            k += 1;
        }
        code <<= 1;
    }
    true
}

/// Canonical JPEG codes: (code, length) per symbol.
fn jpeg_codes(t: &Dht) -> [(u32, u32); 256] {
    let mut c = [(0u32, 0u32); 256];
    let (mut code, mut k) = (0u32, 0);
    for l in 1..=16u32 {
        for _ in 0..t.0[l as usize - 1] {
            c[t.1[k] as usize] = (code, l);
            code += 1;
            k += 1;
        }
        code <<= 1;
    }
    c
}

#[cold]
#[inline(never)]
fn slow_sym(t: &JTable, acc: &mut u64, n: &mut u32) -> Option<u8> {
    let mut code = 0i32;
    for l in 1..=16usize {
        code = (code << 1) | (*acc >> 63) as i32;
        *acc <<= 1;
        *n = n.checked_sub(1)?;
        if code <= t.maxcode[l] {
            return t.vals.get((t.valptr[l] + code - t.mincode[l]) as usize).copied();
        }
    }
    None
}

/// Refills the JPEG reader byte by byte: stuffing, and zeros at a marker or the
/// end, which `made_up` counts in bits.
#[cold]
#[inline(never)]
fn slow_fill(d: &[u8], pos: &mut usize, acc: &mut u64, n: &mut u32, made_up: &mut u32) {
    while *n <= 56 {
        let mut b = 0u8;
        match d.get(*pos) {
            Some(&0xFF) if d.get(*pos + 1) == Some(&0) => {
                b = 0xFF;
                *pos += 2;
            }
            Some(&x) if x != 0xFF => {
                b = x;
                *pos += 1;
            }
            _ => *made_up += 8,
        }
        *acc |= (b as u64) << (56 - *n);
        *n += 8;
    }
}

// ---------- our Huffman codes ----------

/// Length-limited code lengths: sort, two-queue Huffman, JPEG's Annex K.3 cap.
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
    let mut w = [0u64; 512];
    let mut parent = [0u16; 512];
    for i in 0..m {
        w[i] = syms[i] >> 8;
    }
    let (mut li, mut ni, mut nn) = (0usize, m, m);
    while nn < 2 * m - 1 {
        let mut pick = || {
            if li < m && (ni >= nn || w[li] <= w[ni]) {
                li += 1;
                li - 1
            } else {
                ni += 1;
                ni - 1
            }
        };
        let (a, b) = (pick(), pick());
        w[nn] = w[a] + w[b];
        parent[a] = nn as u16;
        parent[b] = nn as u16;
        nn += 1;
    }
    let mut depth = [0u8; 512];
    for i in (0..nn - 1).rev() {
        depth[i] = depth[parent[i] as usize] + 1;
    }
    let mut bits = [0u32; 64];
    for &dp in &depth[..m] {
        bits[dp as usize] += 1;
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
    let mut k = m;
    for l in 1..=MAXLEN as usize {
        for _ in 0..bits[l] {
            k -= 1;
            len[(syms[k] & 0xFF) as usize] = l as u8;
        }
    }
    len
}

/// Canonical codes, bit-reversed for LSB-first writing.
fn codes_of(len: &[u8; 256]) -> [(u16, u8); 256] {
    let mut codes = [(0u16, 0u8); 256];
    let mut code = 0u32;
    for l in 1..=MAXLEN as u8 {
        for s in 0..256 {
            if len[s] == l {
                codes[s] = ((code.reverse_bits() >> (32 - l as u32)) as u16, l);
                code += 1;
            }
        }
        code <<= 1;
    }
    codes
}

/// The decode table (11-bit, LSB-first): (len << 8) | symbol.
fn decode_table(len: &[u8; 256], lut: &mut [u16]) {
    let mut code = 0u32;
    for l in 1..=MAXLEN as u8 {
        for s in 0..256 {
            if len[s] == l {
                let rev = (code.reverse_bits() >> (32 - l as u32)) as usize;
                let mut j = rev;
                while j < (1 << MAXLEN) {
                    lut[j] = ((l as u16) << 8) | s as u16;
                    j += 1 << l;
                }
                code += 1;
            }
        }
        code <<= 1;
    }
}

/// Branch-free LSB-first writer into a buffer sized up front.
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
        // SAFETY: at most 4 bytes per call are kept and the buffer has 16 to spare.
        unsafe { std::ptr::write_unaligned(self.out.as_mut_ptr().add(self.len) as *mut u64, self.acc.to_le()) };
        let bytes = self.n >> 3;
        self.len += bytes as usize;
        self.acc = if bytes == 8 { 0 } else { self.acc >> (8 * bytes) };
        self.n &= 7;
    }
    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out[self.len] = self.acc as u8;
            self.len += 1;
        }
        self.out.truncate(self.len);
        self.out
    }
}

// ---------- encode ----------

/// Re-codes a baseline JPEG; `None` when it isn't one or doesn't parse cleanly.
/// Returns the stored bytes and where the already-coded part starts.
/// Every scan is also re-written from the parsed symbols and raw bits while
/// parsing and compared with the original, so a photo the coder can't represent
/// exactly is left as it is.
pub fn encode(d: &[u8]) -> Option<(Vec<u8>, usize)> {
    let l = Layout::parse(d)?;
    let mut evs: Vec<u16> = Vec::with_capacity(2 * d.len() + 128);
    let mut rawbuf: Vec<u8> = vec![0; d.len() + 64];
    let mut cnt_ac = vec![[0u32; 256]; CTX_AC];
    let mut cnt_dc = vec![[0u32; 256]; CTX_DC];
    let mut counts: Vec<Vec<u8>> = l.comps.iter().map(|c| vec![0u8; c.bw * c.bh]).collect();
    let (mut racc, mut rn, mut rlen) = (0u64, 0u32, 0usize);
    let mut nev = 0usize;
    // SAFETY (whole block): events go to `evs`, which keeps room for a whole block
    // (checked per block); raw bits to `rawbuf`, which holds at least as many bits
    // as the scans; counters are indexed by contexts below CTX_DC / CTX_AC and
    // symbols below 256.
    unsafe {
        let mut evp = evs.as_mut_ptr();
        let rbp = rawbuf.as_mut_ptr();
        for sc in &l.scans {
            if !sc.comps.iter().all(|(_, dc, ac)| one_code_each(dc) && one_code_each(ac)) {
                return None;
            }
            let tabs: Vec<(JTable, JTable)> = sc.comps.iter().map(|(_, dc, ac)| (JTable::new(dc), JTable::new(ac))).collect();
            let dd = &d[..sc.end];
            let (mut acc, mut n, mut pos, mut made_up) = (0u64, 0u32, sc.start, 0u32);
            // Together with `one_code_each`, what `ended!` checks is what makes
            // the coding reversible: every bit of the scan was either a symbol's
            // code or its raw bits, stuffing was the plain FF 00, and all that is
            // left of each stretch is padding of ones, right before its marker.
            // A re-written scan could differ in no other way.
            macro_rules! ended {
                ($next:expr) => {{
                    let left = n.checked_sub(made_up)?;
                    if left > 7 || (left > 0 && (acc >> (64 - left)) != (1u64 << left) - 1) {
                        return None;
                    }
                    if !$next {
                        return None;
                    }
                }};
            }
            let mut rst = 0u8;
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
                    if !done {
                        slow_fill(dd, &mut pos, &mut acc, &mut n, &mut made_up);
                    }
                }};
            }
            // a symbol and its raw bits in one step (size: whole symbol for DC, low 4 bits for AC)
            macro_rules! sym_raw {
                ($t:expr, $ac:expr) => {{
                    if n < 32 {
                        fill!();
                    }
                    let e = *$t.fast.as_ptr().add((acc >> 52) as usize);
                    if e != 0 {
                        let l = (e >> 8) as u32;
                        let sym = e as u8;
                        let s = if $ac { (sym & 15) as u32 } else { (sym as u32).min(16) };
                        let v = acc << l;
                        let r = (((v >> 1) >> (63 - s)) as u32) & ((1u32 << s) - 1);
                        acc = v << s;
                        n = n.checked_sub(l + s)?;
                        (sym as u32, r, s)
                    } else {
                        let sym = slow_sym(&$t, &mut acc, &mut n)?;
                        let s = if $ac { (sym & 15) as u32 } else { sym as u32 };
                        if s > 16 {
                            return None;
                        }
                        if n < 32 {
                            fill!();
                        }
                        let r = if s == 0 { 0 } else { (acc >> (64 - s)) as u32 };
                        acc = if s == 0 { acc } else { acc << s };
                        n -= s;
                        (sym as u32, r, s)
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
            let (mut mcu, mut first) = (0usize, true);
            for (ci, si, b, end_mcu, up, lf) in Blocks::new(&l, sc) {
                if l.dri > 0 && first && mcu > 0 && mcu % l.dri == 0 {
                    ended!(dd.get(pos) == Some(&0xFF) && dd.get(pos + 1) == Some(&(0xD0 + rst)));
                    rst = (rst + 1) & 7;
                    pos += 2;
                    acc = 0;
                    n = 0;
                    made_up = 0;
                }
                first = false;
                // room for a whole block's events (1 + 63 + a few ZRL)
                if nev + 80 > evs.capacity() {
                    evs.set_len(nev);
                    evs.reserve(evs.capacity());
                    evp = evs.as_mut_ptr();
                }
                // raw bits can't outgrow the scan bytes read so far
                if rlen + 64 > rawbuf.len() {
                    return None;
                }
                let cc = ci.min(1);
                let cnt = counts.get_unchecked_mut(ci);
                let nc = nctx(cnt, l.comps.get_unchecked(ci).bw, b, up, lf);
                let (tdc, tac) = (&tabs.get_unchecked(si).0, &tabs.get_unchecked(si).1);
                let (s, r, sz) = sym_raw!(tdc, false);
                if s > 15 {
                    return None;
                }
                let ctx = cc * NCTX + nc;
                *cnt_dc.get_unchecked_mut(ctx).get_unchecked_mut(s as usize) += 1;
                *evp.add(nev) = ((ctx as u16) << 8) | s as u16;
                nev += 1;
                rput!(r, sz);
                let row = cc * 8 * NCTX + nc;
                let (mut k, mut nz) = (1usize, 0u32);
                while k < 64 {
                    let (rs, r, sz) = sym_raw!(tac, true);
                    let actx = row + band(k) * NCTX;
                    *cnt_ac.get_unchecked_mut(actx).get_unchecked_mut(rs as usize) += 1;
                    *evp.add(nev) = (((CTX_DC + actx) as u16) << 8) | rs as u16;
                    nev += 1;
                    rput!(r, sz);
                    if sz == 0 {
                        if rs == 0xF0 {
                            k += 16;
                            continue;
                        }
                        if rs != 0 {
                            return None; // not EOB, ZRL or a value
                        }
                        break;
                    }
                    k += (rs >> 4) as usize + 1;
                    nz += 1;
                }
                if k > 64 {
                    return None;
                }
                *cnt.get_unchecked_mut(b) = nz.min(63) as u8;
                if end_mcu {
                    mcu += 1;
                    first = true;
                }
            }
            ended!(pos == sc.end);
        }
        std::ptr::write_unaligned(rbp.add(rlen) as *mut u64, racc.to_le());
        rlen += ((rn + 7) >> 3) as usize;
        evs.set_len(nev);
    }
    rawbuf.truncate(rlen);
    // tables, then the code stream
    let mut tables = Vec::new();
    let mut codes: Vec<[(u16, u8); 256]> = Vec::with_capacity(CTX_DC + CTX_AC);
    for c in cnt_dc.iter().chain(cnt_ac.iter()) {
        let used = c.iter().any(|&x| x > 0);
        tables.push(used as u8);
        if !used {
            codes.push([(0, 0); 256]);
            continue;
        }
        let len = lengths(c);
        let mut per = [0u32; MAXLEN as usize];
        for &x in len.iter().filter(|&&x| x > 0) {
            per[x as usize - 1] += 1;
        }
        if per.iter().any(|&x| x > 255) {
            return None;
        }
        let per: Vec<u8> = per.iter().map(|&x| x as u8).collect();
        tables.extend_from_slice(&per);
        for x in 1..=MAXLEN as u8 {
            for (s, &ls) in len.iter().enumerate() {
                if ls == x {
                    tables.push(s as u8);
                }
            }
        }
        codes.push(codes_of(&len));
    }
    let mut w = LWriter::with_capacity(evs.len() * 2 + 64);
    for &e in &evs {
        let (c, cl) = codes[(e >> 8) as usize][(e & 0xFF) as usize];
        w.put(c as u32, cl as u32);
    }
    let body = w.finish();
    let raw = rawbuf;
    // the file without its scan data
    let mut hdr = Vec::with_capacity(4096);
    let mut at = 0;
    for sc in &l.scans {
        hdr.extend_from_slice(&d[at..sc.start]);
        at = sc.end;
    }
    hdr.extend_from_slice(&d[at..]);
    let mut out = Vec::with_capacity(hdr.len() + tables.len() + body.len() + raw.len() + 64);
    let uv = |out: &mut Vec<u8>, mut v: u64| loop {
        let b = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            break;
        }
        out.push(b | 0x80);
    };
    uv(&mut out, hdr.len() as u64);
    out.extend_from_slice(&hdr);
    uv(&mut out, l.scans.len() as u64);
    for sc in &l.scans {
        uv(&mut out, sc.start as u64);
        uv(&mut out, (sc.end - sc.start) as u64);
    }
    uv(&mut out, tables.len() as u64);
    out.extend_from_slice(&tables);
    let store_from = out.len();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out.extend_from_slice(&raw);
    (out.len() < d.len()).then_some((out, store_from))
}

// ---------- decode ----------

#[allow(unused_assignments)] // the last pad's resets
pub fn decode(stored: &[u8], size: u64) -> anyhow::Result<Vec<u8>> {
    use anyhow::{bail, ensure, Context};
    let mut p = 0usize;
    let mut uv = || -> anyhow::Result<u64> {
        let mut v = 0u64;
        for s in (0..64).step_by(7) {
            let b = *stored.get(p).context("truncated")?;
            p += 1;
            v |= ((b & 0x7F) as u64) << s;
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        bail!("bad varint")
    };
    let hl = uv()? as usize;
    let hstart = p;
    ensure!(hstart + hl <= stored.len(), "truncated header");
    p += hl;
    let hdr = &stored[hstart..hstart + hl];
    let mut uv = || -> anyhow::Result<u64> {
        let mut v = 0u64;
        for s in (0..64).step_by(7) {
            let b = *stored.get(p).context("truncated")?;
            p += 1;
            v |= ((b & 0x7F) as u64) << s;
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        bail!("bad varint")
    };
    let ns = uv()? as usize;
    let mut spans = Vec::with_capacity(ns.min(64));
    for _ in 0..ns {
        spans.push((uv()? as usize, uv()? as usize));
    }
    let tl = uv()? as usize;
    ensure!(p + tl + 4 <= stored.len(), "truncated tables");
    let t = &stored[p..p + tl];
    p += tl;
    let bl = u32::from_le_bytes(stored[p..p + 4].try_into()?) as usize;
    p += 4;
    ensure!(p + bl <= stored.len(), "truncated code stream");
    // Both streams are padded past anything one block can consume (at most 66
    // symbols: 91 code bytes, 132 raw bytes), and checked once per block.
    const PAD: usize = 1024;
    let mut body = stored[p..p + bl].to_vec();
    body.resize(bl + PAD, 0);
    let mut rawb = stored[p + bl..].to_vec();
    let rl = rawb.len();
    rawb.resize(rl + PAD, 0);

    let l = Layout::parse(hdr).context("bad stored JPEG header")?;
    ensure!(l.scans.len() == ns, "scan count differs");

    // decode tables: an 8-bit table in front of the 11-bit one
    let mut hot = vec![0u16; (CTX_DC + CTX_AC) * 256];
    let mut full: Vec<Vec<u16>> = Vec::with_capacity(CTX_DC + CTX_AC);
    let mut q = 0usize;
    for ci in 0..CTX_DC + CTX_AC {
        let used = *t.get(q).context("truncated tables")?;
        q += 1;
        if used == 0 {
            full.push(Vec::new());
            continue;
        }
        ensure!(q + MAXLEN as usize <= t.len(), "truncated tables");
        let per: Vec<u8> = t[q..q + MAXLEN as usize].to_vec();
        q += MAXLEN as usize;
        let mut len = [0u8; 256];
        for x in 1..=MAXLEN as usize {
            for _ in 0..per[x - 1] {
                len[*t.get(q).context("truncated tables")? as usize] = x as u8;
                q += 1;
            }
        }
        let kraft: u32 = (1..=MAXLEN as usize).map(|x| (per[x - 1] as u32) << (MAXLEN as usize - x)).sum();
        ensure!(kraft <= 1 << MAXLEN, "bad table");
        let mut lut = vec![0u16; 1 << MAXLEN];
        decode_table(&len, &mut lut);
        for i in 0..256 {
            if (lut[i] >> 8) as u32 <= 8 {
                hot[ci * 256 + i] = lut[i];
            }
        }
        full.push(lut);
    }

    let mut out: Vec<u8> = Vec::with_capacity(size as usize + PAD);
    let cap = out.capacity();
    let mut counts: Vec<Vec<u8>> = l.comps.iter().map(|c| vec![0u8; c.bw * c.bh]).collect();
    let (mut cacc, mut cn, mut cp) = (0u64, 0u32, 0usize);
    let (mut racc, mut rn, mut rp) = (0u64, 0u32, 0usize);
    let mut hat = 0usize; // copied up to here in the header bytes
    for (sc, &(off, slen)) in l.scans.iter().zip(&spans) {
        out.extend_from_slice(&hdr[hat..sc.start]);
        hat = sc.start;
        ensure!(out.len() == off, "scan offset differs");
        let jt: Vec<([(u32, u32); 256], [(u32, u32); 256])> = sc.comps.iter().map(|(_, dc, ac)| (jpeg_codes(dc), jpeg_codes(ac))).collect();
        // SAFETY (whole block below): every pointer stays inside its buffer. Reads
        // are checked once per block against the padding above; writes against
        // the output's spare capacity, which also bounds a corrupt stream.
        unsafe {
            let base = out.as_mut_ptr();
            let mut wp = base.add(out.len());
            let (mut wacc, mut wn) = (0u64, 0u32);
            let hotp = hot.as_ptr();
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
                            for b in w.to_be_bytes() {
                                *wp = b;
                                wp = wp.add(1);
                                if b == 0xFF {
                                    *wp = 0;
                                    wp = wp.add(1);
                                }
                            }
                        }
                    }
                }};
            }
            macro_rules! pad {
                () => {{
                    while wn >= 8 {
                        wn -= 8;
                        let b = (wacc >> wn) as u8;
                        *wp = b;
                        wp = wp.add(1);
                        if b == 0xFF {
                            *wp = 0;
                            wp = wp.add(1);
                        }
                    }
                    if wn > 0 {
                        let k = 8 - wn;
                        let b = (((wacc << k) | ((1 << k) - 1)) & 0xFF) as u8;
                        *wp = b;
                        wp = wp.add(1);
                        if b == 0xFF {
                            *wp = 0;
                            wp = wp.add(1);
                        }
                    }
                    wn = 0;
                    wacc = 0;
                }};
            }
            macro_rules! refill {
                ($acc:ident, $n:ident, $p:ident, $buf:ident) => {{
                    let v = u64::from_le(std::ptr::read_unaligned($buf.as_ptr().add($p) as *const u64));
                    $acc |= v << $n;
                    let take = (63 - $n) >> 3;
                    $p += take as usize;
                    $n += take << 3;
                }};
            }
            let (mut mcu, mut rst, mut first) = (0usize, 0u8, true);
            for (ci, si, b, end_mcu, up, lf) in Blocks::new(&l, sc) {
                // one block reads at most 91 + 8 code bytes and 132 + 8 raw bytes,
                // and writes at most 64 symbols of 32 bits doubled by stuffing
                ensure!(cp <= bl + 256 && rp <= rl + 256, "stream overrun");
                ensure!((wp.offset_from(base) as usize) + 600 <= cap, "output overrun");
                if l.dri > 0 && first && mcu > 0 && mcu % l.dri == 0 {
                    pad!();
                    *wp = 0xFF;
                    *wp.add(1) = 0xD0 + rst;
                    wp = wp.add(2);
                    rst = (rst + 1) & 7;
                }
                first = false;
                let cc = ci.min(1);
                let cnt = counts.get_unchecked_mut(ci);
                let nc = nctx(cnt, l.comps.get_unchecked(ci).bw, b, up, lf);
                let (jdc, jac) = (&jt.get_unchecked(si).0, &jt.get_unchecked(si).1);
                // DC
                refill!(cacc, cn, cp, body);
                let ctx = cc * NCTX + nc;
                let mut e = *hotp.add(ctx * 256 + (cacc & 255) as usize);
                if e == 0 {
                    e = *full[ctx].get((cacc & ((1 << MAXLEN) - 1)) as usize).context("bad code")?;
                }
                let cl = ((e >> 8) as u32).max(1);
                let s = (e & 15) as u32;
                cacc >>= cl;
                cn = cn.saturating_sub(cl);
                refill!(racc, rn, rp, rawb);
                let r = (racc & ((1u64 << s) - 1)) as u32;
                racc >>= s;
                rn -= s;
                let (c, jl) = *jdc.get_unchecked(s as usize);
                put!((c << s) | r, jl + s);
                // AC
                let row = CTX_DC + cc * 8 * NCTX + nc;
                let (mut k, mut nz) = (1usize, 0u32);
                while k < 64 {
                    refill!(cacc, cn, cp, body);
                    let ctx = row + band(k) * NCTX;
                    let mut e = *hotp.add(ctx * 256 + (cacc & 255) as usize);
                    if e == 0 {
                        e = *full[ctx].get((cacc & ((1 << MAXLEN) - 1)) as usize).context("bad code")?;
                    }
                    let cl = ((e >> 8) as u32).max(1);
                    let rs = (e & 0xFF) as u32;
                    cacc >>= cl;
                    cn = cn.saturating_sub(cl);
                    let sz = rs & 15;
                    if rn < 16 {
                        refill!(racc, rn, rp, rawb);
                    }
                    let r = (racc & ((1u64 << sz) - 1)) as u32;
                    racc >>= sz;
                    rn -= sz;
                    let (c, jl) = *jac.get_unchecked(rs as usize);
                    put!((c << sz) | r, jl + sz);
                    if sz == 0 {
                        if rs == 0xF0 {
                            k += 16;
                            continue;
                        }
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
            pad!();
            let n = wp.offset_from(base) as usize;
            ensure!(n <= cap, "output overrun");
            out.set_len(n);
        }
        ensure!(out.len() == off + slen, "scan length differs");
    }
    out.extend_from_slice(&hdr[hat..]);
    ensure!(out.len() as u64 == size, "size differs");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// A 24-bit BMP with smooth gradients, noise and repeated tiles.
    fn bmp(w: usize, h: usize, seed: u32) -> Vec<u8> {
        let row = (w * 3 + 3) & !3;
        let mut d = Vec::new();
        d.extend_from_slice(b"BM");
        d.extend_from_slice(&((54 + row * h) as u32).to_le_bytes());
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

    #[test]
    fn photos_from_sips_round_trip() {
        let dir = std::env::temp_dir().join(format!("arca-jpeglite-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut tried = 0;
        for (i, (w, h, q)) in [(64, 48, "40"), (333, 211, "85"), (800, 600, "100"), (17, 9, "60")].iter().enumerate() {
            let src = dir.join(format!("{i}.bmp"));
            let dst = dir.join(format!("{i}.jpg"));
            std::fs::write(&src, bmp(*w, *h, i as u32 + 7)).unwrap();
            let ok = Command::new("sips")
                .args(["-s", "format", "jpeg", "-s", "formatOptions", q])
                .arg(&src)
                .arg("--out")
                .arg(&dst)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            if !ok {
                continue; // no sips (not macOS)
            }
            tried += 1;
            let data = std::fs::read(&dst).unwrap();
            // tiny photos may not pay for the tables; the larger ones must
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

    /// The written codes and the decoder's table agree for skewed, flat and
    /// sparse distributions, including ones that need the length cap.
    #[test]
    fn codes_and_decode_tables_agree() {
        let mut seed = 12345u64;
        let mut rnd = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as u32
        };
        for case in 0..200 {
            let mut cnt = [0u32; 256];
            let used = 1 + (rnd() as usize % 256);
            for _ in 0..used {
                let s = rnd() as usize % 256;
                // geometric-ish weights so some cases need lengths capped at MAXLEN
                cnt[s] += 1 + (1u32 << (rnd() % if case % 3 == 0 { 24 } else { 8 }));
            }
            let len = lengths(&cnt);
            assert!(len.iter().all(|&l| l as u32 <= MAXLEN));
            let kraft: u32 = len.iter().filter(|&&l| l > 0).map(|&l| 1u32 << (MAXLEN - l as u32)).sum();
            assert!(kraft <= 1 << MAXLEN, "case {case}: not a prefix code");
            for (s, &c) in cnt.iter().enumerate() {
                assert_eq!(c > 0, len[s] > 0, "case {case}: symbol {s}");
            }
            let codes = codes_of(&len);
            let mut lut = vec![0u16; 1 << MAXLEN];
            decode_table(&len, &mut lut);
            let syms: Vec<usize> = (0..2000).map(|_| loop {
                let s = rnd() as usize % 256;
                if cnt[s] > 0 { break s; }
            }).collect();
            let mut w = LWriter::with_capacity(syms.len() * 2 + 16);
            for &s in &syms {
                w.put(codes[s].0 as u32, codes[s].1 as u32);
            }
            let mut bytes = w.finish();
            bytes.extend_from_slice(&[0; 16]);
            let (mut acc, mut n, mut p) = (0u64, 0u32, 0usize);
            for &s in &syms {
                if n < 32 {
                    acc |= u64::from_le_bytes(bytes[p..p + 8].try_into().unwrap()) << n;
                    let take = (63 - n) >> 3;
                    p += take as usize;
                    n += take << 3;
                }
                let e = lut[(acc & ((1 << MAXLEN) - 1)) as usize];
                assert_eq!((e & 0xFF) as usize, s, "case {case}");
                let l = (e >> 8) as u32;
                acc >>= l;
                n -= l;
            }
        }
    }

    /// One random change: anywhere, in the headers and tables, or near the end,
    /// where padding and stray bytes live.
    fn mutate(d: &mut Vec<u8>, s: &mut u64) {
        let mut r = |n: usize| {
            *s = s.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(0x1234_5);
            ((*s >> 33) as usize) % n.max(1)
        };
        let n = d.len();
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

    /// Whatever `encode` accepts must come back byte for byte, however the photo
    /// was damaged. Returns how many were accepted.
    fn fuzz(photo: &[u8], rounds: usize, seed: u64) -> usize {
        let (mut s, mut accepted) = (seed, 0);
        for _ in 0..rounds {
            let mut d = photo.to_vec();
            for _ in 0..1 + (s >> 60) as usize % 3 {
                mutate(&mut d, &mut s);
            }
            if let Some((out, _)) = encode(&d) {
                accepted += 1;
                let back = decode(&out, d.len() as u64).expect("an accepted photo decodes");
                assert!(back == d, "an accepted photo came back different");
            }
        }
        accepted
    }

    #[test]
    fn damaged_photos_come_back_or_are_refused() {
        let dir = std::env::temp_dir().join(format!("arca-jpeglite-fuzz-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (src, dst) = (dir.join("a.bmp"), dir.join("a.jpg"));
        std::fs::write(&src, bmp(333, 211, 5)).unwrap();
        let ok = Command::new("sips")
            .args(["-s", "format", "jpeg", "-s", "formatOptions", "85"])
            .arg(&src)
            .arg("--out")
            .arg(&dst)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            let photo = std::fs::read(&dst).unwrap();
            assert!(fuzz(&photo, 3000, 1) > 0);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Long run over real photos: `ARCA_FUZZ_DIR=<folder> ARCA_FUZZ_ROUNDS=<n per
    /// photo> cargo test --release fuzz_folder -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn fuzz_folder() {
        let dir = std::env::var("ARCA_FUZZ_DIR").unwrap();
        let rounds: usize = std::env::var("ARCA_FUZZ_ROUNDS").ok().and_then(|v| v.parse().ok()).unwrap_or(200);
        let paths: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
        let total: usize = std::thread::scope(|sc| {
            let mut hs = Vec::new();
            for part in paths.chunks(paths.len().div_ceil(10)) {
                hs.push(sc.spawn(move || {
                    let mut t = 0;
                    for (i, p) in part.iter().enumerate() {
                        if let Ok(d) = std::fs::read(p) {
                            if d.len() > 16 && encode(&d).is_some() {
                                t += fuzz(&d, rounds, i as u64 * 2 + 1);
                            }
                        }
                    }
                    t
                }));
            }
            hs.into_iter().map(|h| h.join().unwrap()).sum()
        });
        eprintln!("{} photos, {rounds} mutants each: {total} accepted, all identical", paths.len());
    }

    #[test]
    fn rejects_garbage() {
        assert!(encode(b"not a jpeg").is_none());
        assert!(encode(&[0xFF, 0xD8, 0xFF, 0xD9]).is_none());
        assert!(decode(&[1, 2, 3], 10).is_err());
    }
}
