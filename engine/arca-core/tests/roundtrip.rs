use arca_core::{pack, Archive, EntryKind, ExtractOptions, Hooks, Level, PackOptions};
use std::fs;
use std::io::Write;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

fn write(p: &Path, data: &[u8]) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, data).unwrap();
}

/// Deterministic pseudo-random bytes.
fn noise(n: usize, mut s: u64) -> Vec<u8> {
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s as u8
        })
        .collect()
}

fn build_tree(root: &Path) {
    write(&root.join("docs/readme.txt"), b"hello arca\n".repeat(1000).as_slice());
    write(&root.join("docs/empty.txt"), b"");
    write(&root.join("docs/copy-of-readme.txt"), b"hello arca\n".repeat(1000).as_slice());
    write(&root.join("bin/blob.bin"), &noise(3 << 20, 7));
    // Same 3 MiB with a small edit: chunk dedup should store most of it once.
    let mut edited = noise(3 << 20, 7);
    edited[1_500_000..1_500_010].copy_from_slice(b"0123456789");
    write(&root.join("bin/blob-edited.bin"), &edited);
    fs::create_dir_all(root.join("empty-dir/nested")).unwrap();

    let script = root.join("bin/run.sh");
    write(&script, b"#!/bin/sh\necho hi\n");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

    std::os::unix::fs::symlink("../docs/readme.txt", root.join("bin/link-to-readme")).unwrap();
    fs::hard_link(root.join("docs/readme.txt"), root.join("hardlink.txt")).unwrap();

    xattr::set(root.join("docs/readme.txt"), "com.example.tag", b"blue").unwrap();
    xattr::set(root.join("docs/readme.txt"), "com.apple.ResourceFork", &noise(5000, 3)).unwrap();

    let t = filetime::FileTime::from_unix_time(1_600_000_000, 123_456_789);
    filetime::set_file_times(root.join("docs/empty.txt"), t, t).unwrap();
}

fn assert_same(a: &Path, b: &Path) {
    let ma = fs::symlink_metadata(a).unwrap();
    let mb = fs::symlink_metadata(b).unwrap();
    assert_eq!(ma.file_type().is_symlink(), mb.file_type().is_symlink(), "{}", a.display());
    if ma.file_type().is_symlink() {
        assert_eq!(fs::read_link(a).unwrap(), fs::read_link(b).unwrap());
        return;
    }
    assert_eq!(ma.mode() & 0o7777, mb.mode() & 0o7777, "mode of {}", a.display());
    if ma.is_dir() {
        let mut names: Vec<_> = fs::read_dir(a).unwrap().map(|e| e.unwrap().file_name()).collect();
        names.sort();
        let mut other: Vec<_> = fs::read_dir(b).unwrap().map(|e| e.unwrap().file_name()).collect();
        other.sort();
        assert_eq!(names, other, "listing of {}", a.display());
        for n in names {
            assert_same(&a.join(&n), &b.join(&n));
        }
    } else {
        assert_eq!(fs::read(a).unwrap(), fs::read(b).unwrap(), "content of {}", a.display());
        assert_eq!((ma.mtime(), ma.mtime_nsec()), (mb.mtime(), mb.mtime_nsec()), "mtime of {}", a.display());
        let names = |p: &Path| {
            let mut v: Vec<_> = xattr::list(p).unwrap().filter(|n| n != "com.apple.provenance").collect();
            v.sort();
            v
        };
        let xa = names(a);
        assert_eq!(xa, names(b), "xattr names of {}", a.display());
        for name in xa {
            assert_eq!(xattr::get(a, &name).unwrap(), xattr::get(b, &name).unwrap(), "xattr {name:?} of {}", a.display());
        }
    }
}

#[test]
fn full_roundtrip_preserves_everything() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("tree");
    build_tree(&src);

    for level in [Level::Fast, Level::Balanced, Level::Max, Level::Ultra] {
        let archive = tmp.path().join(format!("{level:?}.arca"));
        let stats = pack(std::slice::from_ref(&src), &archive, &PackOptions::preset(level)).unwrap();
        assert!(stats.dup_file_bytes >= 11_000, "identical files should dedup");
        assert!(stats.dup_chunk_bytes > 1 << 20, "edited blob should share chunks, got {}", stats.dup_chunk_bytes);

        let a = Archive::open(&archive).unwrap();
        a.verify(2).unwrap();
        let out = tmp.path().join(format!("out-{level:?}"));
        a.extract(&out, &ExtractOptions::default()).unwrap();
        assert_same(&src, &out.join("tree"));

        let m1 = fs::metadata(out.join("tree/docs/readme.txt")).unwrap();
        let m2 = fs::metadata(out.join("tree/hardlink.txt")).unwrap();
        assert_eq!(m1.ino(), m2.ino(), "hard link must be restored as a hard link");
    }
}

/// Extraction runs as a dependency graph over blocks. Tiny blocks make objects
/// span many of them, and one thread leaves room for only two decoded blocks.
#[test]
fn objects_spanning_many_blocks() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("tree");
    build_tree(&src);
    // A gzip member, expanded by the Deflate transform into many blocks' worth of text.
    let text: Vec<u8> = (0..100_000).flat_map(|i| format!("line {i} of some log output\n").into_bytes()).collect();
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::new(6));
    gz.write_all(&text).unwrap();
    write(&src.join("logs/app.log.gz"), &gz.finish().unwrap());

    let mut opts = PackOptions::preset(Level::Balanced);
    opts.block_size = 128 << 10;
    opts.text_block_size = 128 << 10;
    opts.adaptive_below = 0;
    let archive = tmp.path().join("a.arca");
    let stats = pack(std::slice::from_ref(&src), &archive, &opts).unwrap();
    assert!(stats.transforms.iter().any(|(name, t)| *name == "deflate" && t.files == 1), "{:?}", stats.transforms);

    let a = Archive::open(&archive).unwrap();
    let gz_entry = a.index.entries.iter().find(|e| e.path == "tree/logs/app.log.gz").unwrap();
    let EntryKind::File { object, .. } = gz_entry.kind else { panic!("not a file") };
    let mut blocks: Vec<u32> = a.index.objects[object as usize].chunks.iter().map(|&c| a.index.chunks[c as usize].block).collect();
    blocks.dedup();
    assert!(blocks.len() >= 3, "transformed object should span blocks, got {blocks:?}");

    for threads in [1, 3] {
        a.verify(threads).unwrap();
        let out = tmp.path().join(format!("out-{threads}"));
        a.extract(&out, &ExtractOptions { threads, ..Default::default() }).unwrap();
        assert_same(&src, &out.join("tree"));
    }
}

#[test]
fn extraction_can_be_cancelled() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("tree");
    build_tree(&src);
    let archive = tmp.path().join("a.arca");
    pack(&[src], &archive, &PackOptions::preset(Level::Fast)).unwrap();
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let opts = ExtractOptions { hooks: Hooks { cancel: Some(cancel), progress: None }, ..Default::default() };
    let err = Archive::open(&archive).unwrap().extract(&tmp.path().join("out"), &opts).unwrap_err();
    assert!(format!("{err:#}").contains("cancelled"), "{err:#}");
}

#[test]
fn selective_extraction() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("tree");
    build_tree(&src);
    let archive = tmp.path().join("a.arca");
    pack(&[src], &archive, &PackOptions::preset(Level::Fast)).unwrap();
    let out = tmp.path().join("out");
    Archive::open(&archive)
        .unwrap()
        .extract(&out, &ExtractOptions { only: vec!["tree/docs".into()], ..Default::default() })
        .unwrap();
    assert!(out.join("tree/docs/readme.txt").exists());
    assert!(!out.join("tree/bin").exists());
}

#[test]
fn refuses_existing_files_and_detects_corruption() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("tree");
    build_tree(&src);
    let archive = tmp.path().join("a.arca");
    pack(&[src], &archive, &PackOptions::preset(Level::Fast)).unwrap();

    let out = tmp.path().join("out");
    let a = Archive::open(&archive).unwrap();
    a.extract(&out, &ExtractOptions::default()).unwrap();
    assert!(a.extract(&out, &ExtractOptions::default()).is_err(), "must not overwrite silently");
    a.extract(&out, &ExtractOptions { overwrite: true, ..Default::default() }).unwrap();

    // Flip one byte in the first block body.
    let mut bytes = fs::read(&archive).unwrap();
    bytes[100] ^= 0x55;
    let bad = tmp.path().join("bad.arca");
    fs::write(&bad, &bytes).unwrap();
    let err = Archive::open(&bad).unwrap().verify(2).unwrap_err();
    assert!(format!("{err:#}").contains("damaged"), "{err:#}");
}

#[test]
fn rejects_path_traversal() {
    use arca_core::format::{Entry, EntryKind, Index};
    // Hand-build an index whose entry tries to escape the destination.
    let idx = Index {
        entries: vec![Entry { path: "../evil".into(), kind: EntryKind::Dir, mode: 0o755, mtime: (0, 0), xattrs: vec![] }],
        ..Default::default()
    };
    let raw = idx.encode();
    let comp = zstd::bulk::compress(&raw, 3).unwrap();
    let mut file = arca_core::format::header_bytes(0).to_vec();
    let footer = arca_core::format::Footer {
        index_offset: file.len() as u64,
        index_comp_len: comp.len() as u64,
        index_raw_len: raw.len() as u64,
        index_hash: arca_core::format::hash16(&comp),
    };
    file.extend_from_slice(&comp);
    file.extend_from_slice(&footer.to_bytes());
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("evil.arca");
    fs::write(&p, file).unwrap();
    let err = Archive::open(&p).unwrap().extract(&tmp.path().join("out"), &ExtractOptions::default()).unwrap_err();
    assert!(format!("{err:#}").contains("unsafe entry path"), "{err:#}");
    assert!(!tmp.path().join("evil").exists());
}
