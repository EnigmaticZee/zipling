//! Research tool: splits Mach-O files into arm64 code and everything else.
//! `machosplit DIR OUT_DIR` walks DIR, applies the arm64 branch filter the way
//! packing does, and writes `whole.bin` (filtered files back to back),
//! `code.bin` (their instruction ranges) and `rest.bin` (the other bytes),
//! and splits `rest.bin` again into `strings.bin` (C-string sections and symbol
//! names) and `other.bin`.
use arca_core::classify::{classify, Class, SNIFF_LEN};
use arca_core::transform::arm64;
use std::path::Path;

fn walk(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let t = e.file_type().unwrap();
        if t.is_dir() {
            walk(&e.path(), files);
        } else if t.is_file() {
            files.push(e.path());
        }
    }
}

/// File ranges (offset, length) of string sections and symbol-name tables.
fn string_ranges(d: &[u8]) -> Vec<(usize, usize)> {
    let le32 = |o: usize| d.get(o..o + 4).map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap())) as usize;
    let be32 = |o: usize| d.get(o..o + 4).map_or(0, |b| u32::from_be_bytes(b.try_into().unwrap())) as usize;
    let le64 = |o: usize| d.get(o..o + 8).map_or(0, |b| u64::from_le_bytes(b.try_into().unwrap())) as usize;
    let mut slices = Vec::new();
    match be32(0) {
        0xCAFE_BABE => (0..be32(4).min(32)).for_each(|i| slices.push(be32(8 + i * 20 + 8))),
        0xCAFE_BABF => (0..be32(4).min(32)).for_each(|i| slices.push(be32(8 + i * 32 + 8) << 32 | be32(8 + i * 32 + 12))),
        _ => slices.push(0),
    }
    let mut out = Vec::new();
    for base in slices {
        if le32(base) != 0xFEED_FACF {
            continue;
        }
        let (n, mut lc) = (le32(base + 16), base + 32);
        for _ in 0..n.min(4096) {
            let (cmd, size) = (le32(lc), le32(lc + 4));
            if cmd == 0x19 {
                for k in 0..le32(lc + 64).min(1024) {
                    let sc = lc + 72 + k * 80;
                    let name = &d[sc..sc + 16];
                    let (len, off, kind) = (le64(sc + 40), le32(sc + 48), le32(sc + 64) & 0xFF);
                    let named = name.starts_with(b"__swift5_reflstr") || name.starts_with(b"__swift5_typeref");
                    if (kind == 2 || named) && off > 0 && len > 0 {
                        out.push((base + off, len));
                    }
                }
            } else if cmd == 0x2 {
                out.push((base + le32(lc + 16), le32(lc + 20)));
            }
            if size == 0 {
                break;
            }
            lc += size;
        }
    }
    out.retain(|&(o, l)| o + l <= d.len());
    out.sort_unstable();
    out
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut files = Vec::new();
    walk(Path::new(&a[1]), &mut files);
    let (mut whole, mut code, mut rest, mut n) = (Vec::new(), Vec::new(), Vec::new(), 0);
    let (mut strings, mut other) = (Vec::new(), Vec::new());
    for f in files {
        let mut d = std::fs::read(&f).unwrap();
        if classify(&d[..d.len().min(SNIFF_LEN)]) != Class::MachO {
            continue;
        }
        let ranges = arm64::code_ranges(&d);
        if !ranges.is_empty() {
            arm64::filter(&mut d, &ranges, true).unwrap();
        }
        let mut at = 0;
        for &(off, len) in &ranges {
            let (s, e) = (off as usize, (off + len) as usize);
            rest.extend_from_slice(&d[at..s]);
            code.extend_from_slice(&d[s..e]);
            at = e;
        }
        rest.extend_from_slice(&d[at..]);
        // Strings never overlap code; mark them and split what is left.
        let mut is_code = vec![false; d.len()];
        for &(off, len) in &ranges {
            is_code[off as usize..(off + len) as usize].fill(true);
        }
        let mut is_str = vec![false; d.len()];
        for (off, len) in string_ranges(&d) {
            is_str[off..off + len].fill(true);
        }
        for i in 0..d.len() {
            if !is_code[i] {
                if is_str[i] { strings.push(d[i]) } else { other.push(d[i]) }
            }
        }
        whole.extend_from_slice(&d);
        n += 1;
    }
    let out = Path::new(&a[2]);
    std::fs::write(out.join("whole.bin"), &whole).unwrap();
    std::fs::write(out.join("code.bin"), &code).unwrap();
    std::fs::write(out.join("rest.bin"), &rest).unwrap();
    std::fs::write(out.join("strings.bin"), &strings).unwrap();
    std::fs::write(out.join("other.bin"), &other).unwrap();
    let mb = |v: &Vec<u8>| v.len() as f64 / 1e6;
    println!(
        "{n} Mach-O files: whole {:.1} MB, code {:.1} MB, rest {:.1} MB (strings {:.1}, other {:.1})",
        mb(&whole),
        mb(&code),
        mb(&rest),
        mb(&strings),
        mb(&other)
    );
}
