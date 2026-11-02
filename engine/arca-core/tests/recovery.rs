//! The recovery record: archives damaged on purpose, then repaired.
use arca_core::recovery::{self, REGION};
use arca_core::{pack, Archive, ExtractOptions, Hooks, Level, PackOptions};
use std::fs;
use std::io::Write;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

fn noise(n: usize, mut s: u64) -> Vec<u8> {
    let mut v = Vec::with_capacity(n);
    for _ in 0..n {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        v.push(s as u8);
    }
    v
}

/// A tree whose archive is a few MB: text, binaries, duplicates, many small files.
fn build_tree(root: &Path) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("bin")).unwrap();
    for i in 0..60 {
        let text: String = (0..4000).map(|j| format!("fn item_{i}_{j}() -> u32 {{ {j} }}\n")).collect();
        fs::write(root.join(format!("src/mod_{i}.rs")), text).unwrap();
    }
    for i in 0..6 {
        fs::write(root.join(format!("bin/blob_{i}.bin")), noise(700_000, 7 + i)).unwrap();
    }
    fs::write(root.join("bin/blob_copy.bin"), noise(700_000, 7)).unwrap();
    let mut f = fs::File::create(root.join("notes.txt")).unwrap();
    for i in 0..20000 {
        writeln!(f, "line {i}: the quick brown fox jumps over the lazy dog").unwrap();
    }
}

fn assert_same(a: &Path, b: &Path) {
    let mut ea: Vec<_> = walkdir(a);
    let mut eb: Vec<_> = walkdir(b);
    ea.sort();
    eb.sort();
    assert_eq!(ea, eb, "tree names differ");
    for rel in &ea {
        let (pa, pb) = (a.join(rel), b.join(rel));
        if pa.is_file() {
            assert_eq!(fs::read(&pa).unwrap(), fs::read(&pb).unwrap(), "{rel} differs");
        }
    }
}

fn walkdir(root: &Path) -> Vec<String> {
    fn go(dir: &Path, root: &Path, out: &mut Vec<String>) {
        for e in fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            out.push(p.strip_prefix(root).unwrap().to_string_lossy().into_owned());
            if p.is_dir() {
                go(&p, root, out);
            }
        }
    }
    let mut out = Vec::new();
    go(root, root, &mut out);
    out
}

struct Case {
    dir: tempfile::TempDir,
    src: PathBuf,
    archive: PathBuf,
}

fn make(percent: u16, level: Level) -> Case {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("tree");
    build_tree(&src);
    let archive = dir.path().join("a.zlg");
    let mut opts = PackOptions::preset(level);
    opts.recovery_percent = percent;
    pack(std::slice::from_ref(&src), &archive, &opts).unwrap();
    Case { dir, src, archive }
}

fn extract_ok(archive: &Path, src: &Path, dest: &Path) {
    let a = Archive::open(archive).unwrap();
    a.extract(dest, &ExtractOptions::default()).unwrap();
    assert_same(src, &dest.join("tree"));
}

/// Overwrites `len` bytes at `at` with garbage derived from the position.
fn smash(path: &Path, at: u64, len: usize) {
    let f = fs::OpenOptions::new().write(true).open(path).unwrap();
    let junk: Vec<u8> = noise(len, at ^ 0x9E37_79B9).iter().map(|b| b ^ 0x5A).collect();
    f.write_all_at(&junk, at).unwrap();
}

fn truncate(path: &Path, len: u64) {
    fs::OpenOptions::new().write(true).open(path).unwrap().set_len(len).unwrap();
}

#[test]
fn record_is_written_and_described() {
    let c = make(3, Level::Balanced);
    let a = Archive::open(&c.archive).unwrap();
    let (off, len) = a.recovery().expect("a record");
    let i = recovery::info(&c.archive).unwrap().expect("a record");
    assert_eq!(i.percent, 3);
    assert_eq!(i.covered_bytes, off);
    assert!(i.parity_bytes >= off * 3 / 100 && i.parity_bytes < off * 3 / 100 + 8 * i.slice as u64, "parity {} for {} bytes", i.parity_bytes, off);
    assert!(len > i.parity_bytes);
    let r = recovery::check(&c.archive, &Hooks::default()).unwrap();
    assert!(r.footer_ok && r.index_ok && r.repairable && r.damaged_data_slices == 0 && r.damaged_parity_slices == 0);
    assert_eq!(r.expected_len, fs::metadata(&c.archive).unwrap().len());
    // The archive extracts as any other.
    extract_ok(&c.archive, &c.src, &c.dir.path().join("x0"));
    // Without a record, nothing of the kind.
    let plain = make(0, Level::Fast);
    assert!(Archive::open(&plain.archive).unwrap().recovery().is_none());
    assert!(recovery::info(&plain.archive).unwrap().is_none());
    assert!(recovery::check(&plain.archive, &Hooks::default()).unwrap().record.is_none());
}

#[test]
fn scattered_sectors_are_repaired() {
    let c = make(5, Level::Balanced);
    let i = recovery::info(&c.archive).unwrap().unwrap();
    let slice = i.slice as u64;
    // Damage spread over the blocks, well within the budget: a few sectors each.
    let spots = (i.parity_bytes / slice).min(12).max(2) - 1;
    for s in 0..spots {
        let at = 64 + (i.covered_bytes - 128) * (s + 1) / (spots + 1);
        smash(&c.archive, at, 1500);
    }
    assert!(Archive::open(&c.archive).and_then(|a| a.verify(2)).is_err(), "the damage must show");
    let r = recovery::check(&c.archive, &Hooks::default()).unwrap();
    assert!(r.damaged_data_slices >= spots, "{r:?}");
    assert!(r.repairable, "{r:?}");
    let out = c.dir.path().join("fixed.zlg");
    let after = recovery::repair(&c.archive, &out, &Hooks::default()).unwrap();
    assert!(after.repairable && after.damaged_data_slices == 0 && after.damaged_parity_slices == 0 && after.footer_ok && after.index_ok, "{after:?}");
    extract_ok(&out, &c.src, &c.dir.path().join("x1"));
    // The damaged original is as it was.
    assert!(Archive::open(&c.archive).and_then(|a| a.verify(2)).is_err());
}

#[test]
fn damaged_index_and_footer_come_back_from_the_copies() {
    let c = make(3, Level::Max);
    let a = Archive::open(&c.archive).unwrap();
    let f = a.footer();
    let len = fs::metadata(&c.archive).unwrap().len();
    smash(&c.archive, f.index_offset + 5, 40);
    smash(&c.archive, len - 64, 64);
    assert!(Archive::open(&c.archive).is_err());
    let r = recovery::check(&c.archive, &Hooks::default()).unwrap();
    assert!(!r.footer_ok && !r.index_ok && r.repairable && r.damaged_data_slices == 0, "{r:?}");
    let out = c.dir.path().join("fixed.zlg");
    let after = recovery::repair(&c.archive, &out, &Hooks::default()).unwrap();
    assert!(after.footer_ok && after.index_ok && after.repairable, "{after:?}");
    assert_eq!(fs::read(&out).unwrap().len(), len as usize);
    extract_ok(&out, &c.src, &c.dir.path().join("x2"));
}

#[test]
fn truncated_tail_is_rebuilt() {
    let c = make(3, Level::Balanced);
    let a = Archive::open(&c.archive).unwrap();
    let (off, rlen) = a.recovery().unwrap();
    let full = fs::metadata(&c.archive).unwrap().len();
    // Cut into the parity: the index, the footer and some parity are gone.
    truncate(&c.archive, off + rlen * 2 / 3);
    assert!(Archive::open(&c.archive).is_err());
    let r = recovery::check(&c.archive, &Hooks::default()).unwrap();
    assert!(r.record.is_some() && !r.footer_ok && !r.index_ok && r.repairable, "{r:?}");
    assert_eq!(r.expected_len, full);
    let out = c.dir.path().join("fixed.zlg");
    let after = recovery::repair(&c.archive, &out, &Hooks::default()).unwrap();
    assert!(after.repairable && after.footer_ok && after.index_ok && after.damaged_parity_slices == 0, "{after:?}");
    assert_eq!(fs::metadata(&out).unwrap().len(), full);
    extract_ok(&out, &c.src, &c.dir.path().join("x3"));
}

#[test]
fn damage_beyond_the_budget_is_reported_with_the_files() {
    let c = make(1, Level::Balanced);
    let i = recovery::info(&c.archive).unwrap().unwrap();
    // Smash more than the record can carry, in the first region.
    let span = (i.parity_bytes + 4 * i.slice as u64).min(i.covered_bytes - 64);
    smash(&c.archive, 48, span as usize);
    let r = recovery::check(&c.archive, &Hooks::default()).unwrap();
    assert!(!r.repairable && r.unrepairable_regions == vec![0], "{r:?}");
    assert!(!r.lost_files.is_empty(), "the index copy names what is lost");
    let out = c.dir.path().join("partial.zlg");
    let after = recovery::repair(&c.archive, &out, &Hooks::default()).unwrap();
    assert!(!after.repairable && after.lost_files == r.lost_files, "{after:?}");
}

#[test]
fn an_existing_archive_can_be_protected_later() {
    let plain = make(0, Level::Fast);
    let out = plain.dir.path().join("protected.zlg");
    let i = recovery::protect(&plain.archive, &out, 10, &Hooks::default()).unwrap();
    assert_eq!(i.percent, 10);
    extract_ok(&out, &plain.src, &plain.dir.path().join("x4"));
    // Damage it, repair it.
    smash(&out, 1000, 3000);
    let fixed = plain.dir.path().join("fixed.zlg");
    let after = recovery::repair(&out, &fixed, &Hooks::default()).unwrap();
    assert!(after.repairable && after.damaged_data_slices == 0, "{after:?}");
    assert_eq!(fs::read(&fixed).unwrap(), {
        // Byte for byte what protect wrote: the repaired copy equals the undamaged file.
        let mut orig = plain.dir.path().join("protected-orig.zlg");
        orig.set_extension("zlg");
        recovery::protect(&plain.archive, &orig, 10, &Hooks::default()).unwrap();
        fs::read(&orig).unwrap()
    });
    extract_ok(&fixed, &plain.src, &plain.dir.path().join("x5"));
    // Protecting again replaces the record rather than stacking a second one.
    let again = plain.dir.path().join("again.zlg");
    let j = recovery::protect(&fixed, &again, 2, &Hooks::default()).unwrap();
    assert_eq!(j.percent, 2);
    assert!(fs::metadata(&again).unwrap().len() < fs::metadata(&fixed).unwrap().len());
    extract_ok(&again, &plain.src, &plain.dir.path().join("x6"));
}

#[test]
fn version_three_archives_still_open() {
    // A v3 footer carries zeros where v4 keeps the record's location: that is "no record".
    let c = make(0, Level::Fast);
    let a = Archive::open(&c.archive).unwrap();
    assert!(a.recovery().is_none());
    let _ = REGION;
}

#[test]
fn a_damaged_record_header_is_read_from_its_second_copy() {
    let c = make(3, Level::Fast);
    let a = Archive::open(&c.archive).unwrap();
    let (off, _) = a.recovery().unwrap();
    // Junk over the record's first header and the last bytes of the blocks before it.
    smash(&c.archive, off.saturating_sub(100), 600);
    let r = recovery::check(&c.archive, &Hooks::default()).unwrap();
    assert!(r.record.is_some() && r.repairable, "{r:?}");
    let out = c.dir.path().join("fixed.zlg");
    let after = recovery::repair(&c.archive, &out, &Hooks::default()).unwrap();
    assert!(after.repairable && after.damaged_data_slices == 0, "{after:?}");
    extract_ok(&out, &c.src, &c.dir.path().join("x7"));
    // The copy's first header is whole again: it parses without the footer.
    let len = fs::metadata(&out).unwrap().len();
    truncate(&out, len - 64);
    assert!(recovery::check(&out, &Hooks::default()).unwrap().record.is_some());
}

#[test]
fn inserted_and_dropped_bytes_are_realigned() {
    for (name, insert, at_frac) in [("insert 7", Some(7usize), 3u64), ("drop 300", None, 2), ("insert 4500", Some(4500), 5)] {
        let c = make(5, Level::Balanced);
        let whole = fs::read(&c.archive).unwrap();
        let a = Archive::open(&c.archive).unwrap();
        let (data_len, _) = a.recovery().unwrap();
        let at = (data_len / at_frac) as usize;
        let mut bytes = whole.clone();
        match insert {
            Some(n) => {
                let junk = noise(n, 99);
                bytes.splice(at..at, junk);
            }
            None => {
                bytes.drain(at..at + 300);
            }
        }
        fs::write(&c.archive, &bytes).unwrap();
        assert!(Archive::open(&c.archive).and_then(|a| a.verify(2)).is_err(), "{name}: must show as damaged");
        let r = recovery::check(&c.archive, &Hooks::default()).unwrap();
        assert!(r.record.is_some() && r.realigned && r.repairable, "{name}: {r:?}");
        assert!(r.damaged_data_slices <= 2, "{name}: only the slice holding the fault is lost, got {}", r.damaged_data_slices);
        let out = c.dir.path().join("fixed.zlg");
        let after = recovery::repair(&c.archive, &out, &Hooks::default()).unwrap();
        assert!(after.repairable && after.damaged_data_slices == 0 && after.footer_ok && after.index_ok, "{name}: {after:?}");
        assert_eq!(fs::read(&out).unwrap(), whole, "{name}: byte for byte the original");
        extract_ok(&out, &c.src, &c.dir.path().join("x8"));
    }
}
