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
    unsafe { *BAND.get_unchecked(k) as usize }
}

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

/// LSB-first bit writer for the light stream.
struct LWriter {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}
impl LWriter {
    #[inline(always)]
    fn put(&mut self, v: u32, l: u32) {
        self.acc |= (v as u64) << self.n;
        self.n += l;
        if self.n >= 32 {
            self.out.extend_from_slice(&(self.acc as u32).to_le_bytes());
            self.acc >>= 32;
            self.n -= 32;
        }
    }
    fn finish(mut self) -> Vec<u8> {
        while self.n > 0 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n = self.n.saturating_sub(8);
        }
        self.out.extend_from_slice(&[0; 16]);
        self.out
    }
}

/// Length-limited Huffman code lengths (halving counts until they fit).
fn lengths(cnt: &[u32; 256]) -> [u8; 256] {
    let mut c: Vec<u64> = cnt.iter().map(|&x| x as u64).collect();
    loop {
        let mut len = [0u8; 256];
        let used: Vec<usize> = (0..256).filter(|&i| c[i] > 0).collect();
        if used.len() == 1 {
            len[used[0]] = 1;
            return len;
        }
        let mut nodes: Vec<(i32, i32)> = Vec::new();
        let mut heap = std::collections::BinaryHeap::new();
        for &i in &used {
            nodes.push((-1, i as i32));
            heap.push(std::cmp::Reverse((c[i], nodes.len() - 1)));
        }
        while heap.len() > 1 {
            let std::cmp::Reverse((w1, a)) = heap.pop().unwrap();
            let std::cmp::Reverse((w2, b)) = heap.pop().unwrap();
            nodes.push((a as i32, b as i32));
            heap.push(std::cmp::Reverse((w1 + w2, nodes.len() - 1)));
        }
        let root = heap.pop().unwrap().0 .1;
        let mut stack = vec![(root, 0u32)];
        let mut ok = true;
        while let Some((i, d)) = stack.pop() {
            let (l, r) = nodes[i];
            if l == -1 {
                if d > MAXLEN { ok = false; }
                len[r as usize] = d.max(1) as u8;
            } else {
                stack.push((l as usize, d + 1));
                stack.push((r as usize, d + 1));
            }
        }
        if ok {
            return len;
        }
        for x in c.iter_mut() {
            if *x > 0 { *x = (*x + 1) / 2; }
        }
    }
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
pub static T_WRITE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn pack(d: &[u8], img: &Image) -> Option<Vec<u8>> {
    let t0 = std::time::Instant::now();
    let mut evs: Vec<Ev> = Vec::with_capacity(d.len());
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
        let ok = for_blocks(img, sc, |ci, si, b, end_mcu, up, lf| {
            if img.dri > 0 && first && mcu > 0 && mcu % img.dri == 0 {
                r.restart();
            }
            first = false;
            let cc = ci.min(1);
            let bw = img.comps[ci].bw;
            let nc = nctx(&counts[ci], bw, b, up, lf);
            // DC
            let Some(s) = r.sym(&tabs[si].0) else { return false };
            let s = s as u32;
            if s > 15 { return false; }
            let raw = r.bits(s);
            let ctx = cc * NCTX + nc;
            unsafe { *cnt_dc.get_unchecked_mut(ctx).get_unchecked_mut(s as usize) += 1; }
            evs.push(Ev { ctx: ctx as u16, sym: s as u8, raw: raw as u16, rl: s as u8 });
            // AC
            let mut k = 1usize;
            let mut n = 0u32;
            while k < 64 {
                let Some(rs) = r.sym(&tabs[si].1) else { return false };
                let rs = rs as u32;
                let ctx = CTX_DC + (cc * 8 + band(k)) * NCTX + nc;
                let (run, sz) = (rs >> 4, rs & 15);
                let raw = r.bits(sz);
                unsafe { *cnt_ac.get_unchecked_mut(ctx - CTX_DC).get_unchecked_mut(rs as usize) += 1; }
                evs.push(Ev { ctx: ctx as u16, sym: rs as u8, raw: raw as u16, rl: sz as u8 });
                if sz == 0 {
                    if run == 15 { k += 16; continue; }
                    break; // EOB
                }
                k += run as usize + 1;
                n += 1;
            }
            if k > 64 { return false; }
            counts[ci][b] = n.min(63) as u8;
            if end_mcu {
                mcu += 1;
                first = true;
            }
            true
        });
        if !ok { return None; }
    }
    T_PARSE.fetch_add(t0.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
    let t1 = std::time::Instant::now();
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
    let mut w = LWriter { out: Vec::with_capacity(d.len()), acc: 0, n: 0 };
    for e in &evs {
        let (c, l) = codes[e.ctx as usize][e.sym as usize];
        w.put(c as u32, l as u32);
        w.put(e.raw as u32, e.rl as u32);
    }
    let body = w.finish();
    T_WRITE.fetch_add(t1.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
    let mut res = Vec::with_capacity(4 + tlen + body.len());
    res.extend_from_slice(&(tlen as u32).to_le_bytes());
    res.extend_from_slice(&out);
    res.extend_from_slice(&body);
    Some(res)
}

/// Re-creates the scan data of every scan (concatenated in scan order).
pub static SYMS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static NOWRITE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

pub fn unpack(data: &[u8], img: &Image) -> Vec<Vec<u8>> {
    let nowrite = *NOWRITE.get_or_init(|| std::env::var_os("NOWRITE").is_some());
    let mut syms = 0u64;
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
    let body = &data[4 + tlen..];
    let (mut acc, mut n, mut bp) = (0u64, 0u32, 0usize);
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
        for_blocks(img, sc, |ci, si, b, end_mcu, up, lf| {
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
            let raw = (acc & ((1u64 << s) - 1)) as u32;
            acc >>= s;
            n -= s;
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
                let raw = (acc & ((1u64 << sz) - 1)) as u32;
                acc >>= sz;
                n -= sz;
                syms += 1;
                let (c, cl) = unsafe { *jac.get_unchecked(rs as usize) };
                if nowrite { std::hint::black_box((c << sz) | raw); } else { w.put((c << sz) | raw, cl + sz); }
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
            true
        });
        w.pad();
        outs.push(w.out);
    }
    SYMS.fetch_add(syms, std::sync::atomic::Ordering::Relaxed);
    outs
}
