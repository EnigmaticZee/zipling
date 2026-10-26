//! Content sniffing: decides how each file is routed through the pipeline.
//!
//! Classification looks at bytes, never trusts the extension alone, and is cheap
//! enough to run on every file (first few KiB only).

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Class {
    Text,
    MachO,
    Jpeg,
    /// Containers holding Deflate streams: ZIP family, PNG, PDF, gzip.
    DeflateContainer(DeflateKind),
    /// Formats that are already entropy coded; recompressing wastes CPU.
    Compressed,
    Binary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DeflateKind {
    Zip,
    Png,
    Pdf,
    Gzip,
}

/// Which block lane the stored stream of an object goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Lane {
    Text = 0,
    Binary = 1,
    Store = 2,
}

pub const SNIFF_LEN: usize = 4096;

pub fn classify(head: &[u8]) -> Class {
    let h = head;
    let starts = |m: &[u8]| h.starts_with(m);

    if starts(&[0xFF, 0xD8, 0xFF]) {
        return Class::Jpeg;
    }
    if starts(b"\x89PNG\r\n\x1a\n") {
        return Class::DeflateContainer(DeflateKind::Png);
    }
    if starts(b"PK\x03\x04") {
        return Class::DeflateContainer(DeflateKind::Zip);
    }
    if starts(b"%PDF-") {
        return Class::DeflateContainer(DeflateKind::Pdf);
    }
    if starts(&[0x1F, 0x8B, 0x08]) {
        return Class::DeflateContainer(DeflateKind::Gzip);
    }
    if is_macho(h) {
        return Class::MachO;
    }
    if is_compressed(h) {
        return Class::Compressed;
    }
    if looks_like_text(h) {
        return Class::Text;
    }
    Class::Binary
}

fn is_macho(h: &[u8]) -> bool {
    if h.len() < 8 {
        return false;
    }
    match &h[..4] {
        [0xCF, 0xFA, 0xED, 0xFE] | [0xCE, 0xFA, 0xED, 0xFE] => true,
        // Fat binaries share CAFEBABE with Java class files; Java stores a class
        // file version (>= 45) where fat headers store a small arch count.
        [0xCA, 0xFE, 0xBA, 0xBE] | [0xCA, 0xFE, 0xBA, 0xBF] => {
            let n = u32::from_be_bytes([h[4], h[5], h[6], h[7]]);
            (1..=20).contains(&n)
        }
        _ => false,
    }
}

fn is_compressed(h: &[u8]) -> bool {
    let starts = |m: &[u8]| h.starts_with(m);
    starts(&[0x28, 0xB5, 0x2F, 0xFD]) // zstd
        || starts(&[0xFD, b'7', b'z', b'X', b'Z', 0]) // xz
        || starts(b"BZh") // bzip2
        || starts(&[b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C]) // 7z
        || starts(b"Rar!\x1a\x07") // rar
        || starts(&[0x04, 0x22, 0x4D, 0x18]) // lz4
        || starts(b"bvx2") || starts(b"bvxn") // lzfse
        || starts(b"AA01") || starts(b"YAA1") // Apple Archive
        || starts(b"fLaC")
        || starts(b"OggS")
        || starts(b"ID3")
        || starts(&[0x1A, 0x45, 0xDF, 0xA3]) // matroska / webm
        || starts(b"GIF8")
        || (h.len() >= 12 && &h[0..4] == b"RIFF" && (&h[8..12] == b"WEBP" || &h[8..12] == b"AVI "))
        || (h.len() >= 12 && &h[4..8] == b"ftyp") // mp4, mov, heic, avif, m4a
        || (h.len() >= 2 && h[0] == 0xFF && (h[1] & 0xF6) == 0xF0) // AAC ADTS
        || (h.len() >= 12 && &h[0..12] == b"\x00\x00\x00\x0cJXL \r\n\x87\n")
        || starts(&[0xFF, 0x0A]) // JPEG XL codestream
}

fn looks_like_text(h: &[u8]) -> bool {
    if h.is_empty() {
        return true;
    }
    let mut ctrl = 0usize;
    for &b in h {
        match b {
            0 => return false,
            b'\t' | b'\n' | b'\r' | 0x0C => {}
            0x01..=0x1F | 0x7F => ctrl += 1,
            _ => {}
        }
    }
    // UTF-8 (or Latin-1) prose and code: virtually no control bytes.
    ctrl * 100 <= h.len()
}

/// Cheap compressibility probe: zstd level 1 over three 16 KiB samples (start,
/// middle, end) of a chunk. Run per chunk, so a file that mixes compressed and
/// compressible regions (asset catalogs, disk images, PDFs) is split accordingly.
pub fn probably_incompressible(data: &[u8]) -> bool {
    const PIECE: usize = 16 << 10;
    if data.len() < 512 {
        return false;
    }
    let owned;
    let sample: &[u8] = if data.len() <= 3 * PIECE {
        data
    } else {
        let mid = data.len() / 2 - PIECE / 2;
        owned = [&data[..PIECE], &data[mid..mid + PIECE], &data[data.len() - PIECE..]].concat();
        &owned
    };
    match zstd::bulk::compress(sample, 1) {
        Ok(c) => c.len() as f64 > sample.len() as f64 * 0.97,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffing() {
        assert_eq!(classify(b"\xFF\xD8\xFF\xE0rest"), Class::Jpeg);
        assert_eq!(classify(b"PK\x03\x04...."), Class::DeflateContainer(DeflateKind::Zip));
        assert_eq!(classify(b"fn main() {}\n"), Class::Text);
        assert_eq!(classify(b"\xCA\xFE\xBA\xBE\x00\x00\x00\x02"), Class::MachO);
        // Java class file, version 52
        assert_eq!(classify(b"\xCA\xFE\xBA\xBE\x00\x00\x00\x34"), Class::Binary);
        assert_eq!(classify(b"\x00\x00\x00\x18ftypheic"), Class::Compressed);
    }
}
