//! Research tool: time of undoing and re-applying PNG row filters. `filtbench DIR`
use std::io::Read;
use std::time::Instant;

fn main() {
    let dir = std::env::args().nth(1).unwrap();
    let (mut n, mut tu, mut tr) = (0usize, 0.0, 0.0);
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let d = std::fs::read(e.path()).unwrap();
        if d.len() < 33 || &d[..8] != b"\x89PNG\r\n\x1a\n" || d[24] != 8 || d[28] != 0 {
            continue;
        }
        let (w, h) = (u32::from_be_bytes(d[16..20].try_into().unwrap()) as usize, u32::from_be_bytes(d[20..24].try_into().unwrap()) as usize);
        let ch = match d[25] {
            0 => 1,
            2 => 3,
            4 => 2,
            6 => 4,
            _ => continue,
        };
        let (mut pos, mut idat) = (8usize, Vec::new());
        while pos + 12 <= d.len() {
            let len = u32::from_be_bytes(d[pos..pos + 4].try_into().unwrap()) as usize;
            if &d[pos + 4..pos + 8] == b"IDAT" {
                idat.extend_from_slice(&d[pos + 8..pos + 8 + len]);
            }
            pos += 12 + len;
        }
        let mut rows = Vec::new();
        if flate2::read::ZlibDecoder::new(&idat[..]).read_to_end(&mut rows).is_err() {
            continue;
        }
        let t = Instant::now();
        let Some((types, px)) = arca_core::transform::pixels::unfilter(&rows, w, h, ch) else { continue };
        tu += t.elapsed().as_secs_f64();
        let t = Instant::now();
        let back = arca_core::transform::pixels::refilter(&types, &px, w, h, ch).unwrap();
        tr += t.elapsed().as_secs_f64();
        assert!(back == rows);
        n += px.len();
    }
    println!("{:.1} MB of pixels: unfilter {tu:.2} s ({:.0} MB/s), refilter {tr:.2} s ({:.0} MB/s)", n as f64 / 1e6, n as f64 / 1e6 / tu, n as f64 / 1e6 / tr);
}
