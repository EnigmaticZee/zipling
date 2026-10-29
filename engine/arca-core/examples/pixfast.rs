//! Research tool: faster pixel decoding. `pixfast DIR...` with PIX_K (bands
//! decoded side by side, default 2) and PIX_DEC_REPS. Compares size and decode
//! time with the engine's codec on the same images.
#![allow(dead_code)]
#[path = "pixfast/codec.rs"]
mod codec;

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

static ENC: std::sync::Mutex<(f64, f64)> = std::sync::Mutex::new((0.0, 0.0));

fn main() {
    let k: usize = std::env::var("PIX_K").ok().and_then(|v| v.parse().ok()).unwrap_or(2);
    let reps: usize = std::env::var("PIX_DEC_REPS").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    let min_px: usize = std::env::var("PIX_MIN").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
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
            let kk = if px.len() >= min_px { k } else { 1 };
            let base = arca_core::transform::pixels::encode(&px, png.w, png.h, png.ch);
            let t = Instant::now();
            for _ in 0..if std::env::var_os("PIX_SKIP_BASE").is_some() { 0 } else { reps } {
                let back = arca_core::transform::pixels::decode(&base, png.w, png.h, png.ch).unwrap();
                assert!(back == px);
            }
            let tb = t.elapsed().as_secs_f64();
            let ours = codec::encode_bands(&px, png.w, png.h, png.ch, kk);
            if kk == 1 {
                assert!(ours[4..] == base[..], "encoders differ: {}", f.display());
            }
            let te = Instant::now();
            let _ = codec::encode(&px, png.w, png.h, png.ch);
            let te_old = te.elapsed().as_secs_f64();
            let te = Instant::now();
            let _ = arca_core::transform::pixels::encode(&px, png.w, png.h, png.ch);
            let te_new = te.elapsed().as_secs_f64();
            ENC.lock().unwrap().0 += te_old;
            ENC.lock().unwrap().1 += te_new;
            let t = Instant::now();
            for _ in 0..if std::env::var_os("PIX_SKIP_OURS").is_some() { 0 } else { reps } {
                let back = if std::env::var_os("PIX_FASTB").is_some() { codec::decode_fast_bands(&ours, png.w, png.h, png.ch, kk).unwrap() } else if std::env::var_os("PIX_FAST").is_some() { codec::decode_fast(&ours[4..], png.w, png.h, png.ch).unwrap() } else { codec::decode_bands(&ours, png.w, png.h, png.ch, kk).unwrap() };
                assert!(back == px, "round trip failed: {}", f.display());
            }
            let to = t.elapsed().as_secs_f64();
            if std::env::var_os("PIX_CHOICE").is_some() {
                let z12 = zstd::bulk::compress(&png.filtered, 12).unwrap().len();
                let z1 = zstd::bulk::compress(&png.filtered, 1).unwrap().len();
                println!("CHOICE\t{}\t{}\t{}\t{}\t{}", f.display(), px.len(), base.len(), z12, z1);
            }
            Some((px.len(), base.len(), ours.len(), tb, to))
        })
        .collect();
    let t = rows.iter().fold((0, 0, 0, 0.0, 0.0), |s, r| (s.0 + r.0, s.1 + r.1, s.2 + r.2, s.3 + r.3, s.4 + r.4));
    println!(
        "{} PNGs, {:.1} MB pixels | engine {:.3} MB, decode {:.2} s ({:.0} MB/s) | bands k={k}: {:.3} MB ({:+.2}%), decode {:.2} s ({:.0} MB/s)",
        rows.len(),
        t.0 as f64 / 1e6,
        t.1 as f64 / 1e6,
        t.3,
        t.0 as f64 * reps as f64 / 1e6 / t.3,
        t.2 as f64 / 1e6,
        (t.2 as f64 / t.1 as f64 - 1.0) * 100.0,
        t.4,
        t.0 as f64 * reps as f64 / 1e6 / t.4
    );
    let e = ENC.lock().unwrap();
    println!("encode: old {:.2} s ({:.0} MB/s), new {:.2} s ({:.0} MB/s)", e.0, t.0 as f64 / 1e6 / e.0, e.1, t.0 as f64 / 1e6 / e.1);
}
