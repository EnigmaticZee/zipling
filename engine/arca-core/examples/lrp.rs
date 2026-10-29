//! Research tool: a long-range pre-pass before LZMA. `lrp BLOCK [MIN_LEN] [FB]`
//!
//! Finds repeats of at least MIN_LEN bytes anywhere earlier in the block with a
//! content-defined rolling hash, replaces each with a reference, and LZMA-codes
//! the bytes that remain plus the reference list. Compares size and CPU with
//! LZMA on the whole block, and checks the rebuild.

fn cpu() -> f64 {
    let mut r: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut r) };
    r.ru_utime.tv_sec as f64 + r.ru_utime.tv_usec as f64 / 1e6
}

const fn gear_table() -> [u64; 256] {
    let mut t = [0u64; 256];
    let mut s: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut i = 0;
    while i < 256 {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        t[i] = s;
        i += 1;
    }
    t
}
static GEAR: [u64; 256] = gear_table();
/// The hash covers the last 64 bytes (each byte's contribution shifts out after 64 steps).
const WIN: usize = 64;
const ANCHOR_MASK: u64 = 0x1F << 40; // ~1 in 32 positions
const TABLE_BITS: u32 = 22;

fn put_uv(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push(v as u8 | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn get_uv(b: &[u8], p: &mut usize) -> u64 {
    let (mut v, mut s) = (0u64, 0);
    loop {
        let x = b[*p];
        *p += 1;
        v |= ((x & 0x7F) as u64) << s;
        if x < 0x80 {
            return v;
        }
        s += 7;
    }
}

/// Returns (literal bytes, references as varints (lit_len, len, dist)..., final lit_len).
fn prepass(d: &[u8], min: usize) -> (Vec<u8>, Vec<u8>, usize, usize) {
    let mut table = vec![u32::MAX; 1 << TABLE_BITS];
    let (mut lits, mut ctl) = (Vec::with_capacity(d.len() / 4), Vec::new());
    let (mut h, mut i, mut lit_start, mut refs, mut covered) = (0u64, 0usize, 0usize, 0usize, 0usize);
    while i < d.len() {
        h = (h << 1).wrapping_add(GEAR[d[i] as usize]);
        i += 1;
        if i < WIN || h & ANCHOR_MASK != 0 {
            continue;
        }
        let slot = (h >> (64 - TABLE_BITS)) as usize;
        let cand = table[slot] as usize;
        table[slot] = i as u32;
        if cand == u32::MAX as usize || cand >= i || i - WIN < lit_start || d[cand - WIN..cand] != d[i - WIN..i] {
            continue;
        }
        // Extend backward (not into what's already emitted) and forward.
        let (mut s, mut c) = (i - WIN, cand - WIN);
        while s > lit_start && c > 0 && d[s - 1] == d[c - 1] {
            s -= 1;
            c -= 1;
        }
        let mut e = i;
        let mut ce = cand;
        while e < d.len() && d[e] == d[ce] && ce < e {
            e += 1;
            ce += 1;
        }
        if e - s < min {
            continue;
        }
        lits.extend_from_slice(&d[lit_start..s]);
        put_uv(&mut ctl, (s - lit_start) as u64);
        put_uv(&mut ctl, (e - s) as u64);
        put_uv(&mut ctl, (s - c) as u64);
        refs += 1;
        covered += e - s;
        // Keep indexing the copied region so later repeats can point into it.
        while i < e {
            h = (h << 1).wrapping_add(GEAR[d[i] as usize]);
            i += 1;
            if h & ANCHOR_MASK == 0 {
                table[(h >> (64 - TABLE_BITS)) as usize] = i as u32;
            }
        }
        lit_start = e;
    }
    lits.extend_from_slice(&d[lit_start..]);
    put_uv(&mut ctl, (d.len() - lit_start) as u64);
    (lits, ctl, refs, covered)
}

fn rebuild(lits: &[u8], ctl: &[u8], n: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(n);
    let (mut p, mut lp) = (0usize, 0usize);
    loop {
        let l = get_uv(ctl, &mut p) as usize;
        out.extend_from_slice(&lits[lp..lp + l]);
        lp += l;
        if p >= ctl.len() {
            break;
        }
        let (len, dist) = (get_uv(ctl, &mut p) as usize, get_uv(ctl, &mut p) as usize);
        let from = out.len() - dist;
        for k in 0..len {
            let b = out[from + k];
            out.push(b);
        }
    }
    out
}

fn lzma(d: &[u8], fb: i32) -> usize {
    let s = lzma_sdk::Settings { level: 9, dict_size: (d.len() as u32).clamp(1 << 16, 1 << 26), fb, mc: 0, threads: 1 };
    lzma_sdk::encode(d, s, d.len() + d.len() / 2 + 4096).map(|(c, _)| c.len()).unwrap_or(d.len())
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let d = std::fs::read(&a[1]).unwrap();
    let min: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(256);
    let fb: i32 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(273);
    let t = cpu();
    let base = lzma(&d, fb);
    let t_base = cpu() - t;
    let t = cpu();
    let (lits, ctl, refs, covered) = prepass(&d, min);
    let t_pre = cpu() - t;
    let t = cpu();
    // Columns: literal-run lengths, match lengths, distances (0 = same as the last).
    let (mut c1, mut c2, mut c3) = (Vec::new(), Vec::new(), Vec::new());
    {
        let (mut p, mut last) = (0usize, 0u64);
        loop {
            put_uv(&mut c1, get_uv(&ctl, &mut p));
            if p >= ctl.len() {
                break;
            }
            put_uv(&mut c2, get_uv(&ctl, &mut p));
            let dist = get_uv(&ctl, &mut p);
            put_uv(&mut c3, if dist == last { 0 } else { dist });
            last = dist;
        }
    }
    let (cl, cc) = (lzma(&lits, fb), lzma(&c1, fb) + lzma(&c2, fb) + lzma(&c3, fb));
    let t_lz = cpu() - t;
    assert_eq!(rebuild(&lits, &ctl, d.len()), d, "rebuild mismatch");
    println!(
        "{}: {:.1} MB | LZMA fb{fb}: {:.3} MB in {:.2} s | pre-pass min {min}: {refs} refs cover {:.1}% ({:.2} s), LZMA on {:.1} MB left: {:.3} MB + refs {:.3} MB = {:.3} MB in {:.2} s total",
        a[1].rsplit('/').next().unwrap(),
        d.len() as f64 / 1e6,
        base as f64 / 1e6,
        t_base,
        covered as f64 * 100.0 / d.len() as f64,
        t_pre,
        lits.len() as f64 / 1e6,
        cl as f64 / 1e6,
        cc as f64 / 1e6,
        (cl + cc) as f64 / 1e6,
        t_pre + t_lz
    );
}
