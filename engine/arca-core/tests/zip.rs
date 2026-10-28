//! The ZIP reader against archives made by `ditto`, which is what Finder's
//! Compress uses: contents, permissions, Mac metadata from the `__MACOSX` twins,
//! the hidden flag, links (unsafe ones left out) and a file large enough to be
//! inflated piece by piece.

use arca_core::zip::{ZipArchive, ZipExtractOptions};
use std::fs;
use std::os::macos::fs::MetadataExt as _;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::process::Command;

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

#[test]
fn mac_made_zip_extracts_like_ditto() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src/Meta");
    fs::create_dir_all(src.join("sub")).unwrap();
    fs::write(src.join("tagged.txt"), b"hello\n").unwrap();
    let mut finder = [0u8; 32];
    finder[9] = 0x0C; // a colour label
    xattr::set(src.join("tagged.txt"), "com.apple.FinderInfo", &finder).unwrap();
    xattr::set(src.join("tagged.txt"), "com.example.tag", b"blue").unwrap();
    fs::write(src.join("forked.txt"), b"fork\n").unwrap();
    xattr::set(src.join("forked.txt"), "com.apple.ResourceFork", &noise(3000, 5)).unwrap();
    xattr::set(src.join("sub"), "com.example.dir", b"on a folder").unwrap();
    fs::write(src.join("hidden.txt"), b"hidden\n").unwrap();
    let mut hidden = [0u8; 32];
    hidden[8] = 0x40; // Finder's "invisible" bit
    xattr::set(src.join("hidden.txt"), "com.apple.FinderInfo", &hidden).unwrap();
    fs::write(src.join("run.sh"), b"#!/bin/sh\necho hi\n").unwrap();
    fs::set_permissions(src.join("run.sh"), fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(src.join("big.bin"), noise(70 << 20, 9)).unwrap();
    std::os::unix::fs::symlink("tagged.txt", src.join("link")).unwrap();
    std::os::unix::fs::symlink("../../../outside", src.join("escape")).unwrap();

    let zip = tmp.path().join("meta.zip");
    let ok = Command::new("ditto")
        .args(["-c", "-k", "--sequesterRsrc", "--keepParent"])
        .arg(&src)
        .arg(&zip)
        .status()
        .unwrap()
        .success();
    assert!(ok, "ditto failed");

    let out = tmp.path().join("out");
    let z = ZipArchive::open(&zip).unwrap();
    let stats = z.extract(&out, &ZipExtractOptions { quarantine: Some(b"0083;0;test;".to_vec()), ..Default::default() }).unwrap();
    assert_eq!(stats.skipped_links, vec!["Meta/escape".to_owned()]);
    let got = out.join("Meta");

    for name in ["tagged.txt", "forked.txt", "hidden.txt", "run.sh", "big.bin"] {
        assert_eq!(fs::read(got.join(name)).unwrap(), fs::read(src.join(name)).unwrap(), "{name}");
        let (a, b) = (fs::metadata(src.join(name)).unwrap(), fs::metadata(got.join(name)).unwrap());
        assert_eq!(a.mode() & 0o7777, b.mode() & 0o7777, "mode of {name}");
        assert_eq!(a.mtime(), b.mtime(), "mtime of {name}");
        assert_eq!(xattr::get(got.join(name), "com.apple.quarantine").unwrap().as_deref(), Some(&b"0083;0;test;"[..]));
    }
    assert_eq!(xattr::get(got.join("tagged.txt"), "com.apple.FinderInfo").unwrap().unwrap(), finder);
    assert_eq!(xattr::get(got.join("tagged.txt"), "com.example.tag").unwrap().unwrap(), b"blue");
    assert_eq!(xattr::get(got.join("forked.txt"), "com.apple.ResourceFork").unwrap().unwrap(), noise(3000, 5));
    assert_eq!(xattr::get(got.join("sub"), "com.example.dir").unwrap().unwrap(), b"on a folder");
    assert_ne!(fs::metadata(got.join("hidden.txt")).unwrap().st_flags() & libc::UF_HIDDEN, 0, "hidden flag");
    assert_eq!(fs::read_link(got.join("link")).unwrap(), Path::new("tagged.txt"));
    assert!(!got.join("escape").exists() && fs::symlink_metadata(got.join("escape")).is_err());
    assert!(!out.join("__MACOSX").exists(), "no AppleDouble twins written");

    // Checking without writing agrees, and a damaged archive is caught.
    assert!(z.verify(4).unwrap() > 70 << 20);
    let mut bytes = fs::read(&zip).unwrap();
    let mid = bytes.len() / 3;
    bytes[mid] ^= 0x55;
    let bad = tmp.path().join("bad.zip");
    fs::write(&bad, &bytes).unwrap();
    assert!(ZipArchive::open(&bad).map(|z| z.verify(4)).map_or(true, |r| r.is_err()));
}

#[test]
fn refuses_what_it_does_not_handle() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("plain");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("a.txt"), b"secret").unwrap();
    let zip = tmp.path().join("enc.zip");
    // An encrypted entry, the way `zip -P` writes one.
    let ok = Command::new("zip")
        .current_dir(&src)
        .args(["-q", "-P", "pw"])
        .arg(&zip)
        .arg("a.txt")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        return; // no `zip` tool on this Mac
    }
    let err = ZipArchive::open(&zip).unwrap().extract(&tmp.path().join("o"), &ZipExtractOptions::default()).unwrap_err();
    assert!(err.to_string().starts_with("unsupported:"), "{err}");
}
